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

/**
* A helper binary for building the ground-truth AST mappings used by src/test/optimal_solutions.
*
* Run as `cargo run --bin human_solver -- <name>`, where `<name>` is the name of a directory under
* `src/test/data/diffs/` (e.g. "rust-add-if"). It opens a Ratatui TUI showing the TreeSitter ASTs
* of the before and after code side by side (not the source text), lets a human walk both trees
* independently and mark nodes as matching, deleted or inserted, and saves the result as
* `src/test/data/diffs/<name>/human_mapping.json`. It also creates the corresponding
* `src/test/optimal_solutions/<name>.rs` test file (if one doesn't already exist), which simply
* calls `codediff::test::helper::human_mapping::assert_matches_human_mapping`, a generic comparison
* that parses the code again, computes codediff's own diff and checks it agrees with the human
* mapping. Nodes are addressed by path (kind + sibling position), not by TreeSitter node ID, since
* IDs are not stable across separate parses -- see `test::helper::path_for_node`.
*
* Keybindings:
*   Tab            switch focus between the Before and After panels
*   Up/k, Down/j   move the focused panel's cursor
*   Left/h         collapse the current node, or move to its parent if already collapsed/a leaf
*   Right/l        expand the current node, or move to its first child if already expanded
*   g / G          jump to the first / last visible node
*   m              mark the Before cursor node and the After cursor node as matching. If their
*                  kinds differ, asks for confirmation (codediff never maps different kinds
*                  together, so this will always show as a mismatch, but can be useful for
*                  exploration). If the kinds match and neither node has children, the operation
*                  (Identical/Update) is inferred automatically by comparing their text. If the
*                  kinds match and either has children, asks whether to mark it Identical (the
*                  whole subtree is unchanged) or MatchButNotIdentical (it differs somewhere)
*   M              like `m`, but also recurses into children pairwise as long as both sides
*                  have the same number of children with the same kinds; stops recursing (without
*                  error) at the first level that diverges, leaving it for manual resolution. Only
*                  the top-level pair is interactive (same rules as `m`); matched descendants are
*                  classified automatically, bottom-up: Identical only if every node beneath it is
*                  also Identical, otherwise MatchButNotIdentical -- prompting per node would be
*                  unusable for a tree of any size
*   d / D          mark the Before cursor node as deleted / deleted with its whole subtree
*   i / I          mark the After cursor node as inserted / inserted with its whole subtree
*   u              remove the mark directly on the focused cursor node
*   s              save human_mapping.json and ensure the optimal_solutions test stub exists
*   q / Esc        quit
*/
use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use clap::Parser;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tree_sitter::Node;

use codediff::test::helper::human_mapping::{self, HumanMapping, HumanMappingEntry, HumanOperation};
use codediff::test::helper::{node_for_path, path_for_node};

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Interactively build a human ground-truth AST mapping for an optimal_solutions test case"
)]
struct Args {
    /// Name of the test case, i.e. the directory name under src/test/data/diffs/ (e.g.
    /// "rust-add-if"). Always starts with a language prefix.
    name: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let name = args.name;

    let mut pairs = codediff::test::helper::handmade_test_code_pairs()
        .context("Failed to load test code pairs from src/test/data/diffs")?;

    let (before, after) = pairs.remove(&name).ok_or_else(|| {
        let mut available: Vec<_> = pairs.keys().cloned().collect();
        available.sort();
        anyhow!(
            "No test case named '{}' found in src/test/data/diffs.\nAvailable: {}",
            name,
            available.join(", ")
        )
    })?;

    let before_tree = before
        .ast
        .as_ref()
        .context("Before code has no AST (unsupported or undetected language)")?;
    let after_tree = after
        .ast
        .as_ref()
        .context("After code has no AST (unsupported or undetected language)")?;
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();

    let mapping = human_mapping::load(&name).unwrap_or_default();

    let mut app = App::new(before_root.id(), after_root.id(), mapping);

