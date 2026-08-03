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

use crate::code::{ASTMetadata, ASTNodeMetadata};

use super::common::{
    ContainmentCtx, DeltaTable, ForestDist, Grid, PostorderIndexer, UnitCostModel,
};

/// Whether `APTED_DEBUG` is set, read once and cached - `gted`/`spf_a` check this at every debug
/// write point on the hottest part of the algorithm (`gted` recurses over every node in the tree
/// decomposition), so re-querying the environment there on every call would be pure per-call
/// overhead for a flag that can't change mid-run.
fn apted_debug() -> bool {
    static DEBUG: std::sync::LazyLock<bool> =
        std::sync::LazyLock::new(|| std::env::var("APTED_DEBUG").is_ok());
    *DEBUG
}

/// `strategy[(pre_v, pre_w)]` from `computeOptStrategy_postL`/`_postR`: a *signed* encoded path
/// id (not a distance), separate from `DeltaTable` rather than overloading one buffer for both
/// the way Java does - Java's `delta`/`strategy` are the same `float[][]`, reused in place once
/// `gted` starts consuming a cell (it never needs the strategy value again after routing).
/// Keeping them apart avoids relying on that consumption order, at the cost of one extra buffer.
pub(crate) struct StrategyTable {
    grid: Grid<i64>,
}

impl StrategyTable {
    pub(crate) fn new(rows: usize, cols: usize) -> Self {
        StrategyTable {
            grid: Grid::new(rows, cols, 0i64),
        }
    }

    pub(crate) fn get(&self, pre_v: usize, pre_w: usize) -> i64 {
        self.grid[(pre_v, pre_w)]
    }

    pub(crate) fn set(&mut self, pre_v: usize, pre_w: usize, value: i64) {
        self.grid[(pre_v, pre_w)] = value;
    }
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
pub(crate) struct AptedIndexer {
    /// Number of nodes including the virtual root.
    pub(crate) size: usize,
    /// 0-based preorder index -> real node id, or `None` for the virtual root (index `0`).
    pub(crate) pre_to_node_id: Vec<Option<usize>>,
    /// 0-based preorder index -> 0-based preorder index of the parent, or `-1` for the root.
    pub(crate) parents: Vec<i64>,
    /// 0-based preorder index -> left-to-right ordered list of children's preorder indices.
    pub(crate) children: Vec<Vec<usize>>,
    /// 0-based preorder index -> size of the subtree rooted there (including the virtual root).
    pub(crate) sizes: Vec<usize>,
    /// 0-based preorder index -> 0-based left-to-right postorder index.
    pub(crate) pre_to_post_l: Vec<usize>,
    /// 0-based left-to-right postorder index -> 0-based preorder index.
    pub(crate) post_l_to_pre_l: Vec<usize>,
    /// 0-based left-to-right postorder index -> postorder index of the leftmost leaf descendant.
    pub(crate) post_l_to_lld: Vec<usize>,
    /// 0-based preorder index -> 0-based right-to-left preorder index.
    pub(crate) pre_to_pre_r: Vec<usize>,
    /// 0-based right-to-left preorder index -> 0-based (left-to-right) preorder index.
    pub(crate) pre_r_to_pre_l: Vec<usize>,
    /// 0-based right-to-left postorder index -> right-to-left postorder index of the rightmost
    /// leaf descendant.
    pub(crate) post_r_to_rld: Vec<usize>,
    /// 0-based preorder index -> preorder index of the nearest leaf strictly to its left, or
    /// `-1` if none.
    pub(crate) pre_to_ln: Vec<i64>,
    /// 0-based right-to-left preorder index -> right-to-left preorder index of the nearest leaf
    /// strictly to its right (in left-to-right terms), or `-1` if none.
    pub(crate) pre_r_to_ln: Vec<i64>,
    /// `true` iff the node is its parent's first child (`false` for the root).
    pub(crate) node_type_l: Vec<bool>,
    /// `true` iff the node is its parent's last child (`false` for the root).
    pub(crate) node_type_r: Vec<bool>,
    /// Cost of `spfL` for the subtree rooted at this node [APTED paper, Section 5.2].
    pub(crate) kr_sum: Vec<u64>,
    /// Cost of `spfR` for the subtree rooted at this node.
    pub(crate) rev_kr_sum: Vec<u64>,
    /// Cost of `spfA` for the subtree rooted at this node.
    pub(crate) desc_sum: Vec<u64>,
    /// 0-based preorder index -> total delete cost of every node in its subtree.
    pub(crate) sum_del_cost: Vec<u64>,
    /// 0-based preorder index -> total insert cost of every node in its subtree.
    pub(crate) sum_ins_cost: Vec<u64>,
    /// Count of leaf nodes that are their parent's first (leftmost) child [2, Section 5.3].
    pub(crate) lchl: usize,
    /// Count of leaf nodes that are their parent's last (rightmost) child [2, Section 5.3].
    pub(crate) rchl: usize,
}

impl AptedIndexer {
    pub(crate) fn build(
        metadata: &ASTMetadata,
        root_ids: &[usize],
        node_map: &rustc_hash::FxHashMap<usize, usize>,
    ) -> Self {
        fn visit(
            node_id: usize,
            parent_pre: usize,
            metadata: &ASTMetadata,
            node_map: &rustc_hash::FxHashMap<usize, usize>,
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

        // Right-to-left postorder index of a node's rightmost leaf descendant. Right-to-left
        // postorder of preorder index `pre` is the trivial `size - 1 - pre` (see
        // `pre_to_post_r`/`post_r_to_pre_l`), so processing preorder indices high-to-low visits
        // nodes in ascending right-to-left-postorder order - i.e. children (always a higher
        // preorder than their parent) are finalized before the parent that needs them.
        let mut post_r_to_rld = vec![0usize; size];
        for pre in (0..size).rev() {
            let post_r = size - 1 - pre;
            post_r_to_rld[post_r] = match children[pre].last() {
                None => post_r,
                Some(&last_child) => post_r_to_rld[size - 1 - last_child],
            };
        }

        // Nearest leaf strictly to the left (in preorder)/right (in right-to-left preorder),
        // `-1` if none - needed by `spf_a`'s `updateFnArray` to seed its "next forest member"
        // linked list. A single forward scan each, mirroring Java's `postTraversalIndexing`.
        let mut pre_to_ln = vec![-1i64; size];
        {
            let mut current_leaf: i64 = -1;
            for pre in 0..size {
                pre_to_ln[pre] = current_leaf;
                if sizes[pre] == 1 {
                    current_leaf = pre as i64;
                }
            }
        }
        let mut pre_r_to_ln = vec![-1i64; size];
        {
            let mut current_leaf: i64 = -1;
            for pre_r in 0..size {
                pre_r_to_ln[pre_r] = current_leaf;
                if sizes[pre_r_to_pre_l[pre_r]] == 1 {
                    current_leaf = pre_r as i64;
                }
            }
        }

        let sum_del_cost = vec![0u64; size];
        let sum_ins_cost = vec![0u64; size];

        // `lchl`/`rchl` [2, Section 5.3]: count of leaf nodes that are their parent's first/last
        // child, used by `compute_delta` to pick whichever of postL/postR's preorder direction is
        // cheaper for this tree's shape.
        let mut lchl = 0usize;
        let mut rchl = 0usize;
        for pre in 0..size {
            if sizes[pre] == 1 {
                if node_type_l[pre] {
                    lchl += 1;
                }
                if node_type_r[pre] {
                    rchl += 1;
                }
            }
        }

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
            post_r_to_rld,
            pre_to_ln,
            pre_r_to_ln,
            node_type_l,
            node_type_r,
            kr_sum,
            rev_kr_sum,
            desc_sum,
            sum_del_cost,
            sum_ins_cost,
            lchl,
            rchl,
        }
    }

    /// Fills `sum_del_cost`/`sum_ins_cost` bottom-up. Split out of `build` because it needs the
    /// cost model (the virtual root and any pruned-away node contribute `0`).
    pub(crate) fn fill_subtree_costs(&mut self, meta: &ASTMetadata, cost_model: &UnitCostModel) {
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
    pub(crate) fn pre_l_to_lld(&self, pre: usize) -> usize {
        self.post_l_to_pre_l[self.post_l_to_lld[self.pre_to_post_l[pre]]]
    }

    /// 0-based right-to-left postorder index of `pre`. Trivially `size - 1 - pre`: the
    /// right-to-left-postorder rank of any node equals `size - 1` minus its left-to-right-
    /// preorder rank, for any tree shape (mirrors Java's `preL_to_postR`).
    pub(crate) fn pre_to_post_r(&self, pre: usize) -> usize {
        self.size - 1 - pre
    }

    /// Inverse of `pre_to_post_r` - also self-inverse, by the same identity.
    pub(crate) fn post_r_to_pre_l(&self, post_r: usize) -> usize {
        self.size - 1 - post_r
    }

    /// Left-to-right preorder id of the rightmost leaf descendant of `pre` (itself if `pre` is a
    /// leaf).
    pub(crate) fn pre_l_to_rld(&self, pre: usize) -> usize {
        self.post_r_to_pre_l(self.post_r_to_rld[self.pre_to_post_r(pre)])
    }
}

/// `node.del`/`.ins`/`.ren`, but `None` (the virtual root, or any node pruned because it's
/// already matched) always costs `0` - this is the whole trick that lets `gted` run on a
/// virtual-rooted *forest* and still compute exactly the forest-to-forest distance: matching the
/// two virtual roots is always free, so it's always at least as good as any alternative.
pub(crate) fn vnode<'a>(
    idx: &AptedIndexer,
    meta: &'a ASTMetadata,
    pre: usize,
) -> Option<&'a ASTNodeMetadata> {
    idx.pre_to_node_id[pre].map(|id| {
        meta.node_info
            .get(&id)
            .expect("indexed node must have metadata")
    })
}

pub(crate) fn vdel(cost_model: &UnitCostModel, node: Option<&ASTNodeMetadata>) -> u64 {
    node.map(|n| cost_model.del(n)).unwrap_or(0)
}

pub(crate) fn vins(cost_model: &UnitCostModel, node: Option<&ASTNodeMetadata>) -> u64 {
    node.map(|n| cost_model.ins(n)).unwrap_or(0)
}

pub(crate) const FORBIDDEN_PAIRING_COST: u64 = 1_000_000_000;

