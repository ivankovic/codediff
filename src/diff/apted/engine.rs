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

//! The APTED tree edit distance algorithm: an independent reimplementation, inspired by Mateusz
//! Pawlik and Nikolaus Augsten, "Efficient Computation of the Tree Edit Distance", ACM Transactions
//! on Database Systems 40(1), 2015. Names follow the paper (`gted`, `spf_a`, the single-path
//! functions, key roots), so the long dynamic-programming functions here can be read side by side
//! with its pseudocode.
//!
//! The pipeline runs it only on scoped pairs, bounded by `common::resolve`'s `APTED_MAX_CELLS`,
//! where its exact computation, worst-case `O(n^3)` time, is affordable.
//!
//! Setting the `APTED_DEBUG` environment variable prints the DP's intermediate tables to stderr.

use crate::code::{ASTMetadata, ASTNodeMetadata};

use super::common::{
    ContainmentCtx, DeltaTable, ForestDist, Grid, PostorderIndexer, UnitCostModel,
};

/// Whether `APTED_DEBUG` is set, cached because it is checked on the DP's hot path.
fn apted_debug() -> bool {
    static DEBUG: std::sync::LazyLock<bool> =
        std::sync::LazyLock::new(|| std::env::var("APTED_DEBUG").is_ok());
    *DEBUG
}

/// `strategy[(pre_v, pre_w)]` from [`optimal_strategy_left_postorder`] or
/// [`optimal_strategy_right_postorder`]: a signed, encoded path id, not a distance. Kept apart from
/// `DeltaTable` so correctness does not rest on `gted` consuming each strategy cell before `delta`
/// reuses it.
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

/// Indexes one side's forest under a synthetic virtual root (preorder `0`, no backing node, zero
/// cost), so the single-tree APTED recursion runs unmodified on a multi-root forest: matching the
/// two virtual roots is free, so `treedist(vroot+F1, vroot+F2) == forestdist(F1, F2)`.
///
/// Real node `pre` of the pruned forest sits at virtual index `pre + 1`; the virtual root is the
/// last left-to-right postorder index.
pub(crate) struct AptedIndexer {
    pub(crate) size: usize,
    /// 0-based preorder index -> real node id, or `None` for the virtual root (index `0`).
    pub(crate) pre_to_node_id: Vec<Option<usize>>,
    /// 0-based preorder index -> 0-based preorder index of the parent, or `-1` for the root.
    pub(crate) parents: Vec<i64>,
    pub(crate) children: Vec<Vec<usize>>,
    /// 0-based preorder index -> size of the subtree rooted there.
    pub(crate) sizes: Vec<usize>,
    pub(crate) pre_to_post_l: Vec<usize>,
    pub(crate) post_l_to_pre_l: Vec<usize>,
    /// 0-based left-to-right postorder index -> postorder index of the leftmost leaf descendant.
    pub(crate) post_l_to_lld: Vec<usize>,
    pub(crate) pre_to_pre_r: Vec<usize>,
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
    pub(crate) sum_del_cost: Vec<u64>,
    pub(crate) sum_ins_cost: Vec<u64>,
    /// Count of leaf nodes that are their parent's first (leftmost) child [APTED paper, Section 5.3].
    pub(crate) lchl: usize,
    /// Count of leaf nodes that are their parent's last (rightmost) child [APTED paper, Section 5.3].
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

        let mut sizes = vec![1usize; size];
        let mut kr_sum = vec![0u64; size];
        let mut rev_kr_sum = vec![0u64; size];
        // Sum of the subtree sizes of every node in the subtree.
        let mut desc_sum_total = vec![0u64; size];
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

        // Descending preorder is ascending right-to-left postorder: children before parents.
        let mut post_r_to_rld = vec![0usize; size];
        for pre in (0..size).rev() {
            let post_r = size - 1 - pre;
            post_r_to_rld[post_r] = match children[pre].last() {
                None => post_r,
                Some(&last_child) => post_r_to_rld[size - 1 - last_child],
            };
        }

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

    /// Fills `sum_del_cost`/`sum_ins_cost`; the virtual root contributes 0.
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

