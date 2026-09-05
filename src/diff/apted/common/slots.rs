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
//! Slot repair and promotion: the heuristics that decide whether a match found in one child slot
//! should be pulled up, pushed down, or rejected.
//!
//! Split out of `common.rs`, which was 4,426 lines.

#[allow(unused_imports)]
use super::*;

/// Cost charged for a `ren()` pairing that `ContainmentCtx` has vetoed - deliberately the same
/// value `UnitCostModel::ren` already uses for *disallowed* mismatched kinds (kinds not covered by
/// `kinds_update_allowed`), so a containment-inconsistent pairing is exactly as unattractive to
/// the DP, never merely "more expensive than the best alternative" (which could still lose to an
/// equally bad alternative pairing). Note this is strictly more than the `COST_UPDATE` charged for
/// an *allowed* cross-kind pairing, so the veto still dominates even over that cheaper option.
pub(crate) const FORBIDDEN_RENAME_COST: u64 = COST_DELETE + COST_INSERT + 1;

/// The match target of a before-tree node per the *current* state of this call's decisions plus
/// anything an earlier, coarser pass already anchored (which the DP never revisits). `None` for
/// deleted and undecided nodes.
pub(crate) fn before_match_target(
    id: usize,
    before_decision: &HashMap<usize, BeforeDecision>,
    diff: &ASTDiff,
) -> Option<usize> {
    match_target(id, before_decision, &diff.before_node_map)
}

/// After-side counterpart of `before_match_target`.
pub(crate) fn after_match_target(
    id: usize,
    after_decision: &HashMap<usize, AfterDecision>,
    diff: &ASTDiff,
) -> Option<usize> {
    match_target(id, after_decision, &diff.after_node_map)
}

/// One side's match target: this call's decision if it made one, else the earlier passes'
/// anchor in `node_map` (`0` there means pruned, not matched).
fn match_target<D: SideDecision>(
    id: usize,
    decisions: &HashMap<usize, D>,
    node_map: &rustc_hash::FxHashMap<usize, usize>,
) -> Option<usize> {
    match decisions.get(&id) {
        Some(decision) => decision.match_target(),
        None => node_map.get(&id).copied().filter(|&t| t != 0),
    }
}

/// The ancestor of `node` that is a *direct child* of `ancestor`, or `None` if `ancestor` isn't
/// on `node`'s parent chain. (Returns `node` itself when `node`'s parent is `ancestor`.)
pub(crate) fn ancestor_child_of(
    node: usize,
    ancestor: usize,
    parents: &rustc_hash::FxHashMap<usize, usize>,
) -> Option<usize> {
    let mut cur = node;
    while let Some(&p) = parents.get(&cur) {
        if p == ancestor {
            return Some(cur);
        }
        cur = p;
    }
    None
}

/// Per-node outcome `collect_subtree_targets` needs from its side-specific classifier: whether to
/// record a match target and/or keep recursing into the child. See `collect_before_subtree_targets`'s
/// doc comment for what each variant means in practice.
pub(crate) enum SubtreeTargetOutcome {
    /// A fresh `Match(t)` decision: record `t`, then recurse (the child's own children carry
    /// their own independent decisions).
    MatchAndRecurse(usize),
    /// A fresh `Delete`/`Insert` decision: nothing to record here, but still recurse.
    PruneRecurse,
    /// No fresh decision either way: record the pre-existing anchor if one exists (from an
    /// earlier, coarser pass), but don't recurse - such subtrees are already consistently mapped
    /// below their boundary, so nothing further down can contradict a containment check that the
    /// boundary target itself passes.
    Leaf(Option<usize>),
}

/// Collects the match targets of every matched node strictly below `root`, per `classify`'s
/// per-child verdict (see `SubtreeTargetOutcome`). Shared recursion behind
/// `collect_before_subtree_targets`/`collect_after_subtree_targets`, which differ only in which
/// side's `node_info`/decision map/node map they classify against.
pub(crate) fn collect_subtree_targets(
    root: usize,
    meta: &ASTMetadata,
    out: &mut Vec<usize>,
    classify: &impl Fn(usize) -> SubtreeTargetOutcome,
) {
    let Some(info) = meta.node_info.get(&root) else {
        return;
    };
    for &child in &info.children {
        match classify(child) {
            SubtreeTargetOutcome::MatchAndRecurse(t) => {
                out.push(t);
                collect_subtree_targets(child, meta, out, classify);
            }
            SubtreeTargetOutcome::PruneRecurse => {
                collect_subtree_targets(child, meta, out, classify);
            }
            SubtreeTargetOutcome::Leaf(target) => {
                if let Some(t) = target {
                    out.push(t);
                }
            }
        }
    }
}

/// Collects the match targets of every matched node strictly below `root` in the before tree:
/// fresh `Match` decisions (recursing past them, since their children carry their own independent
/// decisions) and pre-existing anchors from earlier passes (not recursing past those - earlier
/// passes map whole subtrees consistently under the node they fixed, so the boundary target is
/// enough for any containment question).
pub(crate) fn collect_before_subtree_targets(
    root: usize,
    before_meta: &ASTMetadata,
    before_decision: &HashMap<usize, BeforeDecision>,
    diff: &ASTDiff,
    out: &mut Vec<usize>,
) {
    collect_side_subtree_targets(
        root,
        before_meta,
        before_decision,
        &diff.before_node_map,
        out,
    );
}