pub(crate) fn vren(
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
pub(crate) struct EngineCtx<'a> {
    pub(crate) before_idx: &'a AptedIndexer,
    pub(crate) after_idx: &'a AptedIndexer,
    pub(crate) before_meta: &'a ASTMetadata,
    pub(crate) after_meta: &'a ASTMetadata,
    pub(crate) cost_model: &'a UnitCostModel,
    /// Mirrors `forest_dist`'s own `containment` parameter (Zhang-Shasha side, common.rs) - see
    /// `vren_adjusted`, the single place every `vren` call site here routes through so a
    /// "hollowed out" ancestor can't freely rename onto a node that would contradict where its
    /// pruned descendant already landed. `None` whenever this forest has nothing pruned, in which
    /// case `vren_adjusted` is a pure passthrough.
    pub(crate) containment: Option<&'a ContainmentCtx<'a>>,
}

impl<'a> EngineCtx<'a> {
    /// Resolves which indexer/metadata pair is the "path" side and which is "other", given which
    /// global side (`before`/`after`) the path currently lives on. Every single-path function
    /// (`spf_a`, `apted_tree_edit_dist`, `spf_path`) starts with exactly this lookup - pulled out
    /// once here instead of duplicating the same `if path_is_before {...} else {...}` at each of
    /// their 5 call sites.
    fn sides(
        &self,
        path_is_before: bool,
    ) -> (
        &'a AptedIndexer,
        &'a AptedIndexer,
        &'a ASTMetadata,
        &'a ASTMetadata,
    ) {
        if path_is_before {
            (
                self.before_idx,
                self.after_idx,
                self.before_meta,
                self.after_meta,
            )
        } else {
            (
                self.after_idx,
                self.before_idx,
                self.after_meta,
                self.before_meta,
            )
        }
    }
}

/// Applies `ctx.containment`'s `adjust()` to a `vren`-computed `base` cost for the real-node pair
/// at virtual preorder ids `(before_pre, after_pre)` in the fixed global before/after orientation
/// - the Apted-engine equivalent of the `ctx.adjust(before_id, after_id, cost_ren)` call in
///   `forest_dist` (common.rs). A `None` id (the virtual root, or any boundary read that lands on
///   it) is never itself pruned, so it's always left as a no-op - `vren` already prices a lone
///   `None` side as `FORBIDDEN_PAIRING_COST` regardless.
pub(crate) fn vren_adjusted(ctx: &EngineCtx, before_pre: i64, after_pre: i64, base: u64) -> u64 {
    let Some(containment) = ctx.containment else {
        return base;
    };
    let before_id = ctx.before_idx.pre_to_node_id[before_pre as usize];
    let after_id = ctx.after_idx.pre_to_node_id[after_pre as usize];
    match (before_id, after_id) {
        (Some(b), Some(a)) => containment.adjust(b, a, base),
        _ => base,
    }
}

/// Flat, `i64`-valued 2D matrix - backs `spf_a`'s `s`/`t` tables (the role Java's `float[][]`
/// plays there).
pub(crate) type Mat = Grid<i64>;

/// Direct port of APTED.java's `spfA` (the general "inner path" single-path function -
/// Algorithm 3 in the APTED paper), **specialized to `pathType == INNER`**: `gted` only ever
/// calls this once it has already routed `pathType == LEFT`/`RIGHT` to `spf_path` directly, so
/// Java's `pathType == 0`/`pathType == 1` branches (handling this function as a
/// hypothetically general LEFT/RIGHT/INNER entry point) are dead code from that call site and
/// are omitted; every conditional that depended on them is simplified accordingly (e.g. the
/// "deal with nodes to the left of the path" guard becomes plain `leftPart`).
///
/// Variable names mirror Java's as closely as possible (`lF`/`rF` range over `path_idx`'s
/// subtree, `lG`/`rG` over `other_idx`'s) to keep this checkable line-by-line against the
/// original; `treesSwapped` is replaced by `path_is_before` throughout, same as `spf_path`.
/// Costs are `i64` (never negative in practice for a metric cost model, but several
/// intermediate `sp3` terms are differences, so `i64` avoids an underflow panic `u64` would risk
/// on the way to a non-negative result) and cast to `u64` only at the `DeltaTable`/return
/// boundary.
fn update_fn_array(fna: &mut [i64], ln_for_node: i64, node: i64, current_subtree_pre_l: i64) {
    let last = fna.len() - 1;
    if ln_for_node >= current_subtree_pre_l {
        fna[node as usize] = fna[ln_for_node as usize];
        fna[ln_for_node as usize] = node;
    } else {
        fna[node as usize] = fna[last];
        fna[last] = node;
    }
}