    /// Preorder id of the leftmost leaf descendant of `pre` (itself for a leaf).
    pub(crate) fn pre_l_to_lld(&self, pre: usize) -> usize {
        self.post_l_to_pre_l[self.post_l_to_lld[self.pre_to_post_l[pre]]]
    }

    /// Right-to-left postorder is reversed left-to-right preorder, for any tree shape.
    pub(crate) fn pre_to_post_r(&self, pre: usize) -> usize {
        self.size - 1 - pre
    }

    pub(crate) fn post_r_to_pre_l(&self, post_r: usize) -> usize {
        self.size - 1 - post_r
    }

    /// Preorder id of the rightmost leaf descendant of `pre` (itself for a leaf).
    pub(crate) fn pre_l_to_rld(&self, pre: usize) -> usize {
        self.post_r_to_pre_l(self.post_r_to_rld[self.pre_to_post_r(pre)])
    }
}

/// The node at virtual index `pre`, or `None` for the virtual root. `vdel`/`vins` price `None`
/// at 0 and `vren` pairs it only with the other virtual root, which is what keeps the virtual
/// roots from changing the forest distance.
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

/// Everything `gted` and the single-path functions need. `delta` is always indexed
/// `[before_pre][after_pre]`, whichever side a single-path function is decomposing.
pub(crate) struct EngineCtx<'a> {
    pub(crate) before_idx: &'a AptedIndexer,
    pub(crate) after_idx: &'a AptedIndexer,
    pub(crate) before_meta: &'a ASTMetadata,
    pub(crate) after_meta: &'a ASTMetadata,
    pub(crate) cost_model: &'a UnitCostModel,
    /// Applied through `vren_adjusted`; `None` when nothing in the forest is pruned.
    pub(crate) containment: Option<&'a ContainmentCtx<'a>>,
    /// `spf_path`'s `forestdist` table per path orientation (`[0]` path on before, `[1]` on
    /// after), sized by the whole forest and reused by every `spf_path` call: allocating it per
    /// call makes zeroing it dominate the run. Reuse is sound because `sweep_forest_distances`
    /// writes every cell it reads. `RefCell` because the borrow never spans a `gted` re-entry.
    forestdist_scratch: std::cell::RefCell<[Option<ForestDist>; 2]>,
}

impl<'a> EngineCtx<'a> {
    pub(crate) fn new(
        before_idx: &'a AptedIndexer,
        after_idx: &'a AptedIndexer,
        before_meta: &'a ASTMetadata,
        after_meta: &'a ASTMetadata,
        cost_model: &'a UnitCostModel,
        containment: Option<&'a ContainmentCtx<'a>>,
    ) -> Self {
        EngineCtx {
            before_idx,
            after_idx,
            before_meta,
            after_meta,
            cost_model,
            containment,
            forestdist_scratch: std::cell::RefCell::new([None, None]),
        }
    }

    /// Runs `f` with the `forestdist` scratch table for the given path orientation.
    fn with_forestdist<R>(&self, path_is_before: bool, f: impl FnOnce(&mut ForestDist) -> R) -> R {
        let (path_idx, other_idx, _, _) = self.sides(path_is_before);
        let mut scratch = self.forestdist_scratch.borrow_mut();
        let table = scratch[usize::from(!path_is_before)]
            .get_or_insert_with(|| ForestDist::new(path_idx.size + 1, other_idx.size + 1, 0));
        f(table)
    }
    /// `(path_idx, other_idx, path_meta, other_meta)` for the side the path lives on.
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

/// Applies `ctx.containment` to a `vren` cost at virtual preorder ids `(before_pre, after_pre)`,
/// as `forest_dist` does; the virtual root is left unadjusted.
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

/// Backs `spf_a`'s `s`/`t` tables.
pub(crate) type Mat = Grid<i64>;

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

/// `spfA` (Algorithm 3 of the APTED paper), specialized to inner paths: `gted` routes LEFT/RIGHT
/// paths to `spf_path`, so those branches are omitted and their guards simplified.
/// `path_is_before` says which of the two trees holds the path. Costs are `i64` because several
/// `sp3` intermediates are differences.
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
    // `delta` is always indexed `(before, after)`.
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
    // F is `path_idx`'s side, G is `other_idx`'s.
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

