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

use std::collections::HashMap;

use crate::code::{ASTMetadata, ASTNodeMetadata, Code, Language};
use crate::diff::nodes::{self, kinds_update_allowed};
use crate::diff::{
    ASTDiff, ASTMapping, ASTMappingOperation, ASTMappingReason, COST_DELETE, COST_INSERT,
    COST_UPDATE,
};

use super::engine::compute_delta;
#[cfg(test)]
use super::zhang_shasha::compute_delta_zhang_shasha;

/// Cost for updating a literal leaf's value, kept as its own named tier for tuning. It currently
/// equals `COST_UPDATE`, so literals cost what any other update does.
///
/// The tie rule every rename cost here obeys: below `COST_DELETE + COST_INSERT` is a preference,
/// above it is a prohibition, and nothing sits exactly on it - the DP resolves an exact tie toward
/// delete+insert, which silently turns "discourage" into "forbid".
const COST_LITERAL_UPDATE: u64 = 1;

/// Cost model for APTED.
pub(crate) struct UnitCostModel {
    /// The language's operator families, pre-reduced to a bitmask because `ren` runs once per DP
    /// cell; `ren` needs the language only to permit `kinds_update_allowed`'s cross-kind swaps.
    language_family_mask: nodes::FamilyMask,
}

impl UnitCostModel {
    pub(crate) fn new(language: Language) -> Self {
        UnitCostModel {
            language_family_mask: nodes::language_operator_family_mask(&language),
        }
    }

    pub(crate) fn del(&self, _node: &ASTNodeMetadata) -> u64 {
        COST_DELETE
    }

    pub(crate) fn ins(&self, _node: &ASTNodeMetadata) -> u64 {
        COST_INSERT
    }

    /// Cost of matching `node1` to `node2`. Same-kind internal nodes are free (their children
    /// carry the cost) unless they own text directly; cross-kind pairs cost more than
    /// delete+insert, so they are never chosen, except the hand-picked `kinds_update_allowed` swaps.
    pub(crate) fn ren(&self, node1: &ASTNodeMetadata, node2: &ASTNodeMetadata) -> u64 {
        if node1.kind == node2.kind {
            if node1.children.is_empty() && node2.children.is_empty() {
                if node1.text == node2.text {
                    0
                } else if nodes::is_comment(&node1.kind)
                    && is_marker_only(&node1.text) != is_marker_only(&node2.text)
                {
                    // A bare marker (`#`, `//`) is a blank line in a comment block, not the
                    // worded comment edited; at `COST_UPDATE` it ties with the right pairing.
                    COST_DELETE + COST_INSERT + 1
                } else if node1.kind_cost_class.literal_like {
                    COST_LITERAL_UPDATE
                } else {
                    COST_UPDATE
                }
            } else if node1.owned_text_hash == node2.owned_text_hash {
                0
            } else {
                // A node owning text in the gaps between its children (XML attribute values, CSS
                // literals, Rust comments, YAML quoted scalars) is charged nowhere else, so
                // without this arm any two such nodes pair for free.
                COST_UPDATE
            }
        } else if nodes::update_allowed_from_masks(
            &node1.kind_cost_class,
            &node2.kind_cost_class,
            self.language_family_mask,
        ) {
            // e.g. `<` -> `<=`: the same operator slot, priced like a same-kind leaf update.
            COST_UPDATE
        } else {
            COST_DELETE + COST_INSERT + 1
        }
    }
}

/// True for comment text with no words, only markers, punctuation and whitespace.
fn is_marker_only(text: &str) -> bool {
    !text.chars().any(char::is_alphanumeric)
}