    let panic_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        panic_hook(info);
    }));

    let mut terminal = setup_terminal()?;
    let result = run_event_loop(
        &mut terminal,
        &mut app,
        &name,
        before_root,
        after_root,
        before.contents.as_bytes(),
        after.contents.as_bytes(),
    );
    restore_terminal()?;

    result
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, cursor::Hide)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn restore_terminal() -> Result<()> {
    if crossterm::terminal::is_raw_mode_enabled()? {
        execute!(io::stdout(), LeaveAlternateScreen, cursor::Show)?;
        disable_raw_mode()?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Before,
    After,
}

impl Focus {
    fn toggle(self) -> Self {
        match self {
            Focus::Before => Focus::After,
            Focus::After => Focus::Before,
        }
    }
}

struct PanelState {
    cursor_id: usize,
    collapsed: std::collections::HashSet<usize>,
    scroll: usize,
}

impl PanelState {
    fn new(root_id: usize) -> Self {
        Self {
            cursor_id: root_id,
            collapsed: std::collections::HashSet::new(),
            scroll: 0,
        }
    }
}

/// A blocking prompt raised by `m`/`M` that needs a direct human answer before the mapping entry
/// can be finalized. While `App::modal` is `Some`, the event loop routes keys to
/// `handle_modal_key` instead of the normal keybindings.
#[derive(Debug, Clone)]
enum Modal {
    /// The before and after cursor nodes have different kinds. Shown before adding any mapping
    /// for them, since codediff itself never maps nodes of different kinds together (see
    /// `ASTDiff::is_valid`), so confirming this will always show up as a mismatch against
    /// codediff's actual diff -- which is fine for exploration, but worth confirming explicitly.
    ConfirmKindMismatch {
        before_id: usize,
        after_id: usize,
        before_kind: String,
        after_kind: String,
        /// Whether this originated from `M` (recursive), in which case confirming also
        /// auto-matches the rest of the subtree.
        recursive: bool,
    },
    /// The before and after cursor nodes have matching kinds and at least one has children, so
    /// there's no simple text comparison to fall back on: the human has to say whether the
    /// subtree is fully unchanged or not.
    ChooseMatchKind {
        before_id: usize,
        after_id: usize,
        before_kind: String,
        after_kind: String,
        recursive: bool,
    },
}

struct App {
    focus: Focus,
    before: PanelState,
    after: PanelState,
    mapping: HumanMapping,
    dirty: bool,
    status: Option<String>,
    modal: Option<Modal>,
    should_quit: bool,
}

impl App {
    fn new(before_root_id: usize, after_root_id: usize, mapping: HumanMapping) -> Self {
        Self {
            focus: Focus::Before,
            before: PanelState::new(before_root_id),
            after: PanelState::new(after_root_id),
            mapping,
            dirty: false,
            status: Some(
                "Loaded. m match, d/D delete, i/I insert, u unmark, s save, q quit.".to_string(),
            ),
            modal: None,
            should_quit: false,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Tree flattening & node status
// ---------------------------------------------------------------------------------------------

/// Flattens a tree into preorder (node, depth) pairs, skipping the children of collapsed nodes.
fn flatten_visible<'a>(root: Node<'a>, collapsed: &std::collections::HashSet<usize>) -> Vec<(Node<'a>, usize)> {
    let mut out = Vec::new();
    walk_visible(root, 0, collapsed, &mut out);
    out
}

fn walk_visible<'a>(
    node: Node<'a>,
    depth: usize,
    collapsed: &std::collections::HashSet<usize>,
    out: &mut Vec<(Node<'a>, usize)>,
) {
    out.push((node, depth));
    if collapsed.contains(&node.id()) {
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_visible(child, depth + 1, collapsed, out);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeStatus {
    Unmarked,
    Matched,
    /// `inherited` is true when this node isn't marked directly but an ancestor is marked
    /// with `with_children`, implying this node too.
    Marked { with_children: bool, inherited: bool },
}

/// Resolved node IDs for every human-authored entry, used to look up a node's status in O(1)
/// (plus a bounded ancestor walk for inheritance) while rendering.
#[derive(Default)]
struct Caches {
    before_match: HashMap<usize, usize>,
    after_match: HashMap<usize, usize>,
    before_removed: HashMap<usize, bool>,
    after_removed: HashMap<usize, bool>,
    /// Number of entries that couldn't be resolved against the current trees (e.g. a
    /// hand-edited or stale mapping file). Surfaced in the footer rather than treated as fatal,
    /// so a bad mapping file doesn't prevent the TUI from even opening.
    unresolved: usize,
}

fn path_refs(path: &[String]) -> Vec<&str> {
    path.iter().map(String::as_str).collect()
}

/// Builds lookup caches from `entries`, skipping (and counting) any entry that doesn't resolve
/// against the current trees rather than failing outright.
fn rebuild_caches(entries: &[HumanMappingEntry], before_root: Node, after_root: Node) -> Caches {
    let mut caches = Caches::default();

    for entry in entries {
        let resolved = match entry.operation {
            HumanOperation::Identical | HumanOperation::Update | HumanOperation::MatchButNotIdentical => (|| {
                let before_path = entry.before_path.as_ref()?;
                let after_path = entry.after_path.as_ref()?;
                let b = node_for_path(before_root, &path_refs(before_path)).ok()?;
                let a = node_for_path(after_root, &path_refs(after_path)).ok()?;
                caches.before_match.insert(b.id(), a.id());
                caches.after_match.insert(a.id(), b.id());
                Some(())
            })(),
            HumanOperation::Delete | HumanOperation::DeleteWithChildren => (|| {
                let before_path = entry.before_path.as_ref()?;
                let b = node_for_path(before_root, &path_refs(before_path)).ok()?;
                caches
                    .before_removed
                    .insert(b.id(), entry.operation == HumanOperation::DeleteWithChildren);
                Some(())
            })(),
            HumanOperation::Insert | HumanOperation::InsertWithChildren => (|| {
                let after_path = entry.after_path.as_ref()?;
                let a = node_for_path(after_root, &path_refs(after_path)).ok()?;
                caches
                    .after_removed
                    .insert(a.id(), entry.operation == HumanOperation::InsertWithChildren);
                Some(())
            })(),
        };

        if resolved.is_none() {
            caches.unresolved += 1;
        }
    }

    caches
}

/// True if some strict ancestor of `node` is marked with `with_children = true` in `removed`.
fn is_inherited_removed(node: Node, removed: &HashMap<usize, bool>) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if removed.get(&parent.id()) == Some(&true) {
            return true;
        }
        current = parent;
    }
    false
}

fn status_before(node: Node, caches: &Caches) -> NodeStatus {
    if caches.before_match.contains_key(&node.id()) {
        return NodeStatus::Matched;
    }
    if let Some(&with_children) = caches.before_removed.get(&node.id()) {
        return NodeStatus::Marked {
            with_children,
            inherited: false,
        };
    }
    if is_inherited_removed(node, &caches.before_removed) {
        return NodeStatus::Marked {
            with_children: true,
            inherited: true,
        };
    }
    NodeStatus::Unmarked
}

fn status_after(node: Node, caches: &Caches) -> NodeStatus {
    if caches.after_match.contains_key(&node.id()) {
        return NodeStatus::Matched;
    }
    if let Some(&with_children) = caches.after_removed.get(&node.id()) {
        return NodeStatus::Marked {
            with_children,
            inherited: false,
        };
    }
    if is_inherited_removed(node, &caches.after_removed) {
        return NodeStatus::Marked {
            with_children: true,
            inherited: true,
        };
    }
    NodeStatus::Unmarked
}

// ---------------------------------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------------------------------

fn move_cursor(panel: &mut PanelState, flat: &[(Node, usize)], delta: i32) {
    if flat.is_empty() {
        return;
    }
    let idx = flat
        .iter()
        .position(|(n, _)| n.id() == panel.cursor_id)
        .unwrap_or(0);
    let new_idx = (idx as i32 + delta).clamp(0, flat.len() as i32 - 1) as usize;
    panel.cursor_id = flat[new_idx].0.id();
}

fn jump_to_edge(panel: &mut PanelState, flat: &[(Node, usize)], to_start: bool) {
    let edge = if to_start { flat.first() } else { flat.last() };
    if let Some((node, _)) = edge {
        panel.cursor_id = node.id();
    }
}

fn expand_or_descend(panel: &mut PanelState, flat: &[(Node, usize)]) {
    let Some((node, _)) = flat.iter().find(|(n, _)| n.id() == panel.cursor_id) else {
        return;
    };
    let node = *node;
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

fn collapse_or_ascend(panel: &mut PanelState, flat: &[(Node, usize)]) {
    let Some((node, _)) = flat.iter().find(|(n, _)| n.id() == panel.cursor_id) else {
        return;
    };
    let node = *node;
    if node.child_count() > 0 && !panel.collapsed.contains(&node.id()) {
        panel.collapsed.insert(node.id());
        return;
    }
    if let Some(parent) = node.parent() {
        panel.cursor_id = parent.id();
    }
}

fn ensure_visible(scroll: &mut usize, cursor_idx: usize, viewport_height: usize) {
    let viewport_height = viewport_height.max(1);
    if cursor_idx < *scroll {
        *scroll = cursor_idx;
    } else if cursor_idx >= *scroll + viewport_height {
        *scroll = cursor_idx + 1 - viewport_height;
    }
}

// ---------------------------------------------------------------------------------------------
// Marking actions
// ---------------------------------------------------------------------------------------------

/// Removes any existing entry whose before_path resolves to `before_id`, or whose after_path
/// resolves to `after_id`, so that re-marking a node cleanly replaces its previous decision
/// instead of leaving stale/contradictory entries behind.
fn remove_direct_entries_for(
    entries: &mut Vec<HumanMappingEntry>,
    before_id: Option<usize>,
    after_id: Option<usize>,
    before_root: Node,
    after_root: Node,
) {
    entries.retain(|entry| {
        let touches_before = before_id.is_some()
            && entry
                .before_path
                .as_ref()
                .and_then(|p| node_for_path(before_root, &path_refs(p)).ok())
                .map(|n| Some(n.id()) == before_id)
                .unwrap_or(false);
        let touches_after = after_id.is_some()
            && entry
                .after_path
                .as_ref()
                .and_then(|p| node_for_path(after_root, &path_refs(p)).ok())
                .map(|n| Some(n.id()) == after_id)
                .unwrap_or(false);
        !(touches_before || touches_after)
    });
}

fn find_node_by_id<'a>(flat: &[(Node<'a>, usize)], id: usize) -> Option<Node<'a>> {
    flat.iter().find(|(n, _)| n.id() == id).map(|(n, _)| *n)
}

