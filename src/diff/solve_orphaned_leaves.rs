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
use std::collections::HashMap;

/// Whether `before_id` and `after_id` sit between the same things: no immediate sibling on either
/// side is matched to something other than its counterpart.
fn neighbours_correspond(
    before_id: usize,
    after_id: usize,
    before_siblings: &[usize],
    after_siblings: &[usize],
    diff: &ASTDiff,
) -> bool {
    let (Some(before_index), Some(after_index)) = (
        before_siblings.iter().position(|&id| id == before_id),
        after_siblings.iter().position(|&id| id == after_id),
    ) else {
        return false;
    };
    // Rejects only on positive evidence: an unmatched neighbour says nothing, and in a badly broken
    // parse almost no neighbour is matched (css-shadcn-ui-ui-completely-broken-treesitter-parsing).
    let agrees = |before: Option<&usize>, after: Option<&usize>| match (before, after) {
        (Some(&before), Some(&after)) => match (
            diff.before_node_map.get(&before),
            diff.after_node_map.get(&after),
        ) {
            (Some(&partner), _) if partner != 0 => partner == after,
            (_, Some(&partner)) if partner != 0 => partner == before,
            _ => true,
        },
        (None, None) => true,
        _ => false,
    };
    let previous = agrees(
        before_index
            .checked_sub(1)
            .and_then(|i| before_siblings.get(i)),
        after_index
            .checked_sub(1)
            .and_then(|i| after_siblings.get(i)),
    );
    let next = agrees(
        before_siblings.get(before_index + 1),
        after_siblings.get(after_index + 1),
    );
    previous && next
}

