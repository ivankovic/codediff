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
use crate::diff::PassCtx;
use crate::diff::{ASTDiff, ASTMapping, ASTMappingOperation, ASTMappingReason};

/// Every after-side node's index in its parent's child list. A per-candidate `position` scan is
/// quadratic on nodes with tens of thousands of children
/// (css-shadcn-ui-ui-completely-broken-treesitter-parsing).
fn after_child_indices(
    after_metadata: &crate::code::ASTMetadata,
) -> rustc_hash::FxHashMap<usize, usize> {
    let mut indices = rustc_hash::FxHashMap::default();
    for info in after_metadata.node_info.values() {
        for (index, &child) in info.children.iter().enumerate() {
            indices.insert(child, index);
        }
    }
    indices
}

/// The one after-side node between `previous` and `next` when they are adjacent-but-one under a
/// single parent; `None` otherwise.
fn between(
    previous: usize,
    next: usize,
    after_metadata: &crate::code::ASTMetadata,
    after_child_indices: &rustc_hash::FxHashMap<usize, usize>,
) -> Option<usize> {
    let parent = after_metadata.node_to_parent.get(&previous)?;
    if after_metadata.node_to_parent.get(&next)? != parent {
        return None;
    }
    let siblings = &after_metadata.node_info.get(parent)?.children;
    let previous_index = *after_child_indices.get(&previous)?;
    if siblings.get(previous_index + 2) != Some(&next) {
        return None;
    }
    siblings.get(previous_index + 1).copied()
}

