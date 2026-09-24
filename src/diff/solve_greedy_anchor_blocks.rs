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

//! GreedyAnchorBlock: pairs anonymous block containers (an `if` body, a loop body) by an estimated
//! edit cost (`cost_ratio`), cheapest first, through `grouped_greedy_matcher`. The only matcher
//! driven by cost rather than an identity signal; accepted pairs go to real APTED
//! (`anchor_pair_via_apted`), so the cost and operation are never invented.
//!
//! Candidates are compared only when they share a positional key: the same corresponding
//! nearest-matched ancestor and the same kind path down from it (the full root path when nothing
//! above is matched). Content scoring alone cannot tell "same content, same place" from "same
//! content, moved", and no `MAX_COST_RATIO` rejects a near-zero coincidental match without
//! rejecting everything, so the position gate is the fix, not a stricter threshold.

use crate::code::{ASTMetadata, Language};
use crate::diff::PassCtx;
use crate::diff::nodes::{anchor_pair_via_apted, is_block_container};
use crate::diff::{ASTDiff, ASTMappingReason};

const MIN_CHILDREN: usize = 2;
const MIN_SUBTREE_SIZE: usize = 4;
/// Maximum accepted `cost_ratio`; a secondary filter behind the positional gate. Sits mid-plateau:
/// tighter pushes legitimate anchors to the terminal APTED pass, looser buys cheaper-but-wronger
/// reuse.
const MAX_COST_RATIO: f64 = 0.8;

pub fn solve(ctx: &PassCtx, diff: &mut ASTDiff) {
    let before_metadata = ctx.before_metadata();
    let after_metadata = ctx.after_metadata();
    let language = before_metadata.language;

    let before_candidate_ids =
        collect_candidates(before_metadata, &diff.before_node_map, &language);
    let after_candidate_ids = collect_candidates(after_metadata, &diff.after_node_map, &language);
    if before_candidate_ids.is_empty() || after_candidate_ids.is_empty() {
        return;
    }

    // `collect_candidates` is preorder-sorted, as `grouped_greedy_matcher` requires.
    let before_candidates: Vec<(usize, PositionalKey)> = before_candidate_ids
        .iter()
        .map(|&id| (id, positional_key_before(id, before_metadata, diff)))
        .collect();
    let after_candidates: Vec<(usize, PositionalKey)> = after_candidate_ids
        .iter()
        .map(|&id| (id, positional_key_after(id, after_metadata, diff)))
        .collect();

    crate::diff::grouped_greedy_matcher::solve(
        diff,
        &before_candidates,
        &after_candidates,
        |before_id, after_id| {
            cost_ratio(before_id, after_id, before_metadata, after_metadata)
                .unwrap_or(f64::INFINITY)
        },
        Some(MAX_COST_RATIO),
        |before_id, after_id, diff| {
            anchor_pair_via_apted(
                before_id,
                after_id,
                before_metadata,
                after_metadata,
                "greedy_anchor_block",
                ASTMappingReason::GreedyAnchorBlock,
                diff,
            );
        },
    );
}

/// `(nearest matched ancestor as an after-side id, or `None` at the file root; kind path from it
/// down to the candidate)`. Both sides use after-side ids so the keys compare directly.
type PositionalKey = (Option<usize>, Vec<String>);

fn positional_key_before(
    before_id: usize,
    before_metadata: &ASTMetadata,
    diff: &ASTDiff,
) -> PositionalKey {
    let mut path = vec![node_kind(before_id, before_metadata)];
    let mut cur = before_id;
    loop {
        let Some(&parent_id) = before_metadata.node_to_parent.get(&cur) else {
            path.reverse();
            return (None, path);
        };
        if let Some(&after_counterpart) = diff.before_node_map.get(&parent_id) {
            path.reverse();
            return (Some(after_counterpart), path);
        }
        path.push(node_kind(parent_id, before_metadata));
        cur = parent_id;
    }
}

fn positional_key_after(
    after_id: usize,
    after_metadata: &ASTMetadata,
    diff: &ASTDiff,
) -> PositionalKey {
    let mut path = vec![node_kind(after_id, after_metadata)];
    let mut cur = after_id;
    loop {
        let Some(&parent_id) = after_metadata.node_to_parent.get(&cur) else {
            path.reverse();
            return (None, path);
        };
        if diff.after_node_map.contains_key(&parent_id) {
            path.reverse();
            return (Some(parent_id), path);
        }
        path.push(node_kind(parent_id, after_metadata));
        cur = parent_id;
    }
}

fn node_kind(id: usize, metadata: &ASTMetadata) -> String {
    metadata
        .node_info
        .get(&id)
        .map(|info| info.kind.clone())
        .unwrap_or_default()
}