/// A pruned, postorder-indexed view of one side of a forest comparison.
///
/// Any node already in the side's node map is excluded with its whole subtree: it is resolved.
///
/// Index convention: the "boundary" variables of `forest_dist`/`compute_edit_mapping` are prefix
/// lengths (0..=size), so boundary `b` is the node at 0-based postorder index `b - 1`.
pub(crate) struct PostorderIndexer {
    pub(crate) size: usize,
    /// 0-based postorder index -> node id.
    pub(crate) post_to_node_id: Vec<usize>,
    /// 0-based postorder index -> 0-based preorder index.
    pub(crate) post_to_pre: Vec<usize>,
    /// 0-based preorder index -> 0-based postorder index. Test oracle only.
    #[cfg(test)]
    pub(crate) pre_to_post: Vec<usize>,
    /// 0-based postorder index -> 0-based postorder index of the leftmost leaf descendant.
    pub(crate) post_to_lld: Vec<usize>,
    /// 0-based preorder indices of the keyroots: the forest's roots plus every node with a left
    /// sibling. Test oracle only.
    #[cfg(test)]
    pub(crate) keyroots: Vec<usize>,
}

impl PostorderIndexer {
    pub(crate) fn build(
        metadata: &ASTMetadata,
        root_ids: &[usize],
        node_map: &rustc_hash::FxHashMap<usize, usize>,
    ) -> Self {
        fn visit(
            node_id: usize,
            metadata: &ASTMetadata,
            node_map: &rustc_hash::FxHashMap<usize, usize>,
            pre_to_node_id: &mut Vec<usize>,
            node_id_to_pre: &mut rustc_hash::FxHashMap<usize, usize>,
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
        let mut node_id_to_pre: rustc_hash::FxHashMap<usize, usize> =
            rustc_hash::FxHashMap::default();
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

        #[cfg(test)]
        let keyroots: Vec<usize> = {
            let mut has_left_sibling = vec![false; size];
            for children in &pre_children {
                for &child_pre in children.iter().skip(1) {
                    has_left_sibling[child_pre] = true;
                }
            }
            root_pres
                .iter()
                .copied()
                .chain(
                    has_left_sibling
                        .iter()
                        .enumerate()
                        .filter(|&(_, &has_sibling)| has_sibling)
                        .map(|(pre, _)| pre),
                )
                .collect()
        };

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
            #[cfg(test)]
            pre_to_post,
            post_to_lld,
            #[cfg(test)]
            keyroots,
        }
    }

    /// Node id at 1-based boundary `boundary`.
    pub(crate) fn node_id_at(&self, boundary: usize) -> usize {
        self.post_to_node_id[boundary - 1]
    }
}

/// Dense, flat-backed 2D buffer indexed as `grid[(row, col)]`, flat because every table on it is
/// in the DP's innermost loops.
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

/// `spf_a`'s `s`/`t` tables are indexed in a signed path-offset space.
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

pub(crate) type ForestDist = Grid<u64>;

/// `delta[(pre_before, pre_after)]`, the subtree-pair distance, indexed by the pruned trees'
/// 0-based preorder indices. Unset cells read back as 0.
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

