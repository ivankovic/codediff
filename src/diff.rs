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
*/
#[derive(Debug, Clone, Default, Serialize)]
pub struct ASTDiff {
    /// Map of AST nodes from the before AST to the after AST.
    pub mapping: HashMap<(usize, usize), ASTMapping>,
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
    /// The hash of the nodes and their subtrees is identical.
    IdenticalHash,
    /// The hash of the nodes is *not* the same, but their subtrees fully match each-other.
    /// The common situation where this happens is refactoring order-independent blocks, e.g.
    /// fields definitions in a struct.
    FullyMappedSubtrees,
}

/**
* This is the main entry point in the AST diffing algorithm.
*/
pub fn diff_code(before: &Code, after: &Code) -> Diff {
    // Ensure both code objects have their ASTs parsed
    let mut before_parsed = before.clone();
    let mut after_parsed = after.clone();

    let mut parser = tree_sitter::Parser::new();

    // Parse before code if not already parsed
    if before_parsed.ast.is_none()
        && let Some(language) = &before_parsed.metadata.language
    {
        let ts_language = crate::code::language::to_treesitter(language)
            .expect("Unable to convert CodeDiff language to TreeSitter language");
        parser
            .set_language(&ts_language)
            .expect("Unable to set TreeSitter language");
        before_parsed.parse(&mut parser);
    }

    // Parse after code if not already parsed
    if after_parsed.ast.is_none()
        && let Some(language) = &after_parsed.metadata.language
    {
        let ts_language = crate::code::language::to_treesitter(language)
            .expect("Unable to convert CodeDiff language to TreeSitter language");
        parser
            .set_language(&ts_language)
            .expect("Unable to set TreeSitter language");
        after_parsed.parse(&mut parser);
    }

    // Compute the hash of both trees.
    let (before_node_to_hash, before_hash_to_node) =
        hash::hash_code(&before_parsed).expect("Failed to hash before code");
    let (after_node_to_hash, after_hash_to_node) =
        hash::hash_code(&after_parsed).expect("Failed to hash after code");

    let mut mapping = HashMap::new();
    let mut deleted = HashSet::new();
    let mut added = HashSet::new();

    // Top-to-bottom, and stopping before we reach parents of leaf nodes, match nodes that have
    // identical hashes.
    for (before_node_id, before_hash) in &before_node_to_hash {
        if let Some(&after_node_id) = after_hash_to_node.get(before_hash) {
            // Nodes have identical hashes, create mapping
            mapping.insert(
                (*before_node_id, after_node_id),
                ASTMapping {
                    similarity: 1.0, // Identical hash means 100% similarity
                    reason: ASTMappingReason::IdenticalHash,
                },
            );
        } else {
            // Node in before doesn't exist in after, mark as deleted
            deleted.insert(*before_node_id);
        }
    }

    // Find nodes that were added (exist in after but not in before)
    for (after_node_id, after_hash) in &after_node_to_hash {
        if !before_node_to_hash.contains_key(after_node_id)
            && !before_hash_to_node.contains_key(after_hash)
        {
            added.insert(*after_node_id);
        }
    }

    Diff {
        ast: Some(ASTDiff {
            mapping,
            deleted,
            added,
        }),
    }
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

        // Empty Rust code has a root source_file node that maps to itself
        // TreeSitter always creates a root node, even for empty files
        assert_eq!(diff_ast.mapping.len(), 1);
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
        assert_eq!(diff_ast.mapping.len(), 22);

        Ok(())
    }
}
