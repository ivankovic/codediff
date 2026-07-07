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
use std::collections::{HashMap, HashSet};

use tree_sitter::Node;

use crate::code::{ASTMetadata, Code};
use crate::code::metadata::metadata_of;
use crate::diff::{ASTDiff, ASTMapping, ASTMappingOperation, ASTMappingReason, NodeCache};

/**
* The shared engine behind `solve_identical_trees` and `solve_structurally_identical_trees`.
*
* Both passes do the same thing - walk the before tree's reference nodes largest-first, look the
* node's hash up in an after-side reverse map, claim the first not-yet-matched candidate, and then
* pair the two subtrees' descendants positionally - and differ only in *which* hash they consult
* and how a paired node is classified. This spec captures exactly those two degrees of freedom so
* the traversal, the claimed-candidate filtering and the descendant pairing exist once.
*/
pub(crate) struct HashMatchSpec {
    /// Which per-node hash map to read on the before side (full or structural).
    pub node_to_hash: fn(&ASTMetadata) -> &HashMap<usize, u64>,
    /// The corresponding reverse map on the after side.
    pub hash_to_nodes: fn(&ASTMetadata) -> &HashMap<u64, HashSet<usize>>,
    /// Classifies one paired (before, after) node: the operation and its cost. Full-hash matches
    /// are `Identical` by construction; structural matches compare the nodes' text to decide
    /// between `Identical` and `Update`.
    pub classify: fn(before: Node, after: Node, before_src: &[u8], after_src: &[u8]) -> (ASTMappingOperation, u64),
    /// Reason recorded on the reference node itself.
    pub root_reason: ASTMappingReason,
    /// Reason recorded on every descendant paired under it.
    pub descendant_reason: ASTMappingReason,
}

/**
* Perform size-ordered hash matching between two AST trees, as configured by `spec`.
*
* Uses the pre-computed `reference_nodes_ordered` list to visit before-side reference nodes in
* order of decreasing subtree size. For each one, looks for an after-side node with the same hash
* that no earlier before-node has already claimed (when the same hash appears more than once, the
* file simply contains duplicated code - each copy gets its own partner instead of every copy
* collapsing onto one). On a hit, maps the pair with `spec.root_reason` and then descends both
* subtrees in lockstep, pairing children by position and kind with `spec.descendant_reason`.
*/
pub(crate) fn solve(
    before: &Code,
    after: &Code,
    node_cache: &NodeCache,
    diff: &mut ASTDiff,
    spec: &HashMatchSpec,
) {
    solve_with_node_list(before, after, node_cache, diff, spec, |metadata| metadata.reference_nodes_ordered.clone())
}

/// Generic version of solve that accepts a custom node list selector
pub(crate) fn solve_with_node_list(
    before: &Code,
    after: &Code,
    node_cache: &NodeCache,
    diff: &mut ASTDiff,
    spec: &HashMatchSpec,
    node_list_selector: impl Fn(&ASTMetadata) -> Vec<usize>,
) {
    let before_metadata = metadata_of(before);
    let after_metadata = metadata_of(after);
    let before_src = before.contents.as_bytes();
    let after_src = after.contents.as_bytes();

    let before_node_ids = node_list_selector(&before_metadata);
    
    for &before_node_id in &before_node_ids {
        // Skip nodes already mapped, whether by an earlier pass or by a larger ancestor match
        // made earlier in this very loop.
        if diff.before_node_map.contains_key(&before_node_id) {
            continue;
        }
        let Some(&before_node) = node_cache.before.get(&before_node_id) else {
            continue;
        };
        let Some(before_hash) = (spec.node_to_hash)(&before_metadata).get(&before_node_id) else {
            continue;
        };
        let Some(after_candidates) = (spec.hash_to_nodes)(&after_metadata).get(before_hash) else {
            continue;
        };

        // Skip after nodes already claimed - by an earlier pass, or by a previous before-node in
        // this same hash group - so duplicated code pairs up copy-for-copy instead of stealing a
        // match that belongs to another pair. Which unclaimed copy wins is arbitrary (`HashSet`
        // iteration order), which is fine: all candidates are equivalent under this pass's hash.
        let Some(&after_node_id) = after_candidates
            .iter()
            .find(|&&id| !diff.after_node_map.contains_key(&id))
        else {
            continue;
        };
        let Some(&after_node) = node_cache.after.get(&after_node_id) else {
            continue;
        };

        let (operation, cost) = (spec.classify)(before_node, after_node, before_src, after_src);
        diff.add_mapping(
            before_node_id,
            after_node_id,
            ASTMapping {
                cost,
                operation,
                reason: spec.root_reason.clone(),
            },
        );

        // Descend both subtrees in lockstep, pairing children by position and kind.
        let mut stack = vec![(before_node, after_node)];
        while let Some((before_parent, after_parent)) = stack.pop() {
            let mut before_cursor = before_parent.walk();
            let mut after_cursor = after_parent.walk();
            for (before_child, after_child) in before_parent
                .children(&mut before_cursor)
                .zip(after_parent.children(&mut after_cursor))
            {
                if before_child.kind() != after_child.kind() {
                    continue;
                }
                if diff.before_node_map.contains_key(&before_child.id()) {
                    continue;
                }
                let (operation, cost) =
                    (spec.classify)(before_child, after_child, before_src, after_src);
                diff.add_mapping(
                    before_child.id(),
                    after_child.id(),
                    ASTMapping {
                        cost,
                        operation,
                        reason: spec.descendant_reason.clone(),
                    },
                );
                stack.push((before_child, after_child));
            }
        }
    }
}
