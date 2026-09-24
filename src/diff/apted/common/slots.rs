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

use super::*;

/// Cost of a pairing `ContainmentCtx` vetoes: the same as a forbidden cross-kind pair, above
/// delete+insert, so the DP never takes it.
pub(crate) const FORBIDDEN_RENAME_COST: u64 = COST_DELETE + COST_INSERT + 1;

/// A before node's partner per this call's current decisions, else per an earlier pass's anchor.
/// `None` for deleted and undecided nodes.
pub(crate) fn before_match_target(
    id: usize,
    before_decision: &HashMap<usize, BeforeDecision>,
    diff: &ASTDiff,
) -> Option<usize> {
    match_target(id, before_decision, &diff.before_node_map)
}

pub(crate) fn after_match_target(
    id: usize,
    after_decision: &HashMap<usize, AfterDecision>,
    diff: &ASTDiff,
) -> Option<usize> {
    match_target(id, after_decision, &diff.after_node_map)
}

/// This call's decision if it made one, else `node_map`'s anchor (`0` there means pruned).
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

/// Per-child verdict for `collect_subtree_targets`.
pub(crate) enum SubtreeTargetOutcome {
    /// A fresh `Match(t)`: record `t` and recurse, since the children have their own decisions.
    MatchAndRecurse(usize),
    /// A fresh `Delete`/`Insert`: recurse.
    PruneRecurse,
    /// No fresh decision: record an earlier pass's anchor, if any, and stop. Earlier passes map
    /// whole subtrees consistently, so the boundary target answers any containment question.
    Leaf(Option<usize>),
}

/// Collects the match targets of every matched node strictly below `root`, per `classify`.
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

/// The match targets of every matched node strictly below `root` in the before tree (see
/// `SubtreeTargetOutcome`).
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

pub(crate) fn collect_after_subtree_targets(
    root: usize,
    after_meta: &ASTMetadata,
    after_decision: &HashMap<usize, AfterDecision>,
    diff: &ASTDiff,
    out: &mut Vec<usize>,
) {
    collect_side_subtree_targets(root, after_meta, after_decision, &diff.after_node_map, out);
}

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

/// Post-DP slot alignment: reshapes cost-neutral corners of the DP's decisions the way a human
/// reads them, never making the mapping more expensive.
///
/// 1. `validate_fresh_matches` demotes matches with no contextual support. It runs first so the
///    later containment guards see clean decisions.
/// 2. `pull_up_wrapped_matches`: the DP breaks exact ties between a node's same-slot counterpart
///    and an identical node one wrapper deeper (`try { ... }` around a block) arbitrarily; a
///    human reads the slot pairing as the same node.
/// 3. `promote_same_slot_pairs` matches a deleted and an inserted same-kind child in
///    corresponding slots of a matched parent pair. Never dearer than the delete+insert, so the
///    DP left it only by a tie or a since-removed conflict.
// Each parameter is distinct state; a params struct would only relocate the fields.
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
    // Pull-up demotes the deeper partner, which can strand its descendants' matches without the
    // context that justified them; promotion then re-adds anything slot-consistent.
    validate_fresh_matches(&ctx, before_decision, after_decision);
    // After the pull-up, so a pair it moved is not moved again; before promotion, whose
    // containment guards need the corrected ownership.
    reclaim_slot_level_twins(
        before_meta,
        after_meta,
        diff,
        before_parents,
        after_parents,
        before_decision,
        after_decision,
    );
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

/// True if `root`'s subtree has a leaf that is not a generic punctuation/operator token. An
/// identical pair without one (`()`, brace scaffolding) is structure, not evidence.
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

/// Demotes to delete+insert every fresh `Match` without contextual support: small elements belong
/// to the logical unit around them.
///
/// Parents are processed before children with live state, so a demoted container's children are
/// judged as the islands they have just become, each on its own checks, not demoted wholesale.
///
/// An internal pair whose parents are both unmatched survives only with a matched before-ancestor
/// within `MAX_CONTEXT_ANCESTOR_DEPTH` (a rewritten but corresponding region), or as a
/// byte-identical subtree with real content (`return None` moving between arms is a match; `()`
/// is scaffolding).
///
/// Leaf pairs: generic tokens need two-sided context (`update_context_supported`); same-kind
/// Updates need that context or similar text; identical-text leaves need a matched
/// before-ancestor within `MAX_UPDATE_CONTEXT_ANCESTOR_DEPTH`, one level looser because identical
/// text is stronger evidence.
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

    // Never ordered by node id: ids are not stable across parses of the same source, so the
    // cascade would differ between runs. `start_byte` ties between an ancestor and its leftmost
    // descendant, so `preorder_index` breaks them.
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
        // A forest root's context is the caller's; see `SlotCtx`.
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