        // Nodes to the left of the path.
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
                        // The paper increments `current_forest_size2` here; nothing reads it.
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
                            // The `+ 1` belongs to the `s` lookup only; `delta` takes the
                            // bare index.
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
                let mut l_g_iter = l_g_first;
                while l_g_iter >= l_g_last {
                    t[(l_g_iter - it2_pre_l_off, r_g - it2_pre_r_off)] =
                        s[(l_f_last - it1_pre_l_off, l_g_iter - it2_pre_l_off)];
                    l_g_iter = fta[l_g_iter as usize];
                }
                r_g -= 1;
            }
        }

        // Nodes to the right of the path.
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
                let mut r_g_iter = r_g_first2;
                while r_g_iter >= r_g_last2 {
                    t[(l_g - it2_pre_l_off, r_g_iter - it2_pre_r_off)] =
                        s[(r_f_last - it1_pre_r_off, r_g_iter - it2_pre_r_off)];
                    r_g_iter = fta[r_g_iter as usize];
                }
                l_g -= 1;
            }
        }

        start_path_node = end_path_node;
        end_path_node = path_idx.parents[end_path_node as usize];
    }

    min_cost as u64
}

/// `spf1`: closed-form distance when either subtree is a single node. Writes nothing into
/// `delta`; `fill_single_node_distances` already covers those cells.
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

/// Postorder direction of a single-path decomposition: `Left` for `spfL`, `Right` for `spfR`.
/// The strategy functions are not unified over it: their parent propagation swaps buffer roles,
/// not just accessors.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PostDir {
    Left,
    Right,
}

fn pre_to_post(idx: &AptedIndexer, dir: PostDir, pre: usize) -> usize {
    match dir {
        PostDir::Left => idx.pre_to_post_l[pre],
        PostDir::Right => idx.pre_to_post_r(pre),
    }
}

fn post_to_pre(idx: &AptedIndexer, dir: PostDir, post: usize) -> usize {
    match dir {
        PostDir::Left => idx.post_l_to_pre_l[post],
        PostDir::Right => idx.post_r_to_pre_l(post),
    }
}

/// Postorder index of the node's extreme leaf descendant: leftmost for `Left`, rightmost for
/// `Right`, both in `dir`'s own postorder.
fn post_to_extreme_leaf_post(idx: &AptedIndexer, dir: PostDir, post: usize) -> usize {
    match dir {
        PostDir::Left => idx.post_l_to_lld[post],
        PostDir::Right => idx.post_r_to_rld[post],
    }
}

fn pre_to_extreme_leaf(idx: &AptedIndexer, dir: PostDir, pre: usize) -> usize {
    match dir {
        PostDir::Left => idx.pre_l_to_lld(pre),
        PostDir::Right => idx.pre_l_to_rld(pre),
    }
}

