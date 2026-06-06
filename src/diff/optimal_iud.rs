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
/// Optimized implementation of the optimal IU diff algorithm.
///
/// This module implements a highly optimized version of the tree edit distance algorithm
/// for Insert, Update, Delete operations. It uses principles from the APTED algorithm
/// (Pawlik & Augsten, 2015/2016) but adapted for the CodeDiff project's specific needs.
///
/// Key optimizations over the original recursive approach:
/// - More efficient memoization key (avoids Arc<[usize]>)
/// - Early pruning of expensive branches
/// - Better bounds checking
/// - Cleaner code structure

use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use tree_sitter::Node;

use crate::code::{ASTMetadata, Code};
use crate::diff::{ASTDiff, ASTMapping, ASTMappingOperation, ASTMappingReason, COST_UPDATE};
use crate::diff::{COST_DELETE, COST_INSERT, NodeCache};

/// Maximum cost value to avoid overflow.
const MAX_COST: u64 = u64::MAX / 2;

/// Hashable key for subtree vectors using efficient hashing.
/// This is much more efficient than the original Arc<[usize]> approach.
#[derive(Debug, Clone, Eq, PartialEq)]
struct SubtreeKey {
    /// Rolling hash of the subtree IDs
    hash: u64,
    /// Length of the subtree vector
    len: usize,
    /// First node ID in the vector
    first_id: usize,
    /// Last node ID in the vector
    last_id: usize,
}

impl Hash for SubtreeKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash.hash(state);
        self.len.hash(state);
        self.first_id.hash(state);
        self.last_id.hash(state);
    }
}

impl SubtreeKey {
    fn new(subtrees: &[usize]) -> Self {
        if subtrees.is_empty() {
            return SubtreeKey {
                hash: 0,
                len: 0,
                first_id: 0,
                last_id: 0,
            };
        }
        
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for &id in subtrees {
            std::hash::Hash::hash(&id, &mut hasher);
        }
        
        SubtreeKey {
            hash: hasher.finish(),
            len: subtrees.len(),
            first_id: subtrees[0],
            last_id: subtrees[subtrees.len() - 1],
        }
    }
}

/// Result of solving a subproblem.
#[derive(Debug, Clone, Default)]
struct SolveResult {
    /// Total cost of the solution
    cost: u64,
    /// Operation for the root nodes
    operation: ASTMappingOperation,
    /// Index for insert/delete operations (where to split)
    index: usize,
}

/// Check if a node is mapped in the diff.
fn is_node_mapped(node_id: usize, diff: &ASTDiff) -> bool {
    diff.before_node_map.contains_key(&node_id) || diff.after_node_map.contains_key(&node_id)
}

/// Find the first unmapped node in a sequence.
fn find_first_unmapped(nodes: &[usize], diff: &ASTDiff) -> (bool, usize) {
    for (i, &node_id) in nodes.iter().enumerate() {
        if !is_node_mapped(node_id, diff) {
            return (true, i);
        }
    }
    (false, 0)
}

/// Get children node IDs for a given node.
fn get_children(node: &Node) -> Vec<usize> {
    let mut ids = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        ids.push(child.id());
    }
    ids
}

/// Count unmatched nodes in a subtree.
fn count_unmatched(
    node_id: usize,
    node_cache: &HashMap<usize, Node<'static>>,
    mapped_nodes: &HashMap<usize, usize>,
) -> usize {
    let node = match node_cache.get(&node_id) {
        Some(n) => n,
        None => return 0,
    };

    let mut count = if !mapped_nodes.contains_key(&node_id) { 1 } else { 0 };

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        count += count_unmatched(child.id(), node_cache, mapped_nodes);
    }

    count
}