/// After-side counterpart of `collect_before_subtree_targets`.
pub(crate) fn collect_after_subtree_targets(
    root: usize,
    after_meta: &ASTMetadata,
    after_decision: &HashMap<usize, AfterDecision>,
    diff: &ASTDiff,
    out: &mut Vec<usize>,
) {
    collect_side_subtree_targets(root, after_meta, after_decision, &diff.after_node_map, out);
}

/// The shared body of the two above: a fresh match is collected and recursed past, a fresh prune
/// is recursed past, and an earlier pass's anchor (or nothing) is a leaf.
fn collect_side_subtree_targets<D: SideDecision>(
    root: usize,
    meta: &ASTMetadata,
    decisions: &HashMap<usize, D>,
    node_map: &rustc_hash::FxHashMap<usize, usize>,
    out: &mut Vec<usize>,
) {
    collect_subtree_targets(root, meta, out, &|child| match decisions.get(&child) {
        Some(decision) => match decision.match_target() {
            Some(t) => SubtreeTargetOutcome::MatchAndRecurse(t),
            None => SubtreeTargetOutcome::PruneRecurse,
        },
        None => SubtreeTargetOutcome::Leaf(node_map.get(&child).copied().filter(|&t| t != 0)),
    });
}

/// Post-DP slot alignment: reshapes cost-*neutral* corners of the DP's decision so they read the
/// way a human would, without ever making the mapping more expensive. Three mechanisms, in order:
///
/// 1. `validate_fresh_matches`: demote matches with no contextual support - stray generic tokens,
///    cross-context Updates, and internal "skeleton" islands the DP reused across unrelated
///    regions purely because same-kind renames are free. Running this first cleans the decisions
///    the later mechanisms' containment guards consult.
///
/// 2. `pull_up_wrapped_matches`: the DP frequently faces exact ties between matching a node to its
///    same-slot counterpart vs. to an identical-content node one wrapper level deeper (wrapping a
///    block in `try { ... }` creates two same-kind blocks with the same content; both targets cost
///    the same). It breaks such ties arbitrarily; humans always read the *slot* (parent-adjacent)
///    pairing as "the same node" and the deeper one as new. Retargeting between tie candidates is
///    cost-neutral by construction: same-kind internal renames cost 0 either way, and one node of
///    the pair ends up inserted either way.
///
/// 3. `promote_same_slot_pairs`: when a matched parent pair has a deleted child and an inserted
///    child of the same kind in corresponding sibling positions (LCS), match them instead -
///    `MatchButNotIdentical` in the emitted diff, i.e. "this statement, edited". This is never
///    more expensive (a same-kind rename is 0 vs. the 2 the delete+insert paid), so if the DP
///    didn't take it, it was blocked by then-conflicting descendant matches or tied; the
///    containment guard re-checks against the *current* (post-validation, post-pull-up) decisions.
// Each parameter is genuinely distinct read/mutable state (both metadata sets, the diff, the
// parent-lookup tables, the two decision maps) - a params struct here would just relocate the
// same fields, not reduce them.
#[allow(clippy::too_many_arguments)]
pub(crate) fn improve_slot_alignment(
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    diff: &ASTDiff,
    before_root_ids: &[usize],
    before_parents: &rustc_hash::FxHashMap<usize, usize>,
    after_parents: &rustc_hash::FxHashMap<usize, usize>,
    before_decision: &mut HashMap<usize, BeforeDecision>,
    after_decision: &mut HashMap<usize, AfterDecision>,
) {
    let before_forest_roots: std::collections::HashSet<usize> =
        before_root_ids.iter().copied().collect();
    let ctx = SlotCtx {
        before_meta,
        after_meta,
        diff,
        before_parents,
        after_parents,
        before_forest_roots: &before_forest_roots,
    };
    validate_fresh_matches(&ctx, before_decision, after_decision);
    pull_up_wrapped_matches(
        before_meta,
        after_meta,
        diff,
        before_parents,
        after_parents,
        before_decision,
        after_decision,
    );
    // Pull-up demotes the old (deeper) partner of every retargeted pair, which can strand that
    // partner's descendants' matches without the context that justified them a moment ago (e.g.
    // kotlin-add-data-class: the inner `user_type`'s identifier Update loses its matched parent
    // when the outer `user_type` takes over the slot). Re-validate so those get re-checked
    // against the post-pull-up decisions; promotion below then re-adds anything slot-consistent.
    validate_fresh_matches(&ctx, before_decision, after_decision);
    promote_same_slot_pairs(
        before_meta,
        after_meta,
        diff,
        before_parents,
        after_parents,
        before_decision,
        after_decision,
    );
}

