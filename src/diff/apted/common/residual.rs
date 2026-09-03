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
//!
//! Split out of `common.rs`, which was 4,426 lines.

#[allow(unused_imports)]
use super::*;

/// Edit-distance cap for `resolve_residual_forest_via_myers_lcs`'s Myers diff - same role as
/// `FLAT_MAX_EDIT` for `resolve_flat_tree_pair`. If exceeded, every remaining node on both sides
/// is marked delete/insert instead of aligned.
pub(crate) const FALLBACK_MAX_EDIT: usize = 1000;

/// Minimum `node_to_subtree_size` a `resolve_unequal_segment_via_kind_only_anchors` candidate must
/// have on *both* sides before its `KindOnlyHash` match is trusted. `KindOnlyHash` never hashes
/// leaf values (`compute_kind_only_hash`, `code/hash.rs`), so small subtrees collide on shape alone
/// far more than their size suggests is safe - not just true leaves, but shallow, commonly-repeated
/// containers too (`html-gohugoio-hugo-enclose-table-with-div-and-add-thead-tbody`, 2026-08-16: two
/// *different* size-11 `element` nodes, both structurally unique within their segment - i.e. not an
/// ambiguous-hash case either - shared a `KindOnlyHash` purely from having the same tag/attribute
/// shape, and the wrong one got matched, regressing 9 mismatches to 27; `css-mozilla-firefox-
/// firefox-actual-style-changes` regressed the same way at size 14). The one confirmed genuine win
/// found this session (`vimscript-neovim-neovim-improved-asserts`, 53 mismatches to 0) anchored at
/// size 186 - over 10x either false positive - so 50 cleanly separates the known good case from the
/// known bad ones without being reverse-engineered to fit only one fixture.
pub(crate) const KIND_ONLY_ANCHOR_MIN_SIZE: usize = 50;

/// Max `node_to_subtree_size` for a `resolve_residual_forest_via_myers_lcs` segment entry to be
/// treated as a trivial leaf/punctuation node (a stray `;`, `,`, etc.) rather than real structural
/// content, when checking whether an unequal-count segment is actually a wrap/reparent case in
/// disguise. Started at 1 - matches true leaves (`subtree_size == 1`) and entries missing size data
/// entirely (`unwrap_or(0)`, `subtree_size == 0`), nothing larger - the one confirmed case
/// (`cpp-add-templates`, `class_specifier` size 49 wrapped by a new `template_declaration` size 58,
/// with an unrelated size-1 `;` in the same gap) only needs this much; widen only with corpus
/// evidence, per this file's established pattern of starting conservative on size-based trust
/// thresholds (`KIND_ONLY_ANCHOR_MIN_SIZE`'s own history is the cautionary example).
pub(crate) const TRIVIAL_ENTRY_MAX_SIZE: usize = 1;

/// Collects the root id of every *maximal* still-unmatched subtree under `root_id`: a preorder
/// walk that stops descending the instant it finds a node whose *entire* subtree is unmatched, so
/// one whole deleted/inserted block contributes exactly one sequence entry, not one per descendant
/// (generalizes `flat_children`'s "one entry per unmatched child" from one parent's direct
/// children to the whole tree). `node_map` is `diff.before_node_map`/`diff.after_node_map` for the
/// respective side.
///
/// Bug fixed 2026-08-15 (phases-4-7 rearchitecture, `TODO.md`): the original version stopped
/// descending the instant it found *any* unmatched node, `root_id` included - so whenever the
/// root itself was unmatched (true for almost every real edit, since the root's own content hash
/// changes with any edit anywhere in the file), the *entire file* collapsed into one sequence
/// entry, and any smaller genuinely-recoverable pocket nested inside it (e.g. a sibling
/// `attribute_item` or an unrelated, byte-identical enum variant, neither individually a
/// "reference node" or "big enough" for `solve_hash_descent`'s own selector) was silently marked
/// delete/insert instead of matched. Invisible before Phase 1 of this rearchitecture, which
/// promoted this function's caller (`resolve_residual_forest_via_myers_lcs`) from a rare
/// `DiffMode::Fast` safety-valve substitute to the unconditional terminal step - now each node's
/// "does descending still have a chance of finding something" question is answered by a single
/// postorder pass (`subtree_has_any_match`) computed once per call, so the fix stays O(n) like the
/// walk it replaces.
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
            // This node and everything under it is unmatched: nothing to gain by descending
            // further, so it's emitted as one atomic block (same intent as the original check).
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

