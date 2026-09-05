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
use crate::code::{ASTMetadata, Language};
use crate::diff::PassCtx;
use crate::diff::nodes::{anchor_pair_via_apted, is_block_container};
use crate::diff::{ASTDiff, ASTMappingReason};

/**
* GreedyAnchorBlock: the pipeline's only *greedy, cost-estimate-driven* matcher. Every other
* container-pairing heuristic (`solve_syntax_aware_matching`, `solve_bottom_up_propagation`) decides
* "is this pair the same thing" from some form of identity signal - a shared name, a Dice
* coefficient over already-matched descendants. This pass instead estimates the actual edit cost of
* matching each candidate pair (see `sequence_edit_cost`) and accepts whichever pairs are cheap
* enough, cheapest first,
* exactly like a greedy weighted-matching algorithm - useful for containers (e.g. an `if` block, a
* loop body, a function body) that have no name, no arm structure, and no already-matched children
* for a Dice coefficient to count, but whose *content* is still obviously the same block with a few
* edits.
*
* An earlier version of this pass scored *every* candidate pair against every other (an all-pairs
* B*A search) and regressed 9
* `optimal_solutions` fixtures: `sequence_edit_cost` only looks at a pair's own direct children, so
* nothing stopped it scoring two structurally *unrelated* nodes as "cheap to match" whenever their
* children happened to hash-coincide (e.g. two different `call_expression`s - one in a `for` loop
* condition, one in a `return` statement - whose `argument_list` both happened to be `()`). Adding
* `nodes::is_block_container` narrowed the candidate *kinds* enough to fix most of that, but one
* regression survived: a byte-identical `statement_block` that the human mapping relocates into a
* newly-inserted `try_statement` wrapper scored a *near-zero* cost ratio - the cheapest possible
* match by this heuristic's own reckoning - precisely because content-only scoring has no way to
* tell "same content, same place" from "same content, moved". No `MAX_COST_RATIO` value rejects a
* ~0-ratio match without rejecting everything, so this needed a different kind of fix, not a
* stricter threshold (both problems, and this design, are documented in detail in `TODO.md` under
* "TRIED AND REVERTED (2026-07-12)" and its "2026-07-12, positional anchor" follow-up).
*
* The fix: gate candidate pairs on a *positional* signal before cost ever gets consulted -
* `positional_key_before`/`positional_key_after` walk each candidate up to its nearest already-
* matched ancestor (via `ASTMetadata::node_to_parent`) and record the kind of every node passed
* along the way. Two candidates are only ever compared if that walk lands on a *corresponding*
* ancestor pair (the before-side ancestor's counterpart, per `diff.before_node_map`, is exactly the
* after-side ancestor) *and* the kind-path from that ancestor down to each candidate is identical.
* This directly answers both regressions: the two unrelated `call_expression`s have unrelated
* ancestor paths (`for_statement > binary_expression > ...` vs. `return_statement >
* pointer_expression > ...`), so they're never even compared; the relocated `statement_block`'s
* after-side path gains an extra `try_statement` segment the before-side path doesn't have, so its
* path no longer matches and the pair is rejected regardless of how cheap its content score is.
* Nodes with no matched ancestor at all (nothing above them has been resolved yet) fall back to
* their full path from the file root, which is the same idea one level up: "no better anchor exists,
* so require literal structural correspondence from the top."
*
* Candidate generation (`collect_candidates` + the two `positional_key_*` functions above) is this
* pass's own; the grouping, scoring, greedy assignment, and defensive re-check against pairs
* claimed mid-run by an earlier accepted pair's real APTED resolution are all handled generically
* by `grouped_greedy_matcher::solve` (`TODO.md`'s "generalization of phase 4" analysis, 2026-07-18)
* - this pass just supplies the positional key as `grouped_greedy_matcher`'s compatibility key `K`
*   and `cost_ratio` as its cost function, gated by `MAX_COST_RATIO`.
*
* Called from phase 4 (`solve_syntax_aware_matching`), after named-group matching has had its
* chance: by that point every named/keyed anchor is already resolved, so whatever's left is
* genuinely anonymous containers - exactly the case this pass targets. Running before the final
* full-tree APTED pass lets it anchor big blocks cheaply first, shrinking the residual forest the
* much slower exact tree edit distance then has to grind through - the same "the more nodes are
* already matched, the faster it is" rationale documented on the APTED call site in `diff.rs`.
*
* Accepted pairs are handed to `apted::for_nodes` (via `anchor_pair_via_apted`), the same
* "pre-match, then diff for real" idiom every other heuristic pass uses: it guarantees a real
* edit-distance-based cost/operation instead of an invented one, and guarantees completeness
* (nothing under the pair is left unmapped for a later pass to miss). Only the `reason` on the
* resulting root mapping is overwritten afterward, to `GreedyAnchorBlock`, for provenance/
* explainability - and only when `for_nodes` actually matched the pair (it may still choose a
* from-scratch delete+insert if the true edit-distance cost turns out cheaper than reuse, in which
* case there's no root mapping entry to relabel).
*/
const MIN_CHILDREN: usize = 2;
const MIN_SUBTREE_SIZE: usize = 4;
/// Maximum accepted `cost_ratio` (estimated edit cost / combined subtree size) for a candidate
/// pair - 0.0 means "identical children sequence", higher means "more of the block had to be
/// deleted/inserted to align it". Only a secondary filter now that positional anchoring (see the
/// module doc comment) does the heavy lifting of rejecting structurally-unrelated pairs.
///
/// Swept 2026-07-25 (`TODO.md`'s hyperparameter-sweep entry) against `benchmark_optimal_solutions`
/// - the first time this constant was tuned at all, unlike the since-deleted `solve_bottom_up_expansion::
///   DICE_THRESHOLD`'s 2026-07-11 sweep. `{0.2, 0.35}` both landed at 829 mismatches (worse than
///   the old 0.5 default's 816 - too tight a filter rejects legitimate anchors, pushing more work
///   onto the final APTED pass), `0.5` (the old default) at 816, `{0.65}` at 813, and
///   `{0.75, 0.8, 0.85}` all tie at a **788** plateau (`0.95` regresses back to 797) - landed in
///   the middle of that plateau for the same "most margin from either regression cliff" reasoning
///   `DICE_THRESHOLD` used. Re-swept `DICE_THRESHOLD` jointly at `0.8` across `{0.8, 0.9, 0.95}`:
///   all three tie at 788 too, so the two thresholds don't meaningfully interact in this range -
///   the gain is attributable to `MAX_COST_RATIO` alone. Per-fixture:
///   `c-cpython-autogenerated-code` improved 58->33 and `cpp-laydbird-change-function-signature`
///   improved 75->60; `c-postgres-real-logic-change` regressed 8->20 (already flagged elsewhere in
///   `TODO.md` as the corpus's largest single cost-gap outlier, `algorithm_cost` 484 vs.
///   `human_cost` 49 - a looser anchor threshold plausibly lets it grab a cheaper-but-wronger
///   reuse there). Net -28 (-3.4%), 2 improved vs. 1 regressed - kept.
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

    // (id, positional key) pairs, in `collect_candidates`' preorder-sorted order - satisfies
    // `grouped_greedy_matcher`'s determinism contract directly, no extra sort needed here.
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

