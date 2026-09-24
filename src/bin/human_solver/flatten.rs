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
//! Flattening a tree into visible rows, and what each node's mapping status is.

use crate::*;

// ---------------------------------------------------------------------------------------------
// Tree flattening & node status
// ---------------------------------------------------------------------------------------------

/// Preorder (node, depth) rows. A collapsed node keeps its row but not its children; a node in
/// `hidden` loses its row and its whole subtree.
pub(crate) fn flatten_visible<'a>(
    root: Node<'a>,
    collapsed: &std::collections::HashSet<usize>,
    hidden: Option<&std::collections::HashSet<usize>>,
) -> Vec<(Node<'a>, usize)> {
    let mut out = Vec::new();
    walk_visible(root, 0, collapsed, hidden, &mut out);
    out
}

pub(crate) fn walk_visible<'a>(
    node: Node<'a>,
    depth: usize,
    collapsed: &std::collections::HashSet<usize>,
    hidden: Option<&std::collections::HashSet<usize>>,
    out: &mut Vec<(Node<'a>, usize)>,
) {
    if hidden.is_some_and(|hidden| hidden.contains(&node.id())) {
        return;
    }
    out.push((node, depth));
    if collapsed.contains(&node.id()) {
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_visible(child, depth + 1, collapsed, hidden, out);
    }
}

/// `flatten_visible`'s output plus a node id -> row lookup. Cursor moves, marks and redraws all
/// resolve `cursor_id` to a row, and a linear scan per keystroke is visible latency on large trees.
pub(crate) struct FlatIndex<'a> {
    pub(crate) nodes: Vec<(Node<'a>, usize)>,
    pub(crate) by_id: rustc_hash::FxHashMap<usize, usize>,
}

impl<'a> FlatIndex<'a> {
    pub(crate) fn new(nodes: Vec<(Node<'a>, usize)>) -> Self {
        let by_id = nodes
            .iter()
            .enumerate()
            .map(|(index, (node, _))| (node.id(), index))
            .collect();
        Self { nodes, by_id }
    }

    pub(crate) fn index_of(&self, id: usize) -> Option<usize> {
        self.by_id.get(&id).copied()
    }

    /// `None` if `id` isn't currently visible (under a collapsed ancestor, or hidden by `H`).
    pub(crate) fn node_for_id(&self, id: usize) -> Option<Node<'a>> {
        self.index_of(id).map(|index| self.nodes[index].0)
    }
}

impl<'a> std::ops::Deref for FlatIndex<'a> {
    type Target = [(Node<'a>, usize)];

    fn deref(&self) -> &Self::Target {
        &self.nodes
    }
}

/// Node IDs whose node and every descendant are marked: the `hidden` set for `H`. An ancestor of
/// an `Unmarked` node is never included, so an unmarked node stays reachable.
pub(crate) fn fully_solved_nodes(
    root: Node,
    caches: &Caches,
    status_fn: fn(Node, &Caches) -> NodeStatus,
) -> std::collections::HashSet<usize> {
    let mut solved = std::collections::HashSet::new();
    mark_fully_solved(root, caches, status_fn, &mut solved);
    solved
}

/// Returns whether `node`'s subtree is fully solved, recording it in `solved` if so.
pub(crate) fn mark_fully_solved(
    node: Node,
    caches: &Caches,
    status_fn: fn(Node, &Caches) -> NodeStatus,
    solved: &mut std::collections::HashSet<usize>,
) -> bool {
    let mut cursor = node.walk();
    let mut all_children_solved = true;
    for child in node.children(&mut cursor) {
        if !mark_fully_solved(child, caches, status_fn, solved) {
            all_children_solved = false;
        }
    }

    let is_solved = all_children_solved && status_fn(node, caches) != NodeStatus::Unmarked;
    if is_solved {
        solved.insert(node.id());
    }
    is_solved
}

/// codediff's own per-node verdict (from `p`). Unlike `NodeStatus` it has no inherited variants:
/// codediff's node maps carry an entry for every descendant directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AlgoStatus {
    /// Mapped to a node on the other side (whatever the specific `ASTMappingOperation`).
    Matched,
    Deleted,
    Inserted,
    /// No entry, e.g. the tree root (see `ASTDiff::is_complete`) or a stale diff.
    Unknown,
}

fn side_node_map(side: Side, diff_ast: &ASTDiff) -> &rustc_hash::FxHashMap<usize, usize> {
    match side {
        Side::Before => &diff_ast.before_node_map,
        Side::After => &diff_ast.after_node_map,
    }
}

fn side_mapping_key(side: Side, own: usize, partner: usize) -> (usize, usize) {
    match side {
        Side::Before => (own, partner),
        Side::After => (partner, own),
    }
}

pub(crate) fn algo_status(side: Side, node: Node, diff_ast: &ASTDiff) -> AlgoStatus {
    match side_node_map(side, diff_ast).get(&node.id()) {
        Some(0) => match side {
            Side::Before => AlgoStatus::Deleted,
            Side::After => AlgoStatus::Inserted,
        },
        Some(_) => AlgoStatus::Matched,
        None => AlgoStatus::Unknown,
    }
}

pub(crate) fn algo_status_glyph(status: AlgoStatus) -> &'static str {
    match status {
        AlgoStatus::Matched => "M",
        AlgoStatus::Deleted => "-",
        AlgoStatus::Inserted => "+",
        AlgoStatus::Unknown => "?",
    }
}

/// Which pass produced `node`'s mapping entry on `side`. Deletes and inserts have entries too,
/// keyed with `0` for the missing partner; `None` only for `AlgoStatus::Unknown`.
pub(crate) fn algo_reason(side: Side, node: Node, diff_ast: &ASTDiff) -> Option<ASTMappingReason> {
    let partner = *side_node_map(side, diff_ast).get(&node.id())?;
    diff_ast
        .mapping
        .get(&side_mapping_key(side, node.id(), partner))
        .map(|m| m.reason)
}

/// The same bucket label `benchmark_optimal_solutions` uses, so an abbreviation means the same
/// thing in both tools. Drops `APTED`'s provenance; see [`reason_detail`].
pub(crate) fn reason_label(reason: ASTMappingReason) -> &'static str {
    reason.bucket_label()
}

/// [`reason_label`] plus `APTED`'s provenance (e.g. `"APTED:final_pass"`), for the `r` toggle's
/// per-node display.
pub(crate) fn reason_detail(reason: ASTMappingReason) -> String {
    match reason {
        ASTMappingReason::APTED(source) => format!("APTED:{source}"),
        other => reason_label(other).to_string(),
    }
}

/// True if codediff's verdict for `node` differs from the human's, including matching it to a
/// different partner (the same comparison `check_entry` makes). An unmarked node never disagrees.
pub(crate) fn algo_disagrees(side: Side, node: Node, caches: &Caches, diff_ast: &ASTDiff) -> bool {
    let (human_match, human_removed) = match side {
        Side::Before => (&caches.before_match, &caches.before_removed),
        Side::After => (&caches.after_match, &caches.after_removed),
    };
    let algo_partner = side_node_map(side, diff_ast).get(&node.id()).copied();
    if let Some(human_partner) = human_match.get(&node.id()) {
        return algo_partner != Some(*human_partner);
    }
    if human_removed.contains_key(&node.id()) || is_inherited_removed(node, human_removed) {
        return algo_partner != Some(0);
    }
    false
}
