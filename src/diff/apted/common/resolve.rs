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
//! The entry points: `resolve_forest`, the oversized-pair fallback, and the context they thread
//! through everything else.

use super::*;

/// Per-`resolve_forest` context that vetoes pairings contradicting mappings earlier passes fixed.
/// Pruning removes those subtrees from the DP's view, so without it nothing stops a "hollowed
/// out" ancestor from pairing, for free, with any same-kind node.
///
/// Containment: if `before_id`'s pruned descendant landed at `t`, `before_id` may pair only with
/// an ancestor-or-self of `t`, and symmetrically.
///
/// Sibling order, for `PREMATCH_SIBLING_ORDER_SOURCES` only: both nodes of a pair must have the
/// same rank among the trusted pruned chunk roots (by `preorder_index`), so a pairing cannot
/// reorder past a fixed point. Ancestors rank correctly because they precede their descendants.
/// Sources that anchor repeated bodies by similarity produce anchors that are not order-preserving,
/// so the check is opt-in.
pub(crate) struct ContainmentCtx<'a> {
    before_pruned_targets: rustc_hash::FxHashMap<usize, Vec<usize>>,
    after_pruned_targets: rustc_hash::FxHashMap<usize, Vec<usize>>,
    before_parents: &'a rustc_hash::FxHashMap<usize, usize>,
    after_parents: &'a rustc_hash::FxHashMap<usize, usize>,
    before_anchor_preorders: Vec<usize>,
    after_anchor_preorders: Vec<usize>,
    before_meta: &'a ASTMetadata,
    after_meta: &'a ASTMetadata,
}

/// `source` tags that pre-match in strict positional order and so get `ContainmentCtx`'s
/// sibling-order check.
pub(crate) const PREMATCH_SIBLING_ORDER_SOURCES: &[&str] = &[
    "qualified_name",
    "large_flat_subtree_container",
    "unique_named_local",
];

impl<'a> ContainmentCtx<'a> {
    pub(crate) fn build(
        before_root_ids: &[usize],
        after_root_ids: &[usize],
        before_meta: &'a ASTMetadata,
        after_meta: &'a ASTMetadata,
        diff: &ASTDiff,
        source: &str,
    ) -> Self {
        let before_pruned_targets =
            compute_pruned_targets(before_root_ids, before_meta, &diff.before_node_map);
        let after_pruned_targets =
            compute_pruned_targets(after_root_ids, after_meta, &diff.after_node_map);
        let (before_anchor_preorders, after_anchor_preorders) =
            if PREMATCH_SIBLING_ORDER_SOURCES.contains(&source) {
                let mut anchor_preorders: Vec<(usize, usize)> = collect_pruned_chunk_pairs(
                    before_root_ids,
                    after_root_ids,
                    before_meta,
                    after_meta,
                    diff,
                )
                .into_iter()
                .filter_map(|(b, a)| {
                    let bp = before_meta.node_info.get(&b)?.preorder_index;
                    let ap = after_meta.node_info.get(&a)?.preorder_index;
                    Some((bp, ap))
                })
                .collect();
                anchor_preorders.sort_unstable();
                // Both projections come out sorted, as `adjust`'s `partition_point` needs.
                let trusted = longest_increasing_by_second(&anchor_preorders);
                (
                    trusted.iter().map(|&(b, _)| b).collect(),
                    trusted.iter().map(|&(_, a)| a).collect(),
                )
            } else {
                (Vec::new(), Vec::new())
            };
        ContainmentCtx {
            before_pruned_targets,
            after_pruned_targets,
            before_parents: &before_meta.node_to_parent,
            after_parents: &after_meta.node_to_parent,
            before_anchor_preorders,
            after_anchor_preorders,
            before_meta,
            after_meta,
        }
    }