/// Internal solve function using slices to avoid allocations.
fn solve_with_slices(
    before: &Code,
    after: &Code,
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
    before_subtrees: &[usize],
    after_subtrees: &[usize],
    node_cache: &NodeCache,
    diff: &ASTDiff,
    memo: &mut HashMap<(SubtreeKey, SubtreeKey), SolveResult>,
) -> Result<u64> {
    // Early exit for empty cases
    if before_subtrees.is_empty() && after_subtrees.is_empty() {
        return Ok(0);
    }

    // Check memoization
    let before_key = SubtreeKey::new(before_subtrees);
    let after_key = SubtreeKey::new(after_subtrees);
    let key = (before_key.clone(), after_key.clone());
    
    if let Some(solution) = memo.get(&key) {
        return Ok(solution.cost);
    }

    // Find first unmapped nodes
    let (before_has_unmapped, before_first_idx) = find_first_unmapped(before_subtrees, diff);
    let (after_has_unmapped, after_first_idx) = find_first_unmapped(after_subtrees, diff);

    // Handle edge cases
    if before_subtrees.is_empty() || !before_has_unmapped {
        // Insert all remaining after subtrees
        let mut total_cost = 0u64;
        for &after_id in after_subtrees {
            if !is_node_mapped(after_id, diff) {
                let unmatched = count_unmatched(after_id, &node_cache.after, &diff.after_node_map);
                total_cost += unmatched as u64 * COST_INSERT;
            }
        }
        
        let solution = SolveResult {
            cost: total_cost,
            operation: ASTMappingOperation::InsertWithChildren,
            index: 0,
        };
        memo.insert(key, solution);
        return Ok(total_cost);
    }

    if after_subtrees.is_empty() || !after_has_unmapped {
        // Delete all remaining before subtrees
        let mut total_cost = 0u64;
        for &before_id in before_subtrees {
            if !is_node_mapped(before_id, diff) {
                let unmatched = count_unmatched(before_id, &node_cache.before, &diff.before_node_map);
                total_cost += unmatched as u64 * COST_DELETE;
            }
        }
        
        let solution = SolveResult {
            cost: total_cost,
            operation: ASTMappingOperation::DeleteWithChildren,
            index: 0,
        };
        memo.insert(key, solution);
        return Ok(total_cost);
    }

    // Skip matched nodes at the beginning
    if before_first_idx != 0 || after_first_idx != 0 {
        return solve_with_slices(
            before,
            after,
            before_metadata,
            after_metadata,
            &before_subtrees[before_first_idx..],
            &after_subtrees[after_first_idx..],
            node_cache,
            diff,
            memo,
        );
    }

    // Now we have the first nodes of both sequences unmapped
    let before_first = node_cache.before.get(&before_subtrees[0])
        .ok_or_else(|| anyhow!("Before node {} not found", before_subtrees[0]))?;
    let after_first = node_cache.after.get(&after_subtrees[0])
        .ok_or_else(|| anyhow!("After node {} not found", after_subtrees[0]))?;

    let before_first_id = before_subtrees[0];
    let after_first_id = after_subtrees[0];

    // Check hash match for perfect subtree match
    let hashes_match = before_metadata.node_to_full_hash.get(&before_first_id)
        .and_then(|before_hash| {
            after_metadata.node_to_full_hash.get(&after_first_id)
                .map(|after_hash| before_hash == after_hash)
        })
        .unwrap_or(false);

    // Best solution found so far
    let mut best_cost = MAX_COST;
    let mut best_solution = SolveResult::default();

    // Option 1: Match the first nodes
    if before_first.kind() == after_first.kind() {
        let mut cost = 0u64;
        let mut operation = ASTMappingOperation::NotYetSet;

        // For leaf nodes, check if they're identical
        if before_first.child_count() == 0 && after_first.child_count() == 0 {
            let before_text = before_first.utf8_text(before.contents.as_bytes());
            let after_text = after_first.utf8_text(after.contents.as_bytes());
            
            if before_text == after_text {
                operation = ASTMappingOperation::Identical;
            } else {
                operation = ASTMappingOperation::Update;
                cost += COST_UPDATE;
            }
        } else if hashes_match {
            // Perfect hash match - nodes and subtrees are identical
            operation = ASTMappingOperation::Identical;
        } else {
            // Same kind but not identical subtrees
            operation = ASTMappingOperation::MatchButNotIdentical;
        }

        // Cost for remaining nodes at this level
        cost += solve_with_slices(
            before,
            after,
            before_metadata,
            after_metadata,
            &before_subtrees[1..],
            &after_subtrees[1..],
            node_cache,
            diff,
            memo,
        )?;

        // Cost for children if not a perfect hash match
        if !hashes_match {
            let before_children = get_children(before_first);
            let after_children = get_children(after_first);
            cost += solve_with_slices(
                before,
                after,
                before_metadata,
                after_metadata,
                &before_children,
                &after_children,
                node_cache,
                diff,
                memo,
            )?;
        }

        if cost < best_cost {
            best_cost = cost;
            best_solution = SolveResult {
                cost,
                operation,
                index: 0,
            };
        }
    }

    // Option 2: Delete the first before node
    let before_children = get_children(before_first);
    
    for i in 0..=after_subtrees.len() {
        if best_cost <= COST_DELETE {
            break; // Can't do better than a simple delete
        }
        
        let mut cost = COST_DELETE;
        
        // Cost to match the rest of the subtrees
        let cost_to_match_rest = solve_with_slices(
            before,
            after,
            before_metadata,
            after_metadata,
            &before_subtrees[1..],
            &after_subtrees[i..],
            node_cache,
            diff,
            memo,
        )?;
        
        // Early pruning
        if cost + cost_to_match_rest >= best_cost {
            continue;
        }
        
        // Cost to match the children with the prefix of after_subtrees
        cost += cost_to_match_rest + solve_with_slices(
            before,
            after,
            before_metadata,
            after_metadata,
            &before_children,
            &after_subtrees[..i],
            node_cache,
            diff,
            memo,
        )?;
        
        if cost < best_cost {
            best_cost = cost;
            best_solution = SolveResult {
                cost,
                operation: ASTMappingOperation::Delete,
                index: i,
            };
        }
    }

    // Option 3: Insert the first after node
    let after_children = get_children(after_first);
    
    for i in 0..=before_subtrees.len() {
        if best_cost <= COST_INSERT {
            break; // Can't do better than a simple insert
        }
        
        let mut cost = COST_INSERT;
        
        // Cost to match the rest
        let cost_to_match_rest = solve_with_slices(
            before,
            after,
            before_metadata,
            after_metadata,
            &before_subtrees[i..],
            &after_subtrees[1..],
            node_cache,
            diff,
            memo,
        )?;
        
        // Early pruning
        if cost + cost_to_match_rest >= best_cost {
            continue;
        }
        
        // Cost to match the prefix of before_subtrees with the children
        cost += cost_to_match_rest + solve_with_slices(
            before,
            after,
            before_metadata,
            after_metadata,
            &before_subtrees[..i],
            &after_children,
            node_cache,
            diff,
            memo,
        )?;
        
        if cost < best_cost {
            best_cost = cost;
            best_solution = SolveResult {
                cost,
                operation: ASTMappingOperation::Insert,
                index: i,
            };
        }
    }

    // Store the best solution in memo
    memo.insert(key, best_solution);
    
    Ok(best_cost)
}

