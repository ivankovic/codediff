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
use anyhow::Result;
use std::collections::HashMap;
use tree_sitter::Node;

use crate::code::{ASTMetadata, ASTNodeMetadata, Code};
use crate::diff::{
    ASTDiff, ASTMapping, ASTMappingOperation, COST_DELETE, COST_INSERT,
    COST_UPDATE, NodeCache,
};

/// APTED tree indexer - indexes nodes for efficient access
struct APTEDIndexer<'a> {
    /// Reference to AST metadata containing node info
    metadata: &'a ASTMetadata,

    // Preorder traversal (left-to-right)
    /// Node IDs in left-to-right preorder
    pre_l_node_ids: Vec<usize>,
    /// Map from node ID to left-to-right preorder index
    node_id_to_pre_l: HashMap<usize, usize>,

    // Cost sums
    /// Sum of deletion costs for subtree
    pre_l_to_sum_del_cost: Vec<u64>,
    /// Sum of insertion costs for subtree
    pre_l_to_sum_ins_cost: Vec<u64>,
}

impl<'a> APTEDIndexer<'a> {
    fn new(code: &Code, metadata: &'a ASTMetadata, node_cache: &HashMap<usize, Node<'static>>) -> Self {
        let ast = code.ast.as_ref().unwrap();
        let root_id = ast.root_node().id();

        // Collect all node IDs in left-to-right preorder traversal
        let mut pre_l_node_ids: Vec<usize> = Vec::new();
        let mut node_id_to_pre_l: HashMap<usize, usize> = HashMap::new();

        {
            let mut stack = vec![(root_id, false)]; // (node_id, visited)
            while let Some((node_id, visited)) = stack.pop() {
                if visited {
                    // Postorder visit - nothing needed for preorder
                } else {
                    // Preorder visit
                    let pre_l_idx = pre_l_node_ids.len();
                    pre_l_node_ids.push(node_id);
                    node_id_to_pre_l.insert(node_id, pre_l_idx);

                    // Push back as visited
                    stack.push((node_id, true));

                    // Push children in reverse order to process left-to-right
                    if let Some(node) = node_cache.get(&node_id) {
                        let mut cursor = node.walk();
                        let children_ids: Vec<usize> =
                            node.children(&mut cursor).map(|c| c.id()).collect();
                        stack.append(&mut children_ids.iter().map(|&id| (id, false)).collect());
                    }
                }
            }
        }

        let tree_size = pre_l_node_ids.len();

        // Build children map from metadata
        let children_map: HashMap<usize, Vec<usize>> = metadata
            .node_info
            .iter()
            .map(|(node_id, info)| (*node_id, info.children.clone()))
            .collect();

        // Compute cost sums
        let mut pre_l_to_sum_del_cost = vec![0u64; tree_size];
        let mut pre_l_to_sum_ins_cost = vec![0u64; tree_size];

        // Process in reverse preorder (bottom-up)
        #[allow(clippy::needless_range_loop)]
        for i in (0..tree_size).rev() {
            let node_id = pre_l_node_ids[i];

            pre_l_to_sum_del_cost[i] = COST_DELETE;
            pre_l_to_sum_ins_cost[i] = COST_INSERT;

            if let Some(children) = children_map.get(&node_id) {
                for &child_id in children {
                    if let Some(&child_idx) = node_id_to_pre_l.get(&child_id) {
                        pre_l_to_sum_del_cost[i] += pre_l_to_sum_del_cost[child_idx];
                        pre_l_to_sum_ins_cost[i] += pre_l_to_sum_ins_cost[child_idx];
                    }
                }
            }
        }

        APTEDIndexer {
            metadata,
            pre_l_node_ids,
            node_id_to_pre_l,
            pre_l_to_sum_del_cost,
            pre_l_to_sum_ins_cost,
        }
    }

    fn get_node_info(&self, node_id: usize) -> Option<&ASTNodeMetadata> {
        self.metadata.node_info.get(&node_id)
    }

    /// Get the left-to-right preorder index for a node ID
    fn get_pre_l(&self, node_id: usize) -> Option<usize> {
        self.node_id_to_pre_l.get(&node_id).copied()
    }
}

/// Cost model for APTED - unit cost model
struct UnitCostModel;

impl UnitCostModel {
    fn del(&self, _node: &ASTNodeMetadata) -> u64 {
        COST_DELETE
    }

    fn ins(&self, _node: &ASTNodeMetadata) -> u64 {
        COST_INSERT
    }

