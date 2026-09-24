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
use crate::diff::COST_INSERT;
use crate::diff::PassCtx;
use crate::diff::nodes::{anchor_pair_via_apted, kinds_update_allowed};
use crate::diff::{ASTDiff, ASTMapping, ASTMappingOperation, ASTMappingReason, COST_DELETE};

/// Strict bottom-up propagation. An unmatched before-node `B` with children resolves only when its
/// children force a single answer:
///
/// 1. Any undecided child: `B` is skipped.
/// 2. Every child deleted: `B` is `DeleteWithChildren` (mirrored as `InsertWithChildren`).
/// 3. Every child matched into the same direct after-parent `P`, `P` unmapped and the kinds
///    compatible per `kinds_update_allowed`: `B`/`P` is proposed to APTED via
///    `anchor_pair_via_apted`, so the cost and operation are never invented.
/// 4. Anything else (mixed children, disagreeing parents, incompatible kinds) blocks `B`: no
///    threshold, no vote.
///
/// Firing only on a fully consistent answer means it never leaves a guess for a later, more precise
/// pass to fight; a parent is validated by its children, never asserted over them.
pub fn solve(ctx: &PassCtx, diff: &mut ASTDiff) {
    let before_metadata = ctx.before_metadata();
    let after_metadata = ctx.after_metadata();
    let language = before_metadata.language;

    // Deepest first, so one call propagates several levels. Filtered before sorting so the sort is
    // proportional to the unresolved residual; safe because matching only adds entries and
    // `children` is immutable, so no dropped node could become eligible mid-loop.
    let mut before_candidates: Vec<usize> = before_metadata
        .node_to_depth
        .keys()
        .copied()
        .filter(|id| {
            !diff.before_node_map.contains_key(id)
                && before_metadata
                    .node_info
                    .get(id)
                    .is_some_and(|info| !info.children.is_empty())
        })
        .collect();
    sort_deepest_first(&mut before_candidates, before_metadata);

    for before_id in before_candidates {
        if diff.before_node_map.contains_key(&before_id) {
            continue;
        }
        let Some(before_info) = before_metadata.node_info.get(&before_id) else {
            continue;
        };
        if before_info.children.is_empty() {
            continue;
        }

        let mut all_deleted = true;
        let mut matched_after_parent: Option<usize> = None;
        let mut consistent = true;
        for &child_id in &before_info.children {
            match diff.before_node_map.get(&child_id) {
                None => {
                    consistent = false;
                    all_deleted = false;
                    break;
                }
                Some(&0) => {}
                Some(&after_child_id) => {
                    all_deleted = false;
                    let Some(&after_parent_id) = after_metadata.node_to_parent.get(&after_child_id)
                    else {
                        consistent = false;
                        break;
                    };
                    match matched_after_parent {
                        None => matched_after_parent = Some(after_parent_id),
                        Some(existing) if existing != after_parent_id => {
                            consistent = false;
                            break;
                        }
                        Some(_) => {}
                    }
                }
            }
        }
        if !consistent {
            continue;
        }

        if all_deleted {
            diff.add_mapping(
                before_id,
                0,
                ASTMapping {
                    cost: COST_DELETE,
                    operation: ASTMappingOperation::DeleteWithChildren,
                    reason: ASTMappingReason::BottomUpPropagation,
                },
            );
            continue;
        }

        let Some(after_id) = matched_after_parent else {
            continue;
        };
        if diff.after_node_map.contains_key(&after_id) {
            continue;
        }
        let Some(after_info) = after_metadata.node_info.get(&after_id) else {
            continue;
        };
        if before_info.kind != after_info.kind
            && !kinds_update_allowed(&before_info.kind, &after_info.kind, &language)
        {
            continue;
        }

        anchor_pair_via_apted(
            before_id,
            after_id,
            before_metadata,
            after_metadata,
            "bottom_up_propagation",
            ASTMappingReason::BottomUpPropagation,
            diff,
        );
    }

    // Only the insert case needs an after-side sweep: matching `B` to `P` above already sets both
    // sides.
    let mut after_candidates: Vec<usize> = after_metadata.node_to_depth.keys().copied().collect();
    sort_deepest_first(&mut after_candidates, after_metadata);

    for after_id in after_candidates {
        if diff.after_node_map.contains_key(&after_id) {
            continue;
        }
        let Some(after_info) = after_metadata.node_info.get(&after_id) else {
            continue;
        };
        if after_info.children.is_empty() {
            continue;
        }
        let all_inserted = after_info
            .children
            .iter()
            .all(|child_id| matches!(diff.after_node_map.get(child_id), Some(&0)));
        let any_undecided = after_info
            .children
            .iter()
            .any(|child_id| !diff.after_node_map.contains_key(child_id));
        if any_undecided || !all_inserted {
            continue;
        }

        diff.add_mapping(
            0,
            after_id,
            ASTMapping {
                cost: COST_INSERT,
                operation: ASTMappingOperation::InsertWithChildren,
                reason: ASTMappingReason::BottomUpPropagation,
            },
        );
    }
}