/// `(nearest already-matched ancestor - expressed as an after-side id, or `None` if the walk
/// reached the file root without finding one, kind path from that ancestor down to and including
/// the candidate itself)`. See the module doc comment for why both sides express this in the same
/// (after-side) coordinate space, and `positional_key_before`/`positional_key_after` for how each
/// side computes it.
type PositionalKey = (Option<usize>, Vec<String>);

/// `PositionalKey` for a before-side candidate: walks `before_id` up via `before_metadata.
/// node_to_parent`, collecting kinds, until it reaches a parent already present in
/// `diff.before_node_map` - translated to that parent's after-side counterpart, so this is directly
/// comparable to a `positional_key_after` result on the other side. Falls back to `None` (the file
/// root) if no matched ancestor is found.
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

/// `PositionalKey` for an after-side candidate: the mirror of `positional_key_before`, walking
/// `after_metadata.node_to_parent` until it reaches a parent already present in
/// `diff.after_node_map` - used directly as the anchor id (no translation needed: it's already an
/// after-side id, the same coordinate space `positional_key_before` translates into).
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

/// Every not-yet-matched node in `metadata` that is both a recognized block container (see
/// [`is_block_container`]) and big enough to bother anchoring (see `MIN_CHILDREN`/
/// `MIN_SUBTREE_SIZE`), in a fixed, parse-stable order (`preorder_index`, not raw node id - see
/// `ASTNodeMetadata::start_byte`'s doc comment) so grouping and assignment above are reproducible
/// run to run regardless of the `HashMap` iteration order this starts from.
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

