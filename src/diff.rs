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
pub mod reference_nodes;

use anyhow::Result;
use std::collections::{HashMap, HashSet};

use crate::code::{Code, Language};

pub const COST_INSERT: u64 = 1;
pub const COST_DELETE: u64 = 1;
pub const COST_UPDATE: u64 = 1;
pub const COST_MOVE: u64 = 0;

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
* See code.rs for the Code struct.
*/
#[derive(Debug, Clone, Default)]
pub struct Diff {
    /// Difference based on the ASTs.
    pub ast: Option<ASTDiff>,
}

/**
* Difference between two Code structures, based on their TreeSitter ASTs.
*/
#[derive(Debug, Clone, Default)]
pub struct ASTDiff {
    /// Map of AST nodes from the before AST to the after AST.
    pub mapping: HashMap<(usize, usize), ASTMapping>,
    /// Metadata about the before AST, including hashes for all nodes.
    pub before_metadata: Option<ASTMetadata>,
    /// Metadata about the after AST, including hashes for all nodes.
    pub after_metadata: Option<ASTMetadata>,
}

/**
* Information about the mapping of two AST subtrees.
*/
#[derive(Debug, Clone, Default)]
pub struct ASTMapping {
    /// The cost of the mapping.
    ///
    /// The cost is recursively defined the cost to match the root nodes plus the total cost to match
    /// the subtrees of the root nodes. The cost to match the root nodes corresponds to the
    /// operation required to match the nodes in the diff script: a delete, insert, update or move.
    ///
    /// The cost strategy is dynamic, but a common cost strategy is unit cost (1) for delete,
    /// insert and update and a free (0 cost) move.
    pub cost: u64,
    /// What operation has to be done to make this mapping valid?
    pub operation: ASTMappingOperation,
    /// Why were the two subtrees mapping together?
    pub reason: ASTMappingReason,
}

/**
* The operations that can be used to transform one tree into another.
*/
#[derive(Debug, Clone, Default, PartialEq)]
pub enum ASTMappingOperation {
    #[default]
    /// No operation is needed. The match is perfect.
    Identical,
    /// The node and it's entire subtree is moved to a different parent node.
    Move,
    /// The node's value is updated.
    Update,
    /// The node is inserted between a parent node and a consecutive subsequence of the parent
    /// node's children. Note that the subsequence can be empty.
    Insert,
    /// The node is deleted and it's children, if any, are connected to it's parent node. If the
    /// root node is deleted, in theory the children form a forrest of trees instead. This only
    /// happens theorethically during some algorithm computations, since a diff script that deletes
    /// the root node would be guaranteed to create invalid code, unless the code is already empty.
    Delete,
}

/**
* Why were the two subtrees mapped to each other?
*/
#[derive(Debug, Clone, Default, PartialEq)]
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
    FullymappingSubtrees,
    /// The subtrees of the nodes are structurally identical, but the values of leaf nodes differ.
    /// E.g., a constant value was changed but the code structure is identical.
    StructurallyIdenticalSubtrees,
    /// This node is part of a subtree that got matched when a parent or another ancestor node was
    /// matched due to structure of the code. Note that it is possible for these nodes to be
    /// identical in both type or value, or they can only be identical in type. If they are only
    /// identical in type, it implies a "update()" command in the diff script, thus increasing the
    /// cost of the script by 1.
    StructurallyIdenticalAncestor,
    /// Using RTED it was determined that this is the optimal mapping.
    RTED,
}

/**
* Metadata about the AST.
*
* Note that hashes don't make sense for all nodes. E.g., the semicolon in Rust and C++ will have a
* leaf node that is repeated dozens or hundreds of times across a file. Those nodes will all have
* the exact same hash.
*/
#[derive(Debug, Clone, Default)]
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
    /// Set of reference nodes in this tree, ordered by subtree size.
    pub reference_nodes_ordered: Vec<usize>,
}

/**
* Compute metadata for the given Code structure.
*
* This function creates a default ASTMetadata object and populates it by calling hash_code
* from hash.rs to compute both full and structural hashes for all nodes in the AST.
* It also discovers all reference nodes and orders them by subtree size.
*/
pub fn compute_metadata(code: &Code) -> Result<ASTMetadata> {
    let mut metadata = ASTMetadata::default();
    hash::hash_code(code, &mut metadata)?;
    // Discover all reference nodes and order them by subtree size
    discover_reference_nodes(code, &mut metadata)?;
    Ok(metadata)
}

