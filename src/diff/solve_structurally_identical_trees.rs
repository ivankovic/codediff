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
use crate::diff::hash_tree_matching::{self, HashMatchSpec, NodeSelectionConfig};
use crate::diff::{ASTDiff, ASTMappingOperation, ASTMappingReason, COST_UPDATE, NodeCache};

/**
* Perform size-ordered structural matching between two AST trees.
*
* Like `solve_identical_trees`, but keyed on the *structural* hash: for each before-side
* reference node (largest subtrees first) it looks for an unclaimed after-side node whose
* subtree has the same shape - same node kinds in the same arrangement - even if leaf values
* differ. On a hit it maps the pair with the StructurallyIdenticalSubtrees reason, then descends
* the child subtrees, mapping them with the StructurallyIdenticalAncestor reason. Each paired
* node's text is compared to pick the operation: Identical when the values match, Update (cost 1)
* when they differ.
*
* The traversal itself is shared with `solve_identical_trees` - see
* `hash_tree_matching::solve`; this file only configures it for structural hashes.
* 
* Note: Unlike `solve_identical_trees`, this pass uses only reference nodes (not extended
* node selection) to avoid regressions in certain test cases like cpp-ladybird-refactor-variables-if-changes.
*/
pub fn solve(before: &Code, after: &Code, node_cache: &NodeCache, diff: &mut ASTDiff) {
    hash_tree_matching::solve(
        before,
        after,
        node_cache,
        diff,
        &HashMatchSpec {
            node_to_hash: |meta| &meta.node_to_structural_hash,
            hash_to_nodes: |meta| &meta.structural_hash_to_node,
            classify: |before_node, after_node, before_src, after_src| {
                let before_value = before_node.utf8_text(before_src).unwrap_or("");
                let after_value = after_node.utf8_text(after_src).unwrap_or("");
                if before_value == after_value {
                    (ASTMappingOperation::Identical, 0)
                } else {
                    (ASTMappingOperation::Update, COST_UPDATE)
                }
            },
            root_reason: ASTMappingReason::StructurallyIdenticalSubtrees,
            descendant_reason: ASTMappingReason::StructurallyIdenticalAncestor,
        },
    );
}

/// Like `solve`, but with custom node selection thresholds (including non-reference nodes).
/// This variant can be used for experimentation, but the default `solve()` using only
/// reference nodes is recommended for production use.
pub fn solve_with_config(
    before: &Code,
    after: &Code,
    node_cache: &NodeCache,
    diff: &mut ASTDiff,
    config: &NodeSelectionConfig,
) {
    hash_tree_matching::solve_with_node_list(
        before,
        after,
        node_cache,
        diff,
        &HashMatchSpec {
            node_to_hash: |meta| &meta.node_to_structural_hash,
            hash_to_nodes: |meta| &meta.structural_hash_to_node,
            classify: |before_node, after_node, before_src, after_src| {
                let before_value = before_node.utf8_text(before_src).unwrap_or("");
                let after_value = after_node.utf8_text(after_src).unwrap_or("");
                if before_value == after_value {
                    (ASTMappingOperation::Identical, 0)
                } else {
                    (ASTMappingOperation::Update, COST_UPDATE)
                }
            },
            root_reason: ASTMappingReason::StructurallyIdenticalSubtrees,
            descendant_reason: ASTMappingReason::StructurallyIdenticalAncestor,
        },
        config.to_node_list_selector(),
    );
}
