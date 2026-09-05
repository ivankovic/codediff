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
//!
//! Split out of `common.rs`, which was 4,426 lines.

#[allow(unused_imports)]
use super::*;

/// Minimum number of still-unmatched direct children (leaves or subtrees - see `flat_children`)
/// required to trigger the flat-tree optimisation.
pub(crate) const FLAT_MIN_CHILDREN: usize = 50;
/// Edit-distance cap for Myers diff. If d exceeds this, we fall back to mark-as-replaced.
pub(crate) const FLAT_MAX_EDIT: usize = 1000;

/// Returns `root_id`'s full direct-children list (not just the unmatched ones) if it has at
/// least `FLAT_MIN_CHILDREN` still-unmatched children. Children may be leaves or interior nodes;
/// all mapping helpers (`emit_identical_subtree`, `add_delete/insert_mappings`) handle subtrees
/// recursively, so depth-1 is not a requirement.
///
/// Used to filter out already-matched children before returning them at all - correct as long as
/// document order alone disambiguates the rest, but wrong once some of those already-matched
/// children are themselves the only thing that *can* disambiguate a run of otherwise-identical
/// siblings (e.g. XML whitespace `CharData` between already-matched `element`s - see
/// `resolve_flat_tree_pair`'s doc comment for the mechanism and a confirmed live case). The full
/// list, still gated on the same unmatched-count threshold, lets the caller split on those
/// already-matched positions instead of discarding them.
///
/// The gate is the *unmatched* count on purpose. Tree-edit-distance cost is driven by total node
/// count, not entry count, which is why `resolve_flat_tree_pair`'s leftover recursion is capped
/// by `FLAT_UNMATCHED_RECURSE_MAX_TOTAL_SIZE` as well as `FLAT_UNMATCHED_RECURSE_LIMIT` (see that
/// constant's doc comment for the corpus measurement). The gate was switched to the total count
/// once, reverted, and re-attempted with that cap - the chronology and the fixtures it broke are
/// in `src/diff/TODO.md` under "Design history moved out of source".
pub(crate) fn flat_children(root_id: usize, meta: &ASTMetadata) -> Option<Vec<usize>> {
    let info = meta.node_info.get(&root_id)?;
    if info.children.len() >= FLAT_MIN_CHILDREN {
        Some(info.children.clone())
    } else {
        None
    }
}

/// `root_id`'s one flat child (`flat_children`), if exactly one of its children is flat - the
/// "thin wrapper around a flat container" shape `resolve_forest` decomposes rather than solving
/// as one tree. The wrapper's other children (the `struct` keyword, the type name, braces, an
/// `attribute_specifier` - `c-sched-ext-scx`'s structs carry `__attribute__((aligned))` after
/// the field list) are resolved by the same child-sequence pass and may be anything but flat
/// themselves; two flat children would mean the wrapper's own structure is the edit.
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
///
/// `pub(crate)`, re-exported through `apted::myers_lcs`: also used by `diff::text::
/// plain_text_line_diff` (a plain line-level diff for files with no tree-sitter grammar), the one
/// caller outside this module - same primitive, applied to hashed lines instead of hashed subtree
/// roots.
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
            // Choose whether to arrive via a delete (from k-1) or insert (from k+1).
            let x = if k == -(d as i64) {
                v[ki + 1] // forced insert
            } else if k == d as i64 || v[ki - 1] >= v[ki + 1] {
                v[ki - 1] + 1 // delete
            } else {
                v[ki + 1] // insert
            };
            let mut x = x;
            let mut y = (x as i64 - k) as usize;
            // Extend snake (diagonal matches).
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
        // First position on diagonal k after the non-diagonal move.
        let x_enter = if prev_k < k { prev_x + 1 } else { prev_x };
        // Collect snake matches (x_enter..x) in reverse.
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

/// Above this many Myers-unmatched entries on either side, [`resolve_flat_tree_pair`] falls back
/// to the old atomic delete/insert behavior instead of recursing them through APTED - see that
/// function's doc comment for why the recursion exists and why it needs a cap at all. Deliberately
/// small (not `FLAT_MAX_EDIT`'s 1000): the residual here is exactly the content Myers *couldn't*
/// place, i.e. plausibly-real edits worth resolving properly, not the "so much changed, don't
/// bother" case `FLAT_MAX_EDIT` guards against - but each entry can itself be an arbitrarily large
/// subtree, so recursing an unbounded *number* of them would reintroduce the same "large residual,
/// full tree-edit-distance" cost this whole fast path exists to avoid. 20 covers "a handful of
/// entries were actually edited" (confirmed against the fixtures that motivated this - see TODO.md
/// 2026-08-08) with real margin while still bailing out for a large-scale rewrite.
pub(crate) const FLAT_UNMATCHED_RECURSE_LIMIT: usize = 20;

