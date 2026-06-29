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
    let (before, after) = test_diffs
        .get("rust-turbopack-module-rule")
        .unwrap()
        .clone();

    let diff = diff::diff_code(&before, &after);

    assert!(diff.ast.is_some());

    let diff_ast = diff.ast.unwrap();
    let before_ast = before.ast.unwrap();
    let after_ast = after.ast.unwrap();

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

    // This is the ModuleType enum.
    // There are 2 changes in the enum:
    // 1. InlinedBytesJS enum value was deleted
    // 2. Whitespace in StaticUrlJS was removed
    //
    // Only the 1st change is part of the diff. AST diffs ignore whitespace changes.

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

    // TODO: complete the optimal solution for the rest of the file. There is a lot  of changed
    // nodes there, it's a real complicated diff.

    Ok(())
}
