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
fn matches_human_solution() -> Result<()> {
    // Lowered from 175 to 52: the 2026-07-15 default-heuristic change (see TODO.md) disabled
    // solver_import_nodes/solver_bottom_up_expansion by default (plus solver_similar_flow_control,
    // deleted outright 2026-08-14), which measurably improved this fixture's match quality.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "rust-turbopack-module-rule",
        49,
        21,
    )
}

#[test]
fn optimal_solution() -> Result<()> {
    let (before, after) = &*test::helper::handmade_test_code_pair("rust-turbopack-module-rule")?;

    let diff = diff::diff_code(before, after);

    assert!(diff.ast.is_some());

    let diff_ast = diff.ast.unwrap();
    let before_ast = before.ast.as_ref().unwrap();
    let after_ast = after.ast.as_ref().unwrap();

    let before_root = before_ast.root_node();
    let after_root = after_ast.root_node();

    // These are the parts of the code that are not changed at all.
    // The expectation is that these are all quickly matched using the efficient hash based
    // reference node matcher.
    let paths = [
        // The first 8 use declarations are exactly the same
        // as are the last 2. The only changed declaration is number 9
        ["use_declaration:1"],
        ["use_declaration:2"],
        ["use_declaration:3"],
        ["use_declaration:4"],
        ["use_declaration:5"],
        ["use_declaration:6"],
        ["use_declaration:7"],
        ["use_declaration:8"],
        // use_declaration:9 has a node added to it and is tested further on in the test.
        ["use_declaration:10"],
        ["use_declaration:11"],
        // ModuleRule struct is not changed.
        // These are the pub struct ModuleRule and impl ModuleRule segments.
        ["attribute_item:1"],
        ["struct_item:1"],
        // Equally, ModuleRuleEffect enum is not changed.
        // This is the enum with the preeceding 2 attribute_items.
        ["attribute_item:2"],
        ["attribute_item:3"],
        ["enum_item:1"],
    ];
    for path in paths.iter() {
        let mapping =
            test::helper::mapping_for_path(path, path, before_root, after_root, &diff_ast)?;
        assert_eq!(
            mapping.operation,
            ASTMappingOperation::Identical,
            "The 8 inital use declarations are not correctly mapped as identical"
        );
    }

    // This is the use declaration that changes from:
    //
    // use turbopack_ecmascript::{EcmascriptInputTransforms, EcmascriptOptions};
    //
    // to:
    //
    // use turbopack_ecmascript::{
    //   EcmascriptInputTransforms, EcmascriptOptions, bytes_source_transform::BytesSourceTransform,
    // };
    let path = ["use_declaration:9"];
    let mapping = test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(
        mapping.operation,
        ASTMappingOperation::MatchButNotIdentical,
        "The 9th use declarations is not correctly mapped"
    );
    let path = ["use_declaration:9", "scoped_use_list"];
    let mapping = test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(
        mapping.operation,
        ASTMappingOperation::MatchButNotIdentical,
        "The 9th use declarations scoped_use_list is not correctly mapped"
    );
    let path = ["use_declaration:9", "scoped_use_list", "use_list"];
    let mapping = test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(
        mapping.operation,
        ASTMappingOperation::MatchButNotIdentical,
        "The 9th use declarations use_list is not correctly mapped"
    );
    let path = [
        "use_declaration:9",
        "scoped_use_list",
        "use_list",
        "identifier:1",
    ];
    let mapping = test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(
        mapping.operation,
        ASTMappingOperation::Identical,
        "The 9th use declarations EcmascriptInputTransforms is not correctly mapped"
    );
    let path = [
        "use_declaration:9",
        "scoped_use_list",
        "use_list",
        "identifier:2",
    ];
    let mapping = test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(
        mapping.operation,
        ASTMappingOperation::Identical,
        "The 9th use declarations EcmascriptOptions is not correctly mapped"
    );
    // bytes_source_transform::BytesSourceTransform is a scoped path, not a plain identifier,
    // so it appears as a scoped_identifier node in the use_list.
    let path = [
        "use_declaration:9",
        "scoped_use_list",
        "use_list",
        "scoped_identifier",
    ];
    test::helper::was_tree_added(&path, after_root, &diff_ast)?;

    // This is the ModuleType enum. InlinedBytesJS enum variant was deleted.

    // First, the two attribute items above the enum, they are identical.
    let path = ["attribute_item:4"];
    let mapping = test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(
        mapping.operation,
        ASTMappingOperation::Identical,
        "The #[turbo_task] attribute for ModuleType enum is not correctly mapped",
    );
    let path = ["attribute_item:5"];
    let mapping = test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(
        mapping.operation,
        ASTMappingOperation::Identical,
        "The #[derive] attribute for ModuleType enum is not correctly mapped",
    );

    // The ModuleType as a whole.
    let path = ["enum_item:2"];
    let mapping = test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(
        mapping.operation,
        ASTMappingOperation::MatchButNotIdentical,
        "The ModuleType enum is not correctly mapped",
    );

    // These are the enum Values that are not changed at all
    let paths = [
        ["enum_item:2", "enum_variant_list", "enum_variant:1"],
        ["enum_item:2", "enum_variant_list", "enum_variant:2"],
        ["enum_item:2", "enum_variant_list", "enum_variant:3"],
        ["enum_item:2", "enum_variant_list", "enum_variant:4"],
        ["enum_item:2", "enum_variant_list", "enum_variant:5"],
        ["enum_item:2", "enum_variant_list", "enum_variant:6"],
        ["enum_item:2", "enum_variant_list", "enum_variant:7"],
        ["enum_item:2", "enum_variant_list", "enum_variant:8"],
        ["enum_item:2", "enum_variant_list", "enum_variant:9"],
        ["enum_item:2", "enum_variant_list", "enum_variant:10"],
        ["enum_item:2", "enum_variant_list", "enum_variant:11"],
    ];
    for path in paths.iter() {
        let mapping =
            test::helper::mapping_for_path(path, path, before_root, after_root, &diff_ast)?;
        assert_eq!(
            mapping.operation,
            ASTMappingOperation::Identical,
            "The first 11 enum values in ModuleType enum are not correctly detected as identical"
        );
    }

    // The 12th variant is the deleted InlinedBytesJS
    test::helper::was_tree_deleted(
        &["enum_item:2", "enum_variant_list", "enum_variant:12"],
        before_root,
        &diff_ast,
    )?;

    // The remaining 2 variants match as identical, but they are now off-by-one in the two trees.
    let mapping = test::helper::mapping_for_path(
        &["enum_item:2", "enum_variant_list", "enum_variant:13"],
        &["enum_item:2", "enum_variant_list", "enum_variant:12"],
        before_root,
        after_root,
        &diff_ast,
    )?;
    assert_eq!(
        mapping.operation,
        ASTMappingOperation::Identical,
        "The WebAssembly enum value in ModuleType enum is not correctly detected as identical"
    );
    let mapping = test::helper::mapping_for_path(
        &["enum_item:2", "enum_variant_list", "enum_variant:14"],
        &["enum_item:2", "enum_variant_list", "enum_variant:13"],
        before_root,
        after_root,
        &diff_ast,
    )?;
    assert_eq!(
        mapping.operation,
        ASTMappingOperation::Identical,
        "The Custom enum value in ModuleType enum is not correctly detected as identical"
    );

    // Now we deal with impl Display for ModuleType.
    // The only change is that the InlinedBytesJS was removed from the match.
    //
    assert!(
        test::helper::entire_path_has_mapping(
            &["impl_item:2", "declaration_list", "function_item"],
            before_root,
            after_root,
            &diff_ast,
            ASTMappingOperation::MatchButNotIdentical
        )?,
        "The impl Display for ModuleType path is not correctly mapped"
    );

    // The function parameters and the return value are the same
    let path = [
        "impl_item:2",
        "declaration_list",
        "function_item",
        "parameters",
    ];
    let mapping = test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(
        mapping.operation,
        ASTMappingOperation::Identical,
        "The impl Display for ModuleType - fmt function is not correctly mapped",
    );
    let path = [
        "impl_item:2",
        "declaration_list",
        "function_item",
        "scoped_type_identifier",
    ];
    let mapping = test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
    assert_eq!(
        mapping.operation,
        ASTMappingOperation::Identical,
        "The impl Display for ModuleType - fmt function return type is not correctly mapped",
    );

    // This is the block with the match statement
    assert!(
        test::helper::entire_path_has_mapping(
            &[
                "impl_item:2",
                "declaration_list",
                "function_item",
                "block",
                "expression_statement",
                "match_expression",
                "match_block"
            ],
            before_root,
            after_root,
            &diff_ast,
            ASTMappingOperation::MatchButNotIdentical
        )?,
        "The impl Display for ModuleType - fmt function body path is not correctly mapped"
    );

    // We use these as starting points for the helper calls to make the path strings shorter.
    let path = [
        "impl_item:2",
        "declaration_list",
        "function_item",
        "block",
        "expression_statement",
        "match_expression",
        "match_block",
    ];
    let match_block_before = test::helper::node_for_path(before_root, &path)?;
    let match_block_after = test::helper::node_for_path(after_root, &path)?;

    // These are the match arms that are not changed.
    let paths = [
        // The first 11 match_arms are identical, as are the last 2.
        // However, because the 12th was deleted, it moves the indexing on the after side so we only
        // check for 1 to 11 here.
        ["match_arm:1"],
        ["match_arm:2"],
        ["match_arm:3"],
        ["match_arm:4"],
        ["match_arm:5"],
        ["match_arm:6"],
        ["match_arm:7"],
        ["match_arm:8"],
        ["match_arm:9"],
        ["match_arm:10"],
        ["match_arm:11"],
    ];
    for path in paths.iter() {
        let mapping = test::helper::mapping_for_path(
            path,
            path,
            match_block_before,
            match_block_after,
            &diff_ast,
        )?;
        assert_eq!(
            mapping.operation,
            ASTMappingOperation::Identical,
            "The first 11 match arms in impl Display for ModuleType are not correctly mapped"
        );
    }

    assert!(test::helper::was_tree_deleted(
        &["match_arm:12"],
        match_block_before,
        &diff_ast
    )?);

    let mapping = test::helper::mapping_for_path(
        &["match_arm:13"],
        &["match_arm:12"],
        match_block_before,
        match_block_after,
        &diff_ast,
    )?;
    assert_eq!(
        mapping.operation,
        ASTMappingOperation::Identical,
        "The ModuleType::WebAssembly match arm in impl Display for ModuleType is not correctly mapped"
    );
    let mapping = test::helper::mapping_for_path(
        &["match_arm:14"],
        &["match_arm:13"],
        match_block_before,
        match_block_after,
        &diff_ast,
    )?;
    assert_eq!(
        mapping.operation,
        ASTMappingOperation::Identical,
        "The ModuleType::Custom match arm in impl Display for ModuleType is not correctly mapped"
    );

    assert!(
        test::helper::was_tree_added(&["enum_item:3"], after_root, &diff_ast)?,
        "The entire subtree for pub enum ConfiguredModuleType was not correctly marked as added"
    );

    // This is the more subjective part of this diff.
    //
    // impl ConfiguredModuleType fn parse is new, since ConfiguredModuleType is itself new.
    //
    // But the match is partially the logical functionality that was previously in
    // ModuleType::from_str_with_defaults. In particular, the match arms have the same patterns.
    //
    // If we think about the human in this case, what a reviewer would like to see is:
    //
    // 1. These are roughly the same match patterns.
    // 2. Is any pattern missing?
    // 3. Was any pattern added?

    assert!(
        test::helper::was_node_added(&["impl_item:3"], after_root, &diff_ast)?,
        "The impl ConfiguredModuleType impl_item was not correctly marked as added",
    );

    assert!(
        test::helper::was_node_added(&["impl_item:3", "declaration_list"], after_root, &diff_ast)?,
        "The impl ConfiguredModuleType declaration_list was not correctly marked as added",
    );

    assert!(
        test::helper::was_node_added(
            &["impl_item:3", "declaration_list", "function_item:1"],
            after_root,
            &diff_ast
        )?,
        "The impl ConfiguredModuleType fn parse was not correctly marked as added",
    );

    assert!(
        test::helper::was_node_added(
            &[
                "impl_item:3",
                "declaration_list",
                "function_item:1",
                "block"
            ],
            after_root,
            &diff_ast
        )?,
        "The impl ConfiguredModuleType fn parse block was not correctly marked as added",
    );

    assert!(
        test::helper::was_node_added(
            &[
                "impl_item:3",
                "declaration_list",
                "function_item:1",
                "block",
                "call_expression"
            ],
            after_root,
            &diff_ast
        )?,
        "The impl ConfiguredModuleType fn parse block was not correctly marked as added",
    );

    Ok(())
}
