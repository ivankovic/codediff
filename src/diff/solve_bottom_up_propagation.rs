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
use crate::code::Code;
use crate::diff::nodes::{anchor_pair_via_apted, kinds_update_allowed};
use crate::diff::{ASTDiff, ASTMapping, ASTMappingOperation, ASTMappingReason, COST_DELETE};
use crate::diff::{COST_INSERT, NodeCache};

/// Phase-2 of the phases-4-7 rearchitecture (`TODO.md`,
/// `~/.claude/plans/iterative-herding-panda.md`): a strict, unconditional bottom-up propagation
/// rule, replacing (once measured net-positive-or-neutral and made the default -
/// `HeuristicConfig::solver_bottom_up_propagation`) the Dice-threshold `solve_bottom_up_expansion`
/// for this same "a parent's children are already resolved enough to resolve the parent too" role.
///
/// For an unmatched before-node `B` with children `c1..ck` (`k >= 1`):
/// 1. Every direct child of `B` must already have a decision (matched or deleted) - if any child
///    is still undecided, `B` is skipped this round (it may resolve on a later call once its
///    child resolves).
/// 2. If every child is deleted, `B` becomes `DeleteWithChildren` (mirrored on the after side for
///    `InsertWithChildren`).
/// 3. If every child is matched, and every one of them maps into the exact same direct after-side
///    parent `P` (via `after_metadata.node_to_parent`, not "somewhere under `P`"), and `P` is
///    itself unmapped, and `kind(B)`/`kind(P)` are compatible per `kinds_update_allowed`, `B` is
///    proposed to real (but now trivially cheap, since every descendant is already resolved)
///    APTED via `anchor_pair_via_apted` - the same "propose a pair, let real tree-edit-distance
///    confirm and cost it" idiom `solve_bottom_up_expansion`/`solve_greedy_anchor_blocks` use, so
///    the result's cost/operation is never invented.
/// 4. Any other case - mixed matched/deleted children, disagreeing after-parents, or kind
///    incompatibility - blocks `B` unconditionally. No partial credit, no threshold, no vote.
///
/// This only ever fires on a child-forced, 100%-consistent answer, so - unlike
/// `solve_bottom_up_expansion`'s Dice-threshold rule, which accepts a parent on 90% descendant
/// coverage and then lets APTED *improvise* a verdict for the uncovered remainder - it never has
/// an invented decision to fight a later, more precise pass over. Consistent with
/// `UnitCostModel::ren`'s own premise (`apted/common.rs`): a parent's correctness is validated
/// bottom-up by its children's real costs, never asserted top-down by a heuristic guess.
///
/// Runs once, at the top of `PendingDiff::finish`, before the terminal Myers-LCS fallback
/// (`apted::for_roots_fallback`) - shrinking what that lossy, whole-subtree-only fallback has to
/// guess at. The `DeleteWithChildren`/`InsertWithChildren` branches (step 2) can't fire yet at
/// this point in the still-partial rearchitecture: nothing before the terminal fallback marks a
/// node deleted or inserted today, so no child ever has that decision to propagate. They start
/// mattering once Phase 3 (per-region dispatch) introduces intermediate-granularity deletes/
/// inserts ahead of the terminal catch-all - implemented now so this module doesn't need
/// revisiting when that lands.
pub fn solve(before: &Code, after: &Code, _node_cache: &NodeCache, diff: &mut ASTDiff) {
    let before_metadata = crate::code::metadata::metadata_of(before);
    let after_metadata = crate::code::metadata::metadata_of(after);
    let language = before_metadata.language;

    // Deepest nodes first (ties broken by `preorder_index`, not node id - see
    // `ASTNodeMetadata::start_byte`'s doc comment on why raw ids aren't parse-stable), same
    // ordering convention as `solve_bottom_up_expansion` - by the time an ancestor is considered,
    // every descendant already had its chance to resolve in this same call, so one call can
    // propagate a resolution several levels up the tree.
    let mut before_candidates: Vec<usize> = before_metadata.node_to_depth.keys().copied().collect();
    sort_deepest_first(&mut before_candidates, &before_metadata);

    for before_id in before_candidates {
        if diff.before_node_map.contains_key(&before_id) {
            continue;
        }
        let Some(before_info) = before_metadata.node_info.get(&before_id) else {
            continue;
        };
        if before_info.children.is_empty() {
            continue;
        }

        let mut all_deleted = true;
        let mut matched_after_parent: Option<usize> = None;
        let mut consistent = true;
        for &child_id in &before_info.children {
            match diff.before_node_map.get(&child_id) {
                None => {
                    // Undecided child - `B` isn't resolvable yet this round.
                    consistent = false;
                    all_deleted = false;
                    break;
                }
                Some(&0) => {
                    // Deleted child.
                }
                Some(&after_child_id) => {
                    all_deleted = false;
                    let Some(&after_parent_id) = after_metadata.node_to_parent.get(&after_child_id)
                    else {
                        consistent = false;
                        break;
                    };
                    match matched_after_parent {
                        None => matched_after_parent = Some(after_parent_id),
                        Some(existing) if existing != after_parent_id => {
                            consistent = false;
                            break;
                        }
                        Some(_) => {}
                    }
                }
            }
        }
        if !consistent {
            continue;
        }

        if all_deleted {
            diff.add_mapping(
                before_id,
                0,
                ASTMapping {
                    cost: COST_DELETE,
                    operation: ASTMappingOperation::DeleteWithChildren,
                    reason: ASTMappingReason::BottomUpPropagation,
                },
            );
            continue;
        }

        let Some(after_id) = matched_after_parent else {
            continue;
        };
        if diff.after_node_map.contains_key(&after_id) {
            continue;
        }
        let Some(after_info) = after_metadata.node_info.get(&after_id) else {
            continue;
        };
        if before_info.kind != after_info.kind
            && !kinds_update_allowed(&before_info.kind, &after_info.kind, &language)
        {
            continue;
        }

        anchor_pair_via_apted(
            before_id,
            after_id,
            &before_metadata,
            &after_metadata,
            "bottom_up_propagation",
            ASTMappingReason::BottomUpPropagation,
            diff,
        );
    }

    // Symmetric after-side sweep: an unmatched after-node whose every direct child is inserted
    // becomes `InsertWithChildren`. The "all matched into the same before-parent" case never
    // needs a separate after-side pass - it's already fully covered by the before-side sweep
    // above (matching `B` to `P` sets both `before_node_map[B]` and `after_node_map[P]` via
    // `ASTDiff::add_mapping`).
    let mut after_candidates: Vec<usize> = after_metadata.node_to_depth.keys().copied().collect();
    sort_deepest_first(&mut after_candidates, &after_metadata);

    for after_id in after_candidates {
        if diff.after_node_map.contains_key(&after_id) {
            continue;
        }
        let Some(after_info) = after_metadata.node_info.get(&after_id) else {
            continue;
        };
        if after_info.children.is_empty() {
            continue;
        }
        let all_inserted = after_info
            .children
            .iter()
            .all(|child_id| matches!(diff.after_node_map.get(child_id), Some(&0)));
        let any_undecided = after_info
            .children
            .iter()
            .any(|child_id| !diff.after_node_map.contains_key(child_id));
        if any_undecided || !all_inserted {
            continue;
        }

        diff.add_mapping(
            0,
            after_id,
            ASTMapping {
                cost: COST_INSERT,
                operation: ASTMappingOperation::InsertWithChildren,
                reason: ASTMappingReason::BottomUpPropagation,
            },
        );
    }
}

