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
    COST_UPDATE, NodeCache,
};

use super::engine::compute_delta;
#[cfg(test)]
use super::zhang_shasha::compute_delta_zhang_shasha;

/// Cost for updating a literal leaf's value (string/number/etc. contents changed).
///
/// Equal to `COST_UPDATE` since 2026-08-18, kept as a named tier so the literal case stays an
/// explicit seam. It was 2 - "medium, between an identifier rename and delete+insert" - but 2 is
/// *exactly* `COST_DELETE + COST_INSERT`, and a cost equal to delete+insert is not a
/// discouragement, it is a coin flip; measurement showed APTED resolving that tie toward
/// delete+insert every time (raising this to 3 changed nothing corpus-wide), turning "discourage"
/// into a de-facto forbid that `rust-sniffnet-protocol`'s ground truth explicitly contradicts.
/// At 1: -4 mismatches, +1 zero-mismatch fixture, one deliberate +1 (see
/// `rust_add_comments_and_real_new_logic.rs`). The rename-vs-replace ordering rule this encodes:
/// anything cheaper than `COST_DELETE + COST_INSERT` is a preference, anything above it is a
/// prohibition, and nothing should ever sit exactly on the boundary.
const COST_LITERAL_UPDATE: u64 = 1;

/// Cost model for APTED - unit cost model
pub(crate) struct UnitCostModel {
    /// Which operator families the language both sides are parsed as recognizes, in bitmask form
    /// (`nodes::language_operator_family_mask`) - the only thing `ren` needs the language *for*:
    /// permitting a small, hand-picked set of cross-kind operator swaps (see
    /// `kinds_update_allowed`) that would otherwise always be forbidden.
    ///
    /// Stored pre-reduced rather than as a `Language`, because `ren` runs once per tree-edit-
    /// distance DP cell and re-deriving the family list per call is exactly the O(n^2) string
    /// scanning `KindCostClass` exists to remove.
    language_family_mask: nodes::FamilyMask,
}

impl UnitCostModel {
    /// Derives [`UnitCostModel::language_family_mask`] from `language` - the only constructor, so
    /// the two can't fall out of step.
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

