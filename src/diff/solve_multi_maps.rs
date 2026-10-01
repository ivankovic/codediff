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
//! Phase 9c: identical copies become N:M groups. One statement written twice where it was once
//! (`p.skipChildren();` copied into a new `else`, `java-defects4j-jacksondatabind-39-
//! nullifyingdeserializer`), or twice folded into once (`tokens.add(token);`,
//! `java-defects4j-cli-19-posixparser`), is one element on both sides: the ground truth records it
//! as an all-to-all group, and a one-to-one diff can only delete or insert the extra copy.
//!
//! A candidate is a maximal wholly deleted (or inserted) subtree whose full hash equals a node on
//! its own side that is paired with an identical node. The census of 2026-09-30
//! (`nm_candidate_census`) found no gate that is precise on its own; the one the ground truth
//! supports best, and the one used here, is a restructuring:
//!
//! - exactly one such twin, so the group is not a guess among several;
//! - no unpaired identical node on the other side, which would make it a move;
//! - the edit displaced the twin: its nesting ([`nesting`]) differs from its partner's;
//! - the candidate's own parent is gone too (a new branch or loop, a folded one);
//! - it is a statement or declaration ([`is_statement`]);
//! - copy and twin sit in two branches of one conditional ([`in_two_branches`]);
//! - both sit in the same function;
//! - at least [`MIN_COPY_SIZE`] nodes, below which identical code is commodity.
//!
//! Groups are written per position down the two identical subtrees, as the ground truth's solver
//! writes them. Runs after every matching pass, so a twin's parent is paired if it ever will be,
//! and never takes a node from a one-to-one pair other than the twin's own, which it widens.

use crate::code::ASTMetadata;
use crate::diff::{ASTDiff, ASTMapping, ASTMappingReason, PassCtx};

/// Smallest subtree, in nodes, that is grouped with its twin.
const MIN_COPY_SIZE: usize = 5;

/// Kind-name fragments of a function-like construct, for "the same function".
const FUNCTION_WORDS: [&str; 6] = [
    "function",
    "method",
    "constructor",
    "lambda",
    "closure",
    "arrow",
];

pub fn solve(ctx: &PassCtx, diff: &mut ASTDiff) {
    let metadata = [ctx.before_metadata(), ctx.after_metadata()];
    for side in 0..2 {
        let copies = find_copies(side, &metadata, diff);
        for (copy, twin, partner) in copies {
            group_copy(side, copy, twin, partner, &metadata, diff);
        }
    }
}

fn node_map(diff: &ASTDiff, side: usize) -> &rustc_hash::FxHashMap<usize, usize> {
    if side == 0 {
        &diff.before_node_map
    } else {
        &diff.after_node_map
    }
}

/// No real partner: deleted (inserted), or not decided yet - phase 10 records what is still
/// undecided as deleted or inserted.
fn unpaired(id: usize, map: &rustc_hash::FxHashMap<usize, usize>) -> bool {
    map.get(&id).is_none_or(|&partner| partner == 0)
}

fn fully_unmatched(
    id: usize,
    meta: &ASTMetadata,
    map: &rustc_hash::FxHashMap<usize, usize>,
) -> bool {
    unpaired(id, map)
        && meta.node_info.get(&id).is_none_or(|info| {
            info.children
                .iter()
                .all(|&child| fully_unmatched(child, meta, map))
        })
}

fn enclosing_function(id: usize, meta: &ASTMetadata) -> Option<usize> {
    let mut current = *meta.node_to_parent.get(&id)?;
    loop {
        if meta
            .node_info
            .get(&current)
            .is_some_and(|info| FUNCTION_WORDS.iter().any(|w| info.kind.contains(w)))
        {
            return Some(current);
        }
        current = *meta.node_to_parent.get(&current)?;
    }
}

/// Whether `a` and `b` sit in two different branches of one conditional: their lowest common
/// ancestor is an `if`, and they descend from different children of it. The copy of
/// `java-defects4j-jacksondatabind-39-nullifyingdeserializer` is written into the new `if`'s
/// consequence while the original is re-wrapped into its `else`.
fn in_two_branches(a: usize, b: usize, meta: &ASTMetadata) -> bool {
    let chain = |mut id: usize| {
        let mut out = vec![id];
        while let Some(&parent) = meta.node_to_parent.get(&id) {
            out.push(parent);
            id = parent;
        }
        out
    };
    let (ca, cb) = (chain(a), chain(b));
    let Some(lca_index) = ca.iter().position(|id| cb.contains(id)) else {
        return false;
    };
    let lca = ca[lca_index];
    let is_conditional = meta
        .node_info
        .get(&lca)
        .is_some_and(|info| info.kind.starts_with("if"));
    let below = |chain: &Vec<usize>| {
        chain
            .iter()
            .position(|&id| id == lca)
            .and_then(|i| i.checked_sub(1))
            .map(|i| chain[i])
    };
    is_conditional && below(&ca) != below(&cb)
}

