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
use crate::test::helper::human_mapping::assert_matches_human_painting_within_limit;
use anyhow::Result;

#[test]
fn mapping_details() -> Result<()> {
    let (before, after) = &*test::helper::handmade_test_code_pair("python-added-if-block")?;

    let diff = diff::diff_code(before, after);

    assert!(diff.ast.is_some());

    let diff_ast = diff.ast.unwrap();
    let before_ast = before.ast.as_ref().unwrap();
    let after_ast = after.ast.as_ref().unwrap();

    let before_root = before_ast.root_node();
    let after_root = after_ast.root_node();

    // Double check the indetical code at the start is matched
    let path = vec!["function_definition"];
    let mapping = test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::Identical);

    // The interesting part is the added if_statement
    assert!(
        test::helper::entire_path_has_mapping(
            &["if_statement", "block"],
            before_root,
            after_root,
            &diff_ast,
            ASTMappingOperation::MatchButNotIdentical
        )?,
        "The if_statement block path is not correctly mapped"
    );

    let path = vec!["if_statement", "block", "expression_statement:1"];
    let mapping = test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::Identical);

    let path = vec!["if_statement", "block", "expression_statement:2"];
    let mapping = test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::Identical);

    let path = vec!["if_statement", "block", "expression_statement:3"];
    let mapping = test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::Identical);

    // This is the core of the test. What separates AST diff from text diff.
    // It has to recognize the if_statement and it's companion nodes were added, but the
    // expression_statement was moved.
    assert!(test::helper::was_node_added(
        &["if_statement", "block", "if_statement"],
        after_root,
        &diff_ast
    )?);

    assert!(test::helper::was_tree_added(
        &[
            "if_statement",
            "block",
            "if_statement",
            "comparison_operator"
        ],
        after_root,
        &diff_ast
    )?);

    assert!(test::helper::was_node_added(
        &["if_statement", "block", "if_statement", "block"],
        after_root,
        &diff_ast
    )?);

    let before_expression_statement = test::helper::node_for_path(
        before_root,
        &["if_statement", "block", "expression_statement:4"],
    )?;
    let after_expression_statement = test::helper::node_for_path(
        after_root,
        &[
            "if_statement",
            "block",
            "if_statement",
            "block",
            "expression_statement",
        ],
    )?;

    let mapping = diff_ast.mapping.get(&(
        before_expression_statement.id(),
        after_expression_statement.id(),
    ));

    assert!(
        mapping.is_some(),
        "Unable to find mapping for the idented expression statement"
    );
    let mapping = mapping.unwrap();
    assert_eq!(mapping.operation, ASTMappingOperation::Identical);

    Ok(())
}

#[test]
fn mapping() -> Result<()> {
    test::helper::human_mapping::assert_matches_human_mapping("python-added-if-block")
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-08-26: minimal 3.833%, full 0.213%
    assert_matches_human_painting_within_limit("python-added-if-block", 3.85)
}
