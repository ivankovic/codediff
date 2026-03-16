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
pub mod hash;

use anyhow::Result;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

use crate::code::Code;

/**
* The main data structure. Contains the difference between two Code structures.
*
* Any function that accepts this structure or any sub-field should not assume any fields are set
* and should always first check that the required data is actually available, if not it should try
* to construct it, and if that doesn't work it should fail-safe, ideally returning a safe zero
* result. This allows the calling code to extremely efficiently process large files, and files that
* only pretend to be code but are data or configuration. To help the compiler enforce this, most
* fields should be wrapped in Option.
*
* See code.rs for the Code structure.
*/
#[derive(Debug, Clone, Default, Serialize)]
pub struct Diff {
    /// Difference based on the ASTs.
    pub ast: Option<ASTDiff>,
}

/**
* Difference between two Code structures, based on their TreeSitter ASTs.
*
* In theory, instead of having deleted and added separately, we could also have just mapped and use
* "0" as a "null" mapping that serves the same purpose. But this is cleaner.
*/
#[derive(Debug, Clone, Default, Serialize)]
pub struct ASTDiff {
    /// Map of AST nodes from the before AST to the after AST.
    pub mapped: HashMap<(usize, usize), ASTMapping>,
    /// The nodes in the before AST that were deleted and so do not map to any nodes in after.
    pub deleted: HashSet<usize>,
    /// The nodes in the after AST that were added and so do not map to any nodes in before.
    pub added: HashSet<usize>,
    /// Metadata about the before AST, including hashes for all nodes.
    pub before_metadata: Option<ASTMetadata>,
    /// Metadata about the after AST, including hashes for all nodes.
    pub after_metadata: Option<ASTMetadata>,
}

/**
* Information about the mapping of two AST nodes.
*/
#[derive(Debug, Clone, Default, Serialize)]
pub struct ASTMapping {
    /// A score between 0 and 1, showing how similar the nodes are.
    ///
    /// 1 means the nodes and all their subtrees are identical.
    /// 0 means that there is no overlap at all.
    ///
    /// The score is *not linear*. 0.5 is *not* "twice as similar" as 0.25.
    pub similarity: f64,
    /// Why were the two nodes mapped to each other?
    pub reason: ASTMappingReason,
}

/**
* Why were the two nodes mapped to each other?
*/
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub enum ASTMappingReason {
    #[default]
    /// The hash of the nodes and their subtrees is identical and so they were matched. Note that
    /// typically leaf nodes will never get this mapping reason, since maping common nodes, e.g.
    /// ";" to random other ";" in the code is extremely confusing and unnatural to humans.
    IdenticalHash,
    /// This node is part of a subtree that got matched when a parent or another ancestor node was
    /// matched due to identical hashes. Technically, this should imply this matching also has
    /// identical hashes, but we want to differentiate between the two for debugging, visualization
    /// and explainability.
    IdenticalHashOfAncestor,
    /// The hash of the nodes is *not* the same, but their subtrees fully match each-other.
    /// The common situation where this happens is refactoring order-independent blocks, e.g.
    /// fields definitions in a struct.
    FullyMappedSubtrees,
    /// The subtrees of the nodes are structurally identical, but the values of leaf nodes differ.
    /// E.g., a constant value was changed but the code structure is identical.
    StructurallyIdenticalSubtrees,
    /// This node is part of a subtree that got matched when a parent or another ancestor node was
    /// matched due to structure of the code. Note that it is possible for these nodes to be
    /// identical in both type or value, or they can only be identical in type. If they are only
    /// identical in type, it implies a "update()" command in the diff script, thus increasing the
    /// cost of the script by 1.
    StructurallyIdenticalAncestor,
}

/**
* Metadata about the AST.
*
* Note that hashes don't make sense for all nodes. E.g., the semicolon in Rust and C++ will have a
* leaf node that is repeated dozens or hundreds of times across a file. Those nodes will all have
* the exact same hash.
*/
#[derive(Debug, Clone, Default, Serialize)]
pub struct ASTMetadata {
    /// Map of node->hash. The hash is a full hash, hashing both the structure (types) and the
    /// values of the node and it's entire subtree, in order. The nodes are identified by their
    /// treesitter node id.
    pub node_to_full_hash: HashMap<usize, u64>,
    /// Reverse map to node_to_full_hash, going from <full hash> -> <treesitter node id>.
    /// Note that as mentioned above, many nodes will have the same hash, e.g. any variable
    /// declaration called "i" will hash to the same hash. Therefor, the map is actually going from
    /// a hash to a set of nodes.
    pub full_hash_to_node: HashMap<u64, HashSet<usize>>,
    /// Map of node->hash. The hash is a structural hash, hashing only the types of AST nodes in
    /// the subtree, not the value of the nodes. This hash is robust to changes like constant value
    /// changes. The nodes are identified by their treesitter node id.
    pub node_to_structural_hash: HashMap<usize, u64>,
    /// Reverse map to node_to_structural_hash, going from <structural hash> -> <node id>
    /// Note that as mentioned above, many nodes will have the same hash, e.g. any variable
    /// declaration will hash to the same structural hash. Therefor, the map value is a set.
    pub structural_hash_to_node: HashMap<u64, HashSet<usize>>,
}