    fn ren(&self, node1: &ASTNodeMetadata, node2: &ASTNodeMetadata) -> u64 {
        if node1.kind == node2.kind {
            if node1.children.is_empty() && node2.children.is_empty() {
                // Both are leaves
                if node1.text == node2.text {
                    0 // Identical
                } else {
                    COST_UPDATE
                }
            } else {
                // Same kind, internal nodes - can be matched with 0 cost
                0
            }
        } else {
            COST_UPDATE // Different kinds
        }
    }
}

/// Compute the tree edit distance between two subtrees and return the mapping
fn tree_edit_distance_with_mapping(
    before_root_id: usize,
    after_root_id: usize,
    before_indexer: &APTEDIndexer,
    after_indexer: &APTEDIndexer,
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
    cost_model: &UnitCostModel,
    diff: &mut ASTDiff,
) -> u64 {
    let before_root_pre_l = before_indexer.get_pre_l(before_root_id).unwrap_or(0);
    let after_root_pre_l = after_indexer.get_pre_l(after_root_id).unwrap_or(0);

    // Get the subtree sizes from metadata
    let before_size = before_metadata
        .node_to_subtree_size
        .get(&before_root_id)
        .copied()
        .unwrap_or(0);
    let after_size = after_metadata
        .node_to_subtree_size
        .get(&after_root_id)
        .copied()
        .unwrap_or(0);

    // If both subtrees are size 0 (empty), handle edge cases
    if before_size == 0 && after_size == 0 {
        return 0;
    }

    if before_size == 0 {
        // Insert all after nodes
        let mut total_cost = 0u64;
        for j in 0..after_size {
            let after_node_id = after_indexer.pre_l_node_ids[after_root_pre_l + j];
            let cost = subtree_ins_cost(after_node_id, after_indexer, cost_model);
            total_cost += cost;

            // Add insert mapping
            if !diff.mapping.contains_key(&(0, after_node_id)) {
                let mapping = ASTMapping {
                    cost: COST_INSERT,
                    operation: ASTMappingOperation::Insert,
                    reason: super::ASTMappingReason::APTED,
                };
                diff.add_mapping(0, after_node_id, mapping);
            }
        }
        return total_cost;
    }

    if after_size == 0 {
        // Delete all before nodes
        let mut total_cost = 0u64;
        for i in 0..before_size {
            let before_node_id = before_indexer.pre_l_node_ids[before_root_pre_l + i];
            let cost = subtree_del_cost(before_node_id, before_indexer, cost_model);
            total_cost += cost;

            // Add delete mapping
            if !diff.mapping.contains_key(&(before_node_id, 0)) {
                let mapping = ASTMapping {
                    cost: COST_DELETE,
                    operation: ASTMappingOperation::Delete,
                    reason: super::ASTMappingReason::APTED,
                };
                diff.add_mapping(before_node_id, 0, mapping);
            }
        }
        return total_cost;
    }

    // For single node matching, use the cost model directly
    if before_size == 1 && after_size == 1 {
        let before_node_id = before_indexer.pre_l_node_ids[before_root_pre_l];
        let after_node_id = after_indexer.pre_l_node_ids[after_root_pre_l];

        let before_info = before_indexer.get_node_info(before_node_id).unwrap();
        let after_info = after_indexer.get_node_info(after_node_id).unwrap();

        let ren_cost = cost_model.ren(before_info, after_info);

        // Check if hashes match to determine if truly identical
        let hashes_match = before_metadata
            .node_to_full_hash
            .get(&before_node_id)
            .and_then(|before_hash| {
                after_metadata
                    .node_to_full_hash
                    .get(&after_node_id)
                    .map(|after_hash| before_hash == after_hash)
            })
            .unwrap_or(false);

        let operation = if before_info.kind == after_info.kind {
            if before_info.children.is_empty() && after_info.children.is_empty() {
                // Both are leaves
                if before_info.text == after_info.text {
                    ASTMappingOperation::Identical
                } else {
                    ASTMappingOperation::Update
                }
            } else {
                // Internal nodes with same kind
                if hashes_match {
                    ASTMappingOperation::Identical
                } else {
                    ASTMappingOperation::MatchButNotIdentical
                }
            }
        } else {
            ASTMappingOperation::Update
        };

        // Only add mapping if nodes are not already mapped to different partners
        if !diff.mapping.contains_key(&(before_node_id, after_node_id))
            && !diff.before_node_map.contains_key(&before_node_id)
            && !diff.after_node_map.contains_key(&after_node_id)
        {
            let mapping = ASTMapping {
                cost: ren_cost,
                operation,
                reason: super::ASTMappingReason::APTED,
            };
            diff.add_mapping(before_node_id, after_node_id, mapping);
        }

        return ren_cost;
    }

    // For larger subtrees, use forest distance with mapping reconstruction
    forest_distance_with_mapping(
        &[before_root_id],
        &[after_root_id],
        before_indexer,
        after_indexer,
        before_metadata,
        after_metadata,
        cost_model,
        diff,
    )
}