fn is_strict_descendant_of(node: Node, ancestor: Node) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.id() == ancestor.id() {
            return true;
        }
        current = parent;
    }
    false
}

/// Drops any entry whose before_path resolves to a strict descendant of `ancestor`. Used when
/// marking `ancestor` deleted-with-children, so a previous direct mark on one of its descendants
/// (e.g. a Match) can't survive alongside it and export a self-contradictory mapping.
fn clear_before_descendants(entries: &mut Vec<HumanMappingEntry>, ancestor: Node, before_root: Node) {
    entries.retain(|entry| {
        !entry
            .before_path
            .as_ref()
            .and_then(|p| node_for_path(before_root, &path_refs(p)).ok())
            .map(|n| is_strict_descendant_of(n, ancestor))
            .unwrap_or(false)
    });
}

/// Same as [`clear_before_descendants`], but for the After tree (used by insert-with-children).
fn clear_after_descendants(entries: &mut Vec<HumanMappingEntry>, ancestor: Node, after_root: Node) {
    entries.retain(|entry| {
        !entry
            .after_path
            .as_ref()
            .and_then(|p| node_for_path(after_root, &path_refs(p)).ok())
            .map(|n| is_strict_descendant_of(n, ancestor))
            .unwrap_or(false)
    });
}

/// What an `m`/`M` press should do next: either it's fully resolved (a mapping entry was added,
/// or nothing needed to change), or it needs a direct human answer before anything is written.
enum ActionOutcome {
    Done(String),
    NeedsModal(Modal),
}

/// True if `b` and `a` have the exact same text.
fn node_values_equal(b: Node, a: Node, before_src: &[u8], after_src: &[u8]) -> bool {
    b.utf8_text(before_src).unwrap_or("") == a.utf8_text(after_src).unwrap_or("")
}

/// Replaces any existing direct entry touching `b` or `a` with a single new entry pairing them
/// under `operation`.
fn apply_match_entry(
    mapping: &mut HumanMapping,
    before_root: Node,
    after_root: Node,
    b: Node,
    a: Node,
    operation: HumanOperation,
) {
    remove_direct_entries_for(&mut mapping.entries, Some(b.id()), Some(a.id()), before_root, after_root);
    mapping.entries.push(HumanMappingEntry {
        operation,
        before_path: Some(path_for_node(b)),
        after_path: Some(path_for_node(a)),
    });
}