/// The key roots of `subtree_root` for a sweep in `dir`: appends `subtree_root` and, recursively,
/// every off-path sibling met walking up from `path_id` (its extreme leaf in `dir`) to it.
pub(crate) fn collect_key_roots(
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
                collect_key_roots(
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

/// The core of `spfL`/`spfR`: the same recurrence as `forest_dist`, over `dir`'s postorder,
/// writing `delta` at every aligned (tree-vs-tree) point. `path_is_before` says which of the two
/// trees holds the path.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sweep_forest_distances(
    ctx: &EngineCtx,
    delta: &mut DeltaTable,
    dir: PostDir,
    path_is_before: bool,
    path_subtree: usize,
    other_subtree: usize,
    forestdist: &mut ForestDist,
) {
    let (path_idx, other_idx, path_meta, other_meta) = ctx.sides(path_is_before);

    // 1-based boundaries as in `forest_dist`, absolute rather than relative to this subtree, so
    // `lld_i`/`lld_j` (0-based postorder) are also the base-case boundaries.
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

/// `spfL`/`spfR`: the path side (`gted` has already resolved every off-path subtree) against
/// every keyroot of the other side in one sweep, which is what makes APTED cheaper than
/// Zhang-Shasha's keyroot-pair loop.
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
        // Every forest root is its own keyroot. Treating the virtual root as an ordinary node
        // would absorb its first child into the path, and that child would never get the aligned
        // boundary `compute_edit_mapping`'s backtrace reads.
        for &root in &other_idx.children[0] {
            collect_key_roots(
                other_idx,
                dir,
                root,
                pre_to_extreme_leaf(other_idx, dir, root),
                &mut keyroots,
            );
        }
    } else {
        collect_key_roots(
            other_idx,
            dir,
            other_subtree,
            pre_to_extreme_leaf(other_idx, dir, other_subtree),
            &mut keyroots,
        );
    }
    keyroots.sort_by_key(|&pre| pre_to_post(other_idx, dir, pre));

    ctx.with_forestdist(path_is_before, |forestdist| {
        for &kr in &keyroots {
            sweep_forest_distances(
                ctx,
                delta,
                dir,
                path_is_before,
                path_subtree,
                kr,
                forestdist,
            );
        }
        forestdist[(
            pre_to_post(path_idx, dir, path_subtree) + 1,
            pre_to_post(other_idx, dir, other_subtree) + 1,
        )]
    })
}

/// "Infinite" strategy cost, with headroom so the `cost*_i` propagation's sums cannot overflow.
const INNER_DISABLED: i64 = i64::MAX / 4;

/// The optimal strategy: for every `(v, w)` pair, picks the LEFT/RIGHT/INNER path on either side
/// that minimizes `gted`'s single-path work, encoded as a signed path id (decoded by
/// [`decode_path_type`]). Costs are exact `i64`, not the paper's floats.
///
/// `clamp_to_left_right` excludes INNER candidates from selection (their costs are still
/// propagated), to test the L/R machinery apart from `spfA`. Any valid strategy yields the exact
/// distance, so this changes speed only.
pub(crate) fn optimal_strategy_left_postorder(
    before_idx: &AptedIndexer,
    after_idx: &AptedIndexer,
    clamp_to_left_right: bool,
) -> StrategyTable {
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

        // `cost2_*` accumulate within one `v`'s sweep only.
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
            // Stays `-1` for size-1 pairs, which `gted` sends to `spf1` without decoding.
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

/// [`optimal_strategy_left_postorder`] over descending preorder (children before parents), with the
/// L/R roles of the parent propagation swapped.
pub(crate) fn optimal_strategy_right_postorder(
    before_idx: &AptedIndexer,
    after_idx: &AptedIndexer,
    clamp_to_left_right: bool,
) -> StrategyTable {
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

        // `cost2_*` accumulate within one `v`'s sweep only.
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

/// Which of a subtree's root-to-leaf paths a strategy cell chose, and so which single-path
/// function resolves the pair: `spf_path` for the leftmost and rightmost, `spf_a` for any other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PathType {
    Left,
    Right,
    Inner,
}

/// The path type a strategy cell's signed path id encodes, for the subtree rooted at
/// `current_root_node_pre_l`.
pub(crate) fn decode_path_type(
    path_id_with_offset: i64,
    path_id_offset: i64,
    current_root_node_pre_l: usize,
    current_subtree_size: usize,
) -> PathType {
    if path_id_with_offset.is_negative() {
        return PathType::Left;
    }
    let mut path_id = path_id_with_offset.abs() - 1;
    if path_id >= path_id_offset {
        path_id -= path_id_offset;
    }
    if path_id == (current_root_node_pre_l as i64 + current_subtree_size as i64) - 1 {
        return PathType::Right;
    }
    PathType::Inner
}

/// Fills `delta` for every pair where either subtree has size 1, from the subtree cost sums.
/// `gted` sends those pairs to `spf1`, which writes nothing, so a pair absorbed into a path's
/// sweep would otherwise read 0. Runs before `gted`, whose writes never touch these cells.
pub(crate) fn fill_single_node_distances(ctx: &EngineCtx, delta: &mut DeltaTable) {
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

/// `gted`: resolves every off-path subtree of the strategy's path for `(current1, current2)`
/// recursively, then runs the single-path function for that path.
pub(crate) fn gted(
    ctx: &EngineCtx,
    delta: &mut DeltaTable,
    strategy: &StrategyTable,
    path_id_offset: i64,
    current1: usize,
    current2: usize,
) -> u64 {
    // Each forest root gets its own `gted` call rather than being absorbed into a path through
    // the virtual root, on both axes: `spf_a`'s size shortcuts would count the virtual root.
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
    // `||`, not `&&`: a size-1 pair that reaches `spf_path`/`spf_a` has its sweep overwrite the
    // `fill_single_node_distances` cells a sibling call still reads.
    if size1 <= 1 || size2 <= 1 {
        return spf1(ctx, current1, current2);
    }

    let strategy_path_id = strategy.get(current1, current2);
    let current_path_node_global = strategy_path_id.abs() - 1;

    if current_path_node_global < path_id_offset {
        let strategy_path_type =
            decode_path_type(strategy_path_id, path_id_offset, current1, size1);
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
                "gted T1-path: current1={current1} current2={current2} type={strategy_path_type:?} path_id={current_path_node_global}"
            );
        }
        return match strategy_path_type {
            PathType::Left => spf_path(ctx, delta, PostDir::Left, true, current1, current2),
            PathType::Right => spf_path(ctx, delta, PostDir::Right, true, current1, current2),
            PathType::Inner => spf_a(
                ctx,
                delta,
                true,
                current1,
                current2,
                current_path_node_global as usize,
            ),
        };
    }

    let current_path_node_global = current_path_node_global - path_id_offset;
    let strategy_path_type = decode_path_type(strategy_path_id, path_id_offset, current2, size2);
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
            "gted T2-path: current1={current1} current2={current2} type={strategy_path_type:?} path_id={current_path_node_global}"
        );
    }
    match strategy_path_type {
        PathType::Left => spf_path(ctx, delta, PostDir::Left, false, current2, current1),
        PathType::Right => spf_path(ctx, delta, PostDir::Right, false, current2, current1),
        PathType::Inner => spf_a(
            ctx,
            delta,
            false,
            current2,
            current1,
            current_path_node_global as usize,
        ),
    }
}

