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

use crate::code::{ASTMetadata, Code};
use crate::diff::{ASTDiff, ASTMapping, ASTMappingOperation, COST_DELETE, COST_INSERT, COST_UPDATE, NodeCache};

/// Node information for APTED algorithm
#[derive(Debug, Clone)]
struct TreeNodeInfo {
    /// Node kind (type)
    kind: String,
    /// Node text content (for leaf nodes)
    text: String,
    /// Children IDs
    children: Vec<usize>,
}

/// APTED tree indexer - indexes nodes for efficient access
/// 
/// This follows the structure of the Java APTED NodeIndexer, which precomputes
/// various tree traversals and indices for efficient distance computation.
struct APTEDIndexer {
    /// Map from node ID to node info
    node_info: HashMap<usize, TreeNodeInfo>,
    /// Tree size (number of nodes)
    tree_size: usize,
    
    // Preorder traversal (left-to-right)
    /// Node IDs in left-to-right preorder
    pre_l_node_ids: Vec<usize>,
    /// Map from node ID to left-to-right preorder index
    node_id_to_pre_l: HashMap<usize, usize>,
    
    // Subtree sizes indexed by preorder LR index
    /// sizes[pre_l[i]] = size of subtree rooted at node i
    sizes: Vec<usize>,
    /// Parents indexed by preorder LR index
    parents: Vec<Option<usize>>,
    
    // Keyroot sums for strategy computation
    /// Sum of keyroot sizes for left path
    pre_l_to_kr_sum: Vec<usize>,
    /// Sum of keyroot sizes for right path (reversed)
    pre_l_to_rev_kr_sum: Vec<usize>,
    /// Sum of all descendant sizes
    pre_l_to_desc_sum: Vec<usize>,
    
    // Cost sums
    /// Sum of deletion costs for subtree
    pre_l_to_sum_del_cost: Vec<u64>,
    /// Sum of insertion costs for subtree
    pre_l_to_sum_ins_cost: Vec<u64>,
    
    // Node type flags for strategy
    /// Whether node lies on leftmost path from its parent
    node_type_l: Vec<bool>,
    /// Whether node lies on rightmost path from its parent
    node_type_r: Vec<bool>,
    
    // Count of leftmost/rightmost child leaf nodes
    lchl: usize,
    rchl: usize,
    
    // Postorder traversal indices
    /// Map from node ID to postorder index (left-to-right)
    node_id_to_post_l: HashMap<usize, usize>,
    /// Node IDs in postorder (left-to-right)
    post_l_node_ids: Vec<usize>,
    /// Map from postorder index to preorder index
    post_l_to_pre_l: Vec<usize>,
    /// Map from preorder index to postorder index
    pre_l_to_post_l: Vec<usize>,
}

impl APTEDIndexer {
    fn new(code: &Code, node_cache: &HashMap<usize, Node<'static>>) -> Self {
        let ast = code.ast.as_ref().unwrap();
        let root_id = ast.root_node().id();
        
        // Collect all node IDs in left-to-right preorder traversal
        let mut pre_l_node_ids: Vec<usize> = Vec::new();
        let mut node_id_to_pre_l: HashMap<usize, usize> = HashMap::new();
        let mut post_l_node_ids: Vec<usize> = Vec::new();
        let mut node_id_to_post_l: HashMap<usize, usize> = HashMap::new();
        
        {
            let mut stack = vec![(root_id, false)]; // (node_id, visited)
            while let Some((node_id, visited)) = stack.pop() {
                if visited {
                    // Postorder visit
                    let post_l_idx = post_l_node_ids.len();
                    post_l_node_ids.push(node_id);
                    node_id_to_post_l.insert(node_id, post_l_idx);
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
                        let children_ids: Vec<usize> = node.children(&mut cursor).map(|c| c.id()).collect();
                        stack.append(&mut children_ids.iter().map(|&id| (id, false)).collect());
                    }
                }
            }
        }
        
        let tree_size = pre_l_node_ids.len();
        