fn action_match(
    mapping: &mut HumanMapping,
    before_flat: &[(Node, usize)],
    after_flat: &[(Node, usize)],
    before_cursor: usize,
    after_cursor: usize,
    before_root: Node,
    after_root: Node,
    caches: &Caches,
    before_src: &[u8],
    after_src: &[u8],
) -> Result<ActionOutcome> {
    let before_node =
        find_node_by_id(before_flat, before_cursor).context("Before cursor node not found")?;
    let after_node =
        find_node_by_id(after_flat, after_cursor).context("After cursor node not found")?;

    if is_inherited_removed(before_node, &caches.before_removed) {
        bail!("Before node is covered by an ancestor's delete-with-children mark; clear that first (u on the ancestor)");
    }
    if is_inherited_removed(after_node, &caches.after_removed) {
        bail!("After node is covered by an ancestor's insert-with-children mark; clear that first (u on the ancestor)");
    }

    if before_node.kind() != after_node.kind() {
        return Ok(ActionOutcome::NeedsModal(Modal::ConfirmKindMismatch {
            before_id: before_node.id(),
            after_id: after_node.id(),
            before_kind: before_node.kind().to_string(),
            after_kind: after_node.kind().to_string(),
            recursive: false,
        }));
    }

    if before_node.child_count() == 0 && after_node.child_count() == 0 {
        let identical = node_values_equal(before_node, after_node, before_src, after_src);
        let operation = if identical {
            HumanOperation::Identical
        } else {
            HumanOperation::Update
        };
        apply_match_entry(mapping, before_root, after_root, before_node, after_node, operation);
        return Ok(ActionOutcome::Done(format!(
            "Matched '{}' <-> '{}' as {}",
            before_node.kind(),
            after_node.kind(),
            if identical { "Identical" } else { "Update" }
        )));
    }

    Ok(ActionOutcome::NeedsModal(Modal::ChooseMatchKind {
        before_id: before_node.id(),
        after_id: after_node.id(),
        before_kind: before_node.kind().to_string(),
        after_kind: after_node.kind().to_string(),
        recursive: false,
    }))
}

#[allow(clippy::too_many_arguments)]
fn action_match_subtree(
    mapping: &mut HumanMapping,
    before_flat: &[(Node, usize)],
    after_flat: &[(Node, usize)],
    before_cursor: usize,
    after_cursor: usize,
    before_root: Node,
    after_root: Node,
    caches: &Caches,
    before_src: &[u8],
    after_src: &[u8],
) -> Result<ActionOutcome> {
    let before_node =
        find_node_by_id(before_flat, before_cursor).context("Before cursor node not found")?;
    let after_node =
        find_node_by_id(after_flat, after_cursor).context("After cursor node not found")?;

    if is_inherited_removed(before_node, &caches.before_removed) {
        bail!("Before node is covered by an ancestor's delete-with-children mark; clear that first (u on the ancestor)");
    }
    if is_inherited_removed(after_node, &caches.after_removed) {
        bail!("After node is covered by an ancestor's insert-with-children mark; clear that first (u on the ancestor)");
    }

    if before_node.kind() != after_node.kind() {
        return Ok(ActionOutcome::NeedsModal(Modal::ConfirmKindMismatch {
            before_id: before_node.id(),
            after_id: after_node.id(),
            before_kind: before_node.kind().to_string(),
            after_kind: after_node.kind().to_string(),
            recursive: true,
        }));
    }

    // A leaf top pair has no children to auto-fill, so resolve it immediately like `m` does
    // instead of opening a pointless "Identical or MatchButNotIdentical?" modal.
    if before_node.child_count() == 0 && after_node.child_count() == 0 {
        let identical = node_values_equal(before_node, after_node, before_src, after_src);
        let operation = if identical {
            HumanOperation::Identical
        } else {
            HumanOperation::Update
        };
        apply_match_entry(mapping, before_root, after_root, before_node, after_node, operation);
        return Ok(ActionOutcome::Done(format!(
            "Matched '{}' <-> '{}' as {}",
            before_node.kind(),
            after_node.kind(),
            if identical { "Identical" } else { "Update" }
        )));
    }

    Ok(ActionOutcome::NeedsModal(Modal::ChooseMatchKind {
        before_id: before_node.id(),
        after_id: after_node.id(),
        before_kind: before_node.kind().to_string(),
        after_kind: after_node.kind().to_string(),
        recursive: true,
    }))
}

/// Auto-matches `b` <-> `a` and all descendants, with no prompting: leaves are classified
/// Identical/Update by comparing text; container nodes are classified Identical only if every
/// descendant came back Identical too, otherwise MatchButNotIdentical. Recursion stops (without
/// matching further) the moment a level's child-kind sequences diverge, or a node is already
/// covered by an unrelated ancestor mark. Returns whether the whole subtree matched Identically.
///
/// Used to bulk-fill the rest of an `M` (recursive match) after the top-level pair's own
/// operation has already been decided (automatically, or via a `Modal::ChooseMatchKind` answer) --
/// prompting per node for a tree with hundreds of nodes would be unusable.
#[allow(clippy::too_many_arguments)]
fn auto_match_pair(
    mapping: &mut HumanMapping,
    before_root: Node,
    after_root: Node,
    caches: &Caches,
    b: Node,
    a: Node,
    before_src: &[u8],
    after_src: &[u8],
    matched: &mut usize,
    skipped: &mut usize,
) -> bool {
    if is_inherited_removed(b, &caches.before_removed) || is_inherited_removed(a, &caches.after_removed) {
        *skipped += 1;
        return false;
    }

    if b.kind() != a.kind() {
        // Shouldn't happen for children reached via the same_shape check below, but the very
        // first call into this function (the top pair's children) hasn't been shape-checked yet.
        apply_match_entry(mapping, before_root, after_root, b, a, HumanOperation::MatchButNotIdentical);
        *matched += 1;
        return false;
    }

    let mut b_cursor = b.walk();
    let b_children: Vec<Node> = b.children(&mut b_cursor).collect();
    let mut a_cursor = a.walk();
    let a_children: Vec<Node> = a.children(&mut a_cursor).collect();

    if b_children.is_empty() && a_children.is_empty() {
        let identical = node_values_equal(b, a, before_src, after_src);
        apply_match_entry(
            mapping,
            before_root,
            after_root,
            b,
            a,
            if identical { HumanOperation::Identical } else { HumanOperation::Update },
        );
        *matched += 1;
        return identical;
    }

    let same_shape = b_children.len() == a_children.len()
        && b_children.iter().zip(&a_children).all(|(x, y)| x.kind() == y.kind());

    if !same_shape {
        apply_match_entry(mapping, before_root, after_root, b, a, HumanOperation::MatchButNotIdentical);
        *matched += 1;
        return false;
    }

    let mut all_identical = true;
    for (b_child, a_child) in b_children.into_iter().zip(a_children) {
        let child_identical = auto_match_pair(
            mapping, before_root, after_root, caches, b_child, a_child, before_src, after_src, matched, skipped,
        );
        all_identical &= child_identical;
    }

    apply_match_entry(
        mapping,
        before_root,
        after_root,
        b,
        a,
        if all_identical { HumanOperation::Identical } else { HumanOperation::MatchButNotIdentical },
    );
    *matched += 1;
    all_identical
}