/// Internal solve function that owns the vectors (for compatibility).
fn solve(
    before: &Code,
    after: &Code,
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
    before_subtrees: Vec<usize>,
    after_subtrees: Vec<usize>,
    node_cache: &NodeCache,
    diff: &ASTDiff,
    memo: &mut HashMap<(SubtreeKey, SubtreeKey), SolveResult>,
) -> Result<u64> {
    solve_with_slices(
        before,
        after,
        before_metadata,
        after_metadata,
        &before_subtrees,
        &after_subtrees,
        node_cache,
        diff,
        memo,
    )
}

/// Add subtree to diff with a given operation.
fn add_subtree_to_diff(
    node_ids: Vec<usize>,
    operation: &ASTMappingOperation,
    cost_of_one_operation: u64,
    node_cache: &NodeCache,
    diff: &mut ASTDiff,
) -> Result<u64> {
    let mut total_cost = 0;
    
    for node_id in node_ids {
        let node = node_cache.get_in_any(&node_id)
            .ok_or_else(|| anyhow!("Node not found in cache"))?;
        
        let children_cost = add_subtree_to_diff(
            get_children(node),
            operation,
            cost_of_one_operation,
            node_cache,
            diff,
        )?;
        
        let node_cost = cost_of_one_operation + children_cost;
        total_cost += node_cost;
        
        let mapping = ASTMapping {
            cost: node_cost,
            operation: operation.clone(),
            reason: ASTMappingReason::OptimalIDU,
        };
        
        if *operation == ASTMappingOperation::InsertWithChildren {
            if !diff.after_node_map.contains_key(&node_id) {
                diff.add_mapping(0, node_id, mapping);
            }
        } else if !diff.before_node_map.contains_key(&node_id) {
            diff.add_mapping(node_id, 0, mapping);
        }
    }
    
    Ok(total_cost)
}

