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
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program. If not, see <https://www.gnu.org/licenses/>.
 */
use tree_sitter::Node;

use crate::diff::PassCtx;
use crate::diff::{ASTDiff, ASTMappingOperation, ASTMappingReason, NodeCache};

/// Re-tags an `Identical` match as `WrapGrowth` when existing code gained a brand-new parent chain
/// (`try { EXISTING } catch ...`, an existing `if` becoming an `else if` branch). Never creates or
/// moves a mapping; `ranges()` acts on the tag only under
/// [`crate::diff::text::RenderOptions::paint_reindent_only_moves`], because rust-add-if's ground
/// truth paints this shape `Move` under `Full` and not under `Minimal`.
///
/// The verification: climbing from the after-side node through only brand-new levels must land on
/// the node matched to its real before-side parent. Every other child along the climb must be new
/// content or another relocated piece of the same original parent (`is_safe_wrapper_sibling`); a
/// sibling with an identity elsewhere is evidence of a more complex restructuring.
pub fn solve(ctx: &PassCtx, diff: &mut ASTDiff) {
    let node_cache = ctx.node_cache;
    // `node_to_parent`, never `Node::parent()`, which walks down from the root on every call.
    let before_parents = &ctx.before_metadata().node_to_parent;
    let after_parents = &ctx.after_metadata().node_to_parent;
    let candidates: Vec<(usize, usize)> = diff
        .mapping
        .iter()
        .filter(|(_, mapping)| mapping.operation == ASTMappingOperation::Identical)
        .map(|(&ids, _)| ids)
        .collect();

    for (before_id, after_id) in candidates {
        let Some(&before_node) = node_cache.before.get(&before_id) else {
            continue;
        };
        let Some(&after_node) = node_cache.after.get(&after_id) else {
            continue;
        };

        if before_node.start_position() == after_node.start_position() {
            continue;
        }
        // Leaves are out of scope: tagging one (a rewritten `for` header's `in`) can shift an
        // unrelated match's rendering boundary.
        if before_node.child_count() == 0 {
            continue;
        }

        let Some(&before_parent_id) = before_parents.get(&before_id) else {
            continue;
        };

        if !verify_pure_wrap(
            after_node,
            before_parent_id,
            diff,
            node_cache,
            before_parents,
            after_parents,
        ) {
            continue;
        }

        if let Some(mapping) = diff.mapping.get_mut(&(before_id, after_id)) {
            mapping.reason = ASTMappingReason::WrapGrowth;
        }
    }
}

/// Climb bound; a real wrapper is a handful of levels, so this is only defensive.
const MAX_WRAP_DEPTH: usize = 6;

/// Climbs from `after_node` through unmatched ancestors, checking every other child with
/// `is_safe_wrapper_sibling`. True only if the first matched ancestor is `before_parent_id`.
fn verify_pure_wrap(
    after_node: Node,
    before_parent_id: usize,
    diff: &ASTDiff,
    node_cache: &NodeCache,
    before_parents: &rustc_hash::FxHashMap<usize, usize>,
    after_parents: &rustc_hash::FxHashMap<usize, usize>,
) -> bool {
    let mut current = after_node;
    for climbed in 0..MAX_WRAP_DEPTH {
        let Some(&parent) = after_parents
            .get(&current.id())
            .and_then(|id| node_cache.after.get(id))
        else {
            return false;
        };

        let mut cursor = parent.walk();
        for sibling in parent.children(&mut cursor) {
            if sibling.id() == current.id() {
                continue;
            }
            if !is_safe_wrapper_sibling(sibling, diff, before_parents, before_parent_id) {
                return false;
            }
        }

        if let Some(&matched_before_id) = diff.after_node_map.get(&parent.id())
            && matched_before_id != 0
        {
            // `climbed == 0` is a sibling shift at the same level, not a wrap; that is
            // `column_shift_is_meaningful`'s territory in `ranges()` (typescript-refactor-interface).
            return climbed > 0 && matched_before_id == before_parent_id;
        }
        // The after root is never recorded in `ASTDiff`, but the two roots correspond by
        // construction: success if the node's real parent was the before root.
        if !after_parents.contains_key(&parent.id()) {
            return climbed > 0 && !before_parents.contains_key(&before_parent_id);
        }
        current = parent;
    }
    false
}