/// Deepest-first, ties broken by `preorder_index` for determinism regardless of `FxHashMap`
/// iteration order - see `ASTNodeMetadata::start_byte`'s doc comment on why raw node ids aren't
/// parse-stable sort/tiebreak keys.
fn sort_deepest_first(candidates: &mut [usize], metadata: &crate::code::ASTMetadata) {
    candidates.sort_by(|&a, &b| {
        let depth_a = metadata.node_to_depth.get(&a).copied().unwrap_or(0);
        let depth_b = metadata.node_to_depth.get(&b).copied().unwrap_or(0);
        depth_b.cmp(&depth_a).then_with(|| {
            let pre_a = metadata
                .node_info
                .get(&a)
                .map(|i| i.preorder_index)
                .unwrap_or(0);
            let pre_b = metadata
                .node_info
                .get(&b)
                .map(|i| i.preorder_index)
                .unwrap_or(0);
            pre_a.cmp(&pre_b)
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::Language;
    use crate::diff::NodeCache;

    fn solve_only(before: &Code, after: &Code) -> ASTDiff {
        let node_cache = NodeCache::build(before, after);
        let mut diff = ASTDiff::default();
        crate::diff::solve_hash_descent::solve(before, after, &node_cache, &mut diff);
        solve(before, after, &node_cache, &mut diff);
        diff
    }

    /// A function renamed (so its own hash never matches) but whose body is byte-identical to a
    /// counterpart elsewhere must be matched via propagation once all its children resolve.
    #[test]
    fn renamed_function_with_identical_body_matches_via_propagation() {
        let before = Code::from_string("fn old_name() { let q = 1 + 2; q }\n", &Language::Rust);
        let after = Code::from_string("fn new_name() { let q = 1 + 2; q }\n", &Language::Rust);

        let diff = solve_only(&before, &after);

        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();
        let before_fn = before_ast.root_node().child(0).unwrap();
        let after_fn = after_ast.root_node().child(0).unwrap();
        assert_eq!(
            diff.before_node_map.get(&before_fn.id()).copied(),
            Some(after_fn.id()),
            "the renamed function itself should match its counterpart, not delete+insert"
        );
    }

    /// Two children matching into *different* after-parents must not force a match - no
    /// plurality vote, no partial credit.
    #[test]
    fn disagreeing_after_parents_block_the_match() {
        let before = Code::from_string(
            "fn a() { 1; }\nfn b() { 2; }\nfn container() { 1; 2; }\n",
            &Language::Rust,
        );
        let after = Code::from_string("fn a() { 1; }\nfn b() { 2; }\n", &Language::Rust);

        let diff = solve_only(&before, &after);

        let before_ast = before.ast.as_ref().unwrap();
        let container = before_ast.root_node().child(2).unwrap();
        assert!(
            !diff.before_node_map.contains_key(&container.id()),
            "container's two statements match into two different before-functions' bodies, not \
             one - it must stay unmatched, not get force-matched to either"
        );
    }
}
