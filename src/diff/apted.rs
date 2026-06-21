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
#[cfg(test)]
fn compute_delta_zhang_shasha(
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

/// Indexes a forest of (possibly multiple) sibling roots on one side, wrapped under a single
/// synthetic virtual root (preorder id `0`, no backing node, zero del/ins/rename cost). The
/// virtual root lets the APTED recursion (`gted`/`spfL`/`spfR`/`spfA`), which is only defined for
/// a single rooted tree, run unmodified over a multi-root residual forest: matching the two
/// virtual roots is always free, so `treedist(vrootedT1, vrootedT2) == forestdist(F1, F2)`
/// exactly, which is what callers actually want for a forest of unmatched siblings.
///
/// Real node `pre` therefore sits at index `pre` here (vroot owns index `0`); left-to-right
/// postorder is unaffected by the wrapping (vroot, having every other node as a descendant, is
/// simply the last postorder index, `size - 1`).
///
/// Field names and the formulas that derive them mirror Java APTED's `NodeIndexer` (indexNodes /
/// postTraversalIndexing), with one deviation: `kr_sum`/`rev_kr_sum`/`desc_sum` are computed via
/// closed-form bottom-up recurrences instead of replicating Java's single-pass-with-mutable-
/// "Tmp"-fields threading - both compute the exact same values, but the recurrence form doesn't
/// require smuggling a child's partial state through instance fields across recursive calls.
struct AptedIndexer {
    /// Number of nodes including the virtual root.
    size: usize,
    /// 0-based preorder index -> real node id, or `None` for the virtual root (index `0`).
    pre_to_node_id: Vec<Option<usize>>,
    /// 0-based preorder index -> 0-based preorder index of the parent, or `-1` for the root.
    parents: Vec<i64>,
    /// 0-based preorder index -> left-to-right ordered list of children's preorder indices.
    children: Vec<Vec<usize>>,
    /// 0-based preorder index -> size of the subtree rooted there (including the virtual root).
    sizes: Vec<usize>,
    /// 0-based preorder index -> 0-based left-to-right postorder index.
    pre_to_post_l: Vec<usize>,
    /// 0-based left-to-right postorder index -> 0-based preorder index.
    post_l_to_pre_l: Vec<usize>,
    /// 0-based left-to-right postorder index -> postorder index of the leftmost leaf descendant.
    post_l_to_lld: Vec<usize>,
    /// 0-based preorder index -> 0-based right-to-left preorder index.
    pre_to_pre_r: Vec<usize>,
    /// 0-based right-to-left preorder index -> 0-based (left-to-right) preorder index.
    pre_r_to_pre_l: Vec<usize>,
    /// `true` iff the node is its parent's first child (`false` for the root).
    node_type_l: Vec<bool>,
    /// `true` iff the node is its parent's last child (`false` for the root).
    node_type_r: Vec<bool>,
    /// Cost of `spfL` for the subtree rooted at this node [APTED paper, Section 5.2].
    kr_sum: Vec<u64>,
    /// Cost of `spfR` for the subtree rooted at this node.
    rev_kr_sum: Vec<u64>,
    /// Cost of `spfA` for the subtree rooted at this node.
    desc_sum: Vec<u64>,
    /// 0-based preorder index -> total delete cost of every node in its subtree.
    sum_del_cost: Vec<u64>,
    /// 0-based preorder index -> total insert cost of every node in its subtree.
    sum_ins_cost: Vec<u64>,
}

impl AptedIndexer {
    fn build(metadata: &ASTMetadata, root_ids: &[usize], node_map: &HashMap<usize, usize>) -> Self {
        fn visit(
            node_id: usize,
            parent_pre: usize,
            metadata: &ASTMetadata,
            node_map: &HashMap<usize, usize>,
            pre_to_node_id: &mut Vec<Option<usize>>,
            parents: &mut Vec<i64>,
            children: &mut Vec<Vec<usize>>,
        ) -> Option<usize> {
            if node_map.contains_key(&node_id) {
                return None;
            }
            let info = metadata.node_info.get(&node_id)?;
            let my_pre = pre_to_node_id.len();
            pre_to_node_id.push(Some(node_id));
            parents.push(parent_pre as i64);
            children.push(Vec::new());
            for &child_id in &info.children {
                if let Some(child_pre) = visit(
                    child_id,
                    my_pre,
                    metadata,
                    node_map,
                    pre_to_node_id,
                    parents,
                    children,
                ) {
                    children[my_pre].push(child_pre);
                }
            }
            Some(my_pre)
        }

        // Virtual root owns preorder index 0; its parent (`-1`) is never read since `gted`'s
        // walk-up always stops once it reaches the root of the subtree it started decomposing.
        let mut pre_to_node_id: Vec<Option<usize>> = vec![None];
        let mut parents: Vec<i64> = vec![-1];
        let mut children: Vec<Vec<usize>> = vec![Vec::new()];

        for &root_id in root_ids {
            if let Some(root_pre) = visit(
                root_id,
                0,
                metadata,
                node_map,
                &mut pre_to_node_id,
                &mut parents,
                &mut children,
            ) {
                children[0].push(root_pre);
            }
        }

        let size = pre_to_node_id.len();

        // Left-to-right postorder, computed iteratively (children finalized before parent).
        let mut pre_to_post_l = vec![0usize; size];
        let mut post_l_to_pre_l = vec![0usize; size];
        {
            let mut stack: Vec<(usize, bool)> = vec![(0, false)];
            let mut post = 0usize;
            while let Some((pre, visited)) = stack.pop() {
                if visited {
                    post_l_to_pre_l[post] = pre;
                    pre_to_post_l[pre] = post;
                    post += 1;
                } else {
                    stack.push((pre, true));
                    for &child in children[pre].iter().rev() {
                        stack.push((child, false));
                    }
                }
            }
        }

        // Bottom-up (postorder) pass: sizes, kr_sum/rev_kr_sum/desc_sum, node_type_l/r.
        let mut sizes = vec![1usize; size];
        let mut kr_sum = vec![0u64; size];
        let mut rev_kr_sum = vec![0u64; size];
        let mut desc_sum_total = vec![0u64; size]; // sum of sizes of every node in the subtree
        let mut node_type_l = vec![false; size];
        let mut node_type_r = vec![false; size];
        for &pre in &post_l_to_pre_l {
            let n = children[pre].len();
            let mut size_v = 1usize;
            let mut kr = 0u64;
            let mut rkr = 0u64;
            let mut dsum = 0u64;
            for (i, &child) in children[pre].iter().enumerate() {
                size_v += sizes[child];
                dsum += desc_sum_total[child];
                kr += if i == 0 {
                    kr_sum[child] - sizes[child] as u64
                } else {
                    kr_sum[child]
                };
                rkr += if i + 1 == n {
                    rev_kr_sum[child] - sizes[child] as u64
                } else {
                    rev_kr_sum[child]
                };
            }
            sizes[pre] = size_v;
            kr_sum[pre] = kr + size_v as u64;
            rev_kr_sum[pre] = rkr + size_v as u64;
            desc_sum_total[pre] = dsum + size_v as u64;
            if let Some(&first) = children[pre].first() {
                node_type_l[first] = true;
            }
            if let Some(&last) = children[pre].last() {
                node_type_r[last] = true;
            }
        }
        let desc_sum: Vec<u64> = (0..size)
            .map(|pre| {
                let sz = sizes[pre] as u64;
                sz * (sz + 3) / 2 - desc_sum_total[pre]
            })
            .collect();

        let mut post_l_to_lld = vec![0usize; size];
        for post in 0..size {
            let pre = post_l_to_pre_l[post];
            post_l_to_lld[post] = match children[pre].first() {
                None => post,
                Some(&first_child) => post_l_to_lld[pre_to_post_l[first_child]],
            };
        }

        let mut pre_to_pre_r = vec![0usize; size];
        let mut pre_r_to_pre_l = vec![0usize; size];
        for pre in 0..size {
            let pre_r = size - 1 - pre_to_post_l[pre];
            pre_to_pre_r[pre] = pre_r;
            pre_r_to_pre_l[pre_r] = pre;
        }

        let sum_del_cost = vec![0u64; size];
        let sum_ins_cost = vec![0u64; size];

        AptedIndexer {
            size,
            pre_to_node_id,
            parents,
            children,
            sizes,
            pre_to_post_l,
            post_l_to_pre_l,
            post_l_to_lld,
            pre_to_pre_r,
            pre_r_to_pre_l,
            node_type_l,
            node_type_r,
            kr_sum,
            rev_kr_sum,
            desc_sum,
            sum_del_cost,
            sum_ins_cost,
        }
    }

    /// Fills `sum_del_cost`/`sum_ins_cost` bottom-up. Split out of `build` because it needs the
    /// cost model (the virtual root and any pruned-away node contribute `0`).
    fn fill_subtree_costs(&mut self, meta: &ASTMetadata, cost_model: &UnitCostModel) {
        for &pre in &self.post_l_to_pre_l {
            let own_del = vdel(cost_model, vnode(self, meta, pre));
            let own_ins = vins(cost_model, vnode(self, meta, pre));
            let mut del = own_del;
            let mut ins = own_ins;
            for &child in &self.children[pre] {
                del += self.sum_del_cost[child];
                ins += self.sum_ins_cost[child];
            }
            self.sum_del_cost[pre] = del;
            self.sum_ins_cost[pre] = ins;
        }
    }

    /// Left-to-right preorder id of the leftmost leaf descendant of `pre` (itself if `pre` is a
    /// leaf).
    fn pre_l_to_lld(&self, pre: usize) -> usize {
        self.post_l_to_pre_l[self.post_l_to_lld[self.pre_to_post_l[pre]]]
    }
}

/// `node.del`/`.ins`/`.ren`, but `None` (the virtual root, or any node pruned because it's
/// already matched) always costs `0` - this is the whole trick that lets `gted` run on a
/// virtual-rooted *forest* and still compute exactly the forest-to-forest distance: matching the
/// two virtual roots is always free, so it's always at least as good as any alternative.
fn vnode<'a>(idx: &AptedIndexer, meta: &'a ASTMetadata, pre: usize) -> Option<&'a ASTNodeMetadata> {
    idx.pre_to_node_id[pre].map(|id| {
        meta.node_info
            .get(&id)
            .expect("indexed node must have metadata")
    })
}

