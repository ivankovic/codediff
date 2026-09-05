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
//! Pre-matching passes that pin obvious pairs before the general search runs: identical statement
//! siblings, and locals that are unique by name.
//!
//! Split out of `common.rs`, which was 4,426 lines.

#[allow(unused_imports)]
use super::*;

/// Minimum direct-child count worth pre-matching via [`prematch_identical_statement_siblings`] -
/// much lower than `FLAT_MIN_CHILDREN` (50). Safe to set low: unlike [`resolve_flat_tree_pair`],
/// that function never commits a non-match to delete/insert (see its own doc comment), so there is
/// no accuracy downside to trying it on a small sequence - only wasted lookup overhead on a
/// candidate with almost no children, which this excludes.
pub(crate) const STATEMENT_PREMATCH_MIN_CHILDREN: usize = 4;

/// Finds the largest `nodes::is_statement_sequence_body` descendant (inclusive of `root_id`
/// itself) via a plain walk - deliberately *not* `ASTMetadata::node_to_widest_subtree_node` (see
/// `is_statement_sequence_body`'s own doc comment for why that kind-agnostic precomputation can
/// pick the wrong, wider-but-irrelevant node). Bounded by `root_id`'s own subtree size (one
/// function/method, not the whole file), so a plain walk is cheap enough here - unlike
/// `solve_large_flat_subtrees`'s `largest_flat_container_in`, which needed the O(1) precomputation
/// specifically because it searches from the whole file's *many* top-level items.
pub(crate) fn widest_statement_sequence_body(
    root_id: usize,
    meta: &ASTMetadata,
) -> Option<(usize, usize)> {
    // Breadth-first, and returns the *first* (shallowest) match, not the widest one found overall
    // - a real regression this shape once had (`TODO.md`, 2026-08-05): a Python `for` loop's own
    // body is *also* a `block` (the same kind Python uses for a function's own top-level body), so
    // searching for "the widest `block` anywhere inside" could pick the outer function body on one
    // side of a diff but an inner loop's body on the other (whichever happened to have more direct
    // children that specific side), pairing two unrelated statement sequences. A candidate's own
    // top-level body is always the *shallowest* match - nothing legitimately "more it" can be
    // nested inside a shallower body of the same kind - so stopping at the first BFS level that
    // has any match at all, rather than continuing to search deeper for something wider, both
    // fixes that ambiguity and is cheaper (no need to walk past the level that already answered).
    let mut level = vec![root_id];
    while !level.is_empty() {
        let mut best: Option<(usize, usize)> = None;
        let mut next_level = Vec::new();
        for id in level {
            let Some(info) = meta.node_info.get(&id) else {
                continue;
            };
            if nodes::is_statement_sequence_body(&info.kind) {
                let count = info.children.len();
                if best.is_none_or(|(best_count, _)| count > best_count) {
                    best = Some((count, id));
                }
            } else {
                next_level.extend(info.children.iter().copied());
            }
        }
        if best.is_some() {
            return best;
        }
        level = next_level;
    }
    None
}

