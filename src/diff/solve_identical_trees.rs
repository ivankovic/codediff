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

/// Find all nodes worth checking for a full-hash match: reference nodes, plus any node deep and
/// large enough that a match there is still worth the hash lookup. Returned largest-subtree-first.
///
/// MIN_DEPTH/MIN_SUBTREE_SIZE were tuned against the benchmark suite: 2/20 was the best
/// runtime/optimality tradeoff found (4/20 was slower for the same result; going below 2/20
/// started trading mismatches for speed).
fn find_big_enough_nodes(metadata: &ASTMetadata) -> Vec<usize> {
    use crate::diff::nodes::is_reference;

    const MIN_DEPTH: usize = 2;
    const MIN_SUBTREE_SIZE: usize = 20;

    let language = metadata.language;

    let mut big_enough_nodes = Vec::new();

    for (&node_id, info) in &metadata.node_info {
        let subtree_size = metadata.node_to_subtree_size.get(&node_id).copied().unwrap_or(0);
        let depth = metadata.node_to_depth.get(&node_id).copied().unwrap_or(0);

        let is_reference_node = is_reference(&info.kind, &language);
        let is_big_enough = depth >= MIN_DEPTH && subtree_size >= MIN_SUBTREE_SIZE;

        if is_reference_node || is_big_enough {
            big_enough_nodes.push((node_id, subtree_size, info.start_byte));
        }
    }

    // `metadata.node_info` is a `HashMap`, so the loop above visits nodes in a hash-seeded (not
    // deterministic) order. Sorting by `subtree_size` alone doesn't fix that: duplicate/repeated
    // code produces many equal-size nodes, and a stable sort only preserves whatever order they
    // arrived in. Breaking ties by `start_byte` (document position) makes the full ordering - and
    // therefore which duplicate a caller processes first - deterministic. This must be `start_byte`
    // and not `node_id`: node ids are tree-sitter arena slots, stable within one parse but not
    // across separate parses of identical source, so a node_id tiebreak would still let the result
    // vary between process runs even though it looks stable within a single run.
    big_enough_nodes.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)));
    big_enough_nodes.into_iter().map(|(node_id, _, _)| node_id).collect()
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
    /// under the full hash, so the specific assignment is an internal implementation detail (the
    /// order `full_hash_to_node`'s candidate list was built in) rather than a documented contract
    /// - even though, unlike when this test was written, that order is now deterministic.
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
