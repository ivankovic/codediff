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
use crate::code::{Code, ASTMetadata};
use crate::diff::hash_tree_matching::{self, HashMatchSpec};
use crate::diff::{ASTDiff, ASTMappingOperation, ASTMappingReason, NodeCache};
use std::collections::HashMap;

/**
* Compute depth for each node in the AST, given parent-child relationships.
* Returns a map from node_id to depth (0 = root, 1 = direct child of root, etc.)
*/
fn compute_node_depths(metadata: &ASTMetadata) -> HashMap<usize, usize> {
    let mut depths = HashMap::new();
    let mut stack = Vec::new();
    
    // Find the actual root by looking for nodes that are not children of any other node
    let all_children: std::collections::HashSet<usize> = metadata.node_info.values()
        .flat_map(|info| info.children.iter().copied())
        .collect();
    
    // Root nodes are those that are not children of any other node
    let root_nodes: Vec<usize> = metadata.node_info.keys()
        .filter(|&node_id| !all_children.contains(node_id))
        .copied()
        .collect();
    
    // Set depth 0 for root nodes and start BFS
    for &root_id in &root_nodes {
        depths.insert(root_id, 0);
        stack.push((root_id, 0));
    }
    
    // BFS to compute depths
    while let Some((node_id, depth)) = stack.pop() {
        if let Some(info) = metadata.node_info.get(&node_id) {
            for &child_id in &info.children {
                let child_depth = depth + 1;
                // Only set depth if not already set (first visit wins)
                depths.entry(child_id).or_insert(child_depth);
                stack.push((child_id, child_depth));
            }
        }
    }
    
    depths
}

/**
* Find all nodes that are "big enough" to match: either reference nodes OR 
* nodes that meet the configurable size criteria.
* Returns them sorted by subtree size in descending order.
* 
* EXPERIMENTAL: Parameters can be tuned for performance vs. optimality tradeoffs.
*/
fn find_big_enough_nodes(metadata: &ASTMetadata) -> Vec<usize> {
    use crate::diff::nodes::is_reference;
    
    // TUNING PARAMETERS - Fine-tuned for performance vs. optimality
    const MIN_DEPTH: usize = 2;    // At least this many levels deep
    const MIN_SUBTREE_SIZE: usize = 20; // At least this many nodes in subtree
    
    // Experimental findings:
    // - MIN_DEPTH=4, MIN_SUBTREE_SIZE=20: ~4.45s, 0 mismatches (original)
    // - MIN_DEPTH=2, MIN_SUBTREE_SIZE=20: ~3.49s, 0 mismatches (recommended)
    // - MIN_DEPTH=1, MIN_SUBTREE_SIZE=10: ~3.12s, 0 mismatches
    // - MIN_DEPTH=0, MIN_SUBTREE_SIZE=2:  ~2.60s, 0 mismatches (but affects other tests)
    // - MIN_DEPTH=0, MIN_SUBTREE_SIZE=1:  ~2.60s, 25+ mismatches (too aggressive)
    
    let depths = compute_node_depths(metadata);
    let language = metadata.language;
    
    let mut big_enough_nodes = Vec::new();
    
    for (&node_id, info) in &metadata.node_info {
        let subtree_size = metadata.node_to_subtree_size.get(&node_id).copied().unwrap_or(0);
        let depth = depths.get(&node_id).copied().unwrap_or(0);
        
        // Check if this node meets the criteria:
        // 1. It's a reference node, OR
        // 2. It's at least MIN_DEPTH levels deep AND has at least MIN_SUBTREE_SIZE nodes in its subtree
        let is_reference_node = is_reference(&info.kind, &language);
        let is_big_enough = depth >= MIN_DEPTH && subtree_size >= MIN_SUBTREE_SIZE;
        
        if is_reference_node || is_big_enough {
            big_enough_nodes.push((node_id, subtree_size));
        }
    }
    
    // Sort by subtree size in descending order
    big_enough_nodes.sort_by(|a, b| b.1.cmp(&a.1));
    
    // Extract just the node IDs
    big_enough_nodes.into_iter().map(|(node_id, _)| node_id).collect()
}