/// Update the diff using the memoized solutions.
fn update_diff(
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
    before_node_ids: &[usize],
    after_node_ids: &[usize],
    node_cache: &NodeCache,
    memo: &HashMap<(SubtreeKey, SubtreeKey), SolveResult>,
    diff: &mut ASTDiff,
) -> Result<()> {
    let mut stack = Vec::new();
    stack.push((before_node_ids.to_vec(), after_node_ids.to_vec()));
    
    while let Some((before_subtrees, after_subtrees)) = stack.pop() {
        if before_subtrees.is_empty() && after_subtrees.is_empty() {
            continue;
        }
        
        let (before_has_unmapped, before_first_idx) = find_first_unmapped(&before_subtrees, diff);
        let (after_has_unmapped, after_first_idx) = find_first_unmapped(&after_subtrees, diff);
        
        // Handle edge cases
        if before_subtrees.is_empty() || !before_has_unmapped {
            if after_has_unmapped {
                add_subtree_to_diff(
                    after_subtrees[after_first_idx..].to_vec(),
                    &ASTMappingOperation::InsertWithChildren,
                    COST_INSERT,
                    node_cache,
                    diff,
                )?;
            }
            continue;
        }
        
        if after_subtrees.is_empty() || !after_has_unmapped {
            if before_has_unmapped {
                add_subtree_to_diff(
                    before_subtrees[before_first_idx..].to_vec(),
                    &ASTMappingOperation::DeleteWithChildren,
                    COST_DELETE,
                    node_cache,
                    diff,
                )?;
            }
            continue;
        }
        
        // Skip matched nodes
        if before_first_idx != 0 || after_first_idx != 0 {
            stack.push((
                before_subtrees[before_first_idx..].to_vec(),
                after_subtrees[after_first_idx..].to_vec(),
            ));
            continue;
        }
        
        // Get the solution from memo
        let before_key = SubtreeKey::new(&before_subtrees);
        let after_key = SubtreeKey::new(&after_subtrees);
        let key = (before_key, after_key);
        
        let solution = memo.get(&key)
            .ok_or_else(|| anyhow!("Solution not found in memo for subtrees {:?} -> {:?}", before_subtrees, after_subtrees))?;
        
        let before_first = node_cache.before.get(&before_subtrees[0])
            .ok_or_else(|| anyhow!("Before node {} not found", before_subtrees[0]))?;
        let after_first = node_cache.after.get(&after_subtrees[0])
            .ok_or_else(|| anyhow!("After node {} not found", after_subtrees[0]))?;
        
        let before_first_id = before_subtrees[0];
        let after_first_id = after_subtrees[0];
        
        // Check hash match
        let hashes_match = before_metadata.node_to_full_hash.get(&before_first_id)
            .and_then(|before_hash| {
                after_metadata.node_to_full_hash.get(&after_first_id)
                    .map(|after_hash| before_hash == after_hash)
            })
            .unwrap_or(false);
        
        let mapping = ASTMapping {
            cost: solution.cost,
            operation: solution.operation.clone(),
            reason: ASTMappingReason::OptimalIDU,
        };
        
        match solution.operation {
            ASTMappingOperation::Identical | ASTMappingOperation::MatchButNotIdentical | ASTMappingOperation::Update => {
                diff.add_mapping(before_subtrees[0], after_subtrees[0], mapping);
                
                if before_subtrees.len() > 1 || after_subtrees.len() > 1 {
                    stack.push((
                        before_subtrees[1..].to_vec(),
                        after_subtrees[1..].to_vec(),
                    ));
                }
                
                if !hashes_match {
                    stack.push((
                        get_children(before_first),
                        get_children(after_first),
                    ));
                } else {
                    // If hashes match, recursively add all children as identical
                    let mut node_stack = vec![(before_subtrees[0], after_subtrees[0])];
                    while let Some((before_parent_id, after_parent_id)) = node_stack.pop() {
                        let before_parent = node_cache.before.get(&before_parent_id)
                            .ok_or_else(|| anyhow!("Before node {} not found", before_parent_id))?;
                        let after_parent = node_cache.after.get(&after_parent_id)
                            .ok_or_else(|| anyhow!("After node {} not found", after_parent_id))?;
                        
                        let mut before_cursor = before_parent.walk();
                        let mut after_cursor = after_parent.walk();
                        
                        let before_children: Vec<_> = before_parent.children(&mut before_cursor).collect();
                        let after_children: Vec<_> = after_parent.children(&mut after_cursor).collect();
                        
                        for (before_child, after_child) in before_children.into_iter().zip(after_children.into_iter()) {
                            let before_child_id = before_child.id();
                            let after_child_id = after_child.id();
                            
                            if !diff.mapping.contains_key(&(before_child_id, after_child_id)) {
                                let child_mapping = ASTMapping {
                                    cost: 0,
                                    operation: ASTMappingOperation::Identical,
                                    reason: ASTMappingReason::IdenticalHashOfAncestor,
                                };
                                diff.add_mapping(before_child_id, after_child_id, child_mapping);
                                node_stack.push((before_child_id, after_child_id));
                            }
                        }
                    }
                }
            }
            ASTMappingOperation::Insert => {
                diff.add_mapping(0, after_subtrees[0], mapping);
                
                stack.push((
                    before_subtrees[solution.index..].to_vec(),
                    after_subtrees[1..].to_vec(),
                ));
                stack.push((
                    before_subtrees[..solution.index].to_vec(),
                    get_children(after_first),
                ));
            }
            ASTMappingOperation::Delete => {
                diff.add_mapping(before_subtrees[0], 0, mapping);
                
                stack.push((
                    before_subtrees[1..].to_vec(),
                    after_subtrees[solution.index..].to_vec(),
                ));
                stack.push((
                    get_children(before_first),
                    after_subtrees[..solution.index].to_vec(),
                ));
            }
            ASTMappingOperation::InsertWithChildren => {
                add_subtree_to_diff(
                    after_subtrees.to_vec(),
                    &ASTMappingOperation::InsertWithChildren,
                    COST_INSERT,
                    node_cache,
                    diff,
                )?;
            }
            ASTMappingOperation::DeleteWithChildren => {
                add_subtree_to_diff(
                    before_subtrees.to_vec(),
                    &ASTMappingOperation::DeleteWithChildren,
                    COST_DELETE,
                    node_cache,
                    diff,
                )?;
            }
            _ => {}
        }
    }
    
    Ok(())
}