    /// `base`, or `FORBIDDEN_RENAME_COST` if pairing `before_id` with `after_id` breaks
    /// containment or sibling order.
    pub(crate) fn adjust(&self, before_id: usize, after_id: usize, base: u64) -> u64 {
        if base >= FORBIDDEN_RENAME_COST {
            return base;
        }
        if let Some(targets) = self.before_pruned_targets.get(&before_id)
            && targets
                .iter()
                .any(|&t| !is_ancestor_or_self(after_id, t, self.after_parents))
        {
            return FORBIDDEN_RENAME_COST;
        }
        if let Some(targets) = self.after_pruned_targets.get(&after_id)
            && targets
                .iter()
                .any(|&t| !is_ancestor_or_self(before_id, t, self.before_parents))
        {
            return FORBIDDEN_RENAME_COST;
        }
        if !self.before_anchor_preorders.is_empty() || !self.after_anchor_preorders.is_empty() {
            let before_pre = self
                .before_meta
                .node_info
                .get(&before_id)
                .map(|info| info.preorder_index);
            let after_pre = self
                .after_meta
                .node_info
                .get(&after_id)
                .map(|info| info.preorder_index);
            if let (Some(bp), Some(ap)) = (before_pre, after_pre) {
                let rank_before = self.before_anchor_preorders.partition_point(|&p| p < bp);
                let rank_after = self.after_anchor_preorders.partition_point(|&p| p < ap);
                if rank_before != rank_after {
                    return FORBIDDEN_RENAME_COST;
                }
            }
        }
        base
    }
}

/// Largest `before.size * after.size` (pruned node counts) a single subtree pair may hand to the
/// APTED kernel; above it, [`resolve_oversized_pair`] decomposes the pair one level. Multi-root
/// forests are bounded by their callers, but a single whole function or class pair was not, and
/// one such kernel call is what dominates slow diffs. The value is the lowest that leaves every
/// corpus fixture's quality unchanged; lowering it trades quality for latency.
pub(crate) const APTED_MAX_CELLS: usize = 600_000;

/// What `resolve_forest` does with a single pair over [`APTED_MAX_CELLS`]: pairs the two roots
/// (or deletes and inserts them if their kinds may not meet), then resolves their children the
/// way a flat container's are. Each leftover pair re-enters `resolve_forest`, so the gate applies
/// again a level down.
pub(crate) fn resolve_oversized_pair(
    before_root: usize,
    after_root: usize,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    cost_model: &UnitCostModel,
    source: &'static str,
    diff: &mut ASTDiff,
) {
    let before_children = before_meta
        .node_info
        .get(&before_root)
        .map(|info| info.children.clone())
        .unwrap_or_default();
    let after_children = after_meta
        .node_info
        .get(&after_root)
        .map(|info| info.children.clone())
        .unwrap_or_default();

    let roots_may_pair = match (
        before_meta.node_info.get(&before_root),
        after_meta.node_info.get(&after_root),
    ) {
        (Some(b), Some(a)) => kinds_update_allowed(&b.kind, &a.kind, &before_meta.language),
        _ => false,
    };
    if roots_may_pair {
        let (operation, cost) =
            classify_match(before_root, after_root, before_meta, after_meta, cost_model);
        diff.add_mapping(
            before_root,
            after_root,
            ASTMapping {
                cost,
                operation,
                reason: ASTMappingReason::APTED(source),
            },
        );
    } else {
        // Root only; the children get their own decisions below.
        diff.add_mapping(
            before_root,
            0,
            ASTMapping::deleted(ASTMappingReason::APTED(source)),
        );
        diff.add_mapping(
            0,
            after_root,
            ASTMapping::inserted(ASTMappingReason::APTED(source)),
        );
    }

    resolve_child_sequence(
        before_children,
        after_children,
        before_meta,
        after_meta,
        LeftoverPool::Oversized,
        source,
        diff,
    );
}

/// Which tree-edit-distance backend `resolve_forest` runs. Both compute optimal distances;
/// `ZhangShasha` is the test oracle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Algorithm {
    #[cfg(test)]
    ZhangShasha,
    Apted,
    /// APTED on the whole pair with every `resolve_forest` shortcut off (identical emit, flat
    /// containers, thin wrappers, the [`APTED_MAX_CELLS`] gate), however much it costs. Not for
    /// the product: `apted_only_worker` uses it to measure tree edit distance itself, which the
    /// shortcuts would otherwise be credited as.
    AptedWholeTree,
}