fn update_ft_array(fna: &[i64], fta: &mut [i64], ln_for_node: i64, node: i64) {
    fta[node as usize] = ln_for_node;
    if fna[node as usize] > -1 {
        fta[fna[node as usize] as usize] = node;
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn spf_a(
    ctx: &EngineCtx,
    delta: &mut DeltaTable,
    path_is_before: bool,
    path_subtree: usize,
    other_subtree: usize,
    path_id: usize,
) -> u64 {
    let (path_idx, other_idx, path_meta, other_meta) = ctx.sides(path_is_before);
    // `(before, after)`, in that order - the fixed global orientation `delta` is always read and
    // written in, regardless of which side `path_idx`/`other_idx` happen to be (see `spf_path`).
    let delta_order = |path_pre: i64, other_pre: i64| -> (usize, usize) {
        if path_is_before {
            (path_pre as usize, other_pre as usize)
        } else {
            (other_pre as usize, path_pre as usize)
        }
    };

    let path_del_cost = |pre: i64| -> i64 {
        let n = vnode(path_idx, path_meta, pre as usize);
        (if path_is_before {
            vdel(ctx.cost_model, n)
        } else {
            vins(ctx.cost_model, n)
        }) as i64
    };
    let other_ins_cost = |pre: i64| -> i64 {
        let n = vnode(other_idx, other_meta, pre as usize);
        (if path_is_before {
            vins(ctx.cost_model, n)
        } else {
            vdel(ctx.cost_model, n)
        }) as i64
    };
    let ren_cost = |path_pre: i64, other_pre: i64| -> i64 {
        let pn = vnode(path_idx, path_meta, path_pre as usize);
        let on = vnode(other_idx, other_meta, other_pre as usize);
        let (b, a) = if path_is_before { (pn, on) } else { (on, pn) };
        let base = vren(ctx.cost_model, b, a);
        let (before_pre, after_pre) = delta_order(path_pre, other_pre);
        vren_adjusted(ctx, before_pre as i64, after_pre as i64, base) as i64
    };
    let path_subtree_del_cost = |pre: i64| -> i64 {
        (if path_is_before {
            path_idx.sum_del_cost[pre as usize]
        } else {
            path_idx.sum_ins_cost[pre as usize]
        }) as i64
    };
    let other_subtree_ins_cost = |pre: i64| -> i64 {
        (if path_is_before {
            other_idx.sum_ins_cost[pre as usize]
        } else {
            other_idx.sum_del_cost[pre as usize]
        }) as i64
    };

    let current_subtree_pre_l1 = path_subtree as i64;
    let current_subtree_pre_l2 = other_subtree as i64;
    let subtree_size1 = path_idx.sizes[path_subtree] as i64;
    let subtree_size2 = other_idx.sizes[other_subtree] as i64;

    let mut t = Mat::new(subtree_size2 as usize + 1, subtree_size2 as usize + 1, 0);
    let mut s = Mat::new(subtree_size1 as usize + 1, subtree_size2 as usize + 1, 0);
    let mut min_cost: i64 = -1;
    let max_size = (path_idx.size.max(other_idx.size)) as i64;
    let mut fna = vec![-1i64; max_size as usize + 1];
    let mut fta = vec![-1i64; max_size as usize + 1];
    let mut q = vec![0i64; max_size as usize + 1];
    // Incrementally summed forest size/cost - F is `path_idx`'s side, G is `other_idx`'s.
    // The G-side pair is (re)computed from scratch inside the loops before every read, so it
    // gets no initial value.
    let mut current_forest_size1: i64 = 0;
    let mut current_forest_size2: i64;
    let mut current_forest_cost1: i64 = 0;
    let mut current_forest_cost2: i64;

    let mut start_path_node: i64 = -1;
    let mut end_path_node = path_id as i64;
    let mut it1_pre_l_off;
    let it2_pre_l_off = current_subtree_pre_l2;
    let mut it1_pre_r_off;
    let it2_pre_r_off = other_idx.pre_to_pre_r[current_subtree_pre_l2 as usize] as i64;

    // Loop A [1, Algorithm 3] - walk up the path.
    while end_path_node >= current_subtree_pre_l1 {
        it1_pre_l_off = end_path_node;
        it1_pre_r_off = path_idx.pre_to_pre_r[end_path_node as usize] as i64;
        let mut r_f_last: i64 = -1;
        let mut l_f_last: i64;
        let end_path_node_in_pre_r = path_idx.pre_to_pre_r[end_path_node as usize] as i64;
        let start_path_node_in_pre_r = if start_path_node == -1 {
            i64::MAX / 4
        } else {
            path_idx.pre_to_pre_r[start_path_node as usize] as i64
        };
        let parent_of_end_path_node = path_idx.parents[end_path_node as usize];
        let parent_of_end_path_node_in_pre_r = if parent_of_end_path_node == -1 {
            i64::MAX / 4
        } else {
            path_idx.pre_to_pre_r[parent_of_end_path_node as usize] as i64
        };

        let left_part = start_path_node - end_path_node > 1;
        let right_part =
            start_path_node >= 0 && start_path_node_in_pre_r - end_path_node_in_pre_r > 1;

        // Deal with nodes to the left of the path. Java: `pathType == 1 || pathType == 2 &&
        // leftPart`; simplified to `leftPart` per this function's INNER-only specialization.
        if left_part {
            let (r_f_first, l_f_first);
            if start_path_node == -1 {
                r_f_first = end_path_node_in_pre_r;
                l_f_first = end_path_node;
            } else {
                r_f_first = start_path_node_in_pre_r;
                l_f_first = start_path_node - 1;
            }
            if !right_part {
                r_f_last = end_path_node_in_pre_r;
            }
            let r_g_last = other_idx.pre_to_pre_r[current_subtree_pre_l2 as usize] as i64;
            let r_g_first = r_g_last + subtree_size2 - 1;
            l_f_last = if right_part {
                end_path_node + 1
            } else {
                end_path_node
            };
            let fna_last = fna.len() - 1;
            fna[fna_last] = -1;
            for i in current_subtree_pre_l2..(current_subtree_pre_l2 + subtree_size2) {
                fna[i as usize] = -1;
                fta[i as usize] = -1;
            }
            // Store the current size and cost of forest in F.
            let tmp_forest_size1 = current_forest_size1;
            let tmp_forest_cost1 = current_forest_cost1;
            // Loop B [1, Algorithm 3] - for all nodes in G (right-hand input tree).
            let mut r_g = r_g_first;
            while r_g >= r_g_last {
                let l_g_first = other_idx.pre_r_to_pre_l[r_g as usize] as i64;
                let r_g_in_pre_l = l_g_first;
                let r_g_minus1_in_pre_l =
                    if r_g <= other_idx.pre_to_pre_r[current_subtree_pre_l2 as usize] as i64 {
                        i64::MAX / 4
                    } else {
                        other_idx.pre_r_to_pre_l[(r_g - 1) as usize] as i64
                    };
                let parent_of_r_g_in_pre_l = other_idx.parents[r_g_in_pre_l as usize];
                // Decides on the last lG node for Loop D - INNER-only, so always the `else`
                // branch of Java's `if (pathType == 1)`.
                let l_g_last = if l_g_first == current_subtree_pre_l2 {
                    l_g_first
                } else {
                    current_subtree_pre_l2 + 1
                };
                update_fn_array(
                    &mut fna,
                    other_idx.pre_to_ln[l_g_first as usize],
                    l_g_first,
                    current_subtree_pre_l2,
                );
                update_ft_array(
                    &fna,
                    &mut fta,
                    other_idx.pre_to_ln[l_g_first as usize],
                    l_g_first,
                );
                let mut r_f = r_f_first;
                current_forest_size1 = tmp_forest_size1;
                current_forest_cost1 = tmp_forest_cost1;
                // Loop C [1, Algorithm 3] - for all nodes to the left of the path node.
                let mut l_f = l_f_first;
                while l_f >= l_f_last {
                    if l_f == l_f_last && !right_part {
                        r_f = r_f_last;
                    }
                    let l_f_node = l_f;
                    current_forest_size1 += 1;
                    current_forest_cost1 += path_del_cost(l_f_node);
                    current_forest_size2 = other_idx.sizes[l_g_first as usize] as i64;
                    current_forest_cost2 = other_subtree_ins_cost(l_g_first);
                    let l_f_in_pre_r = path_idx.pre_to_pre_r[l_f as usize] as i64;
                    let f_forest_is_tree = l_f_in_pre_r == r_f;
                    let l_f_subtree_size = path_idx.sizes[l_f as usize] as i64;
                    let l_f_is_consecutive_node_of_current_path_node = start_path_node - l_f == 1;
                    let l_f_is_left_sibling_of_current_path_node =
                        l_f + l_f_subtree_size == start_path_node;
                    let sp1s_row = (l_f + 1) - it1_pre_l_off;
                    let sp2s_row = l_f - it1_pre_l_off;
                    let mut sp3s_row = 0i64;
                    let swrite_row = l_f - it1_pre_l_off;
                    let mut sp1source = 1u8;
                    let mut sp3source = 1u8;
                    let mut sp3: i64;
                    if f_forest_is_tree {
                        if l_f_subtree_size == 1 {
                            sp1source = 3;
                        } else if l_f_is_consecutive_node_of_current_path_node {
                            sp1source = 2;
                        }
                        sp3 = 0;
                        sp3source = 2;
                    } else {
                        if l_f_is_consecutive_node_of_current_path_node {
                            sp1source = 2;
                        }
                        sp3 = current_forest_cost1 - path_subtree_del_cost(l_f);
                        if l_f_is_left_sibling_of_current_path_node {
                            sp3source = 3;
                        }
                    }
                    if sp3source == 1 {
                        sp3s_row = (l_f + l_f_subtree_size) - it1_pre_l_off;
                    }

                    let mut l_g = l_g_first;
                    let mut sp1 = match sp1source {
                        1 => s[(sp1s_row, l_g - it2_pre_l_off)],
                        2 => t[(l_g - it2_pre_l_off, r_g - it2_pre_r_off)],
                        _ => current_forest_cost2,
                    };
                    sp1 += path_del_cost(l_f_node);
                    min_cost = sp1;
                    let mut sp2 = if current_forest_size2 == 1 {
                        current_forest_cost1
                    } else {
                        q[l_f as usize]
                    };
                    sp2 += other_ins_cost(l_g);
                    if sp2 < min_cost {
                        min_cost = sp2;
                    }
                    if sp3 < min_cost {
                        let (b, a) = delta_order(l_f_node, l_g);
                        sp3 += delta.get(b, a) as i64;
                        if sp3 < min_cost {
                            sp3 += ren_cost(l_f_node, l_g);
                            if sp3 < min_cost {
                                min_cost = sp3;
                            }
                        }
                    }
                    s[(swrite_row, l_g - it2_pre_l_off)] = min_cost;
                    l_g = fta[l_g as usize];

                    // Loop D [1, Algorithm 3] - for all nodes to the left of rG.
                    while l_g >= l_g_last {
                        // `current_forest_size2` is deliberately not incremented here: it's
                        // re-read only via the `== 1` check above, before this loop runs, and
                        // gets a fresh value on the next outer (`l_f`) iteration regardless - an
                        // increment here was dead (confirmed via clippy's `unused_assignments`),
                        // so it was removed rather than kept as effect-free busywork.
                        current_forest_cost2 += other_ins_cost(l_g);
                        sp1 = match sp1source {
                            1 => s[(sp1s_row, l_g - it2_pre_l_off)] + path_del_cost(l_f_node),
                            2 => {
                                t[(l_g - it2_pre_l_off, r_g - it2_pre_r_off)]
                                    + path_del_cost(l_f_node)
                            }
                            _ => current_forest_cost2 + path_del_cost(l_f_node),
                        };
                        let sp2_row_col = fna[l_g as usize] - it2_pre_l_off;
                        sp2 = s[(sp2s_row, sp2_row_col)] + other_ins_cost(l_g);
                        min_cost = sp1;
                        if sp2 < min_cost {
                            min_cost = sp2;
                        }
                        let (b, a) = delta_order(l_f_node, l_g);
                        sp3 = delta.get(b, a) as i64;
                        if sp3 < min_cost {
                            let fna_target =
                                fna[(l_g + other_idx.sizes[l_g as usize] as i64 - 1) as usize];
                            sp3 += match sp3source {
                                1 => s[(sp3s_row, fna_target - it2_pre_l_off)],
                                2 => current_forest_cost2 - other_subtree_ins_cost(l_g),
                                _ => t[(fna_target - it2_pre_l_off, r_g - it2_pre_r_off)],
                            };
                            if sp3 < min_cost {
                                sp3 += ren_cost(l_f_node, l_g);
                                if sp3 < min_cost {
                                    min_cost = sp3;
                                }
                            }
                        }
                        s[(swrite_row, l_g - it2_pre_l_off)] = min_cost;
                        l_g = fta[l_g as usize];
                    }
                    l_f -= 1;
                }
                if r_g_minus1_in_pre_l == parent_of_r_g_in_pre_l {
                    if !right_part {
                        if left_part {
                            // `other`-axis index is `parent_of_r_g_in_pre_l` (== `r_g_minus1_in_pre_l`
                            // per the gate above) - *not* `+ 1`. The `+1` belongs only to the
                            // s-table's own relative-offset lookup on the line below; Java's
                            // `delta[endPathNode][parent_of_rG_in_preL]` uses the bare value.
                            let (b, a) = delta_order(end_path_node, parent_of_r_g_in_pre_l);
                            let v = s[(
                                l_f_last + 1 - it1_pre_l_off,
                                r_g_minus1_in_pre_l + 1 - it2_pre_l_off,
                            )] as u64;
                            if apted_debug() {
                                eprintln!("spfA write-A: delta[{b}][{a}] = {v}");
                            }
                            delta.set(b, a, v);
                        }
                        if end_path_node > 0
                            && end_path_node == parent_of_end_path_node + 1
                            && end_path_node_in_pre_r == parent_of_end_path_node_in_pre_r + 1
                        {
                            let (b, a) =
                                delta_order(parent_of_end_path_node, parent_of_r_g_in_pre_l);
                            let v = s[(
                                l_f_last - it1_pre_l_off,
                                r_g_minus1_in_pre_l + 1 - it2_pre_l_off,
                            )] as u64;
                            if apted_debug() {
                                eprintln!("spfA write-B: delta[{b}][{a}] = {v}");
                            }
                            delta.set(b, a, v);
                        }
                    }
                    let mut l_f2 = l_f_first;
                    while l_f2 >= l_f_last {
                        q[l_f2 as usize] = s[(
                            l_f2 - it1_pre_l_off,
                            parent_of_r_g_in_pre_l + 1 - it2_pre_l_off,
                        )];
                        l_f2 -= 1;
                    }
                }
                // TODO: first pointers can be precomputed
                let mut l_g_iter = l_g_first;
                while l_g_iter >= l_g_last {
                    t[(l_g_iter - it2_pre_l_off, r_g - it2_pre_r_off)] =
                        s[(l_f_last - it1_pre_l_off, l_g_iter - it2_pre_l_off)];
                    l_g_iter = fta[l_g_iter as usize];
                }
                r_g -= 1;
            }
        }

        // Deal with nodes to the right of the path. Java: `pathType == 0 || pathType == 2 &&
        // rightPart || pathType == 2 && !leftPart && !rightPart`; simplified to `rightPart ||
        // !leftPart` per this function's INNER-only specialization.
        if right_part || !left_part {
            let (l_f_first, r_f_first);
            if start_path_node == -1 {
                l_f_first = end_path_node;
                r_f_first = end_path_node_in_pre_r;
            } else {
                r_f_first = start_path_node_in_pre_r - 1;
                l_f_first = end_path_node + 1;
            }
            l_f_last = end_path_node;
            let l_g_last = current_subtree_pre_l2;
            let l_g_first = l_g_last + subtree_size2 - 1;
            r_f_last = end_path_node_in_pre_r;
            let fna_last = fna.len() - 1;
            fna[fna_last] = -1;
            for i in current_subtree_pre_l2..(current_subtree_pre_l2 + subtree_size2) {
                fna[i as usize] = -1;
                fta[i as usize] = -1;
            }
            let tmp_forest_size1 = current_forest_size1;
            let tmp_forest_cost1 = current_forest_cost1;
            // Loop B' [1, Algorithm 3] - for all nodes in G.
            let mut l_g = l_g_first;
            while l_g >= l_g_last {
                let r_g_first2 = other_idx.pre_to_pre_r[l_g as usize] as i64;
                update_fn_array(
                    &mut fna,
                    other_idx.pre_r_to_ln[r_g_first2 as usize],
                    r_g_first2,
                    other_idx.pre_to_pre_r[current_subtree_pre_l2 as usize] as i64,
                );
                update_ft_array(
                    &fna,
                    &mut fta,
                    other_idx.pre_r_to_ln[r_g_first2 as usize],
                    r_g_first2,
                );
                let mut l_f = l_f_first;
                let l_g_minus1_in_pre_r = if l_g <= current_subtree_pre_l2 {
                    i64::MAX / 4
                } else {
                    other_idx.pre_to_pre_r[(l_g - 1) as usize] as i64
                };
                let parent_of_l_g = other_idx.parents[l_g as usize];
                let parent_of_l_g_in_pre_r = if parent_of_l_g == -1 {
                    -1
                } else {
                    other_idx.pre_to_pre_r[parent_of_l_g as usize] as i64
                };
                current_forest_size1 = tmp_forest_size1;
                current_forest_cost1 = tmp_forest_cost1;
                let r_g_last2 = if r_g_first2
                    == other_idx.pre_to_pre_r[current_subtree_pre_l2 as usize] as i64
                {
                    r_g_first2
                } else {
                    other_idx.pre_to_pre_r[current_subtree_pre_l2 as usize] as i64
                };
                // Loop C' [1, Algorithm 3] - for all nodes to the right of the path node.
                let mut r_f = r_f_first;
                while r_f >= r_f_last {
                    if r_f == r_f_last {
                        l_f = l_f_last;
                    }
                    let r_f_in_pre_l = path_idx.pre_r_to_pre_l[r_f as usize] as i64;
                    current_forest_size1 += 1;
                    current_forest_cost1 += path_del_cost(r_f_in_pre_l);
                    current_forest_size2 = other_idx.sizes[l_g as usize] as i64;
                    current_forest_cost2 = other_subtree_ins_cost(l_g);
                    let r_f_subtree_size = path_idx.sizes[r_f_in_pre_l as usize] as i64;
                    let (
                        r_f_is_consecutive_node_of_current_path_node,
                        r_f_is_right_sibling_of_current_path_node,
                    ) = if start_path_node > 0 {
                        (
                            start_path_node_in_pre_r - r_f == 1,
                            r_f + r_f_subtree_size == start_path_node_in_pre_r,
                        )
                    } else {
                        (false, false)
                    };
                    let f_forest_is_tree = r_f_in_pre_l == l_f;
                    let r_f_node = r_f_in_pre_l;
                    let sp1s_row = (r_f + 1) - it1_pre_r_off;
                    let sp2s_row = r_f - it1_pre_r_off;
                    let mut sp3s_row = 0i64;
                    let swrite_row = r_f - it1_pre_r_off;
                    let sp1t_row = l_g - it2_pre_l_off;
                    let sp3t_row = l_g - it2_pre_l_off;
                    let mut sp1source = 1u8;
                    let mut sp3source = 1u8;
                    let mut sp3;
                    if f_forest_is_tree {
                        if r_f_subtree_size == 1 {
                            sp1source = 3;
                        } else if r_f_is_consecutive_node_of_current_path_node {
                            sp1source = 2;
                        }
                        sp3 = 0;
                        sp3source = 2;
                    } else {
                        if r_f_is_consecutive_node_of_current_path_node {
                            sp1source = 2;
                        }
                        sp3 = current_forest_cost1 - path_subtree_del_cost(r_f_in_pre_l);
                        if r_f_is_right_sibling_of_current_path_node {
                            sp3source = 3;
                        }
                    }
                    if sp3source == 1 {
                        sp3s_row = (r_f + r_f_subtree_size) - it1_pre_r_off;
                    }
                    let mut sp2 = if current_forest_size2 == 1 {
                        current_forest_cost1
                    } else {
                        q[r_f as usize]
                    };

                    let mut r_g = r_g_first2;
                    let r_g_first_in_pre_l = other_idx.pre_r_to_pre_l[r_g_first2 as usize] as i64;
                    // See Loop D above: incrementing `current_forest_size2` here was dead (never
                    // read again before the next outer iteration's fresh assignment).
                    let mut sp1 = match sp1source {
                        1 => s[(sp1s_row, r_g - it2_pre_r_off)],
                        2 => t[(sp1t_row, r_g - it2_pre_r_off)],
                        _ => current_forest_cost2,
                    };
                    sp1 += path_del_cost(r_f_node);
                    min_cost = sp1;
                    sp2 += other_ins_cost(r_g_first_in_pre_l);
                    if sp2 < min_cost {
                        min_cost = sp2;
                    }
                    if sp3 < min_cost {
                        let (b, a) = delta_order(r_f_node, r_g_first_in_pre_l);
                        sp3 += delta.get(b, a) as i64;
                        if sp3 < min_cost {
                            sp3 += ren_cost(r_f_node, r_g_first_in_pre_l);
                            if sp3 < min_cost {
                                min_cost = sp3;
                            }
                        }
                    }
                    s[(swrite_row, r_g - it2_pre_r_off)] = min_cost;
                    r_g = fta[r_g as usize];

                    // Loop D' [1, Algorithm 3] - for all nodes to the right of lG.
                    while r_g >= r_g_last2 {
                        let r_g_in_pre_l = other_idx.pre_r_to_pre_l[r_g as usize] as i64;
                        current_forest_cost2 += other_ins_cost(r_g_in_pre_l);
                        sp1 = match sp1source {
                            1 => s[(sp1s_row, r_g - it2_pre_r_off)] + path_del_cost(r_f_node),
                            2 => t[(sp1t_row, r_g - it2_pre_r_off)] + path_del_cost(r_f_node),
                            _ => current_forest_cost2 + path_del_cost(r_f_node),
                        };
                        let sp2_row_col = fna[r_g as usize] - it2_pre_r_off;
                        sp2 = s[(sp2s_row, sp2_row_col)] + other_ins_cost(r_g_in_pre_l);
                        min_cost = sp1;
                        if sp2 < min_cost {
                            min_cost = sp2;
                        }
                        let (b, a) = delta_order(r_f_node, r_g_in_pre_l);
                        sp3 = delta.get(b, a) as i64;
                        if sp3 < min_cost {
                            let fna_target = fna[(r_g
                                + other_idx.sizes[r_g_in_pre_l as usize] as i64
                                - 1) as usize];
                            sp3 += match sp3source {
                                1 => s[(sp3s_row, fna_target - it2_pre_r_off)],
                                2 => current_forest_cost2 - other_subtree_ins_cost(r_g_in_pre_l),
                                _ => t[(sp3t_row, fna_target - it2_pre_r_off)],
                            };
                            if sp3 < min_cost {
                                sp3 += ren_cost(r_f_node, r_g_in_pre_l);
                                if sp3 < min_cost {
                                    min_cost = sp3;
                                }
                            }
                        }
                        s[(swrite_row, r_g - it2_pre_r_off)] = min_cost;
                        r_g = fta[r_g as usize];
                    }
                    r_f -= 1;
                }
                if l_g > current_subtree_pre_l2 && l_g - 1 == parent_of_l_g {
                    if right_part {
                        let (b, a) = delta_order(end_path_node, parent_of_l_g);
                        let v = s[(
                            r_f_last + 1 - it1_pre_r_off,
                            l_g_minus1_in_pre_r + 1 - it2_pre_r_off,
                        )] as u64;
                        if apted_debug() {
                            eprintln!("spfA write-C: delta[{b}][{a}] = {v}");
                        }
                        delta.set(b, a, v);
                    }
                    if end_path_node > 0
                        && end_path_node == parent_of_end_path_node + 1
                        && end_path_node_in_pre_r == parent_of_end_path_node_in_pre_r + 1
                    {
                        let (b, a) = delta_order(parent_of_end_path_node, parent_of_l_g);
                        let v = s[(
                            r_f_last - it1_pre_r_off,
                            l_g_minus1_in_pre_r + 1 - it2_pre_r_off,
                        )] as u64;
                        if apted_debug() {
                            eprintln!("spfA write-D: delta[{b}][{a}] = {v}");
                        }
                        delta.set(b, a, v);
                    }
                    let mut r_f2 = r_f_first;
                    while r_f2 >= r_f_last {
                        q[r_f2 as usize] = s[(
                            r_f2 - it1_pre_r_off,
                            parent_of_l_g_in_pre_r + 1 - it2_pre_r_off,
                        )];
                        r_f2 -= 1;
                    }
                }
                // TODO: first pointers can be precomputed
                let mut r_g_iter = r_g_first2;
                while r_g_iter >= r_g_last2 {
                    t[(l_g - it2_pre_l_off, r_g_iter - it2_pre_r_off)] =
                        s[(r_f_last - it1_pre_r_off, r_g_iter - it2_pre_r_off)];
                    r_g_iter = fta[r_g_iter as usize];
                }
                l_g -= 1;
            }
        }

        // Walk up the path by one node.
        start_path_node = end_path_node;
        end_path_node = path_idx.parents[end_path_node as usize];
    }

    min_cost as u64
}

/// Direct port of APTED.java's `spf1`: closed-form tree edit distance when at least one of the
/// two subtrees is a single node, avoiding the overhead of the general single-path machinery.
/// Writes nothing into `delta` - the size-1-side cells it would otherwise touch are already
/// covered by `ted_init`.
pub(crate) fn spf1(ctx: &EngineCtx, root1: usize, root2: usize) -> u64 {
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

/// Which postorder direction a single-path decomposition is walking - `Left` (left-to-right,
/// `spfL`'s world) or `Right` (right-to-left, `spfR`'s world). Bundled with the four accessor
/// functions below so the functions that used to be hand-duplicated once per direction
/// (`computeKeyRoots`/`computeRevKeyRoots` -> `compute_keyroots`; `treeEditDist`/`treeEditDistR` ->
/// `apted_tree_edit_dist`; `spfL`/`spfR` -> `spf_path`) can share one implementation instead, the
/// same way `path_is_before: bool` already lets `spf_a` share one implementation across the
/// before/after axis. Checkability against the original Java (which keeps these as separate,
/// unparameterized functions) is no longer a goal here, so this genuine mechanical duplication was
/// worth removing like any other. `compute_opt_strategy_post_l`/`compute_opt_strategy_post_r` are
/// deliberately NOT unified this way - they aren't a pure accessor-swap mirror (the post-`min_cost`
/// parent-propagation step swaps which owned mutable buffer plays which role between the two), so
/// merging them would be a materially bigger and riskier change than this one.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PostDir {
    Left,
    Right,
}

/// Preorder id -> this direction's own postorder index. `Left` is a plain array lookup
/// (`pre_to_post_l`); `Right` is `pre_to_post_r`'s computed `size - 1 - pre`.
fn pre_to_post(idx: &AptedIndexer, dir: PostDir, pre: usize) -> usize {
    match dir {
        PostDir::Left => idx.pre_to_post_l[pre],
        PostDir::Right => idx.pre_to_post_r(pre),
    }
}

/// This direction's postorder index -> preorder id. Inverse of `pre_to_post`.
fn post_to_pre(idx: &AptedIndexer, dir: PostDir, post: usize) -> usize {
    match dir {
        PostDir::Left => idx.post_l_to_pre_l[post],
        PostDir::Right => idx.post_r_to_pre_l(post),
    }
}

/// This direction's postorder index -> this direction's own postorder index of that node's
/// "extreme" leaf descendant (leftmost/`lld` for `Left`, rightmost/`rld` for `Right`).
fn post_to_extreme_leaf_post(idx: &AptedIndexer, dir: PostDir, post: usize) -> usize {
    match dir {
        PostDir::Left => idx.post_l_to_lld[post],
        PostDir::Right => idx.post_r_to_rld[post],
    }
}

/// Preorder id -> preorder id of that node's extreme leaf descendant (itself if it's a leaf;
/// leftmost for `Left`, rightmost for `Right`).
fn pre_to_extreme_leaf(idx: &AptedIndexer, dir: PostDir, pre: usize) -> usize {
    match dir {
        PostDir::Left => idx.pre_l_to_lld(pre),
        PostDir::Right => idx.pre_l_to_rld(pre),
    }
}

/// Direct port of APTED.java's `computeKeyRoots`/`computeRevKeyRoots`, generalized over `dir`:
/// collects, into `keyroots`, every node that is a keyroot of `subtree_root`'s decomposition along
/// `dir` - i.e. `subtree_root` itself, plus (recursively) every sibling on the side opposite `dir`
/// encountered while walking up from `path_id` (the extreme leaf descendant of `subtree_root` in
/// direction `dir`) back to `subtree_root`. `Left`: every node with a left sibling is its own
/// keyroot, reached via each subtree's leftmost leaf descendant. `Right`: the mirror image, via
/// rightmost leaf descendants.
pub(crate) fn compute_keyroots(
    idx: &AptedIndexer,
    dir: PostDir,
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
                compute_keyroots(
                    idx,
                    dir,
                    child,
                    pre_to_extreme_leaf(idx, dir, child),
                    keyroots,
                );
            }
        }
        path_node = parent;
    }
}

