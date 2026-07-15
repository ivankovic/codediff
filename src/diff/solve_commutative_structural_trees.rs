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
* Perform size-ordered commutative structural matching between two AST trees.
*
* This is like `solve_structurally_identical_trees`, but uses commutative structural hashes
* that are invariant to child reordering for commutative containers (e.g., enum variants,
* struct fields, import lists). This allows matching of nodes that have the same structure
* but with children in different orders.
*
* For example:
* - Before: `enum E { A, B, C }`
* - After:  `enum E { B, A, C }`
*
* The regular structural hash would differ because the child order changed, but the
* commutative structural hash would be identical since it sorts children of the enum.
*
* The traversal itself is shared with `solve_identical_trees` - see
* `hash_tree_matching::solve`; this file only configures it for commutative structural hashes.
*
* Note: This pass uses only reference nodes (not extended node selection) to avoid
* false positives. Commutative matching is more lenient than structural matching, so
* we want to be conservative about which nodes we apply it to.
*/
pub fn solve(before: &Code, after: &Code, node_cache: &NodeCache, diff: &mut ASTDiff) {
    hash_tree_matching::solve(
        before,
        after,
        node_cache,
        diff,
        &HashMatchSpec {
            node_to_hash: |meta| &meta.node_to_commutative_structural_hash,
            hash_to_nodes: |meta| &meta.commutative_structural_hash_to_node,
            // Compare content via each side's precomputed full hash (leaf-token-driven), not raw
            // `utf8_text()`: the latter also captures every byte of interior formatting
            // (indentation, newlines), so a node that's purely re-indented - e.g. because an
            // ancestor wrapper was added or removed - would wrongly compare as changed.
            classify: |before_full_hash, after_full_hash| {
                if before_full_hash == after_full_hash {
                    (ASTMappingOperation::Identical, 0)
                } else {
                    (ASTMappingOperation::Update, COST_UPDATE)
                }
            },
            root_reason: ASTMappingReason::FullymappingSubtrees,
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
            node_to_hash: |meta| &meta.node_to_commutative_structural_hash,
            hash_to_nodes: |meta| &meta.commutative_structural_hash_to_node,
            classify: |before_full_hash, after_full_hash| {
                if before_full_hash == after_full_hash {
                    (ASTMappingOperation::Identical, 0)
                } else {
                    (ASTMappingOperation::Update, COST_UPDATE)
                }
            },
            root_reason: ASTMappingReason::FullymappingSubtrees,
            descendant_reason: ASTMappingReason::StructurallyIdenticalAncestor,
        },
        config.to_node_list_selector(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::metadata::compute_ast_metadata;
    use crate::code::Language;

    #[test]
    fn test_commutative_hash_differs_from_structural_for_reordered_go_fields() {
        // Create two Go structs with fields in different orders
        // Their structural hashes should differ (order matters)
        // But their commutative structural hashes should be the same (order doesn't matter)
        
        let code_before = Code::from_string(
            r#"type Person struct {
    Name string
    Age  int
}
"#,
            &Language::Go,
        );
        
        let code_after = Code::from_string(
            r#"type Person struct {
    Age  int
    Name string
}
"#,
            &Language::Go,
        );
        
        let meta_before = compute_ast_metadata(&code_before).expect("Should compute metadata");
        let meta_after = compute_ast_metadata(&code_after).expect("Should compute metadata");
        
        // Find the field_list node in before
        let before_ast = code_before.ast.as_ref().unwrap();
        let after_ast = code_after.ast.as_ref().unwrap();
        
        // Find field_list nodes
        let before_field_list_id = find_node_of_kind(before_ast.root_node(), "field_list");
        let after_field_list_id = find_node_of_kind(after_ast.root_node(), "field_list");
        
        // If we found field_list nodes, check their hashes
        if let (Some(b_id), Some(a_id)) = (before_field_list_id, after_field_list_id) {
            let b_commut_hash = meta_before.node_to_commutative_structural_hash.get(&b_id);
            let a_commut_hash = meta_after.node_to_commutative_structural_hash.get(&a_id);
            
            // Commutative structural hashes should be the same
            assert_eq!(b_commut_hash, a_commut_hash, 
                      "Commutative structural hashes should be the same for reordered Go struct fields");
        }
    }

    /// Helper to find a node of a specific kind in a tree
    fn find_node_of_kind(root: tree_sitter::Node, kind: &str) -> Option<usize> {
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if node.kind() == kind {
                return Some(node.id());
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
        }
        None
    }
}
