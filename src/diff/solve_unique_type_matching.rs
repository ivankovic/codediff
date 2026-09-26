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
use crate::diff::PassCtx;
use crate::diff::nodes::anchor_pair_via_apted;
use crate::diff::{ASTDiff, ASTMappingReason};
use std::collections::HashMap;

/// GumTree Simple's "unique type matching" recovery step: under every matched pair, a child kind
/// with exactly one unmatched child on each side pairs those two, and bounded APTED
/// (`anchor_pair_via_apted`) resolves and costs the pair. Zero or several candidates of a kind on
/// either side is left alone; the value is that a unique pair under known-corresponding parents is
/// unambiguous by construction, not merely likely.
///
/// One pass over a snapshot of matched pairs, not recursive to a fixed point.
pub fn solve(ctx: &PassCtx, diff: &mut ASTDiff) {
    let before_metadata = ctx.before_metadata();
    let after_metadata = ctx.after_metadata();

    // Sorted by preorder index, as node ids are not parse-stable. Pairs with no unmatched
    // before-child are filtered out first so the sort is proportional to the residual; matching only
    // adds entries, so the filter cannot drop a pair that becomes useful later.
    let mut matched_pairs: Vec<(usize, usize)> = diff
        .before_node_map
        .iter()
        .filter_map(|(&before_id, &after_id)| (after_id != 0).then_some((before_id, after_id)))
        .filter(|(before_id, _)| {
            before_metadata
                .node_info
                .get(before_id)
                .is_some_and(|info| {
                    info.children
                        .iter()
                        .any(|child| !diff.before_node_map.contains_key(child))
                })
        })
        .collect();
    matched_pairs.sort_unstable_by_key(|&(before_id, _)| {
        before_metadata
            .node_info
            .get(&before_id)
            .map(|info| info.preorder_index)
            .unwrap_or(usize::MAX)
    });

    for (before_id, after_id) in matched_pairs {
        let Some(before_info) = before_metadata.node_info.get(&before_id) else {
            continue;
        };
        let Some(after_info) = after_metadata.node_info.get(&after_id) else {
            continue;
        };
        if before_info.children.is_empty() || after_info.children.is_empty() {
            continue;
        }

        let mut before_by_kind: HashMap<&str, Vec<usize>> = HashMap::new();
        for &child_id in &before_info.children {
            if diff.before_node_map.contains_key(&child_id) {
                continue;
            }
            if let Some(child_info) = before_metadata.node_info.get(&child_id) {
                before_by_kind
                    .entry(child_info.kind.as_str())
                    .or_default()
                    .push(child_id);
            }
        }
        if before_by_kind.is_empty() {
            continue;
        }

        let mut after_by_kind: HashMap<&str, Vec<usize>> = HashMap::new();
        for &child_id in &after_info.children {
            if diff.after_node_map.contains_key(&child_id) {
                continue;
            }
            if let Some(child_info) = after_metadata.node_info.get(&child_id) {
                after_by_kind
                    .entry(child_info.kind.as_str())
                    .or_default()
                    .push(child_id);
            }
        }

        let mut kinds: Vec<&str> = before_by_kind.keys().copied().collect();
        kinds.sort_unstable();

        for kind in kinds {
            let Some(before_candidates) = before_by_kind.get(kind) else {
                continue;
            };
            let Some(after_candidates) = after_by_kind.get(kind) else {
                continue;
            };
            let ([before_child_id], [after_child_id]) =
                (before_candidates.as_slice(), after_candidates.as_slice())
            else {
                continue;
            };
            if diff.before_node_map.contains_key(before_child_id)
                || diff.after_node_map.contains_key(after_child_id)
            {
                continue;
            }
            anchor_pair_via_apted(
                *before_child_id,
                *after_child_id,
                before_metadata,
                after_metadata,
                "unique_type_matching",
                ASTMappingReason::UniqueTypeMatching,
                diff,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::Code;
    use crate::code::Language;
    use crate::diff::ASTMapping;
    use crate::diff::NodeCache;

    /// Pre-matches the container and runs only this pass: with `solve_hash_descent` in the setup,
    /// its kind-only hash sub-anchoring could make an assertion pass without this pass.
    fn solve_with_container_pre_matched(
        before: &Code,
        after: &Code,
        container_before_id: usize,
        container_after_id: usize,
    ) -> ASTDiff {
        let node_cache = NodeCache::build(before, after);
        let mut diff = ASTDiff::default();
        diff.add_mapping(
            container_before_id,
            container_after_id,
            ASTMapping::identical(ASTMappingReason::IdenticalHash),
        );
        solve(
            &crate::diff::PassCtx::new(before, after, &node_cache),
            &mut diff,
        );
        diff
    }

    /// The `if` changes shape, so the kind-only hash cannot pair it. The block is pre-matched, not
    /// the function: otherwise the block pairs first and its APTED call resolves the `if` as a side
    /// effect, under a different reason.
    #[test]
    fn unique_leftover_child_kind_matches_under_an_already_matched_parent() {
        let before = Code::from_string(
            "fn container() {\n    if flag { x(); }\n}\n",
            &Language::Rust,
        );
        let after = Code::from_string(
            "fn container() {\n    if flag { x(); y(); }\n}\n",
            &Language::Rust,
        );

        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();
        let before_block = before_ast
            .root_node()
            .child(0)
            .unwrap()
            .child_by_field_name("body")
            .unwrap();
        let after_block = after_ast
            .root_node()
            .child(0)
            .unwrap()
            .child_by_field_name("body")
            .unwrap();
        let before_if = before_block.named_child(0).unwrap();
        let after_if = after_block.named_child(0).unwrap();
        assert_eq!(before_if.kind(), after_if.kind(), "test setup sanity check");

        let diff =
            solve_with_container_pre_matched(&before, &after, before_block.id(), after_block.id());

        assert_eq!(
            diff.before_node_map.get(&before_if.id()).copied(),
            Some(after_if.id()),
            "the sole leftover if-node on each side under the already-matched block should \
             pair via unique-type matching, not fall through unmatched"
        );
        assert_eq!(
            diff.mapping
                .get(&(before_if.id(), after_if.id()))
                .map(|m| m.reason),
            Some(ASTMappingReason::UniqueTypeMatching),
            "the pairing decision itself should be attributed to this pass"
        );
    }

    #[test]
    fn ambiguous_multiple_candidates_of_the_same_kind_do_not_match() {
        let before = Code::from_string(
            "fn container() {\n    let a = 1;\n    let b = 2;\n}\n",
            &Language::Rust,
        );
        let after = Code::from_string("fn container() {\n    let x = 9;\n}\n", &Language::Rust);

        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();
        let before_block = before_ast
            .root_node()
            .child(0)
            .unwrap()
            .child_by_field_name("body")
            .unwrap();
        let after_block = after_ast
            .root_node()
            .child(0)
            .unwrap()
            .child_by_field_name("body")
            .unwrap();
        let first_let = before_block.named_child(0).unwrap();
        let second_let = before_block.named_child(1).unwrap();

        let diff =
            solve_with_container_pre_matched(&before, &after, before_block.id(), after_block.id());

        assert!(
            !diff.before_node_map.contains_key(&first_let.id())
                && !diff.before_node_map.contains_key(&second_let.id()),
            "two unmatched `let` statements on the before side is ambiguous against one on the \
             after side - neither should be force-matched"
        );
    }
}