/// Total-node-count cap (summed across every leftover entry on *both* sides combined) for
/// `resolve_flat_tree_pair`'s leftover recursion when there's more than one entry on either side,
/// alongside `FLAT_UNMATCHED_RECURSE_LIMIT`'s entry count (the exactly-one-entry-per-side case is
/// exempt from this cap entirely - see the call site). Added 2026-08-16 (phases-4-7 rearchitecture,
/// `TODO.md`) closing the gap a 2026-08-14 attempt at widening `flat_children`'s gate got stuck on:
/// entry count alone is the wrong signal for "is real APTED still affordable on this residual" -
/// tree-edit-distance cost is driven by total node count, not how many separate entries it's split
/// across.
///
/// An uncapped version (this constant removed entirely, always recursing regardless of size,
/// including multi-entry pools) was tried and reverted the same day: it fixed `xml-odoo-odoo-add-
/// two-attributes` (turned out to be the single-entry case now exempted above), but made `tsx-
/// excalidraw-excalidraw-huge-file-with-real-logic-change` (6 before / 12 after - a genuine
/// multi-candidate pool with mismatched counts) *worse* on both axes at once (1458 mismatches and
/// 19.3s, vs. 231 mismatches and 2.4s capped) - a large, unbounded, multi-entry pooled real-APTED
/// call can apparently produce a *worse* match than atomic delete/insert would for some of those
/// entries (plausibly the same cost-minimization-picks-a-plausible-but-wrong-cross-match risk
/// Phase 3b's own single-entry-gap work hit and fixed by restricting to exactly one entry), not
/// just a slower one. So for the genuine multi-entry-pool case this cap isn't only a latency
/// guard, it's also load-bearing for quality - re-verify both axes together (not just latency)
/// before ever loosening it for that case.
pub(crate) const FLAT_UNMATCHED_RECURSE_MAX_TOTAL_SIZE: usize = 2000;

/// Splits `before_children`/`after_children` into segments delimited by children already
/// matched in `diff` (typically resolved by an earlier phase's name-based matching, e.g.
/// `nodes::is_reference` on XML `element`s, before this flat-diff pass ever runs) - see
/// `resolve_flat_tree_pair`'s doc comment for why the split matters. Reduces to a single segment
/// spanning the whole list, identical to the pre-split behavior, whenever nothing is matched yet.
///
/// An anchor is only used as a split point if its counterpart is still ahead of the last split
/// point on the after side - defensive against a match that landed outside `after_children`
/// entirely (or, in principle, out of order); either way not a usable local anchor, so it's left
/// where it falls instead. Each returned segment is pre-filtered to drop any child still matched
/// in `diff` (mirrors `flat_children`'s original exclusion, now applied per segment).
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

/// Resolve a flat-tree root pair via Myers sequence diff and emit all mappings into `diff`.
///
/// Runs Myers per segment, split at children already matched in `diff` (`split_into_anchored_
/// segments`), rather than once over the whole pooled list of still-unmatched children. Pooling
/// everything together - the original behavior - discards exactly the anchors that would
/// otherwise disambiguate a run of hash-identical children: a run of N indistinguishable entries
/// with one inserted or deleted somewhere inside gives Myers N tied-optimal alignments to choose
/// from, and its own tie-break (not ground truth) decides which one "moved". Splitting first means
/// each run is diffed independently between its bounding anchors, so a shift on one side of an
/// anchor can no longer misalign anything on the other side of it - confirmed against a live case
/// (`xml-nextcloud-android-delete-element`: one `<string>` deleted from ~1137 already-matched
/// `element` siblings; the resulting drift chain spanned every remaining whitespace `CharData`
/// node after it, since none of the ~1137 anchors were part of the Myers input at all).
///
/// Whether the whitespace immediately before or after the deleted entry is "the" deleted one
/// remains a genuine tie even after splitting - a segment of length >1 either side of a single
/// deletion has no ground truth to prefer one over the other. This narrows the tie to that one
/// local segment instead of letting it propagate through the rest of the list.
///
/// Reduces to exactly the old single-pool behavior whenever nothing is matched yet (one segment,
/// spanning the whole list) - the common case where the flat parent itself was just matched and
/// none of its children have been touched by an earlier phase.
// Each parameter is a genuinely distinct piece of context (both roots, both metadata sets, the
// pre-computed children, the source string, the mutable diff) - grouping them into a struct built
// once at this single call site would just move the same information around, not clarify it.
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