/// Resolves the mapping for a forest of sibling roots on each side, any of them possibly already
/// partly mapped, and writes it into `diff`. It touches only descendants of the given roots.
// Each parameter is distinct context; a params struct would only relocate the fields.
#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_forest(
    before_root_ids: Vec<usize>,
    after_root_ids: Vec<usize>,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    cost_model: &UnitCostModel,
    algorithm: Algorithm,
    source: &'static str,
    diff: &mut ASTDiff,
) {
    let before_root_ids = filter_mapped_nodes(&before_root_ids, &diff.before_node_map);
    let after_root_ids = filter_mapped_nodes(&after_root_ids, &diff.after_node_map);
    if before_root_ids.is_empty() && after_root_ids.is_empty() {
        return;
    }
    if before_root_ids.is_empty() {
        for id in after_root_ids {
            add_insert_mappings(id, after_meta, source, diff);
        }
        return;
    }
    if after_root_ids.is_empty() {
        for id in before_root_ids {
            add_delete_mappings(id, before_meta, source, diff);
        }
        return;
    }

    // Every shortcut below is the engine's, not tree edit distance's; `AptedWholeTree` takes none.
    let whole_tree = algorithm == Algorithm::AptedWholeTree;
    if !whole_tree && before_root_ids.len() == 1 && after_root_ids.len() == 1 {
        let b = before_root_ids[0];
        let a = after_root_ids[0];
        let hashes_match = before_meta
            .node_to_full_hash
            .get(&b)
            .zip(after_meta.node_to_full_hash.get(&a))
            .is_some_and(|(bh, ah)| bh == ah);
        if hashes_match {
            emit_identical_subtree(b, a, before_meta, after_meta, source, diff);
            return;
        }
        if let (Some(bc), Some(ac)) = (flat_children(b, before_meta), flat_children(a, after_meta))
        {
            resolve_flat_tree_pair(b, a, bc, ac, before_meta, after_meta, source, diff);
            return;
        }
        // A thin wrapper around a flat container (`struct_specifier` around its field list) is
        // decomposed so the container reaches the flat path: ordered TED cannot express members
        // reordering, and mis-pairs moved fields with same-shaped neighbours.
        if let (Some(bf), Some(af)) = (
            sole_flat_child(b, before_meta),
            sole_flat_child(a, after_meta),
        ) && before_meta.node_info.get(&bf).map(|i| &i.kind)
            == after_meta.node_info.get(&af).map(|i| &i.kind)
        {
            resolve_oversized_pair(b, a, before_meta, after_meta, cost_model, source, diff);
            return;
        }
    }

    let before_idx = PostorderIndexer::build(before_meta, &before_root_ids, &diff.before_node_map);
    let after_idx = PostorderIndexer::build(after_meta, &after_root_ids, &diff.after_node_map);

    if !whole_tree
        && before_root_ids.len() == 1
        && after_root_ids.len() == 1
        && before_idx.size * after_idx.size > APTED_MAX_CELLS
    {
        resolve_oversized_pair(
            before_root_ids[0],
            after_root_ids[0],
            before_meta,
            after_meta,
            cost_model,
            source,
            diff,
        );
        return;
    }

    // Used by both the delta computation and the backtrace; they must agree on every cost.
    let containment = ContainmentCtx::build(
        &before_root_ids,
        &after_root_ids,
        before_meta,
        after_meta,
        diff,
        source,
    );

    let mut delta = match algorithm {
        #[cfg(test)]
        Algorithm::ZhangShasha => compute_delta_zhang_shasha(
            &before_idx,
            &after_idx,
            before_meta,
            after_meta,
            cost_model,
            Some(&containment),
        ),
        Algorithm::Apted | Algorithm::AptedWholeTree => compute_delta(
            &before_idx,
            &after_idx,
            before_meta,
            after_meta,
            cost_model,
            &before_root_ids,
            &after_root_ids,
            &diff.before_node_map,
            &diff.after_node_map,
            Some(&containment),
        ),
    };
    let decisions = compute_edit_mapping(
        &before_idx,
        &after_idx,
        before_meta,
        after_meta,
        cost_model,
        Some(&containment),
        &mut delta,
    );
    let mut before_decision: HashMap<usize, BeforeDecision> = HashMap::new();
    let mut after_decision: HashMap<usize, AfterDecision> = HashMap::new();
    for decision in &decisions {
        match *decision {
            RawDecision::Match(b, a) => {
                before_decision.insert(b, BeforeDecision::Match(a));
                after_decision.insert(a, AfterDecision::Match(b));
            }
            RawDecision::Delete(b) => {
                before_decision.insert(b, BeforeDecision::Delete);
            }
            RawDecision::Insert(a) => {
                after_decision.insert(a, AfterDecision::Insert);
            }
        }
    }

    improve_slot_alignment(
        before_meta,
        after_meta,
        diff,
        &before_root_ids,
        &before_meta.node_to_parent,
        &after_meta.node_to_parent,
        &mut before_decision,
        &mut after_decision,
    );

    let mut before_has_match_below = HashMap::new();
    for &id in &before_root_ids {
        compute_has_match_below(
            id,
            before_meta,
            |n| diff.before_node_map.get(&n).is_some_and(|&x| x != 0),
            |n| matches!(before_decision.get(&n), Some(BeforeDecision::Match(_))),
            &mut before_has_match_below,
        );
    }
    let mut after_has_match_below = HashMap::new();
    for &id in &after_root_ids {
        compute_has_match_below(
            id,
            after_meta,
            |n| diff.after_node_map.get(&n).is_some_and(|&x| x != 0),
            |n| matches!(after_decision.get(&n), Some(AfterDecision::Match(_))),
            &mut after_has_match_below,
        );
    }

    let ctx = ResolveCtx {
        before_meta,
        after_meta,
        before_decision,
        after_decision,
        before_has_match_below,
        after_has_match_below,
        source,
    };

    for &id in &before_root_ids {
        emit_before_subtree(id, &ctx, diff);
    }
    for &id in &after_root_ids {
        if matches!(ctx.after_decision.get(&id), Some(AfterDecision::Insert)) {
            emit_after_subtree(id, &ctx, diff);
        }
    }
}