/// The forest-distance recurrence, `forestDist`: fills `forestdist[(di, dj)]` for every
/// `lld(i) <= di <= i`, `lld(j) <= dj <= j`. Deleting or inserting costs per single node, not per
/// subtree, which is what lets reused content be found at a different depth.
///
/// `write_delta_on_aligned` records `delta` at every aligned interior point. Only the Zhang-Shasha
/// oracle sets it: it builds `delta` from scratch and relies on those interior writes, since many
/// pairs it later looks up are never keyroot pairs. APTED must not set it, or this call's local
/// values clobber the ones spfL/spfR/spfA already wrote.
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

    let dj_info: Vec<(usize, &ASTNodeMetadata, usize, usize)> = ((lld_j + 1)..=j)
        .map(|dj| {
            let after_id = after.node_id_at(dj);
            let node2 = after_meta
                .node_info
                .get(&after_id)
                .expect("indexed node must have metadata");
            (
                after_id,
                node2,
                after.post_to_lld[dj - 1],
                after.post_to_pre[dj - 1],
            )
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

/// One node-level decision produced by `compute_edit_mapping`.
#[derive(Debug, Clone, Copy)]
pub(crate) enum RawDecision {
    Match(usize, usize),
    Delete(usize),
    Insert(usize),
}

/// Backtracks through `forest_dist` to the optimal node-level edit mapping,
/// `computeEditMapping`. Every node in both pruned forests gets exactly one decision.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BeforeDecision {
    Match(usize),
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AfterDecision {
    Match(usize),
    Insert,
}

/// What [`BeforeDecision`] and [`AfterDecision`] share, so the before/after halves of emission
/// and slot logic are one body rather than mirrored copies.
pub(crate) trait SideDecision: Copy {
    /// The fresh match target, or `None` for the side's prune decision (`Delete`/`Insert`).
    fn match_target(self) -> Option<usize>;
}

impl SideDecision for BeforeDecision {
    fn match_target(self) -> Option<usize> {
        match self {
            Self::Match(target) => Some(target),
            Self::Delete => None,
        }
    }
}

impl SideDecision for AfterDecision {
    fn match_target(self) -> Option<usize> {
        match self {
            Self::Match(target) => Some(target),
            Self::Insert => None,
        }
    }
}

/// Which tree a node belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    Before,
    After,
}

impl Side {
    /// The `(before, after)` mapping key pairing this side's `own` node with `partner`.
    pub(crate) fn pair(self, own: usize, partner: usize) -> (usize, usize) {
        match self {
            Side::Before => (own, partner),
            Side::After => (partner, own),
        }
    }

    /// The `(before, after)` mapping key that prunes this side's `own` node (partner 0).
    pub(crate) fn prune_key(self, own: usize) -> (usize, usize) {
        self.pair(own, 0)
    }

    /// The operation and unit cost that pruning one node on this side records.
    pub(crate) fn prune_operation(self) -> (ASTMappingOperation, u64) {
        match self {
            Side::Before => (ASTMappingOperation::Delete, COST_DELETE),
            Side::After => (ASTMappingOperation::Insert, COST_INSERT),
        }
    }

    pub(crate) fn node_map(self, diff: &ASTDiff) -> &rustc_hash::FxHashMap<usize, usize> {
        match self {
            Side::Before => &diff.before_node_map,
            Side::After => &diff.after_node_map,
        }
    }

    /// The cost of pruning one node on this side under `cost_model`.
    pub(crate) fn node_cost(self, cost_model: &UnitCostModel, info: &ASTNodeMetadata) -> u64 {
        match self {
            Side::Before => cost_model.del(info),
            Side::After => cost_model.ins(info),
        }
    }
}

pub(crate) struct ResolveCtx<'a> {
    pub(crate) before_meta: &'a ASTMetadata,
    pub(crate) after_meta: &'a ASTMetadata,
    pub(crate) before_decision: HashMap<usize, BeforeDecision>,
    pub(crate) after_decision: HashMap<usize, AfterDecision>,
    pub(crate) before_has_match_below: HashMap<usize, bool>,
    pub(crate) after_has_match_below: HashMap<usize, bool>,
    /// Provenance label for every `ASTMappingReason::APTED` entry this resolution produces.
    pub(crate) source: &'static str,
}

impl ResolveCtx<'_> {
    pub(crate) fn meta(&self, side: Side) -> &ASTMetadata {
        match side {
            Side::Before => self.before_meta,
            Side::After => self.after_meta,
        }
    }

    /// `id`'s fresh match target on `side`, if this call's decisions matched it.
    fn fresh_match_target(&self, side: Side, id: usize) -> Option<usize> {
        match side {
            Side::Before => self.before_decision.get(&id).and_then(|d| d.match_target()),
            Side::After => self.after_decision.get(&id).and_then(|d| d.match_target()),
        }
    }

    fn has_match_below(&self, side: Side, id: usize) -> bool {
        let map = match side {
            Side::Before => &self.before_has_match_below,
            Side::After => &self.after_has_match_below,
        };
        map.get(&id).copied().unwrap_or(false)
    }
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
    // A pre-existing match is resolved whole, so stop; a fresh match's children still get their
    // own decisions, so keep recursing.
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

