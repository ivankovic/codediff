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
use anyhow::Result;
use std::collections::HashMap;
use tree_sitter::Node;

use crate::diff::{ASTDiff, ASTMapping, COST_UPDATE};
use crate::{code::Code, diff::ASTMappingOperation};

/**
* Find the optimal mapping for all nodes in before and after that have not yet been mapped in diff,
* but only using Insert, Delete and Update operations, and the "null operation" Identical that simply
* matches indentical nodes.
*
* The algorithm is complex and depends on many observations that seem disconnected at first.
*
* There are limits to operations:
*
* 1) Update operations are only possible on leaf nodes of the same kind.
* 2) Identical operation is only permitted on leaf nodes that have the same kind and value.
* 3) Identical operation is only permitted on intermediate nodes that have the same kind.
*
* The nodes in the AST are ordered. The subtrees of the AST are also ordered by using the ordering
* of their nodes. For nodes in different subtrees, we order them based on their ancestors, all the
* way to the root node if necessary.
*
* The solutions for distinct subtrees X and Y are always independent. This is because there is no
* set of operations that can cross independent subtrees. This is crucial because it allows us to
* decompose the problem, but it is also the reason why the Move operation cannot be allowed since
* it would break this requirement by allowing a subtree of X to move to Y.
*
* However, the diff can already contain Move operations, so the algorithm has to be robust to
* existence of Move operations, even if it is not allowed to add new ones. We do this by simply
* skipping any already mapped node. Note that we could use the computed cost for already mapped
* nodes, but it is not necessary because the mapping is symetrical, so the cost will be ignored
* both in the before and after tree for already mapped nodes. This works even if the mapping is
* outside of the subproblem.
*
* This leads to the following function: Cost([B], [A]) where [B] and [A] are lists of nodes in
* before and after, respectively. These nodes are the root nodes of their respective subtrees.
*
* Cost([B], [A]) = minimum between the following 4 choices, corresponding to the Identical, Update,
* Delete and Insert operations:
*
* 1. If the first node in [B] has the same kind and value as [A], map them as identical and
*    recursively call Cost([B] - first([B]), [A] - first([A])) plus add the
*    Cost(children(first[B])), children(first([A])).
* 2. If the first node in [B] has the same kind but NOT value as [A], map them as identical and
*    recursively call Cost([B] - first([B]), [A] - first([A])) plus add the
*    Cost(children(first[B])), children(first([A])) plus one COST_UPDATE.
* 3. For each node in [A], called [A_i], compute the cost of deleting first([B]) as if it's children
*    were nodes [A_0] to [A_i], not including [A_i]. Note that "no nodes" is the valid first
*    choice. For a given i, the cost is COST_DELETE plus Cost(children(first([B])), [A_0 to A_i]) +
*    Cost([B] - first([B]), [A_i to end]). Try for all values of i and keep the smallest result.
* 4. Similar to 3, but in [B] and using COST_INSERT to find the optimal insert operation for the
*    first node in [A].
*
* To reconstruct the actual operations, for each Cost([B], [A]), we need to keep the information on
* what was the minimal cost operation of the 4, and if it was an insert or delete, what was the
* optimal index for the operation.
*/
use crate::diff::{COST_DELETE, COST_INSERT, NodeCache};

pub fn find(before: &Code, after: &Code, node_cache: &NodeCache, diff: &mut ASTDiff) -> Result<()> {
    let mut memoo = HashMap::new();

    let before_root_id = before.ast.as_ref().unwrap().root_node().id();
    let after_root_id = after.ast.as_ref().unwrap().root_node().id();

    solve(
        before,
        after,
        vec![before_root_id],
        vec![after_root_id],
        node_cache,
        diff,
        &mut memoo,
    )?;

    update_diff(before, after, node_cache, &memoo, diff)?;

    Ok(())
}

/**
* Stores the solution for a specific list of subtrees.
*
* It has to keep enough information to allow for the diff to be reconstructed from the partial
* subtree solutions.
*/
#[derive(Clone)]
struct Solution {
    /// Cost of this solution
    cost: u64,
    /// Which operation is optimal for the first root nodes
    operation: ASTMappingOperation,
    /// If the operation is Insert or Delete, what was the optimal index
    index: usize,
}

