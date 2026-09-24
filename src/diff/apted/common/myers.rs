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
//! The flat-tree fast path: Myers LCS over a long run of siblings, and the anchored-segment
//! splitting that feeds it.

use super::*;

/// Minimum number of direct children that makes a node a flat container.
pub(crate) const FLAT_MIN_CHILDREN: usize = 50;
/// Edit-distance cap for Myers diff; past it a segment is marked replaced.
pub(crate) const FLAT_MAX_EDIT: usize = 1000;

/// `root_id`'s full direct-children list, matched ones included, if it has at least
/// `FLAT_MIN_CHILDREN` children. The matched ones are kept because they are the anchors
/// `split_into_anchored_segments` splits on. Children may be subtrees of any depth.
pub(crate) fn flat_children(root_id: usize, meta: &ASTMetadata) -> Option<Vec<usize>> {
    let info = meta.node_info.get(&root_id)?;
    if info.children.len() >= FLAT_MIN_CHILDREN {
        Some(info.children.clone())
    } else {
        None
    }
}

/// `root_id`'s flat child, if exactly one of its children is flat: the thin wrapper around a flat
/// container that `resolve_forest` decomposes. Two flat children mean the wrapper's own structure
/// is the edit.
pub(crate) fn sole_flat_child(root_id: usize, meta: &ASTMetadata) -> Option<usize> {
    let info = meta.node_info.get(&root_id)?;
    let mut flat = info
        .children
        .iter()
        .copied()
        .filter(|&c| flat_children(c, meta).is_some());
    let candidate = flat.next()?;
    if flat.next().is_some() {
        return None;
    }
    Some(candidate)
}

/// Myers O(ND) LCS on two sequences of hashes. Returns matched `(a_idx, b_idx)` pairs
/// in ascending order, or `None` if the edit distance exceeds `max_edit`.
pub(crate) fn myers_lcs(a: &[u64], b: &[u64], max_edit: usize) -> Option<Vec<(usize, usize)>> {
    let n = a.len();
    let m = b.len();
    if n == 0 || m == 0 {
        return Some(vec![]);
    }
    let limit = max_edit.min(n + m);
    let offset = limit + 1; // v[k + offset] for k in [-limit, +limit]
    let v_size = 2 * limit + 3;
    let mut v = vec![0usize; v_size];
    let mut snapshots: Vec<Vec<usize>> = Vec::with_capacity(limit + 1);

    for d in 0..=limit {
        snapshots.push(v.clone()); // snapshots[d] = v before step d's modifications
        for k in (-(d as i64)..=(d as i64)).step_by(2) {
            let ki = (k + offset as i64) as usize;
            let x = if k == -(d as i64) {
                v[ki + 1] // forced insert
            } else if k == d as i64 || v[ki - 1] >= v[ki + 1] {
                v[ki - 1] + 1 // delete
            } else {
                v[ki + 1] // insert
            };
            let mut x = x;
            let mut y = (x as i64 - k) as usize;
            while x < n && y < m && a[x] == b[y] {
                x += 1;
                y += 1;
            }
            v[ki] = x;
            if x >= n && y >= m {
                return Some(backtrack_myers(&snapshots, a, b, d, offset));
            }
        }
    }
    None
}

pub(crate) fn backtrack_myers(
    snapshots: &[Vec<usize>],
    a: &[u64],
    b: &[u64],
    d: usize,
    offset: usize,
) -> Vec<(usize, usize)> {
    let mut matches = Vec::new();
    let mut x = a.len() as i64;
    let mut y = b.len() as i64;

    for step in (1..=d).rev() {
        let v = &snapshots[step]; // v at start of step `step` (= after step `step-1`)
        let k = x - y;
        let ki = (k + offset as i64) as usize;
        let prev_k = if k == -(step as i64) {
            k + 1 // came via insert
        } else if k == step as i64 || v[ki - 1] >= v[ki + 1] {
            k - 1 // came via delete
        } else {
            k + 1 // came via insert
        };
        let prev_x = v[(prev_k + offset as i64) as usize] as i64;
        let prev_y = prev_x - prev_k;
        let x_enter = if prev_k < k { prev_x + 1 } else { prev_x };
        let mut xi = x;
        let mut yi = y;
        while xi > x_enter {
            xi -= 1;
            yi -= 1;
            matches.push((xi as usize, yi as usize));
        }
        x = prev_x;
        y = prev_y;
    }
    // Initial snake: common prefix from (0, 0) to (x, y) after step 0.
    while x > 0 && y > 0 {
        x -= 1;
        y -= 1;
        matches.push((x as usize, y as usize));
    }
    matches.reverse();
    matches
}