/// True if `root`'s subtree (in the before tree) contains any leaf that is *not* a generic
/// punctuation/operator token - i.e. an identifier, literal, or keyword a human would recognize
/// as content. An identical-hash pair with no such leaf (`()` argument lists, brace scaffolding)
/// is structure, not evidence.
pub(crate) fn subtree_has_content(root: usize, meta: &ASTMetadata) -> bool {
    let Some(info) = meta.node_info.get(&root) else {
        return false;
    };
    if info.children.is_empty() {
        return !nodes::is_generic_token_kind(&info.kind);
    }
    info.children
        .iter()
        .any(|&child| subtree_has_content(child, meta))
}

/// Validates every fresh `Match` decision against its context, demoting the unsupported ones to
/// delete+insert - the codified version of "small elements are not independent; they belong to
/// the logical unit around them". Runs before the slot mechanisms below, so their containment
/// guards see cleaned decisions (a stray `(` pairing across half a function otherwise blocks a
/// legitimate `return_statement` slot promotion forever).
///
/// Processed parents-before-children (ascending depth) with live state, so a demoted container's
/// children are evaluated as the island roots they've just become - a controlled cascade in which
/// every node still gets its own keep-checks, rather than the blanket subtree demotion HANDOVER.md
/// warns about.
///
/// Internal pairs ("islands"): a pair whose parents are *both* unmatched has no context vouching
/// for it. It survives only if a matched before-ancestor sits within `MAX_CONTEXT_ANCESTOR_DEPTH`
/// (the pair is inside a rewritten-but-corresponding region: `button.onclick = h` becoming
/// `button.addEventListener(h)` under the same matched expression_statement), or if the two
/// subtrees are byte-identical *and* contain real content (an identical `return None` relocating
/// between an `else` and an `except` arm is a real match; an identical `()` is scaffolding).
///
/// Leaf pairs: generic tokens need two-sided small context (`update_context_supported`);
/// same-kind different-text Updates need that same context or textual similarity; identical-text
/// leaves need a matched before-ancestor within `MAX_UPDATE_CONTEXT_ANCESTOR_DEPTH` (one level
/// looser - identical text is stronger evidence, and legitimate cases sit deeper, e.g. a
/// parameter under `function_expression > formal_parameters` whose matched container is the
/// `arguments` three levels up in javascript-refactor-arrow-func).
pub(crate) fn validate_fresh_matches(
    ctx: &SlotCtx,
    before_decision: &mut HashMap<usize, BeforeDecision>,
    after_decision: &mut HashMap<usize, AfterDecision>,
) {
    let depth_of = |mut node: usize| -> usize {
        let mut depth = 0;
        while let Some(&p) = ctx.before_parents.get(&node) {
            depth += 1;
            node = p;
        }
        depth
    };

    // Ordered by depth, then by document position, then by preorder index (not node id):
    // `before_decision` is a `HashMap`, so the source order here is hash-seeded, and node ids are
    // arena slots that aren't stable across separate parses of identical source - a node_id
    // tiebreak would still let a demotion cascade differently between process runs even though it
    // looks stable within one. `start_byte` alone isn't enough either: an ancestor and its
    // leftmost descendant share a start byte, so ties there still need breaking - `preorder_index`
    // is unique per node and, like `start_byte`, a pure function of the tree's shape.
    let mut pairs: Vec<(usize, usize, usize, usize, usize, usize, usize)> = before_decision
        .iter()
        .filter_map(|(&b, d)| match d {
            BeforeDecision::Match(a) => {
                let before_info = ctx.before_meta.node_info.get(&b)?;
                let after_info = ctx.after_meta.node_info.get(a)?;
                Some((
                    depth_of(b),
                    before_info.start_byte,
                    after_info.start_byte,
                    before_info.preorder_index,
                    after_info.preorder_index,
                    b,
                    *a,
                ))
            }
            BeforeDecision::Delete => None,
        })
        .collect();
    pairs.sort_unstable();

    for (_, _, _, _, _, b, a) in pairs {
        if before_decision.get(&b) != Some(&BeforeDecision::Match(a)) {
            continue;
        }
        // A forest root's context is the caller's business, not ours - see `SlotCtx`.
        if ctx.before_forest_roots.contains(&b) {
            continue;
        }
        let (Some(b_info), Some(a_info)) = (
            ctx.before_meta.node_info.get(&b),
            ctx.after_meta.node_info.get(&a),
        ) else {
            continue;
        };

        let keep = if b_info.children.is_empty() && a_info.children.is_empty() {
            leaf_match_supported(b, a, b_info, a_info, ctx, before_decision)
        } else {
            island_match_supported(b, a, ctx, before_decision, after_decision)
        };
        if !keep {
            before_decision.insert(b, BeforeDecision::Delete);
            after_decision.insert(a, AfterDecision::Insert);
        }
    }
}