/// Unmatched block containers large enough to anchor, in `preorder_index` order (ids are not
/// parse-stable).
fn collect_candidates(
    metadata: &ASTMetadata,
    mapped: &rustc_hash::FxHashMap<usize, usize>,
    language: &Language,
) -> Vec<usize> {
    let mut candidates: Vec<usize> = metadata
        .node_info
        .iter()
        .filter(|(id, info)| {
            !mapped.contains_key(id)
                && is_block_container(&info.kind, language)
                && info.children.len() >= MIN_CHILDREN
                && metadata.node_to_subtree_size.get(id).copied().unwrap_or(0) >= MIN_SUBTREE_SIZE
        })
        .map(|(&id, _)| id)
        .collect();
    candidates.sort_by_key(|id| {
        metadata
            .node_info
            .get(id)
            .map(|info| info.preorder_index)
            .unwrap_or(0)
    });
    candidates
}

/// `sequence_edit_cost` over the pair's combined subtree size: `0.0` when the direct children align
/// perfectly by hash. `None` when a size is missing or zero.
pub(crate) fn cost_ratio(
    before_id: usize,
    after_id: usize,
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
) -> Option<f64> {
    let cost = sequence_edit_cost(before_id, after_id, before_metadata, after_metadata)?;
    let before_size = *before_metadata.node_to_subtree_size.get(&before_id)?;
    let after_size = *after_metadata.node_to_subtree_size.get(&after_id)?;
    let combined = (before_size + after_size) as f64;
    if combined == 0.0 {
        return None;
    }
    Some(cost as f64 / combined)
}

