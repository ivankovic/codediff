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

use crate::code::Code;
use crate::diff::ASTDiff;
use std::collections::HashMap;

/**
* Find the optimal mapping for all nodes in before and after that have not yet been mapped in diff,
* but only using Insert, Delete and Update operations.
*
* The algorithm is pretty complex. The recommended way to understand it is to start by reading the
* tests.
*/
pub fn find(
    _before: &Code,
    _after: &Code,
    _before_cache: Option<&HashMap<usize, tree_sitter::Node>>,
    _after_cache: Option<&HashMap<usize, tree_sitter::Node>>,
    _diff: &mut ASTDiff,
) {
    // TODO: Implement optimal IUD (Insert, Update, Delete) algorithm
}

#[cfg(test)]
mod tests {
    use crate::{
        diff::{COST_DELETE, COST_INSERT},
        test,
    };
    use anyhow::Result;

    use crate::diff::COST_UPDATE;

    use super::*;

    #[test]
    fn is_always_valid() -> Result<()> {
        let test_diffs = test::helper::handmade_test_diffs()?;

        for (diff_name, (before, after)) in test_diffs {
            let mut diff = ASTDiff {
                ..Default::default()
            };

            find(&before, &after, None, None, &mut diff);

            assert!(
                diff.is_valid(&before, &after, None, None),
                "Real diff mappings should always be valid for diff: {}",
                diff_name
            );
            assert!(
                diff.is_complete(&before, &after),
                "Real diff mappings should always be complete for diff: {}",
                diff_name
            );
        }

        Ok(())
    }

    #[test]
    fn hello_world_added_message() -> Result<()> {
        let test_diffs = test::helper::handmade_test_diffs()?;
        let (before, after) = test_diffs.get("hello-world-added-message").unwrap().clone();

        let mut diff = ASTDiff {
            ..Default::default()
        };

        find(&before, &after, None, None, &mut diff);

        assert!(
            diff.is_valid(&before, &after, None, None),
            "Real diff mappings should always be valid"
        );
        assert!(
            diff.is_complete(&before, &after),
            "Real diff mappings should always be complete"
        );

        let after_ast = after.ast.unwrap();

        let added_expression_node = test::helper::node_for_path(
            after_ast.root_node(),
            vec!["function_item", "block", "expression_statement:2"],
        )?;

        let added_expression_node_mapping = diff.mapping.get(&(0, added_expression_node.id()));
        assert!(
            added_expression_node_mapping.is_some(),
            "The node that represents the added line is not mapped as an added node"
        );
        let added_expression_node_mapping = added_expression_node_mapping.unwrap();

        // The added line has 11 nodes, starting with an expression_statement as the root of the
        // subtree.
        assert_eq!(
            added_expression_node_mapping.cost,
            COST_INSERT * 11,
            "String content mapping cost should be COST_UPDATE"
        );

        Ok(())
    }

    #[test]
    fn hello_world_deleted_message() -> Result<()> {
        let test_diffs = test::helper::handmade_test_diffs()?;

        // Note that we flipped after and before so the addition becomes a deletion.
        let (after, before) = test_diffs.get("hello-world-added-message").unwrap().clone();

        let mut diff = ASTDiff {
            ..Default::default()
        };

        find(&before, &after, None, None, &mut diff);

        assert!(
            diff.is_valid(&before, &after, None, None),
            "Real diff mappings should always be valid"
        );
        assert!(
            diff.is_complete(&before, &after),
            "Real diff mappings should always be complete"
        );

        let before_ast = before.ast.unwrap();

        let deleted_expression_node = test::helper::node_for_path(
            before_ast.root_node(),
            vec!["function_item", "block", "expression_statement:2"],
        )?;

        let deleted_expression_node_mapping = diff.mapping.get(&(0, deleted_expression_node.id()));
        assert!(
            deleted_expression_node_mapping.is_some(),
            "The node that represents the added line is not mapped as a deleted node"
        );
        let deleted_expression_node_mapping = deleted_expression_node_mapping.unwrap();

        assert_eq!(
            deleted_expression_node_mapping.cost,
            COST_DELETE * 11,
            "String content mapping cost should be COST_UPDATE"
        );

        Ok(())
    }