/// Pairs a leaf deleted on one side with a leaf inserted on the other when their parents are a
/// matched pair and they read the same. Such leaves come out of several residual paths, so the fix
/// sits after all of them. Matching a `;` to a random other `;` is how this goes wrong, hence the
/// guards:
///
/// * The parents already correspond, as in `solve_unique_type_matching` (which keys on kind alone).
/// * Equal counts per (kind, text) under the pair, so any bijection is the same mapping; unequal
///   counts would mean guessing which leaf survived.
/// * [`neighbours_correspond`]: a `,` between different arguments is a different `,`
///   (csharp-lidarr-call-different-function).
///
/// An `Identical` pair costs 0 against a delete plus an insert, so this only moves the objective
/// toward the human mapping.
pub fn solve(ctx: &PassCtx, diff: &mut ASTDiff) {
    let before_metadata = ctx.before_metadata();
    let after_metadata = ctx.after_metadata();

    // Preorder for determinism; only pairs with a dropped child can produce anything.
    let mut pairs: Vec<(usize, usize)> = diff
        .before_node_map
        .iter()
        .filter_map(|(&before_id, &after_id)| (after_id != 0).then_some((before_id, after_id)))
        .filter(|(before_id, _)| {
            before_metadata
                .node_info
                .get(before_id)
                .is_some_and(|info| {
                    info.children
                        .iter()
                        .any(|child| diff.before_node_map.get(child) == Some(&0))
                })
        })
        .collect();
    pairs.sort_unstable_by_key(|&(before_id, _)| {
        before_metadata
            .node_info
            .get(&before_id)
            .map(|info| info.preorder_index)
            .unwrap_or(usize::MAX)
    });

    for (before_parent, after_parent) in pairs {
        let (Some(before_info), Some(after_info)) = (
            before_metadata.node_info.get(&before_parent),
            after_metadata.node_info.get(&after_parent),
        ) else {
            continue;
        };

        let orphans = |ids: &[usize],
                       metadata: &crate::code::ASTMetadata,
                       map: &rustc_hash::FxHashMap<usize, usize>|
         -> HashMap<(String, String), Vec<usize>> {
            let mut out: HashMap<(String, String), Vec<usize>> = HashMap::new();
            for &id in ids {
                if map.get(&id) != Some(&0) {
                    continue;
                }
                let Some(info) = metadata.node_info.get(&id) else {
                    continue;
                };
                if !info.children.is_empty() {
                    continue;
                }
                out.entry((info.kind.clone(), info.text.clone()))
                    .or_default()
                    .push(id);
            }
            for ids in out.values_mut() {
                ids.sort_unstable_by_key(|id| {
                    metadata
                        .node_info
                        .get(id)
                        .map(|info| info.start_byte)
                        .unwrap_or(usize::MAX)
                });
            }
            out
        };

        let before_orphans = orphans(
            &before_info.children,
            before_metadata,
            &diff.before_node_map,
        );
        if before_orphans.is_empty() {
            continue;
        }
        let after_orphans = orphans(&after_info.children, after_metadata, &diff.after_node_map);
        if after_orphans.is_empty() {
            continue;
        }

        let mut keys: Vec<&(String, String)> = before_orphans.keys().collect();
        keys.sort();
        for key in keys {
            let before_ids = &before_orphans[key];
            let Some(after_ids) = after_orphans.get(key) else {
                continue;
            };
            if before_ids.len() != after_ids.len() {
                continue;
            }
            for (&before_id, &after_id) in before_ids.iter().zip(after_ids.iter()) {
                if !neighbours_correspond(
                    before_id,
                    after_id,
                    &before_info.children,
                    &after_info.children,
                    diff,
                ) {
                    continue;
                }
                diff.remove_delete_mapping(before_id);
                diff.remove_insert_mapping(after_id);
                diff.add_mapping(
                    before_id,
                    after_id,
                    ASTMapping {
                        cost: 0,
                        operation: ASTMappingOperation::Identical,
                        reason: ASTMappingReason::OrphanedLeafUnderMatchedParent,
                    },
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::{Code, Language};
    use crate::diff::NodeCache;
    use crate::test::helper::find_first_of_kind;

    /// Every leaf reading `text`, in document order.
    fn leaves(code: &Code, text: &str) -> Vec<usize> {
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
        found.into_iter().map(|(_, id)| id).collect()
    }

    /// Matches the two argument lists and the given identifier pairs, marks every `,` dropped on
    /// both sides, and runs only this pass.
    fn solve_calls(before_src: &str, after_src: &str, identifiers: &[(&str, &str)]) -> ASTDiff {
        let before = Code::from_string(before_src, &Language::Java);
        let after = Code::from_string(after_src, &Language::Java);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();
        let args = |code: &Code| {
            find_first_of_kind(code.ast.as_ref().unwrap().root_node(), "argument_list")
                .unwrap()
                .id()
        };
        diff.add_mapping(
            args(&before),
            args(&after),
            ASTMapping::matched_not_identical(ASTMappingReason::IdenticalHash),
        );
        for &(b, a) in identifiers {
            diff.add_mapping(
                leaves(&before, b)[0],
                leaves(&after, a)[0],
                ASTMapping::identical(ASTMappingReason::IdenticalHash),
            );
        }
        for id in leaves(&before, ",") {
            diff.add_mapping(id, 0, ASTMapping::deleted(ASTMappingReason::UnresolvedNode));
        }
        for id in leaves(&after, ",") {
            diff.add_mapping(
                0,
                id,
                ASTMapping::inserted(ASTMappingReason::UnresolvedNode),
            );
        }
        solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );
        diff
    }

    fn orphans_paired(diff: &ASTDiff) -> usize {
        diff.mapping
            .values()
            .filter(|m| m.reason == ASTMappingReason::OrphanedLeafUnderMatchedParent)
            .count()
    }

    #[test]
    fn a_dropped_comma_between_the_same_arguments_is_paired() {
        let diff = solve_calls(
            "class C { void f() { g(a, b); } }",
            "class C { void f() { g(a, b); } }",
            &[("a", "a"), ("b", "b")],
        );
        assert_eq!(orphans_paired(&diff), 1);
    }

    #[test]
    fn unequal_orphan_counts_pair_nothing() {
        let diff = solve_calls(
            "class C { void f() { g(a, b, c); } }",
            "class C { void f() { g(a, b); } }",
            &[],
        );
        assert_eq!(orphans_paired(&diff), 0);
    }

    #[test]
    fn a_comma_between_different_arguments_is_not_paired() {
        let diff = solve_calls(
            "class C { void f() { g(a, b); } }",
            "class C { void f() { g(b, a); } }",
            &[("a", "a"), ("b", "b")],
        );
        assert_eq!(orphans_paired(&diff), 0);
    }
}
