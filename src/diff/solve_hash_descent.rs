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

use crate::code::Code;
use crate::code::metadata::metadata_of;
use crate::diff::hash_tree_matching::{self, NodeSelectionConfig};
use crate::diff::solve_import_nodes::{extract_import_path, is_import_node};
use crate::diff::{ASTDiff, ASTMappingReason, NodeCache};

/**
* Phase 1 of the six-phase pipeline rework (`TODO.md`, 2026-07-17): "hash-based, largest-subtree-
* first descent". Runs the generalized `hash_tree_matching::solve_with_hash_map` engine three
* times, once per hash algorithm - each call finds and matches whatever it can, then the next call
* only sees whatever's left unmatched (`PostorderIndexer`/the engine's own `before_node_map` check
* skip anything already claimed):
*
* 1. `KindAndValueHash` - byte-identical subtrees (replaces `solve_identical_trees`). Extended node
*    selection (reference nodes + big-enough nodes), matching `solve_identical_trees`' tuning.
* 2. `KindOnlyHash` - same shape, any leaf value (replaces `solve_structurally_identical_trees`,
*    and folds in `solve_multilevel_hash`'s 4 normalized variants - see `TODO.md`'s accepted
*    precision-loss tradeoff). Reference nodes only, matching `solve_structurally_identical_trees`'
*    narrower selection.
* 3. Normalized import path hash - import statements matched by normalized path rather than syntax
*    (replaces `solve_import_nodes`, folded in as a hash variant instead of its own phase/pass).
*    Reference nodes only; the hash is computed only for recognized import-statement kinds
*    (`solve_import_nodes::is_import_node`), so non-import nodes never collide into this map.
*
* Both `KindAndValueHash` and `KindOnlyHash` are order-independent per `nodes::is_commutative_
* container` at *every* recursion level (see `code::hash::compute_kind_and_value_hash`'s doc
* comment), so `solve_commutative_structural_trees` is not ported forward here - order-independence
* is now inherent to both hashes, not a bolted-on third one.
*/
pub fn solve(
    before: &Code,
    after: &Code,
    node_cache: &NodeCache,
    diff: &mut ASTDiff,
    solve_import_path_hash_enabled: bool,
) {
    let selector_config = NodeSelectionConfig::default();
    let before_metadata = metadata_of(before);
    let after_metadata = metadata_of(after);
    hash_tree_matching::solve_with_hash_map(
        before,
        after,
        node_cache,
        diff,
        &before_metadata.node_to_kind_and_value_hash,
        &after_metadata.kind_and_value_hash_to_node,
        ASTMappingReason::IdenticalHash,
        ASTMappingReason::IdenticalHashOfAncestor,
        selector_config.to_node_list_selector(),
    );

    hash_tree_matching::solve_with_hash_map(
        before,
        after,
        node_cache,
        diff,
        &before_metadata.node_to_kind_only_hash,
        &after_metadata.kind_only_hash_to_node,
        ASTMappingReason::StructurallyIdenticalSubtrees,
        ASTMappingReason::StructurallyIdenticalAncestor,
        |metadata| metadata.reference_nodes_ordered.clone(),
    );
    drop(before_metadata);
    drop(after_metadata);

    if solve_import_path_hash_enabled {
        solve_import_path_hash(before, after, node_cache, diff);
    }
}

/// Builds a normalized-import-path hash map (import nodes only, hashed by
/// `solve_import_nodes::normalize_import_path`'s output) and runs it through the same generalized
/// engine as the other two hash algorithms - see the module doc comment.
fn solve_import_path_hash(before: &Code, after: &Code, node_cache: &NodeCache, diff: &mut ASTDiff) {
    let before_metadata = metadata_of(before);
    let after_metadata = metadata_of(after);

    let before_hash = import_path_hash_map(&before_metadata);
    let after_hash = import_path_hash_map(&after_metadata);

    let mut after_reverse: HashMap<u64, Vec<usize>> = HashMap::new();
    for (&node_id, &hash) in &after_hash {
        after_reverse.entry(hash).or_default().push(node_id);
    }

    hash_tree_matching::solve_with_hash_map(
        before,
        after,
        node_cache,
        diff,
        &before_hash,
        &after_reverse,
        ASTMappingReason::NormalizedImportPath,
        ASTMappingReason::NormalizedImportPath,
        move |metadata| {
            let language = metadata.language;
            metadata
                .node_info
                .iter()
                .filter(|(_, info)| is_import_node(&info.kind, &language))
                .map(|(&id, _)| id)
                .collect()
        },
    );
}

fn import_path_hash_map(metadata: &crate::code::ASTMetadata) -> HashMap<usize, u64> {
    use std::hash::{Hash, Hasher};
    let language = metadata.language;
    let mut result = HashMap::new();
    for (&node_id, info) in &metadata.node_info {
        if !is_import_node(&info.kind, &language) {
            continue;
        }
        let Some(path) = extract_import_path(node_id, metadata) else { continue };
        let mut hasher = rustc_hash::FxHasher::default();
        path.hash(&mut hasher);
        result.insert(node_id, hasher.finish());
    }
    result
}
