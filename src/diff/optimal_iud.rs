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

/**
* Find the optimal mapping for all nodes in before and after that have not yet been mapped in diff,
* but only using Insert, Delete and Update operations.
*
* Insert, Delete and Update operations do not cross across subtrees. This allows the problem to be
* decomposed into subproblems and solved independently. In comparison, the Move operation can move
* a node anywhere in the tree and so it prevents the solution for the entire AST to be decomposed
* in smaller subtrees.
*
* In this doccomment, N_b and N_a refer to a node in the before tree and node in the after tree
* respectively. Child_b_i refers to the i-th child node of the node in the before tree
* (N_b), and Child_a_j the j-th child node of the node in the after tree (N_a).
*
* The optimal algorithm uses the following observations.
*
* 1) Solution for (N_b, N_a) exists only if N_b and N_a have the same kind. Otherwise, the cost is
*    infinite because the solution is impossible.
* 2) If the nodes N_b and N_a have a value, if their values match then the cost of solving (N_b,
*    N_a) is a combination of solutions for their children. If the values differ, the solution is a
*    combination of children solutions plus the unit cost of an update.
* 3) Because AST nodes are ordered, solution for Child_b_i and Child_a_j will depend on solutions
*    for Child_b_0 to Child_b_(i-1) and Child_a_0 to Child_a_(j-1) but will not depend on nodes
*    after i-th child of N_b and j-th child of N_a. This means a dynamic programming solution is
*    possible.
*/
pub fn find(_before: &Code, _after: &Code, _diff: &mut ASTDiff) {
    // TODO: Implement optimal IUD (Insert, Update, Delete) algorithm
}

#[cfg(test)]
mod tests {
    use crate::test;
    use anyhow::Result;

    use crate::diff::COST_UPDATE;

    use super::*;

    #[test]
    #[ignore = "Functionality not yet implemented, pending refactoring"]
    fn test_find_optimal_solution_for_translated_hello_world() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;

        let before = test_codes.get("hello-world.rs").unwrap().clone();
        let after = test_codes.get("zdravo-svijete.rs").unwrap().clone();

        let before_metadata = crate::diff::compute_metadata(&before).unwrap_or_default();
        let after_metadata = crate::diff::compute_metadata(&after).unwrap_or_default();

        let mut diff = ASTDiff {
            before_metadata: Some(before_metadata.clone()),
            after_metadata: Some(after_metadata.clone()),
            ..Default::default()
        };

        find(&before, &after, &mut diff);

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

        // The cost should be COST_UPDATE (1)
        assert_eq!(
            mapping.cost, COST_UPDATE,
            "String content mapping cost should be COST_UPDATE"
        );

        Ok(())
    }
}
