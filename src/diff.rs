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
pub mod optimal_iud;
pub mod reference_nodes;
pub mod solve_identical_trees;
pub mod solve_structurally_identical_trees;
pub mod text;
pub mod text_range;

use std::collections::HashMap;

use tree_sitter::Node;

use crate::code::{Code, Language};
use crate::diff::text::TextDiff;

/// A structure that holds node caches for both before and after Code objects.
#[derive(Debug, Clone, Default)]
pub struct NodeCache {
    /// Cache of nodes from the before Code object, keyed by node ID.
    pub before: HashMap<usize, tree_sitter::Node<'static>>,
    /// Cache of nodes from the after Code object, keyed by node ID.
    pub after: HashMap<usize, tree_sitter::Node<'static>>,
}

impl NodeCache {
    /// Build node caches for both Code objects.
    /// This function will always build the caches - it assumes ASTs are parsed.
    pub fn build(before: &Code, after: &Code) -> Self {
        let before_cache = before
            .ast
            .as_ref()
            .map(|ast| {
                let root_node = ast.root_node();
                let mut cache = HashMap::new();
                let mut stack = vec![root_node];

                while let Some(node) = stack.pop() {
                    // Cache this node
                    cache.insert(node.id(), unsafe { std::mem::transmute(node) });

                    // Add children to stack for traversal
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        stack.push(child);
                    }
                }

                cache
            })
            .unwrap_or_default();

        let after_cache = after
            .ast
            .as_ref()
            .map(|ast| {
                let root_node = ast.root_node();
                let mut cache = HashMap::new();
                let mut stack = vec![root_node];

                while let Some(node) = stack.pop() {
                    // Cache this node
                    cache.insert(node.id(), unsafe { std::mem::transmute(node) });

                    // Add children to stack for traversal
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        stack.push(child);
                    }
                }

                cache
            })
            .unwrap_or_default();

        NodeCache {
            before: before_cache,
            after: after_cache,
        }
    }

    pub fn get_in_any(&self, node_id: &usize) -> Option<&Node<'_>> {
        if self.before.contains_key(node_id) {
            return self.before.get(node_id);
        }
        if self.after.contains_key(node_id) {
            return self.after.get(node_id);
        }
        None
    }
}

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
#[derive(Debug, Clone)]
pub struct Diff {
    /// Difference based on the ASTs.
    pub ast: Option<ASTDiff>,
    /// The language used for this diff.
    pub language: Language,
    /// The difference, provided as a data structure of TreeSitter points.
    /// Very useful when the code is viewed as text, for example in editors.
    pub text: Option<TextDiff>,
}

impl Default for Diff {
    fn default() -> Self {
        Self {
            ast: None,
            language: Language::Unknown,
            text: None,
        }
    }
}

impl Diff {
    /**
     * Creates a new Diff from two Code objects.
     *
     * This is the main entry point in the AST diffing algorithm.
     * The algorithm is encoded in the code and is intentionally not explained in the Doccomment to
     * avoid it going stale. Please see the code.
     */
    pub fn from_code(before: &Code, after: &Code) -> Self {
        // Build node cache for efficient lookup
        let node_cache = NodeCache::build(before, after);

        // Compute metadata fresh for the diff algorithm
        let mut ast_diff = ASTDiff {
            ..Default::default()
        };

        // These are highly efficient for small diffs, and small diffs are half of all
        // diffs.
        solve_identical_trees::solve(before, after, &node_cache, &mut ast_diff);
        solve_structurally_identical_trees::solve(before, after, &node_cache, &mut ast_diff);

        // This is the final, extremely slow algorithm.
        // It finds the optimal solution, but only for Insert/Update/Delete operations. The
        // more nodes are already matched, the faster and better this step is.
        let _ = optimal_iud::find(before, after, &node_cache, &mut ast_diff);

        Self {
            ast: Some(ast_diff),
            language: before
                .metadata
                .language
                .clone()
                .unwrap_or(Language::Unknown),
            text: None,
        }
    }
}