fn vdel(cost_model: &UnitCostModel, node: Option<&ASTNodeMetadata>) -> u64 {
    node.map(|n| cost_model.del(n)).unwrap_or(0)
}

fn vins(cost_model: &UnitCostModel, node: Option<&ASTNodeMetadata>) -> u64 {
    node.map(|n| cost_model.ins(n)).unwrap_or(0)
}

/// The forced-everything-through-the-vroot-subtree-boundary single-path sweeps (`spf1`,
/// `apted_tree_edit_dist`) range over a subtree's *entire* preorder span, root included - correct
/// for a real subtree root, but the virtual root's own span is the entire forest, so these sweeps
/// do, legitimately, end up asking "what would it cost to rename this real node into the virtual
/// root" at every position along the boundary. Rather than special-casing those ranges to carve
/// the root out, give the (real, virtual) pairing a cost no real alternative could ever exceed:
/// the del/insert alternatives computed alongside it are always within the real forest's total
/// size, which is the only thing this sentinel needs to dominate.
const FORBIDDEN_PAIRING_COST: u64 = 1_000_000_000;

fn vren(
    cost_model: &UnitCostModel,
    a: Option<&ASTNodeMetadata>,
    b: Option<&ASTNodeMetadata>,
) -> u64 {
    match (a, b) {
        (Some(x), Some(y)) => cost_model.ren(x, y),
        (None, None) => 0,
        _ => FORBIDDEN_PAIRING_COST,
    }
}

