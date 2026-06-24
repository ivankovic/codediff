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
            // This is more than just an unprincipled choice of *which* duplicate to match: since
            // `full_hash_to_node`'s value is a `HashSet<usize>` (default, randomized hasher),
            // *which* node `.next()` returns can differ between separate process runs of this
            // same binary on the exact same input (though it's consistent across repeated calls
            // within one run, since this is just reading an unmodified set). Worse, nothing here
            // removes an after-node from consideration once some before-node has claimed it, so
            // if `before` also has N>1 nodes sharing this hash, every one of them independently
            // computes the same `.next()` and they all collapse onto that *one* after-node -
            // leaving the other (N-1) equally-valid after-side duplicates unmatched by this pass
            // entirely (see `duplicate_hash_group_collapses_onto_a_single_after_node` below for a
            // test pinning this specific behavior). A later, less precise pass ends up treating
            // those leftover duplicates as an insert/delete pair instead of a no-op match.
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
    /// `solve` doesn't track which after-side candidates an earlier before-node in the same hash
    /// group has already claimed, so every before-node in the group independently picks
    /// `after_node_ids.iter().next()` - which, for an *unmodified* `HashSet` queried repeatedly
    /// within one process, returns the same element every time. The result: every duplicate
    /// before-node maps to the *same* after-node, and the other, equally-valid after-candidates
    /// are never matched by this pass at all.
    ///
    /// This test pins that *structural* outcome (they collapse onto one target) rather than
    /// *which* node wins, since which one wins depends on `HashSet`'s randomized iteration order
    /// and can differ between process runs of the same binary on the same input.
    #[test]
    fn duplicate_hash_group_collapses_onto_a_single_after_node() {
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
            1,
            "all duplicate before-nodes currently collapse onto the same after-node - if this \
             starts failing, the matching strategy improved and this test (and the comment on \
             `after_node_ids.iter().next()` above) should be updated together"
        );
    }
}