/// Postorder fills `out[id] = true` iff some node strictly under `id` (not `id` itself) is present
/// in `node_map` - the precondition `maximal_unmatched_roots` needs to tell "genuinely nothing
/// recoverable in this subtree" apart from "unmatched itself, but has a matched descendant worth
/// digging for."
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

/// `DiffMode::Fast`'s substitute for full whole-tree APTED (phase 6) when
/// `PendingDiff::looks_expensive()` trips: collects every maximal still-unmatched subtree root on
/// each side (`maximal_unmatched_roots`), hashes each with its existing full-subtree content hash
/// (`ASTMetadata::node_to_full_hash`), and runs the same `myers_lcs` primitive
/// `resolve_flat_tree_pair` already uses for one parent's flat children - generalized here to the
/// whole residual forest rather than one parent's direct children. Deliberately does not call
/// `resolve_flat_tree_pair` itself, which is scoped to one parent's direct children
/// (`flat_children`, gated on `FLAT_MIN_CHILDREN`) and always emits one trailing root-pair
/// mapping - the residual here can be scattered across many disjoint subtrees on both sides, with
/// no single shared parent to anchor a trailing mapping to.
///
/// On an LCS hit, emits `emit_identical_subtree` (tagged `ASTMappingReason::APTED(source)`, same
/// convention as `for_nodes`/`for_roots`) for exact-hash pairs. Everything left unpaired between
/// two such anchors (or the sequence ends) then gets one more chance - equal-count segments recurse
/// per position (see below) - before falling back to `add_delete_mappings`/`add_insert_mappings`.
///
/// Phase 3b finding (`TODO.md`, 2026-08-15): a maximal-unmatched-root's *whole-subtree* hash can
/// fail to match even when almost everything inside it is real, reusable structure - a long chain
/// of the same repeated node kind (a left-associative `binary_expression` chain from string
/// concatenation; nested `element`s from nested `<li>`s) has every ancestor's hash change the
/// instant one link is removed, so this function used to see N differing entries where the truth
/// is one deletion plus N-1 relabels through the nesting - and, being a coarse exact-hash-only
/// pass by original design, atomically deleted+inserted every one of them. Confirmed directly (not
/// just inferred) that real bounded APTED already resolves this class correctly when given just
/// the affected region: isolated `apted::for_nodes` on one such fixture's affected `expression_
/// statement` (206/200 nodes) produced 187 Identical/13 MatchButNotIdentical/6 Delete - essentially
/// the correct alignment - rather than a wholesale replace.
///
/// Split entries left unpaired after the exact-hash pass into anchored segments (reusing `split_
/// into_anchored_segments`, keyed off the mappings the exact-hash pass above just wrote - the same
/// "diff each gap between confirmed anchors independently" idiom `resolve_flat_tree_pair` already
/// uses for one parent's flat children, generalized here to the whole-file residual's scattered,
/// unrelated maximal-unmatched-root sequence instead of one parent's ordered siblings). A segment
/// with *equal counts* on each side is recursed through real bounded APTED (`resolve_forest`,
/// `Algorithm::Apted`) instead of atomic delete/insert - one call per position, `before_seg[i]`
/// paired only with `after_seg[i]`, never pooled (unlike `resolve_flat_tree_pair`'s pooled
/// recursion of up to `FLAT_UNMATCHED_RECURSE_LIMIT` entries): `resolve_flat_tree_pair`'s entries
/// are genuine ordered siblings under one shared parent, while this residual's maximal-unmatched-
/// roots are scattered, semantically *unrelated* fragments from anywhere in the file. Confirmed
/// empirically that pooling here is unsafe, not just theoretically risky (`kotlin-refactor-
/// function`, 2026-08-15): with a multi-entry pool, APTED found a plausible-looking but wrong
/// cross-match between an unrelated deleted function's descendants and merely-similar nodes
/// elsewhere in the same gap, flipping a fixture from 0 to 32 mismatches. Per-position recursion
/// avoids that: each call only ever sees one candidate per side, a true, unambiguous 1:1 "this
/// replaced that" correspondence, with no room for APTED to invent a relationship across pairs -
/// generalizing the original "exactly one entry" case (still handled, as `N=1`) to any equal-count
/// gap. Unequal counts (a real insert/delete happened inside the gap too, so there's no fixed
/// positional correspondence left to exploit safely) fall back to the original atomic
/// delete/insert - this can only find *more* reuse than the purely exact-hash version, never less.
///
/// If the top-level `myers_lcs` itself gives up (edit distance exceeds `FALLBACK_MAX_EDIT`), there
/// are no exact-hash anchors to split around - `split_into_anchored_segments` degrades to a single
/// segment spanning everything, which will only qualify for recursion if both sides happen to have
/// equal counts, falling back to the original atomic behavior otherwise, same as before this
/// function recursed at all.
/// Resolve the trivial (leaf) entries the wrap/reparent branch filtered out, rescuing the ones
/// that were wrapped *along with* the code instead of deleting every one of them.
///
/// The calling branch pairs a reparented segment's substantial entries - a `class_specifier` that
/// became a `template_declaration`'s child - and used to delete and re-insert every leaf it had
/// filtered out. Right when the leaf is unrelated; wrong when the leaf moved with the code.
/// `cpp-add-templates` is the minimal case and was its entire remaining mismatch: the
/// declaration's trailing `;` ends up *inside* the new `template_declaration`, and got deleted.
///
/// Two things the tracing (2026-08-24) established, both of which shape this:
///
/// * The counterpart is **not a peer** in this segment - the before side has one leftover leaf and
///   the after side has zero, because the after `;` is a descendant of the partner. A
///   peer-to-peer pairing can never find it.
/// * By the time this runs, the substantial recursion above has already emitted that descendant
///   as an `Insert`, so it no longer looks unmatched. Rescuing it means *re-pointing* an existing
///   insert (`ASTDiff::remove_insert_mapping`), not matching a free node.
///
/// Deliberately narrow, because the enclosing function's doc comment records what happens when
/// this gap guesses: matching a `;` to "random other `;` in the code" is the exact failure it
/// warns about. A leftover leaf is rescued only when the partner subtree contains **exactly one**
/// inserted leaf of the same kind (`node_to_kind_only_hash`, which for a leaf is its kind). One
/// candidate means no choice is being made. Everything else keeps the old behaviour exactly.
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
        // The sole inserted leaf of this kind anywhere inside the reparented partners.
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
        // Never *pooled* (unlike `resolve_flat_tree_pair`'s own leftover recursion, which pools up
        // to `FLAT_UNMATCHED_RECURSE_LIMIT` entries because its entries are genuine ordered
        // siblings under one shared parent). Confirmed empirically that pooling here is unsafe:
        // this residual's maximal-unmatched-roots are scattered, semantically *unrelated*
        // fragments from anywhere in the file, so handing APTED a pool of several candidates per
        // side to freely choose among lets it invent a plausible-looking but wrong cross-match
        // between two merely-similar, unrelated fragments instead of correctly deleting one and
        // inserting the other - caught on `kotlin-refactor-function` (2026-08-15): an unrelated
        // deleted function's parameter/return-expression nodes got matched, via reason
        // `fast_fallback`, to look-alike nodes elsewhere in the same gap, flipping a fixture from 0
        // to 32 mismatches.
        //
        // Equal counts on both sides (2026-08-16) are still handled, just never pooled: each
        // before/after pair is recursed *individually*, at its fixed document-order position
        // within the gap (`before_seg[i]` paired only with `after_seg[i]`) - a true, unambiguous
        // 1:1 "this replaced that" correspondence for every pair, with no room for APTED to invent
        // a relationship across pairs, since each call only ever sees one candidate per side (the
        // same reasoning the original exactly-one-entry case already relied on, generalized from
        // "one pair" to "N independently-recursed pairs"). Mismatched counts (a real insert/delete
        // happened inside the gap too) fall through unchanged to atomic delete/insert - resolving
        // *which* subset corresponds needs real alignment info this gap doesn't have without
        // re-introducing exactly the pooling risk above.
        //
        // Uncapped in size, same as `resolve_flat_tree_pair`'s own equal-count branch (2026-08-16):
        // `RESIDUAL_SEGMENT_MAX_TOTAL_SIZE` bounded a *pool's* cost, but a per-position pair has
        // nothing to cross-match against regardless of size, so it was never protecting against a
        // correctness risk here either - only an unconfirmed latency one, and measurement (on the
        // sibling function) found no fixture worse and no latency movement once removed.
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
            // Wrap/reparent case (2026-08-17): a raw count mismatch can be entirely explained by a
            // trivial leaf entry (punctuation - a stray `;`, `,`, etc.) appearing alongside a real
            // structural change, e.g. `class_specifier` (before) becoming `template_declaration`'s
            // child (after) while an unrelated `;` in the same gap is a genuine, unrelated
            // delete/insert. Filtering out leaf entries (`subtree_size <= TRIVIAL_ENTRY_MAX_SIZE`)
            // from both sides first and re-checking for equal counts among what's left generalizes
            // the equal-count branch's safety argument unchanged: each substantial entry is still
            // the only candidate at its document-order position among substantial entries, so there
            // is still no room for APTED to invent a cross-match - the leaf entries it no longer has
            // to explain are resolved independently (delete/insert, never matched to anything),
            // exactly as an unmatched leaf would be resolved on its own anyway.
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

