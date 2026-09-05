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
//! Moving the cursor through the flattened trees.
//!
//! Split out of `main.rs` along the section banner that already marked this boundary.

use crate::*;

// ---------------------------------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------------------------------

pub(crate) fn move_cursor(panel: &mut PanelState, flat: &FlatIndex, delta: i32) {
    if flat.is_empty() {
        return;
    }
    let idx = flat.index_of(panel.cursor_id).unwrap_or(0);
    let new_idx = (idx as i32 + delta).clamp(0, flat.len() as i32 - 1) as usize;
    panel.cursor_id = flat[new_idx].0.id();
}

pub(crate) fn jump_to_edge(panel: &mut PanelState, flat: &[(Node, usize)], to_start: bool) {
    let edge = if to_start { flat.first() } else { flat.last() };
    if let Some((node, _)) = edge {
        panel.cursor_id = node.id();
    }
}

/// Moves `panel`'s cursor forward to the next node (strictly after the current position) with
/// `NodeStatus::Unmarked`, if one exists. Leaves the cursor untouched otherwise.
pub(crate) fn advance_to_next_unmarked(
    panel: &mut PanelState,
    flat: &FlatIndex,
    caches: &Caches,
    status_fn: fn(Node, &Caches) -> NodeStatus,
) {
    let Some(idx) = flat.index_of(panel.cursor_id) else {
        return;
    };
    for (node, _) in &flat[idx + 1..] {
        if status_fn(*node, caches) == NodeStatus::Unmarked {
            panel.cursor_id = node.id();
            return;
        }
    }
}

/// After a match finalizes, walks both panels' cursors forward to their own next unmarked node
/// (independently), as a quality-of-life step-through-the-tree convenience. Recomputes caches
/// fresh from `app.mapping`, since the caller's caches predate the change that was just applied.
pub(crate) fn advance_both_to_next_unmarked(
    app: &mut App,
    before_flat: &FlatIndex,
    after_flat: &FlatIndex,
    before_root: Node,
    after_root: Node,
) {
    let caches = rebuild_caches_for_mapping(&app.mapping, before_root, after_root);
    advance_to_next_unmarked(&mut app.before, before_flat, &caches, status_before);
    advance_to_next_unmarked(&mut app.after, after_flat, &caches, status_after);
}

/// Same as [`advance_both_to_next_unmarked`], but only for the Before panel: used after a
/// delete, which only touches the Before side, so only that cursor should step forward.
pub(crate) fn advance_before_to_next_unmarked(
    app: &mut App,
    before_flat: &FlatIndex,
    before_root: Node,
    after_root: Node,
) {
    let caches = rebuild_caches_for_mapping(&app.mapping, before_root, after_root);
    advance_to_next_unmarked(&mut app.before, before_flat, &caches, status_before);
}

/// Same as [`advance_both_to_next_unmarked`], but only for the After panel: used after an
/// insert, which only touches the After side, so only that cursor should step forward.
pub(crate) fn advance_after_to_next_unmarked(
    app: &mut App,
    after_flat: &FlatIndex,
    before_root: Node,
    after_root: Node,
) {
    let caches = rebuild_caches_for_mapping(&app.mapping, before_root, after_root);
    advance_to_next_unmarked(&mut app.after, after_flat, &caches, status_after);
}

/// Moves `panel`'s cursor to the next (`forward`) or previous node where `disagrees_fn` is true,
/// relative to its current position, wrapping around the ends of `flat` like a `vim` `n`/`N`
/// search. Returns the node landed on, or `None` if `disagrees_fn` is false for every node.
pub(crate) fn advance_to_next_mismatch<'a>(
    panel: &mut PanelState,
    flat: &FlatIndex<'a>,
    caches: &Caches,
    diff_ast: &ASTDiff,
    disagrees_fn: fn(Node, &Caches, &ASTDiff) -> bool,
    forward: bool,
) -> Option<Node<'a>> {
    if flat.is_empty() {
        return None;
    }
    let len = flat.len();
    let idx = flat.index_of(panel.cursor_id).unwrap_or(0);
    for step in 1..=len {
        let i = if forward {
            (idx + step) % len
        } else {
            (idx + len - step) % len
        };
        let (node, _) = flat[i];
        if disagrees_fn(node, caches, diff_ast) {
            panel.cursor_id = node.id();
            return Some(node);
        }
    }
    None
}