/// Resolves the before and after forests rooted at the given node ids by optimal tree edit
/// distance, writing the mapping into `diff`.
///
/// `source` is a distinct, call-site-specific label (e.g. `"qualified_name"`) recorded on every
/// `ASTMappingReason::APTED` mapping produced, so mappings can be told apart by provenance.
pub fn for_nodes(
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
    before_node_ids: Vec<usize>,
    after_node_ids: Vec<usize>,
    algorithm: Algorithm,
    source: &'static str,
    diff: &mut ASTDiff,
) {
    let cost_model = UnitCostModel::new(before_metadata.language);
    resolve_forest(
        before_node_ids,
        after_node_ids,
        before_metadata,
        after_metadata,
        &cost_model,
        algorithm,
        source,
        diff,
    );
}

/// [`for_nodes`] on the two files' root nodes. A no-op when either side has no AST.
pub fn for_roots(
    before: &Code,
    after: &Code,
    _node_cache: &NodeCache,
    algorithm: Algorithm,
    source: &'static str,
    diff: &mut ASTDiff,
) {
    // `ast: None` is a valid state (no grammar for the language), not a bug.
    if before.ast.is_none() || after.ast.is_none() {
        return;
    }

    let before_metadata = crate::code::metadata::metadata_of(before);
    let after_metadata = crate::code::metadata::metadata_of(after);

    let before_root_id = before.ast.as_ref().unwrap().root_node().id();
    let after_root_id = after.ast.as_ref().unwrap().root_node().id();

    for_nodes(
        &before_metadata,
        &after_metadata,
        vec![before_root_id],
        vec![after_root_id],
        algorithm,
        source,
        diff,
    );
}