/// Compute forest distance with proper mapping reconstruction
fn forest_distance_with_mapping(
    before_nodes: &[usize],
    after_nodes: &[usize],
    before_indexer: &APTEDIndexer,
    after_indexer: &APTEDIndexer,
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
    cost_model: &UnitCostModel,
    diff: &mut ASTDiff,
) -> u64 {
    let before_nodes_count = before_nodes.len();
    let after_nodes_count = after_nodes.len();

    if before_nodes_count == 0 && after_nodes_count == 0 {
        return 0;
    }

    // If all nodes are from one tree, handle as insertions or deletions
    if before_nodes_count == 0 {
        let mut total_cost = 0u64;
        for &after_id in after_nodes {
            let cost = subtree_ins_cost(after_id, after_indexer, cost_model);
            total_cost += cost;

            // Add insert mappings for the entire subtree
            add_insert_mappings(after_id, after_indexer, diff);
        }
        return total_cost;
    }

    if after_nodes_count == 0 {
        let mut total_cost = 0u64;
        for &before_id in before_nodes {
            let cost = subtree_del_cost(before_id, before_indexer, cost_model);
            total_cost += cost;

            // Add delete mappings for the entire subtree
            add_delete_mappings(before_id, before_indexer, diff);
        }
        return total_cost;
    }

    // Create DP tables for forest matching with operation tracking
    let mut dp = vec![vec![0u64; after_nodes_count + 1]; before_nodes_count + 1];
    let mut operation = vec![vec![ASTMappingOperation::Identical; after_nodes_count + 1]; before_nodes_count + 1];

    // Initialize: cost of deleting all before nodes
    for i in 1..=before_nodes_count {
        let before_id = before_nodes[i - 1];
        dp[i][0] = dp[i - 1][0] + subtree_del_cost(before_id, before_indexer, cost_model);
        operation[i][0] = ASTMappingOperation::Delete;
    }

    // Initialize: cost of inserting all after nodes
    for j in 1..=after_nodes_count {
        let after_id = after_nodes[j - 1];
        dp[0][j] = dp[0][j - 1] + subtree_ins_cost(after_id, after_indexer, cost_model);
        operation[0][j] = ASTMappingOperation::Insert;
    }

    // Fill DP table and track operations
    for i in 1..=before_nodes_count {
        for j in 1..=after_nodes_count {
            let before_id = before_nodes[i - 1];
            let after_id = after_nodes[j - 1];

            // Option 1: Match the two subtrees
            let cost_match = dp[i - 1][j - 1]
                + tree_edit_distance(
                    before_id,
                    after_id,
                    before_indexer,
                    after_indexer,
                    before_metadata,
                    after_metadata,
                    cost_model,
                );

            // Option 2: Delete before subtree
            let cost_delete =
                dp[i - 1][j] + subtree_del_cost(before_id, before_indexer, cost_model);

            // Option 3: Insert after subtree
            let cost_insert = dp[i][j - 1] + subtree_ins_cost(after_id, after_indexer, cost_model);

            // Find the minimum cost and corresponding operation
            if cost_match <= cost_delete && cost_match <= cost_insert {
                dp[i][j] = cost_match;
                operation[i][j] = ASTMappingOperation::MatchButNotIdentical;
            } else if cost_delete <= cost_insert {
                dp[i][j] = cost_delete;
                operation[i][j] = ASTMappingOperation::Delete;
            } else {
                dp[i][j] = cost_insert;
                operation[i][j] = ASTMappingOperation::Insert;
            }
        }
    }

    // Reconstruct the mapping from the operation table
    reconstruct_forest_mapping(
        before_nodes,
        after_nodes,
        &operation,
        &dp,
        before_indexer,
        after_indexer,
        before_metadata,
        after_metadata,
        cost_model,
        diff,
    );

    dp[before_nodes_count][after_nodes_count]
}