/// Finishes resolving a `Modal::ConfirmKindMismatch` or `Modal::ChooseMatchKind`: applies the
/// chosen operation to the top pair, and if `recursive`, auto-fills the rest of the subtree via
/// [`auto_match_pair`].
#[allow(clippy::too_many_arguments)]
fn apply_modal_choice(
    mapping: &mut HumanMapping,
    before_flat: &[(Node, usize)],
    after_flat: &[(Node, usize)],
    before_root: Node,
    after_root: Node,
    caches: &Caches,
    before_src: &[u8],
    after_src: &[u8],
    before_id: usize,
    after_id: usize,
    operation: HumanOperation,
    recursive: bool,
) -> String {
    let (Some(b), Some(a)) = (find_node_by_id(before_flat, before_id), find_node_by_id(after_flat, after_id))
    else {
        return "Node no longer available (tree changed?)".to_string();
    };

    apply_match_entry(mapping, before_root, after_root, b, a, operation);

    if !recursive {
        return format!("Matched '{}' <-> '{}' as {:?}", b.kind(), a.kind(), operation);
    }

    let mut matched = 1usize;
    let mut skipped = 0usize;

    let mut b_cursor = b.walk();
    let b_children: Vec<Node> = b.children(&mut b_cursor).collect();
    let mut a_cursor = a.walk();
    let a_children: Vec<Node> = a.children(&mut a_cursor).collect();
    let same_shape = b_children.len() == a_children.len()
        && b_children.iter().zip(&a_children).all(|(x, y)| x.kind() == y.kind());

    if same_shape {
        for (b_child, a_child) in b_children.into_iter().zip(a_children) {
            auto_match_pair(
                mapping, before_root, after_root, caches, b_child, a_child, before_src, after_src, &mut matched,
                &mut skipped,
            );
        }
    }

    if skipped > 0 {
        format!(
            "Matched {} node pair(s) under '{}' <-> '{}'; skipped {} node(s) already covered by an unrelated ancestor mark",
            matched,
            b.kind(),
            a.kind(),
            skipped
        )
    } else {
        format!("Matched {} node pair(s) under '{}' <-> '{}'", matched, b.kind(), a.kind())
    }
}

fn action_delete(
    mapping: &mut HumanMapping,
    before_flat: &[(Node, usize)],
    before_cursor: usize,
    before_root: Node,
    after_root: Node,
    with_children: bool,
    caches: &Caches,
) -> Result<String> {
    let before_node =
        find_node_by_id(before_flat, before_cursor).context("Before cursor node not found")?;

    if is_inherited_removed(before_node, &caches.before_removed) {
        bail!("Node is already covered by an ancestor's delete-with-children mark; clear that first (u on the ancestor)");
    }

    remove_direct_entries_for(&mut mapping.entries, Some(before_node.id()), None, before_root, after_root);
    if with_children {
        clear_before_descendants(&mut mapping.entries, before_node, before_root);
    }
    mapping.entries.push(HumanMappingEntry {
        operation: if with_children {
            HumanOperation::DeleteWithChildren
        } else {
            HumanOperation::Delete
        },
        before_path: Some(path_for_node(before_node)),
        after_path: None,
    });

    Ok(format!(
        "Marked '{}' deleted{}",
        before_node.kind(),
        if with_children { " (with children)" } else { "" }
    ))
}

fn action_insert(
    mapping: &mut HumanMapping,
    after_flat: &[(Node, usize)],
    after_cursor: usize,
    before_root: Node,
    after_root: Node,
    with_children: bool,
    caches: &Caches,
) -> Result<String> {
    let after_node =
        find_node_by_id(after_flat, after_cursor).context("After cursor node not found")?;

    if is_inherited_removed(after_node, &caches.after_removed) {
        bail!("Node is already covered by an ancestor's insert-with-children mark; clear that first (u on the ancestor)");
    }

    remove_direct_entries_for(&mut mapping.entries, None, Some(after_node.id()), before_root, after_root);
    if with_children {
        clear_after_descendants(&mut mapping.entries, after_node, after_root);
    }
    mapping.entries.push(HumanMappingEntry {
        operation: if with_children {
            HumanOperation::InsertWithChildren
        } else {
            HumanOperation::Insert
        },
        before_path: None,
        after_path: Some(path_for_node(after_node)),
    });

    Ok(format!(
        "Marked '{}' inserted{}",
        after_node.kind(),
        if with_children { " (with children)" } else { "" }
    ))
}

fn action_unmark(
    mapping: &mut HumanMapping,
    focus: Focus,
    before_flat: &[(Node, usize)],
    after_flat: &[(Node, usize)],
    before_cursor: usize,
    after_cursor: usize,
    before_root: Node,
    after_root: Node,
    caches: &Caches,
) -> Result<String> {
    let (id, node, removed) = match focus {
        Focus::Before => (
            before_cursor,
            find_node_by_id(before_flat, before_cursor).context("Before cursor node not found")?,
            &caches.before_removed,
        ),
        Focus::After => (
            after_cursor,
            find_node_by_id(after_flat, after_cursor).context("After cursor node not found")?,
            &caches.after_removed,
        ),
    };

    let before_id = if focus == Focus::Before { Some(id) } else { None };
    let after_id = if focus == Focus::After { Some(id) } else { None };

    let before_len = mapping.entries.len();
    remove_direct_entries_for(&mut mapping.entries, before_id, after_id, before_root, after_root);

    if mapping.entries.len() < before_len {
        return Ok(format!("Unmarked '{}'", node.kind()));
    }

    if is_inherited_removed(node, removed) {
        bail!("This node is only covered via an ancestor's with-children mark; clear the ancestor instead");
    }

    Ok(format!("'{}' was not marked", node.kind()))
}