/// The internal-pair arm of `validate_fresh_matches`.
pub(crate) fn island_match_supported(
    b: usize,
    a: usize,
    ctx: &SlotCtx,
    before_decision: &HashMap<usize, BeforeDecision>,
    after_decision: &HashMap<usize, AfterDecision>,
) -> bool {
    // Pairing two tree roots needs no context.
    let (Some(&pb), Some(&pa)) = (ctx.before_parents.get(&b), ctx.after_parents.get(&a)) else {
        return true;
    };
    if before_match_target(pb, before_decision, ctx.diff).is_some()
        || after_match_target(pa, after_decision, ctx.diff).is_some()
    {
        return true;
    }
    if has_nearby_matched_ancestor(b, MAX_CONTEXT_ANCESTOR_DEPTH, ctx, before_decision) {
        return true;
    }
    let hashes_match = ctx
        .before_meta
        .node_to_full_hash
        .get(&b)
        .zip(ctx.after_meta.node_to_full_hash.get(&a))
        .is_some_and(|(bh, ah)| bh == ah);
    hashes_match && subtree_has_content(b, ctx.before_meta)
}

/// The leaf-pair arm of `validate_fresh_matches`.
pub(crate) fn leaf_match_supported(
    b: usize,
    a: usize,
    b_info: &ASTNodeMetadata,
    a_info: &ASTNodeMetadata,
    ctx: &SlotCtx,
    before_decision: &HashMap<usize, BeforeDecision>,
) -> bool {
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
        // The same spelling amid unmatched surroundings is a coincidence of naming.
        return has_nearby_matched_ancestor(
            b,
            MAX_UPDATE_CONTEXT_ANCESTOR_DEPTH,
            ctx,
            before_decision,
        );
    }
    update_context_supported(b, a, ctx, before_decision)
        || nodes::leaf_texts_similar(&b_info.text, &a_info.text)
}