/// Above this many Myers-unmatched entries on either side, [`resolve_flat_tree_pair`] replaces
/// the leftovers atomically instead of pooling them through APTED. Small, unlike `FLAT_MAX_EDIT`:
/// each entry can be a large subtree, and a pool of many is the full-TED cost this path avoids.
pub(crate) const FLAT_UNMATCHED_RECURSE_LIMIT: usize = 20;

/// Cap on the total node count, both sides together, of a pooled leftover call; TED cost follows
/// node count, not entry count. The cap guards quality too: a large pool of unrelated entries can
/// cross-match worse than atomic delete/insert.
pub(crate) const FLAT_UNMATCHED_RECURSE_MAX_TOTAL_SIZE: usize = 2000;

/// Splits the two child lists into segments delimited by children already matched in `diff`,
/// each with its still-matched children filtered out. With nothing matched it is one segment.
/// An anchor whose partner is not ahead of the previous split on the after side is ignored.
pub(crate) fn split_into_anchored_segments(
    before_children: &[usize],
    after_children: &[usize],
    diff: &ASTDiff,
) -> Vec<(Vec<usize>, Vec<usize>)> {
    let after_index_by_id: HashMap<usize, usize> = after_children
        .iter()
        .enumerate()
        .map(|(index, &id)| (id, index))
        .collect();

    let mut segments = Vec::new();
    let mut segment_start_before = 0;
    let mut segment_start_after = 0;

    for (before_index, &before_id) in before_children.iter().enumerate() {
        let Some(&after_id) = diff.before_node_map.get(&before_id) else {
            continue;
        };
        let Some(&after_index) = after_index_by_id.get(&after_id) else {
            continue;
        };
        if after_index < segment_start_after {
            continue;
        }
        segments.push((
            before_children[segment_start_before..before_index]
                .iter()
                .copied()
                .filter(|id| !diff.before_node_map.contains_key(id))
                .collect(),
            after_children[segment_start_after..after_index]
                .iter()
                .copied()
                .filter(|id| !diff.after_node_map.contains_key(id))
                .collect(),
        ));
        segment_start_before = before_index + 1;
        segment_start_after = after_index + 1;
    }
    segments.push((
        before_children[segment_start_before..]
            .iter()
            .copied()
            .filter(|id| !diff.before_node_map.contains_key(id))
            .collect(),
        after_children[segment_start_after..]
            .iter()
            .copied()
            .filter(|id| !diff.after_node_map.contains_key(id))
            .collect(),
    ));

    segments
}

/// Resolves a flat-tree root pair by Myers sequence diff and emits all mappings into `diff`.
///
/// Myers runs per anchored segment, not over one pool: among N hash-identical siblings with one
/// inserted, Myers' tie-break picks which one "moved", and without the already-matched anchors
/// in its input that choice drifts through everything after it. Splitting confines the tie to
/// one segment.
// Each parameter is distinct context; a struct built for this one call site would only move it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_flat_tree_pair(
    before_root: usize,
    after_root: usize,
    before_children: Vec<usize>,
    after_children: Vec<usize>,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    source: &'static str,
    diff: &mut ASTDiff,
) {
    resolve_child_sequence(
        before_children,
        after_children,
        before_meta,
        after_meta,
        LeftoverPool::Flat,
        source,
        diff,
    );
    diff.add_mapping(
        before_root,
        after_root,
        ASTMapping::matched_not_identical(ASTMappingReason::FlatSequenceDiff),
    );
}

