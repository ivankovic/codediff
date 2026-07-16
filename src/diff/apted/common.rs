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
use crate::diff::nodes::{self, is_literal_kind, kinds_update_allowed};
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

    /// Cost for renaming (matching) two nodes.
    ///
    /// Uses an adaptive cost model based on node kinds:
    /// - Identical nodes: 0
    /// - Literal nodes: 2 - medium cost for value changes
    /// - Identifiers and generic punctuation/operators: COST_UPDATE (1) - low cost
    /// - Internal nodes: 0 - cost is accounted for by children
    /// - Different kinds (allowed): COST_UPDATE (1) to COST_DELETE + COST_INSERT + 1
    ///
    /// A prior version of this also special-cased type/field/property identifiers at cost 5, but
    /// that branch was unreachable dead code (`is_identifier_kind`'s check above it already
    /// matches every kind that branch checked for, per `IDENTIFIER_KINDS` in `diff::nodes`) - and
    /// benchmarking the branch made reachable showed cost 5 for those kinds is a net regression
    /// (APTED starts preferring delete+insert over a same-kind rename in several fixtures), so it
    /// was removed rather than fixed. See review discussion for the reachable variant and its
    /// benchmark impact if this is worth revisiting with a smaller cost value.
    pub(crate) fn ren(&self, node1: &ASTNodeMetadata, node2: &ASTNodeMetadata) -> u64 {
        if node1.kind == node2.kind {
            if node1.children.is_empty() && node2.children.is_empty() {
                // Both are leaves
                if node1.text == node2.text {
                    0 // Identical
                } else if is_literal_kind(&node1.kind) {
                    // Literals (strings, numbers, etc.) - medium cost
                    2
                } else {
                    // Identifiers are cheap to update (common in refactorings); generic
                    // punctuation/operators are also low cost.
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

/// Dense, flat-backed 2D buffer indexed as `grid[(row, col)]`. A flat `Vec<T>` (rather than
/// `Vec<Vec<T>>`) avoids one heap allocation and pointer indirection per row, which matters since
/// every table built on this is on the algorithm's hottest inner loops. Shared storage/indexing
/// behind `ForestDist`, `DeltaTable`, `StrategyTable`, and `Mat`, which used to each hand-roll
/// this same flat-`Vec` layout independently - no longer necessary to keep them textually separate
/// now that this code isn't a line-by-line port of APTED.java (whose `float[][]` tables were
/// distinct arrays too, but for reasons - reuse across `delta`/`strategy` - that don't apply here;
/// see `StrategyTable`'s doc comment).
pub(crate) struct Grid<T> {
    pub(crate) cols: usize,
    pub(crate) data: Vec<T>,
}

impl<T: Clone> Grid<T> {
    pub(crate) fn new(rows: usize, cols: usize, fill: T) -> Self {
        Grid {
            cols,
            data: vec![fill; rows * cols],
        }
    }
}

impl<T> std::ops::Index<(usize, usize)> for Grid<T> {
    type Output = T;
    fn index(&self, (row, col): (usize, usize)) -> &T {
        &self.data[row * self.cols + col]
    }
}

impl<T> std::ops::IndexMut<(usize, usize)> for Grid<T> {
    fn index_mut(&mut self, (row, col): (usize, usize)) -> &mut T {
        &mut self.data[row * self.cols + col]
    }
}

/// `spf_a`'s `s`/`t` tables track a signed path-offset space (see the doc comments at their call
/// sites), so their indices are naturally `i64` rather than `usize` - this impl lets `Mat` alias
/// `Grid` directly instead of needing its own wrapper.
impl<T> std::ops::Index<(i64, i64)> for Grid<T> {
    type Output = T;
    fn index(&self, (row, col): (i64, i64)) -> &T {
        &self.data[row as usize * self.cols + col as usize]
    }
}

impl<T> std::ops::IndexMut<(i64, i64)> for Grid<T> {
    fn index_mut(&mut self, (row, col): (i64, i64)) -> &mut T {
        &mut self.data[row as usize * self.cols + col as usize]
    }
}

/// Dense, flat-backed `forestdist[(row, col)]` buffer.
pub(crate) type ForestDist = Grid<u64>;

/// Dense table of `delta[(pre_before, pre_after)]` values, indexed directly by the pruned
/// trees' own 0-based preorder indices. Wraps `Grid` rather than aliasing it directly (unlike
/// `ForestDist`) because unset cells need to read back as `0`, not `Grid`'s own zero-initialized
/// value - `new` fills with a distinct sentinel instead, so a genuine `0` written via `set` stays
/// distinguishable from "never written" while both still read back the same way through `get`.
pub(crate) struct DeltaTable {
    grid: Grid<u64>,
}

impl DeltaTable {
    const UNSET: u64 = u64::MAX;

    pub(crate) fn new(rows: usize, cols: usize) -> Self {
        DeltaTable {
            grid: Grid::new(rows, cols, Self::UNSET),
        }
    }

    pub(crate) fn get(&self, pre_before: usize, pre_after: usize) -> u64 {
        let v = self.grid[(pre_before, pre_after)];
        if v == Self::UNSET { 0 } else { v }
    }

    pub(crate) fn set(&mut self, pre_before: usize, pre_after: usize, value: u64) {
        self.grid[(pre_before, pre_after)] = value;
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

    let mut forestdist = ForestDist::new(size1 + 1, size2 + 1, 0);
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
    /// Provenance label for every `ASTMappingReason::APTED` entry this resolution produces - see
    /// `for_nodes`'s doc comment.
    pub(crate) source: &'static str,
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
    before_parents: &'a rustc_hash::FxHashMap<usize, usize>,
    after_parents: &'a rustc_hash::FxHashMap<usize, usize>,
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
        for child in filter_mapped_nodes(info.children.clone(), &diff.before_node_map) {
            total += emit_before_subtree(child, ctx, diff);
        }
    }
    if let Some(info) = ctx.after_meta.node_info.get(&after_id) {
        for child in filter_mapped_nodes(info.children.clone(), &diff.after_node_map) {
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
            reason: ASTMappingReason::APTED(ctx.source),
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
        add_delete_mappings(before_id, ctx.before_meta, ctx.source, diff);
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
        for child in filter_mapped_nodes(info.children.clone(), &diff.before_node_map) {
            total += emit_before_subtree(child, ctx, diff);
        }
    }
    diff.add_mapping(
        before_id,
        0,
        ASTMapping {
            cost: total,
            operation: ASTMappingOperation::Delete,
            reason: ASTMappingReason::APTED(ctx.source),
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
        add_insert_mappings(after_id, ctx.after_meta, ctx.source, diff);
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
        for child in filter_mapped_nodes(info.children.clone(), &diff.after_node_map) {
            total += emit_after_subtree(child, ctx, diff);
        }
    }
    diff.add_mapping(
        0,
        after_id,
        ASTMapping {
            cost: total,
            operation: ASTMappingOperation::Insert,
            reason: ASTMappingReason::APTED(ctx.source),
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
    source: &'static str,
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
            reason: ASTMappingReason::APTED(source),
        },
    );
    if let (Some(before_info), Some(after_info)) = (
        before_meta.node_info.get(&before_id),
        after_meta.node_info.get(&after_id),
    ) {
        for (&bc, &ac) in before_info.children.iter().zip(after_info.children.iter()) {
            emit_identical_subtree(bc, ac, before_meta, after_meta, source, diff);
        }
    }
}

/// Adds a prune (delete-or-insert) mapping for an entire subtree that's not reused elsewhere,
/// recursively. Shared shape behind `add_delete_mappings`/`add_insert_mappings`, parameterized
/// over the four things that actually differ per side: which side's node map already-mapped
/// nodes are checked against, the `(before, after)` mapping-key shape (`(id, 0)` vs `(0, id)`),
/// the operation label, and the subtree-cost function.
#[allow(clippy::too_many_arguments)]
fn add_prune_mappings(
    node_id: usize,
    meta: &ASTMetadata,
    source: &'static str,
    diff: &mut ASTDiff,
    node_map: fn(&ASTDiff) -> &HashMap<usize, usize>,
    mapping_key: fn(usize) -> (usize, usize),
    operation: &ASTMappingOperation,
    subtree_cost: fn(usize, &ASTMetadata, &UnitCostModel) -> u64,
) {
    if node_id == 0 {
        return;
    }
    if node_map(diff).get(&node_id).is_some_and(|&x| x != 0) {
        return;
    }
    let (before_id, after_id) = mapping_key(node_id);
    if !diff.mapping.contains_key(&(before_id, after_id)) {
        let cost = subtree_cost(
            node_id,
            meta,
            &UnitCostModel {
                language: meta.language,
            },
        );
        diff.add_mapping(
            before_id,
            after_id,
            ASTMapping {
                cost,
                operation: operation.clone(),
                reason: ASTMappingReason::APTED(source),
            },
        );
    }
    if let Some(info) = meta.node_info.get(&node_id) {
        for &child_id in &info.children {
            add_prune_mappings(child_id, meta, source, diff, node_map, mapping_key, operation, subtree_cost);
        }
    }
}

/// Add delete mappings for an entire subtree (used when no part of it is reused elsewhere).
pub(crate) fn add_delete_mappings(node_id: usize, meta: &ASTMetadata, source: &'static str, diff: &mut ASTDiff) {
    add_prune_mappings(
        node_id,
        meta,
        source,
        diff,
        |diff| &diff.before_node_map,
        |id| (id, 0),
        &ASTMappingOperation::Delete,
        subtree_del_cost,
    );
}

/// Add insert mappings for an entire subtree (used when no part of it is reused elsewhere).
pub(crate) fn add_insert_mappings(node_id: usize, meta: &ASTMetadata, source: &'static str, diff: &mut ASTDiff) {
    add_prune_mappings(
        node_id,
        meta,
        source,
        diff,
        |diff| &diff.after_node_map,
        |id| (0, id),
        &ASTMappingOperation::Insert,
        subtree_ins_cost,
    );
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
    source: &'static str,
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
                    source,
                    diff,
                );
            }
            for (i, &id) in before_children.iter().enumerate() {
                if !before_matched[i] {
                    add_delete_mappings(id, before_meta, source, diff);
                }
            }
            for (i, &id) in after_children.iter().enumerate() {
                if !after_matched[i] {
                    add_insert_mappings(id, after_meta, source, diff);
                }
            }
        }
        None => {
            // Edit distance exceeds FLAT_MAX_EDIT: mark all children replaced.
            for &id in &before_children {
                add_delete_mappings(id, before_meta, source, diff);
            }
            for &id in &after_children {
                add_insert_mappings(id, after_meta, source, diff);
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

/// Filter out nodes already mapped in `node_map` (pass `diff.before_node_map`/
/// `diff.after_node_map` for the before/after side respectively).
pub(crate) fn filter_mapped_nodes(node_ids: Vec<usize>, node_map: &HashMap<usize, usize>) -> Vec<usize> {
    node_ids.into_iter().filter(|node_id| !node_map.contains_key(node_id)).collect()
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
enum SubtreeTargetOutcome {
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
fn collect_subtree_targets(
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
fn collect_before_subtree_targets(
    root: usize,
    before_meta: &ASTMetadata,
    before_decision: &HashMap<usize, BeforeDecision>,
    diff: &ASTDiff,
    out: &mut Vec<usize>,
) {
    collect_subtree_targets(root, before_meta, out, &|child| match before_decision.get(&child) {
        Some(BeforeDecision::Match(t)) => SubtreeTargetOutcome::MatchAndRecurse(*t),
        Some(BeforeDecision::Delete) => SubtreeTargetOutcome::PruneRecurse,
        None => SubtreeTargetOutcome::Leaf(diff.before_node_map.get(&child).copied().filter(|&t| t != 0)),
    });
}

/// After-side counterpart of `collect_before_subtree_targets`.
fn collect_after_subtree_targets(
    root: usize,
    after_meta: &ASTMetadata,
    after_decision: &HashMap<usize, AfterDecision>,
    diff: &ASTDiff,
    out: &mut Vec<usize>,
) {
    collect_subtree_targets(root, after_meta, out, &|child| match after_decision.get(&child) {
        Some(AfterDecision::Match(t)) => SubtreeTargetOutcome::MatchAndRecurse(*t),
        Some(AfterDecision::Insert) => SubtreeTargetOutcome::PruneRecurse,
        None => SubtreeTargetOutcome::Leaf(diff.after_node_map.get(&child).copied().filter(|&t| t != 0)),
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
fn improve_slot_alignment(
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
const LARGE_SLOT_SUBTREE: usize = 20;

/// Whether promoting deleted `b` / inserted `a` (same kind, corresponding slots) to a match is
/// consistent with everything already decided, and plausible to a human.
fn slot_promotion_allowed(
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

/// True if `node` is `ancestor` itself or a descendant of it, walking up via `parents`.
fn is_ancestor_or_self(ancestor: usize, mut node: usize, parents: &rustc_hash::FxHashMap<usize, usize>) -> bool {
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
) -> rustc_hash::FxHashMap<usize, Vec<usize>> {
    fn visit(
        node_id: usize,
        meta: &ASTMetadata,
        node_map: &HashMap<usize, usize>,
        memo: &mut rustc_hash::FxHashMap<usize, Vec<usize>>,
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

    let mut memo = rustc_hash::FxHashMap::default();
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
pub(crate) struct ContainmentCtx<'a> {
    before_pruned_targets: rustc_hash::FxHashMap<usize, Vec<usize>>,
    after_pruned_targets: rustc_hash::FxHashMap<usize, Vec<usize>>,
    before_parents: &'a rustc_hash::FxHashMap<usize, usize>,
    after_parents: &'a rustc_hash::FxHashMap<usize, usize>,
}

impl<'a> ContainmentCtx<'a> {
    fn build(
        before_root_ids: &[usize],
        after_root_ids: &[usize],
        before_meta: &'a ASTMetadata,
        after_meta: &'a ASTMetadata,
        diff: &ASTDiff,
    ) -> Self {
        let before_pruned_targets =
            compute_pruned_targets(before_root_ids, before_meta, &diff.before_node_map);
        let after_pruned_targets =
            compute_pruned_targets(after_root_ids, after_meta, &diff.after_node_map);
        // Parent maps are precomputed once per file in `ASTMetadata` (see `node_to_parent`), so
        // borrowing them here - even when this particular forest has nothing pruned and won't
        // end up using them - costs nothing beyond the borrow itself.
        ContainmentCtx {
            before_pruned_targets,
            after_pruned_targets,
            before_parents: &before_meta.node_to_parent,
            after_parents: &after_meta.node_to_parent,
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
            if targets.iter().any(|&t| !is_ancestor_or_self(after_id, t, self.after_parents)) {
                return FORBIDDEN_RENAME_COST;
            }
        }
        if let Some(targets) = self.after_pruned_targets.get(&after_id) {
            if targets.iter().any(|&t| !is_ancestor_or_self(before_id, t, self.before_parents)) {
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
    source: &'static str,
    diff: &mut ASTDiff,
) {
    let before_root_ids = filter_mapped_nodes(before_root_ids, &diff.before_node_map);
    let after_root_ids = filter_mapped_nodes(after_root_ids, &diff.after_node_map);
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
            emit_identical_subtree(b, a, before_meta, after_meta, source, diff);
            return;
        }
        // Fast path: flat trees (single root, all leaf children) → Myers O(ND) sequence diff.
        // Zhang-Shasha has no structural savings on depth-1 trees; Myers is O(N·d) where d is
        // the edit distance, typically much smaller than N for lightly-modified files.
        if let (Some(bc), Some(ac)) = (
            flat_children(b, before_meta, &diff.before_node_map),
            flat_children(a, after_meta, &diff.after_node_map),
        ) {
            resolve_flat_tree_pair(b, a, bc, ac, before_meta, after_meta, source, diff);
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

    // `compute_delta` (engine.rs) is now containment-aware: `EngineCtx.containment` is threaded
    // into every `vren` call site (`spf_a`'s `ren_cost` closure, `apted_tree_edit_dist`,
    // `apted_tree_edit_dist_r`) via `vren_adjusted`, mirroring the `ctx.adjust(...)` call in
    // `forest_dist`'s own `ren` computation (Zhang-Shasha side). So, unlike before, the algorithm
    // choice below no longer needs to fall back to `Algorithm::ZhangShasha` just because this
    // forest has real pruned-descendant constraints - both engines respect them identically.
    // Verified via `test_apted_engine_matches_oracle_fuzz_with_containment` (fuzzes forests with
    // genuine containment constraints, comparing Apted-with-containment against
    // Zhang-Shasha-with-containment) plus a manual check that each of the three `vren_adjusted`
    // sites, when individually disabled, makes that fuzz test fail on a real cost divergence.
    let mut delta = match algorithm {
        Algorithm::ZhangShasha => compute_delta_zhang_shasha(
            &before_idx,
            &after_idx,
            before_meta,
            after_meta,
            cost_model,
            Some(&containment),
        ),
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

/// Compute the optimal tree edit distance using a postorder, single-node-granularity
/// Zhang-Shasha/APTED-style forest distance, given before/after node id lists.
///
/// `source` is a short, call-site-specific label (e.g. `"final_pass"`, `"bottom_up_expansion"`)
/// recorded on every `ASTMappingReason::APTED` entry this resolution produces - see that variant's
/// doc comment. Every caller passes a distinct literal identifying which heuristic invoked APTED,
/// so two `APTED`-reasoned mappings can be told apart by provenance, not just by the fact that
/// APTED produced both.
pub fn for_nodes(
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
    before_node_ids: Vec<usize>,
    after_node_ids: Vec<usize>,
    algorithm: Algorithm,
    source: &'static str,
    diff: &mut ASTDiff,
) {
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
        source,
        diff,
    );
}

/// Compute the tree edit distance for root nodes, using whichever `algorithm` the caller picks.
/// See `for_nodes` for what `source` records.
pub fn for_roots(
    before: &Code,
    after: &Code,
    _node_cache: &NodeCache,
    algorithm: Algorithm,
    source: &'static str,
    diff: &mut ASTDiff,
) {
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
        source,
        diff,
    );
}

#[cfg(test)]
mod tests;