/// Bundles everything `gted`/`spfL`/`spfR`/`spf1` need, in the fixed global "before/after"
/// orientation - `delta` is always written and read as `delta[before_pre][after_pre]`,
/// regardless of which side a given single-path function happens to be decomposing.
struct EngineCtx<'a> {
    before_idx: &'a AptedIndexer,
    after_idx: &'a AptedIndexer,
    before_meta: &'a ASTMetadata,
    after_meta: &'a ASTMetadata,
    cost_model: &'a UnitCostModel,
}

/// Direct port of APTED.java's `spf1`: closed-form tree edit distance when at least one of the
/// two subtrees is a single node, avoiding the overhead of the general single-path machinery.
fn spf1(ctx: &EngineCtx, root1: usize, root2: usize) -> u64 {
    let size1 = ctx.before_idx.sizes[root1];
    let size2 = ctx.after_idx.sizes[root2];
    let n1 = vnode(ctx.before_idx, ctx.before_meta, root1);
    let n2 = vnode(ctx.after_idx, ctx.after_meta, root2);

    if size1 == 1 && size2 == 1 {
        let max_cost = vdel(ctx.cost_model, n1) + vins(ctx.cost_model, n2);
        let ren_cost = vren(ctx.cost_model, n1, n2);
        return ren_cost.min(max_cost);
    }
    if size1 == 1 {
        let cost = ctx.after_idx.sum_ins_cost[root2];
        let max_cost = cost + vdel(ctx.cost_model, n1);
        let mut min_ren_minus_ins: i64 = cost as i64;
        for pre in root2..root2 + size2 {
            let n2i = vnode(ctx.after_idx, ctx.after_meta, pre);
            let delta_v = vren(ctx.cost_model, n1, n2i) as i64 - vins(ctx.cost_model, n2i) as i64;
            min_ren_minus_ins = min_ren_minus_ins.min(delta_v);
        }
        let cost = (cost as i64 + min_ren_minus_ins) as u64;
        return cost.min(max_cost);
    }
    // size2 == 1
    let cost = ctx.before_idx.sum_del_cost[root1];
    let max_cost = cost + vins(ctx.cost_model, n2);
    let mut min_ren_minus_del: i64 = cost as i64;
    for pre in root1..root1 + size1 {
        let n1i = vnode(ctx.before_idx, ctx.before_meta, pre);
        let delta_v = vren(ctx.cost_model, n1i, n2) as i64 - vdel(ctx.cost_model, n1i) as i64;
        min_ren_minus_del = min_ren_minus_del.min(delta_v);
    }
    let cost = (cost as i64 + min_ren_minus_del) as u64;
    cost.min(max_cost)
}