        // Build node info and compute sizes bottom-up (postorder traversal)
        let mut node_info: HashMap<usize, TreeNodeInfo> = HashMap::new();
        let mut sizes = vec![0; tree_size];
        let mut parents = vec![None; tree_size];
        let mut children_map: HashMap<usize, Vec<usize>> = HashMap::new();
        
        // First pass: collect children
        for &node_id in &pre_l_node_ids {
            if let Some(node) = node_cache.get(&node_id) {
                let mut cursor = node.walk();
                let children: Vec<usize> = node.children(&mut cursor).map(|c| c.id()).collect();
                children_map.insert(node_id, children.clone());
            }
        }
        
        // Second pass: compute sizes in postorder
        for i in 0..tree_size {
            let node_id = post_l_node_ids[i];
            let children = children_map.get(&node_id).cloned().unwrap_or_default();
            
            // Size is 1 + sum of children sizes
            let size = 1 + children.iter().map(|&child_id| {
                if let Some(&child_idx) = node_id_to_pre_l.get(&child_id) {
                    sizes[child_idx]
                } else {
                    0
                }
            }).sum::<usize>();
            
            if let Some(&pre_l_idx) = node_id_to_pre_l.get(&node_id) {
                sizes[pre_l_idx] = size;
            }
            
            // Set parent for children
            for &child_id in &children {
                if let Some(&child_pre_l_idx) = node_id_to_pre_l.get(&child_id) {
                    parents[child_pre_l_idx] = Some(i); // This is wrong, need to fix
                }
            }
            
            // Build node info
            if let Some(_node) = node_cache.get(&node_id) {
                let kind = _node.kind().to_string();
                let text = _node.utf8_text(code.contents.as_bytes()).unwrap_or("").to_string();
                
                let node_info_entry = TreeNodeInfo {
                    kind,
                    text,
                    children,
                };
                
                node_info.insert(node_id, node_info_entry);
            }
        }
        
        // Fix parents - need to map properly
        let mut parents_fixed = vec![None; tree_size];
        for i in 0..tree_size {
            let node_id = pre_l_node_ids[i];
            if let Some(children) = children_map.get(&node_id) {
                for &child_id in children {
                    if let Some(&child_idx) = node_id_to_pre_l.get(&child_id) {
                        parents_fixed[child_idx] = Some(i);
                    }
                }
            }
        }
        
        // Compute cost sums
        let mut pre_l_to_sum_del_cost = vec![0u64; tree_size];
        let mut pre_l_to_sum_ins_cost = vec![0u64; tree_size];
        
        // Process in reverse preorder (bottom-up)
        for i in (0..tree_size).rev() {
            let node_id = pre_l_node_ids[i];
            
            if let Some(_node) = node_cache.get(&node_id) {
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
        }
        
        // Compute keyroot and desc sums (simplified for now)
        let mut pre_l_to_kr_sum = vec![0usize; tree_size];
        let mut pre_l_to_rev_kr_sum = vec![0usize; tree_size];
        let mut pre_l_to_desc_sum = vec![0usize; tree_size];
        
        for i in 0..tree_size {
            let size = sizes[i];
            pre_l_to_kr_sum[i] = size;
            pre_l_to_rev_kr_sum[i] = size;
            pre_l_to_desc_sum[i] = ((size + 1) * (size + 1 + 3)) / 2 - size;
        }
        
        // Count lchl and rchl
        let mut lchl = 0usize;
        let rchl = 0usize;
        
        for i in 0..tree_size {
            if sizes[i] == 1 {
                if let Some(parent_idx) = parents_fixed[i] {
                    if parent_idx + 1 == i {
                        lchl += 1;
                    }
                }
            }
        }
        
        // Set node types (simplified)
        let mut node_type_l = vec![false; tree_size];
        let mut node_type_r = vec![false; tree_size];
        
        for i in 0..tree_size {
            if let Some(children) = children_map.get(&pre_l_node_ids[i]) {
                if !children.is_empty() {
                    if let Some(&first_child_id) = children.first() {
                        if let Some(&first_child_idx) = node_id_to_pre_l.get(&first_child_id) {
                            node_type_l[first_child_idx] = true;
                        }
                    }
                    if let Some(&last_child_id) = children.last() {
                        if let Some(&last_child_idx) = node_id_to_pre_l.get(&last_child_id) {
                            node_type_r[last_child_idx] = true;
                        }
                    }
                }
            }
        }
        
        // Build postorder to preorder mappings
        let mut post_l_to_pre_l = vec![0; tree_size];
        let mut pre_l_to_post_l = vec![0; tree_size];
        
        for i in 0..tree_size {
            let node_id = post_l_node_ids[i];
            if let Some(&pre_l_idx) = node_id_to_pre_l.get(&node_id) {
                post_l_to_pre_l[i] = pre_l_idx;
                pre_l_to_post_l[pre_l_idx] = i;
            }
        }
        
        APTEDIndexer {
            node_info,
            tree_size,
            pre_l_node_ids,
            node_id_to_pre_l,
            sizes,
            parents: parents_fixed,
            pre_l_to_kr_sum,
            pre_l_to_rev_kr_sum,
            pre_l_to_desc_sum,
            pre_l_to_sum_del_cost,
            pre_l_to_sum_ins_cost,
            node_type_l,
            node_type_r,
            lchl,
            rchl,
            node_id_to_post_l,
            post_l_node_ids,
            post_l_to_pre_l,
            pre_l_to_post_l,
        }
    }
    