/// Whether `sibling` fits a pure wrap: a leaf, a subtree with no reused identity anywhere in it
/// (the wrapper's own shell), or a node matched to another child of `before_parent_id` (a run of
/// statements wrapped together).
fn is_safe_wrapper_sibling(
    sibling: Node,
    diff: &ASTDiff,
    before_parents: &rustc_hash::FxHashMap<usize, usize>,
    before_parent_id: usize,
) -> bool {
    if sibling.child_count() == 0 {
        return true;
    }
    if let Some(&before_id) = diff.after_node_map.get(&sibling.id())
        && before_id != 0
    {
        return before_parents.get(&before_id) == Some(&before_parent_id);
    }
    let mut stack = vec![sibling];
    while let Some(node) = stack.pop() {
        if let Some(&before_id) = diff.after_node_map.get(&node.id())
            && before_id != 0
        {
            return false;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::Code;
    use crate::code::Language;
    use crate::diff::diff_code;
    use crate::test::helper::find_first_of_kind;

    /// rust-add-if, minimized.
    #[test]
    fn an_existing_if_else_becoming_an_else_if_branch_is_tagged() {
        let before = Code::from_string(
            "fn f() {\n    if number % 2 == 0 {\n        even();\n    } else {\n        odd();\n    }\n}\n",
            &Language::Rust,
        );
        let after = Code::from_string(
            "fn f() {\n    if number == 0 {\n        zero();\n    } else if number % 2 == 0 {\n        even();\n    } else {\n        odd();\n    }\n}\n",
            &Language::Rust,
        );

        let diff = diff_code(&before, &after).ast.expect("ast diff");

        let before_root = before.ast.as_ref().unwrap().root_node();
        let before_if = find_first_of_kind(before_root, "if_expression").unwrap();
        let after_id = diff.before_node_map.get(&before_if.id()).copied();
        let mapping = after_id.and_then(|id| diff.mapping.get(&(before_if.id(), id)));

        assert_eq!(
            mapping.map(|m| &m.reason),
            Some(&ASTMappingReason::WrapGrowth),
            "the reused if/else must be tagged WrapGrowth, not left to a bare column-shift guess"
        );
    }

    #[test]
    fn a_run_of_statements_wrapped_in_a_new_try_block_are_all_tagged() {
        let before = Code::from_string(
            "class C {\n    void m() {\n        a();\n        b();\n        c();\n    }\n}\n",
            &Language::Java,
        );
        let after = Code::from_string(
            "class C {\n    void m() {\n        try {\n            a();\n            b();\n            c();\n        } catch (Exception e) {\n            handle(e);\n        }\n    }\n}\n",
            &Language::Java,
        );

        let diff = diff_code(&before, &after).ast.expect("ast diff");
        let after_root = after.ast.as_ref().unwrap().root_node();

        let tagged_statements = collect_of_kind(after_root, "expression_statement")
            .into_iter()
            .filter(|node| {
                diff.after_node_map
                    .get(&node.id())
                    .and_then(|&before_id| diff.mapping.get(&(before_id, node.id())))
                    .is_some_and(|m| m.reason == ASTMappingReason::WrapGrowth)
            })
            .count();
        assert_eq!(
            tagged_statements, 3,
            "all three reused statements (a(); b(); c();) should be tagged, not just one"
        );
    }

    fn collect_of_kind<'a>(root: Node<'a>, kind: &str) -> Vec<Node<'a>> {
        let mut result = Vec::new();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if node.kind() == kind {
                result.push(node);
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
        }
        result
    }

    #[test]
    fn only_byte_identical_content_is_ever_tagged_even_when_a_sibling_condition_changed() {
        let before_src = "fn f() {\n    if number % 2 == 0 {\n        even();\n    } else {\n        odd();\n    }\n}\n";
        let after_src = "fn f() {\n    if number == 0 {\n        zero();\n    } else if number % 3 == 0 {\n        even();\n    } else {\n        odd();\n    }\n}\n";
        let before = Code::from_string(before_src, &Language::Rust);
        let after = Code::from_string(after_src, &Language::Rust);

        let diff = diff_code(&before, &after).ast.expect("ast diff");
        let node_cache = NodeCache::build(&before, &after);

        let tagged: Vec<_> = diff
            .mapping
            .iter()
            .filter(|(_, m)| m.reason == ASTMappingReason::WrapGrowth)
            .collect();
        for (&(before_id, after_id), _) in tagged {
            let before_node = node_cache.before[&before_id];
            let after_node = node_cache.after[&after_id];
            assert_eq!(
                before_node.utf8_text(before_src.as_bytes()),
                after_node.utf8_text(after_src.as_bytes()),
                "a WrapGrowth-tagged pair must always be byte-identical"
            );
        }
    }

    fn wrap_growth_tagged(diff: &ASTDiff, root: Node, kind: &str) -> usize {
        collect_of_kind(root, kind)
            .into_iter()
            .filter(|node| {
                diff.after_node_map
                    .get(&node.id())
                    .and_then(|&before_id| diff.mapping.get(&(before_id, node.id())))
                    .is_some_and(|m| m.reason == ASTMappingReason::WrapGrowth)
            })
            .count()
    }

    #[test]
    fn a_sibling_shift_at_the_same_level_is_not_a_wrap() {
        let before = Code::from_string("fn f() {\n    a();\n}\n", &Language::Rust);
        let after = Code::from_string("fn f() {\n    z();\n    a();\n}\n", &Language::Rust);

        let diff = diff_code(&before, &after).ast.expect("ast diff");

        assert!(
            diff.mapping
                .values()
                .all(|m| m.reason != ASTMappingReason::WrapGrowth)
        );
    }

    #[test]
    fn top_level_statements_wrapped_in_a_new_try_are_tagged() {
        let before = Code::from_string("a();\nb();\n", &Language::TypeScript);
        let after = Code::from_string(
            "try {\n    a();\n    b();\n} catch (e) {\n    handle(e);\n}\n",
            &Language::TypeScript,
        );

        let diff = diff_code(&before, &after).ast.expect("ast diff");
        let after_root = after.ast.as_ref().unwrap().root_node();

        assert_eq!(
            wrap_growth_tagged(&diff, after_root, "expression_statement"),
            2
        );
    }
}