/// Direct port of APTED.java's `computeKeyRoots` (left-path variant): collects, into `keyroots`,
/// every node that is a keyroot of `subtree_root`'s *leftmost* path decomposition - i.e.
/// `subtree_root` itself, plus (recursively) every right-sibling encountered while walking up
/// from `path_id` (the leftmost leaf descendant of `subtree_root`) back to `subtree_root`.
fn compute_left_keyroots(
    idx: &AptedIndexer,
    subtree_root: usize,
    path_id: usize,
    keyroots: &mut Vec<usize>,
) {
    keyroots.push(subtree_root);
    let mut path_node = path_id;
    while path_node > subtree_root {
        let parent = idx.parents[path_node] as usize;
        for &child in &idx.children[parent] {
            if child != path_node {
                compute_left_keyroots(idx, child, idx.pre_l_to_lld(child), keyroots);
            }
        }
        path_node = parent;
    }
}

/// Direct port of APTED.java's `treeEditDist` (the core of `spfL`): fills `forestdist` with the
/// distances between every subforest pair spanning `[lld(path_subtree), path_subtree]` on the
/// path side against `[lld(other_subtree), other_subtree]` on the other side, and - as a side
/// effect, exactly like `forest_dist` above - writes `delta` for every aligned (tree-vs-tree)
/// position encountered along the way.
///
/// `path_is_before` says whether the path side is `before` (the "T1" of the global
/// before/after orientation) or `after`; this alone determines both the delete/insert cost
/// direction and which axis of `delta` each side's preorder id belongs on - see Java's
/// `treesSwapped` parameter, which this replaces (it served exactly the same purpose, just
/// re-derived here from the orientation that's already implied by `path_is_before`).
#[allow(clippy::too_many_arguments)]
fn apted_tree_edit_dist(
    ctx: &EngineCtx,
    delta: &mut DeltaTable,
    path_is_before: bool,
    path_subtree: usize,
    other_subtree: usize,
    forestdist: &mut ForestDist,
) {
    let (path_idx, other_idx) = if path_is_before {
        (ctx.before_idx, ctx.after_idx)
    } else {
        (ctx.after_idx, ctx.before_idx)
    };
    let (path_meta, other_meta) = if path_is_before {
        (ctx.before_meta, ctx.after_meta)
    } else {
        (ctx.after_meta, ctx.before_meta)
    };

    // `i`/`j`/`di`/`dj` are 1-based boundaries exactly like `forest_dist`'s (boundary `b`
    // corresponds to the node at 0-based postorder `b - 1`) - `forestdist`'s array index *is*
    // this same boundary value directly, so `lld_i`/`lld_j` (0-based postorder of the `lld`,
    // which numerically equals the boundary of the node *before* the `lld`) double as the base-
    // case index without any extra shift. Mirrors `forest_dist` precisely; only the cost
    // direction (`path_is_before`) and the indexer/cost-model plumbing differ.
    let i = path_idx.pre_to_post_l[path_subtree] + 1;
    let j = other_idx.pre_to_post_l[other_subtree] + 1;
    let lld_i = path_idx.post_l_to_lld[i - 1];
    let lld_j = other_idx.post_l_to_lld[j - 1];

    forestdist[(lld_i, lld_j)] = 0;
    for di in (lld_i + 1)..=i {
        let pre = path_idx.post_l_to_pre_l[di - 1];
        let cost = if path_is_before {
            vdel(ctx.cost_model, vnode(path_idx, path_meta, pre))
        } else {
            vins(ctx.cost_model, vnode(path_idx, path_meta, pre))
        };
        forestdist[(di, lld_j)] = forestdist[(di - 1, lld_j)] + cost;
    }
    for dj in (lld_j + 1)..=j {
        let pre = other_idx.post_l_to_pre_l[dj - 1];
        let cost = if path_is_before {
            vins(ctx.cost_model, vnode(other_idx, other_meta, pre))
        } else {
            vdel(ctx.cost_model, vnode(other_idx, other_meta, pre))
        };
        forestdist[(lld_i, dj)] = forestdist[(lld_i, dj - 1)] + cost;
    }

    for di in (lld_i + 1)..=i {
        let path_pre = path_idx.post_l_to_pre_l[di - 1];
        let path_node = vnode(path_idx, path_meta, path_pre);
        let path_lld = path_idx.post_l_to_lld[di - 1];
        let del_cost = if path_is_before {
            vdel(ctx.cost_model, path_node)
        } else {
            vins(ctx.cost_model, path_node)
        };
        for dj in (lld_j + 1)..=j {
            let other_pre = other_idx.post_l_to_pre_l[dj - 1];
            let other_node = vnode(other_idx, other_meta, other_pre);
            let other_lld = other_idx.post_l_to_lld[dj - 1];
            let ins_cost = if path_is_before {
                vins(ctx.cost_model, other_node)
            } else {
                vdel(ctx.cost_model, other_node)
            };
            let (before_node, after_node) = if path_is_before {
                (path_node, other_node)
            } else {
                (other_node, path_node)
            };
            let ren_cost = vren(ctx.cost_model, before_node, after_node);

            let da = forestdist[(di - 1, dj)] + del_cost;
            let db = forestdist[(di, dj - 1)] + ins_cost;
            let (before_pre, after_pre) = if path_is_before {
                (path_pre, other_pre)
            } else {
                (other_pre, path_pre)
            };

            let aligned = path_lld == lld_i && other_lld == lld_j;
            let dc = if aligned {
                let v = forestdist[(di - 1, dj - 1)];
                delta.set(before_pre, after_pre, v);
                v + ren_cost
            } else {
                forestdist[(path_lld, other_lld)] + delta.get(before_pre, after_pre) + ren_cost
            };

            forestdist[(di, dj)] = da.min(db).min(dc);
        }
    }
}