    #[test]
    fn python_leetcode_1_added_if_block() -> Result<()> {
        let test_diffs = test::helper::handmade_test_diffs()?;

        // This test case is a common python edit: adding an if and identing a few lines after the
        // if.
        //
        // The following 13 nodes are added:
        // if_statement
        //      if
        //      comparison_operator
        //          identifier
        //          !=
        //          list
        //              [
        //              integer
        //              ,
        //              integer
        //              ]
        //      :
        //      block
        //
        //  After block, there are 14 nodes that should be mapped as an exact match to the existing
        //  nodes:
        //
        //          expression_statement
        //              call
        //                  identifier
        //                  argument_list
        //                      (
        //                      string
        //                          string_start
        //                          string_content
        //                          interpolation
        //                              {
        //                              identifier
        //                              }
        //                          string_end
        //                      )
        //
        //  The absolutely crucial nodes in this test case are the if_statement and it's child
        //  block. What they show us is that the optimal solution might include TWO dependent
        //  inserts. The optimal solution in theory is:
        //
        //  1) Find the 3rd child of the root "module" node. This is the if_statement node from "if
        //     __name__ == "__main__""
        //  2) Find the 3rd child of that node. This is the "block" node from the main.
        //  3) Insert an if_statement node between the main "block" node and the 4th child of the
        //     "block" node, which is the 4th "expression_statement" node.
        //  4) Insert the "block" node between the newly added "if_statement" node and it's child,
        //     the "expression_statement" node that was "taken" from the main "block" node.
        //  5) Insert the 12 nodes required to form the if condition under the previously inserted
        //     if_statement before the newly added "block" node.
        //
        //  Alternatively, steps 4 and 5 can be swapped, first adding the 12 nodes and then adding
        //  the "block" node in it's correct place.
        //
        //  But either way, the "expression_statement" from the before tree will get a new parent
        //  _twice_ and end up two nodes deep from the main "block".
        //
        //  This is important because this test case makes naive "just do the simple edit distance"
        //  algorithms fail, since those would not usually consider the ability to modify the
        //  parent of a node twice.
        let (before, after) = test_diffs.get("python-added-if-block").unwrap().clone();

        let mut diff = ASTDiff {
            ..Default::default()
        };

        find(&before, &after, None, None, &mut diff);

        assert!(
            diff.is_valid(&before, &after, None, None),
            "Real diff mappings should always be valid"
        );
        assert!(
            diff.is_complete(&before, &after),
            "Real diff mappings should always be complete"
        );

        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let added_if_node = test::helper::node_for_path(
            after_ast.root_node(),
            vec!["if_statement", "block", "if_statement"],
        )?;

        let added_if_node_mapping = diff.mapping.get(&(0, added_if_node.id()));
        assert!(
            added_if_node_mapping.is_some(),
            "The node that represents the added if statement is not mapped as an added node"
        );
        let added_if_node_mapping = added_if_node_mapping.unwrap();

        assert_eq!(
            added_if_node_mapping.cost,
            COST_INSERT * 13,
            "Adding an if should add 13 nodes total."
        );

        let existing_expression_node = test::helper::node_for_path(
            before_ast.root_node(),
            vec!["if_statement", "block", "expression_statement:4"],
        )?;

        let expression_node_in_after_id = diff
            .before_node_map
            .get(&existing_expression_node.id())
            .expect("The indented expression is not found in the diff");

        let existing_expression_node_mapping = diff
            .mapping
            .get(&(existing_expression_node.id(), *expression_node_in_after_id));

        assert!(
            existing_expression_node_mapping.is_some(),
            "The node that represents the newly conditioned line is not in the diff"
        );
        let existing_expression_node_mapping = existing_expression_node_mapping.unwrap();

        assert_eq!(
            existing_expression_node_mapping.cost, 0,
            "The existing expression should have cost 0"
        );

        Ok(())
    }

