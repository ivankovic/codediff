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
    Diff {
        ast: Some(ASTDiff::default()),
    }
}

#[cfg(test)]
mod tests {
    use crate::code::{Language, from_string};
    use anyhow::Result;

    use super::*;

    #[test]
    fn diff_empty_rust_code() -> Result<()> {
        let before = from_string("", &Language::Rust);
        let after = from_string("", &Language::Rust);

        let diff = diff_code(&before, &after);

        assert!(diff.ast.is_some());
        let diff_ast = diff.ast.unwrap();

        assert_eq!(diff_ast.mapping.len(), 0);
        assert_eq!(diff_ast.added.len(), 0);
        assert_eq!(diff_ast.deleted.len(), 0);

        Ok(())
    }

    #[test]
    fn diff_identical_rust_code() -> Result<()> {
        let rust_code = r#"
fn main() {
    println!("Hello, world!");
}
"#;

        let before = from_string(rust_code, &Language::Rust);
        let after = from_string(rust_code, &Language::Rust);

        let diff = diff_code(&before, &after);

        assert!(diff.ast.is_some());
        let diff_ast = diff.ast.unwrap();

        // No changes means no deleted or added nodes.
        assert_eq!(diff_ast.added.len(), 0);
        assert_eq!(diff_ast.deleted.len(), 0);

        // There are exactly 22 nodes in the tree.
        assert_eq!(diff_ast.mapping.len(), 22);

        Ok(())
    }
}