/// The internal-pair arm of `validate_fresh_matches` - see its doc comment.
pub(crate) fn island_match_supported(
    b: usize,
    a: usize,
    ctx: &SlotCtx,
    before_decision: &HashMap<usize, BeforeDecision>,
    after_decision: &HashMap<usize, AfterDecision>,
) -> bool {
    // A tree root has no context to be judged against - pairing the two roots is definitionally
    // correct, not a graft.
    let (Some(&pb), Some(&pa)) = (ctx.before_parents.get(&b), ctx.after_parents.get(&a)) else {
        return true;
    };
    // Anchored context: either immediate parent is matched.
    if before_match_target(pb, before_decision, ctx.diff).is_some()
        || after_match_target(pa, after_decision, ctx.diff).is_some()
    {
        return true;
    }
    // Rewritten-but-corresponding region: a matched before-ancestor close above.
    if has_nearby_matched_ancestor(b, MAX_CONTEXT_ANCESTOR_DEPTH, ctx, before_decision) {
        return true;
    }
    // Byte-identical content-bearing subtree: a real match wherever it sits.
    let hashes_match = ctx
        .before_meta
        .node_to_full_hash
        .get(&b)
        .zip(ctx.after_meta.node_to_full_hash.get(&a))
        .is_some_and(|(bh, ah)| bh == ah);
    hashes_match && subtree_has_content(b, ctx.before_meta)
}

/// The leaf-pair arm of `validate_fresh_matches` - see its doc comment.
pub(crate) fn leaf_match_supported(
    b: usize,
    a: usize,
    b_info: &ASTNodeMetadata,
    a_info: &ASTNodeMetadata,
    ctx: &SlotCtx,
    before_decision: &HashMap<usize, BeforeDecision>,
) -> bool {
    // Generic tokens (and the hand-picked cross-kind operator swaps): two-sided small context.
    if !nodes::matching_allowed(
        &b_info.kind,
        &a_info.kind,
        &ctx.before_meta.language,
        || update_context_supported(b, a, ctx, before_decision),
    ) {
        return false;
    }
    if nodes::is_generic_token_kind(&b_info.kind) || b_info.kind != a_info.kind {
        return true;
    }
    if b_info.text == a_info.text {
        // Identical-text leaf: the same spelling is not the same token when everything around it
        // is unmatched (`response` in a deleted `if` condition vs `response` in a brand-new call
        // is a coincidence of naming - python-api-change).
        return has_nearby_matched_ancestor(
            b,
            MAX_UPDATE_CONTEXT_ANCESTOR_DEPTH,
            ctx,
            before_decision,
        );
    }
    // Same-kind different-text Update: slot context or visible textual similarity.
    update_context_supported(b, a, ctx, before_decision)
        || nodes::leaf_texts_similar(&b_info.text, &a_info.text)
}

/// Mechanism 1 of `improve_slot_alignment` - see its doc comment. For every `Match(b, a)` whose
/// before-parent is matched but to a node *shallower* than `a`'s parent, retarget `b` onto the
/// same-kind inserted ancestor of `a` sitting directly under that parent's target (and vice versa
/// on the before side), demoting the old partner to inserted/deleted. Guarded by containment: the
/// retarget candidate's other descendants must not be matched outside `b`'s/`a`'s subtree.
pub(crate) fn pull_up_wrapped_matches(
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    diff: &ASTDiff,
    before_parents: &rustc_hash::FxHashMap<usize, usize>,
    after_parents: &rustc_hash::FxHashMap<usize, usize>,
    before_decision: &mut HashMap<usize, BeforeDecision>,
    after_decision: &mut HashMap<usize, AfterDecision>,
) {
    // Ordered by document position, then by preorder index, not node id - see the identical
    // rationale on the sort in `validate_fresh_matches`.
    let mut pairs: Vec<(usize, usize, usize, usize, usize, usize)> = before_decision
        .iter()
        .filter_map(|(&b, d)| match d {
            BeforeDecision::Match(a) => {
                let before_info = before_meta.node_info.get(&b)?;
                let after_info = after_meta.node_info.get(a)?;
                Some((
                    before_info.start_byte,
                    after_info.start_byte,
                    before_info.preorder_index,
                    after_info.preorder_index,
                    b,
                    *a,
                ))
            }
            BeforeDecision::Delete => None,
        })
        .collect();
    pairs.sort_unstable();

    for (_, _, _, _, b, a) in pairs {
        // A previous iteration may have retargeted or demoted this pair.
        if before_decision.get(&b) != Some(&BeforeDecision::Match(a)) {
            continue;
        }

        // After-side pull-up: b's parent is matched, but a sits deeper than that match target's
        // children - i.e. the DP descended into a wrapper. Prefer the wrapper-level node.
        if let Some(&pb) = before_parents.get(&b)
            && let Some(pa_target) = before_match_target(pb, before_decision, diff)
            && after_parents.get(&a) != Some(&pa_target)
            && let Some(c) = ancestor_child_of(a, pa_target, after_parents)
            && c != a
            && after_meta.node_info.get(&c).map(|i| i.kind.as_str())
                == before_meta.node_info.get(&b).map(|i| i.kind.as_str())
            && after_decision.get(&c) == Some(&AfterDecision::Insert)
        {
            let mut targets = Vec::new();
            collect_after_subtree_targets(c, after_meta, after_decision, diff, &mut targets);
            if targets
                .iter()
                .all(|&t| is_ancestor_or_self(b, t, before_parents))
            {
                after_decision.insert(a, AfterDecision::Insert);
                before_decision.insert(b, BeforeDecision::Match(c));
                after_decision.insert(c, AfterDecision::Match(b));
                continue;
            }
        }

        // Before-side pull-up: symmetric - a's parent is matched, but b sits deeper.
        if let Some(&pa) = after_parents.get(&a)
            && let Some(pb_target) = after_match_target(pa, after_decision, diff)
            && before_parents.get(&b) != Some(&pb_target)
            && let Some(c) = ancestor_child_of(b, pb_target, before_parents)
            && c != b
            && before_meta.node_info.get(&c).map(|i| i.kind.as_str())
                == after_meta.node_info.get(&a).map(|i| i.kind.as_str())
            && before_decision.get(&c) == Some(&BeforeDecision::Delete)
        {
            let mut targets = Vec::new();
            collect_before_subtree_targets(c, before_meta, before_decision, diff, &mut targets);
            if targets
                .iter()
                .all(|&t| is_ancestor_or_self(a, t, after_parents))
            {
                before_decision.insert(b, BeforeDecision::Delete);
                before_decision.insert(c, BeforeDecision::Match(a));
                after_decision.insert(a, AfterDecision::Match(c));
            }
        }
    }
}