/// Unequal-count fallback for a `resolve_residual_forest_via_myers_lcs` gap (2026-08-16): rather
/// than atomically deleting every `before_seg` entry and inserting every `after_seg` entry, run a
/// second, finer `myers_lcs` pass over the segment's `node_to_kind_only_hash` values (the same
/// coarse-but-order-preserving discriminator phase 1's second hash pass already trusts globally,
/// here further constrained to entries already known to fall inside one shared gap between two
/// exact-hash anchors). A matched pair is still recursed per-position, one candidate per side,
/// exactly like the equal-count branch above - APTED never gets a pool to invent a cross-match
/// from, so the correctness argument that makes that branch safe (`kotlin-refactor-function`,
/// 2026-08-15) carries over verbatim. Entries `myers_lcs` leaves unpaired (a real insert/delete,
/// not just a same-shaped reparent) fall back to atomic delete/insert, same as before this
/// function existed - so this can only find *more* reuse than the plain unequal-count fallback,
/// never less, and degrades to today's behavior whenever no kind-only anchors are found.
///
/// Two extra safety filters beyond plain LCS matching, both found empirically necessary
/// (2026-08-16), not assumed up front - `KindOnlyHash`'s safety in phase 1 turned out to come from
/// its node selector (`reference_nodes_ordered`, large declaration-level nodes only), not from the
/// hash itself, which this local segment doesn't get for free:
/// - **`KIND_ONLY_ANCHOR_MIN_SIZE` floor**: `KindOnlyHash` hashes kind + child hashes but never leaf
///   values (`compute_kind_only_hash`, `code/hash.rs`), so shape alone drives the hash. At the
///   extreme (a leaf, `subtree_size == 1`) this reduces to a pure function of kind, colliding every
///   same-kind leaf in the segment - caught on `swift-swiftlang-swift-enable-checks-remove-todo-
///   comment`: two unrelated `comment` leaves got sub-anchored to each other, regressing 2
///   mismatches to 4. But it isn't only leaves: `html-gohugoio-hugo-enclose-table-with-div-and-add-
///   thead-tbody` regressed 9 mismatches to 27 from two *different*, size-11 `element` nodes (same
///   tag/attribute shape, unrelated content) sharing a hash with no ambiguity to catch (see below) -
///   `css-mozilla-firefox-firefox-actual-style-changes` regressed the same way at size 14. The one
///   confirmed genuine win found this session (`vimscript-neovim-neovim-improved-asserts`, 53 to 0)
///   anchored at size 186 - see `KIND_ONLY_ANCHOR_MIN_SIZE`'s own doc comment for why 50 was picked.
/// - **Segment-local uniqueness**: even above the size floor, a hash can still repeat within one
///   segment if two different candidates are genuinely the same shape - LCS will happily pick *some*
///   pairing among same-hash candidates, but nothing constrains it to the *true* correspondent
///   (unlike the equal-count branch's fixed positional pairing, which has only one candidate per
///   side by construction). A pair is only trusted if its hash value is unique within both
///   `before_seg` and `after_seg` - i.e. there was truly only one candidate per side.
///
/// Both filters only ever *withhold* a match, never invent one that plain LCS didn't already
/// propose - so an excluded entry just falls through to the atomic delete/insert loops below, same
/// as before this function existed.
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

    // `KIND_ONLY_ANCHOR_MIN_SIZE` and the ambiguity check below both exist to make `KindOnlyHash`
    // equality trustworthy - they are guards on *that* signal, not general-purpose caution. A
    // similarity-aligned pair carries a different and stronger signal (see
    // `align_segment_by_similarity`), so applying a size proxy meant for hash collisions to it
    // would reject exactly the small genuine matches it exists to find.
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