/// Anchors a container's exact-hash leftovers by declared member name
/// (`nodes::member_identity_name`), resolving each same-`(kind, name)` pair through its own
/// scoped `resolve_forest`. Returns what is still unanchored, in the input order.
///
/// Exact hashes miss every member whose body changed; the name still says which member is which.
/// It needs no size floor, unlike the kind-only anchors, because a declared name is not ambiguous.
pub(crate) fn anchor_leftovers_by_member_name(
    before_unmatched: Vec<usize>,
    after_unmatched: Vec<usize>,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    source: &'static str,
    diff: &mut ASTDiff,
) -> (Vec<usize>, Vec<usize>) {
    if before_unmatched.is_empty() || after_unmatched.is_empty() {
        return (before_unmatched, after_unmatched);
    }
    let language = before_meta.language;
    let bucket = |ids: &[usize], meta: &ASTMetadata| {
        let mut groups: rustc_hash::FxHashMap<(&'static str, String), Vec<usize>> =
            rustc_hash::FxHashMap::default();
        for &id in ids {
            if let Some(key) = nodes::member_identity_name(id, meta, &language) {
                groups.entry(key).or_default().push(id);
            }
        }
        groups
    };
    let before_groups = bucket(&before_unmatched, before_meta);
    if before_groups.is_empty() {
        return (before_unmatched, after_unmatched);
    }
    let after_groups = bucket(&after_unmatched, after_meta);

    let preorder = |meta: &ASTMetadata, id: usize| {
        meta.node_info
            .get(&id)
            .map(|i| i.preorder_index)
            .unwrap_or(usize::MAX)
    };
    // Equal holder counts (Java overloads, constructors) zip in document order: overloads
    // usually keep their order, and the pool caps would delete them all. Differing counts are
    // left alone: which overload is new is the one question a name cannot answer.
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for (key, before_ids) in &before_groups {
        let Some(after_ids) = after_groups.get(key) else {
            continue;
        };
        if before_ids.len() != after_ids.len() {
            continue;
        }
        let mut before_ids = before_ids.clone();
        let mut after_ids = after_ids.clone();
        before_ids.sort_unstable_by_key(|&id| preorder(before_meta, id));
        after_ids.sort_unstable_by_key(|&id| preorder(after_meta, id));
        pairs.extend(before_ids.into_iter().zip(after_ids));
    }
    if pairs.is_empty() {
        return (before_unmatched, after_unmatched);
    }
    // `HashMap` iteration order is not deterministic.
    pairs.sort_unstable_by_key(|&(b, _)| preorder(before_meta, b));

    let cost_model = UnitCostModel::new(language);
    let mut anchored_before: rustc_hash::FxHashSet<usize> = rustc_hash::FxHashSet::default();
    let mut anchored_after: rustc_hash::FxHashSet<usize> = rustc_hash::FxHashSet::default();
    for (b, a) in pairs {
        resolve_forest(
            vec![b],
            vec![a],
            before_meta,
            after_meta,
            &cost_model,
            Algorithm::Apted,
            source,
            diff,
        );
        anchored_before.insert(b);
        anchored_after.insert(a);
    }
    (
        before_unmatched
            .into_iter()
            .filter(|id| !anchored_before.contains(id))
            .collect(),
        after_unmatched
            .into_iter()
            .filter(|id| !anchored_after.contains(id))
            .collect(),
    )
}

/// How [`resolve_child_sequence`] treats leftovers whose counts differ on the two sides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LeftoverPool {
    /// A flat container's children: pool within `FLAT_UNMATCHED_RECURSE_LIMIT` and
    /// `FLAT_UNMATCHED_RECURSE_MAX_TOTAL_SIZE`, else atomic delete/insert.
    Flat,
    /// An oversized pair's children: pool within `APTED_MAX_CELLS`, else
    /// `resolve_unequal_segment_via_kind_only_anchors`, never straight to atomic, since APTED
    /// would have solved this pair exactly had it fit.
    Oversized,
}

