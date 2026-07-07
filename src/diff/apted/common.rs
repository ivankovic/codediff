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

use anyhow::Result;
use std::collections::HashMap;

use crate::code::{ASTMetadata, ASTNodeMetadata, Code, Language};
use crate::diff::nodes::{self, kinds_update_allowed};
use crate::diff::{
    ASTDiff, ASTMapping, ASTMappingOperation, ASTMappingReason, COST_DELETE, COST_INSERT,
    COST_UPDATE, NodeCache,
};

use super::engine::compute_delta;
use super::zhang_shasha::compute_delta_zhang_shasha;

/// Cost model for APTED - unit cost model
pub(crate) struct UnitCostModel {
    /// The language both sides of the diff are parsed as, consulted by `ren` to allow a small,
    /// hand-picked set of cross-kind operator swaps (see `kinds_update_allowed`) that would
    /// otherwise always be forbidden.
    pub(crate) language: Language,
}

impl UnitCostModel {
    pub(crate) fn del(&self, _node: &ASTNodeMetadata) -> u64 {
        COST_DELETE
    }

    pub(crate) fn ins(&self, _node: &ASTNodeMetadata) -> u64 {
        COST_INSERT
    }

    pub(crate) fn ren(&self, node1: &ASTNodeMetadata, node2: &ASTNodeMetadata) -> u64 {
        if node1.kind == node2.kind {
            if node1.children.is_empty() && node2.children.is_empty() {
                // Both are leaves
                if node1.text == node2.text {
                    0 // Identical
                } else {
                    COST_UPDATE
                }
            } else {
                // Same kind, internal nodes - can be matched with 0 cost (children cost is
                // accounted for separately via `delta`/recursion).
                0
            }
        } else if kinds_update_allowed(&node1.kind, &node2.kind, &self.language) {
            // A hand-picked exception (e.g. `<` -> `<=`): different kinds, but the same
            // conceptual operator slot. These are always leaves with differing text, so this is
            // exactly the same-kind/different-text case above.
            COST_UPDATE
        } else {
            // Different kinds - matching is more expensive than delete + insert
            // to ensure that nodes with different kinds are not matched.
            COST_DELETE + COST_INSERT + 1 // Make it strictly more expensive
        }
    }
}

/// A pruned, postorder-indexed view of one side of a forest comparison.
///
/// "Pruned" means: any node already present in the relevant side of `diff`'s
/// `before_node_map`/`after_node_map` is excluded, along with its entire subtree - it has
/// already been fully resolved by an earlier pass (or an earlier step of this same recursion)
/// and must not be touched again.
///
/// Indices follow the same convention as the Java APTED reference: postorder ids and the
/// "boundary" variables used by `forest_dist`/`compute_edit_mapping` represent prefix lengths
/// (0..=size), while the underlying arrays are plain 0-based `Vec`s. This is deliberately kept
/// close to the Java reference to reduce the risk of off-by-one translation bugs.
pub(crate) struct PostorderIndexer {
    /// Number of nodes in the pruned forest.
    pub(crate) size: usize,
    /// 0-based postorder index -> node id.
    pub(crate) post_to_node_id: Vec<usize>,
    /// 0-based postorder index -> 0-based preorder index.
    pub(crate) post_to_pre: Vec<usize>,
    /// 0-based preorder index -> 0-based postorder index.
    pub(crate) pre_to_post: Vec<usize>,
    /// 0-based postorder index -> 0-based postorder index of the leftmost leaf descendant.
    pub(crate) post_to_lld: Vec<usize>,
    /// 0-based preorder indices of the keyroots: the forest's own roots, plus every node that
    /// has a left sibling. Drives the bottom-up `delta` computation.
    pub(crate) keyroots: Vec<usize>,
}

impl PostorderIndexer {
    pub(crate) fn build(
        metadata: &ASTMetadata,
        root_ids: &[usize],
        node_map: &HashMap<usize, usize>,
    ) -> Self {
        fn visit(
            node_id: usize,
            metadata: &ASTMetadata,
            node_map: &HashMap<usize, usize>,
            pre_to_node_id: &mut Vec<usize>,
            node_id_to_pre: &mut HashMap<usize, usize>,
        ) {
            if node_map.contains_key(&node_id) {
                return;
            }
            let Some(info) = metadata.node_info.get(&node_id) else {
                return;
            };
            let my_pre = pre_to_node_id.len();
            pre_to_node_id.push(node_id);
            node_id_to_pre.insert(node_id, my_pre);
            for &child_id in &info.children {
                visit(child_id, metadata, node_map, pre_to_node_id, node_id_to_pre);
            }
        }

        let mut pre_to_node_id: Vec<usize> = Vec::new();
        let mut node_id_to_pre: HashMap<usize, usize> = HashMap::new();
        let mut root_pres: Vec<usize> = Vec::new();

        for &root_id in root_ids {
            let before_len = pre_to_node_id.len();
            visit(
                root_id,
                metadata,
                node_map,
                &mut pre_to_node_id,
                &mut node_id_to_pre,
            );
            if pre_to_node_id.len() > before_len {
                root_pres.push(before_len);
            }
        }

        let size = pre_to_node_id.len();

        // Pruned, left-to-right children lists, indexed by preorder.
        let mut pre_children: Vec<Vec<usize>> = vec![Vec::new(); size];
        for pre in 0..size {
            let node_id = pre_to_node_id[pre];
            if let Some(info) = metadata.node_info.get(&node_id) {
                pre_children[pre] = info
                    .children
                    .iter()
                    .filter_map(|c| node_id_to_pre.get(c).copied())
                    .collect();
            }
        }

        // A node is a keyroot iff it's one of the forest's own roots, or it has a left sibling.
        let mut has_left_sibling = vec![false; size];
        for children in &pre_children {
            for (idx, &child_pre) in children.iter().enumerate() {
                if idx > 0 {
                    has_left_sibling[child_pre] = true;
                }
            }
        }
        let mut keyroots: Vec<usize> = root_pres.clone();
        for (pre, &has_sibling) in has_left_sibling.iter().enumerate() {
            if has_sibling {
                keyroots.push(pre);
            }
        }

        // Postorder traversal + leftmost-leaf-descendant, computed together: children are
        // finalized (and their `post_to_lld` written) strictly before their parent.
        let mut post_to_pre: Vec<usize> = Vec::with_capacity(size);
        let mut pre_to_post: Vec<usize> = vec![usize::MAX; size];
        let mut post_to_lld: Vec<usize> = Vec::with_capacity(size);

        for &root_pre in &root_pres {
            let mut stack: Vec<(usize, bool)> = vec![(root_pre, false)];
            while let Some((pre, visited)) = stack.pop() {
                if visited {
                    let post = post_to_pre.len();
                    post_to_pre.push(pre);
                    pre_to_post[pre] = post;
                    let lld = match pre_children[pre].first() {
                        None => post,
                        Some(&first_child_pre) => post_to_lld[pre_to_post[first_child_pre]],
                    };
                    post_to_lld.push(lld);
                } else {
                    stack.push((pre, true));
                    for &child_pre in pre_children[pre].iter().rev() {
                        stack.push((child_pre, false));
                    }
                }
            }
        }

        let post_to_node_id: Vec<usize> =
            post_to_pre.iter().map(|&pre| pre_to_node_id[pre]).collect();

        PostorderIndexer {
            size,
            post_to_node_id,
            post_to_pre,
            pre_to_post,
            post_to_lld,
            keyroots,
        }
    }

    /// Node id at a given 1-based "boundary" position (boundary `b` corresponds to the node at
    /// 0-based postorder index `b - 1`).
    pub(crate) fn node_id_at(&self, boundary: usize) -> usize {
        self.post_to_node_id[boundary - 1]
    }
}

/// Dense, flat-backed `forestdist[(row, col)]` buffer. A flat `Vec<u64>` (rather than
/// `Vec<Vec<u64>>`) avoids one heap allocation and pointer indirection per row, which matters
/// since this buffer is the innermost hot loop of the whole algorithm.
pub(crate) struct ForestDist {
    pub(crate) cols: usize,
    pub(crate) data: Vec<u64>,
}

impl ForestDist {
    pub(crate) fn new(rows: usize, cols: usize) -> Self {
        ForestDist {
            cols,
            data: vec![0u64; rows * cols],
        }
    }
}

impl std::ops::Index<(usize, usize)> for ForestDist {
    type Output = u64;
    fn index(&self, (row, col): (usize, usize)) -> &u64 {
        &self.data[row * self.cols + col]
    }
}

impl std::ops::IndexMut<(usize, usize)> for ForestDist {
    fn index_mut(&mut self, (row, col): (usize, usize)) -> &mut u64 {
        &mut self.data[row * self.cols + col]
    }
}

/// Dense table of `delta[(pre_before, pre_after)]` values, indexed directly by the pruned
/// trees' own 0-based preorder indices. A plain `Vec` (rather than a `HashMap<(usize,usize),_>`)
/// avoids hashing entirely in what is the hottest inner loop of the whole algorithm.
pub(crate) struct DeltaTable {
    pub(crate) cols: usize,
    pub(crate) data: Vec<u64>,
}

impl DeltaTable {
    const UNSET: u64 = u64::MAX;

    pub(crate) fn new(rows: usize, cols: usize) -> Self {
        DeltaTable {
            cols,
            data: vec![Self::UNSET; rows * cols],
        }
    }

    pub(crate) fn get(&self, pre_before: usize, pre_after: usize) -> u64 {
        let v = self.data[pre_before * self.cols + pre_after];
        if v == Self::UNSET { 0 } else { v }
    }

    pub(crate) fn set(&mut self, pre_before: usize, pre_after: usize, value: u64) {
        self.data[pre_before * self.cols + pre_after] = value;
    }
}

/// Generic forest-distance recurrence: a direct port of APTED.java's `forestDist`.
///
/// Fills `forestdist[(di, dj)]` for every `lld(i) <= di <= i`, `lld(j) <= dj <= j`, where deleting
/// or inserting a *single* node (leaf or internal) always costs exactly one unit, and matching
/// two nodes as tree roots either recurses directly (when their ranges align exactly with the
/// outer `(i, j)` boundary) or looks up the precomputed `delta` value for their subtree pair.
/// This single-node granularity (as opposed to atomic whole-subtree delete/insert) is what
/// allows reused content to be discovered even when it has moved to a different depth.
///
/// Whenever the aligned branch is taken, this also writes `delta[(pre_di, pre_dj)] =
/// forestdist[(di-1, dj-1)]` as a side effect - mirroring Java's `treeEditDist` (the spfL/spfR
/// helper), which is what actually populates `delta` for every aligned position encountered
/// along the way, not just the final corner of whichever outer (i, j) call triggered it. This is
/// essential: a great many of the `(pre_di, pre_dj)` pairs later looked up by the unaligned
/// branch are *not* themselves a (keyroot, keyroot) pair that `compute_delta`'s outer loop would
/// ever call `forest_dist` on directly - they only ever get a value because some larger keyroot
/// pair's own computation happened to pass through them as an aligned interior point.
#[allow(clippy::too_many_arguments)]
pub(crate) fn forest_dist(
    before: &PostorderIndexer,
    after: &PostorderIndexer,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    cost_model: &UnitCostModel,
    containment: Option<&ContainmentCtx>,
    delta: &mut DeltaTable,
    i: usize,
    j: usize,
    forestdist: &mut ForestDist,
    write_delta_on_aligned: bool,
) {
    let lld_i = before.post_to_lld[i - 1];
    let lld_j = after.post_to_lld[j - 1];

    forestdist[(lld_i, lld_j)] = 0;

    // Precompute per-dj node id/metadata/lld/preorder once, outside the di loop - the di loop
    // would otherwise redo the same `node_info` HashMap lookup (and lld/pre array reads) for
    // every dj on every single di iteration, turning what should be O(range) prep work into
    // O(di_range * dj_range) redundant lookups.
    let dj_info: Vec<(usize, &ASTNodeMetadata, usize, usize)> = ((lld_j + 1)..=j)
        .map(|dj| {
            let after_id = after.node_id_at(dj);
            let node2 = after_meta
                .node_info
                .get(&after_id)
                .expect("indexed node must have metadata");
            (after_id, node2, after.post_to_lld[dj - 1], after.post_to_pre[dj - 1])
        })
        .collect();

    for di in (lld_i + 1)..=i {
        let before_id = before.node_id_at(di);
        let node1 = before_meta
            .node_info
            .get(&before_id)
            .expect("indexed node must have metadata");
        forestdist[(di, lld_j)] = forestdist[(di - 1, lld_j)] + cost_model.del(node1);
        let lld_di = before.post_to_lld[di - 1];
        let pre_di = before.post_to_pre[di - 1];

        for (dj, &(after_id, node2, lld_dj, pre_dj)) in ((lld_j + 1)..=j).zip(dj_info.iter()) {
            forestdist[(lld_i, dj)] = forestdist[(lld_i, dj - 1)] + cost_model.ins(node2);

            let mut cost_ren = cost_model.ren(node1, node2);
            if let Some(ctx) = containment {
                cost_ren = ctx.adjust(before_id, after_id, cost_ren);
            }

            if lld_di == lld_i && lld_dj == lld_j {
                forestdist[(di, dj)] = (forestdist[(di - 1, dj)] + cost_model.del(node1))
                    .min(forestdist[(di, dj - 1)] + cost_model.ins(node2))
                    .min(forestdist[(di - 1, dj - 1)] + cost_ren);
                // Java's `forestDist` deliberately never writes `delta` here (the equivalent
                // line is commented out in APTED.java): overwriting it would clobber the sparse,
                // already-correct values spfL/spfR/spfA wrote during the forward pass with this
                // call's local (possibly different) forestdist value at the same cell. Only the
                // Zhang-Shasha oracle's own keyroot-sweep construction (which has no pre-existing
                // delta to protect - it's building delta from scratch) needs this side effect.
                if write_delta_on_aligned {
                    delta.set(pre_di, pre_dj, forestdist[(di - 1, dj - 1)]);
                }
            } else {
                let delta_val = delta.get(pre_di, pre_dj);
                forestdist[(di, dj)] = (forestdist[(di - 1, dj)] + cost_model.del(node1))
                    .min(forestdist[(di, dj - 1)] + cost_model.ins(node2))
                    .min(forestdist[(lld_di, lld_dj)] + delta_val + cost_ren);
            }
        }
    }
}