/// Subtree size beyond which a slot promotion with *zero* internal match evidence additionally
/// requires a shared descendant hash - matching two big, completely disjoint bodies statement-by-
/// statement is the "wholesale replacement" case humans do NOT read as an edit (see the
/// rust-algorithm-change analysis in TODO.md).
pub(crate) const LARGE_SLOT_SUBTREE: usize = 20;

/// Whether promoting deleted `b` / inserted `a` (same kind, corresponding slots) to a match is
/// consistent with everything already decided, and plausible to a human.
// Each parameter is genuinely distinct evidence this decision consults (both node ids, both
// metadata sets, the diff, the parent tables, the decision maps) - a params struct here would
// just relocate the same fields, not reduce them.
#[allow(clippy::too_many_arguments)]
pub(crate) fn slot_promotion_allowed(
    b: usize,
    a: usize,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    diff: &ASTDiff,
    before_parents: &rustc_hash::FxHashMap<usize, usize>,
    after_parents: &rustc_hash::FxHashMap<usize, usize>,
    before_decision: &HashMap<usize, BeforeDecision>,
    after_decision: &HashMap<usize, AfterDecision>,
) -> bool {
    let mut b_targets = Vec::new();
    collect_before_subtree_targets(b, before_meta, before_decision, diff, &mut b_targets);
    if !b_targets
        .iter()
        .all(|&t| is_ancestor_or_self(a, t, after_parents))
    {
        return false;
    }
    let mut a_targets = Vec::new();
    collect_after_subtree_targets(a, after_meta, after_decision, diff, &mut a_targets);
    if !a_targets
        .iter()
        .all(|&t| is_ancestor_or_self(b, t, before_parents))
    {
        return false;
    }

    if b_targets.is_empty() && a_targets.is_empty() {
        let size_b = before_meta
            .node_to_subtree_size
            .get(&b)
            .copied()
            .unwrap_or(1);
        let size_a = after_meta
            .node_to_subtree_size
            .get(&a)
            .copied()
            .unwrap_or(1);
        if size_b > LARGE_SLOT_SUBTREE
            && size_a > LARGE_SLOT_SUBTREE
            && !share_descendant_hash(b, a, before_meta, after_meta)
        {
            return false;
        }
    }
    true
}

/// True if any descendant of `b` (before tree) shares a full hash with any descendant of `a`
/// (after tree) - the cheapest "do these two big bodies have anything at all in common" signal.
pub(crate) fn share_descendant_hash(
    b: usize,
    a: usize,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
) -> bool {
    fn collect_hashes(root: usize, meta: &ASTMetadata, out: &mut std::collections::HashSet<u64>) {
        let Some(info) = meta.node_info.get(&root) else {
            return;
        };
        for &child in &info.children {
            if let Some(&h) = meta.node_to_full_hash.get(&child) {
                out.insert(h);
            }
            collect_hashes(child, meta, out);
        }
    }

    let mut before_hashes = std::collections::HashSet::new();
    collect_hashes(b, before_meta, &mut before_hashes);

    fn any_shared(
        root: usize,
        meta: &ASTMetadata,
        before_hashes: &std::collections::HashSet<u64>,
    ) -> bool {
        let Some(info) = meta.node_info.get(&root) else {
            return false;
        };
        info.children.iter().any(|&child| {
            meta.node_to_full_hash
                .get(&child)
                .is_some_and(|h| before_hashes.contains(h))
                || any_shared(child, meta, before_hashes)
        })
    }
    any_shared(a, after_meta, &before_hashes)
}