/// Step 2 of `improve_slot_alignment`: for `Match(b, a)` where `a` sits deeper than the partner
/// of `b`'s parent, retargets `b` onto `a`'s same-kind inserted ancestor directly under that
/// partner, and symmetrically. The candidate's other descendants must not be matched outside
/// `b`'s subtree.
pub(crate) fn pull_up_wrapped_matches(
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    diff: &ASTDiff,
    before_parents: &rustc_hash::FxHashMap<usize, usize>,
    after_parents: &rustc_hash::FxHashMap<usize, usize>,
    before_decision: &mut HashMap<usize, BeforeDecision>,
    after_decision: &mut HashMap<usize, AfterDecision>,
) {
    // Not by node id; see `validate_fresh_matches`.
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
        if before_decision.get(&b) != Some(&BeforeDecision::Match(a)) {
            continue;
        }

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

/// The unmatched leaf directly under `slot_parent` with `node`'s kind and text, where `node` is a
/// paired delimiter and `slot_parent` also holds its complement; the first in source order wins.
///
/// Paired delimiters only: a `)` belongs to its construct, while a `.` or `,` is a separator
/// whose identity is its position (`a_separator_is_left_where_the_dp_put_it`). The complement
/// test also keeps a comparison `<` out while admitting an HTML tag's `<`.
fn slot_level_twin<D: SideDecision>(
    slot_parent: usize,
    node: &crate::code::ASTNodeMetadata,
    meta: &ASTMetadata,
    decisions: &HashMap<usize, D>,
) -> Option<usize> {
    let complements = nodes::delimiter_complement_kinds(&node.kind)?;
    let parent_info = meta.node_info.get(&slot_parent)?;
    let holds_complement = parent_info.children.iter().any(|child| {
        meta.node_info
            .get(child)
            .is_some_and(|info| complements.contains(&info.kind.as_str()))
    });
    if !holds_complement {
        return None;
    }
    parent_info
        .children
        .iter()
        .filter(|&&child| {
            decisions
                .get(&child)
                .is_some_and(|decision| decision.match_target().is_none())
        })
        .filter_map(|&child| meta.node_info.get(&child).map(|info| (child, info)))
        .filter(|(_, info)| {
            info.children.is_empty() && info.kind == node.kind && info.text == node.text
        })
        .min_by_key(|(_, info)| (info.start_byte, info.preorder_index))
        .map(|(child, _)| child)
}

/// Moves a leaf match onto the identical delimiter sitting in the slot, when the DP matched the
/// one belonging to a removed subtree instead: `f(a, g())` -> `f(a, b)` must keep the outer
/// call's `)`. `pull_up_wrapped_matches` misses this because the right partner is a sibling of
/// the picked node's ancestor, not an ancestor.
///
/// Identical text only, so the swap is free: `ren` is the same for both, and the only thing
/// decided is the one choice the cost function cannot express. Delimiters follow their container
/// even where the container is matched wrongly; a delimiter contradicting its parent would be
/// two defects cancelling.
// Each parameter is distinct state; a params struct would only relocate the fields.
#[allow(clippy::too_many_arguments)]
pub(crate) fn reclaim_slot_level_twins(
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    diff: &ASTDiff,
    before_parents: &rustc_hash::FxHashMap<usize, usize>,
    after_parents: &rustc_hash::FxHashMap<usize, usize>,
    before_decision: &mut HashMap<usize, BeforeDecision>,
    after_decision: &mut HashMap<usize, AfterDecision>,
) {
    // Not by node id; see `validate_fresh_matches`.
    let mut pairs: Vec<(usize, usize, usize, usize, usize, usize)> = before_decision
        .iter()
        .filter_map(|(&b, decision)| match decision {
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
        if before_decision.get(&b) != Some(&BeforeDecision::Match(a)) {
            continue;
        }

        if let Some(&pa) = after_parents.get(&a)
            && let Some(pb_target) = after_match_target(pa, after_decision, diff)
            && before_parents.get(&b) != Some(&pb_target)
            && let Some(before_info) = before_meta.node_info.get(&b)
            && before_info.children.is_empty()
            && let Some(c) = slot_level_twin(pb_target, before_info, before_meta, before_decision)
        {
            before_decision.insert(b, BeforeDecision::Delete);
            before_decision.insert(c, BeforeDecision::Match(a));
            after_decision.insert(a, AfterDecision::Match(c));
            continue;
        }

        if let Some(&pb) = before_parents.get(&b)
            && let Some(pa_target) = before_match_target(pb, before_decision, diff)
            && after_parents.get(&a) != Some(&pa_target)
            && let Some(after_info) = after_meta.node_info.get(&a)
            && after_info.children.is_empty()
            && let Some(c) = slot_level_twin(pa_target, after_info, after_meta, after_decision)
        {
            after_decision.insert(a, AfterDecision::Insert);
            after_decision.insert(c, AfterDecision::Match(b));
            before_decision.insert(b, BeforeDecision::Match(c));
        }
    }
}

/// Subtree size above which a slot promotion with no internal matches also needs a shared
/// descendant hash: two big disjoint bodies are a replacement, not an edit.
pub(crate) const LARGE_SLOT_SUBTREE: usize = 20;

/// Whether promoting deleted `b` / inserted `a` (same kind, corresponding slots) to a match is
/// consistent with everything already decided, and plausible to a human.
// Each parameter is distinct evidence; a params struct would only relocate the fields.
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

/// True if any descendant of `b` shares a full hash with any descendant of `a`.
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

/// The order-preserving pairing of `0..n` with `0..m` that maximizes total `weight`; weight 0
/// means incompatible.
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

/// Anchor weight in `promote_same_slot_pairs`' LCS: larger than any set of promotions, so
/// promotions fill gaps between existing matches and never displace one.
pub(crate) const SLOT_LCS_ANCHOR_WEIGHT: u64 = 10_000;

/// Step 3 of `improve_slot_alignment`: for every pair this call matched, LCS-aligns the deleted
/// and inserted children by kind, existing matches as anchors, and recurses into promoted pairs.
///
/// It seeds only from this call's own matches, never from a parent matched by an earlier pass:
/// that parent's other children are outside this call's forest, and `resolve_forest` must touch
/// only its own roots' descendants.
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
    // Not by node id; see `validate_fresh_matches`.
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
                // Leaves also need similar text: `Map` -> `Person` in one slot is a replacement,
                // not an Update.
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

/// Retargets a before leaf child matched outside the after parent's children (a `{` paired with
/// a new inner block's `{`) onto the after parent's one inserted leaf with the same kind and
/// text. Cost-neutral; with several candidates (a run of commas) it does nothing.
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

/// True if `node` is `ancestor` or a descendant of it.
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

/// For every node under `root_ids` with an already-mapped descendant, the partners of the
/// topmost mapped nodes below it: where the chunks `PostorderIndexer` prunes landed. Pruned-to-0
/// chunks contribute nothing.
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
    memo.retain(|_, targets| !targets.is_empty());
    memo
}

/// The `(before_id, after_id)` pairs at which `PostorderIndexer` pruning stops on either side,
/// walked from both root lists and unioned by `before_id`, so no pruned chunk is missed.
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

/// Longest subsequence of `pairs` (sorted by `.0`) whose `.1` is also strictly increasing.
///
/// Pruned pairs from passes that match moved content disagree in order with the rest; trusting
/// one as a sibling-order fixed point forbids valid pairings around a legitimate move. Keeping
/// only the longest consistent run leaves moves unchecked instead.
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

#[cfg(test)]
mod reclaim_tests {
    use crate::code::{Code, Language};

    /// What the diff maps each before leaf with text `text` to, in source order (`None` for a
    /// delete).
    fn leaf_targets(
        before_src: &str,
        after_src: &str,
        language: &Language,
        text: &str,
    ) -> Vec<Option<usize>> {
        let before = Code::from_string(before_src, language);
        let after = Code::from_string(after_src, language);
        let diff = crate::diff::diff_code(&before, &after);
        let ast = diff.ast.as_ref().expect("an AST diff");
        let root = before
            .ast
            .as_ref()
            .expect("a parsed before tree")
            .root_node();

        let mut leaves = Vec::new();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if node.child_count() == 0 && &before_src[node.byte_range()] == text {
                leaves.push(node);
            }
            for index in 0..node.child_count() {
                stack.push(node.child(index).expect("child in range"));
            }
        }
        leaves.sort_by_key(|node| node.start_byte());
        leaves
            .into_iter()
            .map(|node| {
                ast.before_node_map
                    .get(&node.id())
                    .copied()
                    .filter(|&target| target != 0)
            })
            .collect()
    }

    #[test]
    fn a_surviving_call_keeps_its_own_closing_paren_when_a_nested_call_is_removed() {
        // Matching either `)` costs the same, so only the reclaim rule keeps the outer call's.
        let targets = leaf_targets(
            "fn m() {\n    self.f(&mut w, common.prim_rect.size());\n}\n",
            "fn m() {\n    self.f(&mut w, common.prim_size);\n}\n",
            &Language::Rust,
            ")",
        );

        // `m()`'s own `)`, then the removed `.size()`'s, then the surviving call's.
        assert_eq!(targets.len(), 3, "expected three `)` in the before tree");
        assert!(targets[0].is_some(), "the signature's `)` is untouched");
        assert_eq!(
            targets[1], None,
            "the `)` of the removed `.size()` call must go with it"
        );
        assert!(
            targets[2].is_some(),
            "the surviving call's own `)` must keep the pairing"
        );
    }

    #[test]
    fn a_separator_is_left_where_the_dp_put_it() {
        // A `.` has no construct to close, so which survives is positional and
        // `slot_level_twin` must decline.
        let targets = leaf_targets(
            "class C {\n    int x = Build.VERSION_CODES.R;\n}\n",
            "class C {\n    int x = AndroidVersions.API_30;\n}\n",
            &Language::Java,
            ".",
        );

        assert_eq!(targets.len(), 2, "expected two `.` in the before tree");
        assert!(
            targets[0].is_some() && targets[1].is_none(),
            "the first `.` should keep the pairing and the second should go, got {targets:?}"
        );
    }
}