/// Direct port of APTED.java's `treeEditDist`/`treeEditDistR` (the core of `spfL`/`spfR`),
/// generalized over `dir`: fills `forestdist` with the distances between every subforest pair
/// spanning `[extreme_leaf(path_subtree), path_subtree]` on the path side against
/// `[extreme_leaf(other_subtree), other_subtree]` on the other side, and - as a side effect,
/// exactly like `forest_dist` above - writes `delta` for every aligned (tree-vs-tree) position
/// encountered along the way. `dir == Left`: "extreme leaf" means leftmost (`lld`), boundaries are
/// left-to-right postorder. `dir == Right`: rightmost (`rld`), right-to-left postorder - the `_i`/
/// `_j`/`lld_*` names below are kept for both directions rather than renamed per-direction, since
/// the DP shape is otherwise identical either way.
///
/// `path_is_before` says whether the path side is `before` (the "T1" of the global
/// before/after orientation) or `after`; this alone determines both the delete/insert cost
/// direction and which axis of `delta` each side's preorder id belongs on - see Java's
/// `treesSwapped` parameter, which this replaces (it served exactly the same purpose, just
/// re-derived here from the orientation that's already implied by `path_is_before`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn apted_tree_edit_dist(
    ctx: &EngineCtx,
    delta: &mut DeltaTable,
    dir: PostDir,
    path_is_before: bool,
    path_subtree: usize,
    other_subtree: usize,
    forestdist: &mut ForestDist,
) {
    let (path_idx, other_idx, path_meta, other_meta) = ctx.sides(path_is_before);

    // `i`/`j`/`di`/`dj` are 1-based boundaries exactly like `forest_dist`'s (boundary `b`
    // corresponds to the node at 0-based postorder `b - 1`) - `forestdist`'s array index *is*
    // this same boundary value directly, so `lld_i`/`lld_j` (0-based postorder of the extreme
    // leaf, which numerically equals the boundary of the node *before* it) double as the base-
    // case index without any extra shift. Mirrors `forest_dist` precisely; only the cost
    // direction (`path_is_before`), the direction (`dir`), and the indexer/cost-model plumbing
    // differ.
    let i = pre_to_post(path_idx, dir, path_subtree) + 1;
    let j = pre_to_post(other_idx, dir, other_subtree) + 1;
    let lld_i = post_to_extreme_leaf_post(path_idx, dir, i - 1);
    let lld_j = post_to_extreme_leaf_post(other_idx, dir, j - 1);

    forestdist[(lld_i, lld_j)] = 0;
    for di in (lld_i + 1)..=i {
        let pre = post_to_pre(path_idx, dir, di - 1);
        let cost = if path_is_before {
            vdel(ctx.cost_model, vnode(path_idx, path_meta, pre))
        } else {
            vins(ctx.cost_model, vnode(path_idx, path_meta, pre))
        };
        forestdist[(di, lld_j)] = forestdist[(di - 1, lld_j)] + cost;
    }
    for dj in (lld_j + 1)..=j {
        let pre = post_to_pre(other_idx, dir, dj - 1);
        let cost = if path_is_before {
            vins(ctx.cost_model, vnode(other_idx, other_meta, pre))
        } else {
            vdel(ctx.cost_model, vnode(other_idx, other_meta, pre))
        };
        forestdist[(lld_i, dj)] = forestdist[(lld_i, dj - 1)] + cost;
    }

    for di in (lld_i + 1)..=i {
        let path_pre = post_to_pre(path_idx, dir, di - 1);
        let path_node = vnode(path_idx, path_meta, path_pre);
        let path_lld = post_to_extreme_leaf_post(path_idx, dir, di - 1);
        let del_cost = if path_is_before {
            vdel(ctx.cost_model, path_node)
        } else {
            vins(ctx.cost_model, path_node)
        };
        for dj in (lld_j + 1)..=j {
            let other_pre = post_to_pre(other_idx, dir, dj - 1);
            let other_node = vnode(other_idx, other_meta, other_pre);
            let other_lld = post_to_extreme_leaf_post(other_idx, dir, dj - 1);
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
            let (before_pre, after_pre) = if path_is_before {
                (path_pre, other_pre)
            } else {
                (other_pre, path_pre)
            };
            let ren_cost = vren_adjusted(
                ctx,
                before_pre as i64,
                after_pre as i64,
                vren(ctx.cost_model, before_node, after_node),
            );

            let da = forestdist[(di - 1, dj)] + del_cost;
            let db = forestdist[(di, dj - 1)] + ins_cost;

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

/// Direct port of APTED.java's `spfL`/`spfR`, generalized over `dir`: the path side
/// (`path_subtree`, already reduced to a single remaining path by `gted`'s caller) against the
/// *entire* other side, decomposed via its own keyroots (in direction `dir`) in one combined
/// sweep - this single combined sweep across all of `other_subtree`'s keyroots, rather than one
/// call per (keyroot, keyroot) pair, is what makes APTED asymptotically cheaper than the classic
/// Zhang-Shasha keyroot loop.
pub(crate) fn spf_path(
    ctx: &EngineCtx,
    delta: &mut DeltaTable,
    dir: PostDir,
    path_is_before: bool,
    path_subtree: usize,
    other_subtree: usize,
) -> u64 {
    let (path_idx, other_idx, _, _) = ctx.sides(path_is_before);

    let mut keyroots = Vec::new();
    if other_subtree == 0 {
        // The virtual root's children are the *forest's own roots* - each is its own keyroot
        // regardless of sibling status on either side (mirroring `PostorderIndexer`'s
        // `root_pres`), since there is no real ancestor whose own path could ever cover more than
        // one of them. Treating the virtual root itself as an ordinary node here would silently
        // absorb its first child into a path that runs all the way down to the forest's overall
        // extreme leaf - that child would then never get its own aligned (tree-vs-tree)
        // boundary, exactly the boundary `compute_edit_mapping`'s backtrace later depends on.
        for &root in &other_idx.children[0] {
            compute_keyroots(
                other_idx,
                dir,
                root,
                pre_to_extreme_leaf(other_idx, dir, root),
                &mut keyroots,
            );
        }
    } else {
        compute_keyroots(
            other_idx,
            dir,
            other_subtree,
            pre_to_extreme_leaf(other_idx, dir, other_subtree),
            &mut keyroots,
        );
    }
    keyroots.sort_by_key(|&pre| pre_to_post(other_idx, dir, pre));

    // Sized and indexed by the same 1-based-boundary convention as `apted_tree_edit_dist`
    // (absolute, not relative to any one call's own extreme-leaf boundary), and reused across the
    // whole keyroot sweep below - see the comment there for why a relative scheme would be
    // unsound here.
    let mut forestdist = ForestDist::new(path_idx.size + 1, other_idx.size + 1, 0);
    for &kr in &keyroots {
        apted_tree_edit_dist(
            ctx,
            delta,
            dir,
            path_is_before,
            path_subtree,
            kr,
            &mut forestdist,
        );
    }
    forestdist[(
        pre_to_post(path_idx, dir, path_subtree) + 1,
        pre_to_post(other_idx, dir, other_subtree) + 1,
    )]
}

/// Direct port of APTED.java's `computeOptStrategy_postL`: for every `(v, w)` pair, picks
/// whichever of v's LEFT/RIGHT/INNER path or w's LEFT/RIGHT/INNER path minimizes the cost of the
/// single-path sweep `gted` would have to run, and encodes that choice as a signed path id (see
/// `getStrategyPathType`/`gted`'s decode). Costs are `i64` here rather than Java's `float` - the
/// products involved (`size * krSum`) are well within `i64` range for any real input, and exact
/// integers sidestep the precision loss `float` would have on a "subtree size" scale; the
/// `INNER_DISABLED` sentinel keeps the same comparison structure as a plain `i64::MAX` would,
/// with enough headroom below `i64::MAX` that summing several of them (the `cost1_I`/`cost2_I`
/// propagation below) can't overflow.
///
/// `clamp_to_left_right`, *not* in the Java original, disables the two INNER candidates at
/// selection time only (the `cost1_I`/`cost2_I` *maintenance* below still runs unconditionally) -
/// used to validate the bidirectional `gted` plus this function's L/R candidates in isolation,
/// before `spfA` exists to handle an INNER choice. Forcing L/R instead of the truly optimal path
/// only affects efficiency, never correctness: `gted`/`spfL`/`spfR` compute the exact distance
/// for *any* valid strategy, optimal or not - which is exactly why the oracle (a distance
/// comparison) can validate this clamped strategy on its own before INNER is enabled.
///
/// Skips Java's `rowsToReuse_L/R/I` stacks (which only recycle `cost1_*` row allocations across
/// nodes that have already been fully consumed - a pure allocation-count optimization with no
/// effect on the values computed); `cost1_L/R/I` are instead `Vec<Option<Vec<i64>>>`, each row
/// allocated fresh the first time a node needs one.
pub(crate) fn compute_opt_strategy_post_l(
    before_idx: &AptedIndexer,
    after_idx: &AptedIndexer,
    clamp_to_left_right: bool,
) -> StrategyTable {
    const INNER_DISABLED: i64 = i64::MAX / 4;

    let size1 = before_idx.size;
    let size2 = after_idx.size;
    let mut strategy = StrategyTable::new(size1, size2);
    let mut cost1_l: Vec<Option<Vec<i64>>> = vec![None; size1];
    let mut cost1_r: Vec<Option<Vec<i64>>> = vec![None; size1];
    let mut cost1_i: Vec<Option<Vec<i64>>> = vec![None; size1];
    let mut cost2_l = vec![0i64; size2];
    let mut cost2_r = vec![0i64; size2];
    let mut cost2_i = vec![0i64; size2];
    let mut cost2_path = vec![0usize; size2];
    let path_id_offset = size1 as i64;

    for v in 0..size1 {
        let v_in_pre_l = before_idx.post_l_to_pre_l[v];
        let is_v_leaf = before_idx.sizes[v_in_pre_l] == 1;
        let parent_v_pre_l = before_idx.parents[v_in_pre_l];
        let parent_v_post_l = if parent_v_pre_l >= 0 {
            Some(before_idx.pre_to_post_l[parent_v_pre_l as usize])
        } else {
            None
        };

        let size_v = before_idx.sizes[v_in_pre_l] as i64;
        let left_path_v = -(before_idx.pre_l_to_lld(v_in_pre_l) as i64 + 1);
        let right_path_v = v_in_pre_l as i64 + size_v;
        let kr_sum_v = before_idx.kr_sum[v_in_pre_l] as i64;
        let revkr_sum_v = before_idx.rev_kr_sum[v_in_pre_l] as i64;
        let desc_sum_v = before_idx.desc_sum[v_in_pre_l] as i64;

        if is_v_leaf {
            cost1_l[v] = Some(vec![0i64; size2]);
            cost1_r[v] = Some(vec![0i64; size2]);
            cost1_i[v] = Some(vec![0i64; size2]);
            for w_pre in 0..size2 {
                strategy.set(v_in_pre_l, w_pre, v_in_pre_l as i64);
            }
        }

        if let Some(parent_post_l) = parent_v_post_l
            && cost1_l[parent_post_l].is_none()
        {
            cost1_l[parent_post_l] = Some(vec![0i64; size2]);
            cost1_r[parent_post_l] = Some(vec![0i64; size2]);
            cost1_i[parent_post_l] = Some(vec![0i64; size2]);
        }

        // Reset for every `v` - `cost2_*` accumulate `w`'s contributions *within this v's own
        // sweep* (mirrors Java's per-`v` `Arrays.fill`); carrying values over from a previous
        // `v` would both be wrong and accumulate unboundedly across the outer loop.
        cost2_l.fill(0);
        cost2_r.fill(0);
        cost2_i.fill(0);
        cost2_path.fill(0);

        for w in 0..size2 {
            let w_in_pre_l = after_idx.post_l_to_pre_l[w];
            let parent_w_pre_l = after_idx.parents[w_in_pre_l];
            let parent_w_post_l = if parent_w_pre_l >= 0 {
                Some(after_idx.pre_to_post_l[parent_w_pre_l as usize])
            } else {
                None
            };

            let size_w = after_idx.sizes[w_in_pre_l] as i64;
            if after_idx.sizes[w_in_pre_l] == 1 {
                cost2_l[w] = 0;
                cost2_r[w] = 0;
                cost2_i[w] = 0;
                cost2_path[w] = w_in_pre_l;
            }

            let mut min_cost = INNER_DISABLED;
            // Java leaves this `-1`: it's never decoded, since `gted` short-circuits straight to
            // `spf1` (writing no `delta`) whenever either side is this small.
            let mut strategy_path: i64 = -1;

            if size_v <= 1 || size_w <= 1 {
                min_cost = size_v.max(size_w);
            } else {
                let cost_l_v = cost1_l[v].as_ref().unwrap()[w];
                let cost_r_v = cost1_r[v].as_ref().unwrap()[w];
                let cost_i_v = cost1_i[v].as_ref().unwrap()[w];

                let kr_sum_w = after_idx.kr_sum[w_in_pre_l] as i64;
                let tmp_cost = size_v * kr_sum_w + cost_l_v;
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = left_path_v;
                }
                let revkr_sum_w = after_idx.rev_kr_sum[w_in_pre_l] as i64;
                let tmp_cost = size_v * revkr_sum_w + cost_r_v;
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = right_path_v;
                }
                if !clamp_to_left_right {
                    let desc_sum_w = after_idx.desc_sum[w_in_pre_l] as i64;
                    let tmp_cost = size_v * desc_sum_w + cost_i_v;
                    if tmp_cost < min_cost {
                        min_cost = tmp_cost;
                        strategy_path = strategy.get(v_in_pre_l, w_in_pre_l) + 1;
                    }
                }
                let tmp_cost = size_w * kr_sum_v + cost2_l[w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path =
                        -(after_idx.pre_l_to_lld(w_in_pre_l) as i64 + path_id_offset + 1);
                }
                let tmp_cost = size_w * revkr_sum_v + cost2_r[w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = w_in_pre_l as i64 + size_w + path_id_offset;
                }
                if !clamp_to_left_right {
                    let tmp_cost = size_w * desc_sum_v + cost2_i[w];
                    if tmp_cost < min_cost {
                        min_cost = tmp_cost;
                        strategy_path = cost2_path[w] as i64 + path_id_offset + 1;
                    }
                }
            }

            if let Some(parent_post_l) = parent_v_post_l {
                let cost_r_v = cost1_r[v].as_ref().unwrap()[w];
                cost1_r[parent_post_l].as_mut().unwrap()[w] += min_cost;
                let cost_i_v = cost1_i[v].as_ref().unwrap()[w];
                let tmp_cost = -min_cost + cost_i_v;
                if tmp_cost < cost1_i[parent_post_l].as_ref().unwrap()[w] {
                    cost1_i[parent_post_l].as_mut().unwrap()[w] = tmp_cost;
                    let inherited = strategy.get(v_in_pre_l, w_in_pre_l);
                    strategy.set(parent_v_pre_l as usize, w_in_pre_l, inherited);
                }
                if before_idx.node_type_r[v_in_pre_l] {
                    let cost_r_parent = cost1_r[parent_post_l].as_ref().unwrap()[w];
                    cost1_i[parent_post_l].as_mut().unwrap()[w] += cost_r_parent;
                    cost1_r[parent_post_l].as_mut().unwrap()[w] += cost_r_v - min_cost;
                }
                if before_idx.node_type_l[v_in_pre_l] {
                    let cost_l_v = cost1_l[v].as_ref().unwrap()[w];
                    cost1_l[parent_post_l].as_mut().unwrap()[w] += cost_l_v;
                } else {
                    cost1_l[parent_post_l].as_mut().unwrap()[w] += min_cost;
                }
            }
            if let Some(parent_post_l) = parent_w_post_l {
                cost2_r[parent_post_l] += min_cost;
                let tmp_cost = -min_cost + cost2_i[w];
                if tmp_cost < cost2_i[parent_post_l] {
                    cost2_i[parent_post_l] = tmp_cost;
                    cost2_path[parent_post_l] = cost2_path[w];
                }
                if after_idx.node_type_r[w_in_pre_l] {
                    cost2_i[parent_post_l] += cost2_r[parent_post_l];
                    cost2_r[parent_post_l] += cost2_r[w] - min_cost;
                }
                if after_idx.node_type_l[w_in_pre_l] {
                    cost2_l[parent_post_l] += cost2_l[w];
                } else {
                    cost2_l[parent_post_l] += min_cost;
                }
            }

            strategy.set(v_in_pre_l, w_in_pre_l, strategy_path);
        }
    }

    strategy
}

/// Mirror of `compute_opt_strategy_post_l`, using right-to-left preorder (equivalently: a single
/// pass over the *plain* (left-to-right) preorder indices from `size-1` down to `0`, since a
/// node's preorder index is always smaller than every one of its descendants') instead of
/// left-to-right postorder, with the parent-propagation step's L/R roles swapped to match - this
/// is a direct port of APTED.java's `computeOptStrategy_postR`. Unlike `compute_opt_strategy_post_l`
/// this needs no `post_l_to_pre_l`/`pre_to_post_l` translation at all: `v`/`w` already *are*
/// preorder indices throughout, simplifying every lookup. The (kr_sum/revkr_sum/desc_sum)
/// candidate-comparison section is unchanged from postL - only the post-`min_cost`
/// parent-propagation swaps which of L/R absorbs `cost_*_v - min_cost` (gated by
/// `node_type_l`/`node_type_r` respectively) versus which one unconditionally adds `min_cost`.
pub(crate) fn compute_opt_strategy_post_r(
    before_idx: &AptedIndexer,
    after_idx: &AptedIndexer,
    clamp_to_left_right: bool,
) -> StrategyTable {
    const INNER_DISABLED: i64 = i64::MAX / 4;

    let size1 = before_idx.size;
    let size2 = after_idx.size;
    let mut strategy = StrategyTable::new(size1, size2);
    let path_id_offset = size1 as i64;

    let mut cost1_l: Vec<Option<Vec<i64>>> = vec![None; size1];
    let mut cost1_r: Vec<Option<Vec<i64>>> = vec![None; size1];
    let mut cost1_i: Vec<Option<Vec<i64>>> = vec![None; size1];
    let mut cost2_l = vec![0i64; size2];
    let mut cost2_r = vec![0i64; size2];
    let mut cost2_i = vec![0i64; size2];
    let mut cost2_path = vec![0usize; size2];

    for v in (0..size1).rev() {
        let is_v_leaf = before_idx.sizes[v] == 1;
        let parent_v_pre_l = before_idx.parents[v];

        let size_v = before_idx.sizes[v] as i64;
        let left_path_v = -(before_idx.pre_l_to_lld(v) as i64 + 1);
        let right_path_v = v as i64 + size_v;
        let kr_sum_v = before_idx.kr_sum[v] as i64;
        let revkr_sum_v = before_idx.rev_kr_sum[v] as i64;
        let desc_sum_v = before_idx.desc_sum[v] as i64;

        if is_v_leaf {
            cost1_l[v] = Some(vec![0i64; size2]);
            cost1_r[v] = Some(vec![0i64; size2]);
            cost1_i[v] = Some(vec![0i64; size2]);
            for w_pre in 0..size2 {
                strategy.set(v, w_pre, v as i64);
            }
        }

        if parent_v_pre_l >= 0 {
            let parent_pre_l = parent_v_pre_l as usize;
            if cost1_l[parent_pre_l].is_none() {
                cost1_l[parent_pre_l] = Some(vec![0i64; size2]);
                cost1_r[parent_pre_l] = Some(vec![0i64; size2]);
                cost1_i[parent_pre_l] = Some(vec![0i64; size2]);
            }
        }

        // Reset for every `v`, mirroring `compute_opt_strategy_post_l`'s same per-`v` reset:
        // `cost2_*` accumulate `w`'s contributions *within this v's own sweep*.
        cost2_l.fill(0);
        cost2_r.fill(0);
        cost2_i.fill(0);
        cost2_path.fill(0);

        for w in (0..size2).rev() {
            let size_w = after_idx.sizes[w] as i64;
            if after_idx.sizes[w] == 1 {
                cost2_l[w] = 0;
                cost2_r[w] = 0;
                cost2_i[w] = 0;
                cost2_path[w] = w;
            }

            let mut min_cost = INNER_DISABLED;
            let mut strategy_path: i64 = -1;

            if size_v <= 1 || size_w <= 1 {
                min_cost = size_v.max(size_w);
            } else {
                let cost_l_v = cost1_l[v].as_ref().unwrap()[w];
                let cost_r_v = cost1_r[v].as_ref().unwrap()[w];
                let cost_i_v = cost1_i[v].as_ref().unwrap()[w];

                let kr_sum_w = after_idx.kr_sum[w] as i64;
                let tmp_cost = size_v * kr_sum_w + cost_l_v;
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = left_path_v;
                }
                let revkr_sum_w = after_idx.rev_kr_sum[w] as i64;
                let tmp_cost = size_v * revkr_sum_w + cost_r_v;
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = right_path_v;
                }
                if !clamp_to_left_right {
                    let desc_sum_w = after_idx.desc_sum[w] as i64;
                    let tmp_cost = size_v * desc_sum_w + cost_i_v;
                    if tmp_cost < min_cost {
                        min_cost = tmp_cost;
                        strategy_path = strategy.get(v, w) + 1;
                    }
                }
                let tmp_cost = size_w * kr_sum_v + cost2_l[w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = -(after_idx.pre_l_to_lld(w) as i64 + path_id_offset + 1);
                }
                let tmp_cost = size_w * revkr_sum_v + cost2_r[w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = w as i64 + size_w + path_id_offset;
                }
                if !clamp_to_left_right {
                    let tmp_cost = size_w * desc_sum_v + cost2_i[w];
                    if tmp_cost < min_cost {
                        min_cost = tmp_cost;
                        strategy_path = cost2_path[w] as i64 + path_id_offset + 1;
                    }
                }
            }

            if parent_v_pre_l >= 0 {
                let parent_pre_l = parent_v_pre_l as usize;
                let cost_l_v = cost1_l[v].as_ref().unwrap()[w];
                cost1_l[parent_pre_l].as_mut().unwrap()[w] += min_cost;
                let cost_i_v = cost1_i[v].as_ref().unwrap()[w];
                let tmp_cost = -min_cost + cost_i_v;
                if tmp_cost < cost1_i[parent_pre_l].as_ref().unwrap()[w] {
                    cost1_i[parent_pre_l].as_mut().unwrap()[w] = tmp_cost;
                    let inherited = strategy.get(v, w);
                    strategy.set(parent_pre_l, w, inherited);
                }
                if before_idx.node_type_l[v] {
                    let cost_l_parent = cost1_l[parent_pre_l].as_ref().unwrap()[w];
                    cost1_i[parent_pre_l].as_mut().unwrap()[w] += cost_l_parent;
                    cost1_l[parent_pre_l].as_mut().unwrap()[w] += cost_l_v - min_cost;
                }
                if before_idx.node_type_r[v] {
                    let cost_r_v = cost1_r[v].as_ref().unwrap()[w];
                    cost1_r[parent_pre_l].as_mut().unwrap()[w] += cost_r_v;
                } else {
                    cost1_r[parent_pre_l].as_mut().unwrap()[w] += min_cost;
                }
            }

            let parent_w_pre_l = after_idx.parents[w];
            if parent_w_pre_l >= 0 {
                let parent_pre_l = parent_w_pre_l as usize;
                cost2_l[parent_pre_l] += min_cost;
                let tmp_cost = -min_cost + cost2_i[w];
                if tmp_cost < cost2_i[parent_pre_l] {
                    cost2_i[parent_pre_l] = tmp_cost;
                    cost2_path[parent_pre_l] = cost2_path[w];
                }
                if after_idx.node_type_l[w] {
                    cost2_i[parent_pre_l] += cost2_l[parent_pre_l];
                    cost2_l[parent_pre_l] += cost2_l[w] - min_cost;
                }
                if after_idx.node_type_r[w] {
                    cost2_r[parent_pre_l] += cost2_r[w];
                } else {
                    cost2_r[parent_pre_l] += min_cost;
                }
            }

            strategy.set(v, w, strategy_path);
        }
    }

    strategy
}

