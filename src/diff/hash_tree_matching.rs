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
use std::collections::HashMap;

use crate::code::{ASTMetadata, Code};
use crate::code::metadata::metadata_of;
use crate::diff::nodes::is_reference;
use crate::diff::{ASTDiff, ASTMapping, ASTMappingOperation, ASTMappingReason, NodeCache};

/// Configuration for selecting which nodes to consider for hash-based matching.
/// 
/// Nodes are included if they are either:
/// - Reference nodes (language-specific structural elements like functions, classes)
/// - OR meet the minimum depth and subtree size thresholds
/// 
/// This allows the hash matching passes to also consider large, deep subtrees
/// that aren't formally "reference nodes" but are still worth matching.
#[derive(Debug, Clone)]
pub struct NodeSelectionConfig {
    /// Minimum tree depth for a non-reference node to be included
    pub min_depth: usize,
    /// Minimum number of nodes in the subtree for a non-reference node to be included
    pub min_subtree_size: usize,
}

impl Default for NodeSelectionConfig {
    fn default() -> Self {
        Self {
            // Tuned against the benchmark suite: 0/45 was found to provide optimal results.
            // With solve_identical_trees using extended selection and solve_structurally_identical_trees
            // using only reference nodes, this gives: 1437 -> 771 (666 fewer mismatches).
            // This is the best configuration found after testing various thresholds.
            min_depth: 0,
            min_subtree_size: 45,
        }
    }
}

impl NodeSelectionConfig {
    /// Create a node list selector function that can be passed to `solve_with_node_list`.
    /// 
    /// The selector includes reference nodes plus any nodes that meet the depth/size thresholds,
    /// sorted by subtree size (largest first) and then by start byte for deterministic ordering.
    pub fn to_node_list_selector(&self) -> impl Fn(&ASTMetadata) -> Vec<usize> + '_ {
        move |metadata: &ASTMetadata| build_extended_node_list(metadata, self)
    }
}

/// Build an extended node list that includes reference nodes plus nodes meeting size thresholds.
/// 
/// Returns nodes sorted by subtree size (largest first), with ties broken by start_byte
/// for deterministic ordering across runs.
pub fn build_extended_node_list(metadata: &ASTMetadata, config: &NodeSelectionConfig) -> Vec<usize> {
    let language = metadata.language;
    let mut nodes_with_info: Vec<(usize, usize, usize)> = Vec::new(); // (node_id, subtree_size, start_byte)

    for (&node_id, info) in &metadata.node_info {
        let subtree_size = metadata.node_to_subtree_size.get(&node_id).copied().unwrap_or(0);
        let depth = metadata.node_to_depth.get(&node_id).copied().unwrap_or(0);
        let start_byte = info.start_byte;

        let is_reference_node = is_reference(&info.kind, &language);
        let is_big_enough = depth >= config.min_depth && subtree_size >= config.min_subtree_size;

        if is_reference_node || is_big_enough {
            nodes_with_info.push((node_id, subtree_size, start_byte));
        }
    }

    // Sort by subtree size descending, then by start_byte ascending for deterministic tiebreaking.
    // Using start_byte (document position) rather than node_id ensures stability across
    // separate parses of identical source, since node_ids are tree-sitter arena slots that
    // may differ between parses even for identical code.
    nodes_with_info.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)));
    nodes_with_info.into_iter().map(|(node_id, _, _)| node_id).collect()
}

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
    pub hash_to_nodes: fn(&ASTMetadata) -> &HashMap<u64, Vec<usize>>,
    /// Classifies one paired (before, after) node: the operation and its cost, given each side's
    /// precomputed full hash (an O(1) map lookup the caller already has, rather than a Node +
    /// source the callee would have to re-hash - `classify` runs once per node in every matched
    /// subtree, so anything more than a lookup here is an easy O(n^2) trap on large subtrees).
    /// Full-hash matches are `Identical` by construction; structural matches compare the two
    /// full hashes to decide between `Identical` and `Update`.
    pub classify: fn(before_full_hash: u64, after_full_hash: u64) -> (ASTMappingOperation, u64),
    /// Reason recorded on the reference node itself.
    pub root_reason: ASTMappingReason,
    /// Reason recorded on every descendant paired under it.
    pub descendant_reason: ASTMappingReason,
}

/**
* Perform size-ordered hash matching between two AST trees, as configured by `spec`.
*
* This is a convenience wrapper around `solve_with_node_list` that uses only reference nodes
* (via `reference_nodes_ordered`). For matching that also includes big-enough non-reference
* nodes, call `solve_with_node_list` directly with a custom node list selector.
*
* Uses the pre-computed `reference_nodes_ordered` list to visit before-side reference nodes in
* order of decreasing subtree size. For each one, looks for an after-side node with the same hash
* that no earlier before-node has already claimed (when the same hash appears more than once, the
* file simply contains duplicated code - each copy gets its own partner instead of every copy
* collapsing onto one). On a hit, maps the pair with `spec.root_reason` and then descends both
* subtrees in lockstep, pairing children by position and kind with `spec.descendant_reason`.
* 
* Note: This function is retained for backward compatibility. New code should use
* `solve_with_node_list` with `NodeSelectionConfig::to_node_list_selector()` for the
* extended node selection (reference nodes + big-enough nodes).
*/
#[allow(dead_code)]
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

        // Among after nodes not already claimed - by an earlier pass, or by a previous before-node
        // in this same hash group - pick whichever sits closest in the file to `before_node`'s own
        // position, rather than just the first unclaimed one in `hash::hash_code`'s visitation
        // order. Two nodes sharing a hash aren't necessarily "the same" content that moved: since
        // `node_to_full_hash` is content-only (kind/text of leaves), unrelated boilerplate in a
        // different context (e.g. the same one-line loop body repeated in two different `impl`
        // blocks) can collide on the same hash without being duplicates of each other at all. A
        // real, unmoved match keeps roughly the same byte offset before/after, so proximity is a
        // much better tiebreak than construction order for telling those apart - true duplicates
        // (interchangeable by definition, see `duplicate_hash_group_matches_each_copy_to_a_distinct_after_node`)
        // still end up paired deterministically, just by position instead of by discovery order.
        let Some(&after_node_id) = after_candidates
            .iter()
            .filter(|&&id| !diff.after_node_map.contains_key(&id))
            .min_by_key(|&&id| {
                node_cache
                    .after
                    .get(&id)
                    .map(|n| n.start_byte().abs_diff(before_node.start_byte()))
                    .unwrap_or(usize::MAX)
            })
        else {
            continue;
        };
        let Some(&after_node) = node_cache.after.get(&after_node_id) else {
            continue;
        };

        let before_full_hash = before_metadata.node_to_full_hash.get(&before_node_id).copied().unwrap_or(0);
        let after_full_hash = after_metadata.node_to_full_hash.get(&after_node_id).copied().unwrap_or(0);
        let (operation, cost) = (spec.classify)(before_full_hash, after_full_hash);
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
                let before_full_hash =
                    before_metadata.node_to_full_hash.get(&before_child.id()).copied().unwrap_or(0);
                let after_full_hash =
                    after_metadata.node_to_full_hash.get(&after_child.id()).copied().unwrap_or(0);
                let (operation, cost) = (spec.classify)(before_full_hash, after_full_hash);
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