/// Deepest first, ties broken by `preorder_index`: raw node ids are not parse-stable.
fn sort_deepest_first(candidates: &mut [usize], metadata: &crate::code::ASTMetadata) {
    candidates.sort_by_cached_key(|&id| {
        let depth = metadata.node_to_depth.get(&id).copied().unwrap_or(0);
        let preorder = metadata
            .node_info
            .get(&id)
            .map(|i| i.preorder_index)
            .unwrap_or(0);
        (std::cmp::Reverse(depth), preorder)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::Code;
    use crate::code::Language;
    use crate::diff::NodeCache;

    fn solve_only(before: &Code, after: &Code) -> ASTDiff {
        let node_cache = NodeCache::build(before, after);
        let mut diff = ASTDiff::default();
        crate::diff::solve_hash_descent::solve(
            &crate::diff::PassCtx::new(before, after, &node_cache),
            &mut diff,
        );
        solve(
            &crate::diff::PassCtx::new(before, after, &node_cache),
            &mut diff,
        );
        diff
    }

    /// A function renamed (so its own hash never matches) but whose body is byte-identical to a
    /// counterpart elsewhere must be matched via propagation once all its children resolve.
    #[test]
    fn renamed_function_with_identical_body_matches_via_propagation() {
        let before = Code::from_string("fn old_name() { let q = 1 + 2; q }\n", &Language::Rust);
        let after = Code::from_string("fn new_name() { let q = 1 + 2; q }\n", &Language::Rust);

        let diff = solve_only(&before, &after);

        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();
        let before_fn = before_ast.root_node().child(0).unwrap();
        let after_fn = after_ast.root_node().child(0).unwrap();
        assert_eq!(
            diff.before_node_map.get(&before_fn.id()).copied(),
            Some(after_fn.id()),
            "the renamed function itself should match its counterpart, not delete+insert"
        );
    }

    /// Two children matching into *different* after-parents must not force a match - no
    /// plurality vote, no partial credit.
    #[test]
    fn disagreeing_after_parents_block_the_match() {
        let before = Code::from_string(
            "fn a() { 1; }\nfn b() { 2; }\nfn container() { 1; 2; }\n",
            &Language::Rust,
        );
        let after = Code::from_string("fn a() { 1; }\nfn b() { 2; }\n", &Language::Rust);

        let diff = solve_only(&before, &after);

        let before_ast = before.ast.as_ref().unwrap();
        let container = before_ast.root_node().child(2).unwrap();
        assert!(
            !diff.before_node_map.contains_key(&container.id()),
            "container's two statements match into two different before-functions' bodies, not \
             one - it must stay unmatched, not get force-matched to either"
        );
    }

    fn deletion() -> ASTMapping {
        ASTMapping {
            cost: COST_DELETE,
            operation: ASTMappingOperation::Delete,
            reason: ASTMappingReason::BottomUpPropagation,
        }
    }

    /// Runs only this pass on `x;`, with the given children of the `expression_statement`
    /// pre-marked deleted, and returns the statement's resulting mapping.
    fn statement_after_deleting(children: usize) -> Option<ASTMapping> {
        let before = Code::from_string("x;\n", &Language::Rust);
        let after = Code::from_string("\n", &Language::Rust);
        let node_cache = NodeCache::build(&before, &after);
        let statement = crate::test::helper::find_first_of_kind(
            before.ast.as_ref().unwrap().root_node(),
            "expression_statement",
        )
        .unwrap();
        let mut diff = ASTDiff::default();
        let mut cursor = statement.walk();
        for child in statement.children(&mut cursor).take(children) {
            diff.add_mapping(child.id(), 0, deletion());
        }
        solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );
        diff.mapping
            .iter()
            .find(|((b, _), _)| *b == statement.id())
            .map(|(_, m)| m.clone())
    }

    #[test]
    fn parent_of_only_deleted_children_is_deleted_with_children() {
        let mapping = statement_after_deleting(2).expect("statement should be decided");
        assert_eq!(mapping.operation, ASTMappingOperation::DeleteWithChildren);
    }

    #[test]
    fn parent_with_an_undecided_child_is_left_undecided() {
        assert!(statement_after_deleting(1).is_none());
    }
}