/// Direct port of APTED.java's `getStrategyPathType`: decodes a signed, offset-encoded path id
/// (see `compute_opt_strategy_post_l`) into which kind of path it is. Java's `it` parameter is
/// unused in the original (dead code) and is dropped here.
pub(crate) fn get_strategy_path_type(
    path_id_with_offset: i64,
    path_id_offset: i64,
    current_root_node_pre_l: usize,
    current_subtree_size: usize,
) -> u8 {
    if path_id_with_offset.is_negative() {
        return 0; // LEFT
    }
    let mut path_id = path_id_with_offset.abs() - 1;
    if path_id >= path_id_offset {
        path_id -= path_id_offset;
    }
    if path_id == (current_root_node_pre_l as i64 + current_subtree_size as i64) - 1 {
        return 1; // RIGHT
    }
    2 // INNER
}

/// Direct port of APTED.java's `tedInit`: densely pre-fills `delta[x][y]` for every (x, y) pair
/// where at least one side's subtree has size 1 - the "subtree distance without the root nodes"
/// in that case is just the cost to insert/delete everything except the size-1 side's own root,
/// computed directly from the subtree cost sums (no recursion needed). `gted`'s own spfL/spfR/spfA
/// write conditions are sparse by design and never populate these size-1-side cells themselves
/// (Java's `gted` bypasses them entirely via the `spf1` shortcut whenever one side has size 1);
/// without this pre-fill, any (x, y) pair absorbed into a path's own contiguous sweep - rather
/// than being given an independent recursive `gted` call - is silently left at delta=0, corrupting
/// later `forest_dist` reads that need the true value. Must run after the strategy is computed
/// (matching Java's `delta = computeOptStrategy_postL(...)` followed immediately by `tedInit()`)
/// and before `gted` starts, since `gted`'s own writes for both-size>1 pairs are disjoint from
/// (and must not be clobbered by) this pre-fill.
pub(crate) fn ted_init(ctx: &EngineCtx, delta: &mut DeltaTable) {
    for x in 1..ctx.before_idx.size {
        let size_x = ctx.before_idx.sizes[x];
        for y in 1..ctx.after_idx.size {
            let size_y = ctx.after_idx.sizes[y];
            if size_x == 1 && size_y == 1 {
                delta.set(x, y, 0);
            } else if size_x == 1 {
                let own_ins = vins(ctx.cost_model, vnode(ctx.after_idx, ctx.after_meta, y));
                delta.set(x, y, ctx.after_idx.sum_ins_cost[y] - own_ins);
            } else if size_y == 1 {
                let own_del = vdel(ctx.cost_model, vnode(ctx.before_idx, ctx.before_meta, x));
                delta.set(x, y, ctx.before_idx.sum_del_cost[x] - own_del);
            }
        }
    }
}