/// A single, single-node-granularity decision produced by `compute_edit_mapping`.
#[derive(Debug, Clone, Copy)]
pub(crate) enum RawDecision {
    Match(usize, usize),
    Delete(usize),
    Insert(usize),
}

/// Backtracks through `forest_dist` to produce the globally optimal node-level edit mapping - a
/// direct port of APTED.java's `computeEditMapping`. Every node in both pruned forests ends up
/// with exactly one decision.
pub(crate) fn compute_edit_mapping(
    before: &PostorderIndexer,
    after: &PostorderIndexer,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    cost_model: &UnitCostModel,
    containment: Option<&ContainmentCtx>,
    delta: &mut DeltaTable,
) -> Vec<RawDecision> {
    let size1 = before.size;
    let size2 = after.size;
    let mut decisions = Vec::new();

    if size1 == 0 && size2 == 0 {
        return decisions;
    }
    if size1 == 0 {
        for post in 0..size2 {
            decisions.push(RawDecision::Insert(after.post_to_node_id[post]));
        }
        return decisions;
    }
    if size2 == 0 {
        for post in 0..size1 {
            decisions.push(RawDecision::Delete(before.post_to_node_id[post]));
        }
        return decisions;
    }

    let mut forestdist = ForestDist::new(size1 + 1, size2 + 1);
    forest_dist(
        before,
        after,
        before_meta,
        after_meta,
        cost_model,
        containment,
        delta,
        size1,
        size2,
        &mut forestdist,
        false,
    );

    let mut root_node_pair = true;
    let mut tree_pairs: Vec<(usize, usize)> = vec![(size1, size2)];

    while let Some((last_row, last_col)) = tree_pairs.pop() {
        if !root_node_pair {
            forest_dist(
                before,
                after,
                before_meta,
                after_meta,
                cost_model,
                containment,
                delta,
                last_row,
                last_col,
                &mut forestdist,
                false,
            );
        }
        root_node_pair = false;

        let first_row = before.post_to_lld[last_row - 1];
        let first_col = after.post_to_lld[last_col - 1];
        let mut row = last_row;
        let mut col = last_col;

        while row > first_row || col > first_col {
            let before_node = row > first_row
                && before_meta
                    .node_info
                    .get(&before.node_id_at(row))
                    .is_some_and(|n| {
                        forestdist[(row - 1, col)] + cost_model.del(n) == forestdist[(row, col)]
                    });
            if before_node {
                decisions.push(RawDecision::Delete(before.node_id_at(row)));
                row -= 1;
                continue;
            }

            let after_node = col > first_col
                && after_meta
                    .node_info
                    .get(&after.node_id_at(col))
                    .is_some_and(|n| {
                        forestdist[(row, col - 1)] + cost_model.ins(n) == forestdist[(row, col)]
                    });
            if after_node {
                decisions.push(RawDecision::Insert(after.node_id_at(col)));
                col -= 1;
                continue;
            }

            let lld_row = before.post_to_lld[row - 1];
            let lld_col = after.post_to_lld[col - 1];
            if lld_row == first_row && lld_col == first_col {
                decisions.push(RawDecision::Match(
                    before.node_id_at(row),
                    after.node_id_at(col),
                ));
                row -= 1;
                col -= 1;
            } else {
                tree_pairs.push((row, col));
                row = lld_row;
                col = lld_col;
            }
        }
    }

    decisions
}

/// What ultimately happens to a before-tree node, per the raw decision list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BeforeDecision {
    Match(usize),
    Delete,
}

/// What ultimately happens to an after-tree node, per the raw decision list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AfterDecision {
    Match(usize),
    Insert,
}

pub(crate) struct ResolveCtx<'a> {
    pub(crate) before_meta: &'a ASTMetadata,
    pub(crate) after_meta: &'a ASTMetadata,
    pub(crate) before_decision: HashMap<usize, BeforeDecision>,
    pub(crate) after_decision: HashMap<usize, AfterDecision>,
    pub(crate) before_has_match_below: HashMap<usize, bool>,
    pub(crate) after_has_match_below: HashMap<usize, bool>,
}

pub(crate) fn compute_has_match_below(
    node_id: usize,
    meta: &ASTMetadata,
    is_pre_matched: impl Fn(usize) -> bool + Copy,
    is_fresh_match: impl Fn(usize) -> bool + Copy,
    memo: &mut HashMap<usize, bool>,
) -> bool {
    if let Some(&cached) = memo.get(&node_id) {
        return cached;
    }
    // A pre-existing match (e.g. from pre_match_identical_subtrees, which runs before the
    // indexer is even built and so never shows up in the decision list at all) is fully
    // resolved already - safe to stop here, nothing below it is ever independently visited.
    // A *fresh* Match decision from this call's compute_edit_mapping is different: its
    // children still get their own independent Match/Delete/Insert decisions, so we must keep
    // recursing into them regardless of whether `node_id` itself matched.
    if is_pre_matched(node_id) {
        memo.insert(node_id, true);
        return true;
    }
    let mut result = is_fresh_match(node_id);
    if let Some(info) = meta.node_info.get(&node_id) {
        for &child_id in &info.children {
            if compute_has_match_below(child_id, meta, is_pre_matched, is_fresh_match, memo) {
                result = true;
            }
        }
    }
    memo.insert(node_id, result);
    result
}

/// Classifies a matched (before_id, after_id) pair into the right `ASTMappingOperation` plus the
/// cost of relabeling just the root pair (children are accounted for separately by the caller).
pub(crate) fn classify_match(
    before_id: usize,
    after_id: usize,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    cost_model: &UnitCostModel,
) -> (ASTMappingOperation, u64) {
    let before_info = before_meta.node_info.get(&before_id).unwrap();
    let after_info = after_meta.node_info.get(&after_id).unwrap();

    if before_info.kind != after_info.kind {
        if kinds_update_allowed(&before_info.kind, &after_info.kind, &before_meta.language) {
            // A hand-picked cross-kind exception (e.g. `<` -> `<=`) that `ren` priced the same as
            // a same-kind, different-text leaf update. Matches the `HumanOperation` convention:
            // matched pairs with different kinds are always `MatchButNotIdentical`, never
            // `Update` (which is reserved for same-kind pairs).
            return (ASTMappingOperation::MatchButNotIdentical, COST_UPDATE);
        }
        // Should not happen in practice: UnitCostModel::ren makes this strictly more expensive
        // than a separate delete + insert, so compute_edit_mapping should never choose it.
        return (
            ASTMappingOperation::Update,
            cost_model.ren(before_info, after_info),
        );
    }

    if before_info.children.is_empty() && after_info.children.is_empty() {
        if before_info.text == after_info.text {
            return (ASTMappingOperation::Identical, 0);
        }
        return (ASTMappingOperation::Update, COST_UPDATE);
    }

    let hashes_match = before_meta
        .node_to_full_hash
        .get(&before_id)
        .zip(after_meta.node_to_full_hash.get(&after_id))
        .map(|(b, a)| b == a)
        .unwrap_or(false);

    if hashes_match {
        (ASTMappingOperation::Identical, 0)
    } else {
        (ASTMappingOperation::MatchButNotIdentical, 0)
    }
}

/// True if `before_id`'s immediate parent is itself matched to `after_id`'s immediate parent -
/// either because an earlier, coarser pass already anchored that correspondence (checked via
/// `diff.before_node_map`, populated before this DP call even started), or because this same DP
/// call itself decided to match them (checked via `ctx.before_decision`, which - unlike
/// `diff.mapping` - is fully populated before any emission happens, so this is safe to call
/// regardless of emission order between a node and its parent).
/// How many ancestor levels to climb from a candidate generic-token leaf while looking for a
/// nearby already-decided match. Bounded, rather than walking all the way to the tree root, so
/// this stays a "small context" check: the enclosing *function* being matched is not evidence
/// that a stray `;` inside it corresponds to a specific other `;` - but climbing more than one
/// level does need to be allowed, to tolerate a single newly-inserted (or removed) wrapper node
/// sitting directly between the leaf and its real small context (e.g. wrapping an existing
/// `identifier` in a new `reference_declarator`, or an existing `;` in a new `type_definition`).
const MAX_CONTEXT_ANCESTOR_DEPTH: usize = 2;

/// Climb bound for the before side of `update_context_supported`, and (via the shared
/// `UPDATE_CONTEXT_DEPTH_BUDGET`) implicitly for the after side too.
const MAX_UPDATE_CONTEXT_ANCESTOR_DEPTH: usize = 3;

/// Combined both-sides depth budget for `update_context_supported`: the before-side climb to the
/// nearest matched ancestor plus the after-side climb from the Update's target up to that
/// ancestor's match target must not exceed this. The asymmetric budget (rather than a fixed
/// per-side bound) is what separates a legitimate deep-nested rename from skeleton reuse: a loop
/// variable 3 expression levels deep in `before` whose `after` counterpart sits *directly under*
/// the matched `for_expression` spends 3+1 and passes, while a reused identifier that is also
/// deep on the after side (buried in a brand-new call chain) spends 3+3 and fails.
const UPDATE_CONTEXT_DEPTH_BUDGET: usize = 4;

/// Read-only context for the decision-level match validation in `improve_slot_alignment` -
/// everything the small-context checks below need to answer "is this pair anchored to matched
/// surroundings", bundled so they don't each take six parameters.
struct SlotCtx<'a> {
    before_meta: &'a ASTMetadata,
    after_meta: &'a ASTMetadata,
    diff: &'a ASTDiff,
    before_parents: &'a HashMap<usize, usize>,
    after_parents: &'a HashMap<usize, usize>,
    /// The before-side roots of the forest this `resolve_forest` call was invoked on. A pair
    /// whose before node is one of these has its context *outside* the forest - the caller
    /// (e.g. the name-keyed pass recursing into an anchored container's children) already vouched
    /// for the surrounding correspondence, but hasn't written its own anchor mapping into `diff`
    /// yet, so parent lookups see nothing. Validation must treat these like tree roots.
    before_forest_roots: &'a std::collections::HashSet<usize>,
}

/// True if `before_id` has an ancestor, within `max_depth` levels, that's already
/// decided as a `Match` (either a pre-existing anchor from an earlier, coarser pass - checked via
/// `diff.before_node_map`, populated before this DP call even started - or a fresh decision from
/// this same DP call).
///
/// Deliberately one-sided: it does not separately check that the matched ancestor's target
/// actually contains `after_id`. It doesn't need to - `before_decision`/`after_decision` are one
/// coherent, already-validated ordered tree mapping (that's what the DP guarantees), so if some
/// ancestor `v` of `before_id` matches target `t`, and `before_id` (a descendant of `v`) matches
/// `after_id`, ancestor-order preservation *guarantees* `after_id` is a descendant of `t`. Finding
/// the ancestor match at all is the only real question.
fn has_nearby_matched_ancestor(
    before_id: usize,
    max_depth: usize,
    ctx: &SlotCtx,
    before_decision: &HashMap<usize, BeforeDecision>,
) -> bool {
    let mut node = before_id;
    for _ in 0..max_depth {
        let Some(&parent) = ctx.before_parents.get(&node) else {
            return false;
        };
        if before_match_target(parent, before_decision, ctx.diff).is_some() {
            return true;
        }
        node = parent;
    }
    false
}

/// Two-sided small-context check for the leaf-pair validation in `validate_fresh_matches`: climbs
/// the before side (up to `MAX_UPDATE_CONTEXT_ANCESTOR_DEPTH`) to the *nearest* matched ancestor,
/// then requires the after side to reach that ancestor's match target within the remaining
/// `UPDATE_CONTEXT_DEPTH_BUDGET`. Only the nearest matched before-ancestor is tried: any higher
/// matched ancestor's target is a strict ancestor of this one's, so the after-side climb to it
/// would only be longer - if the nearest one is over budget, they all are.
///
/// The symmetry is the point: a `(` two levels under a matched function body passes a one-sided
/// check no matter where its partner sits - even inside a brand-new call expression half a
/// function away (cpp-optimize-algorithm's `for (...)` paren pairing with `min_element(`). Budget
/// spent on *both* climbs keeps "nearby" meaning nearby on both sides of the pair.
fn update_context_supported(
    before_id: usize,
    after_id: usize,
    ctx: &SlotCtx,
    before_decision: &HashMap<usize, BeforeDecision>,
) -> bool {
    let mut node = before_id;
    for before_depth in 1..=MAX_UPDATE_CONTEXT_ANCESTOR_DEPTH {
        let Some(&parent) = ctx.before_parents.get(&node) else {
            return false;
        };
        if let Some(target) = before_match_target(parent, before_decision, ctx.diff) {
            let after_budget = UPDATE_CONTEXT_DEPTH_BUDGET.saturating_sub(before_depth);
            let mut after_node = after_id;
            for _ in 0..after_budget {
                let Some(&after_parent) = ctx.after_parents.get(&after_node) else {
                    return false;
                };
                if after_parent == target {
                    return true;
                }
                after_node = after_parent;
            }
            return false;
        }
        node = parent;
    }
    false
}