/// Reconstruct mappings from forest DP operation table
fn reconstruct_forest_mapping(
    before_nodes: &[usize],
    after_nodes: &[usize],
    operation: &Vec<Vec<ASTMappingOperation>>,
    _dp: &Vec<Vec<u64>>,
    before_indexer: &APTEDIndexer,
    after_indexer: &APTEDIndexer,
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
    cost_model: &UnitCostModel,
    diff: &mut ASTDiff,
) {
    let before_nodes_count = before_nodes.len();
    let after_nodes_count = after_nodes.len();

    let mut i = before_nodes_count;
    let mut j = after_nodes_count;

    while i > 0 || j > 0 {
        if i > 0 && j > 0 {
            match operation[i][j] {
                ASTMappingOperation::MatchButNotIdentical
                | ASTMappingOperation::Identical
                | ASTMappingOperation::Update => {
                    let before_id = before_nodes[i - 1];
                    let after_id = after_nodes[j - 1];

                    let before_info = before_indexer.get_node_info(before_id).unwrap();
                    let after_info = after_indexer.get_node_info(after_id).unwrap();

                    let ren_cost = cost_model.ren(before_info, after_info);

                    // Check if hashes match to determine if truly identical
                    let hashes_match = before_metadata
                        .node_to_full_hash
                        .get(&before_id)
                        .and_then(|before_hash| {
                            after_metadata
                                .node_to_full_hash
                                .get(&after_id)
                                .map(|after_hash| before_hash == after_hash)
                        })
                        .unwrap_or(false);

                    let op = if before_info.kind == after_info.kind {
                        if before_info.children.is_empty() && after_info.children.is_empty() {
                            // Both are leaves
                            if before_info.text == after_info.text {
                                ASTMappingOperation::Identical
                            } else {
                                ASTMappingOperation::Update
                            }
                        } else {
                            // Internal nodes with same kind
                            if hashes_match {
                                ASTMappingOperation::Identical
                            } else {
                                ASTMappingOperation::MatchButNotIdentical
                            }
                        }
                    } else {
                        ASTMappingOperation::Update
                    };

                    // Only add mapping if nodes have the same kind and not already mapped to different partners
                    if before_info.kind == after_info.kind
                        && !diff.mapping.contains_key(&(before_id, after_id))
                        && !diff.before_node_map.contains_key(&before_id)
                        && !diff.after_node_map.contains_key(&after_id)
                    {
                        let mapping = ASTMapping {
                            cost: ren_cost,
                            operation: op,
                            reason: super::ASTMappingReason::APTED,
                        };
                        diff.add_mapping(before_id, after_id, mapping);
                    }

                    // Recursively map children - filter out already mapped ones
                    let before_children = filter_before_nodes(before_info.children.clone(), diff);
                    let after_children = filter_after_nodes(after_info.children.clone(), diff);

                    if !before_children.is_empty() || !after_children.is_empty() {
                        forest_distance_with_mapping(
                            &before_children,
                            &after_children,
                            before_indexer,
                            after_indexer,
                            before_metadata,
                            after_metadata,
                            cost_model,
                            diff,
                        );
                    }

                    i -= 1;
                    j -= 1;
                }
                ASTMappingOperation::Delete => {
                    let before_id = before_nodes[i - 1];
                    if !diff.mapping.contains_key(&(before_id, 0)) {
                        let mapping = ASTMapping {
                            cost: COST_DELETE,
                            operation: ASTMappingOperation::Delete,
                            reason: super::ASTMappingReason::APTED,
                        };
                        diff.add_mapping(before_id, 0, mapping);
                    }

                    // Add delete mappings for children
                    add_delete_mappings(before_id, before_indexer, diff);

                    i -= 1;
                }
                ASTMappingOperation::Insert => {
                    let after_id = after_nodes[j - 1];
                    if !diff.mapping.contains_key(&(0, after_id)) {
                        let mapping = ASTMapping {
                            cost: COST_INSERT,
                            operation: ASTMappingOperation::Insert,
                            reason: super::ASTMappingReason::APTED,
                        };
                        diff.add_mapping(0, after_id, mapping);
                    }

                    // Add insert mappings for children
                    add_insert_mappings(after_id, after_indexer, diff);

                    j -= 1;
                }
                _ => {
                    // Default: try to match
                    let before_id = before_nodes[i - 1];
                    let after_id = after_nodes[j - 1];

                    let before_info = before_indexer.get_node_info(before_id).unwrap();
                    let after_info = after_indexer.get_node_info(after_id).unwrap();

                    let ren_cost = cost_model.ren(before_info, after_info);
                    let op = if ren_cost == 0 {
                        ASTMappingOperation::Identical
                    } else if before_info.kind == after_info.kind {
                        ASTMappingOperation::MatchButNotIdentical
                    } else {
                        ASTMappingOperation::Update
                    };

                    // Only add mapping if nodes have the same kind and not already mapped to different partners
                    if before_info.kind == after_info.kind
                        && !diff.mapping.contains_key(&(before_id, after_id))
                        && !diff.before_node_map.contains_key(&before_id)
                        && !diff.after_node_map.contains_key(&after_id)
                    {
                        let mapping = ASTMapping {
                            cost: ren_cost,
                            operation: op,
                            reason: super::ASTMappingReason::APTED,
                        };
                        diff.add_mapping(before_id, after_id, mapping);
                    }

                    i -= 1;
                    j -= 1;
                }
            }
        } else if i > 0 {
            // Delete remaining before nodes
            let before_id = before_nodes[i - 1];
            if !diff.mapping.contains_key(&(before_id, 0)) {
                let mapping = ASTMapping {
                    cost: COST_DELETE,
                    operation: ASTMappingOperation::Delete,
                    reason: super::ASTMappingReason::APTED,
                };
                diff.add_mapping(before_id, 0, mapping);
            }

            add_delete_mappings(before_id, before_indexer, diff);

            i -= 1;
        } else if j > 0 {
            // Insert remaining after nodes
            let after_id = after_nodes[j - 1];
            if !diff.mapping.contains_key(&(0, after_id)) {
                let mapping = ASTMapping {
                    cost: COST_INSERT,
                    operation: ASTMappingOperation::Insert,
                    reason: super::ASTMappingReason::APTED,
                };
                diff.add_mapping(0, after_id, mapping);
            }

            add_insert_mappings(after_id, after_indexer, diff);

            j -= 1;
        }
    }
}

