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
fn optimal_solution() -> Result<()> {
    let (before, after) = test::helper::handmade_test_code_pair("rust-add-if")?;

    let diff = diff::diff_code(&before, &after);

    assert!(diff.ast.is_some());

    let diff_ast = diff.ast.unwrap();
    let before_ast = before.ast.unwrap();
    let after_ast = after.ast.unwrap();

    let before_root = before_ast.root_node();
    let after_root = after_ast.root_node();

    // Root nodes should be mapped
    assert!(
        diff_ast
            .mapping
            .contains_key(&(before_root.id(), after_root.id()))
    );

    // This test case starts with an "if-else" code, and adds a new condition to the *start* of
    // the if, going to "if-else if-else" code. In the Rust AST, what happens is that the entire
    // before if_expression block becomes the child of the else node of the newly added
    // if_expression block.

    let if_expression_before = test::helper::node_for_path(
        before_root,
        &[
            "function_item",
            "block",
            "expression_statement",
            "if_expression",
        ],
    )?;

    let path = vec![
        "function_item",
        "block",
        "expression_statement",
        "if_expression",
    ];
    assert!(
        test::helper::was_node_added(&path, after_root, &diff_ast)?,
        "The added if_expression is not mapped as added"
    );

    let outer_if_expression_after = test::helper::node_for_path(after_root, &path)?;

    let inner_if_expression_after =
        test::helper::node_for_path(outer_if_expression_after, &["else_clause", "if_expression"])?;

    assert!(
        diff_ast
            .mapping
            .contains_key(&(if_expression_before.id(), inner_if_expression_after.id())),
        "The if_expression from before is not correctly mapped as the child of the else_clause of the added if"
    );

    let mapping = diff_ast
        .mapping
        .get(&(if_expression_before.id(), inner_if_expression_after.id()))
        .unwrap();
    assert_eq!(mapping.operation, ASTMappingOperation::Identical);

    Ok(())
}

#[test]
fn matches_human_solution() -> Result<()> {
    test::helper::human_mapping::assert_matches_human_mapping("rust-add-if")
}