    /// Cost for renaming (matching) two nodes.
    ///
    /// Uses an adaptive cost model based on node kinds:
    /// - Identical nodes: 0
    /// - Literal nodes: 2 - medium cost for value changes
    /// - Identifiers and generic punctuation/operators: COST_UPDATE (1) - low cost
    ///   (literals share the same value today - see COST_LITERAL_UPDATE's doc comment)
    /// - Internal nodes: 0 - cost is accounted for by children
    /// - Different kinds (allowed): COST_UPDATE (1) to COST_DELETE + COST_INSERT + 1
    ///
    /// A prior version of this also special-cased type/field/property identifiers at cost 5, but
    /// that branch was unreachable dead code (an identifier-kind check above it already matched
    /// every kind that branch checked for, per `IDENTIFIER_KINDS` in `diff::nodes`) - and
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
                } else if nodes::is_comment(&node1.kind)
                    && is_marker_only(&node1.text) != is_marker_only(&node2.text)
                {
                    // A substantive comment and a bare marker (`#`, `//`, an empty `/* */`) are
                    // not one comment edited: the marker is a blank line in a comment block.
                    // Under plain `COST_UPDATE` the two tie with the right pairing, and the DP
                    // then takes whichever comes first - `ruby-...-process_executer` paired a
                    // rewritten `# @return [Boolean] ...` with the new blank `#` inserted above
                    // it, leaving the real rewrite as an Insert. Strictly dearer than delete +
                    // insert, so the pairing is never chosen, same as a cross-kind pair.
                    COST_DELETE + COST_INSERT + 1
                } else if node1.kind_cost_class.literal_like {
                    // Literals (strings, numbers, etc.) - see the constant's doc comment for why
                    // this tier currently equals COST_UPDATE rather than sitting above it
                    COST_LITERAL_UPDATE
                } else {
                    // Identifiers are cheap to update (common in refactorings); generic
                    // punctuation/operators are also low cost.
                    COST_UPDATE
                }
            } else if node1.owned_text_hash == node2.owned_text_hash {
                // Same kind, internal nodes - can be matched with 0 cost (children cost is
                // accounted for separately via `delta`/recursion).
                0
            } else {
                // ...except that "children carry the cost" is false for a node that owns text
                // *directly*, in the gaps its children don't cover. Nothing else in this model
                // ever charges for those bytes, so without this arm relabelling `role="button"`
                // to `role="menu"` costs zero - and matching an `AttValue` to a completely
                // unrelated one is free, leaving the DP no reason to prefer the right partner.
                //
                // Not an edge case: XML keeps *every* attribute value there, as do CSS's numeric
                // and colour literals, Rust's comments and YAML's quoted scalars (census on
                // `metadata::owned_text_hash_of`).
                //
                // Priced `COST_UPDATE`, strictly cheaper than delete+insert. When this was
                // (accidentally) priced at the then-2 `COST_LITERAL_UPDATE` - exactly
                // `COST_DELETE + COST_INSERT` - the resulting indifference measurably cost
                // `css-wordpress-...-change-simple-values-to-vars` a mapping; see
                // `COST_LITERAL_UPDATE`'s doc comment for the tie rule.
                COST_UPDATE
            }
        } else if nodes::update_allowed_from_masks(
            &node1.kind_cost_class,
            &node2.kind_cost_class,
            self.language_family_mask,
        ) {
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

/// True for comment text that carries no words at all - only its own markers, punctuation and
/// whitespace (`#`, `//`, `/* */`, `--`, `*`). See `UnitCostModel::ren`.
fn is_marker_only(text: &str) -> bool {
    !text.chars().any(char::is_alphanumeric)
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
    /// 0-based preorder index -> 0-based postorder index. Read only by the Zhang-Shasha test
    /// oracle (`zhang_shasha.rs`); the APTED engine builds its own indexer.
    #[cfg(test)]
    pub(crate) pre_to_post: Vec<usize>,
    /// 0-based postorder index -> 0-based postorder index of the leftmost leaf descendant.
    pub(crate) post_to_lld: Vec<usize>,
    /// 0-based preorder indices of the keyroots: the forest's own roots, plus every node that
    /// has a left sibling. Drives the Zhang-Shasha oracle's bottom-up `delta` computation - test
    /// only, like `pre_to_post`.
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
        // Only the Zhang-Shasha test oracle reads them.
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
            #[cfg(test)]
            pre_to_post,
            post_to_lld,
            #[cfg(test)]
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

/// What [`BeforeDecision`] and [`AfterDecision`] have in common, so the before/after halves of
/// the emission and slot logic can share one body instead of a mirrored copy each - every fix
/// that used to have to be made twice (and was, once, made only once) is made once.
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

/// Which tree a node belongs to, for the helpers whose before and after versions differ only in
/// which maps they consult, which key shape `(before, after)` they write, and which of
/// delete/insert they charge.
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

    /// This side's node map in `diff`.
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
    /// Provenance label for every `ASTMappingReason::APTED` entry this resolution produces - see
    /// `for_nodes`'s doc comment.
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
    } else if before_info.owned_text_hash != after_info.owned_text_hash {
        // This function's contract is "the cost of relabeling just the root pair", and for a node
        // owning text directly that cost is not zero - `ren` charges `COST_UPDATE` for exactly
        // this case during the DP search, and `operation_cost` charges it again when scoring, so
        // recording 0 here would leave `mapping.cost` disagreeing with both. (`mapping.cost`
        // drives no decisions - this is consistency, not behavior; twice today a recorded zero
        // that "didn't matter" derailed a diagnosis.)
        (ASTMappingOperation::MatchButNotIdentical, COST_UPDATE)
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
pub(crate) struct SlotCtx<'a> {
    pub(crate) before_meta: &'a ASTMetadata,
    pub(crate) after_meta: &'a ASTMetadata,
    pub(crate) diff: &'a ASTDiff,
    pub(crate) before_parents: &'a rustc_hash::FxHashMap<usize, usize>,
    pub(crate) after_parents: &'a rustc_hash::FxHashMap<usize, usize>,
    /// The before-side roots of the forest this `resolve_forest` call was invoked on. A pair
    /// whose before node is one of these has its context *outside* the forest - the caller
    /// (e.g. the name-keyed pass recursing into an anchored container's children) already vouched
    /// for the surrounding correspondence, but hasn't written its own anchor mapping into `diff`
    /// yet, so parent lookups see nothing. Validation must treat these like tree roots.
    pub(crate) before_forest_roots: &'a std::collections::HashSet<usize>,
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
    emit_subtree(Side::Before, before_id, ctx, diff)
}

pub(crate) fn emit_after_subtree(after_id: usize, ctx: &ResolveCtx, diff: &mut ASTDiff) -> u64 {
    emit_subtree(Side::After, after_id, ctx, diff)
}

/// Emits the mappings for `id`'s subtree on `side` per this call's decisions, returning their
/// total cost: a fresh match is emitted as one; a subtree with nothing reused below it is pruned
/// whole; otherwise just this node is pruned (unit cost) and each child is classified on its own.
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

    // Something below this node is reused elsewhere: prune just this node (unit cost) and let
    // its children be independently classified.
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

/// Compute the cost of deleting an entire subtree.
pub(crate) fn subtree_del_cost(
    node_id: usize,
    meta: &ASTMetadata,
    cost_model: &UnitCostModel,
) -> u64 {
    subtree_cost(Side::Before, node_id, meta, cost_model)
}

/// Compute the cost of inserting an entire subtree.
pub(crate) fn subtree_ins_cost(
    node_id: usize,
    meta: &ASTMetadata,
    cost_model: &UnitCostModel,
) -> u64 {
    subtree_cost(Side::After, node_id, meta, cost_model)
}

/// The cost of pruning `node_id`'s entire subtree on `side` (delete before, insert after).
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

// --- Flat-tree fast path: Myers O(ND) sequence diff ---

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
