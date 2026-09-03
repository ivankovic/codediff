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
    let (before, after) = &*test::helper::handmade_test_code_pair("rust-leetcode-1-bugfix")?;

    let diff = diff::diff_code(before, after);

    assert!(diff.ast.is_some());

    let diff_ast = diff.ast.unwrap();
    let before_ast = before.ast.as_ref().unwrap();
    let after_ast = after.ast.as_ref().unwrap();

    let before_root = before_ast.root_node();
    let after_root = after_ast.root_node();

    let path = vec!["use_declaration"];
    let mapping = test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::Identical);

    let path = vec!["impl_item"];
    let mapping = test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);

    let path = vec!["impl_item", "type_identifier"];
    let mapping = test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::Identical);

    assert!(
        test::helper::entire_path_has_mapping(
            &["impl_item", "declaration_list", "function_item"],
            before_root,
            after_root,
            &diff_ast,
            ASTMappingOperation::MatchButNotIdentical
        )?,
        "The impl_item declaration_list function_item path is not correctly mapped"
    );

    let path = vec![
        "impl_item",
        "declaration_list",
        "function_item",
        "parameters",
    ];
    let mapping = test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(mapping.operation, ASTMappingOperation::Identical);

    assert!(test::helper::was_tree_added(
        &[
            "impl_item",
            "declaration_list",
            "function_item",
            "block",
            "expression_statement:1"
        ],
        after_root,
        &diff_ast
    )?);

    Ok(())
}

#[test]
fn mapping() -> Result<()> {
    test::helper::human_mapping::assert_matches_human_mapping("rust-leetcode-1-bugfix")
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-01: minimal 1.546%, full 0.000% (measured, unexamined)
    assert_matches_human_painting_within_limit("rust-leetcode-1-bugfix", 1.57)
}
