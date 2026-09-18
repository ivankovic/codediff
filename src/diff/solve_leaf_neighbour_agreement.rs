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

/**
* **A leaf matched to the right kind of node in the wrong place, while its own neighbours name
* the right one.**
*
* When a left-nested chain grows at its outer end - `a || b || c` becoming `a || b || c || d`,
* `x.f()` becoming `x.f().g()` - every operator token in the chain keeps its text and its
* position, and one brand-new token of the same text joins them. Under `cost::operation_cost`
* each of those tokens is worth the same as any other, so pairing them by nesting depth and
* pairing them by position cost exactly the same, and which one the residual search reports is an
* arbitrary tie-break. It reports depth: the outermost before `||` pairs with the outermost after
* `||`, which is the new one, and the old token is reported as deleted.
*
* Every human solution in the corpus reads it the other way, and says why in its own shape: in
* `java-defects4j-closure-147-checkglobalthis` the before `||` at 107:33 sits between
* `pType == Token.NAME` and `pType == Token.ASSIGN`, and so does exactly one after `||` - at
* 107:33, unmoved. The `||` between the same two operands is the same `||`. The corpus-wide
* mismatch census (2026-09-17, `research/data/quality/mismatch_census.csv`) has this pairing under
* five different `reason` tags across `binary_expression`, `method_invocation` and
* `field_expression`, so like [`crate::diff::solve_orphaned_leaves`] the fix belongs after the
* search rather than inside any one of its paths.
*
* **The rule, and why it guesses nothing.** A matched leaf is re-pointed only when all of:
*
* * **Both its neighbours are matched.** Not one - a leaf at the start or end of its parent's
*   child list is anchored on one side only, which is the same single-sided evidence that made
*   two earlier attempts at neighbouring ideas false-positive (see
*   [`crate::diff::solve_heritage_clause_growth`]'s history).
* * **Their two partners are themselves adjacent-but-one, under one parent.** Then the node
*   between them is not chosen, it is the only node there is. `f(a, b)` growing to `f(a, X, b)`
*   fails here - the partners of `a` and `b` are three apart, not two - which is exactly right,
*   because there the new `,` really is new.
* * **That node is free, and reads the same.** Same kind and same text as the leaf, and nothing
*   has claimed it. Same text is what makes the swap cost-neutral: an `Identical` pair costs 0
*   either way, so this only ever reports a different member of a set of optima the search was
*   already indifferent between - it never trades cost for agreement.
*
* A leaf already sitting between its neighbours' partners is by construction its own answer, so
* the pass is idempotent and silent on everything it agrees with.
*/
/// Every after-side node's index in its own parent's child list.
///
/// Built once, and only when some candidate has got far enough to need it. The alternative, a
/// `children.iter().position(...)` per candidate, is quadratic in exactly the file shape the
/// corpus keeps: `css-shadcn-ui-ui-completely-broken-treesitter-parsing` parses into a handful of
/// nodes holding tens of thousands of children each, and scanning one of those lists per leaf in
/// it cost 6% of that fixture's whole diff (measured 2026-09-17).
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

/// The one after-side node that sits between `previous` and `next`, when those two are adjacent-
/// but-one under a single parent. `None` the moment that does not hold - each failure is a case
/// where something would have to be guessed, and this pass never guesses.
fn between(
    previous: usize,
    next: usize,
    after_metadata: &crate::code::ASTMetadata,
    after_child_indices: &rustc_hash::FxHashMap<usize, usize>,
) -> Option<usize> {
    // One parent, or "between" means nothing: two adjacent-but-one indices in different child
    // lists describe no position at all.
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

pub fn solve(ctx: &PassCtx, diff: &mut ASTDiff) {
    let before_metadata = ctx.before_metadata();
    let after_metadata = ctx.after_metadata();

    // Walked parent by parent rather than leaf by leaf so a leaf's index among its siblings falls
    // out of the walk instead of costing a scan - see `after_child_indices` for the same concern
    // on the other side. In before-side document order so the pass is deterministic regardless of
    // the hash maps' own iteration order, the same reason `solve_orphaned_leaves` sorts.
    let mut parents: Vec<usize> = before_metadata
        .node_info
        .iter()
        // Fewer than three children cannot hold a leaf with a neighbour on both sides.
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
            // Only a byte-identical pairing has a cost-free twin to move to. Anything else - an
            // `Update`, a `MatchButNotIdentical` - is a pairing the search paid for, and
            // re-pointing it would be overruling the cost model rather than settling a tie inside
            // it.
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
            // Already where its neighbours say it belongs: this pass has nothing to say about it.
            if target == partner {
                continue;
            }
            // Free, and reads the same. `Some(&0)` is an explicit insert and `None` is a node no
            // pass has decided yet; both are unclaimed, and anything else is a pairing this one
            // will not break to make its own.
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

            // The old partner is left with no mapping at all rather than an explicit insert:
            // phase 10's `solve_unresolved_nodes` is the one place that decides what an undecided
            // node is, and it runs after this pass for exactly this reason.
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

    /// The node id of the `n`th leaf reading `text` in document order, so a test can name "the
    /// second `||`" without hand-walking the tree.
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

    /// The motivating shape, end to end through the real pipeline: a `||` chain grows at its outer
    /// end. Java parses `a || b || c` left-nested, so the new term's `||` is the *outermost* one
    /// and every old `||` keeps its text and its position. The pairing is by position.
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

    /// The same shape one grammar level down: `x.f()` becoming `x.f().g()` nests the *old*
    /// invocation inside the new one, so the old `.` ends up a level deeper than the new one.
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

    /// Runs only this pass over a mapping built by hand, so a passing assertion can only be this
    /// pass's own doing - the pipeline's other passes have their own opinions about shapes this
    /// small. Returns the diff for the caller to read.
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

    /// A new argument between two existing ones adds a `,` that really is new, and the existing
    /// `,` really did stay put. The partners of the leaf's neighbours are three apart here, not
    /// two, so "the node between them" names nothing and the pass leaves the pairing alone.
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

    /// Both neighbours must be matched. With only the left one anchored, the leaf is being placed
    /// on one-sided evidence, which is what this pass refuses to do - even though the node to the
    /// right of that neighbour's partner reads the same.
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

    /// The target must be free. A leaf whose neighbours point at a node another pass already
    /// claimed stays where it is rather than taking it - this pass settles ties, it does not win
    /// arguments.
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

        // The outer `||` paired by depth, as the residual search leaves it, with the position-wise
        // correct target already claimed by an unrelated before-side leaf.
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
}