/// Minimum leaf-content Jaccard (`node_to_similarity_sketch`, a bottom-k MinHash over the subtree's
/// leaf hashes) for [`align_segment_by_similarity`] to call two residual entries the same thing.
///
/// Calibrated against every candidate the exact-hash path sees on this corpus (measured 2026-08-20,
/// 128 evaluations, 7 distinct pairs): the one known true positive
/// (`vimscript-neovim-neovim-improved-asserts`) scores **0.938**, while both documented
/// `KindOnlyHash` false positives - the size-11 `element` pair in `html-gohugoio-hugo-...` and the
/// size-14 pair in `css-mozilla-firefox-...` - score **0.556** and **0.538**, with nothing else
/// above 0.667. 0.9 separates them on *content*, which is the axis that actually distinguishes
/// them; `KIND_ONLY_ANCHOR_MIN_SIZE` separates the same three cases only because 186 happens to be
/// far from 11 and 14.
pub(crate) const SEGMENT_SIMILARITY_MIN: f32 = 0.9;

/// Largest `before.len() * after.len()` [`align_segment_by_similarity`] will run its O(n*m) DP over.
/// Every cell costs a bottom-k MinHash Jaccard, so this is a real cost bound, not a formality - and
/// this pass sits on the terminal fallback's path, which exists because p99 matters.
pub(crate) const SEGMENT_SIMILARITY_MAX_CELLS: usize = 4096;