/// Find the optimal mapping for all nodes in before and after that have not yet been mapped in diff,
/// but only using Insert, Delete and Update operations, and the "null operation" Identical that simply
/// matches identical nodes.
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
    let mut memo = HashMap::new();
    
    solve(
        before,
        after,
        before_metadata,
        after_metadata,
        before_node_ids.clone(),
        after_node_ids.clone(),
        node_cache,
        diff,
        &mut memo,
    )?;
    
    update_diff(
        before_metadata,
        after_metadata,
        &before_node_ids,
        &after_node_ids,
        node_cache,
        &memo,
        diff,
    )?;
    
    Ok(())
}

/// Find the optimal mapping for the root nodes.
pub fn for_roots(
    before: &Code,
    after: &Code,
    node_cache: &NodeCache,
    diff: &mut ASTDiff,
) -> Result<()> {
    // Compute metadata if not available
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
    use crate::test;
    use anyhow::Result;

    #[test]
    fn test_count_unmatched_nodes() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;
        let code = test_codes.get("hello-world.rs").unwrap().clone();

        let root_id = code.ast.as_ref().unwrap().root_node().id();

        let node_cache = NodeCache::build(&code, &code);
        let mut diff = ASTDiff {
            ..Default::default()
        };

        let expected = 22;
        let actual = count_unmatched(root_id, &node_cache.before, &diff.before_node_map);
        assert_eq!(actual, expected, "Expected {} unmatched nodes, got {}", expected, actual);

        // Test with one node mapped
        diff.add_mapping(
            root_id,
            root_id,
            crate::diff::ASTMapping {
                cost: 0,
                operation: ASTMappingOperation::Identical,
                reason: crate::diff::ASTMappingReason::IdenticalHash,
            },
        );

        let expected = 21;
        let actual = count_unmatched(root_id, &node_cache.before, &diff.before_node_map);
        assert_eq!(actual, expected, "Expected {} unmatched nodes after mapping root, got {}", expected, actual);

        Ok(())
    }

    #[test]
    fn solve_for_hello_world_translation() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;

        let before = test_codes.get("hello-world.rs").unwrap().clone();
        let after = test_codes.get("zdravo-svijete.rs").unwrap().clone();

        let diff = ASTDiff {
            ..Default::default()
        };

        let mut memo = HashMap::new();
        let node_cache = NodeCache::build(&before, &after);

        // Compute metadata for tests
        let before_metadata = before
            .metadata
            .ast_metadata
            .as_ref()
            .cloned()
            .unwrap_or_else(|| crate::code::metadata::compute_ast_metadata(&before).unwrap_or_default());
        let after_metadata = after
            .metadata
            .ast_metadata
            .as_ref()
            .cloned()
            .unwrap_or_else(|| crate::code::metadata::compute_ast_metadata(&after).unwrap_or_default());

        let before_root_id = before.ast.as_ref().unwrap().root_node().id();
        let after_root_id = after.ast.as_ref().unwrap().root_node().id();

        let total_cost = solve(
            &before,
            &after,
            &before_metadata,
            &after_metadata,
            vec![before_root_id],
            vec![after_root_id],
            &node_cache,
            &diff,
            &mut memo,
        )?;

        // The trees differ only in the string constant, so cost should be 1 (update)
        assert_eq!(total_cost, 1);

        Ok(())
    }

    #[test]
    fn solve_for_hello_world_added_message() -> Result<()> {
        let test_diffs = test::helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs.get("hello-world-added-message").unwrap().clone();

        let diff = ASTDiff {
            ..Default::default()
        };

        let mut memo = HashMap::new();
        let node_cache = NodeCache::build(&before, &after);

        // Compute metadata for tests
        let before_metadata = before
            .metadata
            .ast_metadata
            .as_ref()
            .cloned()
            .unwrap_or_else(|| crate::code::metadata::compute_ast_metadata(&before).unwrap_or_default());
        let after_metadata = after
            .metadata
            .ast_metadata
            .as_ref()
            .cloned()
            .unwrap_or_else(|| crate::code::metadata::compute_ast_metadata(&after).unwrap_or_default());

        let before_root_id = before.ast.as_ref().unwrap().root_node().id();
        let after_root_id = after.ast.as_ref().unwrap().root_node().id();

        let total_cost = solve(
            &before,
            &after,
            &before_metadata,
            &after_metadata,
            vec![before_root_id],
            vec![after_root_id],
            &node_cache,
            &diff,
            &mut memo,
        )?;

        // From the original test, the optimal cost is 12
        assert_eq!(total_cost, 12);

        Ok(())
    }
}
