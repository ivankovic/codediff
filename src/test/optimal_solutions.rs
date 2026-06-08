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
#[cfg(test)]
mod tests {
    use crate::diff;
    use crate::diff::ASTMappingOperation;
    use crate::test;

    use anyhow::Result;

    #[test]
    fn no_change() -> Result<()> {
        let test_diffs = test::helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs.get("no-change").unwrap().clone();

        let diff = diff::diff_code(&before, &after);

        assert!(diff.ast.is_some());

        let diff_ast = diff.ast.unwrap();
        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let mapping = diff_ast
            .mapping
            .get(&(before_ast.root_node().id(), after_ast.root_node().id()))
            .unwrap();
        assert_eq!(mapping.operation, ASTMappingOperation::Identical);

        Ok(())
    }

    #[test]
    fn hello_world_added_message() -> Result<()> {
        let test_diffs = test::helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs.get("hello-world-added-message").unwrap().clone();

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

        assert!(test::helper::was_tree_added(
            &["function_item", "block", "expression_statement:2"],
            after_root,
            &diff_ast
        )?);

        Ok(())
    }

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

    #[test]
    fn rust_hash_optimization() -> Result<()> {
        let test_diffs = test::helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs.get("rust-hash-optimization").unwrap().clone();

        let diff = diff::diff_code(&before, &after);

        assert!(diff.ast.is_some());

        let diff_ast = diff.ast.unwrap();
        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let before_root = before_ast.root_node();
        let after_root = after_ast.root_node();

        // The use declarations, the second was added.
        let path = vec!["use_declaration:1"];
        let mapping =
            test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
        assert_eq!(mapping.operation, ASTMappingOperation::Identical);

        assert!(test::helper::was_tree_added(
            &["use_declaration:2"],
            after_root,
            &diff_ast
        )?);

        // struct SubtreeKey
        let path = vec!["struct_item"];
        let mapping =
            test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
        assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);

        // #derive
        assert!(test::helper::was_tree_added(
            &["attribute_item"],
            after_root,
            &diff_ast
        )?);

        // First 2 fields are added, the first/third is pre-existing
        assert!(test::helper::was_tree_added(
            &[
                "struct_item",
                "field_declaration_list",
                "field_declaration:1"
            ],
            after_root,
            &diff_ast
        )?);

        assert!(test::helper::was_tree_added(
            &[
                "struct_item",
                "field_declaration_list",
                "field_declaration:2"
            ],
            after_root,
            &diff_ast
        )?);

        let mapping = test::helper::mapping_for_path(
            &[
                "struct_item",
                "field_declaration_list",
                "field_declaration:1",
            ],
            &[
                "struct_item",
                "field_declaration_list",
                "field_declaration:3",
            ],
            before_root,
            after_root,
            &diff_ast,
        )?;
        assert_eq!(mapping.operation, ASTMappingOperation::Identical);

        // Verify that fn main and it's code is solved
        let path = vec!["function_item"];
        let mapping =
            test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
        assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);

        // The first let is similar, but Vec<usize> was changed to SubtreeKey
        let path = vec!["function_item", "block", "let_declaration:1"];
        let mapping =
            test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
        assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);

        // The second 2 are identical
        let path = vec!["function_item", "block", "let_declaration:2"];
        let mapping =
            test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
        assert_eq!(mapping.operation, ASTMappingOperation::Identical);

        let path = vec!["function_item", "block", "let_declaration:3"];
        let mapping =
            test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff_ast)?;
        assert_eq!(mapping.operation, ASTMappingOperation::Identical);

        // Followed by 2 added let declarations
        assert!(test::helper::was_tree_added(
            &["function_item", "block", "let_declaration:4"],
            after_root,
            &diff_ast
        )?);

        assert!(test::helper::was_tree_added(
            &["function_item", "block", "let_declaration:5"],
            after_root,
            &diff_ast
        )?);

        // And the final let was modified from _subtrees to _key
        let mapping = test::helper::mapping_for_path(
            &["function_item", "block", "let_declaration:4"],
            &["function_item", "block", "let_declaration:6"],
            before_root,
            after_root,
            &diff_ast,
        )?;
        assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);

        // The after code has impl Hash for SubtreeKey followed by impl SubtreeKey.
        // The before code only has impl SubtreeKey.
        let mapping = test::helper::mapping_for_path(
            &["impl_item:1"],
            &["impl_item:2"],
            before_root,
            after_root,
            &diff_ast,
        )?;
        assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);

        let mapping = test::helper::mapping_for_path(
            &["impl_item:1", "declaration_list", "function_item"],
            &["impl_item:2", "declaration_list", "function_item"],
            before_root,
            after_root,
            &diff_ast,
        )?;
        assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);

        let mapping = test::helper::mapping_for_path(
            &["impl_item:1", "declaration_list", "function_item", "block"],
            &["impl_item:2", "declaration_list", "function_item", "block"],
            before_root,
            after_root,
            &diff_ast,
        )?;
        assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);

        assert!(test::helper::was_tree_added(
            &[
                "impl_item:2",
                "declaration_list",
                "function_item",
                "block",
                "let_declaration"
            ],
            after_root,
            &diff_ast
        )?);
        assert!(test::helper::was_tree_added(
            &[
                "impl_item:2",
                "declaration_list",
                "function_item",
                "block",
                "expression_statement"
            ],
            after_root,
            &diff_ast
        )?);

        let mapping = test::helper::mapping_for_path(
            &[
                "impl_item:1",
                "declaration_list",
                "function_item",
                "block",
                "struct_expression",
            ],
            &[
                "impl_item:2",
                "declaration_list",
                "function_item",
                "block",
                "struct_expression",
            ],
            before_root,
            after_root,
            &diff_ast,
        )?;
        assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);

        assert!(test::helper::was_tree_added(
            &[
                "impl_item:2",
                "declaration_list",
                "function_item",
                "block",
                "struct_expression",
                "field_initializer_list",
                "field_initializer:1"
            ],
            after_root,
            &diff_ast
        )?);
        assert!(test::helper::was_tree_added(
            &[
                "impl_item:2",
                "declaration_list",
                "function_item",
                "block",
                "struct_expression",
                "field_initializer_list",
                "field_initializer:2"
            ],
            after_root,
            &diff_ast
        )?);

        let mapping = test::helper::mapping_for_path(
            &[
                "impl_item:1",
                "declaration_list",
                "function_item",
                "block",
                "struct_expression",
                "field_initializer_list",
                "field_initializer:1",
            ],
            &[
                "impl_item:2",
                "declaration_list",
                "function_item",
                "block",
                "struct_expression",
                "field_initializer_list",
                "field_initializer:3",
            ],
            before_root,
            after_root,
            &diff_ast,
        )?;
        assert_eq!(mapping.operation, ASTMappingOperation::Identical);

        // impl Hash for SubtreeKey was added
        assert!(test::helper::was_tree_added(
            &["impl_item:1"],
            after_root,
            &diff_ast
        )?);

        Ok(())
    }
}