/// Direct port of APTED.java's `spfL`: the path side (`path_subtree`, already reduced to a
/// single remaining path by `gted`'s caller) against the *entire* other side, decomposed via its
/// own left-path keyroots in one combined sweep - this single combined sweep across all of
/// `other_subtree`'s keyroots, rather than one call per (keyroot, keyroot) pair, is what makes
/// APTED asymptotically cheaper than the classic Zhang-Shasha keyroot loop.
fn spf_l(
    ctx: &EngineCtx,
    delta: &mut DeltaTable,
    path_is_before: bool,
    path_subtree: usize,
    other_subtree: usize,
) -> u64 {
    let (path_idx, other_idx) = if path_is_before {
        (ctx.before_idx, ctx.after_idx)
    } else {
        (ctx.after_idx, ctx.before_idx)
    };

    let mut keyroots = Vec::new();
    if other_subtree == 0 {
        // The virtual root's children are the *forest's own roots* - each is its own keyroot
        // regardless of left-sibling status (mirroring `PostorderIndexer`'s `root_pres`), since
        // there is no real ancestor whose own leftmost path could ever cover more than one of
        // them. Treating the virtual root itself as an ordinary node here would silently absorb
        // its first child into a path that runs all the way down to the forest's overall
        // leftmost leaf - that child would then never get its own aligned (tree-vs-tree)
        // boundary, exactly the boundary `compute_edit_mapping`'s backtrace later depends on.
        for &root in &other_idx.children[0] {
            compute_left_keyroots(other_idx, root, other_idx.pre_l_to_lld(root), &mut keyroots);
        }
    } else {
        compute_left_keyroots(
            other_idx,
            other_subtree,
            other_idx.pre_l_to_lld(other_subtree),
            &mut keyroots,
        );
    }
    keyroots.sort_by_key(|&pre| other_idx.pre_to_post_l[pre]);

    // Sized and indexed by the same 1-based-boundary convention as `apted_tree_edit_dist`
    // (absolute, not relative to any one call's own `lld`), and reused across the whole keyroot
    // sweep below - see the comment there for why a relative scheme would be unsound here.
    let mut forestdist = ForestDist::new(path_idx.size + 1, other_idx.size + 1);
    for &kr in &keyroots {
        apted_tree_edit_dist(
            ctx,
            delta,
            path_is_before,
            path_subtree,
            kr,
            &mut forestdist,
        );
    }
    forestdist[(
        path_idx.pre_to_post_l[path_subtree] + 1,
        other_idx.pre_to_post_l[other_subtree] + 1,
    )]
}