/**
* Discover all reference nodes in the AST and order them by subtree size.
*
* This function traverses the AST to find all nodes that are considered reference nodes
* (as defined by is_reference_node), calculates their subtree sizes, and stores them
* in the metadata.reference_nodes_ordered vector, sorted by size in descending order.
*/
fn discover_reference_nodes(code: &Code, metadata: &mut ASTMetadata) -> Result<()> {
    let ast = code.ast.as_ref().expect("AST must be parsed");
    let root_node = ast.root_node();
    let language = code
        .metadata
        .language
        .as_ref()
        .expect("Language must be set");

    let mut cursor = root_node.walk();
    let mut stack = Vec::new();
    stack.push(root_node);

    let mut reference_nodes_with_sizes = Vec::new();
    while let Some(node) = stack.pop() {
        let node_id = node.id();

        if reference_nodes::is_reference_node(node.kind(), language) {
            let subtree_size = count_subtree_nodes(&node);
            reference_nodes_with_sizes.push((node_id, subtree_size));
        }

        // Continue traversal - add children to stack
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }

    // Sort reference nodes by subtree size in descending order
    reference_nodes_with_sizes.sort_by(|a, b| b.1.cmp(&a.1));

    // Extract just the node IDs in order
    metadata.reference_nodes_ordered = reference_nodes_with_sizes
        .into_iter()
        .map(|(node_id, _)| node_id)
        .collect();

    Ok(())
}

/**
* Count the number of nodes in a subtree rooted at the given node.
*
* This function performs a depth-first traversal to count all nodes
* in the subtree, including the root node itself.
*/
fn count_subtree_nodes(node: &tree_sitter::Node) -> usize {
    let mut cursor = node.walk();
    let mut stack = Vec::new();
    stack.push(*node);
    let mut count = 0;

    while let Some(current_node) = stack.pop() {
        count += 1;

        // Add children to stack for processing
        for child in current_node.children(&mut cursor) {
            stack.push(child);
        }
    }

    count
}

