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
//!
//! Split out of `main.rs` along the section banner that already marked this boundary.

use crate::*;

// ---------------------------------------------------------------------------------------------
// Tree flattening & node status
// ---------------------------------------------------------------------------------------------

/// Flattens a tree into preorder (node, depth) pairs, skipping the children of collapsed nodes and
/// (if `hidden` is given) any node -- and its whole subtree -- present in `hidden` entirely. A
/// node that's hidden this way doesn't get a row of its own, unlike a collapsed one.
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

/// `flatten_visible`'s output, paired with a node id -> index lookup table built alongside it.
/// Resolving a node id back to its position in the flat list -- almost always
/// `PanelState::cursor_id`, to move it or to find where the cursor row is for scrolling/rendering
/// -- used to mean an O(n) linear scan of the flat list (`.position()`/`.find()`), repeated on
/// every cursor move, every mark, and every single redraw. On a 30k-node tree that scan alone
/// showed up as real per-keystroke latency; this makes it O(1) instead.
///
/// Derefs to `[(Node, usize)]`, so any caller that only ever iterated or indexed the flat list
/// positionally (never by node id) needs no changes at all -- it's a drop-in replacement for the
/// bare `Vec<(Node, usize)>` `flatten_visible` used to hand back directly.
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

    /// `id`'s position in the flat list, in O(1).
    pub(crate) fn index_of(&self, id: usize) -> Option<usize> {
        self.by_id.get(&id).copied()
    }

    /// The node with id `id`, in O(1). `None` if `id` isn't currently visible (e.g. hidden under
    /// a collapsed ancestor, or under `H`'s hide-solved filter).
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

/// Node IDs whose entire subtree -- the node itself and every descendant -- has `NodeStatus`
/// other than `Unmarked`: nothing left in it to review. Used by the `H` (hide solved) toggle to
/// prune those subtrees from the flattened view (via `flatten_visible`'s `hidden` set) while any
/// node that's still `Unmarked` stays visible, along with its full ancestor chain (an ancestor of
/// an `Unmarked` node can never itself be fully solved, so it's never included here).
pub(crate) fn fully_solved_nodes(
    root: Node,
    caches: &Caches,
    status_fn: fn(Node, &Caches) -> NodeStatus,
) -> std::collections::HashSet<usize> {
    let mut solved = std::collections::HashSet::new();
    mark_fully_solved(root, caches, status_fn, &mut solved);
    solved
}

/// Post-order: returns whether `node`'s own subtree is fully solved, recording it in `solved` if
/// so. A node counts as solved only if it is itself marked *and* every child is fully solved.
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

/// codediff's own per-node verdict, computed from an `ASTDiff` (via `p`) the same way `NodeStatus`
/// is computed from the human mapping, but collapsed to a single glyph rather than distinguishing
/// with-children/inherited marks: `before_node_map`/`after_node_map` already carry that down to
/// every descendant node directly, since codediff maps (or zero-maps) every node in the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AlgoStatus {
    /// Mapped to a node on the other side (whatever the specific `ASTMappingOperation`).
    Matched,
    Deleted,
    Inserted,
    /// No entry for this node at all, e.g. the tree root (see `ASTDiff::is_complete`) or a diff
    /// that hasn't been recomputed since the tree changed underneath it.
    Unknown,
}

/// `side`'s node map in `diff_ast`, and the `(before, after)` mapping key that pairs `own` on
/// that side with `partner` on the other - the two things every before/after twin below used to
/// differ in.
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

/// What codediff did with `node` on `side`.
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

/// Which pass produced `node`'s mapping entry on `side`, if any -- `diff_ast.mapping` has one
/// entry per node (see `apted::common::add_delete_mappings`/`add_insert_mappings`), keyed by
/// `(before_id, after_id)` with `0` standing in for "no partner" on whichever side is missing, so
/// this looks up the entry the same way for a match, a delete, or (in principle) an unresolved
/// node -- `None` only when the side's node map has no entry at all (`AlgoStatus::Unknown`).
pub(crate) fn algo_reason(side: Side, node: Node, diff_ast: &ASTDiff) -> Option<ASTMappingReason> {
    let partner = *side_node_map(side, diff_ast).get(&node.id())?;
    diff_ast
        .mapping
        .get(&side_mapping_key(side, node.id(), partner))
        .map(|m| m.reason)
}

/// Short column-style label for an `ASTMappingReason`. Thin wrapper around
/// `ASTMappingReason::bucket_label`, shared with `src/bin/benchmark_optimal_solutions.rs`'s
/// reason-count columns so the same abbreviation means the same thing in both tools. Collapses
/// `APTED`'s provenance payload to a bare "APTED" - see [`reason_detail`] for the version that
/// shows it.
pub(crate) fn reason_label(reason: ASTMappingReason) -> &'static str {
    reason.bucket_label()
}

/// Same short label as [`reason_label`], except for `APTED`, where it also appends the
/// provenance payload (e.g. `"APTED:final_pass"`) - see `ASTMappingReason::APTED`'s doc comment
/// on why that payload exists. Used for the `r`-toggle's per-node display (`render_panel`), where
/// "which pass matched it" is exactly the point; `reason_label` stays the bare bucket label
/// everywhere a stable, provenance-independent abbreviation is needed instead (the reason-count
/// table this tool shares an abbreviation scheme with).
pub(crate) fn reason_detail(reason: ASTMappingReason) -> String {
    match reason {
        ASTMappingReason::APTED(source) => format!("APTED:{source}"),
        other => reason_label(other).to_string(),
    }
}

/// True if codediff's verdict for the Before `node` disagrees with the human's, once the human has
/// actually made a decision about it: not just whether both sides call it "matched", but whether
/// they agree on *what* it's matched to (mirrors the comparison `check_entry` makes for the
/// `optimal_solutions` tests). A node the human hasn't marked yet has nothing to disagree with, so
/// it's never flagged, even if codediff already has an opinion.
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