/// Recursive tree-decomposition driver, forcing the "always decompose `before`'s leftmost path"
/// strategy (i.e. always `spfL`, exactly like the classic Zhang-Shasha keyroot loop above always
/// does) rather than APTED's real per-subtree-optimal strategy choice
/// (`computeOptStrategy_postL/postR`, not yet ported). Forcing the strategy this way means this
/// is *not yet* the asymptotic improvement the Java reference provides - it's a stepping stone
/// that validates the indexing/virtual-root plumbing against the Zhang-Shasha oracle before the
/// real strategy (and `spfR`/`spfA`) are layered on top.
///
/// Direct port of APTED.java's `gted`, restricted to the `strategyPathType == LEFT` branch on
/// the `before` side.
fn gted_forced_left(ctx: &EngineCtx, delta: &mut DeltaTable, current1: usize, current2: usize) -> u64 {
    // Deliberately *not* Java's `if subtreeSize1 == 1 || subtreeSize2 == 1: return spf1(...)`
    // shortcut: `current2` never moves off the virtual root in this forced-left milestone (real
    // APTED's strategy normally narrows both sides in lockstep, which is what makes that
    // shortcut safe there), so a `spf1`-only leaf comparison would only ever get written against
    // the *whole* other forest, never against the individual after-keyroots an ancestor's own
    // sweep later needs to look up. `spf_l` below already handles a single-node `path_subtree`
    // correctly - its own outermost row trivially satisfies the "aligned" check - so every node,
    // leaf or not, goes through the same per-after-keyroot sweep.
    let mut current_path_node = ctx.before_idx.pre_l_to_lld(current1);
    loop {
        let parent = ctx.before_idx.parents[current_path_node];
        if parent < 0 || (parent as usize) < current1 {
            break;
        }
        let parent = parent as usize;
        for &child in &ctx.before_idx.children[parent] {
            if child != current_path_node {
                gted_forced_left(ctx, delta, child, current2);
            }
        }
        current_path_node = parent;
    }

    spf_l(ctx, delta, true, current1, current2)
}