// ---------------------------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Side {
    Before,
    After,
}

fn node_label(node: Node, src: &[u8]) -> String {
    if node.child_count() == 0 {
        let text = node.utf8_text(src).unwrap_or("");
        let truncated: String = text.chars().take(40).collect();
        let ellipsis = if text.chars().count() > 40 { "..." } else { "" };
        format!("{} {:?}{}", node.kind(), truncated, ellipsis)
    } else {
        node.kind().to_string()
    }
}

fn status_glyph_and_style(status: NodeStatus) -> (&'static str, Style) {
    match status {
        NodeStatus::Unmarked => (" ", Style::default().fg(Color::Gray)),
        NodeStatus::Matched => ("M", Style::default().fg(Color::Cyan)),
        NodeStatus::Marked {
            with_children: false,
            inherited: false,
        } => ("x", Style::default().fg(Color::Red)),
        NodeStatus::Marked {
            with_children: true,
            inherited: false,
        } => ("X", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        NodeStatus::Marked { inherited: true, .. } => (
            "x",
            Style::default().fg(Color::Red).add_modifier(Modifier::DIM),
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn render_panel(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    flat: &[(Node, usize)],
    panel: &mut PanelState,
    caches: &Caches,
    side: Side,
    src: &[u8],
    focused: bool,
) {
    let inner_height = area.height.saturating_sub(2) as usize;
    let cursor_idx = flat
        .iter()
        .position(|(n, _)| n.id() == panel.cursor_id)
        .unwrap_or(0);
    ensure_visible(&mut panel.scroll, cursor_idx, inner_height);

    let items: Vec<ListItem> = flat
        .iter()
        .enumerate()
        .skip(panel.scroll)
        .take(inner_height.max(1))
        .map(|(idx, (node, depth))| {
            let status = match side {
                Side::Before => status_before(*node, caches),
                Side::After => status_after(*node, caches),
            };
            let (glyph, mut style) = status_glyph_and_style(status);
            let indent = "  ".repeat(*depth);
            let text = format!("{}{} {}", indent, glyph, node_label(*node, src));

            if idx == cursor_idx {
                style = style
                    .bg(if focused { Color::Yellow } else { Color::DarkGray })
                    .fg(Color::Black);
            }

            ListItem::new(Line::from(Span::styled(text, style)))
        })
        .collect();

    // Count unmarked nodes across the *whole* tree, not just the visible page, for the header.
    let total_unmarked = flat
        .iter()
        .filter(|(node, _)| {
            let status = match side {
                Side::Before => status_before(*node, caches),
                Side::After => status_after(*node, caches),
            };
            status == NodeStatus::Unmarked
        })
        .count();

    let border_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            "{} — {} nodes, {} unmarked",
            title,
            flat.len(),
            total_unmarked
        ))
        .border_style(border_style);

    frame.render_widget(List::new(items).block(block), area);
}

fn draw_ui(
    frame: &mut Frame,
    app: &mut App,
    before_flat: &[(Node, usize)],
    after_flat: &[(Node, usize)],
    caches: &Caches,
    before_src: &[u8],
    after_src: &[u8],
    name: &str,
) {
    let size = frame.size();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(3), Constraint::Length(2)])
        .split(size);

    frame.render_widget(
        Paragraph::new(format!(" human_solver — {} ", name))
            .style(Style::default().add_modifier(Modifier::BOLD)),
        chunks[0],
    );

    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    render_panel(
        frame,
        panels[0],
        "Before",
        before_flat,
        &mut app.before,
        caches,
        Side::Before,
        before_src,
        app.focus == Focus::Before,
    );
    render_panel(
        frame,
        panels[1],
        "After",
        after_flat,
        &mut app.after,
        caches,
        Side::After,
        after_src,
        app.focus == Focus::After,
    );

    let footer = format!(
        "{}{}{}\nm/M match[+children]  d/D delete[+children]  i/I insert[+children]  u unmark  h/l ←/→ collapse/expand  j/k ↑/↓ move  g/G top/bottom  Tab switch  s save  q quit",
        app.status.clone().unwrap_or_default(),
        if app.dirty { "  [UNSAVED]" } else { "" },
        if caches.unresolved > 0 {
            format!(
                "  [{} mapping entries could not be resolved against the current tree and were ignored]",
                caches.unresolved
            )
        } else {
            String::new()
        },
    );
    frame.render_widget(Paragraph::new(footer).wrap(Wrap { trim: true }), chunks[2]);

    if let Some(modal) = &app.modal {
        render_modal(frame, size, modal);
    }
}