/// Weighted LCS over the two nodes' *direct* children, compared only by full subtree hash: a
/// reused child is free, any other costs its subtree size. Blind to a child that merely resembles
/// its counterpart by design; a softer equality would cost as much as the tree edit distance this
/// estimate avoids. No substitute move: it would cost exactly a delete plus an insert.
fn sequence_edit_cost(
    before_id: usize,
    after_id: usize,
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
) -> Option<u64> {
    let before_children = &before_metadata.node_info.get(&before_id)?.children;
    let after_children = &after_metadata.node_info.get(&after_id)?.children;

    let before_weights: Vec<u64> = before_children
        .iter()
        .map(|id| {
            before_metadata
                .node_to_subtree_size
                .get(id)
                .copied()
                .unwrap_or(1) as u64
        })
        .collect();
    let after_weights: Vec<u64> = after_children
        .iter()
        .map(|id| {
            after_metadata
                .node_to_subtree_size
                .get(id)
                .copied()
                .unwrap_or(1) as u64
        })
        .collect();

    let n = before_children.len();
    let m = after_children.len();
    let mut dp = vec![vec![0u64; m + 1]; n + 1];
    for i in 1..=n {
        dp[i][0] = dp[i - 1][0] + before_weights[i - 1];
    }
    for j in 1..=m {
        dp[0][j] = dp[0][j - 1] + after_weights[j - 1];
    }
    for i in 1..=n {
        let before_hash = before_metadata
            .node_to_full_hash
            .get(&before_children[i - 1]);
        for j in 1..=m {
            let after_hash = after_metadata.node_to_full_hash.get(&after_children[j - 1]);
            dp[i][j] = if before_hash.is_some() && before_hash == after_hash {
                dp[i - 1][j - 1]
            } else {
                (dp[i - 1][j] + before_weights[i - 1]).min(dp[i][j - 1] + after_weights[j - 1])
            };
        }
    }
    Some(dp[n][m])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::Code;
    use crate::code::Language;
    use crate::diff::NodeCache;
    use crate::test::helper::find_first_of_kind as first_child_of_kind;

    #[test]
    fn identical_children_sequence_costs_nothing() {
        let before_src = "fn f() {\n    let a = 1;\n    let b = 2;\n    let c = 3;\n}\n";
        let after_src = before_src;
        let before = Code::from_string(before_src, &Language::Rust);
        let after = Code::from_string(after_src, &Language::Rust);
        let before_metadata = crate::code::metadata::metadata_of(&before);
        let after_metadata = crate::code::metadata::metadata_of(&after);

        let before_block =
            first_child_of_kind(before.ast.as_ref().unwrap().root_node(), "block").unwrap();
        let after_block =
            first_child_of_kind(after.ast.as_ref().unwrap().root_node(), "block").unwrap();

        let cost = sequence_edit_cost(
            before_block.id(),
            after_block.id(),
            &before_metadata,
            &after_metadata,
        )
        .unwrap();
        assert_eq!(cost, 0);
    }

    #[test]
    fn one_changed_statement_only_costs_that_statement() {
        let before_src = "fn f() {\n    let a = 1;\n    let b = 2;\n    let c = 3;\n}\n";
        let after_src = "fn f() {\n    let a = 1;\n    let b = 99;\n    let c = 3;\n}\n";
        let before = Code::from_string(before_src, &Language::Rust);
        let after = Code::from_string(after_src, &Language::Rust);
        let before_metadata = crate::code::metadata::metadata_of(&before);
        let after_metadata = crate::code::metadata::metadata_of(&after);

        let before_block =
            first_child_of_kind(before.ast.as_ref().unwrap().root_node(), "block").unwrap();
        let after_block =
            first_child_of_kind(after.ast.as_ref().unwrap().root_node(), "block").unwrap();

        let cost = sequence_edit_cost(
            before_block.id(),
            after_block.id(),
            &before_metadata,
            &after_metadata,
        )
        .unwrap();
        let before_stmt_size = *before_metadata
            .node_to_subtree_size
            .get(
                &before_metadata
                    .node_info
                    .get(&before_block.id())
                    .unwrap()
                    .children[1],
            )
            .unwrap();
        let after_stmt_size = *after_metadata
            .node_to_subtree_size
            .get(
                &after_metadata
                    .node_info
                    .get(&after_block.id())
                    .unwrap()
                    .children[1],
            )
            .unwrap();
        assert_eq!(cost, (before_stmt_size + after_stmt_size) as u64);
        assert!(cost > 0);
        let whole_block_size = *before_metadata
            .node_to_subtree_size
            .get(&before_block.id())
            .unwrap();
        assert!(
            (cost as usize) < whole_block_size,
            "changing one statement should be far cheaper than the whole block"
        );
    }

    #[test]
    fn anonymous_if_block_with_mostly_identical_body_is_anchored() {
        // A single-arm `if` has no identity signal; its body is 4/5 identical statements.
        let before_src = "fn f(x: i32) {\n    if x > 0 {\n        let a = 1;\n        let b = 2;\n        let c = 3;\n        let d = 4;\n        let e = 5;\n    }\n}\n";
        let after_src = "fn f(x: i32) {\n    if x > 0 {\n        let a = 1;\n        let b = 2;\n        let c = 3;\n        let d = 4;\n        let e = 99;\n    }\n}\n";
        let before = Code::from_string(before_src, &Language::Rust);
        let after = Code::from_string(after_src, &Language::Rust);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        let before_fn =
            first_child_of_kind(before.ast.as_ref().unwrap().root_node(), "function_item").unwrap();
        let after_fn =
            first_child_of_kind(after.ast.as_ref().unwrap().root_node(), "function_item").unwrap();
        diff.add_mapping(
            before_fn.id(),
            after_fn.id(),
            crate::diff::ASTMapping::matched_not_identical(ASTMappingReason::APTED("test")),
        );

        solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );

        let before_if = first_child_of_kind(before_fn, "if_expression").unwrap();
        let after_if = first_child_of_kind(after_fn, "if_expression").unwrap();
        let before_block = first_child_of_kind(before_if, "block").unwrap();
        let after_block = first_child_of_kind(after_if, "block").unwrap();
        let mapping = diff
            .mapping
            .get(&(before_block.id(), after_block.id()))
            .expect("the two if-block bodies should be anchored to each other");
        assert_eq!(mapping.reason, ASTMappingReason::GreedyAnchorBlock);
    }

    #[test]
    fn completely_different_blocks_are_not_anchored() {
        let before_src =
            "fn f() {\n    let a = 1;\n    let b = 2;\n    let c = 3;\n    let d = 4;\n}\n";
        let after_src = "fn g() {\n    println!(\"one\");\n    println!(\"two\");\n    println!(\"three\");\n    println!(\"four\");\n}\n";
        let before = Code::from_string(before_src, &Language::Rust);
        let after = Code::from_string(after_src, &Language::Rust);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );

        let before_block =
            first_child_of_kind(before.ast.as_ref().unwrap().root_node(), "block").unwrap();
        let after_block =
            first_child_of_kind(after.ast.as_ref().unwrap().root_node(), "block").unwrap();
        assert!(
            !diff
                .mapping
                .contains_key(&(before_block.id(), after_block.id())),
            "blocks with no shared content should not be anchored"
        );
    }

    #[test]
    fn blocks_in_unrelated_structural_positions_are_not_anchored_even_with_similar_content() {
        // Similar bodies, but different root-relative kind paths and no matched ancestor.
        let before_src = "fn f() {\n    if true {\n        let a = 1;\n        let b = 2;\n        let c = 3;\n    }\n}\n";
        let after_src = "fn g() {\n    if true {\n        if true {\n            let a = 1;\n            let b = 2;\n            let c = 3;\n        }\n    }\n}\n";
        let before = Code::from_string(before_src, &Language::Rust);
        let after = Code::from_string(after_src, &Language::Rust);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );

        let before_if =
            first_child_of_kind(before.ast.as_ref().unwrap().root_node(), "if_expression").unwrap();
        let before_block = first_child_of_kind(before_if, "block").unwrap();
        let after_root = after.ast.as_ref().unwrap().root_node();
        let after_inner_if = first_child_of_kind(
            first_child_of_kind(after_root, "if_expression").unwrap(),
            "if_expression",
        )
        .unwrap();
        let after_block = first_child_of_kind(after_inner_if, "block").unwrap();

        assert!(
            !diff
                .mapping
                .contains_key(&(before_block.id(), after_block.id())),
            "blocks at different structural depths (different kind-paths) should not be anchored"
        );
    }
}
