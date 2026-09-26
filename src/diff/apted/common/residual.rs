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
//! What happens to the nodes the main pass left unmatched: kind-only anchoring, similarity
//! alignment, and a second Myers pass over the residual forest.

use super::*;

/// Edit-distance cap for the residual Myers diffs, as `FLAT_MAX_EDIT` is for flat containers.
pub(crate) const FALLBACK_MAX_EDIT: usize = 1000;

/// Minimum subtree size, on both sides, for a kind-only hash pair in
/// `resolve_unequal_segment_via_kind_only_anchors` to be trusted. The hash ignores leaf values,
/// so small subtrees - shallow repeated containers as well as leaves - collide on shape alone.
pub(crate) const KIND_ONLY_ANCHOR_MIN_SIZE: usize = 50;

/// Largest subtree size of a residual entry treated as trivial punctuation (a stray `;`) when
/// testing whether an unequal-count segment is a wrap/reparent in disguise
/// (`cpp_add_templates`). Widen only with corpus evidence: size-based trust thresholds here are
/// kept conservative.
pub(crate) const TRIVIAL_ENTRY_MAX_SIZE: usize = 1;

/// The root of every maximal still-unmatched subtree under `root_id`, in preorder: descent stops
/// only at a node whose whole subtree is unmatched, so an unmatched node with a matched
/// descendant (usually the root itself) is descended into rather than emitted.
pub(crate) fn maximal_unmatched_roots(
    root_id: usize,
    meta: &ASTMetadata,
    node_map: &rustc_hash::FxHashMap<usize, usize>,
) -> Vec<usize> {
    let mut has_matched_descendant = rustc_hash::FxHashMap::default();
    subtree_has_any_match(root_id, meta, node_map, &mut has_matched_descendant);

    let mut result = Vec::new();
    let mut stack = vec![root_id];
    while let Some(id) = stack.pop() {
        let matched_here = node_map.contains_key(&id);
        let matched_below = has_matched_descendant.get(&id).copied().unwrap_or(false);
        if !matched_here && !matched_below {
            result.push(id);
            continue;
        }
        if let Some(info) = meta.node_info.get(&id) {
            for &child in info.children.iter().rev() {
                stack.push(child);
            }
        }
    }
    result
}

/// Fills `out[id]` with whether some node strictly under `id` is in `node_map`, and returns it.
pub(crate) fn subtree_has_any_match(
    id: usize,
    meta: &ASTMetadata,
    node_map: &rustc_hash::FxHashMap<usize, usize>,
    out: &mut rustc_hash::FxHashMap<usize, bool>,
) -> bool {
    let Some(info) = meta.node_info.get(&id) else {
        return node_map.contains_key(&id);
    };
    let mut any_matched = false;
    for &child in &info.children {
        let child_matched = node_map.contains_key(&child);
        let child_has_matched_descendant = subtree_has_any_match(child, meta, node_map, out);
        if child_matched || child_has_matched_descendant {
            any_matched = true;
        }
    }
    out.insert(id, any_matched);
    any_matched
}