impl Solution {
    fn new() -> Self {
        Solution {
            cost: 0,
            operation: ASTMappingOperation::NotYetSet,
            index: 0,
        }
    }
}

/**
* Counts the number of unmatched nodes for the given subtree.
*/
fn count_unmatched_nodes(
    node_id: usize,
    node_cache: &HashMap<usize, tree_sitter::Node<'static>>,
    mapped_nodes: &HashMap<usize, usize>,
) -> usize {
    // Get the node from cache
    let node = match node_cache.get(&node_id) {
        Some(n) => n,
        None => return 0,
    };

    let mut count = 0;

    if !mapped_nodes.contains_key(&node_id) {
        count += 1;
    }

    // Recursively count children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        count += count_unmatched_nodes(child.id(), node_cache, mapped_nodes);
    }

    count
}

/**
* Returns the ids of the child nodes.
*/
fn ids_of_children(node: &Node) -> Vec<usize> {
    let mut ids = Vec::new();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        ids.push(child.id());
    }

    ids
}

fn skip_matched_nodes(subtrees: &[usize], diff: &ASTDiff) -> (bool, usize) {
    let mut first_unmached_node_index = 0;
    let mut has_unmached_nodes = false;

    for (i, node_id) in subtrees.iter().enumerate() {
        if !diff.is_node_mapped(node_id) {
            has_unmached_nodes = true;
            first_unmached_node_index = i;
            break;
        }
    }

    (has_unmached_nodes, first_unmached_node_index)
}

/**
* This is the branch of the algorithm that we choose based on the current state.
*
* We want to re-use exactly the same logic for solving the problem and for reconstructing the
* solution so we need this enum to tell us what we did.
*/
#[derive(Debug, Clone, PartialEq)]
enum AlgorithmChoice {
    EmptyState,
    AllNodesMatched,
    SkipMatchedNodes,
    InsertAll,
    DeleteAll,
    SolveFirstRoots,
}

fn choose_algorithm_branch(
    before_subtrees: &[usize],
    after_subtrees: &[usize],
    before_has_unmached_nodes: bool,
    before_first_unmached_node_index: usize,
    after_has_unmached_nodes: bool,
    after_first_unmached_node_index: usize,
) -> AlgorithmChoice {
    if before_subtrees.is_empty() && after_subtrees.is_empty() {
        return AlgorithmChoice::EmptyState;
    } else if !before_has_unmached_nodes && !after_has_unmached_nodes {
        return AlgorithmChoice::AllNodesMatched;
    } else if before_subtrees.is_empty() || !before_has_unmached_nodes {
        return AlgorithmChoice::InsertAll;
    } else if after_subtrees.is_empty() || !after_has_unmached_nodes {
        return AlgorithmChoice::DeleteAll;
    } else if before_first_unmached_node_index != 0 || after_first_unmached_node_index != 0 {
        return AlgorithmChoice::SkipMatchedNodes;
    }

    AlgorithmChoice::SolveFirstRoots
}