/// Direct port of APTED.java's `gted`: reads the strategy chosen for `(current1, current2)`,
/// walks the indicated path on whichever side it lives on (recursing into every off-path
/// sibling first), then dispatches to the matching single-path function for the resolved path.
///
/// Two deliberate deviations from the Java original:
/// - No `spf1` shortcut for `size <= 1` (see `gted_forced_right`'s comment - `current2`/`current1`
///   can sit at a much larger node than the strategy "expects" mid-recursion here exactly the way
///   it could in that forced-right driver, for the same structural reason: the virtual
///   root makes one side's subtree larger than any single real node, and only `spfL`/`spfR`'s own
///   per-keyroot sweep - not a single aggregate `spf1` comparison - leaves every delta entry an
///   ancestor's sweep might need behind).
/// - The virtual root (preorder `0`) is never path-walked on whichever axis it appears on: unlike
///   every other node, it does not have a single "leftmost child on the path" - *all* of its
///   children are independent forest roots, so each gets its own fully independent `gted` call
///   instead of being silently absorbed into a path that runs past it. This mirrors the
///   `other_subtree == 0` fix in `spf_path`, just applied to `gted`'s own recursion instead
///   of the keyroot-seeding helpers.
pub(crate) fn gted(
    ctx: &EngineCtx,
    delta: &mut DeltaTable,
    strategy: &StrategyTable,
    path_id_offset: i64,
    current1: usize,
    current2: usize,
) -> u64 {
    // Expand virtual-root axes *before* ever reading the strategy or calling a single-path
    // function - on *either* axis, not just whichever one the strategy ends up choosing to
    // decompose. `spf_path` tolerates a vroot-sized *other_subtree* (its own
    // `other_subtree == 0` fix), but `spf_a` does not: its size-based shortcuts (e.g. "G is a
    // single node") key off the *raw* subtree size, which the virtual root inflates by one
    // without representing anything real, so it doesn't get to see vroot as a boundary at all.
    if current1 == 0 {
        let mut total = 0;
        for &child in &ctx.before_idx.children[0] {
            total += gted(ctx, delta, strategy, path_id_offset, child, current2);
        }
        return total;
    }
    if current2 == 0 {
        let mut total = 0;
        for &child in &ctx.after_idx.children[0] {
            total += gted(ctx, delta, strategy, path_id_offset, current1, child);
        }
        return total;
    }

    let size1 = ctx.before_idx.sizes[current1];
    let size2 = ctx.after_idx.sizes[current2];
    // Direct port of Java's `gted`: whenever EITHER side has size 1, shortcut to `spf1` - a pure
    // scalar computation that writes nothing into `delta`. This must be `||`, not `&&`: any
    // size-1-side pair that instead falls through to spf_path/spf_a gets its boundary cells
    // *written* by that call's keyroot sweep, clobbering the values `ted_init` already deposited
    // for exactly these size-1-side pairs (confirmed via a 10-node repro,
    // debug_check_gted_return_n10 - `&&` let an off-path `gted(id3-alone, id8-subtree)` call run
    // spf_path (then still named spf_l), which overwrote `ted_init`'s delta[id3][id9]=0
    // mid-computation, corrupting a sibling spf_a call's read of that same cell even though the
    // final delta value looked correct again by the time `gted` returned). Since `ted_init`
    // already covers every size-1-side cell spf_path could otherwise write, this loses no coverage.
    if size1 <= 1 || size2 <= 1 {
        return spf1(ctx, current1, current2);
    }

    let strategy_path_id = strategy.get(current1, current2);
    let current_path_node_global = strategy_path_id.abs() - 1;

    if current_path_node_global < path_id_offset {
        // Path lives on the `before` side.
        let strategy_path_type =
            get_strategy_path_type(strategy_path_id, path_id_offset, current1, size1);
        let mut current_path_node = current_path_node_global as usize;
        loop {
            let parent = ctx.before_idx.parents[current_path_node];
            if parent < 0 || (parent as usize) < current1 {
                break;
            }
            let parent = parent as usize;
            for &child in &ctx.before_idx.children[parent] {
                if child != current_path_node {
                    gted(ctx, delta, strategy, path_id_offset, child, current2);
                }
            }
            current_path_node = parent;
        }
        if apted_debug() {
            eprintln!(
                "gted T1-path: current1={current1} current2={current2} type={strategy_path_type} path_id={current_path_node_global}"
            );
        }
        return match strategy_path_type {
            0 => spf_path(ctx, delta, PostDir::Left, true, current1, current2),
            1 => spf_path(ctx, delta, PostDir::Right, true, current1, current2),
            _ => spf_a(
                ctx,
                delta,
                true,
                current1,
                current2,
                current_path_node_global as usize,
            ),
        };
    }

    // Path lives on the `after` side. (`current2 == 0` is impossible here - handled at the top
    // of this function, before the strategy was ever read.)
    let current_path_node_global = current_path_node_global - path_id_offset;
    let strategy_path_type =
        get_strategy_path_type(strategy_path_id, path_id_offset, current2, size2);
    let mut current_path_node = current_path_node_global as usize;
    loop {
        let parent = ctx.after_idx.parents[current_path_node];
        if parent < 0 || (parent as usize) < current2 {
            break;
        }
        let parent = parent as usize;
        for &child in &ctx.after_idx.children[parent] {
            if child != current_path_node {
                gted(ctx, delta, strategy, path_id_offset, current1, child);
            }
        }
        current_path_node = parent;
    }
    if apted_debug() {
        eprintln!(
            "gted T2-path: current1={current1} current2={current2} type={strategy_path_type} path_id={current_path_node_global}"
        );
    }
    match strategy_path_type {
        0 => spf_path(ctx, delta, PostDir::Left, false, current2, current1),
        1 => spf_path(ctx, delta, PostDir::Right, false, current2, current1),
        _ => spf_a(
            ctx,
            delta,
            false,
            current2,
            current1,
            current_path_node_global as usize,
        ),
    }
}