/// Pre-matches the byte-identical *direct children* of `before_id`/`after_id`'s own statement-
/// sequence body (`widest_statement_sequence_body` above) - intended to run right before a
/// named-group candidate's own container-wide `apted::for_nodes` call, so that call's
/// `PostorderIndexer` (which prunes any node already in `diff.before_node_map`/`after_node_map` -
/// see `PostorderIndexer::build`) has far less left to index once the mostly-unchanged statements
/// around one real edit are already resolved.
///
/// **Deliberately not `resolve_flat_tree_pair`, and not gated the same way**: that function is
/// safe at any size *because* it commits every remaining child to delete/insert once Myers can't
/// pair it - a sound tradeoff when there are enough siblings that a wrong call on one or two of
/// them barely moves the total, which is exactly what `FLAT_MIN_CHILDREN` = 50 is calibrated for.
/// For a function body with 10-40 statements, that tradeoff inverts: the 1-2 statements that
/// genuinely differ are worth a real, correctness-preserving tree-edit-distance resolution, not a
/// hard-committed guess. This function only ever emits the identical *matches* Myers finds
/// (`emit_identical_subtree`, exact scoped hash matches - the same safety property
/// `resolve_flat_tree_pair`'s matched half already has) and leaves everything else in `diff`
/// completely untouched, so the real APTED call that follows still gets to resolve those
/// genuinely-different statements properly - this can only ever shrink that call's own residual,
/// never take a decision away from it.
///
/// Measured live (2026-08-05, `TODO.md`): every dominant slow `apted::for_nodes` call examined in
/// this corpus (`rust-tauri-cli-ios-dev`, `ruby-homebrew-add-or-expression`, `cpp-ladybird-
/// refactor-variables-if-changes`, `c-linux-small-change-struct-to-char`) resolves to exactly this
/// shape: one large body blob, well under 50 direct children, 83-97% of them byte-identical to a
/// sibling on the other side.
pub(crate) fn prematch_identical_statement_siblings(
    before_id: usize,
    after_id: usize,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    source: &'static str,
    diff: &mut ASTDiff,
) {
    let Some((before_count, before_flat)) = widest_statement_sequence_body(before_id, before_meta)
    else {
        return;
    };
    let Some((after_count, after_flat)) = widest_statement_sequence_body(after_id, after_meta)
    else {
        return;
    };
    if before_count < STATEMENT_PREMATCH_MIN_CHILDREN
        || after_count < STATEMENT_PREMATCH_MIN_CHILDREN
    {
        return;
    }

    let Some(before_info) = before_meta.node_info.get(&before_flat) else {
        return;
    };
    let Some(after_info) = after_meta.node_info.get(&after_flat) else {
        return;
    };
    let before_children: Vec<usize> = before_info
        .children
        .iter()
        .copied()
        .filter(|id| !diff.before_node_map.contains_key(id))
        .collect();
    let after_children: Vec<usize> = after_info
        .children
        .iter()
        .copied()
        .filter(|id| !diff.after_node_map.contains_key(id))
        .collect();
    if before_children.is_empty() || after_children.is_empty() {
        return;
    }

    let before_hashes: Vec<u64> = before_children
        .iter()
        .map(|&id| before_meta.node_to_full_hash.get(&id).copied().unwrap_or(0))
        .collect();
    let after_hashes: Vec<u64> = after_children
        .iter()
        .map(|&id| after_meta.node_to_full_hash.get(&id).copied().unwrap_or(0))
        .collect();

    let Some(pairs) = myers_lcs(&before_hashes, &after_hashes, FLAT_MAX_EDIT) else {
        return;
    };
    for (bi, ai) in pairs {
        emit_identical_subtree(
            before_children[bi],
            after_children[ai],
            before_meta,
            after_meta,
            source,
            diff,
        );
    }
}