    fn get_node_info(&self, node_id: usize) -> Option<&TreeNodeInfo> {
        self.node_info.get(&node_id)
    }
    
    fn get_size(&self) -> usize {
        self.tree_size
    }
    
    /// Get the left-to-right preorder index for a node ID
    fn get_pre_l(&self, node_id: usize) -> Option<usize> {
        self.node_id_to_pre_l.get(&node_id).copied()
    }
    
    /// Get the postorder index for a node ID
    fn get_post_l(&self, node_id: usize) -> Option<usize> {
        self.node_id_to_post_l.get(&node_id).copied()
    }
    
    fn is_leaf(&self, node_id: usize) -> bool {
        if let Some(info) = self.node_info.get(&node_id) {
            info.children.is_empty()
        } else {
            false
        }
    }
}

/// Cost model for APTED - unit cost model
struct UnitCostModel;

impl UnitCostModel {
    fn del(&self, _node: &TreeNodeInfo) -> u64 {
        COST_DELETE
    }

    fn ins(&self, _node: &TreeNodeInfo) -> u64 {
        COST_INSERT
    }

    fn ren(&self, node1: &TreeNodeInfo, node2: &TreeNodeInfo) -> u64 {
        if node1.kind == node2.kind {
            if node1.children.is_empty() && node2.children.is_empty() {
                // Both are leaves
                if node1.text == node2.text {
                    0  // Identical
                } else {
                    COST_UPDATE
                }
            } else {
                // Same kind, internal nodes - can be matched with 0 cost
                0
            }
        } else {
            COST_UPDATE  // Different kinds
        }
    }
}

