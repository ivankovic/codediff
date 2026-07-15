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
use crate::code::{ASTMetadata, Code};
use crate::diff::hash_tree_matching::{self, HashMatchSpec};
use crate::diff::{ASTDiff, ASTMappingOperation, ASTMappingReason, COST_UPDATE, NodeCache};

/**
* Perform multi-level normalized hash matching between two AST trees.
*
* This solver uses normalized hashes at different levels of abstraction to match nodes
* that have the same structure but differ in specific ways:
*
* - Level 2 (NormalizedStructuralIgnorePunctuation): Ignores punctuation/whitespace differences.
*   Matches code with same structure but different formatting.
* - Level 3 (NormalizedStructuralIgnoreLiterals): Normalizes all literals to placeholders.
*   Matches code with same structure but different literal values (strings, numbers, etc.).
* - Level 4 (NormalizedStructuralIgnoreIdentifiers): Normalizes all identifiers to placeholders.
*   Matches code with same structure but different variable/function names.
* - Level 5 (NormalizedStructuralIgnorePunctuationAndLiterals): Combines punctuation and literal normalization.
*   Matches code with same structure but different formatting and literal values.
*
* Each level is tried in order from most specific to most general, so that more precise
* matches are found first. This allows the algorithm to handle various types of changes
* while still maintaining accuracy.
*
* The traversal is shared with other hash-based solvers - see `hash_tree_matching::solve`;
* this file only configures it for each normalized hash level.
*/
pub fn solve(before: &Code, after: &Code, node_cache: &NodeCache, diff: &mut ASTDiff) {
    // Try each normalization level in order from most specific to most general
    // Level 2: Ignore punctuation
    hash_tree_matching::solve(
        before,
        after,
        node_cache,
        diff,
        &HashMatchSpec {
            node_to_hash: |meta: &ASTMetadata| &meta.node_to_normalized_punct_hash,
            hash_to_nodes: |meta: &ASTMetadata| &meta.normalized_punct_hash_to_node,
            classify: classify_matched_nodes,
            root_reason: ASTMappingReason::NormalizedStructuralIgnorePunctuation,
            descendant_reason: ASTMappingReason::StructurallyIdenticalAncestor,
        },
    );

    // Level 3: Ignore literals
    hash_tree_matching::solve(
        before,
        after,
        node_cache,
        diff,
        &HashMatchSpec {
            node_to_hash: |meta: &ASTMetadata| &meta.node_to_normalized_literal_hash,
            hash_to_nodes: |meta: &ASTMetadata| &meta.normalized_literal_hash_to_node,
            classify: classify_matched_nodes,
            root_reason: ASTMappingReason::NormalizedStructuralIgnoreLiterals,
            descendant_reason: ASTMappingReason::StructurallyIdenticalAncestor,
        },
    );

    // Level 4: Ignore identifiers
    hash_tree_matching::solve(
        before,
        after,
        node_cache,
        diff,
        &HashMatchSpec {
            node_to_hash: |meta: &ASTMetadata| &meta.node_to_normalized_identifier_hash,
            hash_to_nodes: |meta: &ASTMetadata| &meta.normalized_identifier_hash_to_node,
            classify: classify_matched_nodes,
            root_reason: ASTMappingReason::NormalizedStructuralIgnoreIdentifiers,
            descendant_reason: ASTMappingReason::StructurallyIdenticalAncestor,
        },
    );

    // Level 5: Ignore both punctuation and literals
    hash_tree_matching::solve(
        before,
        after,
        node_cache,
        diff,
        &HashMatchSpec {
            node_to_hash: |meta: &ASTMetadata| &meta.node_to_normalized_punct_literal_hash,
            hash_to_nodes: |meta: &ASTMetadata| &meta.normalized_punct_literal_hash_to_node,
            classify: classify_matched_nodes,
            root_reason: ASTMappingReason::NormalizedStructuralIgnorePunctuationAndLiterals,
            descendant_reason: ASTMappingReason::StructurallyIdenticalAncestor,
        },
    );
}