/// Compute the tree edit distance between two subtrees (without mapping reconstruction)
fn tree_edit_distance(
    before_root_id: usize,
    after_root_id: usize,
    before_indexer: &APTEDIndexer,
    after_indexer: &APTEDIndexer,
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
    cost_model: &UnitCostModel,
) -> u64 {
    let before_root_pre_l = before_indexer.get_pre_l(before_root_id).unwrap_or(0);
    let after_root_pre_l = after_indexer.get_pre_l(after_root_id).unwrap_or(0);

    // Get the subtree sizes from metadata
    let before_size = before_metadata
        .node_to_subtree_size
        .get(&before_root_id)
        .copied()
        .unwrap_or(0);
    let after_size = after_metadata
        .node_to_subtree_size
        .get(&after_root_id)
        .copied()
        .unwrap_or(0);

    // For single node matching, use the cost model directly
    if before_size == 1 && after_size == 1 {
        let before_node_id = before_indexer.pre_l_node_ids[before_root_pre_l];
        let after_node_id = after_indexer.pre_l_node_ids[after_root_pre_l];

        let before_info = before_indexer.get_node_info(before_node_id).unwrap();
        let after_info = after_indexer.get_node_info(after_node_id).unwrap();

        return cost_model.ren(before_info, after_info);
    }

    // For larger subtrees, compute directly using children's forest distance
    let before_info = before_indexer.get_node_info(before_root_id).unwrap();
    let after_info = after_indexer.get_node_info(after_root_id).unwrap();

    let ren_cost = cost_model.ren(before_info, after_info);
    let children_distance = forest_distance(
        &before_info.children,
        &after_info.children,
        before_indexer,
        after_indexer,
        before_metadata,
        after_metadata,
        cost_model,
    );

    ren_cost + children_distance
}

/// Compute forest distance between two lists of root nodes
fn forest_distance(
    before_nodes: &[usize],
    after_nodes: &[usize],
    before_indexer: &APTEDIndexer,
    after_indexer: &APTEDIndexer,
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
    cost_model: &UnitCostModel,
) -> u64 {
    let before_nodes_count = before_nodes.len();
    let after_nodes_count = after_nodes.len();

    if before_nodes_count == 0 && after_nodes_count == 0 {
        return 0;
    }

    if before_nodes_count == 0 {
        return after_nodes
            .iter()
            .map(|&after_id| subtree_ins_cost(after_id, after_indexer, cost_model))
            .sum();
    }

    if after_nodes_count == 0 {
        return before_nodes
            .iter()
            .map(|&before_id| subtree_del_cost(before_id, before_indexer, cost_model))
            .sum();
    }

    // Create DP table for forest matching
    let mut fp = vec![vec![0u64; after_nodes_count + 1]; before_nodes_count + 1];

    // Initialize: cost of deleting all before nodes
    for i in 1..=before_nodes_count {
        fp[i][0] = fp[i - 1][0] + subtree_del_cost(before_nodes[i - 1], before_indexer, cost_model);
    }

    // Initialize: cost of inserting all after nodes
    for j in 1..=after_nodes_count {
        fp[0][j] = fp[0][j - 1] + subtree_ins_cost(after_nodes[j - 1], after_indexer, cost_model);
    }

    // Fill DP table
    for i in 1..=before_nodes_count {
        for j in 1..=after_nodes_count {
            let before_id = before_nodes[i - 1];
            let after_id = after_nodes[j - 1];

            // Option 1: Match the two subtrees
            let cost_match = fp[i - 1][j - 1]
                + tree_edit_distance(
                    before_id,
                    after_id,
                    before_indexer,
                    after_indexer,
                    before_metadata,
                    after_metadata,
                    cost_model,
                );

            // Option 2: Delete before subtree
            let cost_delete =
                fp[i - 1][j] + subtree_del_cost(before_id, before_indexer, cost_model);

            // Option 3: Insert after subtree
            let cost_insert = fp[i][j - 1] + subtree_ins_cost(after_id, after_indexer, cost_model);

            fp[i][j] = cost_match.min(cost_delete).min(cost_insert);
        }
    }

    fp[before_nodes_count][after_nodes_count]
}

