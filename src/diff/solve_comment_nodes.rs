/*  This file is part of the CodeDiff code diffing tool.
 *
 *  Copyright (C) 2026 Marko Ivankovic
 *
 *  This program is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU Affero General Public License as published
 *  by the Free Software Foundation, either version 3 of the License, or
 *  (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General License
 *  along with this program. If not, see <https://www.gnu.org/licenses/>.
 */
use std::collections::HashSet;

use tree_sitter::Node;

use crate::code::Code;
use crate::diff::nodes::is_comment;
use crate::diff::{ASTDiff, ASTMapping, ASTMappingOperation, ASTMappingReason, NodeCache};

/// Match comment nodes that immediately precede already-matched nodes.
///
/// Runs after the hash and structural matching passes, catching leading comments early so the
/// slow tree edit distance pass has less work left, especially in heavily-documented codebases.
///
/// Deliberately conservative: a comment only matches its counterpart if it's the immediate
/// preceding sibling of an already-matched node, unmatched itself, and textually identical. This
/// avoids matching comments that belong to different code sections. Comment chains (multiple
/// consecutive comments) are not handled - only the single comment directly touching the matched
/// node.
pub fn solve(before: &Code, after: &Code, node_cache: &NodeCache, diff: &mut ASTDiff) {
    let before_src = before.contents.as_bytes();
    let after_src = after.contents.as_bytes();

    let matched_before_ids: HashSet<usize> = diff.before_node_map.keys().cloned().collect();
    let matched_after_ids: HashSet<usize> = diff.after_node_map.keys().cloned().collect();

    let current_mappings: Vec<(usize, usize)> = diff.before_node_map.iter().map(|(&k, &v)| (k, v)).collect();

    for (before_id, after_id) in current_mappings {
        // Skip if either node is 0 (delete/insert)
        if before_id == 0 || after_id == 0 {
            continue;
        }

        let Some(&before_node) = node_cache.before.get(&before_id) else {
            continue;
        };

        let Some(&after_node) = node_cache.after.get(&after_id) else {
            continue;
        };

        let before_comment = find_immediate_preceding_comment_sibling(before_node, &matched_before_ids);
        let after_comment = find_immediate_preceding_comment_sibling(after_node, &matched_after_ids);

        if let (Some((before_comment_node, before_comment_id)), Some((after_comment_node, after_comment_id))) = (before_comment, after_comment) {
            // Check if the comment text is identical
            let before_text = before_comment_node.utf8_text(before_src).unwrap_or("");
            let after_text = after_comment_node.utf8_text(after_src).unwrap_or("");

            if before_text == after_text {
                // Match the comment nodes
                diff.add_mapping(
                    before_comment_id,
                    after_comment_id,
                    ASTMapping {
                        cost: 0, // Comments with identical text have 0 cost
                        operation: ASTMappingOperation::Identical,
                        reason: ASTMappingReason::CommentSibling,
                    },
                );

                // The comment node's own text matched byte-for-byte, so every descendant (e.g.
                // the `//`/`/*`/`*/` marker tokens) is identical too - map them now. Without
                // this, `PostorderIndexer` would prune the whole subtree the moment it sees the
                // comment node itself already mapped, leaving those descendants with no mapping
                // at all (not even a delete) for every later pass to find.
                map_identical_descendants(before_comment_node, after_comment_node, diff);
            }
        }
    }
}