/**
* Perform top-down matching between two AST trees.
*
* This function does a pre-order (parent before children) traversal of the before tree.
* For each node, it checks if the node is a "source_file" or "function_item". If they are,
* it checks if a node with the same full hash exists in the after tree. If they do, it adds
* the two nodes to the mapping collection with the IdenticalHash reason, and then recursively
* adds all their children nodes with the IdenticalHashOfAncestor reason.
*/
fn top_down_matching(before: &Code, after: &Code, language: &Language, diff: &mut ASTDiff) {
    let before_tree = before.ast.as_ref().expect("Before code must be parsed");
    let after_tree = after.ast.as_ref().expect("After code must be parsed");
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();

    let before_metadata = diff
        .before_metadata
        .as_ref()
        .expect("Before metadata must be computed");
    let after_metadata = diff
        .after_metadata
        .as_ref()
        .expect("After metadata must be computed");

    // Stack for pre-order traversal: (node, is_child_of_mapping_parent)
    let mut stack = Vec::new();
    stack.push((before_root, false));

    while let Some((before_node, _is_child_of_mapping)) = stack.pop() {
        let before_node_id = before_node.id();

        // Skip already mapping nodes
        if diff
            .mapping
            .iter()
            .any(|((before_id, _), _)| *before_id == before_node_id)
        {
            continue;
        }

        // Check if this is a reference node (source_file or function_item)
        if reference_nodes::is_reference_node(before_node.kind(), language)
            && let Some(before_hash) = before_metadata.node_to_full_hash.get(&before_node_id)
        {
            // Look for a node in the after tree with the same hash
            if let Some(after_node_ids) = after_metadata.full_hash_to_node.get(before_hash) {
                // If we have multiple reference nodes with exactly the same full hash, then
                // there is simply duplicated code in the file, and quite big chunks of it too.
                // For now, we take the first node and match to that.
                //
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
                        diff.mapping.insert(
                            (before_node_id, after_node_id),
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
                                        diff.mapping.insert(
                                            (before_child_id, after_child_id),
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

        // Continue traversal - add children to stack in reverse order for pre-order traversal
        let mut children_cursor = before_node.walk();
        let children: Vec<_> = before_node.children(&mut children_cursor).collect();
        for child in children.into_iter().rev() {
            let is_child_mapping = diff
                .mapping
                .iter()
                .any(|((child_id, _), _)| *child_id == child.id());
            stack.push((child, is_child_mapping));
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
    // Compute metadata for both before and after code
    // Use the original code objects that we know have ASTs (checked above)
    let before_metadata = compute_metadata(before).unwrap_or_default();
    let after_metadata = compute_metadata(after).unwrap_or_default();

    let mut diff = ASTDiff {
        before_metadata: Some(before_metadata.clone()),
        after_metadata: Some(after_metadata.clone()),
        ..Default::default()
    };

    // Perform top-down matching
    let language = before
        .metadata
        .language
        .as_ref()
        .expect("Language must be set");

    top_down_matching(before, after, language, &mut diff);

    Diff { ast: Some(diff) }
}

#[cfg(test)]
mod tests {
    use crate::{
        code::{Code, Language},
        test,
    };
    use anyhow::Result;

    use super::*;

    #[test]
    fn test_compute_metadata() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;
        let code = test_codes.get("hello-world.rs").unwrap().clone();

        // Compute metadata
        let metadata = compute_metadata(&code)?;

        // Verify metadata is computed
        assert!(!metadata.node_to_full_hash.is_empty());
        assert!(!metadata.full_hash_to_node.is_empty());
        assert!(!metadata.node_to_structural_hash.is_empty());
        assert!(!metadata.structural_hash_to_node.is_empty());
        assert!(!metadata.reference_nodes_ordered.is_empty());

        // The first, largest reference node must always be the root.
        let root_id = code.ast.as_ref().unwrap().root_node().id();

        assert_eq!(metadata.reference_nodes_ordered[0], root_id);
        assert_eq!(metadata.reference_nodes_ordered.len(), 2);

        Ok(())
    }

    #[test]
    fn diff_empty_rust_code() -> Result<()> {
        let before = Code::from_string("", &Language::Rust);
        let after = Code::from_string("", &Language::Rust);

        let diff = diff_code(&before, &after);

        assert!(diff.ast.is_some());
        let diff_ast = diff.ast.unwrap();

        // Rust treesitter files contain a source_file node even when empty.
        // The algorithm should map this node correctly.
        assert_eq!(diff_ast.mapping.len(), 1);

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
        assert_eq!(diff_ast.mapping.len(), 22);

        // The root node of before should match to the root node of after, and the reason should be
        // an identical hash. All other nodes should have the reason being an identical hash of
        // ancestors.

        // Check that root node has IdenticalHash reason
        let before_root_id = before.ast.as_ref().unwrap().root_node().id();
        let after_root_id = after.ast.as_ref().unwrap().root_node().id();

        let root_mapping = diff_ast
            .mapping
            .get(&(before_root_id, after_root_id))
            .expect("Root node should be mapping");
        assert_eq!(root_mapping.reason, ASTMappingReason::IdenticalHash);

        // A fully identical code can never have a cost.
        assert_eq!(root_mapping.cost, 0);

        // Check that all other nodes have IdenticalHashOfAncestor reason
        for ((before_id, after_id), mapping) in &diff_ast.mapping {
            if *before_id != before_root_id && *after_id != after_root_id {
                assert_eq!(mapping.reason, ASTMappingReason::IdenticalHashOfAncestor);
            }
        }

        Ok(())
    }

    #[test]
    fn identical_code_must_always_match() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;

        for (filename, code) in &test_codes {
            let diff = diff_code(code, code);

            assert!(
                diff.ast.is_some(),
                "AST diff should be computed for {}",
                filename
            );
            let diff_ast = diff.ast.unwrap();

            let before_root_id = code.ast.as_ref().unwrap().root_node().id();
            let after_root_id = code.ast.as_ref().unwrap().root_node().id();

            // Check that root node has IdenticalHash reason
            let root_mapping = diff_ast
                .mapping
                .get(&(before_root_id, after_root_id))
                .expect("Root node should be mapping");
            assert_eq!(
                root_mapping.reason,
                ASTMappingReason::IdenticalHash,
                "Root node should have IdenticalHash reason for {}",
                filename
            );

            // A fully identical code can never have a cost.
            assert_eq!(root_mapping.cost, 0);

            // Check that all other nodes have IdenticalHashOfAncestor reason
            for ((before_id, after_id), mapping) in &diff_ast.mapping {
                if *before_id != before_root_id || *after_id != after_root_id {
                    assert_eq!(
                        mapping.reason,
                        ASTMappingReason::IdenticalHashOfAncestor,
                        "Non-root node should have IdenticalHashOfAncestor reason for {}, got {:?}",
                        filename,
                        mapping.reason
                    );
                }
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

        // One mapping is an update, but it still maps.
        assert_eq!(diff_ast.mapping.len(), 22);

        Ok(())
    }
}