/// Floor for [`align_segment_by_mutual_similarity`]: below this, two entries share too little
/// leaf content to be called an edit of one another even when nothing else is closer.
/// `javascript-add-event-listener`'s `button.onclick = handleClick;` against
/// `button.addEventListener('click', handleClick);` scores about a third - the genuine "this
/// statement was rewritten" floor this exists for; unrelated statements that merely share a
/// keyword and a semicolon sit well under it.
pub(crate) const SEGMENT_MUTUAL_SIMILARITY_MIN: f32 = 0.3;

/// The unequal-count gap's last resort before atomic delete/insert, for the shape
/// [`SEGMENT_SIMILARITY_MIN`]'s absolute floor cannot admit: one side's entries all have an
/// obvious counterpart on the other, and the surplus entries are plain inserts (or deletes).
/// `typescript-add-generics` is the canonical case - one `const` statement rewritten to use the
/// new generic *and* a second, brand-new `const` beside it; the rewritten pair scores ~0.5, which
/// `SEGMENT_SIMILARITY_MIN` (0.9, calibrated to reject two known 0.55 false positives) can never
/// accept, yet it is unmistakable *relative to the alternatives*.
///
/// So the criterion is relative, not absolute: a pair is accepted only when each is the other's
/// single best candidate (mutual best, same kind, at least
/// [`SEGMENT_MUTUAL_SIMILARITY_MIN`]), the accepted pairs are order-preserving, and **every entry
/// of the smaller side is paired** - the last condition is what makes "the rest are inserts" a
/// reading of the evidence rather than a guess, and what keeps a gap of unrelated fragments
/// (the `kotlin-refactor-function` pooling hazard) from being partially, wrongly stitched. Any
/// shortfall returns nothing and the gap falls through to atomic delete/insert exactly as before.
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
    // A declaration's own name: its first direct identifier-like child (`impl ModuleType`'s
    // `type_identifier`, a function's `identifier`). Only consulted for reference-node kinds.
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
                    // A named declaration whose name changed is not "the same thing rewritten"
                    // on similarity evidence alone: `rust-turbopack-module-rule`'s
                    // `impl ModuleType` scores 0.69 against the new `impl ConfiguredModuleType`
                    // (both are string-matching `from_str`s), and the human deletes one and
                    // inserts the other - the type `ModuleType` still exists, the impl for it
                    // simply went away. Renames the corpus does want paired arrive here
                    // already matched by the name-based passes, never through this gap.
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
    // "Single best": a strict maximum, so a tie between two candidates disqualifies both - there
    // is then no evidence which one is the counterpart.
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
    // Order-preserving (pairs come out sorted by `bi`; the `ai`s must be increasing too) and
    // covering the smaller side entirely - otherwise this is not the "rest are inserts" shape.
    let ordered = pairs.windows(2).all(|w| w[0].1 < w[1].1);
    if !ordered || pairs.len() != n.min(m) {
        pairs.clear();
    }
    pairs
}