/// Re-points a leaf matched to an identical twin in the wrong place when its neighbours name the
/// right one. A left-nested chain growing at its outer end (`a || b` to `a || b || c`, `x.f()` to
/// `x.f().g()`) leaves same-text tokens whose pairing by depth or by position costs the same; the
/// search reports depth, every human reads position (java-defects4j-closure-147-checkglobalthis).
///
/// A leaf moves only when both neighbours are matched, their partners are adjacent-but-one under
/// one parent, and the node between them is free with the same kind and text. Single-sided evidence
/// false-positives; an inserted argument (`f(a, b)` to `f(a, X, b)`) puts the partners three apart;
/// and same text keeps the swap cost-neutral, so it only picks another member of a set of optima.
pub fn solve(ctx: &PassCtx, diff: &mut ASTDiff) {
    let before_metadata = ctx.before_metadata();
    let after_metadata = ctx.after_metadata();

    // Parent by parent, so a leaf's sibling index comes free; in preorder for determinism.
    let mut parents: Vec<usize> = before_metadata
        .node_info
        .iter()
        .filter_map(|(&id, info)| (info.children.len() >= 3).then_some(id))
        .collect();
    parents.sort_unstable_by_key(|id| {
        before_metadata
            .node_info
            .get(id)
            .map(|info| info.preorder_index)
            .unwrap_or(usize::MAX)
    });

    let mut after_indices = None;

    for parent in parents {
        let Some(siblings) = before_metadata
            .node_info
            .get(&parent)
            .map(|info| &info.children)
        else {
            continue;
        };
        for index in 1..siblings.len().saturating_sub(1) {
            let leaf = siblings[index];
            if before_metadata
                .node_info
                .get(&leaf)
                .is_none_or(|info| !info.children.is_empty())
            {
                continue;
            }
            let partner_of = |id: usize| match diff.before_node_map.get(&id) {
                Some(&partner) if partner != 0 => Some(partner),
                _ => None,
            };
            let (Some(partner), Some(previous), Some(next)) = (
                partner_of(leaf),
                partner_of(siblings[index - 1]),
                partner_of(siblings[index + 1]),
            ) else {
                continue;
            };
            // Anything but `Identical` is a pairing the search paid for; moving it overrules the
            // cost model rather than settling a tie.
            if !diff
                .mapping
                .get(&(leaf, partner))
                .is_some_and(|entry| entry.operation == ASTMappingOperation::Identical)
            {
                continue;
            }
            let after_indices =
                after_indices.get_or_insert_with(|| after_child_indices(after_metadata));
            let Some(target) = between(previous, next, after_metadata, after_indices) else {
                continue;
            };
            if target == partner {
                continue;
            }
            // An explicit insert (`Some(&0)`) and an undecided node (`None`) are both free.
            if diff
                .after_node_map
                .get(&target)
                .is_some_and(|&claimed| claimed != 0)
            {
                continue;
            }
            let (Some(leaf_info), Some(target_info)) = (
                before_metadata.node_info.get(&leaf),
                after_metadata.node_info.get(&target),
            ) else {
                continue;
            };
            if leaf_info.kind != target_info.kind || leaf_info.text != target_info.text {
                continue;
            }

            // The old partner is left undecided for `solve_unresolved_nodes`, which runs after this.
            diff.remove_match_mapping(leaf, partner);
            diff.remove_insert_mapping(target);
            diff.add_mapping(
                leaf,
                target,
                ASTMapping {
                    cost: 0,
                    operation: ASTMappingOperation::Identical,
                    reason: ASTMappingReason::LeafBetweenMatchedNeighbours,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::{Code, Language};
    use crate::diff::{NodeCache, diff_code};

    /// The `n`th leaf reading `text`, in document order.
    fn nth_leaf(code: &Code, text: &str, n: usize) -> usize {
        let mut found = Vec::new();
        let mut stack = vec![code.ast.as_ref().unwrap().root_node()];
        while let Some(node) = stack.pop() {
            if node.child_count() == 0 && code.contents.get(node.byte_range()) == Some(text) {
                found.push((node.start_byte(), node.id()));
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
        }
        found.sort_unstable();
        found[n].1
    }

    /// Java parses `a || b || c` left-nested, so the new `||` is the outermost one.
    #[test]
    fn an_operator_chain_that_grew_keeps_each_existing_token_where_it_was() {
        let before = Code::from_string(
            "class C { boolean f() { return a == 1 || a == 2 || a == 3; } }",
            &Language::Java,
        );
        let after = Code::from_string(
            "class C { boolean f() { return a == 1 || a == 2 || a == 3 || a == 4; } }",
            &Language::Java,
        );

        let diff = diff_code(&before, &after);
        let ast = diff.ast.as_ref().unwrap();

        for index in 0..2 {
            assert_eq!(
                ast.before_node_map.get(&nth_leaf(&before, "||", index)),
                Some(&nth_leaf(&after, "||", index)),
                "the `||` at position {index} is the same `||` on both sides",
            );
        }
        // The one the reader actually added is the one reported as new.
        assert_eq!(ast.after_node_map.get(&nth_leaf(&after, "||", 2)), Some(&0),);
    }

    /// `x.f().g()` nests the old invocation inside the new one, a level deeper.
    #[test]
    fn an_invocation_chain_that_grew_keeps_its_existing_dot() {
        let before = Code::from_string("class C { void f() { x.f(); } }", &Language::Java);
        let after = Code::from_string("class C { void f() { x.f().g(); } }", &Language::Java);

        let diff = diff_code(&before, &after);
        let ast = diff.ast.as_ref().unwrap();

        assert_eq!(
            ast.before_node_map.get(&nth_leaf(&before, ".", 0)),
            Some(&nth_leaf(&after, ".", 0)),
        );
        assert_eq!(ast.after_node_map.get(&nth_leaf(&after, ".", 1)), Some(&0));
    }

    /// Runs only this pass over a hand-built mapping, so a result can only be this pass's doing.
    fn solve_over(before: &Code, after: &Code, pairs: &[(usize, usize)]) -> ASTDiff {
        let node_cache = NodeCache::build(before, after);
        let mut diff = ASTDiff::default();
        for &(before_id, after_id) in pairs {
            diff.add_mapping(
                before_id,
                after_id,
                ASTMapping::identical(ASTMappingReason::IdenticalHash),
            );
        }
        solve(
            &crate::diff::PassCtx::new(before, after, &node_cache),
            &mut diff,
        );
        diff
    }

    /// The neighbours' partners are three apart, so "between them" names nothing.
    #[test]
    fn a_comma_is_not_re_pointed_when_an_argument_was_inserted_between_its_neighbours() {
        let before = Code::from_string("class C { void f() { g(a, b); } }", &Language::Java);
        let after = Code::from_string("class C { void f() { g(a, x, b); } }", &Language::Java);

        let comma = nth_leaf(&before, ",", 0);
        let diff = solve_over(
            &before,
            &after,
            &[
                (nth_leaf(&before, "a", 0), nth_leaf(&after, "a", 0)),
                (comma, nth_leaf(&after, ",", 0)),
                (nth_leaf(&before, "b", 0), nth_leaf(&after, "b", 0)),
            ],
        );

        assert_eq!(
            diff.before_node_map.get(&comma),
            Some(&nth_leaf(&after, ",", 0)),
            "the first `,` is where it always was",
        );
    }

    #[test]
    fn a_leaf_with_only_one_matched_neighbour_is_left_alone() {
        let before = Code::from_string("class C { void f() { g(a, b); } }", &Language::Java);
        let after = Code::from_string("class C { void f() { g(a, c); } }", &Language::Java);

        let comma = nth_leaf(&before, ",", 0);
        let diff = solve_over(
            &before,
            &after,
            &[(nth_leaf(&before, "a", 0), nth_leaf(&after, "a", 0))],
        );

        assert_eq!(diff.before_node_map.get(&comma), None);
    }

    #[test]
    fn a_claimed_target_is_never_taken() {
        let before = Code::from_string(
            "class C { boolean f() { return a == 1 || a == 2 || a == 3; } }",
            &Language::Java,
        );
        let after = Code::from_string(
            "class C { boolean f() { return a == 1 || a == 2 || a == 3 || a == 4; } }",
            &Language::Java,
        );

        // The outer `||` paired by depth; the position-wise target is already claimed.
        let outer_before = nth_leaf(&before, "||", 1);
        let outer_after = nth_leaf(&after, "||", 2);
        let claimed = nth_leaf(&after, "||", 1);
        let diff = solve_over(
            &before,
            &after,
            &[
                (outer_before, outer_after),
                (nth_leaf(&before, "||", 0), claimed),
            ],
        );

        assert_eq!(diff.before_node_map.get(&outer_before), Some(&outer_after));
    }

    /// The `n`th node of `kind` spelling `text`, in document order.
    fn nth_node(code: &Code, kind: &str, text: &str, n: usize) -> usize {
        let mut found = Vec::new();
        let mut stack = vec![code.ast.as_ref().unwrap().root_node()];
        while let Some(node) = stack.pop() {
            if node.kind() == kind && code.contents.get(node.byte_range()) == Some(text) {
                found.push((node.start_byte(), node.id()));
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
        }
        found.sort_unstable();
        found[n].1
    }

    /// The outer before `||` paired by depth with the new after `||`, its two operands matched,
    /// under the given operation. Returns the diff and the `||`'s expected position-wise partner.
    fn grown_chain_paired_by_depth(operation: ASTMappingOperation) -> (ASTDiff, usize, usize) {
        let before = Code::from_string(
            "class C { boolean f() { return a == 1 || a == 2 || a == 3; } }",
            &Language::Java,
        );
        let after = Code::from_string(
            "class C { boolean f() { return a == 1 || a == 2 || a == 3 || a == 4; } }",
            &Language::Java,
        );
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();
        let inner = "a == 1 || a == 2";
        diff.add_mapping(
            nth_node(&before, "binary_expression", inner, 0),
            nth_node(&after, "binary_expression", inner, 0),
            ASTMapping::identical(ASTMappingReason::IdenticalHash),
        );
        diff.add_mapping(
            nth_node(&before, "binary_expression", "a == 3", 0),
            nth_node(&after, "binary_expression", "a == 3", 0),
            ASTMapping::identical(ASTMappingReason::IdenticalHash),
        );
        let outer_before = nth_leaf(&before, "||", 1);
        diff.add_mapping(
            outer_before,
            nth_leaf(&after, "||", 2),
            ASTMapping {
                cost: 0,
                operation,
                reason: ASTMappingReason::IdenticalHash,
            },
        );
        solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );
        (diff, outer_before, nth_leaf(&after, "||", 1))
    }

    #[test]
    fn an_identical_leaf_between_matched_neighbours_is_re_pointed_to_their_middle() {
        let (diff, leaf, between) = grown_chain_paired_by_depth(ASTMappingOperation::Identical);
        assert_eq!(diff.before_node_map.get(&leaf), Some(&between));
    }

    #[test]
    fn a_non_identical_leaf_pairing_is_never_re_pointed() {
        let (diff, leaf, between) =
            grown_chain_paired_by_depth(ASTMappingOperation::MatchButNotIdentical);
        assert_ne!(diff.before_node_map.get(&leaf), Some(&between));
    }
}