pub(crate) fn emit_match(
    before_id: usize,
    after_id: usize,
    ctx: &ResolveCtx,
    diff: &mut ASTDiff,
) -> u64 {
    if let Some(mapping) = diff.mapping.get(&(before_id, after_id)) {
        return mapping.cost;
    }

    let (operation, root_cost) = classify_match(
        before_id,
        after_id,
        ctx.before_meta,
        ctx.after_meta,
        &UnitCostModel {
            language: ctx.before_meta.language,
        },
    );
    let mut total = root_cost;

    if let Some(info) = ctx.before_meta.node_info.get(&before_id) {
        for child in filter_before_nodes(info.children.clone(), diff) {
            total += emit_before_subtree(child, ctx, diff);
        }
    }
    if let Some(info) = ctx.after_meta.node_info.get(&after_id) {
        for child in filter_after_nodes(info.children.clone(), diff) {
            if matches!(ctx.after_decision.get(&child), Some(AfterDecision::Insert)) {
                total += emit_after_subtree(child, ctx, diff);
            }
            // A Match after-child was already (or will be) handled via the before-children
            // loop above, walking that child's before-side partner.
        }
    }

    diff.add_mapping(
        before_id,
        after_id,
        ASTMapping {
            cost: total,
            operation,
            reason: ASTMappingReason::APTED,
        },
    );
    total
}

pub(crate) fn emit_before_subtree(before_id: usize, ctx: &ResolveCtx, diff: &mut ASTDiff) -> u64 {
    if let Some(BeforeDecision::Match(after_id)) = ctx.before_decision.get(&before_id) {
        return emit_match(before_id, *after_id, ctx, diff);
    }

    if !ctx
        .before_has_match_below
        .get(&before_id)
        .copied()
        .unwrap_or(false)
    {
        add_delete_mappings(before_id, ctx.before_meta, diff);
        return subtree_del_cost(
            before_id,
            ctx.before_meta,
            &UnitCostModel {
                language: ctx.before_meta.language,
            },
        );
    }

    // Something below this node is reused elsewhere: delete just this node (cost 1) and let
    // its children be independently classified.
    let mut total = COST_DELETE;
    if let Some(info) = ctx.before_meta.node_info.get(&before_id) {
        for child in filter_before_nodes(info.children.clone(), diff) {
            total += emit_before_subtree(child, ctx, diff);
        }
    }
    diff.add_mapping(
        before_id,
        0,
        ASTMapping {
            cost: total,
            operation: ASTMappingOperation::Delete,
            reason: ASTMappingReason::APTED,
        },
    );
    total
}

pub(crate) fn emit_after_subtree(after_id: usize, ctx: &ResolveCtx, diff: &mut ASTDiff) -> u64 {
    if let Some(AfterDecision::Match(before_id)) = ctx.after_decision.get(&after_id) {
        return emit_match(*before_id, after_id, ctx, diff);
    }

    if !ctx
        .after_has_match_below
        .get(&after_id)
        .copied()
        .unwrap_or(false)
    {
        add_insert_mappings(after_id, ctx.after_meta, diff);
        return subtree_ins_cost(
            after_id,
            ctx.after_meta,
            &UnitCostModel {
                language: ctx.after_meta.language,
            },
        );
    }

    let mut total = COST_INSERT;
    if let Some(info) = ctx.after_meta.node_info.get(&after_id) {
        for child in filter_after_nodes(info.children.clone(), diff) {
            total += emit_after_subtree(child, ctx, diff);
        }
    }
    diff.add_mapping(
        0,
        after_id,
        ASTMapping {
            cost: total,
            operation: ASTMappingOperation::Insert,
            reason: ASTMappingReason::APTED,
        },
    );
    total
}

/// Mark a pair of bit-for-bit identical subtrees (and all their descendants) as `Identical`,
/// without running any tree-edit-distance computation. Safe because identical full hashes
/// guarantee identical structure (so children lists line up 1:1, in order).
pub(crate) fn emit_identical_subtree(
    before_id: usize,
    after_id: usize,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    diff: &mut ASTDiff,
) {
    if diff.mapping.contains_key(&(before_id, after_id)) {
        return;
    }
    diff.add_mapping(
        before_id,
        after_id,
        ASTMapping {
            cost: 0,
            operation: ASTMappingOperation::Identical,
            reason: ASTMappingReason::APTED,
        },
    );
    if let (Some(before_info), Some(after_info)) = (
        before_meta.node_info.get(&before_id),
        after_meta.node_info.get(&after_id),
    ) {
        for (&bc, &ac) in before_info.children.iter().zip(after_info.children.iter()) {
            emit_identical_subtree(bc, ac, before_meta, after_meta, diff);
        }
    }
}

/// Add delete mappings for an entire subtree (used when no part of it is reused elsewhere).
pub(crate) fn add_delete_mappings(node_id: usize, meta: &ASTMetadata, diff: &mut ASTDiff) {
    if node_id == 0 {
        return;
    }
    if diff.before_node_map.get(&node_id).is_some_and(|&x| x != 0) {
        return;
    }
    if !diff.mapping.contains_key(&(node_id, 0)) {
        let cost = subtree_del_cost(
            node_id,
            meta,
            &UnitCostModel {
                language: meta.language,
            },
        );
        diff.add_mapping(
            node_id,
            0,
            ASTMapping {
                cost,
                operation: ASTMappingOperation::Delete,
                reason: ASTMappingReason::APTED,
            },
        );
    }
    if let Some(info) = meta.node_info.get(&node_id) {
        for &child_id in &info.children {
            add_delete_mappings(child_id, meta, diff);
        }
    }
}

/// Add insert mappings for an entire subtree (used when no part of it is reused elsewhere).
pub(crate) fn add_insert_mappings(node_id: usize, meta: &ASTMetadata, diff: &mut ASTDiff) {
    if node_id == 0 {
        return;
    }
    if diff.after_node_map.get(&node_id).is_some_and(|&x| x != 0) {
        return;
    }
    if !diff.mapping.contains_key(&(0, node_id)) {
        let cost = subtree_ins_cost(
            node_id,
            meta,
            &UnitCostModel {
                language: meta.language,
            },
        );
        diff.add_mapping(
            0,
            node_id,
            ASTMapping {
                cost,
                operation: ASTMappingOperation::Insert,
                reason: ASTMappingReason::APTED,
            },
        );
    }
    if let Some(info) = meta.node_info.get(&node_id) {
        for &child_id in &info.children {
            add_insert_mappings(child_id, meta, diff);
        }
    }
}

/// Compute the cost of deleting an entire subtree.
pub(crate) fn subtree_del_cost(
    node_id: usize,
    meta: &ASTMetadata,
    cost_model: &UnitCostModel,
) -> u64 {
    if node_id == 0 {
        return 0;
    }
    let Some(info) = meta.node_info.get(&node_id) else {
        return 0;
    };
    let mut cost = cost_model.del(info);
    for &child_id in &info.children {
        cost += subtree_del_cost(child_id, meta, cost_model);
    }
    cost
}

/// Compute the cost of inserting an entire subtree.
pub(crate) fn subtree_ins_cost(
    node_id: usize,
    meta: &ASTMetadata,
    cost_model: &UnitCostModel,
) -> u64 {
    if node_id == 0 {
        return 0;
    }
    let Some(info) = meta.node_info.get(&node_id) else {
        return 0;
    };
    let mut cost = cost_model.ins(info);
    for &child_id in &info.children {
        cost += subtree_ins_cost(child_id, meta, cost_model);
    }
    cost
}

// --- Flat-tree fast path: Myers O(ND) sequence diff ---

/// Minimum number of leaf children required to trigger the flat-tree optimisation.
const FLAT_MIN_CHILDREN: usize = 50;
/// Edit-distance cap for Myers diff. If d exceeds this, we fall back to mark-as-replaced.
const FLAT_MAX_EDIT: usize = 1000;

/// Returns the unmatched direct children of `root_id` if the root has at least
/// `FLAT_MIN_CHILDREN` unmatched children. Children may be leaves or interior nodes;
/// all mapping helpers (`emit_identical_subtree`, `add_delete/insert_mappings`) handle
/// subtrees recursively, so depth-1 is not a requirement.
fn flat_children(
    root_id: usize,
    meta: &ASTMetadata,
    node_map: &HashMap<usize, usize>,
) -> Option<Vec<usize>> {
    let info = meta.node_info.get(&root_id)?;
    let children: Vec<usize> = info
        .children
        .iter()
        .copied()
        .filter(|&id| !node_map.contains_key(&id))
        .collect();
    if children.len() >= FLAT_MIN_CHILDREN {
        Some(children)
    } else {
        None
    }
}