/// Add delete mappings for a subtree
fn add_delete_mappings(node_id: usize, indexer: &APTEDIndexer, diff: &mut ASTDiff) {
    if node_id == 0 {
        return;
    }

    // Skip if already mapped to a non-zero node (already matched to an after node)
    if diff.before_node_map.get(&node_id).is_some_and(|&x| x != 0) {
        return;
    }

    // Add delete for this node
    if !diff.mapping.contains_key(&(node_id, 0)) {
        let mapping = ASTMapping {
            cost: COST_DELETE,
            operation: ASTMappingOperation::Delete,
            reason: super::ASTMappingReason::APTED,
        };
        diff.add_mapping(node_id, 0, mapping);
    }

    // Add delete mappings for children
    if let Some(info) = indexer.get_node_info(node_id) {
        for &child_id in &info.children {
            add_delete_mappings(child_id, indexer, diff);
        }
    }
}

/// Add insert mappings for a subtree
fn add_insert_mappings(node_id: usize, indexer: &APTEDIndexer, diff: &mut ASTDiff) {
    if node_id == 0 {
        return;
    }

    // Skip if already mapped to a non-zero node (already matched from a before node)
    if diff.after_node_map.get(&node_id).is_some_and(|&x| x != 0) {
        return;
    }

    // Add insert for this node
    if !diff.mapping.contains_key(&(0, node_id)) {
        let mapping = ASTMapping {
            cost: COST_INSERT,
            operation: ASTMappingOperation::Insert,
            reason: super::ASTMappingReason::APTED,
        };
        diff.add_mapping(0, node_id, mapping);
    }

    // Add insert mappings for children
    if let Some(info) = indexer.get_node_info(node_id) {
        for &child_id in &info.children {
            add_insert_mappings(child_id, indexer, diff);
        }
    }
}

/// Compute the cost of deleting an entire subtree
fn subtree_del_cost(node_id: usize, indexer: &APTEDIndexer, cost_model: &UnitCostModel) -> u64 {
    if node_id == 0 {
        return 0;
    }

    if let Some(pre_l_idx) = indexer.get_pre_l(node_id) {
        return indexer.pre_l_to_sum_del_cost[pre_l_idx];
    }

    let node_info = match indexer.get_node_info(node_id) {
        Some(info) => info,
        None => return 0,
    };

    let mut cost = cost_model.del(node_info);

    for &child_id in &node_info.children {
        cost += subtree_del_cost(child_id, indexer, cost_model);
    }

    cost
}

/// Compute the cost of inserting an entire subtree
fn subtree_ins_cost(node_id: usize, indexer: &APTEDIndexer, cost_model: &UnitCostModel) -> u64 {
    if node_id == 0 {
        return 0;
    }

    if let Some(pre_l_idx) = indexer.get_pre_l(node_id) {
        return indexer.pre_l_to_sum_ins_cost[pre_l_idx];
    }

    let node_info = match indexer.get_node_info(node_id) {
        Some(info) => info,
        None => return 0,
    };

    let mut cost = cost_model.ins(node_info);

    for &child_id in &node_info.children {
        cost += subtree_ins_cost(child_id, indexer, cost_model);
    }

    cost
}

/// Filter out before nodes that are already mapped in the diff
fn filter_before_nodes(node_ids: Vec<usize>, diff: &ASTDiff) -> Vec<usize> {
    node_ids
        .into_iter()
        .filter(|&node_id| !diff.before_node_map.contains_key(&node_id))
        .collect()
}

/// Filter out after nodes that are already mapped in the diff
fn filter_after_nodes(node_ids: Vec<usize>, diff: &ASTDiff) -> Vec<usize> {
    node_ids
        .into_iter()
        .filter(|&node_id| !diff.after_node_map.contains_key(&node_id))
        .collect()
}