/**
* Recursively solve the subtree mapping problem using only Insert, Delete, Update and Identical
* operations.
*
* The function returns the cost, but the actual mapping can be reconstructed using the memoo
* memooization map.
*/
fn solve(
    before: &Code,
    after: &Code,
    before_subtrees: Vec<usize>,
    after_subtrees: Vec<usize>,
    node_cache: &NodeCache,
    diff: &ASTDiff,
    memoo: &mut HashMap<(Vec<usize>, Vec<usize>), Solution>,
) -> Result<u64> {
    // If both subtrees are empty, there is nothing to do.
    if before_subtrees.is_empty() && after_subtrees.is_empty() {
        return Ok(0);
    }

    // Check if memoo already has the solution for this input and return that.
    if let Some(solution) = memoo.get(&(before_subtrees.clone(), after_subtrees.clone())) {
        return Ok(solution.cost);
    }
    let mut result = Solution::new();

    let (before_has_unmached_nodes, before_first_unmached_node_index) =
        skip_matched_nodes(&before_subtrees, diff);
    let (after_has_unmached_nodes, after_first_unmached_node_index) =
        skip_matched_nodes(&after_subtrees, diff);

    let algorithm_branch = choose_algorithm_branch(
        &before_subtrees,
        &after_subtrees,
        before_has_unmached_nodes,
        before_first_unmached_node_index,
        after_has_unmached_nodes,
        after_first_unmached_node_index,
    );

    match algorithm_branch {
        AlgorithmChoice::EmptyState | AlgorithmChoice::AllNodesMatched => {
            result.cost = 0;
            result.operation = ASTMappingOperation::DoNothing;
        }
        AlgorithmChoice::SkipMatchedNodes => {
            result.cost = solve(
                before,
                after,
                before_subtrees[before_first_unmached_node_index..].to_vec(),
                after_subtrees[after_first_unmached_node_index..].to_vec(),
                node_cache,
                diff,
                memoo,
            )?;
            result.operation = ASTMappingOperation::DoNothing;
        }
        AlgorithmChoice::InsertAll => {
            let mut total_cost = 0;
            for &after_id in &after_subtrees {
                let unmatched =
                    count_unmatched_nodes(after_id, &node_cache.after, &diff.after_node_map);
                total_cost += unmatched as u64 * COST_INSERT;
            }
            result.cost = total_cost;
            result.operation = ASTMappingOperation::InsertWithChildren;
        }
        AlgorithmChoice::DeleteAll => {
            let mut total_cost = 0;
            for &before_id in &before_subtrees {
                let unmatched =
                    count_unmatched_nodes(before_id, &node_cache.before, &diff.before_node_map);
                total_cost += unmatched as u64 * COST_DELETE;
            }
            result.cost = total_cost;
            result.operation = ASTMappingOperation::DeleteWithChildren;
        }
        AlgorithmChoice::SolveFirstRoots => {
            let before_first_node = node_cache
                .before
                .get(&before_subtrees[0])
                .ok_or_else(|| anyhow::anyhow!("Node not found in cache"))?;
            let after_first_node = node_cache
                .after
                .get(&after_subtrees[0])
                .ok_or_else(|| anyhow::anyhow!("Node not found in cache"))?;

            // The cost if we match the first roots
            let mut solution_if_match = Solution::new();
            solution_if_match.cost = u64::MAX;

            if before_first_node.kind() == after_first_node.kind() {
                let mut cost = 0;

                if before_first_node.child_count() == 0 && after_first_node.child_count() == 0 {
                    let before_text = before_first_node.utf8_text(before.contents.as_bytes());
                    let after_text = after_first_node.utf8_text(after.contents.as_bytes());

                    if before_text == after_text {
                        solution_if_match.operation = ASTMappingOperation::Identical;
                    } else {
                        solution_if_match.operation = ASTMappingOperation::Update;
                        cost += COST_UPDATE;
                    }
                } else {
                    solution_if_match.operation = ASTMappingOperation::Identical;
                }
                cost += solve(
                    before,
                    after,
                    before_subtrees[1..].to_vec(),
                    after_subtrees[1..].to_vec(),
                    node_cache,
                    diff,
                    memoo,
                )? + solve(
                    before,
                    after,
                    ids_of_children(before_first_node),
                    ids_of_children(after_first_node),
                    node_cache,
                    diff,
                    memoo,
                )?;
                solution_if_match.cost = cost;
            }

            // The cost if we delete the first root in before
            // We need to check all possible subsequences of root nodes in after_subtrees, including
            // the empty set, to check which is the optimal number of nodes in after_subtrees to match
            // with the children of the first root node in before_subtrees.
            let mut solution_if_delete = Solution::new();
            solution_if_delete.operation = ASTMappingOperation::Delete;
            solution_if_delete.cost = u64::MAX;

            for i in 0..=after_subtrees.len() {
                let mut cost = COST_DELETE;

                cost += solve(
                    before,
                    after,
                    before_subtrees[1..].to_vec(),
                    after_subtrees[i..].to_vec(),
                    node_cache,
                    diff,
                    memoo,
                )? + solve(
                    before,
                    after,
                    ids_of_children(before_first_node),
                    after_subtrees[..i].to_vec(),
                    node_cache,
                    diff,
                    memoo,
                )?;

                if cost < solution_if_delete.cost {
                    solution_if_delete.cost = cost;
                    solution_if_delete.index = i;
                }
            }

            // The cost if we insert the first root in after
            // We need to check all possible subsequences of root nodes in before_subtrees, including
            // the empty set, to check which is the optimal number of nodes in before_subtrees to match
            // with the children of the first root node in after_subtrees.
            let mut solution_if_insert = Solution::new();
            solution_if_insert.operation = ASTMappingOperation::Insert;
            solution_if_insert.cost = u64::MAX;

            for i in 0..=before_subtrees.len() {
                let mut cost = COST_INSERT;

                cost += solve(
                    before,
                    after,
                    before_subtrees[i..].to_vec(),
                    after_subtrees[1..].to_vec(),
                    node_cache,
                    diff,
                    memoo,
                )? + solve(
                    before,
                    after,
                    before_subtrees[..i].to_vec(),
                    ids_of_children(after_first_node),
                    node_cache,
                    diff,
                    memoo,
                )?;

                if cost < solution_if_insert.cost {
                    solution_if_insert.cost = cost;
                    solution_if_insert.index = i;
                }
            }

            // Pick the cheapest of the tree costs, that is our final result.
            //
            // There is a subtle preference here, if the costs are exactly equal, we prefer matching
            // and if delete and insert are both equal and cheaper than match, we prefer a delete.
            //
            // This is based on how the diff is displayed to humans and personal human preference of
            // the author.
            if solution_if_match.cost <= solution_if_delete.cost {
                if solution_if_match.cost <= solution_if_insert.cost {
                    result = solution_if_match;
                } else {
                    result = solution_if_insert;
                }
            } else if solution_if_delete.cost <= solution_if_insert.cost {
                result = solution_if_delete;
            } else {
                result = solution_if_insert;
            }
        }
    }

    // Insert the solution into memoo with the before and after subtrees as the key.
    memoo.insert((before_subtrees, after_subtrees), result.clone());
    Ok(result.cost)
}