/**
* Difference between two Code structures, based on their TreeSitter ASTs.
*/
#[derive(Debug, Clone, Default)]
pub struct ASTDiff {
    /// Map of AST nodes from the before AST to the after AST.
    pub mapping: HashMap<(usize, usize), ASTMapping>,
    /// Map of nodes from the before tree to the after tree, or 0 if it is a delete.
    /// Useful if you are walking the before tree and need to look up the mapping.
    pub before_node_map: HashMap<usize, usize>,
    /// Map of nodes from the after tree to the before tree, or 0 if it is an insert.
    /// Useful if you are walking the after tree and need to look up the mapping.
    pub after_node_map: HashMap<usize, usize>,
}

impl ASTDiff {
    /**
     * Add a mapping between two nodes to the diff.
     *
     * Note that either id can be zero. Zero is used for insert or delete.
     */
    pub fn add_mapping(&mut self, before_id: usize, after_id: usize, mapping: ASTMapping) {
        self.mapping.insert((before_id, after_id), mapping);
        self.before_node_map.insert(before_id, after_id);
        self.after_node_map.insert(after_id, before_id);
    }

    /**
     * Checks that the mapping is valid for given trees.
     *
     * Useful in tests.
     */
    pub fn is_valid(&self, _before: &Code, _after: &Code, node_cache: &NodeCache) -> bool {
        // Check that each mapping only maps nodes of the same type
        for (before_id, after_id) in self.mapping.keys() {
            if *before_id == 0 || *after_id == 0 {
                // Null-mapping is an Insert/Delete.
                continue;
            }

            // Get nodes from cache - if not found, mapping is invalid
            let before_node = node_cache.before.get(before_id);
            let after_node = node_cache.after.get(after_id);

            // If we can't get either node, the mapping is invalid
            if before_node.is_none() || after_node.is_none() {
                return false;
            }

            // Check that the node types match
            let before_node = before_node.unwrap();
            let after_node = after_node.unwrap();
            if before_node.kind() != after_node.kind() {
                return false;
            }
        }

        true
    }

    /**
     * Checks that the mapping covers all nodes in both trees.
     *
     * Useful in tests.
     */
    pub fn is_complete(&self, before: &Code, after: &Code, node_cache: &NodeCache) -> bool {
        // Create sets of all nodes seen in mappings
        let mut seen_before_nodes = HashMap::new();
        let mut seen_after_nodes = HashMap::new();

        for ((before_id, after_id), mapping) in &self.mapping {
            seen_before_nodes.insert(before_id, mapping);
            seen_after_nodes.insert(after_id, mapping);
        }

        // Check that all nodes in before tree are covered
        for node_id in node_cache.before.keys() {
            if !seen_before_nodes.contains_key(node_id) {
                // Check if this is a root node that might not need mapping
                if node_id == &before.ast.as_ref().unwrap().root_node().id() {
                    continue;
                }
                return false;
            }
        }

        // Check that all nodes in after tree are covered
        for node_id in node_cache.after.keys() {
            if !seen_after_nodes.contains_key(node_id) {
                // Check if this is a root node that might not need mapping
                if node_id == &after.ast.as_ref().unwrap().root_node().id() {
                    continue;
                }
                return false;
            }
        }

        true
    }