/// Resolves the trivial (leaf) entries the wrap/reparent branch filtered out, rescuing any that
/// moved with the code instead of deleting them all; e.g. a declaration's trailing `;` that now
/// sits inside the new `template_declaration`.
///
/// That counterpart is not a peer in the segment but a descendant of a substantial partner, and
/// the recursion has already emitted it as an Insert, so rescuing it re-points that insert. A
/// leaf is rescued only when the partners contain exactly one inserted leaf of its kind: matching
/// a `;` to some other `;` is a guess, and one candidate means no choice is made.
pub(crate) fn rescue_wrapped_trivial_entries(
    before_seg: &[usize],
    after_seg: &[usize],
    after_substantial: &[usize],
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    source: &'static str,
    diff: &mut ASTDiff,
) {
    let is_trivial = |id: usize, meta: &ASTMetadata| {
        meta.node_to_subtree_size.get(&id).copied().unwrap_or(0) <= TRIVIAL_ENTRY_MAX_SIZE
    };
    let is_descendant_of = |mut id: usize, ancestor: usize, meta: &ASTMetadata| {
        while let Some(&parent) = meta.node_to_parent.get(&id) {
            if parent == ancestor {
                return true;
            }
            id = parent;
        }
        false
    };

    let cost_model = UnitCostModel::new(before_meta.language);
    for &b in before_seg {
        if !is_trivial(b, before_meta) || diff.before_node_map.contains_key(&b) {
            continue;
        }
        let Some(&kind) = before_meta.node_to_kind_only_hash.get(&b) else {
            continue;
        };
        let mut candidate = None;
        let mut ambiguous = false;
        for (&after_id, &after_kind) in &after_meta.node_to_kind_only_hash {
            if after_kind != kind
                || !is_trivial(after_id, after_meta)
                || diff.after_node_map.get(&after_id) != Some(&0)
                || !after_substantial
                    .iter()
                    .any(|&root| is_descendant_of(after_id, root, after_meta))
            {
                continue;
            }
            if candidate.is_some() {
                ambiguous = true;
                break;
            }
            candidate = Some(after_id);
        }
        if ambiguous {
            continue;
        }
        let Some(a) = candidate else { continue };
        diff.remove_insert_mapping(a);
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

    for &id in before_seg {
        if is_trivial(id, before_meta) && !diff.before_node_map.contains_key(&id) {
            add_delete_mappings(id, before_meta, source, diff);
        }
    }
    for &id in after_seg {
        if is_trivial(id, after_meta) && !diff.after_node_map.contains_key(&id) {
            add_insert_mappings(id, after_meta, source, diff);
        }
    }
}

/// The terminal fallback (phase 6): Myers LCS over the full hashes of every maximal
/// still-unmatched subtree root on each side, identical pairs emitted as such, then each gap
/// between them resolved.
///
/// Gap entries are scattered, unrelated fragments of the whole file, so they are never pooled:
/// given a pool, APTED finds plausible but wrong cross-matches between them. Equal-count gaps
/// recurse per position, uncapped, since a lone pair has nothing to cross-match; a whole-subtree
/// hash misses a chain (nested `<li>`s, a `+` chain) that lost one link, and APTED recovers it.
/// Unequal gaps try the wrap/reparent filter, then kind-only and similarity anchors, then
/// replace atomically.
pub(crate) fn resolve_residual_forest_via_myers_lcs(
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    before_root_id: usize,
    after_root_id: usize,
    source: &'static str,
    diff: &mut ASTDiff,
) {
    let before_roots = maximal_unmatched_roots(before_root_id, before_meta, &diff.before_node_map);
    let after_roots = maximal_unmatched_roots(after_root_id, after_meta, &diff.after_node_map);

    let before_hashes: Vec<u64> = before_roots
        .iter()
        .map(|&id| before_meta.node_to_full_hash.get(&id).copied().unwrap_or(0))
        .collect();
    let after_hashes: Vec<u64> = after_roots
        .iter()
        .map(|&id| after_meta.node_to_full_hash.get(&id).copied().unwrap_or(0))
        .collect();

    if let Some(pairs) = myers_lcs(&before_hashes, &after_hashes, FALLBACK_MAX_EDIT) {
        for (bi, ai) in pairs {
            emit_identical_subtree(
                before_roots[bi],
                after_roots[ai],
                before_meta,
                after_meta,
                source,
                diff,
            );
        }
    }

    let segments = split_into_anchored_segments(&before_roots, &after_roots, diff);
    for (before_seg, after_seg) in segments {
        if before_seg.is_empty() && after_seg.is_empty() {
            continue;
        }
        let recursable = !before_seg.is_empty() && before_seg.len() == after_seg.len();
        if recursable {
            let cost_model = UnitCostModel::new(before_meta.language);
            for (&b, &a) in before_seg.iter().zip(after_seg.iter()) {
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
        } else if !before_seg.is_empty() && !after_seg.is_empty() {
            // Wrap/reparent: the count mismatch may be only trivial leaves beside a real reparent
            // (`class_specifier` becoming `template_declaration`'s child). With leaves filtered
            // out, equal counts keep the per-position safety argument.
            let before_substantial: Vec<usize> = before_seg
                .iter()
                .copied()
                .filter(|id| {
                    before_meta
                        .node_to_subtree_size
                        .get(id)
                        .copied()
                        .unwrap_or(0)
                        > TRIVIAL_ENTRY_MAX_SIZE
                })
                .collect();
            let after_substantial: Vec<usize> = after_seg
                .iter()
                .copied()
                .filter(|id| {
                    after_meta
                        .node_to_subtree_size
                        .get(id)
                        .copied()
                        .unwrap_or(0)
                        > TRIVIAL_ENTRY_MAX_SIZE
                })
                .collect();
            if !before_substantial.is_empty() && before_substantial.len() == after_substantial.len()
            {
                let cost_model = UnitCostModel::new(before_meta.language);
                for (&b, &a) in before_substantial.iter().zip(after_substantial.iter()) {
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
                rescue_wrapped_trivial_entries(
                    &before_seg,
                    &after_seg,
                    &after_substantial,
                    before_meta,
                    after_meta,
                    source,
                    diff,
                );
            } else {
                resolve_unequal_segment_via_kind_only_anchors(
                    &before_seg,
                    &after_seg,
                    before_meta,
                    after_meta,
                    source,
                    diff,
                );
            }
        } else {
            for &id in &before_seg {
                add_delete_mappings(id, before_meta, source, diff);
            }
            for &id in &after_seg {
                add_insert_mappings(id, after_meta, source, diff);
            }
        }
    }
}

/// Unequal-count fallback for a residual gap: aligns entries by Myers over
/// `node_to_kind_only_hash`, else by [`align_segment_by_similarity`], else by
/// [`align_segment_by_mutual_similarity`]; recurses each aligned pair on its own and replaces the
/// rest atomically. Pairs are fixed before APTED runs, so APTED never gets a pool.
///
/// A kind-only pair is trusted only if both entries reach `KIND_ONLY_ANCHOR_MIN_SIZE` and its hash
/// is unique in both segments: LCS picks some pairing among same-hash candidates, not the true one.
/// Both filters only withhold matches.
pub(crate) fn resolve_unequal_segment_via_kind_only_anchors(
    before_seg: &[usize],
    after_seg: &[usize],
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    source: &'static str,
    diff: &mut ASTDiff,
) {
    let before_hashes: Vec<u64> = before_seg
        .iter()
        .map(|id| {
            before_meta
                .node_to_kind_only_hash
                .get(id)
                .copied()
                .unwrap_or(0)
        })
        .collect();
    let after_hashes: Vec<u64> = after_seg
        .iter()
        .map(|id| {
            after_meta
                .node_to_kind_only_hash
                .get(id)
                .copied()
                .unwrap_or(0)
        })
        .collect();

    let before_hash_counts = count_occurrences(&before_hashes);
    let after_hash_counts = count_occurrences(&after_hashes);

    // The size floor and ambiguity check guard kind-only hash collisions only; applied to a
    // similarity-aligned pair they would reject the small genuine matches it exists to find.
    let mut pairs = myers_lcs(&before_hashes, &after_hashes, FALLBACK_MAX_EDIT).unwrap_or_default();
    let from_hash = !pairs.is_empty();
    if pairs.is_empty() {
        pairs = align_segment_by_similarity(before_seg, after_seg, before_meta, after_meta);
    }
    if pairs.is_empty() {
        pairs = align_segment_by_mutual_similarity(before_seg, after_seg, before_meta, after_meta);
    }

    let mut matched_before = vec![false; before_seg.len()];
    let mut matched_after = vec![false; after_seg.len()];
    let cost_model = UnitCostModel::new(before_meta.language);
    for (bi, ai) in &pairs {
        if from_hash {
            let before_size = before_meta
                .node_to_subtree_size
                .get(&before_seg[*bi])
                .copied()
                .unwrap_or(0);
            let after_size = after_meta
                .node_to_subtree_size
                .get(&after_seg[*ai])
                .copied()
                .unwrap_or(0);
            if before_size < KIND_ONLY_ANCHOR_MIN_SIZE || after_size < KIND_ONLY_ANCHOR_MIN_SIZE {
                continue;
            }
            let ambiguous = before_hash_counts
                .get(&before_hashes[*bi])
                .copied()
                .unwrap_or(0)
                > 1
                || after_hash_counts
                    .get(&after_hashes[*ai])
                    .copied()
                    .unwrap_or(0)
                    > 1;
            if ambiguous {
                continue;
            }
        }
        matched_before[*bi] = true;
        matched_after[*ai] = true;
        resolve_forest(
            vec![before_seg[*bi]],
            vec![after_seg[*ai]],
            before_meta,
            after_meta,
            &cost_model,
            Algorithm::Apted,
            source,
            diff,
        );
    }
    for (i, &id) in before_seg.iter().enumerate() {
        if !matched_before[i] {
            add_delete_mappings(id, before_meta, source, diff);
        }
    }
    for (i, &id) in after_seg.iter().enumerate() {
        if !matched_after[i] {
            add_insert_mappings(id, after_meta, source, diff);
        }
    }
}

/// Minimum leaf-content Jaccard (`node_to_similarity_sketch`) for
/// [`align_segment_by_similarity`] to call two residual entries the same thing. It sits between
/// the known true positive and the known kind-only hash false positives, separating them on
/// content rather than on size.
pub(crate) const SEGMENT_SIMILARITY_MIN: f32 = 0.9;

/// Largest `before.len() * after.len()` the similarity alignments run over; every cell is a
/// MinHash Jaccard, on the terminal fallback's path.
pub(crate) const SEGMENT_SIMILARITY_MAX_CELLS: usize = 4096;

/// Floor for [`align_segment_by_mutual_similarity`]: below it two entries are not an edit of one
/// another even when nothing is closer. A rewritten statement (`x.onclick = f;` to
/// `x.addEventListener('click', f);`) sits just above it.
pub(crate) const SEGMENT_MUTUAL_SIMILARITY_MIN: f32 = 0.3;

/// The unequal-count gap's last resort before atomic delete/insert, for a rewrite plus plain
/// inserts (or deletes) that [`SEGMENT_SIMILARITY_MIN`] is too strict to admit.
///
/// The criterion is relative: a pair must be each other's strict single best candidate, of the
/// same kind, at least [`SEGMENT_MUTUAL_SIMILARITY_MIN`]; the pairs must preserve order; and
/// every entry of the smaller side must be paired, else nothing is returned. That last condition
/// is what makes "the rest are inserts" evidence rather than a guess, and keeps a gap of
/// unrelated fragments from being partly stitched.
pub(crate) fn align_segment_by_mutual_similarity(
    before_seg: &[usize],
    after_seg: &[usize],
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
) -> Vec<(usize, usize)> {
    let (n, m) = (before_seg.len(), after_seg.len());
    if n == 0 || m == 0 || n * m > SEGMENT_SIMILARITY_MAX_CELLS {
        return Vec::new();
    }
    fn kind_of(meta: &ASTMetadata, id: usize) -> Option<&str> {
        meta.node_info.get(&id).map(|i| i.kind.as_str())
    }
    // A declaration's first direct identifier-like child.
    fn declared_name(meta: &ASTMetadata, id: usize) -> Option<&str> {
        meta.node_info.get(&id)?.children.iter().find_map(|c| {
            let info = meta.node_info.get(c)?;
            nodes::is_identifier_kind(&info.kind).then_some(info.text.as_str())
        })
    }
    let language = before_meta.language;
    let sim: Vec<Vec<f32>> = (0..n)
        .map(|bi| {
            (0..m)
                .map(|ai| {
                    let kind = kind_of(before_meta, before_seg[bi]);
                    if kind != kind_of(after_meta, after_seg[ai]) {
                        return 0.0;
                    }
                    // A renamed declaration is not "the same thing rewritten" on similarity
                    // alone; wanted renames are matched by the name-based passes before this.
                    if kind.is_some_and(|k| nodes::is_reference(k, &language))
                        && let (Some(b), Some(a)) = (
                            declared_name(before_meta, before_seg[bi]),
                            declared_name(after_meta, after_seg[ai]),
                        )
                        && b != a
                    {
                        return 0.0;
                    }
                    match (
                        before_meta.node_to_similarity_sketch.get(&before_seg[bi]),
                        after_meta.node_to_similarity_sketch.get(&after_seg[ai]),
                    ) {
                        (Some(b), Some(a)) => b.jaccard(a),
                        _ => 0.0,
                    }
                })
                .collect()
        })
        .collect();
    // A tie disqualifies both candidates.
    let strict_argmax = |scores: &mut dyn Iterator<Item = (usize, f32)>| -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        let mut tied = false;
        for (idx, v) in scores {
            match best {
                Some((_, bv)) if v > bv => {
                    best = Some((idx, v));
                    tied = false;
                }
                Some((_, bv)) if v == bv => tied = true,
                None => best = Some((idx, v)),
                _ => {}
            }
        }
        match best {
            Some((idx, v)) if !tied && v >= SEGMENT_MUTUAL_SIMILARITY_MIN => Some(idx),
            _ => None,
        }
    };
    let best_after: Vec<Option<usize>> = (0..n)
        .map(|bi| strict_argmax(&mut (0..m).map(|ai| (ai, sim[bi][ai]))))
        .collect();
    let best_before: Vec<Option<usize>> = (0..m)
        .map(|ai| strict_argmax(&mut (0..n).map(|bi| (bi, sim[bi][ai]))))
        .collect();

    let mut pairs: Vec<(usize, usize)> = (0..n)
        .filter_map(|bi| {
            let ai = best_after[bi]?;
            (best_before[ai] == Some(bi)).then_some((bi, ai))
        })
        .collect();
    let ordered = pairs.windows(2).all(|w| w[0].1 < w[1].1);
    if !ordered || pairs.len() != n.min(m) {
        pairs.clear();
    }
    pairs
}

/// Order-preserving alignment of a residual gap's entries maximizing total leaf-content
/// similarity, pairs below [`SEGMENT_SIMILARITY_MIN`] excluded. Kind-only hash equality fails on
/// a single added statement anywhere inside; similarity does not.
pub(crate) fn align_segment_by_similarity(
    before_seg: &[usize],
    after_seg: &[usize],
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
) -> Vec<(usize, usize)> {
    let (n, m) = (before_seg.len(), after_seg.len());
    if n == 0 || m == 0 || n * m > SEGMENT_SIMILARITY_MAX_CELLS {
        return Vec::new();
    }

    let similarity = |bi: usize, ai: usize| -> f32 {
        match (
            before_meta.node_to_similarity_sketch.get(&before_seg[bi]),
            after_meta.node_to_similarity_sketch.get(&after_seg[ai]),
        ) {
            (Some(b), Some(a)) => {
                let j = b.jaccard(a);
                if j >= SEGMENT_SIMILARITY_MIN { j } else { 0.0 }
            }
            _ => 0.0,
        }
    };

    // score[i][j] = best total similarity aligning before_seg[..i] with after_seg[..j].
    let mut score = vec![vec![0.0f32; m + 1]; n + 1];
    for i in 1..=n {
        for j in 1..=m {
            let skip = score[i - 1][j].max(score[i][j - 1]);
            let sim = similarity(i - 1, j - 1);
            let take = if sim > 0.0 {
                score[i - 1][j - 1] + sim
            } else {
                0.0
            };
            score[i][j] = skip.max(take);
        }
    }

    let (mut i, mut j) = (n, m);
    let mut pairs = Vec::new();
    while i > 0 && j > 0 {
        let sim = similarity(i - 1, j - 1);
        if sim > 0.0 && (score[i][j] - (score[i - 1][j - 1] + sim)).abs() < f32::EPSILON {
            pairs.push((i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if score[i - 1][j] >= score[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    pairs.reverse();
    pairs
}

pub(crate) fn count_occurrences(values: &[u64]) -> rustc_hash::FxHashMap<u64, usize> {
    let mut counts = rustc_hash::FxHashMap::default();
    for &v in values {
        *counts.entry(v).or_insert(0) += 1;
    }
    counts
}

/// Total node count of the subtrees rooted at `ids`.
pub(crate) fn subtree_size_sum(ids: &[usize], meta: &ASTMetadata) -> usize {
    ids.iter()
        .map(|id| meta.node_to_subtree_size.get(id).copied().unwrap_or(0))
        .sum()
}

/// `node_ids` without the ones already in `node_map`.
pub(crate) fn filter_mapped_nodes(
    node_ids: &[usize],
    node_map: &rustc_hash::FxHashMap<usize, usize>,
) -> Vec<usize> {
    node_ids
        .iter()
        .copied()
        .filter(|node_id| !node_map.contains_key(node_id))
        .collect()
}
