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

//! Total edit cost of a finished mapping, root to every leaf (not APTED's internal DP). `diff_cost`
//! and `human_mapping::human_mapping_cost` both sum `operation_cost`, so codediff's cost and the
//! human's are comparable.
use crate::code::ASTMetadata;
use crate::diff::{ASTDiff, ASTMappingOperation, COST_DELETE, COST_INSERT, COST_UPDATE};

/// Unit cost of one mapping entry, mirroring `apted::common::UnitCostModel`. `subtree_size` is
/// read only by the `*WithChildren` operations, which stand in for a whole subtree (human mappings
/// use them; the pipeline does not).
///
/// `MatchButNotIdentical` is free, since its descendants' differences carry their own entries,
/// unless `owned_text_changed`: text a node owns in the gaps between its children has no
/// descendant entry, so it costs `COST_UPDATE` like the equivalent leaf change.
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

/// Total cost of a finished `ASTDiff`: `operation_cost` summed over its entries, one per node.
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
            // Id 0 on a delete/insert's missing side finds nothing.
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

    /// Text a node owns between its children has no descendant entry to carry its cost
    /// (`yaml-draios-sysdig-string-url-change`).
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

    /// `diff_cost` derives the flag from the nodes rather than being told it.
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
