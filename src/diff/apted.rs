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

use crate::code::{ASTMetadata, ASTNodeMetadata, Code};
use crate::diff::{
    ASTDiff, ASTMapping, ASTMappingOperation, ASTMappingReason, COST_DELETE, COST_INSERT,
    COST_UPDATE, NodeCache,
};

/// Cost model for APTED - unit cost model
struct UnitCostModel;

impl UnitCostModel {
    fn del(&self, _node: &ASTNodeMetadata) -> u64 {
        COST_DELETE
    }

    fn ins(&self, _node: &ASTNodeMetadata) -> u64 {
        COST_INSERT
    }

    fn ren(&self, node1: &ASTNodeMetadata, node2: &ASTNodeMetadata) -> u64 {
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
struct PostorderIndexer {
    /// Number of nodes in the pruned forest.
    size: usize,
    /// 0-based postorder index -> node id.
    post_to_node_id: Vec<usize>,
    /// 0-based postorder index -> 0-based preorder index.
    post_to_pre: Vec<usize>,
    /// 0-based preorder index -> 0-based postorder index.
    pre_to_post: Vec<usize>,
    /// 0-based postorder index -> 0-based postorder index of the leftmost leaf descendant.
    post_to_lld: Vec<usize>,
    /// 0-based preorder indices of the keyroots: the forest's own roots, plus every node that
    /// has a left sibling. Drives the bottom-up `delta` computation.
    keyroots: Vec<usize>,
}

impl PostorderIndexer {
    fn build(metadata: &ASTMetadata, root_ids: &[usize], node_map: &HashMap<usize, usize>) -> Self {
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
    fn node_id_at(&self, boundary: usize) -> usize {
        self.post_to_node_id[boundary - 1]
    }
}

/// Dense, flat-backed `forestdist[(row, col)]` buffer. A flat `Vec<u64>` (rather than
/// `Vec<Vec<u64>>`) avoids one heap allocation and pointer indirection per row, which matters
/// since this buffer is the innermost hot loop of the whole algorithm.
struct ForestDist {
    cols: usize,
    data: Vec<u64>,
}

impl ForestDist {
    fn new(rows: usize, cols: usize) -> Self {
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
struct DeltaTable {
    cols: usize,
    data: Vec<u64>,
}

impl DeltaTable {
    const UNSET: u64 = u64::MAX;

    fn new(rows: usize, cols: usize) -> Self {
        DeltaTable {
            cols,
            data: vec![Self::UNSET; rows * cols],
        }
    }

    fn get(&self, pre_before: usize, pre_after: usize) -> u64 {
        let v = self.data[pre_before * self.cols + pre_after];
        if v == Self::UNSET { 0 } else { v }
    }

    fn set(&mut self, pre_before: usize, pre_after: usize, value: u64) {
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
fn forest_dist(
    before: &PostorderIndexer,
    after: &PostorderIndexer,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    cost_model: &UnitCostModel,
    delta: &mut DeltaTable,
    i: usize,
    j: usize,
    forestdist: &mut ForestDist,
) {
    let lld_i = before.post_to_lld[i - 1];
    let lld_j = after.post_to_lld[j - 1];

    forestdist[(lld_i, lld_j)] = 0;

    // Precompute per-dj node metadata/lld/preorder once, outside the di loop - the di loop
    // would otherwise redo the same `node_info` HashMap lookup (and lld/pre array reads) for
    // every dj on every single di iteration, turning what should be O(range) prep work into
    // O(di_range * dj_range) redundant lookups.
    let dj_info: Vec<(&ASTNodeMetadata, usize, usize)> = ((lld_j + 1)..=j)
        .map(|dj| {
            let node2 = after_meta
                .node_info
                .get(&after.node_id_at(dj))
                .expect("indexed node must have metadata");
            (node2, after.post_to_lld[dj - 1], after.post_to_pre[dj - 1])
        })
        .collect();

    for di in (lld_i + 1)..=i {
        let node1 = before_meta
            .node_info
            .get(&before.node_id_at(di))
            .expect("indexed node must have metadata");
        forestdist[(di, lld_j)] = forestdist[(di - 1, lld_j)] + cost_model.del(node1);
        let lld_di = before.post_to_lld[di - 1];
        let pre_di = before.post_to_pre[di - 1];

        for (dj, &(node2, lld_dj, pre_dj)) in ((lld_j + 1)..=j).zip(dj_info.iter()) {
            forestdist[(lld_i, dj)] = forestdist[(lld_i, dj - 1)] + cost_model.ins(node2);

            let cost_ren = cost_model.ren(node1, node2);

            if lld_di == lld_i && lld_dj == lld_j {
                forestdist[(di, dj)] = (forestdist[(di - 1, dj)] + cost_model.del(node1))
                    .min(forestdist[(di, dj - 1)] + cost_model.ins(node2))
                    .min(forestdist[(di - 1, dj - 1)] + cost_ren);
                delta.set(pre_di, pre_dj, forestdist[(di - 1, dj - 1)]);
            } else {
                let delta_val = delta.get(pre_di, pre_dj);
                forestdist[(di, dj)] = (forestdist[(di - 1, dj)] + cost_model.del(node1))
                    .min(forestdist[(di, dj - 1)] + cost_model.ins(node2))
                    .min(forestdist[(lld_di, lld_dj)] + delta_val + cost_ren);
            }
        }
    }
}

/// Populates `delta[(pre_before, pre_after)]` - the fully-resolved tree edit distance between the
/// subtree rooted at `pre_before` and the subtree rooted at `pre_after` - for every keyroot pair.
/// Classic Zhang-Shasha keyroot decomposition (no `spfA`/`spfL`/`spfR` single-path optimization -
/// correct, simpler, and sufficient given APTED only ever runs on the small unmatched residual
/// left by the earlier, cheaper matching passes).
///
/// Keyroots are processed in ascending postorder index on both sides: this is what guarantees
/// that any `delta` lookup `forest_dist` performs for a given keyroot pair was already computed
/// in an earlier iteration (any interior point requiring a lookup is itself a keyroot pair with
/// strictly smaller postorder ids on both sides).
fn compute_delta(
    before: &PostorderIndexer,
    after: &PostorderIndexer,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    cost_model: &UnitCostModel,
) -> DeltaTable {
    let mut delta = DeltaTable::new(before.size.max(1), after.size.max(1));
    if before.size == 0 || after.size == 0 {
        return delta;
    }

    let mut before_keyroots = before.keyroots.clone();
    before_keyroots.sort_by_key(|&pre| before.pre_to_post[pre]);
    let mut after_keyroots = after.keyroots.clone();
    after_keyroots.sort_by_key(|&pre| after.pre_to_post[pre]);

    let mut forestdist = ForestDist::new(before.size + 1, after.size + 1);

    for &kr1_pre in &before_keyroots {
        let kr1_boundary = before.pre_to_post[kr1_pre] + 1;
        for &kr2_pre in &after_keyroots {
            let kr2_boundary = after.pre_to_post[kr2_pre] + 1;
            forest_dist(
                before,
                after,
                before_meta,
                after_meta,
                cost_model,
                &mut delta,
                kr1_boundary,
                kr2_boundary,
                &mut forestdist,
            );
        }
    }

    delta
}

/// A single, single-node-granularity decision produced by `compute_edit_mapping`.
#[derive(Debug, Clone, Copy)]
enum RawDecision {
    Match(usize, usize),
    Delete(usize),
    Insert(usize),
}

/// Backtracks through `forest_dist` to produce the globally optimal node-level edit mapping - a
/// direct port of APTED.java's `computeEditMapping`. Every node in both pruned forests ends up
/// with exactly one decision.
fn compute_edit_mapping(
    before: &PostorderIndexer,
    after: &PostorderIndexer,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    cost_model: &UnitCostModel,
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
        delta,
        size1,
        size2,
        &mut forestdist,
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
                delta,
                last_row,
                last_col,
                &mut forestdist,
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
enum BeforeDecision {
    Match(usize),
    Delete,
}

/// What ultimately happens to an after-tree node, per the raw decision list.
enum AfterDecision {
    Match(usize),
    Insert,
}

struct ResolveCtx<'a> {
    before_meta: &'a ASTMetadata,
    after_meta: &'a ASTMetadata,
    before_decision: HashMap<usize, BeforeDecision>,
    after_decision: HashMap<usize, AfterDecision>,
    before_has_match_below: HashMap<usize, bool>,
    after_has_match_below: HashMap<usize, bool>,
}

fn compute_has_match_below(
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
fn classify_match(
    before_id: usize,
    after_id: usize,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    cost_model: &UnitCostModel,
) -> (ASTMappingOperation, u64) {
    let before_info = before_meta.node_info.get(&before_id).unwrap();
    let after_info = after_meta.node_info.get(&after_id).unwrap();

    if before_info.kind != after_info.kind {
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

fn emit_match(before_id: usize, after_id: usize, ctx: &ResolveCtx, diff: &mut ASTDiff) -> u64 {
    if let Some(mapping) = diff.mapping.get(&(before_id, after_id)) {
        return mapping.cost;
    }

    let (operation, root_cost) = classify_match(
        before_id,
        after_id,
        ctx.before_meta,
        ctx.after_meta,
        &UnitCostModel,
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

fn emit_before_subtree(before_id: usize, ctx: &ResolveCtx, diff: &mut ASTDiff) -> u64 {
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
        return subtree_del_cost(before_id, ctx.before_meta, &UnitCostModel);
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

fn emit_after_subtree(after_id: usize, ctx: &ResolveCtx, diff: &mut ASTDiff) -> u64 {
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
        return subtree_ins_cost(after_id, ctx.after_meta, &UnitCostModel);
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
fn emit_identical_subtree(
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
fn add_delete_mappings(node_id: usize, meta: &ASTMetadata, diff: &mut ASTDiff) {
    if node_id == 0 {
        return;
    }
    if diff.before_node_map.get(&node_id).is_some_and(|&x| x != 0) {
        return;
    }
    if !diff.mapping.contains_key(&(node_id, 0)) {
        let cost = subtree_del_cost(node_id, meta, &UnitCostModel);
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
fn add_insert_mappings(node_id: usize, meta: &ASTMetadata, diff: &mut ASTDiff) {
    if node_id == 0 {
        return;
    }
    if diff.after_node_map.get(&node_id).is_some_and(|&x| x != 0) {
        return;
    }
    if !diff.mapping.contains_key(&(0, node_id)) {
        let cost = subtree_ins_cost(node_id, meta, &UnitCostModel);
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
fn subtree_del_cost(node_id: usize, meta: &ASTMetadata, cost_model: &UnitCostModel) -> u64 {
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
fn subtree_ins_cost(node_id: usize, meta: &ASTMetadata, cost_model: &UnitCostModel) -> u64 {
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

/// Filter out before nodes that are already mapped in the diff.
fn filter_before_nodes(node_ids: Vec<usize>, diff: &ASTDiff) -> Vec<usize> {
    node_ids
        .into_iter()
        .filter(|node_id| !diff.before_node_map.contains_key(node_id))
        .collect()
}

/// Filter out after nodes that are already mapped in the diff.
fn filter_after_nodes(node_ids: Vec<usize>, diff: &ASTDiff) -> Vec<usize> {
    node_ids
        .into_iter()
        .filter(|node_id| !diff.after_node_map.contains_key(node_id))
        .collect()
}

/// Resolve the mapping for a forest of (possibly already partially mapped) sibling roots on
/// each side. This is the single entry point that builds the pruned postorder indexers, runs
/// the keyroot-based forest-distance computation, and translates the result into `diff`.
fn resolve_forest(
    before_root_ids: Vec<usize>,
    after_root_ids: Vec<usize>,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    cost_model: &UnitCostModel,
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
    }

    let before_idx = PostorderIndexer::build(before_meta, &before_root_ids, &diff.before_node_map);
    let after_idx = PostorderIndexer::build(after_meta, &after_root_ids, &diff.after_node_map);

    let mut delta = compute_delta(&before_idx, &after_idx, before_meta, after_meta, cost_model);
    let decisions = compute_edit_mapping(
        &before_idx,
        &after_idx,
        before_meta,
        after_meta,
        cost_model,
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
    diff: &mut ASTDiff,
) -> Result<()> {
    let cost_model = UnitCostModel;
    resolve_forest(
        before_node_ids,
        after_node_ids,
        before_metadata,
        after_metadata,
        &cost_model,
        diff,
    );
    Ok(())
}

/// Compute APTED for root nodes.
pub fn for_roots(
    before: &Code,
    after: &Code,
    _node_cache: &NodeCache,
    diff: &mut ASTDiff,
) -> Result<()> {
    // Compute metadata once at the top level
    let before_metadata = before
        .metadata
        .ast_metadata
        .as_ref()
        .cloned()
        .unwrap_or_else(|| crate::code::metadata::compute_ast_metadata(before).unwrap_or_default());
    let after_metadata = after
        .metadata
        .ast_metadata
        .as_ref()
        .cloned()
        .unwrap_or_else(|| crate::code::metadata::compute_ast_metadata(after).unwrap_or_default());

    let before_root_id = before.ast.as_ref().unwrap().root_node().id();
    let after_root_id = after.ast.as_ref().unwrap().root_node().id();

    for_nodes(
        &before_metadata,
        &after_metadata,
        vec![before_root_id],
        vec![after_root_id],
        diff,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{ASTMappingOperation, ASTMappingReason};
    use crate::test::helper;

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
        let (before, after) = test_diffs.get("leet-code-1-bugfix").unwrap().clone();

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
        for_roots(&before, &after, &node_cache, &mut diff)?;

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

        for_roots(&before, &after, &node_cache, &mut diff)?;

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
        let (before, after) = test_diffs.get("no-change").unwrap().clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        for_roots(&before, &after, &node_cache, &mut diff)?;

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
        let (before, after) = test_diffs.get("hello-world-added-message").unwrap().clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        for_roots(&before, &after, &node_cache, &mut diff)?;

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
            .get("hello-world-removed-message")
            .unwrap()
            .clone();

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        for_roots(&before, &after, &node_cache, &mut diff)?;

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

        for_roots(&before, &after, &node_cache, &mut diff)?;

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

        for_roots(&before, &after, &node_cache, &mut diff)?;

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

        for_roots(&before, &after, &node_cache, &mut diff)?;

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
}