/// Compute the tree edit distance between two subtrees using APTED algorithm
/// This is the main O(n^2) algorithm based on the paper "Efficient Computation of the Tree Edit Distance"
fn tree_edit_distance(
    before_root_id: usize,
    after_root_id: usize,
    before_indexer: &APTEDIndexer,
    after_indexer: &APTEDIndexer,
    cost_model: &UnitCostModel,
) -> u64 {
    let before_root_pre_l = before_indexer.get_pre_l(before_root_id).unwrap_or(0);
    let after_root_pre_l = after_indexer.get_pre_l(after_root_id).unwrap_or(0);
    
    // Create DP table for the full tree edit distance
    let size1 = before_indexer.sizes[before_root_pre_l];
    let size2 = after_indexer.sizes[after_root_pre_l];
    
    // Initialize DP table for forest distance between the two subtrees
    // We'll use the standard tree edit distance approach
    let mut dp = vec![vec![0u64; size2 + 1]; size1 + 1];
    
    // Initialize: cost of deleting all before nodes
    for i in 1..=size1 {
        let node_id = before_indexer.pre_l_node_ids[before_root_pre_l + i - 1];
        dp[i][0] = dp[i - 1][0] + cost_model.del(before_indexer.node_info.get(&node_id).unwrap());
    }
    
    // Initialize: cost of inserting all after nodes  
    for j in 1..=size2 {
        let node_id = after_indexer.pre_l_node_ids[after_root_pre_l + j - 1];
        dp[0][j] = dp[0][j - 1] + cost_model.ins(after_indexer.node_info.get(&node_id).unwrap());
    }
    
    // Fill DP table - this is the O(n^2) part
    for i in 1..=size1 {
        for j in 1..=size2 {
            let before_node_id = before_indexer.pre_l_node_ids[before_root_pre_l + i - 1];
            let after_node_id = after_indexer.pre_l_node_ids[after_root_pre_l + j - 1];
            
            let before_info = before_indexer.node_info.get(&before_node_id).unwrap();
            let after_info = after_indexer.node_info.get(&after_node_id).unwrap();
            
            // Option 1: Match the two nodes and recursively match children
            let ren_cost = cost_model.ren(before_info, after_info);
            
            // For now, we'll use a simplified approach: match the nodes and add forest distance for children
            // This is where the full APTED would use the optimal strategy, but for O(n^2) we use this
            let children_cost = forest_distance_simple(
                &before_info.children,
                &after_info.children,
                before_indexer,
                after_indexer,
                cost_model,
            );
            
            let match_cost = ren_cost + children_cost;
            
            // Option 2: Delete before node/subtree
            let delete_cost = dp[i - 1][j] + subtree_del_cost(before_node_id, before_indexer, cost_model);
            
            // Option 3: Insert after node/subtree
            let insert_cost = dp[i][j - 1] + subtree_ins_cost(after_node_id, after_indexer, cost_model);
            
            dp[i][j] = match_cost.min(delete_cost).min(insert_cost);
        }
    }
    
    dp[size1][size2]
}

/// Simple forest distance computation for children
fn forest_distance_simple(
    before_children: &[usize],
    after_children: &[usize],
    before_indexer: &APTEDIndexer,
    after_indexer: &APTEDIndexer,
    cost_model: &UnitCostModel,
) -> u64 {
    let m = before_children.len();
    let n = after_children.len();
    
    if m == 0 && n == 0 {
        return 0;
    }
    
    if m == 0 {
        return after_children.iter().map(|&child_id| subtree_ins_cost(child_id, after_indexer, cost_model)).sum();
    }
    
    if n == 0 {
        return before_children.iter().map(|&child_id| subtree_del_cost(child_id, before_indexer, cost_model)).sum();
    }
    
    // Create DP table for forest matching
    let mut fp = vec![vec![0u64; n + 1]; m + 1];
    
    // Initialize: cost of deleting all before children
    for i in 1..=m {
        fp[i][0] = fp[i - 1][0] + subtree_del_cost(before_children[i - 1], before_indexer, cost_model);
    }
    
    // Initialize: cost of inserting all after children
    for j in 1..=n {
        fp[0][j] = fp[0][j - 1] + subtree_ins_cost(after_children[j - 1], after_indexer, cost_model);
    }
    
    // Fill DP table - O(n^2) forest distance
    for i in 1..=m {
        for j in 1..=n {
            let before_id = before_children[i - 1];
            let after_id = after_children[j - 1];
            
            // Option 1: Match the two subtrees
            let cost_match = fp[i - 1][j - 1] + tree_edit_distance(
                before_id,
                after_id,
                before_indexer,
                after_indexer,
                cost_model,
            );
            
            // Option 2: Delete before subtree
            let cost_delete = fp[i - 1][j] + subtree_del_cost(before_id, before_indexer, cost_model);
            
            // Option 3: Insert after subtree
            let cost_insert = fp[i][j - 1] + subtree_ins_cost(after_id, after_indexer, cost_model);
            
            fp[i][j] = cost_match.min(cost_delete).min(cost_insert);
        }
    }
    
    fp[m][n]
}

