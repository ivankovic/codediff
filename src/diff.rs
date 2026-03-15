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
    let diff = ASTDiff {
        ..Default::default()
    };

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