/// A statement or declaration: the unit a copy is written in. Fragments of expressions are left
/// alone - an argument list whose call was wrapped in another call is not a copied line
/// (`kotlin-nextcloud-change-function-fingerprint`).
fn is_statement(kind: &str) -> bool {
    kind.ends_with("_statement") || kind.ends_with("_declaration")
}

/// The kinds of `id`'s ancestors up to its enclosing function, innermost first: where the node sits,
/// read from its own tree. Two identical nodes whose nesting differs were displaced by the edit
/// (re-wrapped into a new `else`), whatever the rest of the diff paired; reading it from the node
/// maps instead would let a parent codediff failed to pair (an `if` that grew an `else`,
/// `java-defects4j-closure-134-typedscopecreator`) pass for a displacement.
fn nesting(id: usize, meta: &ASTMetadata) -> Vec<&str> {
    let mut kinds = Vec::new();
    let mut current = meta.node_to_parent.get(&id).copied();
    while let Some(ancestor) = current {
        let Some(info) = meta.node_info.get(&ancestor) else {
            break;
        };
        kinds.push(info.kind.as_str());
        if FUNCTION_WORDS.iter().any(|w| info.kind.contains(w)) {
            break;
        }
        current = meta.node_to_parent.get(&ancestor).copied();
    }
    kinds
}

/// `(copy, twin, twin's partner)` for every candidate on `side` that passes the gate.
fn find_copies(
    side: usize,
    metadata: &[&ASTMetadata; 2],
    diff: &ASTDiff,
) -> Vec<(usize, usize, usize)> {
    let (meta, other) = (metadata[side], metadata[1 - side]);
    let map = node_map(diff, side);
    let other_map = node_map(diff, 1 - side);
    let mut copies = Vec::new();
    let mut ids: Vec<usize> = meta
        .node_info
        .keys()
        .copied()
        .filter(|id| unpaired(*id, map))
        .collect();
    ids.sort_unstable();
    for id in ids {
        if meta.node_to_subtree_size.get(&id).copied().unwrap_or(0) < MIN_COPY_SIZE
            || !fully_unmatched(id, meta, map)
        {
            continue;
        }
        let Some(&parent) = meta.node_to_parent.get(&id) else {
            continue;
        };
        // Its parent gone too. Maximal among candidates, not among unpaired subtrees: a copied
        // statement inside a new block is the copy, the block around it is new.
        if !unpaired(parent, map) {
            continue;
        }
        if !meta
            .node_info
            .get(&id)
            .is_some_and(|info| is_statement(&info.kind))
        {
            continue;
        }
        let Some(&hash) = meta.node_to_full_hash.get(&id) else {
            continue;
        };
        let twins: Vec<(usize, usize)> = meta
            .full_hash_to_node
            .get(&hash)
            .into_iter()
            .flatten()
            .filter_map(|&twin| {
                let partner = *map.get(&twin)?;
                (twin != id && partner != 0 && other.node_to_full_hash.get(&partner) == Some(&hash))
                    .then_some((twin, partner))
            })
            .collect();
        let [(twin, partner)] = twins[..] else {
            continue;
        };
        // An unpaired identical node on the other side makes this a move the move pass declined,
        // not a copy (`c-linux-small-bugfix`: a statement moved into a new `else`).
        let move_target = other
            .full_hash_to_node
            .get(&hash)
            .into_iter()
            .flatten()
            .any(|&candidate| unpaired(candidate, other_map));
        if move_target {
            continue;
        }
        if diff.before_group.contains_key(&twin) || diff.after_group.contains_key(&twin) {
            continue;
        }
        let displaced = nesting(twin, meta) != nesting(partner, other);
        let branches = in_two_branches(id, twin, meta);
        let same_function =
            enclosing_function(id, meta).is_some_and(|f| enclosing_function(twin, meta) == Some(f));
        if displaced && branches && same_function && other_map.get(&partner) == Some(&twin) {
            copies.push((id, twin, partner));
        }
    }
    // A copy inside another copy is grouped with it.
    let roots: rustc_hash::FxHashSet<usize> = copies.iter().map(|&(id, _, _)| id).collect();
    copies.retain(|&(id, _, _)| {
        let mut current = meta.node_to_parent.get(&id).copied();
        while let Some(ancestor) = current {
            if roots.contains(&ancestor) {
                return false;
            }
            current = meta.node_to_parent.get(&ancestor).copied();
        }
        true
    });
    copies
}

