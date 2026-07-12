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

/**
* Total edit cost of a *finished* mapping between two subtrees - not APTED's internal DP (which
* only ever sees a small unmatched residual, see `apted::UnitCostModel`), but a single number for
* "how expensive is this complete before-to-after mapping, root to every leaf."
*
* Two callers need this and must agree on the same numbers, or "is codediff's mapping cheaper or
* more expensive than the human's" stops being a meaningful question:
* - `diff_cost`, over codediff's own `ASTDiff` output.
* - `crate::test::helper::human_mapping::human_mapping_cost`, over a human-authored
*   `human_mapping.json` (kept in `human_mapping.rs`, not here, since it depends on
*   `test`-only path-resolution helpers - this module stays free of that dependency).
*
* Both reduce to summing `operation_cost` over a flat list of (operation, subtree size) pairs, one
* per mapping entry - see that function's doc comment for the per-operation cost table this
* encodes, and why matched-but-not-identical nodes cost 0 at the root rather than double-counting
* their descendants' own entries.
*
* This is the "current cost" baseline the cost model can be extended from: `benchmark_optimal_
* solutions` prints both sides' totals per fixture so a future cost-model change can be judged by
* whether it moves codediff's cost *toward* the human's, not just by the existing node-by-node
* mismatch count.
*/
use crate::code::ASTMetadata;
use crate::diff::{ASTDiff, ASTMappingOperation, COST_DELETE, COST_INSERT, COST_MOVE, COST_UPDATE};

/**
* Unit cost of one mapping entry with the given `operation`, whose subtree has `subtree_size`
* nodes (only consulted for the two `*WithChildren` operations - everything else is a single-node
* cost, since a matched internal node's differing descendants each get their own separate entry
* rather than being folded into their ancestor's cost).
*
* Mirrors `apted::common::UnitCostModel`'s per-operation costs, generalized from "match/rename a
* pair of node labels" (what APTED's DP evaluates candidate by candidate) to "here is the fully
* resolved operation for this entry" (what a finished mapping already records):
* - `Identical`, `MatchButNotIdentical`, `Move`, `DoNothing`, `NotYetSet` -> 0. A same-kind
*   internal-node match costs nothing at the root; `UnitCostModel::ren` already returns 0 for that
*   case for the same reason - the real cost of any actual difference inside the subtree shows up
*   as its own, separate entries for the differing descendants, and double-charging the ancestor
*   for them would count that cost twice.
* - `Update` -> `COST_UPDATE`, `Delete` -> `COST_DELETE`, `Insert` -> `COST_INSERT`: single-node
*   costs, matching `UnitCostModel::del`/`ins`/`ren`'s leaf-rename case exactly.
* - `DeleteWithChildren`/`InsertWithChildren` -> `COST_DELETE`/`COST_INSERT` times `subtree_size`:
*   these operations fold an entire subtree into one entry (no separate per-descendant entries),
*   so the entry's own cost has to stand in for all of them at once. Not currently produced by any
*   pipeline pass (`add_delete_mappings`/`add_insert_mappings` always recurse to one entry per
*   node - see `apted::common`), but a human-authored mapping uses them routinely, so this must be
*   handled correctly for `human_mapping_cost` even though `diff_cost` should never hit this arm in
*   practice today.
*/
pub fn operation_cost(operation: &ASTMappingOperation, subtree_size: usize) -> u64 {
    match operation {
        ASTMappingOperation::Identical
        | ASTMappingOperation::MatchButNotIdentical
        | ASTMappingOperation::DoNothing
        | ASTMappingOperation::NotYetSet => 0,
        ASTMappingOperation::Move => COST_MOVE,
        ASTMappingOperation::Update => COST_UPDATE,
        ASTMappingOperation::Delete => COST_DELETE,
        ASTMappingOperation::Insert => COST_INSERT,
        ASTMappingOperation::DeleteWithChildren => COST_DELETE * subtree_size as u64,
        ASTMappingOperation::InsertWithChildren => COST_INSERT * subtree_size as u64,
    }
}