/// Recursive tree-decomposition driver, forcing APTED's RIGHT strategy: always decomposes
/// `before`'s *rightmost* path, recursing into off-path (non-last) children, then
/// `spf_path(.., PostDir::Right, ..)` for the resolved path. A `#[cfg(test)]`-only validator: pins
/// `spf_path`/`compute_keyroots`/`apted_tree_edit_dist` (all three with `PostDir::Right`) against
/// the oracle in isolation, since the live engine's strategy
/// choice (`compute_opt_strategy_post_l`) doesn't otherwise guarantee the right-side machinery
/// gets exercised on every test case.
#[cfg(test)]
pub(crate) fn gted_forced_right(
    ctx: &EngineCtx,
    delta: &mut DeltaTable,
    current1: usize,
    current2: usize,
) -> u64 {
    let mut current_path_node = ctx.before_idx.pre_l_to_rld(current1);
    loop {
        let parent = ctx.before_idx.parents[current_path_node];
        if parent < 0 || (parent as usize) < current1 {
            break;
        }
        let parent = parent as usize;
        for &child in &ctx.before_idx.children[parent] {
            if child != current_path_node {
                gted_forced_right(ctx, delta, child, current2);
            }
        }
        current_path_node = parent;
    }

    spf_path(ctx, delta, PostDir::Right, true, current1, current2)
}

