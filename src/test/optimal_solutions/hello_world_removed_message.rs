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
fn hello_world_removed_message() -> Result<()> {
    let test_diffs = test::helper::handmade_test_code_pairs()?;
    let (before, after) = test_diffs
        .get("hello-world-removed-message")
        .unwrap()
        .clone();

    let diff = diff::diff_code(&before, &after);

    assert!(diff.ast.is_some());

    let diff_ast = diff.ast.unwrap();
    let before_ast = before.ast.unwrap();
    let after_ast = after.ast.unwrap();

    let before_root = before_ast.root_node();
    let after_root = after_ast.root_node();

    let path = vec!["function_item"];
    let mapping =
        test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);

    let path = vec!["function_item", "block", "expression_statement:1"];
    let mapping =
        test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::Identical);

    assert!(test::helper::was_tree_deleted(
        &["function_item", "block", "expression_statement:2"],
        before_root,
        &diff_ast
    )?);

    Ok(())
}