/**
* Perform size-ordered matching between two AST trees.
*
* This pass walks the pre-computed reference_nodes_ordered list (largest subtrees first) and, for
* each before-side reference node, looks for an unclaimed after-side node with the same *full*
* hash - byte-for-byte identical subtree content. On a hit it maps the pair with the
* IdenticalHash reason, then recursively maps all their children with the
* IdenticalHashOfAncestor reason. Duplicated code (several nodes sharing one full hash) pairs up
* copy-for-copy, since already-claimed after nodes are skipped.
*
* The traversal itself is shared with `solve_structurally_identical_trees` - see
* `hash_tree_matching::solve`; this file only configures it for full hashes.
* 
* EXPERIMENTAL: Now also matches "big enough" non-reference nodes (at least 4 levels deep 
* AND at least 20 nodes, OR rooted in a reference node).
*/
pub fn solve(before: &Code, after: &Code, node_cache: &NodeCache, diff: &mut ASTDiff) {
    let spec = HashMatchSpec {
        node_to_hash: |meta| &meta.node_to_full_hash,
        hash_to_nodes: |meta| &meta.full_hash_to_node,
        // A full-hash match is byte-identical by construction, no text comparison needed.
        classify: |_, _, _, _| (ASTMappingOperation::Identical, 0),
        root_reason: ASTMappingReason::IdenticalHash,
        descendant_reason: ASTMappingReason::IdenticalHashOfAncestor,
    };
    
    // Use the experimental big enough nodes logic
    hash_tree_matching::solve_with_node_list(
        before,
        after,
        node_cache,
        diff,
        &spec,
        |metadata| find_big_enough_nodes(metadata),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::{Code, Language, Metadata, metadata};
    use std::collections::HashSet;

    fn parsed_rust_code(contents: &str) -> Code {
        let mut code = Code {
            contents: contents.to_string(),
            metadata: Metadata {
                language: Some(Language::Rust),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&crate::code::language::to_treesitter(&Language::Rust).unwrap())
            .expect("set Rust grammar");
        code.parse(&mut parser);
        code
    }

    /// When the same statement appears more than once in a file, every copy hashes identically.
    /// `solve` skips after-side candidates that an earlier before-node has already claimed, so
    /// each duplicate before-node gets its *own* after-node: the copies pair up one-to-one
    /// instead of all collapsing onto a single target (which is what this pass used to do, and
    /// what left the remaining copies to be mis-reported as insert/delete pairs by later passes).
    ///
    /// Which copy pairs with which is deliberately not asserted: all candidates are equivalent
    /// under the full hash, and `HashSet` iteration order makes the specific assignment vary
    /// between process runs.
    #[test]
    fn duplicate_hash_group_matches_each_copy_to_a_distinct_after_node() {
        // `solve_identical_trees` only ever considers "reference nodes" (`reference_nodes.rs`;
        // for Rust: function/struct/impl items, not arbitrary statements), so the duplicated
        // node here has to be a whole `function_item`. `after` repeats the same two duplicated
        // functions as `before` *plus* a trailing, unique one, so nothing at the root/module
        // level is byte-identical and the algorithm can't shortcut by matching an enclosing node
        // wholesale - it has to decide, at the `function_item` level itself, which of the two
        // duplicated functions is "the" match. (The two functions sharing a name is invalid Rust,
        // but tree-sitter parses it fine, which is all `solve` needs.)
        let before_contents = "fn dup() {\n    1 + 1;\n}\nfn dup() {\n    1 + 1;\n}\n";
        let after_contents =
            "fn dup() {\n    1 + 1;\n}\nfn dup() {\n    1 + 1;\n}\nfn other() {\n    2;\n}\n";
        let before = parsed_rust_code(before_contents);
        let after = parsed_rust_code(after_contents);

        let before_metadata =
            metadata::compute_ast_metadata(&before).expect("compute before metadata");
        let after_metadata =
            metadata::compute_ast_metadata(&after).expect("compute after metadata");
        let node_cache = NodeCache::build(&before, &after);

        // Find the *outermost* hash group shared by at least two nodes on both sides - i.e. the
        // one with the largest byte span, since that's the duplicated subtree with no larger
        // duplicated ancestor to instead absorb the match. (Every node nested inside a duplicated
        // function is itself duplicated too - e.g. the `1` literal appears four times - so
        // picking by *count* would instead find one of those nested leaf groups, which aren't
        // reference nodes at all and so are never visited by `solve`'s own outer loop.)
        let (_, duplicate_before_ids) = before_metadata
            .full_hash_to_node
            .iter()
            .filter(|(hash, before_ids)| {
                before_ids.len() >= 2
                    && after_metadata
                        .full_hash_to_node
                        .get(*hash)
                        .is_some_and(|after_ids| after_ids.len() >= 2)
            })
            .max_by_key(|(_, before_ids)| {
                before_ids
                    .iter()
                    .filter_map(|id| node_cache.before.get(id))
                    .map(|node| node.byte_range().len())
                    .max()
                    .unwrap_or(0)
            })
            .expect("the duplicated `dup` functions must produce a shared-hash group");
        assert_eq!(
            duplicate_before_ids.len(),
            2,
            "expected exactly the two duplicated `function_item` nodes"
        );

        let mut diff = ASTDiff::default();
        solve(&before, &after, &node_cache, &mut diff);

        let after_targets: Vec<usize> = duplicate_before_ids
            .iter()
            .map(|before_id| {
                diff.mapping
                    .keys()
                    .find(|(b, _)| b == before_id)
                    .unwrap_or_else(|| panic!("before-node {before_id} was never matched at all"))
                    .1
            })
            .collect();

        assert_eq!(
            after_targets.iter().collect::<HashSet<_>>().len(),
            after_targets.len(),
            "each duplicate before-node must claim its own after-node"
        );
    }
}