/// Recursive collector behind [`prematch_unique_named_locals`]: every descendant of `node_id`
/// (inclusive) for which `nodes::local_identity_name` returns an identity, bucketed by
/// `(kind_bucket, name)`. Stops descending into anything already in `node_map` - matching
/// `PostorderIndexer::build`'s own pruning, since a resolved subtree has nothing further to offer
/// here and would otherwise double-count. Deliberately does *not* stop at nested function/closure
/// boundaries: searching the whole subtree only makes the caller's uniqueness check *stricter*
/// (two same-named locals in different nested scopes count as 2, correctly disqualifying the
/// name, rather than being silently invisible to each other).
pub(crate) fn collect_local_identities(
    node_id: usize,
    meta: &ASTMetadata,
    node_map: &rustc_hash::FxHashMap<usize, usize>,
    language: &Language,
    groups: &mut rustc_hash::FxHashMap<(&'static str, String), Vec<usize>>,
) {
    if node_map.contains_key(&node_id) {
        return;
    }
    let Some(info) = meta.node_info.get(&node_id) else {
        return;
    };
    if let Some(key) = nodes::local_identity_name(node_id, meta, language) {
        groups.entry(key).or_default().push(node_id);
    }
    for &child_id in &info.children {
        collect_local_identities(child_id, meta, node_map, language, groups);
    }
}

/// Pre-matches scope-locally-named entities (parameters, local variable declarations, shell
/// variable assignments - see `nodes::local_identity_name`) within `before_id`/`after_id`'s
/// subtree whose name is unique on both sides, before that pair's own real APTED call.
///
/// **The gap this closes**: when new content is inserted mid-sequence (a new parameter, a new
/// local variable), everything after it shifts position. Unit-cost APTED has no notion that "same
/// name, different position" should beat "different name, same position" - it just prices
/// whichever pairing is cheaper under the raw cost model, which the shifted-identity pairing
/// often wins by accident (see `shellscript-ansible-ansible-add-variable-and-string-expansion`'s
/// test comment for a fully-worked cost example: matching by coincidental array-index position
/// beat matching by variable name by exactly 1 unit). Confirmed 2026-08-06 (`TODO.md`) on three
/// fixtures across three languages (Kotlin parameters, C# local variables, shell variable
/// assignments) - the same mechanism, recurring.
///
/// **Safety**: only pre-matches a pair when each side has *exactly one* candidate with that
/// `(kind_bucket, name)` key - an ambiguous (shadowed, overloaded, or duplicated) name is left
/// alone for real APTED to resolve however it can, never guessed at. Unlike
/// `prematch_identical_statement_siblings`, this never assumes the matched pair's *content* is
/// identical - each accepted pair gets a real, scoped `apted::for_nodes` call (the same idiom
/// `anchor_pair_via_apted` uses), so a pair whose content also changed (not just its position)
/// still gets a correct `MatchButNotIdentical` resolution instead of a false `Identical`.
pub(crate) fn prematch_unique_named_locals(
    before_id: usize,
    after_id: usize,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    source: &'static str,
    diff: &mut ASTDiff,
) {
    let language = before_meta.language;
    if !nodes::has_local_identity_coverage(&language) {
        return;
    }

    let mut before_groups: rustc_hash::FxHashMap<(&'static str, String), Vec<usize>> =
        rustc_hash::FxHashMap::default();
    collect_local_identities(
        before_id,
        before_meta,
        &diff.before_node_map,
        &language,
        &mut before_groups,
    );
    if before_groups.is_empty() {
        return;
    }
    let mut after_groups: rustc_hash::FxHashMap<(&'static str, String), Vec<usize>> =
        rustc_hash::FxHashMap::default();
    collect_local_identities(
        after_id,
        after_meta,
        &diff.after_node_map,
        &language,
        &mut after_groups,
    );
    if after_groups.is_empty() {
        return;
    }

    let mut pairs: Vec<(usize, usize)> = before_groups
        .iter()
        .filter(|(_, ids)| ids.len() == 1)
        .filter_map(|(key, before_ids)| {
            let after_ids = after_groups.get(key)?;
            (after_ids.len() == 1).then(|| (before_ids[0], after_ids[0]))
        })
        .collect();
    // Deterministic order (`HashMap` iteration order isn't) - and processing outer-to-inner by
    // document position, though any accepted pair here is already disjoint from every other by
    // construction (each node id appears in at most one bucket), so this is about reproducibility
    // across runs, not about later pairs depending on earlier ones the way some other passes do.
    pairs.sort_unstable_by_key(|&(b, _)| {
        before_meta
            .node_info
            .get(&b)
            .map(|i| i.preorder_index)
            .unwrap_or(usize::MAX)
    });

    for (before_child, after_child) in pairs {
        for_nodes(
            before_meta,
            after_meta,
            vec![before_child],
            vec![after_child],
            Algorithm::Apted,
            source,
            diff,
        );
    }
}

// --- The terminal whole-residual fallback: Myers O(ND) sequence diff, generalized from
// `resolve_flat_tree_pair`'s one-parent's-direct-children scope to the entire still-unmatched
// forest under a root pair. ---