/// Classifies a matched pair into its `ASTMappingOperation` plus the cost of relabeling just the
/// root pair; the caller accounts for children.
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
            // `HumanOperation` convention: a cross-kind pair is `MatchButNotIdentical`, never
            // `Update`, which is reserved for same-kind pairs.
            return (ASTMappingOperation::MatchButNotIdentical, COST_UPDATE);
        }
        // Unreachable in practice: `ren` prices this above delete+insert.
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
    } else if before_info.owned_text_hash != after_info.owned_text_hash {
        // Agrees with `ren` and `operation_cost`, which both charge for owned text.
        (ASTMappingOperation::MatchButNotIdentical, COST_UPDATE)
    } else {
        (ASTMappingOperation::MatchButNotIdentical, 0)
    }
}

/// How many ancestor levels to climb from a generic-token leaf looking for an already-decided
/// match. Two, not one, to tolerate a single inserted or removed wrapper node; bounded, because
/// the enclosing function matching is no evidence that a stray `;` inside it pairs with another.
const MAX_CONTEXT_ANCESTOR_DEPTH: usize = 2;

/// Climb bound for the before side of `update_context_supported`.
const MAX_UPDATE_CONTEXT_ANCESTOR_DEPTH: usize = 3;

/// Combined before+after climb budget for `update_context_supported`. A shared budget, not a
/// per-side bound, separates a deep rename (3 levels before, 1 after: passes) from skeleton reuse
/// that is deep on both sides (3+3: fails).
const UPDATE_CONTEXT_DEPTH_BUDGET: usize = 4;

/// Read-only context for the small-context match validation in `improve_slot_alignment`.
pub(crate) struct SlotCtx<'a> {
    pub(crate) before_meta: &'a ASTMetadata,
    pub(crate) after_meta: &'a ASTMetadata,
    pub(crate) diff: &'a ASTDiff,
    pub(crate) before_parents: &'a rustc_hash::FxHashMap<usize, usize>,
    pub(crate) after_parents: &'a rustc_hash::FxHashMap<usize, usize>,
    /// The before-side roots of this `resolve_forest` call's forest. Validation treats them like
    /// tree roots: the caller vouched for their context but has not yet written its anchor.
    pub(crate) before_forest_roots: &'a std::collections::HashSet<usize>,
}

/// True if `before_id` has an ancestor within `max_depth` levels that is matched, by an earlier
/// pass or by this DP call.
///
/// One-sided on purpose: the decisions are an ordered tree mapping, so the partner of a matched
/// ancestor already contains `after_id`.
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

