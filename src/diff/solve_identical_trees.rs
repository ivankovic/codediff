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
use crate::diff::{ASTDiff, ASTMapping, ASTMappingOperation, ASTMappingReason, NodeCache};

/**
* Perform size-ordered matching between two AST trees.
*
* This function uses the pre-computed reference_nodes_ordered list to efficiently
* find reference nodes in order of decreasing subtree size. For each reference node,
* it checks if a node with the same full hash exists in the after tree. If they do, it adds
* the two nodes to the mapping collection with the IdenticalHash reason, and then recursively
* adds all their children nodes with the IdenticalHashOfAncestor reason.
*/
pub fn solve(before: &Code, after: &Code, node_cache: &NodeCache, diff: &mut ASTDiff) {
    let _before_tree = before.ast.as_ref().expect("Before code must be parsed");
    let _after_tree = after.ast.as_ref().expect("After code must be parsed");

    // Use existing metadata or compute if not available
    // Note: We clone to avoid lifetime issues, but in practice metadata is usually already computed
    let before_metadata =
        before.metadata.ast_metadata.clone().unwrap_or_else(|| {
            crate::code::metadata::compute_ast_metadata(before).unwrap_or_default()
        });

    let after_metadata =
        after.metadata.ast_metadata.clone().unwrap_or_else(|| {
            crate::code::metadata::compute_ast_metadata(after).unwrap_or_default()
        });

    // Get the pre-computed reference nodes ordered by subtree size (largest first)
    let reference_nodes_ordered = &before_metadata.reference_nodes_ordered;

    // Iterate over reference nodes in order (largest subtrees first)
    for &before_node_id in reference_nodes_ordered {
        // Skip already mapped nodes
        if diff
            .mapping
            .iter()
            .any(|((before_id, _), _)| *before_id == before_node_id)
        {
            continue;
        }

        // Get the node from the cache
        let before_node = node_cache.before.get(&before_node_id).cloned();
        let Some(before_node) = before_node else {
            continue;
        };

        // Get the hash for this reference node
        let Some(before_hash) = before_metadata.node_to_full_hash.get(&before_node_id) else {
            continue;
        };

        // Look for a node in the after tree with the same hash
        if let Some(after_node_ids) = after_metadata.full_hash_to_node.get(before_hash) {
            // If we have multiple reference nodes with exactly the same full hash, then
            // there is simply duplicated code in the file, and quite big chunks of it too.
            // For now, we take the first node and match to that.
            //
            // TODO: Implement better matching strategy for multiple nodes with same hash
            if let Some(&after_node_id) = after_node_ids.iter().next() {
                // Find the actual node with the matching ID - use cache for O(1) lookup
                let matching_after_node = node_cache.after.get(&after_node_id).cloned();
                let Some(matching_after_node) = matching_after_node else {
                    continue;
                };

                {
                    let after_node_id = matching_after_node.id();

                    // Add this mapping
                    diff.add_mapping(
                        before_node_id,
                        after_node_id,
                        ASTMapping {
                            cost: 0,
                            operation: ASTMappingOperation::Identical,
                            reason: ASTMappingReason::IdenticalHash,
                        },
                    );

                    // Recursively add all descendants with IdenticalHashOfAncestor reason
                    let mut stack = vec![(before_node, matching_after_node)];

                    while let Some((before_parent, after_parent)) = stack.pop() {
                        let mut before_children_cursor = before_parent.walk();
                        let mut after_children_cursor = after_parent.walk();

                        let before_children: Vec<_> = before_parent
                            .children(&mut before_children_cursor)
                            .collect();
                        let after_children: Vec<_> =
                            after_parent.children(&mut after_children_cursor).collect();

                        // Match children by position and kind
                        for (before_child, after_child) in
                            before_children.into_iter().zip(after_children.into_iter())
                        {
                            if before_child.kind() == after_child.kind() {
                                let before_child_id = before_child.id();
                                let after_child_id = after_child.id();

                                if !diff
                                    .mapping
                                    .iter()
                                    .any(|((child_id, _), _)| *child_id == before_child_id)
                                {
                                    diff.add_mapping(
                                        before_child_id,
                                        after_child_id,
                                        ASTMapping {
                                            cost: 0,
                                            operation: ASTMappingOperation::Identical,
                                            reason: ASTMappingReason::IdenticalHashOfAncestor,
                                        },
                                    );

                                    // Add this child to stack to process its children
                                    stack.push((before_child, after_child));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