/// Classify matched nodes based on their full hashes
fn classify_matched_nodes(before_full_hash: u64, after_full_hash: u64) -> (ASTMappingOperation, u64) {
    if before_full_hash == after_full_hash {
        (ASTMappingOperation::Identical, 0)
    } else {
        (ASTMappingOperation::Update, COST_UPDATE)
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::metadata::compute_ast_metadata;
    use crate::code::Language;

    #[test]
    fn test_normalized_punct_hash_ignores_formatting() {
        // Create two functions with same logic but different formatting
        let code_before = Code::from_string(
            r#"fn main() {
    let x = 1;
    let y = 2;
    println!("{} {}", x, y);
}
"#,
            &Language::Rust,
        );

        let code_after = Code::from_string(
            r#"fn main() {
    let x=1;
    let y=2;
    println!("{} {}",x,y);
}
"#,
            &Language::Rust,
        );

        let meta_before = compute_ast_metadata(&code_before).expect("Should compute metadata");
        let meta_after = compute_ast_metadata(&code_after).expect("Should compute metadata");

        // Get the function_item nodes
        let before_ast = code_before.ast.as_ref().unwrap();
        let after_ast = code_after.ast.as_ref().unwrap();

        let before_func_id = find_node_of_kind(before_ast.root_node(), "function_item");
        let after_func_id = find_node_of_kind(after_ast.root_node(), "function_item");

        if let (Some(b_id), Some(a_id)) = (before_func_id, after_func_id) {
            let b_punct_hash = meta_before.node_to_normalized_punct_hash.get(&b_id);
            let a_punct_hash = meta_after.node_to_normalized_punct_hash.get(&a_id);

            // Normalized punct hashes should be the same even with different formatting
            assert_eq!(b_punct_hash, a_punct_hash,
                      "Normalized punct hashes should be the same for differently formatted code");
        }
    }

    #[test]
    fn test_normalized_literal_hash_ignores_string_values() {
        // Create two functions with same structure but different string literals
        let code_before = Code::from_string(
            r#"fn main() {
    println!("Hello, World!");
}
"#,
            &Language::Rust,
        );

        let code_after = Code::from_string(
            r#"fn main() {
    println!("Zdravo, Svijete!");
}
"#,
            &Language::Rust,
        );

        let meta_before = compute_ast_metadata(&code_before).expect("Should compute metadata");
        let meta_after = compute_ast_metadata(&code_after).expect("Should compute metadata");

        // Get the function_item nodes
        let before_ast = code_before.ast.as_ref().unwrap();
        let after_ast = code_after.ast.as_ref().unwrap();

        let before_func_id = find_node_of_kind(before_ast.root_node(), "function_item");
        let after_func_id = find_node_of_kind(after_ast.root_node(), "function_item");

        if let (Some(b_id), Some(a_id)) = (before_func_id, after_func_id) {
            let b_literal_hash = meta_before.node_to_normalized_literal_hash.get(&b_id);
            let a_literal_hash = meta_after.node_to_normalized_literal_hash.get(&a_id);

            // Normalized literal hashes should be the same even with different string values
            assert_eq!(b_literal_hash, a_literal_hash,
                      "Normalized literal hashes should be the same for code with different string literals");
        }
    }

    #[test]
    fn test_normalized_identifier_hash_ignores_variable_names() {
        // Create two functions with same structure but different variable names
        let code_before = Code::from_string(
            r#"fn main() {
    let x = 1;
    let y = 2;
    println!("{} {}", x, y);
}
"#,
            &Language::Rust,
        );

        let code_after = Code::from_string(
            r#"fn main() {
    let a = 1;
    let b = 2;
    println!("{} {}", a, b);
}
"#,
            &Language::Rust,
        );

        let meta_before = compute_ast_metadata(&code_before).expect("Should compute metadata");
        let meta_after = compute_ast_metadata(&code_after).expect("Should compute metadata");

        // Get the function_item nodes
        let before_ast = code_before.ast.as_ref().unwrap();
        let after_ast = code_after.ast.as_ref().unwrap();

        let before_func_id = find_node_of_kind(before_ast.root_node(), "function_item");
        let after_func_id = find_node_of_kind(after_ast.root_node(), "function_item");

        if let (Some(b_id), Some(a_id)) = (before_func_id, after_func_id) {
            let b_id_hash = meta_before.node_to_normalized_identifier_hash.get(&b_id);
            let a_id_hash = meta_after.node_to_normalized_identifier_hash.get(&a_id);

            // Normalized identifier hashes should be the same even with different variable names
            assert_eq!(b_id_hash, a_id_hash,
                      "Normalized identifier hashes should be the same for code with different variable names");
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