/// Two-sided small-context check for `validate_fresh_matches`: the before-side climb to the
/// nearest matched ancestor plus the after-side climb to that ancestor's partner must fit
/// `UPDATE_CONTEXT_DEPTH_BUDGET`. Only the nearest ancestor is tried; a higher one's partner is
/// only further away. Unlike `has_nearby_matched_ancestor`, this bounds the after side too, so a
/// `(` near a matched body cannot pair with one inside an unrelated new call far away.
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
        &UnitCostModel::new(ctx.before_meta.language),
    );
    let mut total = root_cost;

    if let Some(info) = ctx.before_meta.node_info.get(&before_id) {
        for child in filter_mapped_nodes(&info.children, &diff.before_node_map) {
            total += emit_before_subtree(child, ctx, diff);
        }
    }
    if let Some(info) = ctx.after_meta.node_info.get(&after_id) {
        for child in filter_mapped_nodes(&info.children, &diff.after_node_map) {
            if matches!(ctx.after_decision.get(&child), Some(AfterDecision::Insert)) {
                total += emit_after_subtree(child, ctx, diff);
            }
            // A matched after-child is emitted through its before-side partner.
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
    emit_subtree(Side::Before, before_id, ctx, diff)
}

pub(crate) fn emit_after_subtree(after_id: usize, ctx: &ResolveCtx, diff: &mut ASTDiff) -> u64 {
    emit_subtree(Side::After, after_id, ctx, diff)
}

/// Emits the mappings for `id`'s subtree on `side` per this call's decisions and returns their
/// total cost.
fn emit_subtree(side: Side, id: usize, ctx: &ResolveCtx, diff: &mut ASTDiff) -> u64 {
    if let Some(partner) = ctx.fresh_match_target(side, id) {
        let (before_id, after_id) = side.pair(id, partner);
        return emit_match(before_id, after_id, ctx, diff);
    }

    let meta = ctx.meta(side);
    if !ctx.has_match_below(side, id) {
        match side {
            Side::Before => add_delete_mappings(id, meta, ctx.source, diff),
            Side::After => add_insert_mappings(id, meta, ctx.source, diff),
        }
        return subtree_cost(side, id, meta, &UnitCostModel::new(meta.language));
    }

    let (operation, unit_cost) = side.prune_operation();
    let mut total = unit_cost;
    if let Some(info) = meta.node_info.get(&id) {
        for child in filter_mapped_nodes(&info.children, side.node_map(diff)) {
            total += emit_subtree(side, child, ctx, diff);
        }
    }
    let (before_id, after_id) = side.prune_key(id);
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

/// Marks a pair of subtrees with equal full hashes, and all their descendants, as `Identical`.
/// Equal full hashes guarantee the children line up 1:1.
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
        ASTMapping::identical(ASTMappingReason::APTED(source)),
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

/// Adds a delete or insert mapping for every not-yet-mapped node of a subtree.
#[allow(clippy::too_many_arguments)]
fn add_prune_mappings(
    node_id: usize,
    meta: &ASTMetadata,
    source: &'static str,
    diff: &mut ASTDiff,
    node_map: fn(&ASTDiff) -> &rustc_hash::FxHashMap<usize, usize>,
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
        let cost = subtree_cost(node_id, meta, &UnitCostModel::new(meta.language));
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
            add_prune_mappings(
                child_id,
                meta,
                source,
                diff,
                node_map,
                mapping_key,
                operation,
                subtree_cost,
            );
        }
    }
}

/// Add delete mappings for an entire subtree (used when no part of it is reused elsewhere).
pub(crate) fn add_delete_mappings(
    node_id: usize,
    meta: &ASTMetadata,
    source: &'static str,
    diff: &mut ASTDiff,
) {
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
pub(crate) fn add_insert_mappings(
    node_id: usize,
    meta: &ASTMetadata,
    source: &'static str,
    diff: &mut ASTDiff,
) {
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

pub(crate) fn subtree_del_cost(
    node_id: usize,
    meta: &ASTMetadata,
    cost_model: &UnitCostModel,
) -> u64 {
    subtree_cost(Side::Before, node_id, meta, cost_model)
}

pub(crate) fn subtree_ins_cost(
    node_id: usize,
    meta: &ASTMetadata,
    cost_model: &UnitCostModel,
) -> u64 {
    subtree_cost(Side::After, node_id, meta, cost_model)
}

fn subtree_cost(side: Side, node_id: usize, meta: &ASTMetadata, cost_model: &UnitCostModel) -> u64 {
    if node_id == 0 {
        return 0;
    }
    let Some(info) = meta.node_info.get(&node_id) else {
        return 0;
    };
    let mut cost = side.node_cost(cost_model, info);
    for &child_id in &info.children {
        cost += subtree_cost(side, child_id, meta, cost_model);
    }
    cost
}

mod myers;
pub(crate) use myers::*;
mod prematch;
pub(crate) use prematch::*;
mod residual;
pub(crate) use residual::*;
mod slots;
pub(crate) use slots::*;
mod resolve;
pub use resolve::*;

#[cfg(test)]
mod tests;