/// Implements `n`/`N`: moves the focused panel's cursor to the next/previous node where codediff's
/// verdict (`p`) disagrees with the human mapping (the same condition that draws the trailing `*`
/// in `render_panel`). Requires `p` to have been run at least once for the current case.
pub(crate) fn action_next_mismatch(
    app: &mut App,
    focus: Focus,
    before_flat: &FlatIndex,
    after_flat: &FlatIndex,
    caches: &Caches,
    forward: bool,
) -> Result<String> {
    let diff_ast = app
        .algo_diff
        .as_ref()
        .context("No codediff result yet; press 'p' to run it first")?;
    let found = match focus {
        Focus::Before => advance_to_next_mismatch(
            &mut app.before,
            before_flat,
            caches,
            diff_ast,
            algo_disagrees_before,
            forward,
        ),
        Focus::After => advance_to_next_mismatch(
            &mut app.after,
            after_flat,
            caches,
            diff_ast,
            algo_disagrees_after,
            forward,
        ),
    };
    match found {
        Some(node) => Ok(format!("Jumped to mismatch: '{}'", node.kind())),
        None => bail!("No mismatches in this panel"),
    }
}

/// Moves `panel`'s cursor to the next leaf node (in `flat`'s order, i.e. document order) whose own
/// text contains `query`, wrapping around like `n`/`N`'s mismatch search
/// (`advance_to_next_mismatch`, which this otherwise mirrors exactly - kept separate rather than
/// generalized into one shared function, since the two predicates need different captured context,
/// `Caches`+`ASTDiff` vs. `query`+`src`, and a bare `fn` pointer can't capture either).
///
/// Leaf nodes only (`child_count() == 0`), not "any node whose subtree's text contains query": a
/// non-leaf node's own text is the concatenation of all its descendants', so almost every ancestor
/// of a real match would *also* "match" under a naive whole-subtree check - starting from most
/// cursor positions, that means landing on some enclosing container (often the file root) instead
/// of the actual token, which is not what a human doing the AST-browser equivalent of Ctrl-F wants.
pub(crate) fn advance_to_next_search_match<'a>(
    panel: &mut PanelState,
    flat: &FlatIndex<'a>,
    src: &[u8],
    query: &str,
) -> Option<Node<'a>> {
    if flat.is_empty() {
        return None;
    }
    let len = flat.len();
    let idx = flat.index_of(panel.cursor_id).unwrap_or(0);
    for step in 1..=len {
        let i = (idx + step) % len;
        let (node, _) = flat[i];
        if node.child_count() == 0 && node.utf8_text(src).unwrap_or("").contains(query) {
            panel.cursor_id = node.id();
            return Some(node);
        }
    }
    None
}

/// Implements `/`'s search: moves the focused panel's cursor to the next leaf node containing
/// `query`, starting just after its current position and wrapping around. Case-sensitive, plain
/// substring match - no regex, matching what was actually asked for.
pub(crate) fn action_search(
    app: &mut App,
    focus: Focus,
    before_flat: &FlatIndex,
    after_flat: &FlatIndex,
    before_src: &[u8],
    after_src: &[u8],
    query: &str,
) -> Result<String> {
    let found = match focus {
        Focus::Before => {
            advance_to_next_search_match(&mut app.before, before_flat, before_src, query)
        }
        Focus::After => advance_to_next_search_match(&mut app.after, after_flat, after_src, query),
    };
    match found {
        Some(node) => Ok(format!("Found '{query}' in '{}'", node.kind())),
        None => bail!("No node containing '{query}' found in this panel"),
    }
}

pub(crate) fn expand_or_descend(panel: &mut PanelState, flat: &FlatIndex) {
    let Some(node) = flat.node_for_id(panel.cursor_id) else {
        return;
    };
    if node.child_count() == 0 {
        return;
    }
    if panel.collapsed.remove(&node.id()) {
        return; // was collapsed; expanding is enough, stay put
    }
    let mut cursor = node.walk();
    if let Some(first_child) = node.children(&mut cursor).next() {
        panel.cursor_id = first_child.id();
    }
}

pub(crate) fn collapse_or_ascend(panel: &mut PanelState, flat: &FlatIndex) {
    let Some(node) = flat.node_for_id(panel.cursor_id) else {
        return;
    };
    if node.child_count() > 0 && !panel.collapsed.contains(&node.id()) {
        panel.collapsed.insert(node.id());
        return;
    }
    if let Some(parent) = node.parent() {
        panel.cursor_id = parent.id();
    }
}

pub(crate) fn ensure_visible(scroll: &mut usize, cursor_idx: usize, viewport_height: usize) {
    let viewport_height = viewport_height.max(1);
    if cursor_idx < *scroll {
        *scroll = cursor_idx;
    } else if cursor_idx >= *scroll + viewport_height {
        *scroll = cursor_idx + 1 - viewport_height;
    }
}

