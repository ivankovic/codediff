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
use crate::diff::{ASTDiff, ASTMappingOperation, COST_DELETE, COST_INSERT, COST_UPDATE};

/**
* Unit cost of one mapping entry with the given `operation`, whose subtree has `subtree_size`
* nodes (only consulted for the two `*WithChildren` operations - everything else is a single-node
* cost, since a matched internal node's differing descendants each get their own separate entry
* rather than being folded into their ancestor's cost).
*
* Mirrors `apted::common::UnitCostModel`'s per-operation costs, generalized from "match/rename a
* pair of node labels" (what APTED's DP evaluates candidate by candidate) to "here is the fully
* resolved operation for this entry" (what a finished mapping already records):
* - `Identical` and `NotYetSet` -> 0, and `MatchButNotIdentical` -> 0 *unless*
*   `owned_text_changed`. A same-kind internal-node match costs nothing at the root because the
*   real cost of any difference inside the subtree shows up as its own, separate entries for the
*   differing descendants, and double-charging the ancestor would count it twice - the same premise
*   `UnitCostModel::ren` rests on. The exception is a node that owns text *directly*, in the gaps
*   its children don't cover: there is no descendant entry carrying that difference, so charging 0
*   loses it outright. That is not hypothetical - it is why
*   `yaml-draios-sysdig-string-url-change` scored `algorithm_cost 0 / human_cost 0` for a file in
*   which six URLs changed. See `ASTNodeMetadata::owned_text_hash` for how widespread gap-owned
*   text is (every XML attribute value, every CSS numeric and colour literal, every Rust comment).
*   Priced at `COST_UPDATE`, exactly as the equivalent leaf change would be.
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
pub fn operation_cost(
    operation: &ASTMappingOperation,
    subtree_size: usize,
    owned_text_changed: bool,
) -> u64 {
    match operation {
        ASTMappingOperation::MatchButNotIdentical if owned_text_changed => COST_UPDATE,
        ASTMappingOperation::Identical
        | ASTMappingOperation::MatchButNotIdentical
        | ASTMappingOperation::NotYetSet => 0,
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
pub fn diff_cost(
    diff: &ASTDiff,
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
) -> u64 {
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
            // Only meaningful for a matched pair; `before_id`/`after_id` is 0 on the missing side
            // of a lone delete/insert, and a lookup for it simply finds nothing.
            let owned_text_hash = |metadata: &ASTMetadata, id: usize| {
                metadata.node_info.get(&id).map(|info| info.owned_text_hash)
            };
            let owned_text_changed = owned_text_hash(before_metadata, before_id)
                != owned_text_hash(after_metadata, after_id);
            operation_cost(&m.operation, subtree_size, owned_text_changed)
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
        ASTMapping {
            cost: 0,
            operation,
            reason: ASTMappingReason::APTED("test"),
        }
    }

    #[test]
    fn identical_and_unchanged_match_but_not_identical_cost_nothing() {
        assert_eq!(
            operation_cost(&ASTMappingOperation::Identical, 50, false),
            0
        );
        assert_eq!(
            operation_cost(&ASTMappingOperation::MatchButNotIdentical, 50, false),
            0
        );
    }

    #[test]
    fn single_node_operations_cost_one_regardless_of_subtree_size() {
        assert_eq!(
            operation_cost(&ASTMappingOperation::Update, 50, false),
            COST_UPDATE
        );
        assert_eq!(
            operation_cost(&ASTMappingOperation::Delete, 50, false),
            COST_DELETE
        );
        assert_eq!(
            operation_cost(&ASTMappingOperation::Insert, 50, false),
            COST_INSERT
        );
    }

    #[test]
    fn with_children_operations_scale_by_subtree_size() {
        assert_eq!(
            operation_cost(&ASTMappingOperation::DeleteWithChildren, 7, false),
            7 * COST_DELETE
        );
        assert_eq!(
            operation_cost(&ASTMappingOperation::InsertWithChildren, 3, false),
            3 * COST_INSERT
        );
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

    /// A matched pair whose *own* text differs must not be free. `MatchButNotIdentical` is priced
    /// at 0 because a matched internal node's differences show up as separate entries for its
    /// differing descendants - which is exactly wrong for a node owning text in the gaps its
    /// children don't cover, since no such descendant entry exists. Concretely: this is why
    /// `yaml-draios-sysdig-string-url-change` reported a total cost of 0 for a file in which six
    /// URLs changed.
    #[test]
    fn match_but_not_identical_charges_for_a_node_s_own_changed_text() {
        assert_eq!(
            operation_cost(&ASTMappingOperation::MatchButNotIdentical, 1, true),
            COST_UPDATE
        );
        assert_eq!(
            operation_cost(&ASTMappingOperation::MatchButNotIdentical, 1, false),
            0
        );
        // `Identical` cannot have differing owned text (the subtrees are byte-identical), and the
        // flag must not leak into operations that already price their own difference.
        assert_eq!(operation_cost(&ASTMappingOperation::Identical, 1, true), 0);
        assert_eq!(
            operation_cost(&ASTMappingOperation::Update, 1, true),
            COST_UPDATE
        );
    }

    /// `diff_cost` must derive that flag from the nodes, not be told it - the regression guard for
    /// the whole-pipeline path, where a gap-owning pair used to contribute nothing at all.
    #[test]
    fn diff_cost_charges_a_gap_owning_matched_pair() {
        let node = |owned_text_hash: u64| crate::code::ASTNodeMetadata {
            owned_text_hash,
            ..crate::code::ASTNodeMetadata::new("AttValue".to_string(), String::new(), vec![], 0, 0)
        };
        let mut before_meta = ASTMetadata::default();
        let mut after_meta = ASTMetadata::default();
        before_meta.node_info.insert(1, node(0xABC));
        after_meta.node_info.insert(2, node(0xDEF));

        let mut diff = ASTDiff::default();
        diff.add_mapping(1, 2, mapping(ASTMappingOperation::MatchButNotIdentical));
        assert_eq!(diff_cost(&diff, &before_meta, &after_meta), COST_UPDATE);

        // Same pair, same owned text: back to free.
        after_meta.node_info.insert(2, node(0xABC));
        assert_eq!(diff_cost(&diff, &before_meta, &after_meta), 0);
    }
}