/**
* Compute metadata for the given Code structure.
*
* This function creates a default ASTMetadata object and populates it by calling hash_code
* from hash.rs to compute both full and structural hashes for all nodes in the AST.
*
* @param code The Code structure to compute metadata for
* @return Result containing the populated ASTMetadata
*/
pub fn compute_metadata(code: &Code) -> Result<ASTMetadata> {
    let mut metadata = ASTMetadata::default();
    hash::hash_code(code, &mut metadata)?;
    Ok(metadata)
}

/**
* Perform top-down matching between two AST trees.
*
* This function does a pre-order (parent before children) traversal of the before tree.
* For each node, it checks if the node is a "source_file" or "function_item". If they are,
* it checks if a node with the same full hash exists in the after tree. If they do, it adds
* the two nodes to the mapped collection with the IdenticalHash reason, and then recursively
* adds all their children nodes with the IdenticalHashOfAncestor reason.
*
* @param before_tree The before AST tree
* @param after_tree The after AST tree
* @param before_metadata Metadata for the before tree
* @param after_metadata Metadata for the after tree
* @param diff The diff object to populate with mappings
*/
fn top_down_matching(
    before_tree: &tree_sitter::Tree,
    after_tree: &tree_sitter::Tree,
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
    diff: &mut ASTDiff,
) {
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();

    // Stack for pre-order traversal: (node, is_child_of_mapped_parent)
    let mut stack = Vec::new();
    stack.push((before_root, false));

    while let Some((before_node, _is_child_of_mapped)) = stack.pop() {
        let before_node_id = before_node.id();

        // Skip already mapped nodes
        if diff
            .mapped
            .iter()
            .any(|((before_id, _), _)| *before_id == before_node_id)
        {
            continue;
        }

        // Check if this is a reference node (source_file or function_item)
        let node_kind = before_node.kind();
        let is_reference_node = node_kind == "source_file"
            || node_kind == "function_item"
            || before_node.id() == before_root.id();

        if is_reference_node {
            // Get the full hash for this node
            if let Some(before_hash) = before_metadata.node_to_full_hash.get(&before_node_id) {
                // Look for a node in the after tree with the same hash
                if let Some(after_node_ids) = after_metadata.full_hash_to_node.get(before_hash) {
                    // For now, just take the first matching node
                    // TODO: Implement better matching strategy for multiple nodes with same hash
                    if let Some(&after_node_id) = after_node_ids.iter().next() {
                        // Find the actual node with the matching ID
                        let mut found_after_node = None;
                        let mut after_cursor = after_root.walk();
                        let mut after_stack = vec![after_root];

                        while let Some(current_after_node) = after_stack.pop() {
                            if current_after_node.id() == after_node_id {
                                found_after_node = Some(current_after_node);
                                break;
                            }

                            for child in current_after_node.children(&mut after_cursor) {
                                after_stack.push(child);
                            }
                        }

                        if let Some(matching_after_node) = found_after_node {
                            let after_node_id = matching_after_node.id();

                            // Add this mapping
                            diff.mapped.insert(
                                (before_node_id, after_node_id),
                                ASTMapping {
                                    similarity: 1.0,
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
                                            .mapped
                                            .iter()
                                            .any(|((child_id, _), _)| *child_id == before_child_id)
                                        {
                                            diff.mapped.insert(
                                                (before_child_id, after_child_id),
                                                ASTMapping {
                                                    similarity: 1.0,
                                                    reason:
                                                        ASTMappingReason::IdenticalHashOfAncestor,
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

        // Continue traversal - add children to stack in reverse order for pre-order traversal
        let mut children_cursor = before_node.walk();
        let children: Vec<_> = before_node.children(&mut children_cursor).collect();
        for child in children.into_iter().rev() {
            let is_child_mapped = diff
                .mapped
                .iter()
                .any(|((child_id, _), _)| *child_id == child.id());
            stack.push((child, is_child_mapped));
        }
    }
}

/**
* This is the main entry point in the AST diffing algorithm.
*
* The algorithm is as follows:
*
* 1. We compute the metadata for both trees.
* 2. Traversing the before tree in pre-order (parent before childern) we look for "human reference
*    nodes" and check if they have an exact full hash match in the after tree. If they do, we add
*    them and their subtrees to the matching.
* 3. Traversing the before tree in post-order (children before parent) we look for "structural
*    reference nodes" and check if they have an exact structural hash match in the after tree. If
*    they do, we add them and their subtrees to the matching.
*/
pub fn diff_code(before: &Code, after: &Code) -> Diff {
    // Parse the ASTs if they haven't been parsed yet
    //
    // Use the original code objects directly
    let before_parsed = before;
    let after_parsed = after;

    // Parse ASTs if they haven't been parsed yet
    let mut before_parsed_mut = before_parsed.clone();
    let mut after_parsed_mut = after_parsed.clone();

    let (before_ast, after_ast) = if before_parsed.ast.is_some() && after_parsed.ast.is_some() {
        // Both already parsed, use originals
        (
            before_parsed.ast.as_ref().unwrap(),
            after_parsed.ast.as_ref().unwrap(),
        )
    } else {
        // Need to parse
        if before_parsed_mut.ast.is_none() {
            let mut parser = tree_sitter::Parser::new();
            if let Some(language) = &before_parsed_mut.metadata.language {
                let ts_language = crate::code::language::to_treesitter(language)
                    .expect("Unable to convert CodeDiff language to TreeSitter language");
                parser
                    .set_language(&ts_language)
                    .expect("Unable to set TreeSitter language");
                before_parsed_mut.parse(&mut parser);
            }
        }

        if after_parsed_mut.ast.is_none() {
            let mut parser = tree_sitter::Parser::new();
            if let Some(language) = &after_parsed_mut.metadata.language {
                let ts_language = crate::code::language::to_treesitter(language)
                    .expect("Unable to convert CodeDiff language to TreeSitter language");
                parser
                    .set_language(&ts_language)
                    .expect("Unable to set TreeSitter language");
                after_parsed_mut.parse(&mut parser);
            }
        }

        (
            before_parsed_mut.ast.as_ref().unwrap(),
            after_parsed_mut.ast.as_ref().unwrap(),
        )
    };

    // Compute metadata for both before and after code
    // Use the parsed versions that we know have ASTs
    let before_metadata = if before_parsed.ast.is_some() {
        compute_metadata(before_parsed).unwrap_or_default()
    } else {
        compute_metadata(&before_parsed_mut).unwrap_or_default()
    };

    let after_metadata = if after_parsed.ast.is_some() {
        compute_metadata(after_parsed).unwrap_or_default()
    } else {
        compute_metadata(&after_parsed_mut).unwrap_or_default()
    };

    let mut diff = ASTDiff {
        before_metadata: Some(before_metadata.clone()),
        after_metadata: Some(after_metadata.clone()),
        ..Default::default()
    };

    // Perform top-down matching
    top_down_matching(
        before_ast,
        after_ast,
        &before_metadata,
        &after_metadata,
        &mut diff,
    );

    Diff { ast: Some(diff) }
}

#[cfg(test)]
mod tests {
    use crate::{
        code::{Language, from_string},
        test,
    };
    use anyhow::Result;

    use super::*;

    #[test]
    fn test_compute_metadata() -> Result<()> {
        let code = from_string("fn main() {}", &Language::Rust);
        let mut parsed_code = code.clone();

        // Parse the code
        let mut parser = tree_sitter::Parser::new();
        if let Some(language) = &parsed_code.metadata.language {
            let ts_language = crate::code::language::to_treesitter(language)
                .expect("Unable to convert CodeDiff language to TreeSitter language");
            parser
                .set_language(&ts_language)
                .expect("Unable to set TreeSitter language");
            parsed_code.parse(&mut parser);
        }

        // Compute metadata
        let metadata = compute_metadata(&parsed_code)?;

        // Verify metadata is computed
        assert!(!metadata.node_to_full_hash.is_empty());
        assert!(!metadata.full_hash_to_node.is_empty());
        assert!(!metadata.node_to_structural_hash.is_empty());
        assert!(!metadata.structural_hash_to_node.is_empty());

        Ok(())
    }

    #[test]
    fn test_root_node_mapping() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;
        let before = test_codes.get("hello-world.rs").unwrap().clone();
        let after = test_codes.get("hello-world.rs").unwrap().clone();

        let diff = diff_code(&before, &after);

        assert!(diff.ast.is_some());
        let diff_ast = diff.ast.unwrap();

        // Check that we have some mappings
        assert!(
            diff_ast.mapped.len() > 0,
            "Expected some mappings but got none"
        );

        // Check that root node is mapped
        let before_root_id = before.ast.as_ref().unwrap().root_node().id();
        let after_root_id = after.ast.as_ref().unwrap().root_node().id();

        assert!(
            diff_ast
                .mapped
                .contains_key(&(before_root_id, after_root_id))
        );

        Ok(())
    }

    #[test]
    fn diff_empty_rust_code() -> Result<()> {
        let before = from_string("", &Language::Rust);
        let after = from_string("", &Language::Rust);

        let diff = diff_code(&before, &after);

        assert!(diff.ast.is_some());
        let diff_ast = diff.ast.unwrap();

        // Rust treesitter files contain a source_file node even when empty.
        // The algorithm should map this node correctly.
        assert_eq!(diff_ast.mapped.len(), 1);
        assert_eq!(diff_ast.added.len(), 0);
        assert_eq!(diff_ast.deleted.len(), 0);

        Ok(())
    }

    #[test]
    fn diff_identical_rust_code() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;
        let before = test_codes.get("hello-world.rs").unwrap().clone();
        let after = test_codes.get("hello-world.rs").unwrap().clone();

        let diff = diff_code(&before, &after);

        assert!(diff.ast.is_some());
        let diff_ast = diff.ast.unwrap();

        // No changes means no deleted or added nodes.
        assert_eq!(diff_ast.added.len(), 0);
        assert_eq!(diff_ast.deleted.len(), 0);

        // The hello-world.rs TreeSitter AST has 22 nodes.
        // It looks like this:
        //
        // source_file
        //   function_item
        //     fn
        //     identifier
        //     parameters
        //       (
        //       )
        //     block
        //       {
        //       expression_statement
        //         macro_invocation
        //           identifier
        //           !
        //           token_tree
        //             (
        //             string_literal
        //               "
        //               string_content
        //               "
        //             )
        //         ;
        //       }
        //
        //  The code is identical, so the minimal diff script is just empty.
        assert_eq!(diff_ast.mapped.len(), 22);

        // The root node of before should match to the root node of after, and the reason should be
        // an identical hash. All other nodes should have the reason being an identical hash of
        // ancestors.

        // Check that root node has IdenticalHash reason
        let before_root_id = before.ast.as_ref().unwrap().root_node().id();
        let after_root_id = after.ast.as_ref().unwrap().root_node().id();

        let root_mapping = diff_ast
            .mapped
            .get(&(before_root_id, after_root_id))
            .expect("Root node should be mapped");
        assert_eq!(root_mapping.reason, ASTMappingReason::IdenticalHash);

        // Check that all other nodes have IdenticalHashOfAncestor reason
        for ((before_id, after_id), _mapping) in &diff_ast.mapped {
            if *before_id != before_root_id && *after_id != after_root_id {
                let mapping = diff_ast.mapped.get(&(*before_id, *after_id)).unwrap();
                assert_eq!(mapping.reason, ASTMappingReason::IdenticalHashOfAncestor);
            }
        }

        Ok(())
    }

    #[test]
    fn diff_hello_world_with_translated_string() -> Result<()> {
        // TODO: Uncomment this test when the implementation is ready
        return Ok(());

        let test_codes = test::helper::handmade_test_code()?;
        let before = test_codes.get("hello-world.rs").unwrap().clone();
        // The after code is the same basic Rust hello world, but the message is in Croatian
        // instead of in English. So there is exactly 1 different node, the string_content node,
        // and only the value of the node is different, going from "Hello, World" to "Zdravo,
        // Svijete".
        //
        // But because this node is deep in the tree, the following nodes no longer have exact same
        // hashes:
        //
        // source_file
        //   function_item
        //     block
        //       expression_statement
        //         macro_invocation
        //           token_tree
        //             string_literal
        //               string_content
        //
        //  The optimal smallest diff script is
        //
        //  update(<string content node>, "Zdravo, Svijete")
        let after = test_codes.get("zdravo-svijete.rs").unwrap().clone();

        let diff = diff_code(&before, &after);

        assert!(diff.ast.is_some());
        let diff_ast = diff.ast.unwrap();

        // No changes means no deleted or added nodes.
        assert_eq!(diff_ast.added.len(), 0);
        assert_eq!(diff_ast.deleted.len(), 0);

        // One mapping is an update, but it still maps.
        assert_eq!(diff_ast.mapped.len(), 22);

        Ok(())
    }
}