    #[test]
    fn python_leetcode_1_added_if_block_reverse() -> Result<()> {
        let test_diffs = test::helper::handmade_test_diffs()?;

        // Note that we do a sneaky and flip (before, after) to get a delete instead of an add.
        let (after, before) = test_diffs.get("python-added-if-block").unwrap().clone();

        let mut diff = ASTDiff {
            ..Default::default()
        };

        find(&before, &after, None, None, &mut diff);

        assert!(
            diff.is_valid(&before, &after, None, None),
            "Real diff mappings should always be valid"
        );
        assert!(
            diff.is_complete(&before, &after),
            "Real diff mappings should always be complete"
        );

        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let deleted_if_node = test::helper::node_for_path(
            after_ast.root_node(),
            vec!["if_statement", "block", "if_statement"],
        )?;

        let deleted_if_node_mapping = diff.mapping.get(&(0, deleted_if_node.id()));
        assert!(
            deleted_if_node_mapping.is_some(),
            "The node that represents the deleted if statement is not mapped as an deleted node"
        );
        let deleted_if_node_mapping = deleted_if_node_mapping.unwrap();

        assert_eq!(
            deleted_if_node_mapping.cost,
            COST_DELETE * 13,
            "Deleting the if deletes 13 nodes."
        );

        let existing_expression_node = test::helper::node_for_path(
            before_ast.root_node(),
            vec!["if_statement", "block", "expression_statement:4"],
        )?;

        let expression_node_in_after_id = diff
            .before_node_map
            .get(&existing_expression_node.id())
            .expect("The un-indented expression is not found in the diff");

        let existing_expression_node_mapping = diff
            .mapping
            .get(&(existing_expression_node.id(), *expression_node_in_after_id));

        assert!(
            existing_expression_node_mapping.is_some(),
            "The node that represents the newly un-conditioned line is not in the diff"
        );
        let existing_expression_node_mapping = existing_expression_node_mapping.unwrap();

        assert_eq!(
            existing_expression_node_mapping.cost, 0,
            "The existing expression should have cost 0"
        );

        Ok(())
    }

    #[test]
    fn translated_hello_world() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;

        let before = test_codes.get("hello-world.rs").unwrap().clone();
        let after = test_codes.get("zdravo-svijete.rs").unwrap().clone();

        let mut diff = ASTDiff {
            ..Default::default()
        };

        find(&before, &after, None, None, &mut diff);

        assert!(diff.is_valid(&before, &after, None, None));
        assert!(diff.is_complete(&before, &after));

        // The trees are almost identical. A complete solution exists.
        assert_eq!(diff.mapping.len(), 22);

        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let before_string_node = test::helper::node_for_path(
            before_ast.root_node(),
            vec![
                "function_item",
                "block",
                "expression_statement",
                "macro_invocation",
                "token_tree",
                "string_literal",
                "string_content",
            ],
        )?;
        let after_string_node = test::helper::node_for_path(
            after_ast.root_node(),
            vec![
                "function_item",
                "block",
                "expression_statement",
                "macro_invocation",
                "token_tree",
                "string_literal",
                "string_content",
            ],
        )?;

        let before_node_id = before_string_node.id();
        let after_node_id = after_string_node.id();

        let mapping = diff.mapping.get(&(before_node_id, after_node_id));
        assert!(
            mapping.is_some(),
            "String content nodes should be mapped to each other"
        );
        let mapping = mapping.unwrap();

        // The cost should be COST_UPDATE (1), since only the string constant value has changed.
        assert_eq!(
            mapping.cost, COST_UPDATE,
            "String content mapping cost should be COST_UPDATE"
        );

        Ok(())
    }
}