fn add_subtree_to_diff(
    node_ids: Vec<usize>,
    operation: &ASTMappingOperation,
    cost_of_one_operation: u64,
    node_cache: &NodeCache,
    diff: &mut ASTDiff,
) -> Result<u64> {
    let mut total_cost = 0;

    for node_id in node_ids {
        let node = node_cache
            .get_in_any(&node_id)
            .ok_or_else(|| anyhow::anyhow!("Node not found in cache"))?;

        let children_cost = add_subtree_to_diff(
            ids_of_children(node),
            operation,
            cost_of_one_operation,
            node_cache,
            diff,
        )?;

        let node_cost = cost_of_one_operation + children_cost;
        total_cost += node_cost;

        let mapping = ASTMapping {
            cost: node_cost,
            operation: operation.clone(),
            reason: super::ASTMappingReason::OptimalIDU,
        };

        if *operation == ASTMappingOperation::InsertWithChildren {
            if !diff.after_node_map.contains_key(&node_id) {
                diff.add_mapping(0, node_id, mapping);
            }
        } else if !diff.before_node_map.contains_key(&node_id) {
            diff.add_mapping(node_id, 0, mapping);
        }
    }

    Ok(total_cost)
}

/**
* Update the diff using the solution stored in memoo.
*
* Note that this function is linear in the number of nodes, since it knows exactly which path to
* choose because of the already computed memoo map.
*/
fn update_diff(
    before: &Code,
    after: &Code,
    node_cache: &NodeCache,
    memoo: &HashMap<(Vec<usize>, Vec<usize>), Solution>,
    diff: &mut ASTDiff,
) -> Result<()> {
    let mut stack = Vec::new();

    let before_root_id = before.ast.as_ref().unwrap().root_node().id();
    let after_root_id = after.ast.as_ref().unwrap().root_node().id();

    stack.push((vec![before_root_id], vec![after_root_id]));

    while let Some((before_subtrees, after_subtrees)) = stack.pop() {
        if before_subtrees.is_empty() && after_subtrees.is_empty() {
            continue;
        }

        let (before_has_unmached_nodes, before_first_unmached_node_index) =
            skip_matched_nodes(&before_subtrees, diff);
        let (after_has_unmached_nodes, after_first_unmached_node_index) =
            skip_matched_nodes(&after_subtrees, diff);

        let algorithm_branch = choose_algorithm_branch(
            &before_subtrees,
            &after_subtrees,
            before_has_unmached_nodes,
            before_first_unmached_node_index,
            after_has_unmached_nodes,
            after_first_unmached_node_index,
        );

        let solution = memoo
            .get(&(before_subtrees.clone(), after_subtrees.clone()))
            .ok_or_else(|| anyhow::anyhow!("Solution for a subproblem not found"))?;
        let mapping = ASTMapping {
            cost: solution.cost,
            operation: solution.operation.clone(),
            reason: super::ASTMappingReason::OptimalIDU,
        };

        match algorithm_branch {
            AlgorithmChoice::EmptyState | AlgorithmChoice::AllNodesMatched => {
                continue;
            }
            AlgorithmChoice::SkipMatchedNodes => {
                stack.push((
                    before_subtrees[before_first_unmached_node_index..].to_vec(),
                    after_subtrees[after_first_unmached_node_index..].to_vec(),
                ));
            }
            AlgorithmChoice::InsertAll => {
                // Terminal branch, nothing to add to the stack.
                add_subtree_to_diff(
                    after_subtrees,
                    &solution.operation,
                    COST_INSERT,
                    node_cache,
                    diff,
                )?;
            }
            AlgorithmChoice::DeleteAll => {
                add_subtree_to_diff(
                    before_subtrees,
                    &solution.operation,
                    COST_DELETE,
                    node_cache,
                    diff,
                )?;
                // Terminal branch, nothing to add to the stack.
            }
            AlgorithmChoice::SolveFirstRoots => {
                let before_first_node = node_cache
                    .before
                    .get(&before_subtrees[0])
                    .ok_or_else(|| anyhow::anyhow!("Node not found in cache"))?;
                let after_first_node = node_cache
                    .after
                    .get(&after_subtrees[0])
                    .ok_or_else(|| anyhow::anyhow!("Node not found in cache"))?;
                match solution.operation {
                    ASTMappingOperation::Identical | ASTMappingOperation::Update => {
                        diff.add_mapping(before_subtrees[0], after_subtrees[0], mapping);
                        if before_subtrees.len() > 1 || after_subtrees.len() > 1 {
                            stack.push((
                                before_subtrees[1..].to_vec(),
                                after_subtrees[1..].to_vec(),
                            ));
                        }

                        stack.push((
                            ids_of_children(before_first_node),
                            ids_of_children(after_first_node),
                        ));
                    }
                    ASTMappingOperation::Insert => {
                        diff.add_mapping(0, after_subtrees[0], mapping);

                        stack.push((
                            before_subtrees[solution.index..].to_vec(),
                            after_subtrees[1..].to_vec(),
                        ));
                        stack.push((
                            before_subtrees[..solution.index].to_vec(),
                            ids_of_children(after_first_node),
                        ));
                    }
                    ASTMappingOperation::Delete => {
                        diff.add_mapping(before_subtrees[0], 0, mapping);

                        stack.push((
                            before_subtrees[1..].to_vec(),
                            after_subtrees[solution.index..].to_vec(),
                        ));
                        stack.push((
                            ids_of_children(before_first_node),
                            after_subtrees[..solution.index].to_vec(),
                        ));
                    }
                    _ => {
                        unreachable!("Invalid operation for first root solution");
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        diff::{ASTMappingReason, COST_DELETE, COST_INSERT},
        test,
    };
    use anyhow::Result;

    use crate::diff::COST_UPDATE;

    use super::*;

    #[test]
    fn test_count_unmatched_nodes() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;
        let code = test_codes.get("hello-world.rs").unwrap().clone();

        let root_id = code.ast.as_ref().unwrap().root_node().id();

        let node_cache = NodeCache::build(&code, &code);
        let mut diff = ASTDiff {
            ..Default::default()
        };

        assert_eq!(
            count_unmatched_nodes(root_id, &node_cache.before, &diff.before_node_map),
            22
        );

        diff.add_mapping(
            root_id,
            root_id,
            crate::diff::ASTMapping {
                cost: 0,
                operation: ASTMappingOperation::Identical,
                reason: crate::diff::ASTMappingReason::IdenticalHash,
            },
        );

        assert_eq!(
            count_unmatched_nodes(root_id, &node_cache.before, &diff.before_node_map),
            21
        );

        Ok(())
    }

    #[test]
    fn skip_matched_nodes_test() -> Result<()> {
        let test_diffs = test::helper::handmade_test_diffs()?;
        let (_, after) = test_diffs.get("hello-world-added-message").unwrap().clone();

        let diff = ASTDiff {
            ..Default::default()
        };

        let after_ast = after.ast.unwrap();

        let added_subtree = test::helper::node_for_path(
            after_ast.root_node(),
            vec!["function_item", "block", "expression_statement:2"],
        )?;
        let addedd_subtree_root_id = added_subtree.id();

        let (has_unmached_nodes, first_unmached_node_index) =
            skip_matched_nodes(&[addedd_subtree_root_id], &diff);

        assert!(has_unmached_nodes);
        assert_eq!(first_unmached_node_index, 0);

        Ok(())
    }

    #[test]
    fn solve_for_hello_world_translation() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;

        let before = test_codes.get("hello-world.rs").unwrap().clone();
        let after = test_codes.get("zdravo-svijete.rs").unwrap().clone();

        let diff = ASTDiff {
            ..Default::default()
        };

        let mut memoo = HashMap::new();
        let node_cache = NodeCache::build(&before, &after);

        let before_root_id = before.ast.as_ref().unwrap().root_node().id();
        let after_root_id = after.ast.as_ref().unwrap().root_node().id();

        let total_cost = solve(
            &before,
            &after,
            vec![before_root_id],
            vec![after_root_id],
            &node_cache,
            &diff,
            &mut memoo,
        )?;

        assert_eq!(total_cost, 1);

        Ok(())
    }

    #[test]
    fn solve_for_hello_world_added_message() -> Result<()> {
        let test_diffs = test::helper::handmade_test_diffs()?;
        let (before, after) = test_diffs.get("hello-world-added-message").unwrap().clone();

        let diff = ASTDiff {
            ..Default::default()
        };

        let mut memoo = HashMap::new();
        let node_cache = NodeCache::build(&before, &after);

        let before_root_id = before.ast.as_ref().unwrap().root_node().id();
        let after_root_id = after.ast.as_ref().unwrap().root_node().id();

        // The optimal solution is to add 12 new nodes.
        // To find this, the algorithm should find the following optimal branch:
        //
        // ([source_file], [source_file])
        //      -> Branch: SolveFirstRoots Operation: Identical
        //
        // ([function_item], [function_item])
        //      -> Branch: SolveFirstRoots Operation: Identical
        //
        // ([fn, identifier, params, block], [fn, identifier, params, block])
        //      -> Branch: SolveFirstRoots Operation: Identical
        //
        // ([identifier, params, block], [identifier, params, block])
        //      -> Branch: SolveFirstRoots Operation: Identical
        //
        // ([params, block], [params, block])
        //      -> Branch: SolveFirstRoots Operation: Identical
        //
        // ([block], [block])
        //      -> Branch: SolveFirstRoots Operation: Identical
        //
        // ([{, expression_statement, }], [{, expression_statement, expression_statement, }])
        //      -> Branch: SolveFirstRoots Operation: Identical
        //
        // ([expression_statement, }], [expression_statement, expression_statement, }])
        //      -> Branch: SolveFirstRoots Operation: Identical
        //
        // ([}], [expression_statement, }])
        //      -> Branch: SolveFirstRoots Operation: Insert Index: 0
        //
        // ([], [macro_invocation, ;])
        //      -> Branch: InsertAll Operation: InsertWithChildren
        //
        // ([}], [}])
        //      -> Branch: SolveFirstRoots Operation: Identical
        //
        // Of course it will explore other sub-problems too, but they should all be more expensive.

        let total_cost = solve(
            &before,
            &after,
            vec![before_root_id],
            vec![after_root_id],
            &node_cache,
            &diff,
            &mut memoo,
        )?;

        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        // First check that the subproblem where before is [] and after is the
        // expression_statement node that as added, and it's subtree, was solved
        let added_subtree = test::helper::node_for_path(
            after_ast.root_node(),
            vec!["function_item", "block", "expression_statement:2"],
        )?;
        let addedd_subtree_root_id = added_subtree.id();

        let solution = memoo
            .get(&(Vec::new(), vec![addedd_subtree_root_id]))
            .unwrap();
        assert_eq!(solution.operation, ASTMappingOperation::InsertWithChildren);
        assert_eq!(solution.cost, 12);

        // Now we work backwards as explained in the comment above...
        let before_closing_bracket = test::helper::node_for_path(
            before_ast.root_node(),
            vec!["function_item", "block", "}"],
        )?;
        let before_closing_bracket_id = before_closing_bracket.id();

        let after_closing_bracket = test::helper::node_for_path(
            after_ast.root_node(),
            vec!["function_item", "block", "}"],
        )?;
        let after_closing_bracket_id = after_closing_bracket.id();

        let solution = memoo
            .get(&(
                vec![before_closing_bracket_id],
                vec![addedd_subtree_root_id, after_closing_bracket_id],
            ))
            .unwrap();
        assert_eq!(solution.cost, 12);
        assert_eq!(solution.index, 0);
        assert_eq!(solution.operation, ASTMappingOperation::Insert);

        // Finally, check the return value
        assert_eq!(total_cost, 12);

        Ok(())
    }

    #[test]
    fn solve_for_hello_world_deleted_message() -> Result<()> {
        let test_diffs = test::helper::handmade_test_diffs()?;
        // Swap before and after around to turn an add into a delete.
        let (after, before) = test_diffs.get("hello-world-added-message").unwrap().clone();

        let diff = ASTDiff {
            ..Default::default()
        };

        let mut memoo = HashMap::new();
        let node_cache = NodeCache::build(&before, &after);

        let before_root_id = before.ast.as_ref().unwrap().root_node().id();
        let after_root_id = after.ast.as_ref().unwrap().root_node().id();

        let total_cost = solve(
            &before,
            &after,
            vec![before_root_id],
            vec![after_root_id],
            &node_cache,
            &diff,
            &mut memoo,
        )?;

        assert_eq!(total_cost, 12);

        Ok(())
    }

    #[test]
    fn solve_for_leetcode_1_bugfix() -> Result<()> {
        let test_diffs = test::helper::handmade_test_diffs()?;
        // Swap before and after around to turn an add into a delete.
        let (after, before) = test_diffs.get("leet-code-1-bugfix").unwrap().clone();

        let diff = ASTDiff {
            ..Default::default()
        };

        let mut memoo = HashMap::new();
        let node_cache = NodeCache::build(&before, &after);

        let before_root_id = before.ast.as_ref().unwrap().root_node().id();
        let after_root_id = after.ast.as_ref().unwrap().root_node().id();

        let total_cost = solve(
            &before,
            &after,
            vec![before_root_id],
            vec![after_root_id],
            &node_cache,
            &diff,
            &mut memoo,
        )?;

        // 41 new nodes are added
        assert_eq!(total_cost, 41);

        Ok(())
    }

    #[test]
    fn is_always_valid() -> Result<()> {
        let test_diffs = test::helper::handmade_test_diffs()?;

        for (diff_name, (before, after)) in test_diffs {
            let mut diff = ASTDiff {
                ..Default::default()
            };

            let node_cache = NodeCache::build(&before, &after);
            find(&before, &after, &node_cache, &mut diff)?;

            assert!(
                diff.is_valid(&before, &after, &node_cache),
                "Real diff mappings should always be valid for diff: {}",
                diff_name
            );
            assert!(
                diff.is_complete(&before, &after, &node_cache),
                "Real diff mappings should always be complete for diff: {}",
                diff_name
            );
        }

        Ok(())
    }

    #[test]
    fn cost_should_be_the_same_if_code_is_switched_around() -> Result<()> {
        let test_diffs = test::helper::handmade_test_diffs()?;

        for (diff_name, (before, after)) in test_diffs {
            let b = before.clone();
            let a = after.clone();

            let before_root_id = before.ast.as_ref().unwrap().root_node().id();
            let after_root_id = after.ast.as_ref().unwrap().root_node().id();

            let mut b_a_diff = ASTDiff {
                ..Default::default()
            };
            let node_cache_ba = NodeCache::build(&b, &a);
            find(&b, &a, &node_cache_ba, &mut b_a_diff)?;

            let mut a_b_diff = ASTDiff {
                ..Default::default()
            };
            let node_cache_ab = NodeCache::build(&a, &b);
            find(&a, &b, &node_cache_ab, &mut a_b_diff)?;

            let b_a_root_mapping = b_a_diff
                .mapping
                .get(&(before_root_id, after_root_id))
                .unwrap_or_else(|| panic!("Root node should be mapped for diff: {}", diff_name));
            let a_b_root_mapping = a_b_diff
                .mapping
                .get(&(after_root_id, before_root_id))
                .unwrap_or_else(|| panic!("Root node should be mapped for diff: {}", diff_name));

            assert_eq!(
                b_a_root_mapping.cost, a_b_root_mapping.cost,
                "Cost must be the same independent from which code is before and which is after: {}",
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

        let node_cache = NodeCache::build(&before, &after);
        find(&before, &after, &node_cache, &mut diff)?;

        assert!(
            diff.is_valid(&before, &after, &node_cache),
            "Real diff mappings should always be valid"
        );
        assert!(
            diff.is_complete(&before, &after, &node_cache),
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

        // The added line has 12 nodes, starting with an expression_statement as the root of the
        // subtree.
        assert_eq!(
            added_expression_node_mapping.cost,
            COST_INSERT * 12,
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

        let node_cache = NodeCache::build(&before, &after);
        find(&before, &after, &node_cache, &mut diff)?;

        assert!(
            diff.is_valid(&before, &after, &node_cache),
            "Real diff mappings should always be valid"
        );
        assert!(
            diff.is_complete(&before, &after, &node_cache),
            "Real diff mappings should always be complete"
        );

        let before_ast = before.ast.unwrap();

        let deleted_expression_node = test::helper::node_for_path(
            before_ast.root_node(),
            vec!["function_item", "block", "expression_statement:2"],
        )?;

        let deleted_expression_node_mapping = diff.mapping.get(&(deleted_expression_node.id(), 0));
        assert!(
            deleted_expression_node_mapping.is_some(),
            "The node that represents the deleted line is not mapped as a deleted node"
        );
        let deleted_expression_node_mapping = deleted_expression_node_mapping.unwrap();

        assert_eq!(
            deleted_expression_node_mapping.cost,
            COST_DELETE * 12,
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

        let node_cache = NodeCache::build(&before, &after);
        find(&before, &after, &node_cache, &mut diff)?;

        assert!(
            diff.is_valid(&before, &after, &node_cache),
            "Real diff mappings should always be valid"
        );
        assert!(
            diff.is_complete(&before, &after, &node_cache),
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

        let node_cache = NodeCache::build(&before, &after);
        find(&before, &after, &node_cache, &mut diff)?;

        assert!(
            diff.is_valid(&before, &after, &node_cache),
            "Real diff mappings should always be valid"
        );
        assert!(
            diff.is_complete(&before, &after, &node_cache),
            "Real diff mappings should always be complete"
        );

        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let deleted_if_node = test::helper::node_for_path(
            before_ast.root_node(),
            vec!["if_statement", "block", "if_statement"],
        )?;

        let deleted_if_node_mapping = diff.mapping.get(&(deleted_if_node.id(), 0));
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
            after_ast.root_node(),
            vec!["if_statement", "block", "expression_statement:4"],
        )?;

        let expression_node_in_before_id = diff
            .after_node_map
            .get(&existing_expression_node.id())
            .expect("The un-indented expression is not found in the diff");

        let existing_expression_node_mapping = diff
            .mapping
            .get(&(*expression_node_in_before_id, existing_expression_node.id()));

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

        let node_cache = NodeCache::build(&before, &after);
        find(&before, &after, &node_cache, &mut diff)?;

        assert!(diff.is_valid(&before, &after, &node_cache));
        assert!(diff.is_complete(&before, &after, &node_cache));

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

    #[test]
    fn leetcode_1_bugfix() -> Result<()> {
        let test_diffs = test::helper::handmade_test_diffs()?;
        // Swap before and after around to turn an add into a delete.
        let (after, before) = test_diffs.get("leet-code-1-bugfix").unwrap().clone();

        let mut diff = ASTDiff {
            ..Default::default()
        };

        let node_cache = NodeCache::build(&before, &after);
        find(&before, &after, &node_cache, &mut diff)?;

        assert!(diff.is_valid(&before, &after, &node_cache));
        assert!(diff.is_complete(&before, &after, &node_cache));

        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let before_root_id = before_ast.root_node().id();
        let after_root_id = after_ast.root_node().id();

        // Check that root node has IdenticalHash reason
        let root_mapping = diff
            .mapping
            .get(&(before_root_id, after_root_id))
            .expect("Root node should be mapped");
        assert_eq!(root_mapping.reason, ASTMappingReason::OptimalIDU,);

        // A fully identical code can never have a cost.
        assert_eq!(root_mapping.cost, 41);

        Ok(())
    }
}