/// Recursively map every descendant of an already-matched, byte-identical pair - same idiom as
/// `solve_identical_trees`/`solve_identical_diagnostic_statements`. Safe here because the caller
/// only reaches this after confirming the comment nodes' full text is identical, which guarantees
/// their subtrees line up 1:1 in both structure and content.
fn map_identical_descendants<'a>(before_node: Node<'a>, after_node: Node<'a>, diff: &mut ASTDiff) {
    let mut stack = vec![(before_node, after_node)];
    while let Some((before_parent, after_parent)) = stack.pop() {
        let mut before_cursor = before_parent.walk();
        let mut after_cursor = after_parent.walk();
        let before_children: Vec<_> = before_parent.children(&mut before_cursor).collect();
        let after_children: Vec<_> = after_parent.children(&mut after_cursor).collect();

        for (before_child, after_child) in before_children.into_iter().zip(after_children) {
            if diff.before_node_map.contains_key(&before_child.id()) {
                continue;
            }
            diff.add_mapping(
                before_child.id(),
                after_child.id(),
                ASTMapping {
                    cost: 0,
                    operation: ASTMappingOperation::Identical,
                    reason: ASTMappingReason::IdenticalHashOfAncestor,
                },
            );
            stack.push((before_child, after_child));
        }
    }
}

/// Find the immediately preceding sibling that is a comment node and hasn't been matched yet
fn find_immediate_preceding_comment_sibling<'a>(
    node: Node<'a>,
    already_matched: &HashSet<usize>,
) -> Option<(Node<'a>, usize)> {
    let parent = node.parent()?;
    let node_id = node.id();

    let mut cursor = parent.walk();
    let mut prev_sibling: Option<Node> = None;

    for child in parent.children(&mut cursor) {
        if child.id() == node_id {
            let prev = prev_sibling?;
            let prev_id = prev.id();
            return (is_comment(prev.kind()) && !already_matched.contains(&prev_id)).then_some((prev, prev_id));
        }
        prev_sibling = Some(child);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::{Code, Language};

    #[test]
    fn test_is_comment_function() {
        // Test various comment node kinds
        assert!(is_comment("comment"));
        assert!(is_comment("line_comment"));
        assert!(is_comment("block_comment"));
        assert!(is_comment("js_comment"));

        // Test non-comment node kinds
        assert!(!is_comment("function_item"));
        assert!(!is_comment("identifier"));
        assert!(!is_comment("string"));
    }

    #[test]
    fn matches_leading_comment_of_an_unchanged_sibling_function() {
        // `hello` and its leading comment are unchanged, but `other`'s body gains an `if`
        // statement, changing its tree shape - so neither the hash nor the structural pass can
        // match the whole file as one unit; only `hello`'s `function_item` matches, via
        // `solve_identical_trees`. Its leading comment is a *sibling*, not a descendant, so it's
        // untouched by that match and left for this pass to pick up specifically, which we
        // verify via `ASTMappingReason::CommentSibling`.
        let before = Code::from_string(
            "// This is a comment\nfn hello() {\n    println!(\"Hello\");\n}\nfn other() {\n    1;\n}\n",
            &Language::Rust,
        );
        let after = Code::from_string(
            "// This is a comment\nfn hello() {\n    println!(\"Hello\");\n}\nfn other() {\n    if true {\n        2;\n    }\n}\n",
            &Language::Rust,
        );

        let diff = crate::diff::diff_code(&before, &after).ast.expect("ast diff");

        let comment_mapping = diff
            .mapping
            .values()
            .find(|m| m.reason == ASTMappingReason::CommentSibling)
            .expect("leading comment should be matched via CommentSibling");
        assert_eq!(comment_mapping.operation, ASTMappingOperation::Identical);

        // The comment node itself matching isn't enough - tree-sitter's Rust grammar gives
        // `line_comment` its own `//` child. If that child isn't also mapped, later passes can
        // never find it (its parent is already mapped, so `PostorderIndexer` prunes the whole
        // subtree), leaving it with no mapping at all.
        let before_root = before.ast.as_ref().unwrap().root_node();
        let before_marker = find_first(before_root, "//").expect("before `//` token should exist");
        assert!(
            diff.before_node_map.contains_key(&before_marker.id()),
            "the comment's `//` marker token should also be mapped, not just the comment node"
        );
    }

    fn find_first<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
        if node.kind() == kind {
            return Some(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = find_first(child, kind) {
                return Some(found);
            }
        }
        None
    }
}