/// Computes the tree edit distance and populates `delta` for a forest pair, using the real
/// APTED engine instead of classic Zhang-Shasha keyroot decomposition. Each side's forest is
/// wrapped under a virtual root (see `AptedIndexer`) so the single-rooted APTED recursion can
/// run unmodified; the resulting virtual-space `delta` is then translated back into a `DeltaTable`
/// indexed by the real (non-virtual) preorder ids that `compute_edit_mapping` expects.
fn compute_delta(
    before: &PostorderIndexer,
    after: &PostorderIndexer,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    cost_model: &UnitCostModel,
    before_root_ids: &[usize],
    after_root_ids: &[usize],
    before_node_map: &HashMap<usize, usize>,
    after_node_map: &HashMap<usize, usize>,
) -> DeltaTable {
    let mut real_delta = DeltaTable::new(before.size.max(1), after.size.max(1));
    if before.size == 0 || after.size == 0 {
        return real_delta;
    }

    let mut before_idx = AptedIndexer::build(before_meta, before_root_ids, before_node_map);
    let mut after_idx = AptedIndexer::build(after_meta, after_root_ids, after_node_map);
    before_idx.fill_subtree_costs(before_meta, cost_model);
    after_idx.fill_subtree_costs(after_meta, cost_model);

    let ctx = EngineCtx {
        before_idx: &before_idx,
        after_idx: &after_idx,
        before_meta,
        after_meta,
        cost_model,
    };
    let mut virtual_delta = DeltaTable::new(before_idx.size, after_idx.size);
    // Drive `gted_forced_left` once per top-level real root (the virtual root's children) -
    // exactly the `other_subtree == 0` fix in `spf_l` above, mirrored on the before/T1 axis:
    // starting from the virtual root itself would walk its leftmost path all the way down to
    // the forest's overall leftmost leaf, silently absorbing the first real root into that path
    // instead of giving it (and every sibling root) its own aligned boundary.
    for &before_root in &before_idx.children[0] {
        gted_forced_left(&ctx, &mut virtual_delta, before_root, 0);
    }

    // Translate virtual-space (vroot-inclusive) preorder ids back to real preorder ids: vroot
    // sits at virtual index `0`, real node `pre` sits at virtual index `pre + 1`, on both sides.
    for before_pre in 0..before.size {
        for after_pre in 0..after.size {
            let v = virtual_delta.get(before_pre + 1, after_pre + 1);
            if v != 0 {
                real_delta.set(before_pre, after_pre, v);
            }
        }
    }

    real_delta
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

    let mut delta = compute_delta(
        &before_idx,
        &after_idx,
        before_meta,
        after_meta,
        cost_model,
        &before_root_ids,
        &after_root_ids,
        &diff.before_node_map,
        &diff.after_node_map,
    );
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
        let cost_model = UnitCostModel;

        let before_idx = PostorderIndexer::build(before_meta, before_root_ids, before_node_map);
        let after_idx = PostorderIndexer::build(after_meta, after_root_ids, after_node_map);

        let mut oracle_delta = compute_delta_zhang_shasha(
            &before_idx,
            &after_idx,
            before_meta,
            after_meta,
            &cost_model,
        );
        let oracle_decisions = compute_edit_mapping(
            &before_idx,
            &after_idx,
            before_meta,
            after_meta,
            &cost_model,
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
            &mut new_delta,
        );
        let new_cost = mapping_total_cost(&new_decisions, before_meta, after_meta, &cost_model);

        assert_eq!(
            new_cost, oracle_cost,
            "new engine cost {new_cost} != oracle cost {oracle_cost}\nbefore_roots={before_root_ids:?} after_roots={after_root_ids:?}"
        );
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
                    rng, next_id, depth + 1, max_depth, kinds, texts, nodes,
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
        let cost_model = UnitCostModel;
        let empty_map = HashMap::new();
        let before_idx = PostorderIndexer::build(&before, &[0], &empty_map);
        let after_idx = PostorderIndexer::build(&after, &[7], &empty_map);

        let mut oracle_delta =
            compute_delta_zhang_shasha(&before_idx, &after_idx, &before, &after, &cost_model);
        let oracle_decisions = compute_edit_mapping(
            &before_idx,
            &after_idx,
            &before,
            &after,
            &cost_model,
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
    }

    #[test]
    fn test_apted_engine_matches_oracle_fuzz() {
        let kinds = ["a", "b", "c"];
        let texts = ["x", "y", "z"];
        for seed in 0..3000u64 {
            let mut rng = Rng(seed.wrapping_mul(2685821657736338717).wrapping_add(1));
            let mut before_nodes = Vec::new();
            let mut next_id = 0usize;
            let before_root = gen_random_tree(&mut rng, &mut next_id, 0, 4, &kinds, &texts, &mut before_nodes);
            let mut after_nodes = Vec::new();
            let after_root =
                gen_random_tree(&mut rng, &mut next_id, 0, 4, &kinds, &texts, &mut after_nodes);

            let before_meta = meta_from_owned(&before_nodes);
            let after_meta = meta_from_owned(&after_nodes);

            let result = std::panic::catch_unwind(|| {
                assert_distance_matches_oracle(&before_meta, &after_meta, &[before_root], &[after_root]);
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