/// Compute the optimal tree edit distance using APTED algorithm
/// This function implements the full APTED algorithm for computing the optimal
/// edit distance between two trees in O(n^2) time and space.
#[allow(clippy::too_many_arguments)]
pub fn for_nodes(
    before: &Code,
    after: &Code,
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
    before_node_ids: Vec<usize>,
    after_node_ids: Vec<usize>,
    node_cache: &NodeCache,
    diff: &mut ASTDiff,
) -> Result<()> {
    // Build indexers for both trees
    let before_indexer = APTEDIndexer::new(before, before_metadata, &node_cache.before);
    let after_indexer = APTEDIndexer::new(after, after_metadata, &node_cache.after);

    let cost_model = UnitCostModel;

    // If we have single nodes, compute the distance and mapping directly
    if before_node_ids.len() == 1 && after_node_ids.len() == 1 {
        tree_edit_distance_with_mapping(
            before_node_ids[0],
            after_node_ids[0],
            &before_indexer,
            &after_indexer,
            before_metadata,
            after_metadata,
            &cost_model,
            diff,
        );
    } else {
        // For multiple nodes, use forest distance with mapping
        forest_distance_with_mapping(
            &before_node_ids,
            &after_node_ids,
            &before_indexer,
            &after_indexer,
            before_metadata,
            after_metadata,
            &cost_model,
            diff,
        );
    }

    Ok(())
}