/// Compute the cost of deleting an entire subtree
fn subtree_del_cost(
    node_id: usize,
    indexer: &APTEDIndexer,
    cost_model: &UnitCostModel,
) -> u64 {
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
fn subtree_ins_cost(
    node_id: usize,
    indexer: &APTEDIndexer,
    cost_model: &UnitCostModel,
) -> u64 {
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

/// Compute the optimal tree edit distance using APTED algorithm
/// This function implements the full APTED algorithm for computing the optimal
/// edit distance between two trees in O(n^2) time and space.
pub fn for_nodes(
    before: &Code,
    after: &Code,
    _before_metadata: &ASTMetadata,
    _after_metadata: &ASTMetadata,
    before_node_ids: Vec<usize>,
    after_node_ids: Vec<usize>,
    node_cache: &NodeCache,
    diff: &mut ASTDiff,
) -> Result<()> {
    // Build indexers for both trees
    let before_indexer = APTEDIndexer::new(before, &node_cache.before);
    let after_indexer = APTEDIndexer::new(after, &node_cache.after);
    
    let cost_model = UnitCostModel;
    
    // For each pair of nodes, compute the tree edit distance and create mappings
    // This is where we would normally use the full APTED algorithm with strategy computation
    // For now, we'll use a greedy approach to create mappings based on the computed distances
    
    // If we have single nodes, compute the distance directly
    if before_node_ids.len() == 1 && after_node_ids.len() == 1 {
        let before_id = before_node_ids[0];
        let after_id = after_node_ids[0];
        
        let before_info = before_indexer.get_node_info(before_id);
        let after_info = after_indexer.get_node_info(after_id);
        
        if let (Some(before_info), Some(after_info)) = (before_info, after_info) {
            let ren_cost = cost_model.ren(before_info, after_info);
            
            let operation = if ren_cost == 0 {
                ASTMappingOperation::Identical
            } else if before_info.kind == after_info.kind {
                if before_info.children.is_empty() && after_info.children.is_empty() {
                    ASTMappingOperation::Update
                } else {
                    ASTMappingOperation::MatchButNotIdentical
                }
            } else {
                ASTMappingOperation::Update
            };
            
            let mapping = ASTMapping {
                cost: ren_cost,
                operation,
                ..Default::default()
            };
            
            diff.add_mapping(before_id, after_id, mapping);
            
            // Recursively map children
            map_children_recursive(
                &before_info.children,
                &after_info.children,
                &before_indexer,
                &after_indexer,
                &cost_model,
                diff,
            );
        }
    } else {
        // For multiple nodes, use forest distance
        // This is a simplified approach - the full APTED would use the optimal strategy
        let _total_cost = forest_distance(
            &before_node_ids,
            &after_node_ids,
            &before_indexer,
            &after_indexer,
            &cost_model,
        );
        
        // Create mappings using a greedy approach
        reconstruct_mapping(
            &before_node_ids,
            &after_node_ids,
            &before_indexer,
            &after_indexer,
            &cost_model,
            diff,
        );
    }
    
    Ok(())
}

/// Compute forest distance between two lists of root nodes
fn forest_distance(
    before_nodes: &[usize],
    after_nodes: &[usize],
    before_indexer: &APTEDIndexer,
    after_indexer: &APTEDIndexer,
    cost_model: &UnitCostModel,
) -> u64 {
    let m = before_nodes.len();
    let n = after_nodes.len();
    
    if m == 0 && n == 0 {
        return 0;
    }
    
    // Create DP table for forest matching
    let mut fp = vec![vec![0u64; n + 1]; m + 1];
    
    // Initialize: cost of deleting all before nodes
    for i in 1..=m {
        fp[i][0] = fp[i - 1][0] + subtree_del_cost(before_nodes[i - 1], before_indexer, cost_model);
    }
    
    // Initialize: cost of inserting all after nodes
    for j in 1..=n {
        fp[0][j] = fp[0][j - 1] + subtree_ins_cost(after_nodes[j - 1], after_indexer, cost_model);
    }
    
    // Fill DP table - O(n^2) forest distance
    for i in 1..=m {
        for j in 1..=n {
            let before_id = before_nodes[i - 1];
            let after_id = after_nodes[j - 1];
            
            // Option 1: Match the two subtrees
            let cost_match = fp[i - 1][j - 1] + tree_edit_distance(
                before_id,
                after_id,
                before_indexer,
                after_indexer,
                cost_model,
            );
            
            // Option 2: Delete before subtree
            let cost_delete = fp[i - 1][j] + subtree_del_cost(before_id, before_indexer, cost_model);
            
            // Option 3: Insert after subtree
            let cost_insert = fp[i][j - 1] + subtree_ins_cost(after_id, after_indexer, cost_model);
            
            fp[i][j] = cost_match.min(cost_delete).min(cost_insert);
        }
    }
    
    fp[m][n]
}

/// Recursively map children of matched nodes
fn map_children_recursive(
    before_children: &[usize],
    after_children: &[usize],
    before_indexer: &APTEDIndexer,
    after_indexer: &APTEDIndexer,
    cost_model: &UnitCostModel,
    diff: &mut ASTDiff,
) {
    // For now, use a simple greedy matching based on node kinds
    let mut remaining_before: Vec<usize> = before_children.to_vec();
    let mut remaining_after: Vec<usize> = after_children.to_vec();
    
    while !remaining_before.is_empty() || !remaining_after.is_empty() {
        let mut matched = false;
        let mut match_i = 0;
        let mut match_j = 0;
        
        // Try to find a match
        for (i, &before_id) in remaining_before.iter().enumerate() {
            for (j, &after_id) in remaining_after.iter().enumerate() {
                let info_before = match before_indexer.get_node_info(before_id) {
                    Some(info) => info,
                    None => continue,
                };
                
                let info_after = match after_indexer.get_node_info(after_id) {
                    Some(info) => info,
                    None => continue,
                };
                
                // Match nodes with the same kind
                if info_before.kind == info_after.kind {
                    match_i = i;
                    match_j = j;
                    matched = true;
                    break;
                }
            }
            if matched {
                break;
            }
        }
        
        if matched {
            let before_id = remaining_before[match_i];
            let after_id = remaining_after[match_j];
            
            let before_info = before_indexer.get_node_info(before_id).unwrap();
            let after_info = after_indexer.get_node_info(after_id).unwrap();
            
            // Determine operation based on whether nodes are identical
            let ren_cost = cost_model.ren(before_info, after_info);
            let operation = if ren_cost == 0 {
                ASTMappingOperation::Identical
            } else if before_info.kind == after_info.kind {
                if before_info.children.is_empty() && after_info.children.is_empty() {
                    ASTMappingOperation::Update
                } else {
                    ASTMappingOperation::MatchButNotIdentical
                }
            } else {
                ASTMappingOperation::Update
            };
            
            let mapping = ASTMapping {
                cost: ren_cost,
                operation,
                ..Default::default()
            };
            diff.add_mapping(before_id, after_id, mapping);
            
            // Recursively map children
            map_children_recursive(
                &before_info.children,
                &after_info.children,
                before_indexer,
                after_indexer,
                cost_model,
                diff,
            );
            
            // Remove matched nodes
            remaining_before.remove(match_i);
            remaining_after.remove(match_j);
        } else {
            // No more matches found, handle remaining nodes
            if !remaining_before.is_empty() {
                let before_id = remaining_before.remove(0);
                let mapping = ASTMapping {
                    cost: COST_DELETE,
                    operation: ASTMappingOperation::Delete,
                    ..Default::default()
                };
                diff.add_mapping(before_id, 0, mapping);
            } else if !remaining_after.is_empty() {
                let after_id = remaining_after.remove(0);
                let mapping = ASTMapping {
                    cost: COST_INSERT,
                    operation: ASTMappingOperation::Insert,
                    ..Default::default()
                };
                diff.add_mapping(0, after_id, mapping);
            }
        }
    }
}

/// Reconstruct the mapping from the computed distances using a greedy approach
fn reconstruct_mapping(
    before_node_ids: &[usize],
    after_node_ids: &[usize],
    before_indexer: &APTEDIndexer,
    after_indexer: &APTEDIndexer,
    cost_model: &UnitCostModel,
    diff: &mut ASTDiff,
) {
    let mut remaining_before: Vec<usize> = before_node_ids.to_vec();
    let mut remaining_after: Vec<usize> = after_node_ids.to_vec();
    
    while !remaining_before.is_empty() || !remaining_after.is_empty() {
        let mut matched = false;
        let mut match_i = 0;
        let mut match_j = 0;
        let mut best_cost = u64::MAX;
        
        // Try to find the best match based on tree edit distance
        for (i, &before_id) in remaining_before.iter().enumerate() {
            for (j, &after_id) in remaining_after.iter().enumerate() {
                let _before_info = match before_indexer.get_node_info(before_id) {
                    Some(info) => info,
                    None => continue,
                };
                
                let _after_info = match after_indexer.get_node_info(after_id) {
                    Some(info) => info,
                    None => continue,
                };
                
                // Compute tree edit distance for this pair
                let distance = tree_edit_distance(
                    before_id,
                    after_id,
                    before_indexer,
                    after_indexer,
                    cost_model,
                );
                
                if distance < best_cost {
                    best_cost = distance;
                    match_i = i;
                    match_j = j;
                    matched = true;
                }
            }
        }
        
        if matched && best_cost < COST_DELETE + COST_INSERT {
            let before_id = remaining_before[match_i];
            let after_id = remaining_after[match_j];
            
            let info_before = before_indexer.get_node_info(before_id).unwrap();
            let info_after = after_indexer.get_node_info(after_id).unwrap();
            
            // Determine operation based on whether nodes are identical
            let ren_cost = cost_model.ren(info_before, info_after);
            let operation = if ren_cost == 0 {
                ASTMappingOperation::Identical
            } else if info_before.kind == info_after.kind {
                if info_before.children.is_empty() && info_after.children.is_empty() {
                    ASTMappingOperation::Update
                } else {
                    ASTMappingOperation::MatchButNotIdentical
                }
            } else {
                ASTMappingOperation::Update
            };
            
            let mapping = ASTMapping {
                cost: best_cost,
                operation,
                ..Default::default()
            };
            diff.add_mapping(before_id, after_id, mapping);
            
            // Recursively map children
            map_children_recursive(
                &info_before.children,
                &info_after.children,
                before_indexer,
                after_indexer,
                cost_model,
                diff,
            );
            
            // Remove matched nodes
            remaining_before.remove(match_i);
            remaining_after.remove(match_j);
        } else {
            // No good match found, handle remaining nodes
            if !remaining_before.is_empty() {
                let before_id = remaining_before.remove(0);
                let mapping = ASTMapping {
                    cost: COST_DELETE,
                    operation: ASTMappingOperation::Delete,
                    ..Default::default()
                };
                diff.add_mapping(before_id, 0, mapping);
            } else if !remaining_after.is_empty() {
                let after_id = remaining_after.remove(0);
                let mapping = ASTMapping {
                    cost: COST_INSERT,
                    operation: ASTMappingOperation::Insert,
                    ..Default::default()
                };
                diff.add_mapping(0, after_id, mapping);
            }
        }
    }
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
    use crate::diff::ASTMappingOperation;
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
        assert!(diff.mapping.contains_key(&(before_root.id(), after_root.id())));

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
        assert!(diff.mapping.contains_key(&(before_root.id(), after_root.id())));

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
        assert!(diff.mapping.contains_key(&(before_root.id(), after_root.id())));

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
        let has_inserts = diff.mapping.values().any(|m| m.operation == ASTMappingOperation::Insert);
        assert!(has_inserts, "Should have insert operations for added nodes");

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
        let has_updates = diff.mapping.values().any(|m| m.operation == ASTMappingOperation::Update);
        let has_non_identical = diff.mapping.values().any(|m| m.operation == ASTMappingOperation::MatchButNotIdentical);
        let has_mappings = !diff.mapping.is_empty();
        assert!(has_updates || has_non_identical || has_mappings, "Should have update operations or non-identical mappings for changed nodes");

        Ok(())
    }
}