/// Myers O(ND) LCS on two sequences of hashes. Returns matched `(a_idx, b_idx)` pairs
/// in ascending order, or `None` if the edit distance exceeds `max_edit`.
fn myers_lcs(a: &[u64], b: &[u64], max_edit: usize) -> Option<Vec<(usize, usize)>> {
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

fn backtrack_myers(
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

/// Resolve a flat-tree root pair via Myers sequence diff and emit all mappings into `diff`.
fn resolve_flat_tree_pair(
    before_root: usize,
    after_root: usize,
    before_children: Vec<usize>,
    after_children: Vec<usize>,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    diff: &mut ASTDiff,
) {
    let before_hashes: Vec<u64> = before_children
        .iter()
        .map(|&id| before_meta.node_to_full_hash.get(&id).copied().unwrap_or(0))
        .collect();
    let after_hashes: Vec<u64> = after_children
        .iter()
        .map(|&id| after_meta.node_to_full_hash.get(&id).copied().unwrap_or(0))
        .collect();

    match myers_lcs(&before_hashes, &after_hashes, FLAT_MAX_EDIT) {
        Some(pairs) => {
            let mut before_matched = vec![false; before_children.len()];
            let mut after_matched = vec![false; after_children.len()];
            for (bi, ai) in pairs {
                before_matched[bi] = true;
                after_matched[ai] = true;
                // Matched by identical hash.
                emit_identical_subtree(
                    before_children[bi],
                    after_children[ai],
                    before_meta,
                    after_meta,
                    diff,
                );
            }
            for (i, &id) in before_children.iter().enumerate() {
                if !before_matched[i] {
                    add_delete_mappings(id, before_meta, diff);
                }
            }
            for (i, &id) in after_children.iter().enumerate() {
                if !after_matched[i] {
                    add_insert_mappings(id, after_meta, diff);
                }
            }
        }
        None => {
            // Edit distance exceeds FLAT_MAX_EDIT: mark all children replaced.
            for &id in &before_children {
                add_delete_mappings(id, before_meta, diff);
            }
            for &id in &after_children {
                add_insert_mappings(id, after_meta, diff);
            }
        }
    }

    diff.add_mapping(
        before_root,
        after_root,
        ASTMapping {
            cost: 0,
            operation: ASTMappingOperation::MatchButNotIdentical,
            reason: ASTMappingReason::FlatSequenceDiff,
        },
    );
}

/// Filter out before nodes that are already mapped in the diff.
pub(crate) fn filter_before_nodes(node_ids: Vec<usize>, diff: &ASTDiff) -> Vec<usize> {
    node_ids
        .into_iter()
        .filter(|node_id| !diff.before_node_map.contains_key(node_id))
        .collect()
}

/// Filter out after nodes that are already mapped in the diff.
pub(crate) fn filter_after_nodes(node_ids: Vec<usize>, diff: &ASTDiff) -> Vec<usize> {
    node_ids
        .into_iter()
        .filter(|node_id| !diff.after_node_map.contains_key(node_id))
        .collect()
}

/// Cost charged for a `ren()` pairing that `ContainmentCtx` has vetoed - deliberately the same
/// value `UnitCostModel::ren` already uses for *disallowed* mismatched kinds (kinds not covered by
/// `kinds_update_allowed`), so a containment-inconsistent pairing is exactly as unattractive to
/// the DP, never merely "more expensive than the best alternative" (which could still lose to an
/// equally bad alternative pairing). Note this is strictly more than the `COST_UPDATE` charged for
/// an *allowed* cross-kind pairing, so the veto still dominates even over that cheaper option.
const FORBIDDEN_RENAME_COST: u64 = COST_DELETE + COST_INSERT + 1;

/// The match target of a before-tree node per the *current* state of this call's decisions plus
/// anything an earlier, coarser pass already anchored (which the DP never revisits). `None` for
/// deleted and undecided nodes.
fn before_match_target(
    id: usize,
    before_decision: &HashMap<usize, BeforeDecision>,
    diff: &ASTDiff,
) -> Option<usize> {
    match before_decision.get(&id) {
        Some(BeforeDecision::Match(t)) => Some(*t),
        Some(BeforeDecision::Delete) => None,
        None => diff.before_node_map.get(&id).copied().filter(|&t| t != 0),
    }
}

/// After-side counterpart of `before_match_target`.
fn after_match_target(
    id: usize,
    after_decision: &HashMap<usize, AfterDecision>,
    diff: &ASTDiff,
) -> Option<usize> {
    match after_decision.get(&id) {
        Some(AfterDecision::Match(t)) => Some(*t),
        Some(AfterDecision::Insert) => None,
        None => diff.after_node_map.get(&id).copied().filter(|&t| t != 0),
    }
}

/// The ancestor of `node` that is a *direct child* of `ancestor`, or `None` if `ancestor` isn't
/// on `node`'s parent chain. (Returns `node` itself when `node`'s parent is `ancestor`.)
fn ancestor_child_of(
    node: usize,
    ancestor: usize,
    parents: &HashMap<usize, usize>,
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

/// Collects the match targets of every matched node strictly below `root` in the before tree:
/// fresh `Match` decisions (recursing past them, since their children carry their own independent
/// decisions) and pre-existing anchors from earlier passes (not recursing past those - earlier
/// passes map whole subtrees consistently under the node they fixed, so the boundary target is
/// enough for any containment question).
fn collect_before_subtree_targets(
    root: usize,
    before_meta: &ASTMetadata,
    before_decision: &HashMap<usize, BeforeDecision>,
    diff: &ASTDiff,
    out: &mut Vec<usize>,
) {
    let Some(info) = before_meta.node_info.get(&root) else {
        return;
    };
    for &child in &info.children {
        match before_decision.get(&child) {
            Some(BeforeDecision::Match(t)) => {
                out.push(*t);
                collect_before_subtree_targets(child, before_meta, before_decision, diff, out);
            }
            Some(BeforeDecision::Delete) => {
                collect_before_subtree_targets(child, before_meta, before_decision, diff, out);
            }
            None => {
                if let Some(&t) = diff.before_node_map.get(&child)
                    && t != 0
                {
                    out.push(t);
                }
                // Pre-decided (pruned) subtrees are consistently mapped below their boundary;
                // nothing further down can contradict a containment check that the boundary
                // target itself passes.
            }
        }
    }
}

/// After-side counterpart of `collect_before_subtree_targets`.
fn collect_after_subtree_targets(
    root: usize,
    after_meta: &ASTMetadata,
    after_decision: &HashMap<usize, AfterDecision>,
    diff: &ASTDiff,
    out: &mut Vec<usize>,
) {
    let Some(info) = after_meta.node_info.get(&root) else {
        return;
    };
    for &child in &info.children {
        match after_decision.get(&child) {
            Some(AfterDecision::Match(t)) => {
                out.push(*t);
                collect_after_subtree_targets(child, after_meta, after_decision, diff, out);
            }
            Some(AfterDecision::Insert) => {
                collect_after_subtree_targets(child, after_meta, after_decision, diff, out);
            }
            None => {
                if let Some(&t) = diff.after_node_map.get(&child)
                    && t != 0
                {
                    out.push(t);
                }
            }
        }
    }
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
fn improve_slot_alignment(
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    diff: &ASTDiff,
    before_root_ids: &[usize],
    before_parents: &HashMap<usize, usize>,
    after_parents: &HashMap<usize, usize>,
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
fn subtree_has_content(root: usize, meta: &ASTMetadata) -> bool {
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
fn validate_fresh_matches(
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

    let mut pairs: Vec<(usize, usize, usize)> = before_decision
        .iter()
        .filter_map(|(&b, d)| match d {
            BeforeDecision::Match(a) => Some((depth_of(b), b, *a)),
            BeforeDecision::Delete => None,
        })
        .collect();
    pairs.sort_unstable();

    for (_, b, a) in pairs {
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
fn island_match_supported(
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
fn leaf_match_supported(
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
fn pull_up_wrapped_matches(
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    diff: &ASTDiff,
    before_parents: &HashMap<usize, usize>,
    after_parents: &HashMap<usize, usize>,
    before_decision: &mut HashMap<usize, BeforeDecision>,
    after_decision: &mut HashMap<usize, AfterDecision>,
) {
    let mut pairs: Vec<(usize, usize)> = before_decision
        .iter()
        .filter_map(|(&b, d)| match d {
            BeforeDecision::Match(a) => Some((b, *a)),
            BeforeDecision::Delete => None,
        })
        .collect();
    pairs.sort_unstable();

    for (b, a) in pairs {
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
const LARGE_SLOT_SUBTREE: usize = 20;

/// Whether promoting deleted `b` / inserted `a` (same kind, corresponding slots) to a match is
/// consistent with everything already decided, and plausible to a human.
fn slot_promotion_allowed(
    b: usize,
    a: usize,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    diff: &ASTDiff,
    before_parents: &HashMap<usize, usize>,
    after_parents: &HashMap<usize, usize>,
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
        let size_b = before_meta.node_to_subtree_size.get(&b).copied().unwrap_or(1);
        let size_a = after_meta.node_to_subtree_size.get(&a).copied().unwrap_or(1);
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
fn share_descendant_hash(
    b: usize,
    a: usize,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
) -> bool {
    fn collect_hashes(
        root: usize,
        meta: &ASTMetadata,
        out: &mut std::collections::HashSet<u64>,
    ) {
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
fn weighted_lcs_pairs(n: usize, m: usize, weight: impl Fn(usize, usize) -> u64) -> Vec<(usize, usize)> {
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
const SLOT_LCS_ANCHOR_WEIGHT: u64 = 10_000;

/// Mechanism 2 of `improve_slot_alignment` - see its doc comment. Walks every matched parent pair
/// (fresh decisions, plus pre-existing anchors that have residual children in this forest) and
/// LCS-aligns their deleted × inserted children by kind, using already-matched child pairs as
/// heavyweight anchors so promotions never cross or displace an existing match. Promoted pairs
/// are enqueued and their own children aligned recursively.
fn promote_same_slot_pairs(
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    diff: &ASTDiff,
    before_parents: &HashMap<usize, usize>,
    after_parents: &HashMap<usize, usize>,
    before_decision: &mut HashMap<usize, BeforeDecision>,
    after_decision: &mut HashMap<usize, AfterDecision>,
) {
    use std::collections::HashSet;

    let mut queue: Vec<(usize, usize)> = Vec::new();
    for (&b, d) in before_decision.iter() {
        if let BeforeDecision::Match(a) = d {
            queue.push((b, *a));
        }
        // A pre-matched (pruned) parent whose residual children carry fresh decisions is also a
        // slot-alignment site: its deleted/inserted children are this forest's roots.
        if let Some(&pb) = before_parents.get(&b)
            && !before_decision.contains_key(&pb)
            && let Some(&t) = diff.before_node_map.get(&pb)
            && t != 0
        {
            queue.push((pb, t));
        }
    }
    queue.sort_unstable();
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
                let (Some(b_info), Some(a_info)) = (
                    before_meta.node_info.get(&b),
                    after_meta.node_info.get(&a),
                ) else {
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
fn repair_leaf_slots(
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

/// Build a child -> parent map covering every node in `meta`, so ancestor/descendant checks don't
/// need repeated subtree walks. `ASTNodeMetadata` has no parent pointer, so this is derived once
/// from the children lists.
fn build_parent_map(meta: &ASTMetadata) -> HashMap<usize, usize> {
    let mut parents = HashMap::new();
    for (&id, info) in &meta.node_info {
        for &child in &info.children {
            parents.insert(child, id);
        }
    }
    parents
}

/// True if `node` is `ancestor` itself or a descendant of it, walking up via `parents`.
fn is_ancestor_or_self(ancestor: usize, mut node: usize, parents: &HashMap<usize, usize>) -> bool {
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
fn compute_pruned_targets(
    root_ids: &[usize],
    meta: &ASTMetadata,
    node_map: &HashMap<usize, usize>,
) -> HashMap<usize, Vec<usize>> {
    fn visit(
        node_id: usize,
        meta: &ASTMetadata,
        node_map: &HashMap<usize, usize>,
        memo: &mut HashMap<usize, Vec<usize>>,
    ) -> Vec<usize> {
        if let Some(cached) = memo.get(&node_id) {
            return cached.clone();
        }
        let result = if let Some(&target) = node_map.get(&node_id) {
            if target == 0 { Vec::new() } else { vec![target] }
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

    let mut memo = HashMap::new();
    for &root_id in root_ids {
        visit(root_id, meta, node_map, &mut memo);
    }
    // Only nodes with at least one pruned descendant are useful to callers; drop the rest (most
    // of the tree) to keep the lookup small.
    memo.retain(|_, targets| !targets.is_empty());
    memo
}

/// Per-`resolve_forest`-call context letting `ren()` refuse pairings that would contradict a
/// mapping some earlier pass already fixed. `PostorderIndexer` prunes already-matched descendants
/// out of the forest entirely, so without this, nothing stops the DP from matching a "hollowed
/// out" ancestor - one whose real child was pruned away into some unrelated part of the other
/// tree - to any other same-kind node, including ones that break the ancestor-order-preservation
/// every tree-edit-distance mapping is supposed to guarantee. Concretely: if `before_id`'s pruned
/// descendant landed at `t`, then `before_id` may only be matched to an ancestor-or-self of `t`;
/// symmetrically for the after side. (See the `rust-add-if` case this was written for: the
/// `if`/`else` wrapper statement was getting matched deep inside the sibling `if` branch its own
/// child had already been pruned into, for free, since both were internal nodes of the same kind.)
///
/// Built once per `resolve_forest` call and threaded down into `forest_dist` everywhere it's
/// invoked (both the keyroot sweep and the backtrace). Parent maps are only built when the
/// corresponding pruned-targets map is non-empty, so the common case (nothing pruned in this
/// particular forest) costs nothing beyond two empty-map lookups.
pub(crate) struct ContainmentCtx {
    before_pruned_targets: HashMap<usize, Vec<usize>>,
    after_pruned_targets: HashMap<usize, Vec<usize>>,
    before_parents: Option<HashMap<usize, usize>>,
    after_parents: Option<HashMap<usize, usize>>,
}

impl ContainmentCtx {
    fn build(
        before_root_ids: &[usize],
        after_root_ids: &[usize],
        before_meta: &ASTMetadata,
        after_meta: &ASTMetadata,
        diff: &ASTDiff,
    ) -> Self {
        let before_pruned_targets =
            compute_pruned_targets(before_root_ids, before_meta, &diff.before_node_map);
        let after_pruned_targets =
            compute_pruned_targets(after_root_ids, after_meta, &diff.after_node_map);
        let before_parents = (!after_pruned_targets.is_empty()).then(|| build_parent_map(before_meta));
        let after_parents = (!before_pruned_targets.is_empty()).then(|| build_parent_map(after_meta));
        ContainmentCtx {
            before_pruned_targets,
            after_pruned_targets,
            before_parents,
            after_parents,
        }
    }

    /// Adjusts a `ren()`-computed `base` cost: if matching `before_id` to `after_id` would
    /// contradict where an already-pruned descendant of either landed, escalate to
    /// `FORBIDDEN_RENAME_COST` so the DP looks elsewhere. `base` is returned unchanged whenever
    /// neither side has any pruned descendants to check (the common case).
    pub(crate) fn adjust(&self, before_id: usize, after_id: usize, base: u64) -> u64 {
        if base >= FORBIDDEN_RENAME_COST {
            return base;
        }
        if let Some(targets) = self.before_pruned_targets.get(&before_id) {
            let parents = self.after_parents.as_ref().expect("built when map non-empty");
            if targets.iter().any(|&t| !is_ancestor_or_self(after_id, t, parents)) {
                return FORBIDDEN_RENAME_COST;
            }
        }
        if let Some(targets) = self.after_pruned_targets.get(&after_id) {
            let parents = self.before_parents.as_ref().expect("built when map non-empty");
            if targets.iter().any(|&t| !is_ancestor_or_self(before_id, t, parents)) {
                return FORBIDDEN_RENAME_COST;
            }
        }
        base
    }
}

/// Resolve the mapping for a forest of (possibly already partially mapped) sibling roots on
/// each side. This is the single entry point that builds the pruned postorder indexers, runs
/// the keyroot-based forest-distance computation, and translates the result into `diff`.
/// Which tree-edit-distance algorithm `resolve_forest` should run to populate the delta table
/// that `compute_edit_mapping` then backtraces through. Both produce optimal distances; this is
/// purely a backend choice threaded through from `for_roots`/`for_nodes`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Algorithm {
    ZhangShasha,
    Apted,
}

pub(crate) fn resolve_forest(
    before_root_ids: Vec<usize>,
    after_root_ids: Vec<usize>,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    cost_model: &UnitCostModel,
    algorithm: Algorithm,
    diff: &mut ASTDiff,
) {
    let before_root_ids = filter_before_nodes(before_root_ids, diff);
    let after_root_ids = filter_after_nodes(after_root_ids, diff);
    if before_root_ids.is_empty() && after_root_ids.is_empty() {
        return;
    }
    if before_root_ids.is_empty() {
        for id in after_root_ids {
            add_insert_mappings(id, after_meta, diff);
        }
        return;
    }
    if after_root_ids.is_empty() {
        for id in before_root_ids {
            add_delete_mappings(id, before_meta, diff);
        }
        return;
    }

    // Fast path: a single root pair whose subtrees are bit-for-bit identical (same structure
    // and same leaf values) never benefits from running the expensive tree-edit-distance
    // machinery - just walk both subtrees in lockstep and mark everything Identical. Unlike a
    // general hash-based pre-matching pass over arbitrary interior nodes (tried and reverted -
    // see git history - it could pick a same-kind-but-unrelated node as a "free rename" partner
    // for genuinely new content, since the cost model treats any two same-kind internal nodes
    // as freely renameable regardless of content), this is always safe: it only ever matches
    // the exact pair it was asked to resolve, never an arbitrary interior node.
    if before_root_ids.len() == 1 && after_root_ids.len() == 1 {
        let b = before_root_ids[0];
        let a = after_root_ids[0];
        let hashes_match = before_meta
            .node_to_full_hash
            .get(&b)
            .zip(after_meta.node_to_full_hash.get(&a))
            .is_some_and(|(bh, ah)| bh == ah);
        if hashes_match {
            emit_identical_subtree(b, a, before_meta, after_meta, diff);
            return;
        }
        // Fast path: flat trees (single root, all leaf children) → Myers O(ND) sequence diff.
        // Zhang-Shasha has no structural savings on depth-1 trees; Myers is O(N·d) where d is
        // the edit distance, typically much smaller than N for lightly-modified files.
        if let (Some(bc), Some(ac)) = (
            flat_children(b, before_meta, &diff.before_node_map),
            flat_children(a, after_meta, &diff.after_node_map),
        ) {
            resolve_flat_tree_pair(b, a, bc, ac, before_meta, after_meta, diff);
            return;
        }
    }

    let before_idx = PostorderIndexer::build(before_meta, &before_root_ids, &diff.before_node_map);
    let after_idx = PostorderIndexer::build(after_meta, &after_root_ids, &diff.after_node_map);

    // Built once and threaded into every `ren()` evaluation below (both the delta sweep and the
    // backtrace), so a "hollowed out" ancestor left behind by pruning can't freely rename onto a
    // node that would contradict where its pruned descendant already landed. See `ContainmentCtx`.
    let containment = ContainmentCtx::build(
        &before_root_ids,
        &after_root_ids,
        before_meta,
        after_meta,
        diff,
    );

    let mut delta = match algorithm {
        Algorithm::ZhangShasha => compute_delta_zhang_shasha(
            &before_idx,
            &after_idx,
            before_meta,
            after_meta,
            cost_model,
            Some(&containment),
        ),
        // NOTE: `compute_delta` (engine.rs) does not know about `containment` - its own `vren`
        // has no containment-consistency check. `compute_edit_mapping` below still applies
        // `containment` via `forest_dist` regardless of which branch filled `delta`, so if this
        // branch is ever exercised on a forest with a real containment violation, its `delta`
        // table and the shared backtrace's `forest_dist` calls would disagree. Currently safe
        // because nothing in the pipeline passes `Algorithm::Apted` (see the TODO on
        // `Diff::from_code`) and the existing Apted oracle tests don't hit that scenario - but fix
        // this in lockstep with `compute_delta_zhang_shasha` before switching production over.
        Algorithm::Apted => compute_delta(
            &before_idx,
            &after_idx,
            before_meta,
            after_meta,
            cost_model,
            &before_root_ids,
            &after_root_ids,
            &diff.before_node_map,
            &diff.after_node_map,
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

    let before_parents = build_parent_map(before_meta);
    let after_parents = build_parent_map(after_meta);
    improve_slot_alignment(
        before_meta,
        after_meta,
        diff,
        &before_root_ids,
        &before_parents,
        &after_parents,
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

/// Compute the optimal tree edit distance using a postorder, single-node-granularity
/// Zhang-Shasha/APTED-style forest distance, given before/after node id lists.
pub fn for_nodes(
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
    before_node_ids: Vec<usize>,
    after_node_ids: Vec<usize>,
    algorithm: Algorithm,
    diff: &mut ASTDiff,
) -> Result<()> {
    let cost_model = UnitCostModel {
        language: before_metadata.language,
    };
    resolve_forest(
        before_node_ids,
        after_node_ids,
        before_metadata,
        after_metadata,
        &cost_model,
        algorithm,
        diff,
    );
    Ok(())
}

/// Compute the tree edit distance for root nodes, using whichever `algorithm` the caller picks.
pub fn for_roots(
    before: &Code,
    after: &Code,
    _node_cache: &NodeCache,
    algorithm: Algorithm,
    diff: &mut ASTDiff,
) -> Result<()> {
    // Compute metadata once at the top level
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
        diff,
    )
}

#[cfg(test)]
mod tests {
    use super::super::engine::*;
    use super::*;
    use crate::diff::{ASTMappingOperation, ASTMappingReason};
    use crate::test::helper;

    fn synthetic_meta(nodes: &[(usize, &str, &str, &[usize])]) -> ASTMetadata {
        let mut node_info = HashMap::new();
        for &(id, kind, text, children) in nodes {
            node_info.insert(
                id,
                ASTNodeMetadata {
                    kind: kind.to_string(),
                    text: text.to_string(),
                    children: children.to_vec(),
                },
            );
        }
        ASTMetadata {
            node_info,
            ..Default::default()
        }
    }

    fn mapping_total_cost(
        decisions: &[RawDecision],
        before_meta: &ASTMetadata,
        after_meta: &ASTMetadata,
        cost_model: &UnitCostModel,
    ) -> u64 {
        decisions
            .iter()
            .map(|d| match *d {
                RawDecision::Match(b, a) => {
                    cost_model.ren(&before_meta.node_info[&b], &after_meta.node_info[&a])
                }
                RawDecision::Delete(b) => cost_model.del(&before_meta.node_info[&b]),
                RawDecision::Insert(a) => cost_model.ins(&after_meta.node_info[&a]),
            })
            .sum()
    }

    /// Differential check: the new APTED-engine-backed `compute_delta` must produce a mapping
    /// with the exact same total cost as the classic Zhang-Shasha oracle, for the given forests.
    fn assert_distance_matches_oracle(
        before_meta: &ASTMetadata,
        after_meta: &ASTMetadata,
        before_root_ids: &[usize],
        after_root_ids: &[usize],
    ) {
        assert_distance_matches_oracle_pruned(
            before_meta,
            after_meta,
            before_root_ids,
            after_root_ids,
            &HashMap::new(),
            &HashMap::new(),
        );
    }

    fn assert_distance_matches_oracle_pruned(
        before_meta: &ASTMetadata,
        after_meta: &ASTMetadata,
        before_root_ids: &[usize],
        after_root_ids: &[usize],
        before_node_map: &HashMap<usize, usize>,
        after_node_map: &HashMap<usize, usize>,
    ) {
        let cost_model = UnitCostModel {
            language: Language::Unknown,
        };

        let before_idx = PostorderIndexer::build(before_meta, before_root_ids, before_node_map);
        let after_idx = PostorderIndexer::build(after_meta, after_root_ids, after_node_map);

        let mut oracle_delta = compute_delta_zhang_shasha(
            &before_idx,
            &after_idx,
            before_meta,
            after_meta,
            &cost_model,
        None,
        );
        let oracle_decisions = compute_edit_mapping(
            &before_idx,
            &after_idx,
            before_meta,
            after_meta,
            &cost_model,
            None,
            &mut oracle_delta,
        );
        let oracle_cost =
            mapping_total_cost(&oracle_decisions, before_meta, after_meta, &cost_model);

        let mut new_delta = compute_delta(
            &before_idx,
            &after_idx,
            before_meta,
            after_meta,
            &cost_model,
            before_root_ids,
            after_root_ids,
            before_node_map,
            after_node_map,
        );
        let new_decisions = compute_edit_mapping(
            &before_idx,
            &after_idx,
            before_meta,
            after_meta,
            &cost_model,
            None,
            &mut new_delta,
        );
        let new_cost = mapping_total_cost(&new_decisions, before_meta, after_meta, &cost_model);

        assert_eq!(
            new_cost, oracle_cost,
            "new engine cost {new_cost} != oracle cost {oracle_cost}\nbefore_roots={before_root_ids:?} after_roots={after_root_ids:?}"
        );
    }

    /// Same as `assert_distance_matches_oracle_pruned`, but pins the forced-RIGHT driver
    /// instead of the live (forced-left) engine - validates `spf_r`/`compute_right_keyroots`/
    /// `apted_tree_edit_dist_r` in isolation, the same way the forced-left tests above pin `spf_l`.
    fn assert_distance_matches_oracle_forced_right(
        before_meta: &ASTMetadata,
        after_meta: &ASTMetadata,
        before_root_ids: &[usize],
        after_root_ids: &[usize],
    ) {
        let cost_model = UnitCostModel {
            language: Language::Unknown,
        };
        let empty_map = HashMap::new();

        let before_idx = PostorderIndexer::build(before_meta, before_root_ids, &empty_map);
        let after_idx = PostorderIndexer::build(after_meta, after_root_ids, &empty_map);

        let mut oracle_delta = compute_delta_zhang_shasha(
            &before_idx,
            &after_idx,
            before_meta,
            after_meta,
            &cost_model,
        None,
        );
        let oracle_decisions = compute_edit_mapping(
            &before_idx,
            &after_idx,
            before_meta,
            after_meta,
            &cost_model,
            None,
            &mut oracle_delta,
        );
        let oracle_cost =
            mapping_total_cost(&oracle_decisions, before_meta, after_meta, &cost_model);

        let mut new_delta = compute_delta_forced_right(
            &before_idx,
            &after_idx,
            before_meta,
            after_meta,
            &cost_model,
            before_root_ids,
            after_root_ids,
            &empty_map,
            &empty_map,
        );
        let new_decisions = compute_edit_mapping(
            &before_idx,
            &after_idx,
            before_meta,
            after_meta,
            &cost_model,
            None,
            &mut new_delta,
        );
        let new_cost = mapping_total_cost(&new_decisions, before_meta, after_meta, &cost_model);

        assert_eq!(
            new_cost, oracle_cost,
            "forced-right cost {new_cost} != oracle cost {oracle_cost}\nbefore_roots={before_root_ids:?} after_roots={after_root_ids:?}"
        );
    }

    #[test]
    fn test_apted_engine_forced_right_matches_oracle_fuzz() {
        let kinds = ["a", "b", "c"];
        let texts = ["x", "y", "z"];
        for seed in 0..3000u64 {
            let mut rng = Rng(seed.wrapping_mul(2685821657736338717).wrapping_add(7));
            let mut before_nodes = Vec::new();
            let mut next_id = 0usize;
            let before_root = gen_random_tree(
                &mut rng,
                &mut next_id,
                0,
                4,
                &kinds,
                &texts,
                &mut before_nodes,
            );
            let mut after_nodes = Vec::new();
            let after_root = gen_random_tree(
                &mut rng,
                &mut next_id,
                0,
                4,
                &kinds,
                &texts,
                &mut after_nodes,
            );

            let before_meta = meta_from_owned(&before_nodes);
            let after_meta = meta_from_owned(&after_nodes);

            let result = std::panic::catch_unwind(|| {
                assert_distance_matches_oracle_forced_right(
                    &before_meta,
                    &after_meta,
                    &[before_root],
                    &[after_root],
                );
            });
            if result.is_err() {
                panic!(
                    "forced-right fuzz failure at seed {seed}\nbefore_nodes={before_nodes:?}\nafter_nodes={after_nodes:?}"
                );
            }
        }
    }

    #[test]
    fn test_apted_engine_matches_oracle_single_leaf() {
        let before = synthetic_meta(&[(0, "leaf", "a", &[])]);
        let after = synthetic_meta(&[(0, "leaf", "b", &[])]);
        assert_distance_matches_oracle(&before, &after, &[0], &[0]);
    }

    #[test]
    fn test_apted_engine_matches_oracle_small_trees() {
        // before: root(a, b)   after: root(a, b, c)
        let before = synthetic_meta(&[
            (0, "root", "", &[1, 2]),
            (1, "leaf", "a", &[]),
            (2, "leaf", "b", &[]),
        ]);
        let after = synthetic_meta(&[
            (10, "root", "", &[11, 12, 13]),
            (11, "leaf", "a", &[]),
            (12, "leaf", "b", &[]),
            (13, "leaf", "c", &[]),
        ]);
        assert_distance_matches_oracle(&before, &after, &[0], &[10]);
    }

    #[test]
    fn test_apted_engine_matches_oracle_multi_root_forest() {
        // before forest: leaf(a), leaf(b), inner(c -> leaf(d))
        let before = synthetic_meta(&[
            (0, "leaf", "a", &[]),
            (1, "leaf", "b", &[]),
            (2, "inner", "", &[3]),
            (3, "leaf", "d", &[]),
        ]);
        // after forest: leaf(a), inner(c -> leaf(d), leaf(e)), leaf(z)
        let after = synthetic_meta(&[
            (10, "leaf", "a", &[]),
            (11, "inner", "", &[12, 13]),
            (12, "leaf", "d", &[]),
            (13, "leaf", "e", &[]),
            (14, "leaf", "z", &[]),
        ]);
        assert_distance_matches_oracle(&before, &after, &[0, 1, 2], &[10, 11, 14]);
    }

    #[test]
    fn test_apted_engine_matches_oracle_deep_unbalanced() {
        // before: a deep left chain with a branchy right side.
        let before = synthetic_meta(&[
            (0, "root", "", &[1, 6]),
            (1, "chain", "", &[2]),
            (2, "chain", "", &[3]),
            (3, "chain", "", &[4]),
            (4, "leaf", "x", &[]),
            (6, "branch", "", &[7, 8, 9]),
            (7, "leaf", "p", &[]),
            (8, "leaf", "q", &[]),
            (9, "leaf", "r", &[]),
        ]);
        let after = synthetic_meta(&[
            (100, "root", "", &[101, 106]),
            (101, "chain", "", &[102]),
            (102, "chain", "", &[104]),
            (104, "leaf", "x", &[]),
            (106, "branch", "", &[107, 109, 108]),
            (107, "leaf", "p", &[]),
            (108, "leaf", "q", &[]),
            (109, "leaf", "s", &[]),
        ]);
        assert_distance_matches_oracle(&before, &after, &[0], &[100]);
    }

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
        fn range(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    fn gen_random_tree(
        rng: &mut Rng,
        next_id: &mut usize,
        depth: usize,
        max_depth: usize,
        kinds: &[&str],
        texts: &[&str],
        nodes: &mut Vec<(usize, String, String, Vec<usize>)>,
    ) -> usize {
        let id = *next_id;
        *next_id += 1;
        let kind = kinds[rng.range(kinds.len())];
        let is_leaf = depth >= max_depth || rng.range(3) == 0;
        if is_leaf {
            let text = texts[rng.range(texts.len())];
            nodes.push((id, kind.to_string(), text.to_string(), Vec::new()));
        } else {
            let nchildren = 1 + rng.range(3);
            let mut child_ids = Vec::new();
            for _ in 0..nchildren {
                child_ids.push(gen_random_tree(
                    rng,
                    next_id,
                    depth + 1,
                    max_depth,
                    kinds,
                    texts,
                    nodes,
                ));
            }
            nodes.push((id, kind.to_string(), String::new(), child_ids));
        }
        id
    }

    fn meta_from_owned(nodes: &[(usize, String, String, Vec<usize>)]) -> ASTMetadata {
        let mut node_info = HashMap::new();
        for (id, kind, text, children) in nodes {
            node_info.insert(
                *id,
                ASTNodeMetadata {
                    kind: kind.clone(),
                    text: text.clone(),
                    children: children.clone(),
                },
            );
        }
        ASTMetadata {
            node_info,
            ..Default::default()
        }
    }

    /// Perfectly balanced binary tree: `2^depth - 1` nodes, the adversarial shape for an
    /// L/R-only (no spfA/INNER) decomposition strategy per the APTED papers.
    fn gen_balanced_binary_tree(
        next_id: &mut usize,
        depth: usize,
        kinds: &[&str],
        texts: &[&str],
        nodes: &mut Vec<(usize, String, String, Vec<usize>)>,
    ) -> usize {
        let id = *next_id;
        *next_id += 1;
        let kind = kinds[id % kinds.len()];
        if depth == 0 {
            let text = texts[id % texts.len()];
            nodes.push((id, kind.to_string(), text.to_string(), Vec::new()));
        } else {
            let left = gen_balanced_binary_tree(next_id, depth - 1, kinds, texts, nodes);
            let right = gen_balanced_binary_tree(next_id, depth - 1, kinds, texts, nodes);
            nodes.push((id, kind.to_string(), String::new(), vec![left, right]));
        }
        id
    }

    #[test]
    #[ignore = "ad hoc timing measurement, not a correctness check"]
    fn bench_compute_delta_large_balanced_trees() {
        let kinds = ["a", "b", "c"];
        let texts = ["x", "y", "z"];
        let cost_model = UnitCostModel {
            language: Language::Unknown,
        };
        let empty_map = HashMap::new();

        for depth in [9usize, 10, 11, 12, 14] {
            let mut before_nodes = Vec::new();
            let mut next_id = 0usize;
            let before_root =
                gen_balanced_binary_tree(&mut next_id, depth, &kinds, &texts, &mut before_nodes);
            let mut after_nodes = Vec::new();
            // A different balanced tree of the same shape/size, so nothing trivially matches by
            // id and the engine has to do real work, like an unmatched-residual diff would.
            let after_root =
                gen_balanced_binary_tree(&mut next_id, depth, &kinds, &texts, &mut after_nodes);

            let before_meta = meta_from_owned(&before_nodes);
            let after_meta = meta_from_owned(&after_nodes);
            let before_idx = PostorderIndexer::build(&before_meta, &[before_root], &empty_map);
            let after_idx = PostorderIndexer::build(&after_meta, &[after_root], &empty_map);
            let n = before_nodes.len();

            let t0 = std::time::Instant::now();
            let mut new_delta = compute_delta(
                &before_idx,
                &after_idx,
                &before_meta,
                &after_meta,
                &cost_model,
                &[before_root],
                &[after_root],
                &empty_map,
                &empty_map,
            );
            let _ = compute_edit_mapping(
                &before_idx,
                &after_idx,
                &before_meta,
                &after_meta,
                &cost_model,
                None,
                &mut new_delta,
            );
            let new_elapsed = t0.elapsed();

            let t0 = std::time::Instant::now();
            let mut oracle_delta = compute_delta_zhang_shasha(
                &before_idx,
                &after_idx,
                &before_meta,
                &after_meta,
                &cost_model,
            None,
            );
            let _ = compute_edit_mapping(
                &before_idx,
                &after_idx,
                &before_meta,
                &after_meta,
                &cost_model,
                None,
                &mut oracle_delta,
            );
            let oracle_elapsed = t0.elapsed();

            eprintln!(
                "depth={depth} n={n}: new_engine={new_elapsed:?} zhang_shasha_oracle={oracle_elapsed:?}"
            );
        }
    }

    #[test]
    #[ignore = "ad hoc timing measurement, not a correctness check"]
    fn bench_compute_delta_typical_random_trees() {
        let kinds = ["a", "b", "c"];
        let texts = ["x", "y", "z"];
        let cost_model = UnitCostModel {
            language: Language::Unknown,
        };
        let empty_map = HashMap::new();

        for depth in [8usize, 9, 10] {
            let mut rng = Rng(depth as u64 * 7919 + 1);
            let mut before_nodes = Vec::new();
            let mut next_id = 0usize;
            let before_root = gen_random_tree(
                &mut rng,
                &mut next_id,
                0,
                depth,
                &kinds,
                &texts,
                &mut before_nodes,
            );
            let mut after_nodes = Vec::new();
            let after_root = gen_random_tree(
                &mut rng,
                &mut next_id,
                0,
                depth,
                &kinds,
                &texts,
                &mut after_nodes,
            );

            let before_meta = meta_from_owned(&before_nodes);
            let after_meta = meta_from_owned(&after_nodes);
            let before_idx = PostorderIndexer::build(&before_meta, &[before_root], &empty_map);
            let after_idx = PostorderIndexer::build(&after_meta, &[after_root], &empty_map);
            let n = before_nodes.len() + after_nodes.len();

            let t0 = std::time::Instant::now();
            let mut new_delta = compute_delta(
                &before_idx,
                &after_idx,
                &before_meta,
                &after_meta,
                &cost_model,
                &[before_root],
                &[after_root],
                &empty_map,
                &empty_map,
            );
            let _ = compute_edit_mapping(
                &before_idx,
                &after_idx,
                &before_meta,
                &after_meta,
                &cost_model,
                None,
                &mut new_delta,
            );
            let new_elapsed = t0.elapsed();

            let t0 = std::time::Instant::now();
            let mut oracle_delta = compute_delta_zhang_shasha(
                &before_idx,
                &after_idx,
                &before_meta,
                &after_meta,
                &cost_model,
            None,
            );
            let _ = compute_edit_mapping(
                &before_idx,
                &after_idx,
                &before_meta,
                &after_meta,
                &cost_model,
                None,
                &mut oracle_delta,
            );
            let oracle_elapsed = t0.elapsed();

            eprintln!(
                "depth={depth} n={n}: new_engine={new_elapsed:?} zhang_shasha_oracle={oracle_elapsed:?}"
            );
        }
    }

    #[test]
    fn test_apted_engine_matches_oracle_fuzz_minimal_repro() {
        let before = meta_from_owned(&[
            (2, "a".into(), "x".into(), vec![]),
            (5, "a".into(), "z".into(), vec![]),
            (4, "c".into(), "".into(), vec![5]),
            (6, "a".into(), "x".into(), vec![]),
            (3, "c".into(), "".into(), vec![4, 6]),
            (1, "c".into(), "".into(), vec![2, 3]),
            (0, "a".into(), "".into(), vec![1]),
        ]);
        let after = meta_from_owned(&[
            (9, "b".into(), "z".into(), vec![]),
            (8, "b".into(), "".into(), vec![9]),
            (11, "b".into(), "z".into(), vec![]),
            (12, "a".into(), "x".into(), vec![]),
            (10, "a".into(), "".into(), vec![11, 12]),
            (14, "b".into(), "y".into(), vec![]),
            (13, "b".into(), "".into(), vec![14]),
            (7, "b".into(), "".into(), vec![8, 10, 13]),
        ]);
        assert_distance_matches_oracle(&before, &after, &[0], &[7]);
    }

    #[test]
    fn test_apted_engine_matches_oracle_tiny_repro() {
        let before = meta_from_owned(&[
            (1, "c".into(), "z".into(), vec![]),
            (4, "c".into(), "x".into(), vec![]),
            (3, "b".into(), "".into(), vec![4]),
            (5, "a".into(), "z".into(), vec![]),
            (2, "a".into(), "".into(), vec![3, 5]),
            (0, "c".into(), "".into(), vec![1, 2]),
        ]);
        let after = meta_from_owned(&[(6, "c".into(), "y".into(), vec![])]);
        assert_distance_matches_oracle(&before, &after, &[0], &[6]);
    }

    fn debug_dump_case(
        before: &ASTMetadata,
        after: &ASTMetadata,
        before_root: usize,
        after_root: usize,
    ) {
        let cost_model = UnitCostModel {
            language: Language::Unknown,
        };
        let empty_map = HashMap::new();
        let before_idx = PostorderIndexer::build(before, &[before_root], &empty_map);
        let after_idx = PostorderIndexer::build(after, &[after_root], &empty_map);

        let mut oracle_delta =
            compute_delta_zhang_shasha(&before_idx, &after_idx, before, after, &cost_model, None);
        // Snapshot *before* compute_edit_mapping, which mutates delta in place as it recomputes
        // forest_dist - comparing post-mutation tables would compare the wrong thing.
        let oracle_snapshot: Vec<Vec<u64>> = (0..before_idx.size)
            .map(|b| {
                (0..after_idx.size)
                    .map(|a| oracle_delta.get(b, a))
                    .collect()
            })
            .collect();
        let oracle_decisions = compute_edit_mapping(
            &before_idx,
            &after_idx,
            before,
            after,
            &cost_model,
            None,
            &mut oracle_delta,
        );
        eprintln!("ORACLE decisions: {oracle_decisions:?}");

        let mut new_delta = compute_delta(
            &before_idx,
            &after_idx,
            before,
            after,
            &cost_model,
            &[before_root],
            &[after_root],
            &empty_map,
            &empty_map,
        );
        let new_snapshot: Vec<Vec<u64>> = (0..before_idx.size)
            .map(|b| (0..after_idx.size).map(|a| new_delta.get(b, a)).collect())
            .collect();
        let new_decisions = compute_edit_mapping(
            &before_idx,
            &after_idx,
            before,
            after,
            &cost_model,
            None,
            &mut new_delta,
        );
        eprintln!("NEW decisions: {new_decisions:?}");

        for b in 0..before_idx.size {
            for a in 0..after_idx.size {
                let bn = before_idx.node_id_at(before_idx.pre_to_post[b] + 1);
                let an = after_idx.node_id_at(after_idx.pre_to_post[a] + 1);
                let ov = oracle_snapshot[b][a];
                let nv = new_snapshot[b][a];
                eprintln!("delta[{b}(id={bn})][{a}(id={an})]: oracle={ov} new={nv}");
            }
        }

        // Recompute the top-level forest_dist table fresh from each snapshot, to find exactly
        // where the two diverge (since compute_edit_mapping mutates delta as it runs).
        let rebuild = |snap: &[Vec<u64>]| {
            let mut d = DeltaTable::new(before_idx.size.max(1), after_idx.size.max(1));
            for (b, row) in snap.iter().enumerate() {
                for (a, &v) in row.iter().enumerate() {
                    if v != 0 {
                        d.set(b, a, v);
                    }
                }
            }
            d
        };
        let mut oracle_d2 = rebuild(&oracle_snapshot);
        let mut new_d2 = rebuild(&new_snapshot);
        let mut oracle_fd = ForestDist::new(before_idx.size + 1, after_idx.size + 1);
        let mut new_fd = ForestDist::new(before_idx.size + 1, after_idx.size + 1);
        forest_dist(
            &before_idx,
            &after_idx,
            before,
            after,
            &cost_model,
            None,
            &mut oracle_d2,
            before_idx.size,
            after_idx.size,
            &mut oracle_fd,
            false,
        );
        forest_dist(
            &before_idx,
            &after_idx,
            before,
            after,
            &cost_model,
            None,
            &mut new_d2,
            before_idx.size,
            after_idx.size,
            &mut new_fd,
            false,
        );
        for di in 0..=before_idx.size {
            for dj in 0..=after_idx.size {
                let ov = oracle_fd[(di, dj)];
                let nv = new_fd[(di, dj)];
                eprintln!(
                    "forestdist[{di}][{dj}]: oracle={ov} new={nv}{}",
                    if ov != nv { " <<<" } else { "" }
                );
            }
        }
    }

    #[test]
    fn debug_dump_tiny_repro() {
        let before = meta_from_owned(&[
            (1, "c".into(), "z".into(), vec![]),
            (4, "c".into(), "x".into(), vec![]),
            (3, "b".into(), "".into(), vec![4]),
            (5, "a".into(), "z".into(), vec![]),
            (2, "a".into(), "".into(), vec![3, 5]),
            (0, "c".into(), "".into(), vec![1, 2]),
        ]);
        let after = meta_from_owned(&[(6, "c".into(), "y".into(), vec![])]);
        debug_dump_case(&before, &after, 0, 6);
    }

    #[test]
    fn debug_dump_n7_repro() {
        let before = meta_from_owned(&[
            (1, "c".into(), "z".into(), vec![]),
            (3, "a".into(), "z".into(), vec![]),
            (2, "a".into(), "".into(), vec![3]),
            (4, "a".into(), "y".into(), vec![]),
            (0, "a".into(), "".into(), vec![1, 2, 4]),
        ]);
        let after = meta_from_owned(&[
            (6, "c".into(), "x".into(), vec![]),
            (5, "a".into(), "".into(), vec![6]),
        ]);
        debug_dump_case(&before, &after, 0, 5);
    }

    #[test]
    fn debug_dump_minimal_repro() {
        let before = meta_from_owned(&[
            (2, "a".into(), "x".into(), vec![]),
            (5, "a".into(), "z".into(), vec![]),
            (4, "c".into(), "".into(), vec![5]),
            (6, "a".into(), "x".into(), vec![]),
            (3, "c".into(), "".into(), vec![4, 6]),
            (1, "c".into(), "".into(), vec![2, 3]),
            (0, "a".into(), "".into(), vec![1]),
        ]);
        let after = meta_from_owned(&[
            (9, "b".into(), "z".into(), vec![]),
            (8, "b".into(), "".into(), vec![9]),
            (11, "b".into(), "z".into(), vec![]),
            (12, "a".into(), "x".into(), vec![]),
            (10, "a".into(), "".into(), vec![11, 12]),
            (14, "b".into(), "y".into(), vec![]),
            (13, "b".into(), "".into(), vec![14]),
            (7, "b".into(), "".into(), vec![8, 10, 13]),
        ]);
        let cost_model = UnitCostModel {
            language: Language::Unknown,
        };
        let empty_map = HashMap::new();
        let before_idx = PostorderIndexer::build(&before, &[0], &empty_map);
        let after_idx = PostorderIndexer::build(&after, &[7], &empty_map);

        let mut oracle_delta =
            compute_delta_zhang_shasha(&before_idx, &after_idx, &before, &after, &cost_model, None);
        let oracle_decisions = compute_edit_mapping(
            &before_idx,
            &after_idx,
            &before,
            &after,
            &cost_model,
            None,
            &mut oracle_delta,
        );
        eprintln!("ORACLE decisions: {oracle_decisions:?}");

        let mut new_delta = compute_delta(
            &before_idx,
            &after_idx,
            &before,
            &after,
            &cost_model,
            &[0],
            &[7],
            &empty_map,
            &empty_map,
        );
        let new_decisions = compute_edit_mapping(
            &before_idx,
            &after_idx,
            &before,
            &after,
            &cost_model,
            None,
            &mut new_delta,
        );
        eprintln!("NEW decisions: {new_decisions:?}");

        for b in 0..before_idx.size {
            for a in 0..after_idx.size {
                let ov = oracle_delta.get(b, a);
                let nv = new_delta.get(b, a);
                if ov != nv {
                    let bn = before_idx.node_id_at(before_idx.pre_to_post[b] + 1);
                    let an = after_idx.node_id_at(after_idx.pre_to_post[a] + 1);
                    eprintln!(
                        "delta mismatch: before_pre={b}(id={bn}) after_pre={a}(id={an}) oracle={ov} new={nv}"
                    );
                }
            }
        }

        // Dump the strategy table's choices (virtual-space, vroot included) to see which
        // (v, w) pairs picked INNER (the new, previously-unexercised path).
        let mut bidx = AptedIndexer::build(&before, &[0], &empty_map);
        let mut aidx = AptedIndexer::build(&after, &[7], &empty_map);
        bidx.fill_subtree_costs(&before, &cost_model);
        aidx.fill_subtree_costs(&after, &cost_model);
        let strategy = compute_opt_strategy_post_l(&bidx, &aidx, false);
        let path_id_offset = bidx.size as i64;
        for v in 0..bidx.size {
            for w in 0..aidx.size {
                if bidx.sizes[v] <= 1 || aidx.sizes[w] <= 1 {
                    continue;
                }
                let sp = strategy.get(v, w);
                let node = sp.abs() - 1;
                let is_t1 = node < path_id_offset;
                let (idx, root, sz) = if is_t1 {
                    (&bidx, v, bidx.sizes[v])
                } else {
                    (&aidx, w, aidx.sizes[w])
                };
                let local_node = if is_t1 { node } else { node - path_id_offset };
                let ty = get_strategy_path_type(sp, path_id_offset, root, sz);
                if ty == 2 {
                    eprintln!(
                        "INNER: v={v} w={w} sp={sp} is_t1={is_t1} local_node={local_node} (idx size={})",
                        idx.size
                    );
                }
            }
        }
    }

    #[test]
    #[ignore = "shrinker for the spfA bug hunt, not a correctness check"]
    fn shrink_apted_engine_fuzz_failure() {
        let kinds = ["a", "b", "c"];
        let texts = ["x", "y", "z"];
        let mut smallest: Option<(usize, u64, Vec<_>, Vec<_>)> = None;
        for seed in 0..20000u64 {
            let mut rng = Rng(seed.wrapping_mul(2685821657736338717).wrapping_add(1));
            let mut before_nodes = Vec::new();
            let mut next_id = 0usize;
            let before_root = gen_random_tree(
                &mut rng,
                &mut next_id,
                0,
                3,
                &kinds,
                &texts,
                &mut before_nodes,
            );
            let mut after_nodes = Vec::new();
            let after_root = gen_random_tree(
                &mut rng,
                &mut next_id,
                0,
                3,
                &kinds,
                &texts,
                &mut after_nodes,
            );
            let n = before_nodes.len() + after_nodes.len();
            if let Some((best_n, ..)) = &smallest
                && n >= *best_n
            {
                continue;
            }

            let before_meta = meta_from_owned(&before_nodes);
            let after_meta = meta_from_owned(&after_nodes);
            let result = std::panic::catch_unwind(|| {
                assert_distance_matches_oracle(
                    &before_meta,
                    &after_meta,
                    &[before_root],
                    &[after_root],
                );
            });
            if result.is_err() {
                smallest = Some((n, seed, before_nodes, after_nodes));
            }
        }
        match smallest {
            Some((n, seed, before_nodes, after_nodes)) => panic!(
                "smallest failure: n={n} seed={seed}\nbefore_nodes={before_nodes:?}\nafter_nodes={after_nodes:?}"
            ),
            None => eprintln!("no failures found"),
        }
    }

    #[test]
    fn test_apted_engine_matches_oracle_fuzz() {
        let kinds = ["a", "b", "c"];
        let texts = ["x", "y", "z"];
        for seed in 0..3000u64 {
            let mut rng = Rng(seed.wrapping_mul(2685821657736338717).wrapping_add(1));
            let mut before_nodes = Vec::new();
            let mut next_id = 0usize;
            let before_root = gen_random_tree(
                &mut rng,
                &mut next_id,
                0,
                4,
                &kinds,
                &texts,
                &mut before_nodes,
            );
            let mut after_nodes = Vec::new();
            let after_root = gen_random_tree(
                &mut rng,
                &mut next_id,
                0,
                4,
                &kinds,
                &texts,
                &mut after_nodes,
            );

            let before_meta = meta_from_owned(&before_nodes);
            let after_meta = meta_from_owned(&after_nodes);

            let result = std::panic::catch_unwind(|| {
                assert_distance_matches_oracle(
                    &before_meta,
                    &after_meta,
                    &[before_root],
                    &[after_root],
                );
            });
            if result.is_err() {
                panic!(
                    "fuzz failure at seed {seed}\nbefore_nodes={before_nodes:?}\nafter_nodes={after_nodes:?}"
                );
            }
        }
    }

    #[test]
    fn test_apted_engine_matches_oracle_with_pruned_descendants() {
        // root(a, b, c, d) vs root(a, x, c, y) - but `b`/`d` (before) and `x`/`y` (after) are
        // already matched elsewhere, so only `root`+`a`+`c` survive pruning into a forest of
        // multiple unmatched roots per side (since the pruned nodes break contiguity).
        let before = synthetic_meta(&[
            (0, "root", "", &[1, 2, 3, 4]),
            (1, "leaf", "a", &[]),
            (2, "leaf", "b", &[]),
            (3, "leaf", "c", &[]),
            (4, "leaf", "d", &[]),
        ]);
        let after = synthetic_meta(&[
            (10, "root", "", &[11, 12, 13, 14]),
            (11, "leaf", "a", &[]),
            (12, "leaf", "x", &[]),
            (13, "leaf", "c", &[]),
            (14, "leaf", "y", &[]),
        ]);
        let before_map: HashMap<usize, usize> = [(2, 12), (4, 14)].into_iter().collect();
        let after_map: HashMap<usize, usize> = [(12, 2), (14, 4)].into_iter().collect();
        assert_distance_matches_oracle_pruned(
            &before,
            &after,
            &[0],
            &[10],
            &before_map,
            &after_map,
        );
    }

    #[test]
    fn test_already_matched_nodes_are_skipped() -> Result<()> {
        // This test verifies that APTED properly skips nodes
        // that are already matched in the diff.
        //
        // Strategy: Use a code pair where nodes change, pre-populate the diff with
        // a mapping that matches a node to a DIFFERENT node than what APTED would
        // naturally choose, then verify that APTED doesn't create a second mapping
        // for the same node.
        let test_diffs = helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs.get("rust-leetcode-1-bugfix").unwrap().clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();
        let before_root = before_ast.root_node();
        let after_root = after_ast.root_node();

        // Get some child nodes to create an artificial mapping
        let mut before_cursor = before_root.walk();
        let before_children: Vec<_> = before_root.children(&mut before_cursor).collect();

        let mut after_cursor = after_root.walk();
        let after_children: Vec<_> = after_root.children(&mut after_cursor).collect();

        // If we have at least 2 children in both trees, create a cross-mapping
        // that APTED would not naturally choose
        if before_children.len() >= 2 && after_children.len() >= 2 {
            let before_node_1 = before_children[0];
            let before_node_2 = before_children[1];
            let after_node_1 = after_children[0];
            let after_node_2 = after_children[1];

            // Create a mapping that swaps the natural order
            // Map before_node_1 to after_node_2 (wrong partner)
            // and before_node_2 to after_node_1 (wrong partner)
            // This forces APTED to potentially create additional correct mappings
            let wrong_mapping_1 = ASTMapping {
                cost: 0,
                operation: ASTMappingOperation::Identical,
                reason: ASTMappingReason::OptimalIDU,
            };
            diff.add_mapping(before_node_1.id(), after_node_2.id(), wrong_mapping_1);

            let wrong_mapping_2 = ASTMapping {
                cost: 0,
                operation: ASTMappingOperation::Identical,
                reason: ASTMappingReason::OptimalIDU,
            };
            diff.add_mapping(before_node_2.id(), after_node_1.id(), wrong_mapping_2);
        }

        // Now call APTED with the diff that already has these artificial mappings
        for_roots(
            &before,
            &after,
            &node_cache,
            Algorithm::ZhangShasha,
            &mut diff,
        )?;

        // Check if any before node appears in multiple mappings
        let mut before_node_counts = std::collections::HashMap::new();
        for (before_id, _) in diff.mapping.keys() {
            *before_node_counts.entry(*before_id).or_insert(0) += 1;
        }

        // Check if any after node appears in multiple mappings
        let mut after_node_counts = std::collections::HashMap::new();
        for (_, after_id) in diff.mapping.keys() {
            *after_node_counts.entry(*after_id).or_insert(0) += 1;
        }

        // Find nodes that are mapped multiple times
        let before_nodes_with_multiple_mappings: Vec<_> = before_node_counts
            .iter()
            .filter(|&(_, count)| *count > 1)
            .map(|(&node_id, &count)| (node_id, count))
            .collect();

        let after_nodes_with_multiple_mappings: Vec<_> = after_node_counts
            .iter()
            .filter(|&(_, count)| *count > 1)
            .map(|(&node_id, &count)| (node_id, count))
            .collect();

        // Assert that no nodes are mapped multiple times
        assert!(
            before_nodes_with_multiple_mappings.is_empty(),
            "Nodes should not be mapped multiple times. Found before nodes with multiple mappings: {:?}",
            before_nodes_with_multiple_mappings
        );
        assert!(
            after_nodes_with_multiple_mappings.is_empty(),
            "Nodes should not be mapped multiple times. Found after nodes with multiple mappings: {:?}",
            after_nodes_with_multiple_mappings
        );

        Ok(())
    }

    #[test]
    fn test_honors_pre_existing_match_and_still_finds_nested_reuse() -> Result<()> {
        // Combines two things apted must get right at once: honoring a match that some earlier
        // pass already made (here, faked by hand, same technique as
        // test_already_matched_nodes_are_skipped), and still discovering the nested-reuse
        // match (the print(...) call moved one level deeper) for everything else.
        let test_diffs = helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs
            .get("python-added-if-block-small")
            .unwrap()
            .clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();
        let before_root = before_ast.root_node();
        let after_root = after_ast.root_node();

        // Pre-map the `numer = 12` assignment statement by hand, as if an earlier pass had
        // already matched it.
        let assignment_path = vec!["if_statement", "block", "expression_statement:1"];
        let before_assignment = helper::node_for_path(before_root, &assignment_path)?;
        let after_assignment = helper::node_for_path(after_root, &assignment_path)?;
        diff.add_mapping(
            before_assignment.id(),
            after_assignment.id(),
            ASTMapping {
                cost: 0,
                operation: ASTMappingOperation::Identical,
                reason: ASTMappingReason::OptimalIDU,
            },
        );

        for_roots(
            &before,
            &after,
            &node_cache,
            Algorithm::ZhangShasha,
            &mut diff,
        )?;

        // The pre-existing match must survive untouched.
        assert_eq!(
            diff.mapping
                .get(&(before_assignment.id(), after_assignment.id()))
                .map(|m| &m.reason),
            Some(&ASTMappingReason::OptimalIDU)
        );

        // The print(...) call should still be found and reused one level deeper inside the new
        // if-block, despite the unrelated pre-existing match elsewhere in the same forest.
        let print_call_before = helper::node_for_path(
            before_root,
            &["if_statement", "block", "expression_statement:2"],
        )?;
        assert!(
            diff.before_node_map
                .get(&print_call_before.id())
                .is_none_or(|&id| id != 0),
            "the reused print(...) call should be matched, not deleted"
        );

        let mapping = diff
            .mapping
            .get(&(before_root.id(), after_root.id()))
            .unwrap();
        assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);
        // Same total as test_python_added_if_block_small - the pre-existing match was for a
        // node that would have cost 0 anyway, so honoring it changes nothing about the total.
        assert_eq!(mapping.cost, 8);

        Ok(())
    }

    #[test]
    fn test_no_change() -> Result<()> {
        let test_diffs = helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs.get("rust-no-change").unwrap().clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        for_roots(
            &before,
            &after,
            &node_cache,
            Algorithm::ZhangShasha,
            &mut diff,
        )?;

        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let mapping = diff
            .mapping
            .get(&(before_ast.root_node().id(), after_ast.root_node().id()))
            .unwrap();
        assert_eq!(mapping.operation, ASTMappingOperation::Identical);
        assert_eq!(mapping.cost, 0);

        Ok(())
    }

    #[test]
    fn test_hello_world_added_message() -> Result<()> {
        let test_diffs = helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs
            .get("rust-hello-world-added-message")
            .unwrap()
            .clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        for_roots(
            &before,
            &after,
            &node_cache,
            Algorithm::ZhangShasha,
            &mut diff,
        )?;

        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let before_root = before_ast.root_node();
        let after_root = after_ast.root_node();

        let path = vec!["function_item", "block", "expression_statement:2"];

        assert!(
            helper::was_tree_added(&path, after_root, &diff)?,
            "The inserted line is not correctly marked as Insert"
        );

        let added_node = helper::node_for_path(after_root, &path)?;
        let mapping = diff.mapping.get(&(0, added_node.id())).unwrap();
        assert_eq!(mapping.operation, ASTMappingOperation::Insert);
        // 12 nodes in total are added. expression_statement + 11 more.
        assert_eq!(mapping.cost, 12);

        let mapping = diff
            .mapping
            .get(&(before_root.id(), after_root.id()))
            .unwrap();
        assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);
        // The cost should correctly transfer upwards to the root node.
        assert_eq!(mapping.cost, 12);

        Ok(())
    }

    #[test]
    fn test_hello_world_removed_message() -> Result<()> {
        let test_diffs = helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs
            .get("rust-hello-world-removed-message")
            .unwrap()
            .clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        for_roots(
            &before,
            &after,
            &node_cache,
            Algorithm::ZhangShasha,
            &mut diff,
        )?;

        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let before_root = before_ast.root_node();
        let after_root = after_ast.root_node();

        let path = vec!["function_item", "block", "expression_statement:2"];

        assert!(
            helper::was_tree_deleted(&path, before_root, &diff)?,
            "The removed line is not correctly marked as Delete"
        );

        let deleted_node = helper::node_for_path(before_root, &path)?;
        let mapping = diff.mapping.get(&(deleted_node.id(), 0)).unwrap();
        assert_eq!(mapping.operation, ASTMappingOperation::Delete);
        // 12 nodes in total are removed. expression_statement + 11 more.
        assert_eq!(mapping.cost, 12);

        let mapping = diff
            .mapping
            .get(&(before_root.id(), after_root.id()))
            .unwrap();
        assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);
        // The cost should correctly transfer upwards to the root node.
        assert_eq!(mapping.cost, 12);

        Ok(())
    }

    #[test]
    fn test_python_added_if_block_small() -> Result<()> {
        let test_diffs = helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs
            .get("python-added-if-block-small")
            .unwrap()
            .clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        for_roots(
            &before,
            &after,
            &node_cache,
            Algorithm::ZhangShasha,
            &mut diff,
        )?;

        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let before_root = before_ast.root_node();
        let after_root = after_ast.root_node();

        let mapping = diff
            .mapping
            .get(&(before_root.id(), after_root.id()))
            .unwrap();
        assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);
        // The best solution is to simply insert the 8 if_expression nodes in the tree.
        assert_eq!(mapping.cost, 8);

        Ok(())
    }

    #[test]
    fn test_python_added_if_block() -> Result<()> {
        // Larger, more realistic version of test_python_added_if_block_small: a function
        // definition precedes the if-block, and the wrapped statement is an f-string print
        // call. Pins that the nested-reuse fix generalizes beyond the minimal repro.
        let test_diffs = helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs.get("python-added-if-block").unwrap().clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        for_roots(
            &before,
            &after,
            &node_cache,
            Algorithm::ZhangShasha,
            &mut diff,
        )?;

        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let before_root = before_ast.root_node();
        let after_root = after_ast.root_node();

        // The print(...) call should be reused (not deleted+reinserted) even though it's now
        // nested one level deeper inside the new `if result != [0, 1]:` wrapper.
        let print_call_before = helper::node_for_path(
            before_root,
            &["if_statement", "block", "expression_statement:4"],
        )?;
        assert!(
            diff.before_node_map
                .get(&print_call_before.id())
                .is_none_or(|&id| id != 0),
            "the reused print(...) call should be matched, not deleted"
        );

        let mapping = diff
            .mapping
            .get(&(before_root.id(), after_root.id()))
            .unwrap();
        assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);
        // Only the new `if result != [0, 1]:` wrapper is genuinely new (if_statement, if,
        // comparison_operator, identifier, !=, list, [, integer, ",", integer, ], :, block =
        // 13 nodes); the print(...) call itself is fully reused at zero cost, not
        // deleted-and-reinserted.
        assert_eq!(mapping.cost, 13);

        Ok(())
    }

    #[test]
    fn test_rust_add_if() -> Result<()> {
        // Same wrap-in-a-new-if pattern as test_python_added_if_block*, but for Rust's grammar
        // and with the existing if/else demoted to an `else if` branch (nested one level
        // deeper as the new if's else_clause) instead of nested inside a block - guards
        // against tree-sitter-shape-specific assumptions in the fix.
        let test_diffs = helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs.get("rust-add-if").unwrap().clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        for_roots(
            &before,
            &after,
            &node_cache,
            Algorithm::ZhangShasha,
            &mut diff,
        )?;

        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let before_root = before_ast.root_node();
        let after_root = after_ast.root_node();

        // The entire original `if number % 2 == 0 { ... } else { ... }` should be reused intact
        // as the new `else if`'s content, not deleted and rebuilt.
        let original_if = helper::node_for_path(
            before_root,
            &[
                "function_item",
                "block",
                "expression_statement",
                "if_expression",
            ],
        )?;
        assert!(
            diff.before_node_map
                .get(&original_if.id())
                .is_none_or(|&id| id != 0),
            "the reused if/else should be matched, not deleted"
        );

        let mapping = diff
            .mapping
            .get(&(before_root.id(), after_root.id()))
            .unwrap();
        assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);
        // Only the new outer `if number == 0 { println!("Zero"); } else if ...` wrapper plus
        // its own new println!("Zero") body is genuinely new; the entire original if/else is
        // reused intact (at zero cost) as the new else-if's content.
        assert_eq!(mapping.cost, 23);

        Ok(())
    }

    #[test]
    fn flat_tree_myers_diff_matches_changed_tokens() -> Result<()> {
        // Two token_tree-like flat structures: 100 identical tokens plus one changed value.
        // Myers should match 100 identical tokens and mark 2 as delete/insert.
        let before_tokens: Vec<&str> = (0..50)
            .map(|_| "tok")
            .chain(std::iter::once("old_value"))
            .chain((0..50).map(|_| "tok"))
            .collect();
        let after_tokens: Vec<&str> = (0..50)
            .map(|_| "tok")
            .chain(std::iter::once("new_value"))
            .chain((0..50).map(|_| "tok"))
            .collect();

        // Build synthetic metadata where the root has 101 leaf children.
        let mut before_meta = ASTMetadata::default();
        let mut after_meta = ASTMetadata::default();

        fn build_flat(tokens: &[&str], meta: &mut ASTMetadata) -> (usize, Vec<usize>) {
            let root_id = 9000;
            let child_ids: Vec<usize> = (0..tokens.len()).map(|i| i + 1).collect();
            for (i, &tok) in tokens.iter().enumerate() {
                let id = i + 1;
                meta.node_info.insert(
                    id,
                    ASTNodeMetadata {
                        kind: "token".to_string(),
                        text: tok.to_string(),
                        children: vec![],
                    },
                );
                // Use the token text as hash so identical tokens match.
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                tok.hash(&mut h);
                meta.node_to_full_hash.insert(id, h.finish());
            }
            meta.node_info.insert(
                root_id,
                ASTNodeMetadata {
                    kind: "token_tree".to_string(),
                    text: String::new(),
                    children: child_ids.clone(),
                },
            );
            meta.node_to_full_hash.insert(root_id, 0); // different hashes → will not short-circuit
            (root_id, child_ids)
        }

        let (before_root, _) = build_flat(&before_tokens, &mut before_meta);
        let (after_root, _) = build_flat(&after_tokens, &mut after_meta);
        // Make root hashes differ so the identical fast-path is skipped.
        before_meta.node_to_full_hash.insert(before_root, 1);
        after_meta.node_to_full_hash.insert(after_root, 2);

        let mut diff = ASTDiff::default();
        for_nodes(
            &before_meta,
            &after_meta,
            vec![before_root],
            vec![after_root],
            Algorithm::ZhangShasha,
            &mut diff,
        )?;

        // Root pair should be mapped via flat-tree path.
        let root_mapping = diff
            .mapping
            .get(&(before_root, after_root))
            .expect("root mapped");
        assert_eq!(root_mapping.reason, ASTMappingReason::FlatSequenceDiff);
        assert_eq!(
            root_mapping.operation,
            ASTMappingOperation::MatchButNotIdentical
        );

        // The 100 identical "tok" tokens should all be matched (Identical).
        let identical_count = diff
            .mapping
            .values()
            .filter(|m| m.operation == ASTMappingOperation::Identical)
            .count();
        assert_eq!(
            identical_count, 100,
            "all 100 identical tokens should be matched"
        );

        // "old_value" (before child 51) should be deleted; "new_value" (after child 51) inserted.
        assert!(
            diff.mapping.contains_key(&(51, 0)),
            "old_value token should be deleted"
        );
        assert!(
            diff.mapping.contains_key(&(0, 51)),
            "new_value token should be inserted"
        );

        Ok(())
    }

    #[test]
    fn myers_lcs_basic() {
        // [1,2,3] vs [1,4,3]: matches at positions (0,0) and (2,2).
        let a = [1u64, 2, 3];
        let b = [1u64, 4, 3];
        let matches = myers_lcs(&a, &b, 100).expect("should find solution");
        assert_eq!(matches, vec![(0, 0), (2, 2)]);
    }

    #[test]
    fn myers_lcs_identical() {
        let a = [1u64, 2, 3, 4, 5];
        let matches = myers_lcs(&a, &a, 0).expect("d=0 for identical");
        assert_eq!(matches, vec![(0, 0), (1, 1), (2, 2), (3, 3), (4, 4)]);
    }

    #[test]
    fn myers_lcs_empty() {
        assert_eq!(myers_lcs(&[], &[1u64], 10), Some(vec![]));
        assert_eq!(myers_lcs(&[1u64], &[], 10), Some(vec![]));
        assert_eq!(myers_lcs(&[], &[], 10), Some(vec![]));
    }

    #[test]
    fn myers_lcs_exceeds_limit() {
        // a and b share no elements → d = n+m; with limit=3 it should return None.
        let a = [1u64, 2, 3];
        let b = [4u64, 5, 6];
        assert!(myers_lcs(&a, &b, 3).is_none());
    }
}
