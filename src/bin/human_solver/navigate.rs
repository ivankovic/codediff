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

/// Moves the cursor to the next `Unmarked` node strictly after it; no wrap-around.
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

/// Recomputes caches from `app.mapping`, since the caller's caches predate the mark just applied.
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

/// [`advance_both_to_next_unmarked`] for one side, after a delete or insert. `flat` is that side's.
pub(crate) fn advance_side_to_next_unmarked(
    app: &mut App,
    side: Side,
    flat: &FlatIndex,
    before_root: Node,
    after_root: Node,
) {
    let caches = rebuild_caches_for_mapping(&app.mapping, before_root, after_root);
    let (panel, status_fn): (&mut PanelState, fn(Node, &Caches) -> NodeStatus) = match side {
        Side::Before => (&mut app.before, status_before),
        Side::After => (&mut app.after, status_after),
    };
    advance_to_next_unmarked(panel, flat, &caches, status_fn);
}

/// Moves the cursor to the next (`forward`) or previous node where `disagrees_fn` holds, wrapping
/// around like vim's `n`/`N`. `None`, cursor untouched, if it holds nowhere.
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

/// `n`/`N`: the next/previous node drawn with a trailing `*`. Errors if `p` has not run.
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
            |node, caches, diff_ast| algo_disagrees(Side::Before, node, caches, diff_ast),
            forward,
        ),
        Focus::After => advance_to_next_mismatch(
            &mut app.after,
            after_flat,
            caches,
            diff_ast,
            |node, caches, diff_ast| algo_disagrees(Side::After, node, caches, diff_ast),
            forward,
        ),
    };
    match found {
        Some(node) => Ok(format!("Jumped to mismatch: '{}'", node.kind())),
        None => bail!("No mismatches in this panel"),
    }
}

/// Moves the cursor forward, wrapping, to the next leaf whose text contains `query`. Leaves only:
/// an ancestor's text contains all its descendants', so a subtree match lands on the enclosing
/// container instead of the token.
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

/// `/`: case-sensitive plain substring search, no regex.
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

/// Finds `id` in `root`'s subtree regardless of collapse state, unlike `flatten_visible`.
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

pub(crate) fn expand_ancestors(collapsed: &mut std::collections::HashSet<usize>, node: Node) {
    let mut current = node;
    while let Some(parent) = current.parent() {
        collapsed.remove(&parent.id());
        current = parent;
    }
}

/// Puts `panel`'s cursor on `target_id`, expanding its collapsed ancestors and, if it was not
/// already on screen, centering the viewport on it (clamped at the tree's ends). `None`, nothing
/// moved, when `root` has no such node. `a`/`A` move the other panel, `A` in the text view this
/// side's, hence the explicit `panel`.
pub(crate) fn reveal_node<'tree>(
    panel: &mut PanelState,
    root: Node<'tree>,
    target_id: usize,
) -> Option<Node<'tree>> {
    let was_visible = FlatIndex::new(flatten_visible(root, &panel.collapsed, None))
        .index_of(target_id)
        .is_some_and(|idx| {
            idx >= panel.scroll && idx < panel.scroll + panel.viewport_height.max(1)
        });

    let target_node = find_node_by_id_anywhere(root, target_id)?;
    expand_ancestors(&mut panel.collapsed, target_node);
    panel.cursor_id = target_id;

    if !was_visible {
        let flat = FlatIndex::new(flatten_visible(root, &panel.collapsed, None));
        let idx = flat.index_of(target_id).unwrap_or(0);
        let height = panel.viewport_height.max(1);
        let max_scroll = flat.len().saturating_sub(height);
        panel.scroll = idx.saturating_sub(height / 2).min(max_scroll);
    }

    Some(target_node)
}

/// Moves the panel opposite `focus` to `target_id`, via [`reveal_node`].
pub(crate) fn align_cursor_to(
    app: &mut App,
    focus: Focus,
    before_root: Node,
    after_root: Node,
    target_id: usize,
) -> Result<String> {
    let (other_root, other) = match focus {
        Focus::Before => (after_root, &mut app.after),
        Focus::After => (before_root, &mut app.before),
    };

    let target_node =
        reveal_node(other, other_root, target_id).context("Matched node not found in tree")?;

    Ok(format!("Aligned to matched '{}'", target_node.kind()))
}

/// `a`: aligns the other panel to the cursor node's partner in the human mapping.
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

/// `A`: like `a`, but uses codediff's mapping from `p`. Errors if `p` has not run or codediff
/// deleted/inserted the cursor node.
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