/// Order-preserving alignment of one residual gap's entries by leaf-content similarity, for the
/// case where [`resolve_unequal_segment_via_kind_only_anchors`]' exact-hash pass found nothing.
///
/// That pass keys on `node_to_kind_only_hash`, which is coarser than the full hash but still an
/// *equality* test: two subtrees align only if their entire shape matches exactly, so one added
/// statement anywhere inside is enough to prevent it. Measured on the corpus (2026-08-20) that
/// leaves it nearly inert - 7 distinct candidate pairs across 468 fixtures - which is why the
/// unequal-count gap so often falls through to atomic delete/insert, and why 87% of this pass's
/// visible mismatches are nodes the human matched and it mapped to 0.
///
/// This is the same idea one step further: keep the order-preservation that makes the result safe
/// to recurse per-position (a pair is fixed before APTED ever sees it, so APTED still never gets a
/// pool to invent a cross-match from - see the caller's own note on `kotlin-refactor-function`),
/// but score candidate pairs by *similarity* instead of requiring hash equality. A standard LCS DP
/// maximising total similarity, with [`SEGMENT_SIMILARITY_MIN`] as the floor below which two
/// entries are not the same thing at all.
///
/// Only consulted when the exact-hash pass returns nothing, so it can only ever find *more* reuse,
/// never contradict a hash-exact alignment.
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

/// Counts how many times each value occurs in `values` - used by
/// `resolve_unequal_segment_via_kind_only_anchors` to detect a segment-local hash collision (more
/// than one same-hash candidate on a side, i.e. a genuinely ambiguous match) before trusting it.
pub(crate) fn count_occurrences(values: &[u64]) -> rustc_hash::FxHashMap<u64, usize> {
    let mut counts = rustc_hash::FxHashMap::default();
    for &v in values {
        *counts.entry(v).or_insert(0) += 1;
    }
    counts
}

/// Sums `node_to_subtree_size` over `ids` - the total node count a pooled `resolve_forest` call
/// would actually have to compare, used by `FLAT_UNMATCHED_RECURSE_MAX_TOTAL_SIZE`'s gate
/// (`resolve_flat_tree_pair`'s pooled unequal-count branch - the only remaining size-capped path;
/// both functions' equal-count/per-position branches are uncapped, see their own doc comments).
pub(crate) fn subtree_size_sum(ids: &[usize], meta: &ASTMetadata) -> usize {
    ids.iter()
        .map(|id| meta.node_to_subtree_size.get(id).copied().unwrap_or(0))
        .sum()
}

/// Filter out nodes already mapped in `node_map` (pass `diff.before_node_map`/
/// `diff.after_node_map` for the before/after side respectively). Takes `node_ids` by reference,
/// not by value: every `emit_*`/`emit_match` call site below already has a borrowed
/// `&info.children` in hand and previously had to `.clone()` it just to satisfy an owned-`Vec`
/// signature this function never needed (it only ever reads each id, never mutates or reuses the
/// input `Vec` itself).
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