/// Computes the tree edit distance and populates `delta` for a forest pair, using the real
/// APTED engine instead of classic Zhang-Shasha keyroot decomposition. Each side's forest is
/// wrapped under a virtual root (see `AptedIndexer`) so the single-rooted APTED recursion can
/// run unmodified; the resulting virtual-space `delta` is then translated back into a `DeltaTable`
/// indexed by the real (non-virtual) preorder ids that `compute_edit_mapping` expects.
#[allow(clippy::too_many_arguments)]
pub(crate) fn compute_delta(
    before: &PostorderIndexer,
    after: &PostorderIndexer,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    cost_model: &UnitCostModel,
    before_root_ids: &[usize],
    after_root_ids: &[usize],
    before_node_map: &rustc_hash::FxHashMap<usize, usize>,
    after_node_map: &rustc_hash::FxHashMap<usize, usize>,
    containment: Option<&ContainmentCtx>,
) -> DeltaTable {
    let mut real_delta = DeltaTable::new(before.size.max(1), after.size.max(1));
    if before.size == 0 || after.size == 0 {
        return real_delta;
    }

    let mut before_idx = AptedIndexer::build(before_meta, before_root_ids, before_node_map);
    let mut after_idx = AptedIndexer::build(after_meta, after_root_ids, after_node_map);
    before_idx.fill_subtree_costs(before_meta, cost_model);
    after_idx.fill_subtree_costs(after_meta, cost_model);

    // `lchl < rchl` heuristic from APTED.java's `ted()` [2, Section 5.3]: pick whichever of
    // postL/postR's preorder direction is cheaper for this tree's shape (counted via `lchl`/
    // `rchl` on `before_idx` - matching Java, which only ever looks at `it1`, the source tree).
    // The strategy table's *contents* (signed, offset-encoded path ids) mean the same thing
    // regardless of which function computed them, so `gted`/`spf_path`/`spf_a` don't need to
    // know or care which branch ran.
    //
    // Unclamped (INNER/spfA enabled) is correctness-verified: full fuzz suite
    // (test_apted_engine_matches_oracle_fuzz) and a 20,000-seed shrinker sweep
    // (shrink_apted_engine_fuzz_failure) both pass. Getting here required three real, ground-
    // truthed fixes against the actual Java APTED.java source (built and instrumented under
    // tmp/apted):
    //   1. `ted_init` was missing entirely - Java's `tedInit()` densely pre-fills `delta[x][y]`
    //      for every pair where one side's subtree has size 1, computed directly from subtree
    //      cost sums; gted's spfL/spfR/spfA write conditions never cover these cells themselves.
    //   2. spfA's write-A/B used a stray `+ 1` on the delta write's "other"-axis index (copied
    //      from the adjacent s-table lookup, which legitimately needs the offset; the delta
    //      write does not).
    //   3. gted's spf1 shortcut required `size1 <= 1 && size2 <= 1`; Java uses `||`. With `&&`,
    //      a size-1-side pair fell through to spf_path, whose keyroot sweep overwrote cells
    //      `ted_init` had already correctly populated.
    let strategy = if before_idx.lchl < before_idx.rchl {
        compute_opt_strategy_post_l(&before_idx, &after_idx, false)
    } else {
        compute_opt_strategy_post_r(&before_idx, &after_idx, false)
    };
    let path_id_offset = before_idx.size as i64;

    let ctx = EngineCtx {
        before_idx: &before_idx,
        after_idx: &after_idx,
        before_meta,
        after_meta,
        cost_model,
        containment,
    };
    let mut virtual_delta = DeltaTable::new(before_idx.size, after_idx.size);
    ted_init(&ctx, &mut virtual_delta);
    gted(&ctx, &mut virtual_delta, &strategy, path_id_offset, 0, 0);

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

/// Shared by the `#[cfg(test)]`-only forced-left/forced-right oracle validators: builds both
/// sides' `AptedIndexer`s, runs `drive` once per top-level real root (see the comment on the loop
/// below), and translates the resulting virtual-space `delta` back into real preorder ids. The
/// live `compute_delta` no longer uses this - the real bidirectional `gted` handles the virtual
/// root's children inside its own recursion (see its doc comment), so it only needs a single
/// `gted(0, 0)` call, not a per-before-root loop.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn compute_delta_with_driver(
    before: &PostorderIndexer,
    after: &PostorderIndexer,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    cost_model: &UnitCostModel,
    before_root_ids: &[usize],
    after_root_ids: &[usize],
    before_node_map: &rustc_hash::FxHashMap<usize, usize>,
    after_node_map: &rustc_hash::FxHashMap<usize, usize>,
    containment: Option<&ContainmentCtx>,
    drive: impl Fn(&EngineCtx, &mut DeltaTable, usize, usize) -> u64,
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
        containment,
    };
    let mut virtual_delta = DeltaTable::new(before_idx.size, after_idx.size);
    // Drive once per top-level real root (the virtual root's children) - exactly the
    // `other_subtree == 0` fix in `spf_path` above, mirrored on the before/T1 axis:
    // starting from the virtual root itself would walk its leftmost/rightmost path all the way
    // down to the forest's overall extreme leaf, silently absorbing the first real root into
    // that path instead of giving it (and every sibling root) its own aligned boundary.
    for &before_root in &before_idx.children[0] {
        drive(&ctx, &mut virtual_delta, before_root, 0);
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

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn compute_delta_forced_right(
    before: &PostorderIndexer,
    after: &PostorderIndexer,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    cost_model: &UnitCostModel,
    before_root_ids: &[usize],
    after_root_ids: &[usize],
    before_node_map: &rustc_hash::FxHashMap<usize, usize>,
    after_node_map: &rustc_hash::FxHashMap<usize, usize>,
    containment: Option<&ContainmentCtx>,
) -> DeltaTable {
    compute_delta_with_driver(
        before,
        after,
        before_meta,
        after_meta,
        cost_model,
        before_root_ids,
        after_root_ids,
        before_node_map,
        after_node_map,
        containment,
        gted_forced_right,
    )
}