/// The child-level half of [`resolve_flat_tree_pair`], without the root pairing: anchors by exact
/// hash (Myers per segment), then by member name, then resolves the leftovers positionally, as a
/// bounded pool, or atomically. Children may be subtrees of any depth.
pub(crate) fn resolve_child_sequence(
    before_children: Vec<usize>,
    after_children: Vec<usize>,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    leftover_pool: LeftoverPool,
    source: &'static str,
    diff: &mut ASTDiff,
) {
    let segments = split_into_anchored_segments(&before_children, &after_children, diff);

    let mut before_unmatched: Vec<usize> = Vec::new();
    let mut after_unmatched: Vec<usize> = Vec::new();

    for (before_seg, after_seg) in segments {
        let before_hashes: Vec<u64> = before_seg
            .iter()
            .map(|&id| before_meta.node_to_full_hash.get(&id).copied().unwrap_or(0))
            .collect();
        let after_hashes: Vec<u64> = after_seg
            .iter()
            .map(|&id| after_meta.node_to_full_hash.get(&id).copied().unwrap_or(0))
            .collect();

        match myers_lcs(&before_hashes, &after_hashes, FLAT_MAX_EDIT) {
            Some(pairs) => {
                let mut before_matched = vec![false; before_seg.len()];
                let mut after_matched = vec![false; after_seg.len()];
                for (bi, ai) in pairs {
                    before_matched[bi] = true;
                    after_matched[ai] = true;
                    emit_identical_subtree(
                        before_seg[bi],
                        after_seg[ai],
                        before_meta,
                        after_meta,
                        source,
                        diff,
                    );
                }
                before_unmatched.extend(
                    before_seg
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| !before_matched[*i])
                        .map(|(_, &id)| id),
                );
                after_unmatched.extend(
                    after_seg
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| !after_matched[*i])
                        .map(|(_, &id)| id),
                );
            }
            None => {
                for &id in &before_seg {
                    add_delete_mappings(id, before_meta, source, diff);
                }
                for &id in &after_seg {
                    add_insert_mappings(id, after_meta, source, diff);
                }
            }
        }
    }

    let (before_unmatched, after_unmatched) = anchor_leftovers_by_member_name(
        before_unmatched,
        after_unmatched,
        before_meta,
        after_meta,
        source,
        diff,
    );

    // Leftovers failed only exact-hash equality, so they go through real APTED rather than being
    // replaced. Equal counts pair per position, one uncapped call each: a lone pair has nothing
    // to cross-match against, while pooling equal counts invites exactly that. Unequal counts
    // have no positional correspondence, so they pool, under a cap because a pool's cost and its
    // cross-match risk grow with its size.
    let unmatched_total_size = subtree_size_sum(&before_unmatched, before_meta)
        + subtree_size_sum(&after_unmatched, after_meta);
    if !before_unmatched.is_empty() && before_unmatched.len() == after_unmatched.len() {
        let cost_model = UnitCostModel::new(before_meta.language);
        for (b, a) in before_unmatched.into_iter().zip(after_unmatched) {
            resolve_forest(
                vec![b],
                vec![a],
                before_meta,
                after_meta,
                &cost_model,
                Algorithm::Apted,
                source,
                diff,
            );
        }
    } else if !before_unmatched.is_empty()
        && !after_unmatched.is_empty()
        && match leftover_pool {
            LeftoverPool::Flat => {
                before_unmatched.len() <= FLAT_UNMATCHED_RECURSE_LIMIT
                    && after_unmatched.len() <= FLAT_UNMATCHED_RECURSE_LIMIT
                    && unmatched_total_size <= FLAT_UNMATCHED_RECURSE_MAX_TOTAL_SIZE
            }
            LeftoverPool::Oversized => {
                subtree_size_sum(&before_unmatched, before_meta)
                    * subtree_size_sum(&after_unmatched, after_meta)
                    <= APTED_MAX_CELLS
            }
        }
    {
        let cost_model = UnitCostModel::new(before_meta.language);
        resolve_forest(
            before_unmatched,
            after_unmatched,
            before_meta,
            after_meta,
            &cost_model,
            Algorithm::Apted,
            source,
            diff,
        );
    } else if leftover_pool == LeftoverPool::Oversized {
        resolve_unequal_segment_via_kind_only_anchors(
            &before_unmatched,
            &after_unmatched,
            before_meta,
            after_meta,
            source,
            diff,
        );
    } else {
        for &id in &before_unmatched {
            add_delete_mappings(id, before_meta, source, diff);
        }
        for &id in &after_unmatched {
            add_insert_mappings(id, after_meta, source, diff);
        }
    }
}