/**
* Total cost of a finished `ASTDiff`: sums `operation_cost` over every entry in `diff.mapping`: one
* entry per node (either half of a matched pair, or a lone delete/insert), so this is a straight
* sum with no double-counting.
*
* `before_metadata`/`after_metadata` supply subtree sizes for `*WithChildren` operations - see
* `operation_cost`'s doc comment on why those should never actually appear here today.
*/
pub fn diff_cost(diff: &ASTDiff, before_metadata: &ASTMetadata, after_metadata: &ASTMetadata) -> u64 {
    diff.mapping
        .iter()
        .map(|(&(before_id, after_id), m)| {
            let subtree_size = match m.operation {
                ASTMappingOperation::DeleteWithChildren => before_metadata
                    .node_to_subtree_size
                    .get(&before_id)
                    .copied()
                    .unwrap_or(1),
                ASTMappingOperation::InsertWithChildren => after_metadata
                    .node_to_subtree_size
                    .get(&after_id)
                    .copied()
                    .unwrap_or(1),
                _ => 1,
            };
            operation_cost(&m.operation, subtree_size)
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{ASTMapping, ASTMappingReason};

    fn metadata_with_sizes(sizes: &[(usize, usize)]) -> ASTMetadata {
        let mut metadata = ASTMetadata::default();
        for &(id, size) in sizes {
            metadata.node_to_subtree_size.insert(id, size);
        }
        metadata
    }

    fn mapping(operation: ASTMappingOperation) -> ASTMapping {
        ASTMapping { cost: 0, operation, reason: ASTMappingReason::APTED("test") }
    }

    #[test]
    fn identical_and_match_but_not_identical_cost_nothing() {
        assert_eq!(operation_cost(&ASTMappingOperation::Identical, 50), 0);
        assert_eq!(operation_cost(&ASTMappingOperation::MatchButNotIdentical, 50), 0);
        assert_eq!(operation_cost(&ASTMappingOperation::Move, 50), COST_MOVE);
    }

    #[test]
    fn single_node_operations_cost_one_regardless_of_subtree_size() {
        assert_eq!(operation_cost(&ASTMappingOperation::Update, 50), COST_UPDATE);
        assert_eq!(operation_cost(&ASTMappingOperation::Delete, 50), COST_DELETE);
        assert_eq!(operation_cost(&ASTMappingOperation::Insert, 50), COST_INSERT);
    }

    #[test]
    fn with_children_operations_scale_by_subtree_size() {
        assert_eq!(operation_cost(&ASTMappingOperation::DeleteWithChildren, 7), 7 * COST_DELETE);
        assert_eq!(operation_cost(&ASTMappingOperation::InsertWithChildren, 3), 3 * COST_INSERT);
    }

    #[test]
    fn diff_cost_sums_single_node_entries_without_double_counting() {
        let mut diff = ASTDiff::default();
        // A matched pair (1 <-> 1), one real content update elsewhere (2 <-> 2), one delete (3),
        // one insert (4). Matches `add_delete_mappings`/`add_insert_mappings`'s "one entry per
        // node" shape.
        diff.add_mapping(1, 1, mapping(ASTMappingOperation::Identical));
        diff.add_mapping(2, 2, mapping(ASTMappingOperation::Update));
        diff.add_mapping(3, 0, mapping(ASTMappingOperation::Delete));
        diff.add_mapping(0, 4, mapping(ASTMappingOperation::Insert));

        let before_meta = ASTMetadata::default();
        let after_meta = ASTMetadata::default();

        assert_eq!(
            diff_cost(&diff, &before_meta, &after_meta),
            COST_UPDATE + COST_DELETE + COST_INSERT
        );
    }

    #[test]
    fn diff_cost_scales_with_children_entries_by_metadata_subtree_size() {
        let mut diff = ASTDiff::default();
        diff.add_mapping(10, 0, mapping(ASTMappingOperation::DeleteWithChildren));
        diff.add_mapping(0, 20, mapping(ASTMappingOperation::InsertWithChildren));

        let before_meta = metadata_with_sizes(&[(10, 6)]);
        let after_meta = metadata_with_sizes(&[(20, 4)]);

        assert_eq!(
            diff_cost(&diff, &before_meta, &after_meta),
            6 * COST_DELETE + 4 * COST_INSERT
        );
    }
}