/// `gted` forced to `before`'s rightmost path, so tests exercise `PostDir::Right` against the
/// oracle on every case, not just where the optimal strategy picks it.
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

/// Runs APTED on a forest pair and returns `delta` indexed by the pruned forests' real preorder
/// ids, as `compute_edit_mapping` expects.
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

    // `lchl < rchl` [APTED paper, Section 5.3] picks the cheaper strategy direction; both
    // produce the same path-id encoding.
    let strategy = if before_idx.lchl < before_idx.rchl {
        optimal_strategy_left_postorder(&before_idx, &after_idx, false)
    } else {
        optimal_strategy_right_postorder(&before_idx, &after_idx, false)
    };
    let path_id_offset = before_idx.size as i64;

    let ctx = EngineCtx::new(
        &before_idx,
        &after_idx,
        before_meta,
        after_meta,
        cost_model,
        containment,
    );
    let mut virtual_delta = DeltaTable::new(before_idx.size, after_idx.size);
    fill_single_node_distances(&ctx, &mut virtual_delta);
    gted(&ctx, &mut virtual_delta, &strategy, path_id_offset, 0, 0);

    // Virtual index `pre + 1` is real preorder `pre`.
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

/// `compute_delta` with a test-only `drive` in place of `gted`, called once per before-side
/// forest root.
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

    let ctx = EngineCtx::new(
        &before_idx,
        &after_idx,
        before_meta,
        after_meta,
        cost_model,
        containment,
    );
    let mut virtual_delta = DeltaTable::new(before_idx.size, after_idx.size);
    // Per forest root, for the same reason `spf_path` seeds keyroots per root.
    for &before_root in &before_idx.children[0] {
        drive(&ctx, &mut virtual_delta, before_root, 0);
    }

    // Virtual index `pre + 1` is real preorder `pre`.
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