/// Second anchoring pass over a container's exact-hash leftovers, keyed on each member's declared
/// name (`nodes::member_identity_name`): a leftover pair with the same `(kind, name)`, unique
/// among the leftovers on both sides, is resolved on its own through `resolve_forest` (a real,
/// scoped, size-gated APTED call, so a member whose body changed gets a proper
/// `MatchButNotIdentical` mapping, never a false `Identical`). Returns whatever is still
/// unanchored, in document order, for the positional / pooled / atomic handling that follows.
///
/// The gap this closes is the flat-container pass's biggest: it anchors direct children by
/// *whole-subtree* hash only, so every member whose body changed at all fell to the leftover
/// branches, none of which used the member's identity. Two fixtures put ~15% of the corpus's
/// mismatches there (2026-09-01): `java-pdftk-...-real-change-all-across-the-file` (888 - more
/// than 20 changed methods trip the pool cap, and the cap-exceeded branch deletes every one of
/// them wholesale) and `c-sched-ext-scx-many-many-moves-...` (245 - the equal-count zip pairs
/// near-identical `volatile <type> <name>;` fields with the wrong twins).
///
/// Same shape as `prematch_unique_named_locals` (unique-on-both-sides, per-pair scoped APTED) and
/// the kind-only sub-anchor before it; an exact declared name is a far less ambiguous key than a
/// kind-only hash, so it needs no size floor. It only ever *adds* same-name pairs that the exact
/// hash missed - a name present on one side only is left exactly where it was.
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
    // A name with exactly one holder per side is an unambiguous pair. A name with the *same*
    // number of holders on both sides - Java's overloads and constructors, which all carry the
    // class's own name - is zipped in document order: overloads keep their relative order across
    // an edit far more often than not, and the alternative (leaving every constructor to the
    // pool caps) is what deleted all six of `java-pdftk-...`'s constructors wholesale. A name
    // whose count differs (an overload was added or removed) is left alone: which one is new is
    // exactly the question this pass has no evidence for.
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
    // Deterministic order - `HashMap` iteration isn't - by document position.
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

/// How [`resolve_child_sequence`] treats leftovers whose counts differ on the two sides - the one
/// place a pooled, multi-candidate APTED call is on the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LeftoverPool {
    /// A flat container's children: pool up to `FLAT_UNMATCHED_RECURSE_LIMIT` entries /
    /// `FLAT_UNMATCHED_RECURSE_MAX_TOTAL_SIZE` nodes, else atomic delete/insert - the caps whose
    /// history is on those constants.
    Flat,
    /// An oversized pair's children (`resolve_oversized_pair`): the whole point is to stay under
    /// `APTED_MAX_CELLS`, so the pool is bounded by that product rather than by entry count, and
    /// a pool that still doesn't fit goes through `resolve_unequal_segment_via_kind_only_anchors`
    /// (shape anchors, then similarity, then atomic) instead of straight to atomic - the pair
    /// would have been solved exactly by APTED before the gate existed, so plain atomic
    /// delete/insert here is the one outcome measurably worse than the old behaviour
    /// (`lua-luakit-...-merging-two-tests-into-one` 107 -> 389 with atomic, 2026-09-02).
    Oversized,
}

/// The child-level half of [`resolve_flat_tree_pair`], without the root pairing: anchors
/// `before_children`/`after_children` by exact hash (Myers per already-anchored segment), then
/// resolves the leftovers positionally, as a bounded pool, or as atomic delete/insert - see the
/// comments inside for each branch's history. Nothing here assumes the children are leaves; the
/// "flat" in the caller's name is that caller's gate, not this function's requirement, which is
/// what lets [`resolve_oversized_pair`] reuse it for an arbitrarily deep child list.
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
                    // Matched by identical hash.
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
                // Edit distance exceeds FLAT_MAX_EDIT within this segment: mark it fully replaced.
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

    // A Myers-unmatched entry only failed *exact-hash* equality - it can still share real
    // structure with an entry on the other side (a small edit inside an otherwise-
    // unchanged dictionary/list entry, say). Recurse the leftover through real APTED instead of
    // unconditionally treating every one of them as fully replaced - `resolve_forest` still has
    // delete/insert available for genuinely unrelated pairs, so this can only find *more* reuse
    // than the atomic version, never less. Confirmed against a live case (`vimscript-neovim-
    // neovim-i-have-no-idea-what-this-diff-does`: one dictionary entry out of ~185 had an internal
    // edit; the atomic version deleted+inserted all ~40 of its descendant nodes instead of
    // recognizing the ~38 that were untouched - see TODO.md's 2026-08-08 entry).
    //
    // Equal counts on both sides are recursed *per position*, never pooled, and **uncapped in
    // size** (2026-08-16, phases-4-7 rearchitecture `TODO.md`): `before_unmatched[i]` against only
    // `after_unmatched[i]`, one independent `resolve_forest` call per pair - a true, unambiguous
    // 1:1 "this replaced that" correspondence for every pair, with no room for APTED to invent a
    // relationship across pairs. Pooling equal-count entries is unsafe even at small scale, and
    // a size cap on per-position pairs costs real fixtures for no benefit: a per-position pair
    // has nothing to cross-match against regardless of its size, so there was never a
    // correctness reason to cap it. The fixtures that established both are in `src/diff/TODO.md`
    // under "Design history moved out of source".
    //
    // Unequal counts (a real insert/delete happened inside the segment too, so there's no fixed
    // positional correspondence) still use the original pooled `resolve_forest` call, bounded by
    // both `FLAT_UNMATCHED_RECURSE_LIMIT` (entry count) and `FLAT_UNMATCHED_RECURSE_MAX_TOTAL_SIZE`
    // (total node count) - pooling is a genuine, deliberate risk/reward trade for this case (no
    // safer alternative exists short of atomic delete/insert), not an oversight, and *does* still
    // need the size cap since a pool's cost genuinely depends on its combined size.
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
