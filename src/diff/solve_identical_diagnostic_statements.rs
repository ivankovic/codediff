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
use std::collections::{HashMap, VecDeque};

use crate::diff::PassCtx;
use crate::diff::nodes::{collect_unmatched, is_diagnostic_statement, map_identical_descendants};
use crate::diff::{ASTDiff, ASTMapping, ASTMappingReason};

/// Phase 2: pairs still-unmatched diagnostic statements (logging, `bail!`, assertions; see
/// [`crate::diff::nodes::is_diagnostic_statement`]) whose whole statement is byte-identical on both
/// sides, one-to-one.
///
/// After phase 1, so an incidental identical `log::debug!` cannot fragment a whole-function match;
/// before phase 4, so it still finds such statements inside a function with no same-named
/// counterpart. Exact hash only: a changed message is left for APTED to size up as an update.
pub fn solve(ctx: &PassCtx, diff: &mut ASTDiff) {
    let (before, after, node_cache) = (ctx.before, ctx.after, ctx.node_cache);
    let before_metadata = ctx.before_metadata();
    let after_metadata = ctx.after_metadata();

    let Some(before_ast) = before.ast.as_ref() else {
        return;
    };
    let Some(after_ast) = after.ast.as_ref() else {
        return;
    };
    let language = before_metadata.language;

    let before_source = before.contents.as_bytes();
    let after_source = after.contents.as_bytes();

    let before_candidates =
        collect_unmatched(before_ast.root_node(), &diff.before_node_map, |node| {
            is_diagnostic_statement(node, &language, before_source)
        });
    let after_candidates = collect_unmatched(after_ast.root_node(), &diff.after_node_map, |node| {
        is_diagnostic_statement(node, &language, after_source)
    });
    if before_candidates.is_empty() || after_candidates.is_empty() {
        return;
    }

    let mut after_by_hash: HashMap<u64, VecDeque<usize>> = HashMap::new();
    for node in &after_candidates {
        if let Some(&hash) = after_metadata.node_to_full_hash.get(&node.id()) {
            after_by_hash.entry(hash).or_default().push_back(node.id());
        }
    }

    let mut before_candidates = before_candidates;
    before_candidates.sort_by(|a, b| {
        let size_a = before_metadata
            .node_to_subtree_size
            .get(&a.id())
            .copied()
            .unwrap_or(0);
        let size_b = before_metadata
            .node_to_subtree_size
            .get(&b.id())
            .copied()
            .unwrap_or(0);
        size_b.cmp(&size_a)
    });

    for before_node in before_candidates {
        if diff.before_node_map.contains_key(&before_node.id()) {
            continue;
        }
        let Some(hash) = before_metadata.node_to_full_hash.get(&before_node.id()) else {
            continue;
        };
        let Some(queue) = after_by_hash.get_mut(hash) else {
            continue;
        };
        let Some(after_node_id) = queue.pop_front() else {
            continue;
        };
        let Some(after_node) = node_cache.after.get(&after_node_id).copied() else {
            continue;
        };

        diff.add_mapping(
            before_node.id(),
            after_node_id,
            ASTMapping::identical(ASTMappingReason::IdenticalHash),
        );

        map_identical_descendants(before_node, after_node, diff);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::Code;
    use crate::code::Language;
    use crate::diff::ASTMappingOperation;
    use crate::diff::NodeCache;
    use crate::test::helper::find_first_of_kind;
    use tree_sitter::Node;

    fn find_all<'a>(node: Node<'a>, kind: &str, out: &mut Vec<Node<'a>>) {
        if node.kind() == kind {
            out.push(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            find_all(child, kind, out);
        }
    }

    #[test]
    fn identical_bail_macro_is_matched_across_renamed_functions() {
        // The renamed function has no name to match on.
        let before_src = r#"
fn from_str(s: &str) -> Result<i32> {
    if s.is_empty() {
        bail!("input must not be empty");
    }
    Ok(1)
}
"#;
        let after_src = r#"
fn parse(s: &str) -> Result<i32> {
    if s.is_empty() {
        bail!("input must not be empty");
    }
    Ok(2)
}
"#;
        let before = Code::from_string(before_src, &Language::Rust);
        let after = Code::from_string(after_src, &Language::Rust);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );

        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();
        let before_bail = find_first_of_kind(before_ast.root_node(), "macro_invocation").unwrap();
        let after_bail = find_first_of_kind(after_ast.root_node(), "macro_invocation").unwrap();

        let mapping = diff
            .mapping
            .get(&(before_bail.id(), after_bail.id()))
            .expect("the two identical `bail!` calls should be mapped to each other");
        assert_eq!(mapping.operation, ASTMappingOperation::Identical);
    }

    #[test]
    fn changed_diagnostic_statement_is_not_matched() {
        let before_src = r#"
fn a() {
    log::error!("first message");
}
"#;
        let after_src = r#"
fn b() {
    log::error!("second message");
}
"#;
        let before = Code::from_string(before_src, &Language::Rust);
        let after = Code::from_string(after_src, &Language::Rust);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );

        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();
        let before_log = find_first_of_kind(before_ast.root_node(), "macro_invocation").unwrap();
        let after_log = find_first_of_kind(after_ast.root_node(), "macro_invocation").unwrap();

        assert!(
            !diff
                .mapping
                .contains_key(&(before_log.id(), after_log.id())),
            "non-identical diagnostic statements should not be matched by this pass"
        );
    }

    #[test]
    fn non_diagnostic_identical_call_is_not_matched() {
        let before_src = r#"
fn a() {
    compute(1, 2);
}
"#;
        let after_src = r#"
fn b() {
    compute(1, 2);
}
"#;
        let before = Code::from_string(before_src, &Language::Rust);
        let after = Code::from_string(after_src, &Language::Rust);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );

        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();
        let before_call = find_first_of_kind(before_ast.root_node(), "call_expression").unwrap();
        let after_call = find_first_of_kind(after_ast.root_node(), "call_expression").unwrap();

        assert!(
            !diff
                .mapping
                .contains_key(&(before_call.id(), after_call.id())),
            "non-diagnostic calls should not be matched by this pass, even if identical"
        );
    }

    #[test]
    fn duplicate_identical_diagnostic_calls_are_matched_one_to_one() {
        let before_src = r#"
fn a() {
    if true { bail!("dup"); }
    if false { bail!("dup"); }
}
"#;
        let after_src = r#"
fn b() {
    if true { bail!("dup"); }
    if false { bail!("dup"); }
}
"#;
        let before = Code::from_string(before_src, &Language::Rust);
        let after = Code::from_string(after_src, &Language::Rust);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );

        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();
        let mut before_bails = Vec::new();
        let mut after_bails = Vec::new();
        find_all(
            before_ast.root_node(),
            "macro_invocation",
            &mut before_bails,
        );
        find_all(after_ast.root_node(), "macro_invocation", &mut after_bails);
        assert_eq!(before_bails.len(), 2);
        assert_eq!(after_bails.len(), 2);

        let targets: std::collections::HashSet<usize> = before_bails
            .iter()
            .map(|b| {
                *diff
                    .before_node_map
                    .get(&b.id())
                    .unwrap_or_else(|| panic!("before bail! {} was never matched", b.id()))
            })
            .collect();
        assert_eq!(
            targets.len(),
            2,
            "each duplicate bail! should match a distinct after-node"
        );
    }
}