    /**
     * Returns true if the node is mapped in any subtree.
     */
    pub fn is_node_mapped(&self, node_id: &usize) -> bool {
        if self.before_node_map.contains_key(node_id) || self.after_node_map.contains_key(node_id) {
            return true;
        }
        false
    }
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
    /// What operation has to be done to the root nodes to make this mapping valid?
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
    /// Sentinel value.
    NotYetSet,
    /// A meta-operation that only makes sense if the diff is partially solved by other means.
    DoNothing,
    /// No operation is needed. The match is perfect.
    Identical,
    /// The node and it's entire subtree is moved to a different parent node.
    Move,
    /// The node's value is updated.
    Update,
    /// The node is inserted between a parent node and a consecutive subsequence of the parent
    /// node's children. Note that the subsequence can be empty.
    Insert,
    /// The node and all it's children are inserted. This is a special operation that makes the
    /// algorithm more efficient but also uglier to implement. It results in much shorter edit
    /// scripts and shallower recursion depth, but it changes the domain of operations from "one
    /// node" to "subtrees".
    InsertWithChildren,
    /// The node is deleted and it's children, if any, are connected to it's parent node. If the
    /// root node is deleted, in theory the children form a forrest of trees instead. This only
    /// happens theorethically during some algorithm computations, since a diff script that deletes
    /// the root node would be guaranteed to create invalid code, unless the code is already empty.
    Delete,
    /// The node and all it's children are deleted. Same as InsertWithChildren, this is a more
    /// complex operation that results in more efficient code.
    DeleteWithChildren,
    /// The node maps to a different node, but not all of their children are identical.
    MatchButNotIdentical,
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
    /// Using highly modified edit distance algorithm it was determined that this is the optimal
    /// mapping if only Insert, Delete, Update and Identical operations are allowed.
    OptimalIDU,
}

