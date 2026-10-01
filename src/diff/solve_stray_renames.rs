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
//! Phase 9b: undoes a rename that crossed into another construct. A single-token node the
//! mapping renames (`Update`) must sit in the same place on both sides: under parents that are
//! paired with each other, or at least of one kind. Otherwise the tree edit distance has taken one
//! leaf of a replaced expression and relabelled it into the new one - `RegularTimePeriod` of
//! `RegularTimePeriod.DEFAULT_TIME_ZONE` renamed into `zone` (`java-defects4j-chart-8-week`) -
//! where the reader sees the old expression gone and a new one in its place.
//!
//! Only unpairs; phase 10 records the delete and the insert.

use crate::code::ASTMetadata;
use crate::diff::{ASTDiff, ASTMappingOperation, PassCtx};

/// Whether another named child of `parent` than `leaf` has no partner - deleted, inserted on the
/// after side, or still undecided: the construct lost content beside the renamed leaf, so it was replaced rather than
/// reduced to that leaf (`!hasColorSpaceId` loses only its `!`).
fn lost_named_sibling(
    meta: &ASTMetadata,
    parent: usize,
    leaf: usize,
    node_map: &rustc_hash::FxHashMap<usize, usize>,
) -> bool {
    meta.node_info.get(&parent).is_some_and(|info| {
        info.children.iter().any(|&child| {
            child != leaf
                && node_map.get(&child).is_none_or(|&partner| partner == 0)
                && meta.node_info.get(&child).is_some_and(|c| c.is_named)
        })
    })
}

fn kind(meta: &ASTMetadata, id: usize) -> Option<&str> {
    meta.node_info.get(&id).map(|info| info.kind.as_str())
}

pub fn solve(ctx: &PassCtx, diff: &mut ASTDiff) {
    let (before, after) = (ctx.before_metadata(), ctx.after_metadata());
    let is_leaf = |meta: &ASTMetadata, id: usize| {
        meta.node_info
            .get(&id)
            .is_some_and(|info| info.children.is_empty())
    };

    let mut stray: Vec<(usize, usize)> = diff
        .mapping
        .iter()
        .filter(|&(&(b, a), mapping)| {
            b != 0 && a != 0 && mapping.operation == ASTMappingOperation::Update
        })
        .map(|(&pair, _)| pair)
        .filter(|&(b, a)| {
            if !is_leaf(before, b) || !is_leaf(after, a) {
                return false;
            }
            let (Some(&before_parent), Some(&after_parent)) =
                (before.node_to_parent.get(&b), after.node_to_parent.get(&a))
            else {
                return false;
            };
            diff.before_node_map.get(&before_parent) != Some(&after_parent)
                && kind(before, before_parent) != kind(after, after_parent)
                && lost_named_sibling(before, before_parent, b, &diff.before_node_map)
        })
        .collect();
    // Sorted so the removal order, and so `mapping`'s history, is reproducible run to run.
    stray.sort_unstable();
    for (b, a) in stray {
        diff.remove_match_mapping(b, a);
    }
}