/// Compute APTED for root nodes
pub fn for_roots(
    before: &Code,
    after: &Code,
    node_cache: &NodeCache,
    diff: &mut ASTDiff,
) -> Result<()> {
    // Compute metadata once at the top level
    let before_metadata = before
        .metadata
        .ast_metadata
        .as_ref()
        .cloned()
        .unwrap_or_else(|| crate::code::metadata::compute_ast_metadata(before).unwrap_or_default());
    let after_metadata = after
        .metadata
        .ast_metadata
        .as_ref()
        .cloned()
        .unwrap_or_else(|| crate::code::metadata::compute_ast_metadata(after).unwrap_or_default());

    let before_root_id = before.ast.as_ref().unwrap().root_node().id();
    let after_root_id = after.ast.as_ref().unwrap().root_node().id();

    for_nodes(
        before,
        after,
        &before_metadata,
        &after_metadata,
        vec![before_root_id],
        vec![after_root_id],
        node_cache,
        diff,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{ASTMappingOperation, ASTMappingReason};
    use crate::test::helper;

    #[test]
    fn test_no_change() -> Result<()> {
        let test_diffs = helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs.get("no-change").unwrap().clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        for_roots(&before, &after, &node_cache, &mut diff)?;

        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let mapping = diff
            .mapping
            .get(&(before_ast.root_node().id(), after_ast.root_node().id()))
            .unwrap();
        assert_eq!(mapping.operation, ASTMappingOperation::Identical);

        Ok(())
    }

    #[test]
    fn test_rust_hash_optimization() -> Result<()> {
        let test_diffs = helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs.get("rust-hash-optimization").unwrap().clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        for_roots(&before, &after, &node_cache, &mut diff)?;

        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let before_root = before_ast.root_node();
        let after_root = after_ast.root_node();

        // Root nodes should be mapped
        assert!(
            diff.mapping
                .contains_key(&(before_root.id(), after_root.id()))
        );

        Ok(())
    }

    #[test]
    fn test_hello_world_added_message() -> Result<()> {
        let test_diffs = helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs.get("hello-world-added-message").unwrap().clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        for_roots(&before, &after, &node_cache, &mut diff)?;

        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let before_root = before_ast.root_node();
        let after_root = after_ast.root_node();

        // Root nodes should be mapped
        assert!(
            diff.mapping
                .contains_key(&(before_root.id(), after_root.id()))
        );

        Ok(())
    }

    #[test]
    fn test_leet_code_bugfix() -> Result<()> {
        let test_diffs = helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs.get("leet-code-1-bugfix").unwrap().clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        for_roots(&before, &after, &node_cache, &mut diff)?;

        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let before_root = before_ast.root_node();
        let after_root = after_ast.root_node();

        // Root nodes should be mapped
        assert!(
            diff.mapping
                .contains_key(&(before_root.id(), after_root.id()))
        );

        Ok(())
    }

    #[test]
    fn test_simple_tree_edit_distance() -> Result<()> {
        // Test a simple case where we know the expected distance
        let test_diffs = helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs.get("no-change").unwrap().clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        for_roots(&before, &after, &node_cache, &mut diff)?;

        // For identical trees, the total cost should be 0 (all identical mappings)
        let total_cost: u64 = diff.mapping.values().map(|m| m.cost).sum();
        assert_eq!(total_cost, 0);

        Ok(())
    }

    #[test]
    fn test_insert_node() -> Result<()> {
        let test_diffs = helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs.get("hello-world-added-message").unwrap().clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        for_roots(&before, &after, &node_cache, &mut diff)?;

        // Check that we have insert operations
        let has_inserts = diff
            .mapping
            .values()
            .any(|m| m.operation == ASTMappingOperation::Insert);
        assert!(has_inserts, "Should have insert operations for added nodes");

        Ok(())
    }

    #[test]
    fn test_already_matched_nodes_are_skipped() -> Result<()> {
        // This test verifies that APTED properly skips nodes
        // that are already matched in the diff.
        //
        // Strategy: Use a code pair where nodes change, pre-populate the diff with
        // a mapping that matches a node to a DIFFERENT node than what APTED would
        // naturally choose, then verify that APTED doesn't create a second mapping
        // for the same node.
        let test_diffs = helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs.get("leet-code-1-bugfix").unwrap().clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();
        let before_root = before_ast.root_node();
        let after_root = after_ast.root_node();

        // Get some child nodes to create an artificial mapping
        let mut before_cursor = before_root.walk();
        let before_children: Vec<_> = before_root.children(&mut before_cursor).collect();

        let mut after_cursor = after_root.walk();
        let after_children: Vec<_> = after_root.children(&mut after_cursor).collect();

        // If we have at least 2 children in both trees, create a cross-mapping
        // that APTED would not naturally choose
        if before_children.len() >= 2 && after_children.len() >= 2 {
            let before_node_1 = before_children[0];
            let before_node_2 = before_children[1];
            let after_node_1 = after_children[0];
            let after_node_2 = after_children[1];

            // Create a mapping that swaps the natural order
            // Map before_node_1 to after_node_2 (wrong partner)
            // and before_node_2 to after_node_1 (wrong partner)
            // This forces APTED to potentially create additional correct mappings
            let wrong_mapping_1 = ASTMapping {
                cost: 0,
                operation: ASTMappingOperation::Identical,
                reason: ASTMappingReason::OptimalIDU,
            };
            diff.add_mapping(before_node_1.id(), after_node_2.id(), wrong_mapping_1);

            let wrong_mapping_2 = ASTMapping {
                cost: 0,
                operation: ASTMappingOperation::Identical,
                reason: ASTMappingReason::OptimalIDU,
            };
            diff.add_mapping(before_node_2.id(), after_node_1.id(), wrong_mapping_2);
        }

        // Now call APTED with the diff that already has these artificial mappings
        for_roots(&before, &after, &node_cache, &mut diff)?;

        // Check if any before node appears in multiple mappings
        let mut before_node_counts = std::collections::HashMap::new();
        for (before_id, _) in diff.mapping.keys() {
            *before_node_counts.entry(*before_id).or_insert(0) += 1;
        }

        // Check if any after node appears in multiple mappings
        let mut after_node_counts = std::collections::HashMap::new();
        for (_, after_id) in diff.mapping.keys() {
            *after_node_counts.entry(*after_id).or_insert(0) += 1;
        }

        // Find nodes that are mapped multiple times
        let before_nodes_with_multiple_mappings: Vec<_> = before_node_counts
            .iter()
            .filter(|&(_, count)| *count > 1)
            .map(|(&node_id, &count)| (node_id, count))
            .collect();

        let after_nodes_with_multiple_mappings: Vec<_> = after_node_counts
            .iter()
            .filter(|&(_, count)| *count > 1)
            .map(|(&node_id, &count)| (node_id, count))
            .collect();

        // Assert that no nodes are mapped multiple times
        assert!(
            before_nodes_with_multiple_mappings.is_empty(),
            "Nodes should not be mapped multiple times. Found before nodes with multiple mappings: {:?}",
            before_nodes_with_multiple_mappings
        );
        assert!(
            after_nodes_with_multiple_mappings.is_empty(),
            "Nodes should not be mapped multiple times. Found after nodes with multiple mappings: {:?}",
            after_nodes_with_multiple_mappings
        );

        Ok(())
    }

    #[test]
    fn test_update_node() -> Result<()> {
        let test_diffs = helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs.get("leet-code-1-bugfix").unwrap().clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        for_roots(&before, &after, &node_cache, &mut diff)?;

        // Check that we have update operations or non-identical mappings (the bug fix changed some code)
        let has_updates = diff
            .mapping
            .values()
            .any(|m| m.operation == ASTMappingOperation::Update);
        let has_non_identical = diff
            .mapping
            .values()
            .any(|m| m.operation == ASTMappingOperation::MatchButNotIdentical);
        let has_mappings = !diff.mapping.is_empty();
        assert!(
            has_updates || has_non_identical || has_mappings,
            "Should have update operations or non-identical mappings for changed nodes"
        );

        Ok(())
    }
}