/**
* Creates a Diff from two Code objects.
*
* This is a convenience wrapper around Diff::from_code for backwards compatibility.
* The algorithm is encoded in the code and is intentionally not explained in the Doccomment to
* avoid it going stale. Please see the code.
*/
pub fn diff_code(before: &Code, after: &Code) -> Diff {
    Diff::from_code(before, after)
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
        let metadata = crate::code::metadata::compute_ast_metadata(&code)?;

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
    fn diff_hello_world_with_translated_string() -> Result<()> {
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

        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let before_string_node = test::helper::node_for_path(
            before_ast.root_node(),
            &[
                "function_item",
                "block",
                "expression_statement",
                "macro_invocation",
                "token_tree",
                "string_literal",
                "string_content",
            ],
        )?;
        let after_string_node = test::helper::node_for_path(
            after_ast.root_node(),
            &[
                "function_item",
                "block",
                "expression_statement",
                "macro_invocation",
                "token_tree",
                "string_literal",
                "string_content",
            ],
        )?;

        // Verify that these nodes are mapped in the diff
        let before_node_id = before_string_node.id();
        let after_node_id = after_string_node.id();

        let mapping = diff_ast.mapping.get(&(before_node_id, after_node_id));
        assert!(mapping.is_some(), "String content nodes should be mapped");

        let mapping = mapping.unwrap();

        // The mapping should be an Update operation since the content changed
        assert_eq!(
            mapping.operation,
            ASTMappingOperation::Update,
            "String content mapping should be an Update operation"
        );

        // The reason should be StructurallyIdenticalAncestor since this node is part of a
        // structurally identical subtree (the parent nodes match structurally)
        assert_eq!(
            mapping.reason,
            ASTMappingReason::StructurallyIdenticalAncestor,
            "String content mapping reason should be StructurallyIdenticalAncestor"
        );

        // The cost should be COST_UPDATE (1)
        assert_eq!(
            mapping.cost, COST_UPDATE,
            "String content mapping cost should be COST_UPDATE"
        );

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

            let node_cache = NodeCache::build(code, code);
            assert!(
                diff_ast.is_valid(code, code, &node_cache),
                "Identical code must always produce a valid diff: {}",
                filename
            );
            assert!(
                diff_ast.is_complete(code, code, &node_cache),
                "Identical code must always produce a complete diff: {}",
                filename
            );

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
    fn hello_world_translations_in_all_langauges() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;

        for (filename, before) in &test_codes {
            if !filename.starts_with("hello-world") {
                continue;
            }
            let after = test_codes
                .get(&filename.replace("hello-world", "zdravo-svijete"))
                .unwrap()
                .clone();

            let diff = diff_code(before, &after);

            assert!(
                diff.ast.is_some(),
                "AST diff should be computed for {}",
                filename
            );
            let diff_ast = diff.ast.unwrap();

            let before_root_id = before.ast.as_ref().unwrap().root_node().id();
            let after_root_id = after.ast.as_ref().unwrap().root_node().id();

            // Check that root node has IdenticalHash reason
            let root_mapping = diff_ast
                .mapping
                .get(&(before_root_id, after_root_id))
                .expect("Root node should be mapping");
            assert_eq!(
                root_mapping.reason,
                ASTMappingReason::StructurallyIdenticalSubtrees,
                "Root node should have StructurallyIdenticalSubtrees reason for {}",
                filename
            );

            // Cost should always be exactly 1, since in all languages it is a simple string
            // constant update.
            assert_eq!(root_mapping.cost, 1);

            // Check that all other nodes have IdenticalHashOfAncestor reason
            for ((before_id, after_id), mapping) in &diff_ast.mapping {
                if *before_id != before_root_id || *after_id != after_root_id {
                    assert_eq!(
                        mapping.reason,
                        ASTMappingReason::StructurallyIdenticalAncestor,
                        "Non-root node should have StructurallyIdenticalAncestor reason for {}, got {:?}",
                        filename,
                        mapping.reason
                    );
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_is_valid_with_identical_code() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;
        let before = test_codes.get("hello-world.rs").unwrap().clone();
        let after = test_codes.get("hello-world.rs").unwrap().clone();

        let diff = diff_code(&before, &after);
        let diff_ast = diff.ast.unwrap();

        // The mapping should be valid for identical code
        let node_cache = NodeCache::build(&before, &after);
        assert!(diff_ast.is_valid(&before, &after, &node_cache));

        Ok(())
    }

    #[test]
    fn test_is_valid_with_different_code() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;
        let before = test_codes.get("hello-world.rs").unwrap().clone();
        let after = test_codes.get("zdravo-svijete.rs").unwrap().clone();

        let diff = diff_code(&before, &after);
        let diff_ast = diff.ast.unwrap();

        // The mapping should still be valid for different code as long as node types match
        let node_cache = NodeCache::build(&before, &after);
        assert!(diff_ast.is_valid(&before, &after, &node_cache));

        Ok(())
    }

    #[test]
    fn test_is_valid_with_invalid_mapping() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;
        let before = test_codes.get("hello-world.rs").unwrap().clone();
        let after = test_codes.get("hello-world.rs").unwrap().clone();

        let mut diff = diff_code(&before, &after);
        let diff_ast = diff.ast.as_mut().unwrap();

        // Create an invalid mapping by mapping nodes of different types
        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();

        let before_root = before_ast.root_node();
        let after_root = after_ast.root_node();

        // Find nodes of different types by traversing the tree
        let mut before_cursor = before_root.walk();
        let mut after_cursor = after_root.walk();

        // Get the function_item node (first child of root)
        let before_function_item = before_root.children(&mut before_cursor).next().unwrap();
        let after_function_item = after_root.children(&mut after_cursor).next().unwrap();

        // Now get different types of nodes - function_item vs some leaf node
        let mut before_leaf_cursor = before_function_item.walk();
        let mut after_leaf_cursor = after_function_item.walk();

        // Find a leaf node (like identifier or string_literal)
        let before_leaf = before_function_item
            .children(&mut before_leaf_cursor)
            .find(|child| child.kind() == "identifier")
            .unwrap();
        let after_leaf = after_function_item
            .children(&mut after_leaf_cursor)
            .find(|child| child.kind() == "block")
            .unwrap();

        // Create an invalid mapping by mapping different types
        let invalid_before_id = before_leaf.id();
        let invalid_after_id = after_leaf.id();

        // Clear existing mapping and add invalid one
        diff_ast.mapping.clear();
        diff_ast.mapping.insert(
            (invalid_before_id, invalid_after_id),
            ASTMapping {
                cost: 0,
                operation: ASTMappingOperation::Identical,
                reason: ASTMappingReason::IdenticalHash,
            },
        );

        // The mapping should be invalid
        let node_cache = NodeCache::build(&before, &after);
        assert!(
            !diff_ast.is_valid(&before, &after, &node_cache),
            "Mapping should be invalid for different node types: {} vs {}",
            before_leaf.kind(),
            after_leaf.kind()
        );

        Ok(())
    }

    #[test]
    fn test_is_valid_with_real_diff() -> Result<()> {
        let test_diffs = test::helper::handmade_test_code_pairs()?;

        // Test that all real diffs are valid
        for (diff_name, (before, after)) in test_diffs {
            let diff = Diff::from_code(&before, &after);
            let diff_ast = diff.ast.unwrap();

            // The mapping from a real diff should always be valid
            let node_cache = NodeCache::build(&before, &after);
            assert!(
                diff_ast.is_valid(&before, &after, &node_cache),
                "Real diff mappings should always be valid for diff: {}",
                diff_name
            );
        }

        Ok(())
    }

    #[test]
    fn test_is_valid_with_null_mapping() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;
        let before = test_codes.get("hello-world.rs").unwrap().clone();
        let after = test_codes.get("hello-world.rs").unwrap().clone();

        let mut diff = diff_code(&before, &after);
        let diff_ast = diff.ast.as_mut().unwrap();

        // Clear existing mapping and add a null mapping (ID 0 represents insert/delete)
        diff_ast.mapping.clear();
        diff_ast.add_mapping(
            0,
            123, // 0 represents a null node (insert)
            ASTMapping {
                cost: COST_INSERT,
                operation: ASTMappingOperation::Insert,
                reason: ASTMappingReason::OptimalIDU,
            },
        );
        diff_ast.add_mapping(
            456,
            0, // 0 represents a null node (delete)
            ASTMapping {
                cost: COST_DELETE,
                operation: ASTMappingOperation::Delete,
                reason: ASTMappingReason::OptimalIDU,
            },
        );

        // Null mappings should be considered valid
        let node_cache = NodeCache::build(&before, &after);
        assert!(
            diff_ast.is_valid(&before, &after, &node_cache),
            "Null mappings (insert/delete) should be valid"
        );

        Ok(())
    }

    #[test]
    fn test_add_mapping_updates_all_maps() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;
        let before = test_codes.get("hello-world.rs").unwrap().clone();
        let after = test_codes.get("hello-world.rs").unwrap().clone();

        let mut diff = ASTDiff {
            ..Default::default()
        };

        let before_root_id = before.ast.as_ref().unwrap().root_node().id();
        let after_root_id = after.ast.as_ref().unwrap().root_node().id();

        // Add a mapping using the new method
        diff.add_mapping(
            before_root_id,
            after_root_id,
            ASTMapping {
                cost: 0,
                operation: ASTMappingOperation::Identical,
                reason: ASTMappingReason::IdenticalHash,
            },
        );

        // Verify that all maps were updated correctly
        assert_eq!(diff.mapping.len(), 1);
        assert_eq!(diff.before_node_map.len(), 1);
        assert_eq!(diff.after_node_map.len(), 1);

        // Check that the main mapping contains the correct entry
        assert!(diff.mapping.contains_key(&(before_root_id, after_root_id)));

        // Check that before_node_map contains the correct mapping
        assert_eq!(
            diff.before_node_map.get(&before_root_id),
            Some(&after_root_id)
        );

        // Check that after_node_map contains the correct mapping
        assert_eq!(
            diff.after_node_map.get(&after_root_id),
            Some(&before_root_id)
        );

        Ok(())
    }

    #[test]
    fn test_diff_populates_all_maps() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;
        let before = test_codes.get("hello-world.rs").unwrap().clone();
        let after = test_codes.get("hello-world.rs").unwrap().clone();

        let diff = diff_code(&before, &after);
        let diff_ast = diff.ast.unwrap();

        // All maps should have the same number of entries
        assert_eq!(diff_ast.mapping.len(), diff_ast.before_node_map.len());
        assert_eq!(diff_ast.mapping.len(), diff_ast.after_node_map.len());

        // For identical code, we should have a complete mapping
        assert!(!diff_ast.mapping.is_empty());

        // Verify that the maps are consistent
        for (before_id, after_id) in diff_ast.mapping.keys() {
            assert_eq!(diff_ast.before_node_map.get(before_id), Some(after_id));
            assert_eq!(diff_ast.after_node_map.get(after_id), Some(before_id));
        }

        Ok(())
    }
}