/// Widens the twin's pair into one all-to-all group per position of the two identical subtrees.
fn group_copy(
    side: usize,
    copy: usize,
    twin: usize,
    partner: usize,
    metadata: &[&ASTMetadata; 2],
    diff: &mut ASTDiff,
) {
    let (meta, other) = (metadata[side], metadata[1 - side]);
    let start =
        |meta: &ASTMetadata, id: usize| meta.node_info.get(&id).map_or(0, |info| info.start_byte);
    let mut stack = vec![(copy, twin, partner)];
    while let Some((c, t, p)) = stack.pop() {
        let mut members = [c, t];
        members.sort_unstable_by_key(|&id| start(meta, id));
        if side == 0 {
            diff.remove_match_mapping(t, p);
            if diff.before_node_map.get(&c) == Some(&0) {
                diff.remove_delete_mapping(c);
            }
            diff.add_group(
                &members,
                &[p],
                ASTMapping::identical(ASTMappingReason::MultiMap),
            );
        } else {
            diff.remove_match_mapping(p, t);
            if diff.after_node_map.get(&c) == Some(&0) {
                diff.remove_insert_mapping(c);
            }
            diff.add_group(
                &[p],
                &members,
                ASTMapping::identical(ASTMappingReason::MultiMap),
            );
        }
        let children = |meta: &ASTMetadata, id: usize| {
            meta.node_info
                .get(&id)
                .map(|info| info.children.clone())
                .unwrap_or_default()
        };
        let (cc, tc, pc) = (children(meta, c), children(meta, t), children(other, p));
        if cc.len() == tc.len() && tc.len() == pc.len() {
            for i in 0..cc.len() {
                stack.push((cc[i], tc[i], pc[i]));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::code::{Code, Language};
    use crate::diff::diff_code;

    fn statements(code: &Code, text: &str) -> Vec<usize> {
        let mut out = Vec::new();
        let mut stack = vec![code.ast.as_ref().unwrap().root_node()];
        while let Some(node) = stack.pop() {
            if node.kind() == "expression_statement" && &code.contents[node.byte_range()] == text {
                out.push(node.id());
            }
            let mut cursor = node.walk();
            stack.extend(node.children(&mut cursor));
        }
        out
    }

    /// The shape of `java-defects4j-jacksondatabind-39-nullifyingdeserializer`: the statement is
    /// re-wrapped into a new `else` and written again into the new `if`'s other branch. Both are
    /// the one statement of the before side.
    #[test]
    fn a_statement_rewrapped_and_copied_into_the_other_branch_is_one_group() {
        let before = Code::from_string(
            "class A { void f(P p) {\n    p.skipChildren();\n} }\n",
            &Language::Java,
        );
        let after = Code::from_string(
            "class A { void f(P p) {\n    if (p.named()) {\n        while (true) {\n            p.skipChildren();\n        }\n    } else {\n        p.skipChildren();\n    }\n} }\n",
            &Language::Java,
        );

        let ast = diff_code(&before, &after).ast.unwrap();

        let [original] = statements(&before, "p.skipChildren();")[..] else {
            panic!("one statement before");
        };
        let group = ast
            .before_group(original)
            .expect("the statement is grouped");
        let mut copies = statements(&after, "p.skipChildren();");
        copies.sort_unstable();
        let mut members = group.after.clone();
        members.sort_unstable();
        assert_eq!(members, copies);
    }

    /// A line written again beside an original that did not move is new code, not a copy
    /// (`java-defects4j-closure-134-typedscopecreator`).
    #[test]
    fn a_statement_written_again_beside_an_untouched_original_is_not_grouped() {
        let before = Code::from_string(
            "class A { void f(P p) {\n    if (p.named()) {\n        p.skipChildren();\n    }\n} }\n",
            &Language::Java,
        );
        let after = Code::from_string(
            "class A { void f(P p) {\n    if (p.named()) {\n        p.skipChildren();\n    } else {\n        while (true) {\n            p.skipChildren();\n        }\n    }\n} }\n",
            &Language::Java,
        );

        let ast = diff_code(&before, &after).ast.unwrap();

        assert!(ast.groups.is_empty());
    }
}
