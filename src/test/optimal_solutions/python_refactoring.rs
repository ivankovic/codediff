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

use anyhow::Result;

#[test]
fn python_refactoring() -> Result<()> {
    let test_diffs = test::helper::handmade_test_code_pairs()?;
    let (before, after) = test_diffs.get("python-refactoring").unwrap().clone();

    let diff = diff::diff_code(&before, &after);

    assert!(diff.ast.is_some());

    let diff_ast = diff.ast.unwrap();
    let before_ast = before.ast.unwrap();
    let after_ast = after.ast.unwrap();

    let before_root = before_ast.root_node();
    let after_root = after_ast.root_node();

    let path = vec!["function_definition"];
    let mapping =
        test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);

    let path = vec!["function_definition", "parameters"];
    let mapping =
        test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::Identical);

    let path = vec!["function_definition", "block"];
    let mapping =
        test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);

    let path = vec!["function_definition", "block", "expression_statement:1"];
    let mapping =
        test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::Identical);

    let path = vec!["function_definition", "block", "if_statement"];
    let mapping =
        test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::Identical);

    let path = vec!["function_definition", "block", "expression_statement:2"];
    let mapping =
        test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);

    // Here, we examine:
    //
    // total = 0
    //
    // changing to
    //
    // total = sum(numbers)
    //
    // This is the core of this diff, where AST diffing performs much better than text based
    // diff. nvim -d detect this as a delete and then an update of the for loop, which is
    // perhaps reasonable for text based diffing, but is obviously suboptimal. A much better
    // diff that more closely follows the logic of code and what the human has actually done is
    // to say that the for loop was deleted, and that the assignment was changed.
    //
    // Here's how the AST looks like if we focus on the assignment:
    //
    // Before:
    //
    // assignment
    //   |- identifier
    //   |- =
    //   |- integer
    //
    // After:
    //
    // assignment
    //   |- identifier
    //   |- =
    //   |- call
    //       |- identifier
    //       |- argument_list
    //            |- (
    //            |- identifier
    //            |- )
    //
    //  With the AST visible, it's clear that the optimal solution is that the identifier and
    //  equals signs are an Identical match, the integer is a delete and the call with it's
    //  subtree is an add.

    let path = vec![
        "function_definition",
        "block",
        "expression_statement:2",
        "assignment",
    ];
    let mapping =
        test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);

    let path = vec![
        "function_definition",
        "block",
        "expression_statement:2",
        "assignment",
        "identifier",
    ];
    let mapping =
        test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::Identical);

    assert!(test::helper::was_tree_deleted(
        &[
            "function_definition",
            "block",
            "expression_statement:2",
            "assignment",
            "integer"
        ],
        before_root,
        &diff_ast
    )?);

    assert!(test::helper::was_tree_added(
        &[
            "function_definition",
            "block",
            "expression_statement:2",
            "assignment",
            "call"
        ],
        after_root,
        &diff_ast
    )?);

    let path = vec!["function_definition", "block", "expression_statement:3"];
    let mapping =
        test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);

    let path = vec!["function_definition", "block", "expression_statement:4"];
    let mapping =
        test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::Identical);

    assert!(test::helper::was_tree_deleted(
        &["function_definition", "block", "for_statement:1"],
        before_root,
        &diff_ast
    )?);

    assert!(test::helper::was_tree_deleted(
        &["function_definition", "block", "for_statement:2"],
        before_root,
        &diff_ast
    )?);

    let path = vec!["function_definition", "block", "return_statement"];
    let mapping =
        test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);

    let path = vec![
        "function_definition",
        "block",
        "return_statement",
        "dictionary",
    ];
    let mapping =
        test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);

    let path = vec![
        "function_definition",
        "block",
        "return_statement",
        "dictionary",
        "pair:1",
    ];
    let mapping =
        test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::Identical);

    let path = vec![
        "function_definition",
        "block",
        "return_statement",
        "dictionary",
        "pair:2",
    ];
    let mapping =
        test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::Identical);

    let path = vec![
        "function_definition",
        "block",
        "return_statement",
        "dictionary",
        "pair:3",
    ];
    let mapping =
        test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::Identical);

    let path = vec![
        "function_definition",
        "block",
        "return_statement",
        "dictionary",
        "pair:4",
    ];
    let mapping =
        test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::Identical);

    assert!(test::helper::was_tree_added(
        &[
            "function_definition",
            "block",
            "return_statement",
            "dictionary",
            "pair:5",
        ],
        after_root,
        &diff_ast
    )?);

    Ok(())
}
