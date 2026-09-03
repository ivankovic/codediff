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
    let (before, after) =
        &*test::helper::handmade_test_code_pair("rust-hello-world-removed-message")?;

    let diff = diff::diff_code(before, after);

    assert!(diff.ast.is_some());

    let diff_ast = diff.ast.unwrap();
    let before_ast = before.ast.as_ref().unwrap();
    let after_ast = after.ast.as_ref().unwrap();

    let before_root = before_ast.root_node();
    let after_root = after_ast.root_node();

    let path = vec!["function_item"];
    let mapping = test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);

    let path = vec!["function_item", "block", "expression_statement:1"];
    let mapping = test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::Identical);

    assert!(test::helper::was_tree_deleted(
        &["function_item", "block", "expression_statement:2"],
        before_root,
        &diff_ast
    )?);

    Ok(())
}

#[test]
fn mapping() -> Result<()> {
    test::helper::human_mapping::assert_matches_human_mapping("rust-hello-world-removed-message")
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-08-27: minimal 0.000% (0 bytes), full 0.685% (1 of 146 bytes)
    //
    // The mirror image of `rust-hello-world-added-message`, and it scores identically - which is
    // the useful part: the same edit read backwards costs the same, so nothing here is
    // direction-dependent.
    assert_matches_human_painting_within_limit("rust-hello-world-removed-message", 0.69)
}