/// `sequence_edit_cost`, normalized by the pair's combined subtree size so pairs of any size are
/// comparable on one scale - `0.0` means the two blocks' direct children align perfectly by hash,
/// higher means more of the block had to be paid for as a delete or an insert. Returns `None` if
/// either node's subtree size is missing (should not happen for a `collect_candidates` output) or
/// zero (nothing to compare).
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

/**
* Cheap "sequence edit distance" cost estimate for matching `before_id` <-> `after_id` as a whole:
* treats each node's *direct* children as an alphabet of opaque tokens (no recursion into
* grandchildren - this is what keeps it fast relative to a real tree edit distance, which is
* exactly why this pass can afford to try many more candidate pairs than APTED itself could), where
* two children are only considered "the same token" when their full subtree hashes are identical -
* i.e. this is blind to a child that merely *resembles* its counterpart, by design: any softer
* equality test would make the estimate as expensive as the tree edit distance it's meant to avoid.
*
* This is exactly a weighted longest-common-subsequence alignment: `dp[i][j]` is the minimum total
* subtree-size paid to turn `before`'s first `i` children into `after`'s first `j` via deletes
* (`before` child not reused) and inserts (`after` child not reused) - a matched pair costs
* nothing, everything else costs the full subtree size of whichever child didn't survive. There is
* deliberately no separate "substitute" move: aligning a non-identical pair diagonally would cost
* exactly `size(before_child) + size(after_child)`, the same total as deleting one and inserting
* the other via two separate steps, so omitting it changes nothing about the minimum while keeping
* the recurrence to two cases instead of three.
*
* Returns `None` if either node's metadata can't be found (should not happen for a node that came
* from `collect_candidates`, but this stays defensive rather than panicking on a corrupt candidate).
*/
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
        // Neither `if` has a name or arm-signature overlap for the other heuristics to key on
        // (single-arm `if`, no `else`), but the block's body is 4/5 identical statements - well
        // under `MAX_COST_RATIO`. The enclosing function is matched first (so the `if`-blocks
        // share a positional anchor), and the `block` node itself (not the enclosing
        // `if_expression`) is the candidate that wins - see the equivalent comment this test had
        // before positional anchoring, kept in `TODO.md`'s writeup, for why.
        let before_src = "fn f(x: i32) {\n    if x > 0 {\n        let a = 1;\n        let b = 2;\n        let c = 3;\n        let d = 4;\n        let e = 5;\n    }\n}\n";
        let after_src = "fn f(x: i32) {\n    if x > 0 {\n        let a = 1;\n        let b = 2;\n        let c = 3;\n        let d = 4;\n        let e = 99;\n    }\n}\n";
        let before = Code::from_string(before_src, &Language::Rust);
        let after = Code::from_string(after_src, &Language::Rust);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        // Match everything except the `if` block's own body - simulating an earlier pass having
        // already resolved the function signature but not descended into an unnamed `if`.
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
        // Two `if` blocks whose bodies happen to hash-coincide closely, but which sit under
        // different (both unmatched) parents with no shared positional anchor - regression guard
        // for the original bug (an unrelated `call_expression` pair matched via coincidental
        // content similarity). Neither function is matched here, so each `if`-block's nearest
        // matched ancestor walk reaches the file root with a *different* full path (different
        // function names aren't part of the kind-path, but the two functions themselves are
        // distinct un-matched nodes, so the two blocks are in different, uncorrelated groups only
        // if their full root-relative kind-paths differ - construct the source so they do, via a
        // different nesting depth on one side).
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