/// Finds the node with id `id` anywhere in `root`'s subtree, regardless of collapse state (unlike
/// `flatten_visible`, which only sees expanded nodes). Used by `action_align` to locate a matched
/// node that may currently be hidden under a collapsed ancestor.
pub(crate) fn find_node_by_id_anywhere(root: Node, id: usize) -> Option<Node> {
    if root.id() == id {
        return Some(root);
    }
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if let Some(found) = find_node_by_id_anywhere(child, id) {
            return Some(found);
        }
    }
    None
}

/// Removes every strict ancestor of `node` from `collapsed`, so `node` is guaranteed to appear in
/// that panel's `flatten_visible` output.
pub(crate) fn expand_ancestors(collapsed: &mut std::collections::HashSet<usize>, node: Node) {
    let mut current = node;
    while let Some(parent) = current.parent() {
        collapsed.remove(&parent.id());
        current = parent;
    }
}

/// Shared tail of `action_align`/`action_align_algo`: moves the *other* panel's cursor to
/// `target_id`. If the target is hidden under a collapsed ancestor, expands every ancestor along
/// its path so it becomes visible. If the target wasn't already on screen (whether because it was
/// hidden, or just scrolled out of view), centers the other panel's viewport on it;
/// `idx.saturating_sub(half).min(max_scroll)` naturally clamps that centering at the start/end of
/// the tree, where a true center isn't possible.
pub(crate) fn align_cursor_to(
    app: &mut App,
    focus: Focus,
    before_root: Node,
    after_root: Node,
    target_id: usize,
) -> Result<String> {
    let other_root = match focus {
        Focus::Before => after_root,
        Focus::After => before_root,
    };
    let other = match focus {
        Focus::Before => &mut app.after,
        Focus::After => &mut app.before,
    };

    let was_visible = FlatIndex::new(flatten_visible(other_root, &other.collapsed, None))
        .index_of(target_id)
        .is_some_and(|idx| {
            idx >= other.scroll && idx < other.scroll + other.viewport_height.max(1)
        });

    let target_node = find_node_by_id_anywhere(other_root, target_id)
        .context("Matched node not found in tree")?;
    expand_ancestors(&mut other.collapsed, target_node);
    other.cursor_id = target_id;

    if !was_visible {
        let flat = FlatIndex::new(flatten_visible(other_root, &other.collapsed, None));
        let idx = flat.index_of(target_id).unwrap_or(0);
        let height = other.viewport_height.max(1);
        let max_scroll = flat.len().saturating_sub(height);
        other.scroll = idx.saturating_sub(height / 2).min(max_scroll);
    }

    Ok(format!("Aligned to matched '{}'", target_node.kind()))
}

/// Implements `a`: aligns to the node the *human mapping* says the cursor node is matched with, if
/// any. See [`align_cursor_to`] for how the target is made visible.
pub(crate) fn action_align(
    app: &mut App,
    focus: Focus,
    before_root: Node,
    after_root: Node,
    caches: &Caches,
) -> Result<String> {
    let (own_cursor, matches) = match focus {
        Focus::Before => (app.before.cursor_id, &caches.before_match),
        Focus::After => (app.after.cursor_id, &caches.after_match),
    };

    let target_id = *matches
        .get(&own_cursor)
        .context("Cursor node is not matched to anything")?;

    align_cursor_to(app, focus, before_root, after_root, target_id)
}

/// Implements `A`: like `a`, but aligns to the node *codediff's own diff* (`p`) says the cursor
/// node is mapped to, instead of the human mapping. Requires `p` to have been run at least once
/// for the current case, and fails if codediff mapped the cursor node to nothing (i.e. it
/// considers it deleted/inserted rather than matched).
pub(crate) fn action_align_algo(
    app: &mut App,
    focus: Focus,
    before_root: Node,
    after_root: Node,
) -> Result<String> {
    let target_id = {
        let diff_ast = app
            .algo_diff
            .as_ref()
            .context("No codediff result yet; press 'p' to run it first")?;
        let own_cursor = match focus {
            Focus::Before => app.before.cursor_id,
            Focus::After => app.after.cursor_id,
        };
        let node_map = match focus {
            Focus::Before => &diff_ast.before_node_map,
            Focus::After => &diff_ast.after_node_map,
        };
        *node_map
            .get(&own_cursor)
            .context("codediff has no verdict for this node")?
    };

    if target_id == 0 {
        bail!("codediff maps this node to nothing (deleted/inserted), not to a matching node");
    }

    align_cursor_to(app, focus, before_root, after_root, target_id)
}