/// A `percent_x` x `percent_y` box centered within `area`.
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn render_modal(frame: &mut Frame, area: Rect, modal: &Modal) {
    let (title, body) = match modal {
        Modal::ConfirmKindMismatch {
            before_kind,
            after_kind,
            ..
        } => (
            "Node kinds do not match!",
            format!(
                "Before: {}\nAfter:  {}\n\nAre you sure you want to add this mapping? (y/n)",
                before_kind, after_kind
            ),
        ),
        Modal::ChooseMatchKind {
            before_kind,
            after_kind,
            ..
        } => (
            "Choose match type",
            format!(
                "'{}' <-> '{}' both have children.\n\n[y] Identical    [n] MatchButNotIdentical    [Esc] Cancel",
                before_kind, after_kind
            ),
        ),
    };

    let popup_area = centered_rect(60, 30, area);
    frame.render_widget(Clear, popup_area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
    frame.render_widget(Paragraph::new(body).block(block).wrap(Wrap { trim: true }), popup_area);
}

// ---------------------------------------------------------------------------------------------
// Event loop
// ---------------------------------------------------------------------------------------------

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    name: &str,
    before_root: Node,
    after_root: Node,
    before_src: &[u8],
    after_src: &[u8],
) -> Result<()> {
    loop {
        let before_flat = flatten_visible(before_root, &app.before.collapsed);
        let after_flat = flatten_visible(after_root, &app.after.collapsed);
        let caches = rebuild_caches(&app.mapping.entries, before_root, after_root);

        terminal.draw(|f| {
            draw_ui(
                f,
                app,
                &before_flat,
                &after_flat,
                &caches,
                before_src,
                after_src,
                name,
            )
        })?;

        if event::poll(Duration::from_millis(250))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            if app.modal.is_some() {
                handle_modal_key(
                    app,
                    key.code,
                    &before_flat,
                    &after_flat,
                    before_root,
                    after_root,
                    &caches,
                    before_src,
                    after_src,
                );
            } else {
                handle_key(
                    app,
                    key.code,
                    &before_flat,
                    &after_flat,
                    before_root,
                    after_root,
                    &caches,
                    name,
                    before_src,
                    after_src,
                );
            }
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_key(
    app: &mut App,
    code: KeyCode,
    before_flat: &[(Node, usize)],
    after_flat: &[(Node, usize)],
    before_root: Node,
    after_root: Node,
    caches: &Caches,
    name: &str,
    before_src: &[u8],
    after_src: &[u8],
) {
    let focus = app.focus;

    let result: Option<Result<String>> = match code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.should_quit = true;
            None
        }
        KeyCode::Tab => {
            app.focus = focus.toggle();
            None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            let (panel, flat) = match focus {
                Focus::Before => (&mut app.before, before_flat),
                Focus::After => (&mut app.after, after_flat),
            };
            move_cursor(panel, flat, -1);
            None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let (panel, flat) = match focus {
                Focus::Before => (&mut app.before, before_flat),
                Focus::After => (&mut app.after, after_flat),
            };
            move_cursor(panel, flat, 1);
            None
        }
        KeyCode::Left | KeyCode::Char('h') => {
            let (panel, flat) = match focus {
                Focus::Before => (&mut app.before, before_flat),
                Focus::After => (&mut app.after, after_flat),
            };
            collapse_or_ascend(panel, flat);
            None
        }
        KeyCode::Right | KeyCode::Char('l') => {
            let (panel, flat) = match focus {
                Focus::Before => (&mut app.before, before_flat),
                Focus::After => (&mut app.after, after_flat),
            };
            expand_or_descend(panel, flat);
            None
        }
        KeyCode::Char('g') => {
            let (panel, flat) = match focus {
                Focus::Before => (&mut app.before, before_flat),
                Focus::After => (&mut app.after, after_flat),
            };
            jump_to_edge(panel, flat, true);
            None
        }
        KeyCode::Char('G') => {
            let (panel, flat) = match focus {
                Focus::Before => (&mut app.before, before_flat),
                Focus::After => (&mut app.after, after_flat),
            };
            jump_to_edge(panel, flat, false);
            None
        }
        KeyCode::Char('m') => {
            match action_match(
                &mut app.mapping,
                before_flat,
                after_flat,
                app.before.cursor_id,
                app.after.cursor_id,
                before_root,
                after_root,
                caches,
                before_src,
                after_src,
            ) {
                Ok(ActionOutcome::Done(msg)) => {
                    app.dirty = true;
                    app.status = Some(msg);
                }
                Ok(ActionOutcome::NeedsModal(modal)) => app.modal = Some(modal),
                Err(err) => app.status = Some(format!("Error: {:#}", err)),
            }
            None
        }
        KeyCode::Char('M') => {
            match action_match_subtree(
                &mut app.mapping,
                before_flat,
                after_flat,
                app.before.cursor_id,
                app.after.cursor_id,
                before_root,
                after_root,
                caches,
                before_src,
                after_src,
            ) {
                Ok(ActionOutcome::Done(msg)) => {
                    app.dirty = true;
                    app.status = Some(msg);
                }
                Ok(ActionOutcome::NeedsModal(modal)) => app.modal = Some(modal),
                Err(err) => app.status = Some(format!("Error: {:#}", err)),
            }
            None
        }
        KeyCode::Char('d') | KeyCode::Char('D') => {
            if focus != Focus::Before {
                Some(Err(anyhow!(
                    "d/D only apply to the Before panel; press Tab to switch"
                )))
            } else {
                let res = action_delete(
                    &mut app.mapping,
                    before_flat,
                    app.before.cursor_id,
                    before_root,
                    after_root,
                    code == KeyCode::Char('D'),
                    caches,
                );
                if res.is_ok() {
                    app.dirty = true;
                }
                Some(res)
            }
        }
        KeyCode::Char('i') | KeyCode::Char('I') => {
            if focus != Focus::After {
                Some(Err(anyhow!(
                    "i/I only apply to the After panel; press Tab to switch"
                )))
            } else {
                let res = action_insert(
                    &mut app.mapping,
                    after_flat,
                    app.after.cursor_id,
                    before_root,
                    after_root,
                    code == KeyCode::Char('I'),
                    caches,
                );
                if res.is_ok() {
                    app.dirty = true;
                }
                Some(res)
            }
        }
        KeyCode::Char('u') => {
            let res = action_unmark(
                &mut app.mapping,
                focus,
                before_flat,
                after_flat,
                app.before.cursor_id,
                app.after.cursor_id,
                before_root,
                after_root,
                caches,
            );
            if res.is_ok() {
                app.dirty = true;
            }
            Some(res)
        }
        KeyCode::Char('s') => Some(action_save(&mut app.mapping, &mut app.dirty, name)),
        _ => None,
    };

    if let Some(res) = result {
        app.status = Some(match res {
            Ok(msg) => msg,
            Err(err) => format!("Error: {:#}", err),
        });
    }
}