/// Weighted LCS over two child index ranges. `weight(i, j)` returns 0 for incompatible positions;
/// the DP maximizes total weight over an order-preserving pairing, and the returned list is the
/// chosen pairs (in order). Child lists are small, so the O(n*m) table is fine.
pub(crate) fn weighted_lcs_pairs(
    n: usize,
    m: usize,
    weight: impl Fn(usize, usize) -> u64,
) -> Vec<(usize, usize)> {
    let mut dp = vec![vec![0u64; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            let mut best = dp[i + 1][j].max(dp[i][j + 1]);
            let w = weight(i, j);
            if w > 0 {
                best = best.max(dp[i + 1][j + 1] + w);
            }
            dp[i][j] = best;
        }
    }
    let mut pairs = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        let w = weight(i, j);
        if w > 0 && dp[i][j] == dp[i + 1][j + 1] + w {
            pairs.push((i, j));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    pairs
}

/// Anchor weight for `promote_same_slot_pairs`' LCS: any already-matched pair must dominate every
/// possible combination of fresh promotions, so promotions can only fill the gaps *between*
/// anchors, never displace one (which would silently unmatch nodes the DP or an earlier pass
/// already paired).
pub(crate) const SLOT_LCS_ANCHOR_WEIGHT: u64 = 10_000;

/// Mechanism 2 of `improve_slot_alignment` - see its doc comment. Walks every matched parent pair
/// (this call's own fresh decisions - see below for what this deliberately excludes) and
/// LCS-aligns their deleted × inserted children by kind, using already-matched child pairs as
/// heavyweight anchors so promotions never cross or displace an existing match. Promoted pairs
/// are enqueued and their own children aligned recursively.
///
/// Deliberately does *not* also seed from a matched node's *parent* (`before_parents.get(&b)`),
/// even when that parent is matched in the shared, global `diff` rather than this call's own
/// `before_decision`/`after_decision` - a prior version did exactly that ("a pre-matched parent
/// whose residual children carry fresh decisions is also a slot-alignment site"), and it's the
/// confirmed root cause of a real correctness bug: `resolve_forest`'s contract is "only touches
/// nodes within the given root ids' own descendant sets," but that parent (and therefore its
/// *other* children, which this call has no ownership of) can sit anywhere in the file - including
/// inside a completely different candidate's own subtree. This is exactly what let two
/// independent, non-ancestor/descendant candidate pairs collide during the 2026-07-25 dependency-
/// aware parallel-batching attempt (`TODO.md`): both workers' calls could each reach the same
/// shared, already-matched ancestor via this path and write conflicting decisions for its
/// children. Measured 2026-08-02: this path never fired once across the entire `optimal_solutions`
/// corpus (157 fixtures) - removing it is zero-risk by direct measurement, not just a hedge.
pub(crate) fn promote_same_slot_pairs(
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    diff: &ASTDiff,
    before_parents: &rustc_hash::FxHashMap<usize, usize>,
    after_parents: &rustc_hash::FxHashMap<usize, usize>,
    before_decision: &mut HashMap<usize, BeforeDecision>,
    after_decision: &mut HashMap<usize, AfterDecision>,
) {
    use std::collections::HashSet;

    let mut queue: Vec<(usize, usize)> = Vec::new();
    for (&b, d) in before_decision.iter() {
        if let BeforeDecision::Match(a) = d {
            queue.push((b, *a));
        }
    }
    // Ordered by document position, then by preorder index, not node id - see the identical
    // rationale on the sort in `validate_fresh_matches`. Position-sorting still groups equal
    // `(b, a)` pairs adjacently (they share both keys), so the following `dedup` is unaffected.
    queue.sort_unstable_by_key(|&(b, a)| {
        let before_info = before_meta.node_info.get(&b);
        let after_info = after_meta.node_info.get(&a);
        (
            before_info.map(|i| i.start_byte).unwrap_or(usize::MAX),
            after_info.map(|i| i.start_byte).unwrap_or(usize::MAX),
            before_info.map(|i| i.preorder_index).unwrap_or(usize::MAX),
            after_info.map(|i| i.preorder_index).unwrap_or(usize::MAX),
        )
    });
    queue.dedup();

    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    while let Some((pb, pa)) = queue.pop() {
        if !seen.insert((pb, pa)) {
            continue;
        }
        let (Some(b_info), Some(a_info)) = (
            before_meta.node_info.get(&pb),
            after_meta.node_info.get(&pa),
        ) else {
            continue;
        };
        let b_children = b_info.children.clone();
        let a_children = a_info.children.clone();

        let promoted = {
            let weight = |i: usize, j: usize| -> u64 {
                let (b, a) = (b_children[i], a_children[j]);
                let b_target = before_match_target(b, before_decision, diff);
                if let Some(t) = b_target {
                    return if t == a { SLOT_LCS_ANCHOR_WEIGHT } else { 0 };
                }
                if after_match_target(a, after_decision, diff).is_some() {
                    return 0;
                }
                // Both unmatched: promotable iff genuinely Delete x Insert (not merely absent
                // from the maps) and the same kind. Leaf pairs additionally need their texts to
                // agree (or nearly): pairing `return` with `return` in the same slot is what this
                // mechanism is for, but pairing identifier `Map` with identifier `Person` just
                // because both are identifiers in the same slot manufactures an Update a human
                // reads as a replacement (kotlin-add-data-class's ground truth).
                let deletable = before_decision.get(&b) == Some(&BeforeDecision::Delete);
                let insertable = after_decision.get(&a) == Some(&AfterDecision::Insert);
                if !deletable || !insertable {
                    return 0;
                }
                let (Some(b_info), Some(a_info)) =
                    (before_meta.node_info.get(&b), after_meta.node_info.get(&a))
                else {
                    return 0;
                };
                if b_info.kind != a_info.kind {
                    return 0;
                }
                if b_info.children.is_empty()
                    && a_info.children.is_empty()
                    && b_info.text != a_info.text
                    && !nodes::leaf_texts_similar(&b_info.text, &a_info.text)
                {
                    return 0;
                }
                1
            };
            weighted_lcs_pairs(b_children.len(), a_children.len(), weight)
        };

        for (i, j) in promoted {
            let (b, a) = (b_children[i], a_children[j]);
            // Skip anchors; only act on fresh Delete x Insert pairs.
            if before_decision.get(&b) != Some(&BeforeDecision::Delete)
                || after_decision.get(&a) != Some(&AfterDecision::Insert)
            {
                continue;
            }
            if !slot_promotion_allowed(
                b,
                a,
                before_meta,
                after_meta,
                diff,
                before_parents,
                after_parents,
                before_decision,
                after_decision,
            ) {
                continue;
            }
            before_decision.insert(b, BeforeDecision::Match(a));
            after_decision.insert(a, AfterDecision::Match(b));
            queue.push((b, a));
        }

        repair_leaf_slots(
            &b_children,
            &a_children,
            before_meta,
            after_meta,
            before_decision,
            after_decision,
        );
    }
}

/// Leaf-level slot repair for one matched parent pair: when a leaf child of the before parent is
/// freshly matched to a token *deeper* than the after parent's own children (a leftover from the
/// DP descending into a wrapper - e.g. before `{` paired with the `{` of a new inner `try` block
/// while the after parent's own `{` sits unmatched), and the after parent has exactly one
/// same-kind, same-text `Insert` leaf child, retarget the leaf onto that slot sibling. Identical
/// text means the retarget swaps one 0-cost pairing for another - strictly cost-neutral. The
/// exactly-one requirement keeps this away from genuinely ambiguous repeated tokens (a run of
/// commas), which are left for the ordered LCS above.
pub(crate) fn repair_leaf_slots(
    b_children: &[usize],
    a_children: &[usize],
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    before_decision: &mut HashMap<usize, BeforeDecision>,
    after_decision: &mut HashMap<usize, AfterDecision>,
) {
    for &x in b_children {
        let Some(x_info) = before_meta.node_info.get(&x) else {
            continue;
        };
        if !x_info.children.is_empty() {
            continue;
        }
        // Only fresh decisions are retargetable, and only ones pointing outside the slot.
        let Some(&BeforeDecision::Match(t)) = before_decision.get(&x) else {
            continue;
        };
        if a_children.contains(&t) {
            continue;
        }
        let mut candidates = a_children.iter().copied().filter(|&y| {
            after_decision.get(&y) == Some(&AfterDecision::Insert)
                && after_meta.node_info.get(&y).is_some_and(|y_info| {
                    y_info.children.is_empty()
                        && y_info.kind == x_info.kind
                        && y_info.text == x_info.text
                })
        });
        let (Some(y), None) = (candidates.next(), candidates.next()) else {
            continue;
        };
        after_decision.insert(t, AfterDecision::Insert);
        before_decision.insert(x, BeforeDecision::Match(y));
        after_decision.insert(y, AfterDecision::Match(x));
    }
}

/// True if `node` is `ancestor` itself or a descendant of it, walking up via `parents`.
pub(crate) fn is_ancestor_or_self(
    ancestor: usize,
    mut node: usize,
    parents: &rustc_hash::FxHashMap<usize, usize>,
) -> bool {
    loop {
        if node == ancestor {
            return true;
        }
        match parents.get(&node) {
            Some(&parent) => node = parent,
            None => return false,
        }
    }
}

/// For every node in `root_ids`' original (unpruned) subtrees, the ids of nodes already present
/// in `node_map` reachable below it - i.e. where the chunks `PostorderIndexer` prunes away
/// actually landed. Computed bottom-up in one pass (as opposed to a fresh subtree walk per query)
/// since `ContainmentCtx` needs this for every node that might appear as a `ren()` candidate, not
/// just one. Does not recurse past an already-mapped node: earlier passes match descendants
/// consistently under the node they fixed, so the immediate boundary is enough to know where a
/// whole pruned chunk landed.
pub(crate) fn compute_pruned_targets(
    root_ids: &[usize],
    meta: &ASTMetadata,
    node_map: &rustc_hash::FxHashMap<usize, usize>,
) -> rustc_hash::FxHashMap<usize, Vec<usize>> {
    fn visit(
        node_id: usize,
        meta: &ASTMetadata,
        node_map: &rustc_hash::FxHashMap<usize, usize>,
        memo: &mut rustc_hash::FxHashMap<usize, Vec<usize>>,
    ) -> Vec<usize> {
        if let Some(cached) = memo.get(&node_id) {
            return cached.clone();
        }
        let result = if let Some(&target) = node_map.get(&node_id) {
            if target == 0 {
                Vec::new()
            } else {
                vec![target]
            }
        } else if let Some(info) = meta.node_info.get(&node_id) {
            info.children
                .iter()
                .flat_map(|&child| visit(child, meta, node_map, memo))
                .collect()
        } else {
            Vec::new()
        };
        memo.insert(node_id, result.clone());
        result
    }

    let mut memo = rustc_hash::FxHashMap::default();
    for &root_id in root_ids {
        visit(root_id, meta, node_map, &mut memo);
    }
    // Only nodes with at least one pruned descendant are useful to callers; drop the rest (most
    // of the tree) to keep the lookup small.
    memo.retain(|_, targets| !targets.is_empty());
    memo
}

/// The `(before_id, after_id)` pairs where a walk down from `root_ids` first hits `node_map` on
/// either side - i.e. exactly the points where `PostorderIndexer::build`'s own `visit()` stops
/// and excludes a subtree. Never recurses past such a node: everything below it is pruned too,
/// the same invariant `compute_pruned_targets` relies on. Walks from *both* sides and unions the
/// results by `before_id` (a pruned chunk should be reachable from both root lists, but walking
/// only one side risks silently missing it if reachability ever differs) so `ContainmentCtx` sees
/// every gap pruning left in this forest.
pub(crate) fn collect_pruned_chunk_pairs(
    before_root_ids: &[usize],
    after_root_ids: &[usize],
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    diff: &ASTDiff,
) -> Vec<(usize, usize)> {
    fn visit(
        node_id: usize,
        meta: &ASTMetadata,
        node_map: &rustc_hash::FxHashMap<usize, usize>,
        out: &mut Vec<usize>,
    ) {
        if node_map.contains_key(&node_id) {
            out.push(node_id);
            return;
        }
        if let Some(info) = meta.node_info.get(&node_id) {
            for &child_id in &info.children {
                visit(child_id, meta, node_map, out);
            }
        }
    }
    let mut pairs: rustc_hash::FxHashMap<usize, usize> = rustc_hash::FxHashMap::default();
    let mut before_roots = Vec::new();
    for &root_id in before_root_ids {
        visit(
            root_id,
            before_meta,
            &diff.before_node_map,
            &mut before_roots,
        );
    }
    for id in before_roots {
        if let Some(&after_id) = diff.before_node_map.get(&id) {
            pairs.insert(id, after_id);
        }
    }
    let mut after_roots = Vec::new();
    for &root_id in after_root_ids {
        visit(root_id, after_meta, &diff.after_node_map, &mut after_roots);
    }
    for id in after_roots {
        if let Some(&before_id) = diff.after_node_map.get(&id) {
            pairs.entry(before_id).or_insert(id);
        }
    }
    pairs.into_iter().collect()
}

/// Longest subsequence of `pairs` (which must already be sorted by `.0`) whose `.1` values are
/// also strictly increasing, found via patience sorting (`O(n log n)`) and reconstructed via
/// parent pointers.
///
/// Why this is needed: not every pruned pair is a safe sibling-order fixed point.
/// `solve_moved_subtrees`/`solve_greedy_anchor_blocks` and similar passes can legitimately match
/// content that was genuinely *moved*, which leaves pruned pairs whose before/after order
/// disagrees with some other pruned pair's. Treating a disagreeing pair as a fixed point would
/// forbid perfectly valid pairings elsewhere for a move that's supposed to be exactly that: out
/// of order. Keeping only the longest mutually order-consistent run - and silently dropping
/// everything that conflicts with it - means a real move never trips the sibling-order check,
/// at the cost of not enforcing that check across the move itself (no worse than before this
/// check existed). Confirmed necessary, not just theoretical: an early version without this
/// filter regressed `c-cpython-autogenerated-code` (33 -> 58 mismatches) via exactly this
/// mechanism - repeated, reordered `case` bodies pruned by `greedy_anchor_block`.
pub(crate) fn longest_increasing_by_second(pairs: &[(usize, usize)]) -> Vec<(usize, usize)> {
    if pairs.is_empty() {
        return Vec::new();
    }
    // `tails[k]` = index into `pairs` of the smallest-tailed increasing run of length `k + 1`
    // found so far; `parent[i]` = index of the element preceding `i` in `i`'s own best run.
    let mut tails: Vec<usize> = Vec::new();
    let mut parent: Vec<Option<usize>> = vec![None; pairs.len()];
    for i in 0..pairs.len() {
        let val = pairs[i].1;
        let pos = tails.partition_point(|&t| pairs[t].1 < val);
        if pos > 0 {
            parent[i] = Some(tails[pos - 1]);
        }
        if pos == tails.len() {
            tails.push(i);
        } else {
            tails[pos] = i;
        }
    }
    let mut result = Vec::with_capacity(tails.len());
    let mut cur = tails.last().copied();
    while let Some(i) = cur {
        result.push(pairs[i]);
        cur = parent[i];
    }
    result.reverse();
    result
}
