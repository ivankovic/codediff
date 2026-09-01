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
    let (before, after) = &*test::helper::handmade_test_code_pair("rust-add-value-to-enum")?;

    let diff = diff::diff_code(before, after);

    assert!(diff.ast.is_some());

    let diff_ast = diff.ast.unwrap();
    let before_ast = before.ast.as_ref().unwrap();
    let after_ast = after.ast.as_ref().unwrap();

    let before_root = before_ast.root_node();
    let after_root = after_ast.root_node();

    assert!(
        test::helper::entire_path_has_mapping(
            &["enum_item", "enum_variant_list"],
            before_root,
            after_root,
            &diff_ast,
            ASTMappingOperation::MatchButNotIdentical
        )?,
        "The enum path is not correctly mapped"
    );

    let path = ["enum_item", "enum_variant_list", "enum_variant:1"];
    let mapping = test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(
        mapping.operation,
        ASTMappingOperation::Identical,
        "The first enum variant, Ecmascript, is not correctly mapped"
    );

    test::helper::was_tree_added(
        &["enum_item", "enum_variant_list", "enum_variant:2"],
        after_root,
        &diff_ast,
    )?;

    let mapping = test::helper::mapping_for_path(
        &["enum_item", "enum_variant_list", "enum_variant:2"],
        &["enum_item", "enum_variant_list", "enum_variant:3"],
        before_root,
        after_root,
        &diff_ast,
    )?;
    assert_eq!(
        mapping.operation,
        ASTMappingOperation::Identical,
        "The last enum variant, Webassembly, is not correctly mapped"
    );

    Ok(())
}

#[test]
fn matches_human_solution() -> Result<()> {
    test::helper::human_mapping::assert_matches_human_mapping("rust-add-value-to-enum")
}

#[test]
fn optimal_solution_for_reversed_diff() -> Result<()> {
    let (after, before) = &*test::helper::handmade_test_code_pair("rust-add-value-to-enum")?;

    let diff = diff::diff_code(before, after);

    assert!(diff.ast.is_some());

    let diff_ast = diff.ast.unwrap();
    let before_ast = before.ast.as_ref().unwrap();
    let after_ast = after.ast.as_ref().unwrap();

    let before_root = before_ast.root_node();
    let after_root = after_ast.root_node();

    assert!(
        test::helper::entire_path_has_mapping(
            &["enum_item", "enum_variant_list"],
            before_root,
            after_root,
            &diff_ast,
            ASTMappingOperation::MatchButNotIdentical
        )?,
        "The enum path is not correctly mapped"
    );

    let path = ["enum_item", "enum_variant_list", "enum_variant:1"];
    let mapping = test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(
        mapping.operation,
        ASTMappingOperation::Identical,
        "The first enum variant, Ecmascript, is not correctly mapped"
    );

    test::helper::was_tree_deleted(
        &["enum_item", "enum_variant_list", "enum_variant:2"],
        before_root,
        &diff_ast,
    )?;

    let mapping = test::helper::mapping_for_path(
        &["enum_item", "enum_variant_list", "enum_variant:3"],
        &["enum_item", "enum_variant_list", "enum_variant:2"],
        before_root,
        after_root,
        &diff_ast,
    )?;
    assert_eq!(
        mapping.operation,
        ASTMappingOperation::Identical,
        "The last enum variant, Webassembly, is not correctly mapped"
    );

    Ok(())
}