/// Routes a keypress while `app.modal` is `Some`: only y/Y, n/N and Esc are understood, everything
/// else is ignored so it doesn't leak through to the normal keybindings underneath the popup.
#[allow(clippy::too_many_arguments)]
fn handle_modal_key(
    app: &mut App,
    code: KeyCode,
    before_flat: &[(Node, usize)],
    after_flat: &[(Node, usize)],
    before_root: Node,
    after_root: Node,
    caches: &Caches,
    before_src: &[u8],
    after_src: &[u8],
) {
    let Some(modal) = app.modal.take() else {
        return;
    };

    match modal {
        Modal::ConfirmKindMismatch {
            before_id,
            after_id,
            before_kind,
            after_kind,
            recursive,
        } => match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                app.dirty = true;
                app.status = Some(apply_modal_choice(
                    &mut app.mapping,
                    before_flat,
                    after_flat,
                    before_root,
                    after_root,
                    caches,
                    before_src,
                    after_src,
                    before_id,
                    after_id,
                    HumanOperation::MatchButNotIdentical,
                    recursive,
                ));
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                app.status = Some("Cancelled: node kinds do not match".to_string());
            }
            _ => {
                app.modal = Some(Modal::ConfirmKindMismatch {
                    before_id,
                    after_id,
                    before_kind,
                    after_kind,
                    recursive,
                });
            }
        },
        Modal::ChooseMatchKind {
            before_id,
            after_id,
            before_kind,
            after_kind,
            recursive,
        } => match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                app.dirty = true;
                app.status = Some(apply_modal_choice(
                    &mut app.mapping,
                    before_flat,
                    after_flat,
                    before_root,
                    after_root,
                    caches,
                    before_src,
                    after_src,
                    before_id,
                    after_id,
                    HumanOperation::Identical,
                    recursive,
                ));
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                app.dirty = true;
                app.status = Some(apply_modal_choice(
                    &mut app.mapping,
                    before_flat,
                    after_flat,
                    before_root,
                    after_root,
                    caches,
                    before_src,
                    after_src,
                    before_id,
                    after_id,
                    HumanOperation::MatchButNotIdentical,
                    recursive,
                ));
            }
            KeyCode::Esc => {
                app.status = Some("Cancelled".to_string());
            }
            _ => {
                app.modal = Some(Modal::ChooseMatchKind {
                    before_id,
                    after_id,
                    before_kind,
                    after_kind,
                    recursive,
                });
            }
        },
    }
}

fn action_save(mapping: &mut HumanMapping, dirty: &mut bool, name: &str) -> Result<String> {
    human_mapping::save(name, mapping)?;
    let created = ensure_stub_test(name)?;
    *dirty = false;
    Ok(if created {
        format!(
            "Saved human_mapping.json and created optimal_solutions/{}.rs",
            module_name(name)
        )
    } else {
        "Saved human_mapping.json".to_string()
    })
}

// ---------------------------------------------------------------------------------------------
// optimal_solutions test stub generation
// ---------------------------------------------------------------------------------------------

const LICENSE_HEADER: &str = "/*  This file is part of the CodeDiff code diffing tool.
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
";

fn module_name(name: &str) -> String {
    name.replace('-', "_")
}

fn optimal_solutions_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("optimal_solutions")
}

fn optimal_solutions_mod_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("optimal_solutions.rs")
}

/// Creates `optimal_solutions/<name>.rs` if it doesn't already exist, and makes sure it's
/// registered in `optimal_solutions.rs`. Returns whether the stub `.rs` file was newly created.
fn ensure_stub_test(name: &str) -> Result<bool> {
    let module = module_name(name);
    let stub_path = optimal_solutions_dir().join(format!("{module}.rs"));

    let created = if stub_path.exists() {
        false
    } else {
        let contents = format!(
            "{LICENSE_HEADER}use anyhow::Result;\n\nuse crate::test;\n\n#[test]\nfn optimal_solution() -> Result<()> {{\n    test::helper::human_mapping::assert_matches_human_mapping(\"{name}\")\n}}\n"
        );
        fs::write(&stub_path, contents)
            .with_context(|| format!("writing stub test to {:?}", stub_path))?;
        true
    };

    insert_mod_declaration(&module)?;

    Ok(created)
}

/// Adds `#[cfg(test)]\nmod <module>;` to optimal_solutions.rs, keeping the list sorted, unless
/// it's already present.
fn insert_mod_declaration(module: &str) -> Result<()> {
    let mod_file = optimal_solutions_mod_file();
    let content = fs::read_to_string(&mod_file)
        .with_context(|| format!("reading {:?}", mod_file))?;

    let mut lines = content.lines().peekable();
    let mut header_lines = Vec::new();
    while let Some(&line) = lines.peek() {
        if line.trim() == "#[cfg(test)]" {
            break;
        }
        header_lines.push(line.to_string());
        lines.next();
    }

    let mut entries: Vec<String> = Vec::new();
    while let Some(line) = lines.next() {
        if line.trim() != "#[cfg(test)]" {
            continue;
        }
        let mod_line = lines
            .next()
            .with_context(|| format!("'#[cfg(test)]' not followed by a mod line in {:?}", mod_file))?;
        let trimmed = mod_line.trim();
        let mod_name = trimmed
            .strip_prefix("mod ")
            .and_then(|rest| rest.strip_suffix(';'))
            .with_context(|| format!("unexpected line after '#[cfg(test)]' in {:?}: {:?}", mod_file, mod_line))?;
        entries.push(mod_name.to_string());
    }

    if !entries.iter().any(|e| e == module) {
        entries.push(module.to_string());
        entries.sort();
    }

    let mut out = header_lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    for entry in &entries {
        out.push_str("#[cfg(test)]\n");
        out.push_str(&format!("mod {entry};\n"));
    }

    fs::write(&mod_file, out).with_context(|| format!("writing {:?}", mod_file))?;
    Ok(())
}
