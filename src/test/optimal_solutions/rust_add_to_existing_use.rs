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
use crate::diff;
use crate::diff::ASTMappingOperation;
use crate::test;

use anyhow::{Ok, Result};

#[test]
fn optimal_solution() -> Result<()> {
    let test_diffs = test::helper::handmade_test_code_pairs()?;
    let (before, after) = test_diffs.get("rust-add-to-existing-use").unwrap().clone();

    let diff = diff::diff_code(&before, &after);

    assert!(diff.ast.is_some());

    let diff_ast = diff.ast.unwrap();
    let before_ast = before.ast.unwrap();
    let after_ast = after.ast.unwrap();

    let before_root = before_ast.root_node();
    let after_root = after_ast.root_node();

    // The code changes from this:
    //
    // use std::fmt::Display;
    //
    // to this:
    //
    // use std::fmt::{
    //   Display, Debug
    // };
    //
    // The interesting part of the tree changes from this:
    //
    // use_declaration
    //      use
    //      scoped_identifier
    //          scoped_identifier
    //              identifier
    //              ::
    //              identifier
    //          ::
    //          identifier
    //      ;
    //
    // to this:
    //
    // use_declaration
    //      use
    //      scoped_use_list
    //          scoped_identifier
    //              identifier
    //              ::
    //              identifier
    //          ::
    //          use_list
    //              {
    //              identifier
    //              ,
    //              identifier
    //              }
    //      ;
    //
    // For this test case, it is important to note that the operation that changes one node _kind_
    // into another is NOT available. This means the optimal solution is:
    //
    // 1. Add a "scoped_use_list" node to the use_declaration at position 2 or 3.
    // 2. Move the entire subtree of the "use_declaration -> scoped_identifier" under the
    //    scoped_use_list.
    // 3. Delete the now leaf "use_declaration -> scoped_identifier".
    // 4. Insert a "use_list" node as the last child of the scoped_use_list, with the currently last
    //    child, the identifier node, as it's child.
    // 5. Insert '{'
    // 6. Insert ','
    // 7. Insert identifier
    // 8. Insert '}'

    let path = ["use_declaration"];
    let mapping = test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(
        mapping.operation,
        ASTMappingOperation::MatchButNotIdentical,
        "The use declaration as a whole is not correctly mapped"
    );

    test::helper::was_node_added(
        &["use_declaration", "scoped_use_list"],
        after_root,
        &diff_ast,
    )?;
    test::helper::was_node_deleted(
        &["use_declaration", "scoped_identifier"],
        before_root,
        &diff_ast,
    )?;

    let mapping = test::helper::mapping_for_path(
        &["use_declaration", "scoped_identifier", "scoped_identifier"],
        &["use_declaration", "scoped_use_list", "scoped_identifier"],
        before_root,
        after_root,
        &diff_ast,
    )?;
    assert_eq!(
        mapping.operation,
        ASTMappingOperation::Identical,
        "The (2nd) scoped_identifier was not correctly matched"
    );

    test::helper::was_node_added(
        &["use_declaration", "scoped_use_list", "use_list"],
        after_root,
        &diff_ast,
    )?;

    let mapping = test::helper::mapping_for_path(
        &["use_declaration", "scoped_identifier", "identifier"],
        &[
            "use_declaration",
            "scoped_use_list",
            "use_list",
            "identifier",
        ],
        before_root,
        after_root,
        &diff_ast,
    )?;
    assert_eq!(
        mapping.operation,
        ASTMappingOperation::Identical,
        "The (2nd) scoped_identifier was not correctly matched"
    );

    test::helper::was_node_added(
        &["use_declaration", "scoped_use_list", "use_list", "{"],
        after_root,
        &diff_ast,
    )?;

    test::helper::was_node_added(
        &[
            "use_declaration",
            "scoped_use_list",
            "use_list",
            "identifier:2",
        ],
        after_root,
        &diff_ast,
    )?;

    Ok(())
}
