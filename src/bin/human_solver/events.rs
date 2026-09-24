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
//! The key dispatch: one arm per keybinding, and the modal handler.

use crate::*;

// ---------------------------------------------------------------------------------------------
// Event loop
// ---------------------------------------------------------------------------------------------

/// Everything derived from the current trees, mapping, and collapse/hide state that drawing a
/// frame or interpreting a keystroke needs. Rebuilding it costs several whole-tree passes, so
/// `run_case_session` caches it across keys that cannot change it.
pub(crate) struct FrameState<'a> {
    /// `None` in text-only mode: a language with no tree-sitter grammar (a `BUILD` file, say).
    /// The flat indexes are then empty and tree-reading keys never reach `handle_key`; painting
    /// still works on the raw text. Both roots are `Some` or both `None`.
    pub(crate) before_root: Option<Node<'a>>,
    pub(crate) after_root: Option<Node<'a>>,
    pub(crate) before_src: &'a [u8],
    pub(crate) after_src: &'a [u8],
    pub(crate) caches: Caches,
    pub(crate) before_flat: FlatIndex<'a>,
    pub(crate) after_flat: FlatIndex<'a>,
    /// Counts of `Unmarked` nodes in `before_flat`/`after_flat`, for `render_panel`'s "N unmarked"
    /// header. Kept here because counting walks every node, not just the visible ones.
    pub(crate) before_unmarked: usize,
    pub(crate) after_unmarked: usize,
}

pub(crate) fn count_unmarked(
    flat: &[(Node, usize)],
    caches: &Caches,
    status_fn: fn(Node, &Caches) -> NodeStatus,
) -> usize {
    flat.iter()
        .filter(|(n, _)| status_fn(*n, caches) == NodeStatus::Unmarked)
        .count()
}

pub(crate) fn compute_frame_state<'a>(
    before: &'a Code,
    after: &'a Code,
    app: &App,
) -> Result<FrameState<'a>> {
    let before_src = before.contents.as_bytes();
    let after_src = after.contents.as_bytes();

    let (Some(before_tree), Some(after_tree)) = (before.ast.as_ref(), after.ast.as_ref()) else {
        return Ok(FrameState {
            before_root: None,
            after_root: None,
            before_src,
            after_src,
            caches: Caches::default(),
            before_flat: FlatIndex::new(Vec::new()),
            after_flat: FlatIndex::new(Vec::new()),
            before_unmarked: 0,
            after_unmarked: 0,
        });
    };
    let before_root = before_tree.root_node();
    let after_root = after_tree.root_node();

    let caches = rebuild_caches_for_mapping(&app.mapping, before_root, after_root);

    let before_hidden = app
        .hide_solved
        .then(|| fully_solved_nodes(before_root, &caches, status_before));
    let after_hidden = app
        .hide_solved
        .then(|| fully_solved_nodes(after_root, &caches, status_after));

    let before_flat = FlatIndex::new(flatten_visible(
        before_root,
        &app.before.collapsed,
        before_hidden.as_ref(),
    ));
    let after_flat = FlatIndex::new(flatten_visible(
        after_root,
        &app.after.collapsed,
        after_hidden.as_ref(),
    ));

    let before_unmarked = count_unmarked(&before_flat, &caches, status_before);
    let after_unmarked = count_unmarked(&after_flat, &caches, status_after);

    Ok(FrameState {
        before_root: Some(before_root),
        after_root: Some(after_root),
        before_src,
        after_src,
        caches,
        before_flat,
        after_flat,
        before_unmarked,
        after_unmarked,
    })
}

impl<'a> FrameState<'a> {
    /// Both trees, or `None` in text-only mode - see [`FrameState::before_root`].
    pub(crate) fn roots(&self) -> Option<(Node<'a>, Node<'a>)> {
        self.before_root.zip(self.after_root)
    }
}

/// How `run_case_session` ended: the user quit, or asked to switch to a different case.
pub(crate) enum SessionEnd {
    Quit,
    Open(OpenTarget),
}

pub(crate) fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    mut before: Code,
    mut after: Code,
) -> Result<()> {
    loop {
        // The session borrows `before`/`after` immutably, so its cached `FrameState` can live for
        // the whole session; only this loop reassigns them, between sessions.
        match run_case_session(terminal, app, &before, &after)? {
            SessionEnd::Quit => break,
            SessionEnd::Open(OpenTarget::Diffs(name)) => match load_case(&name) {
                Ok((new_before, new_after)) => {
                    let before_root_id = starting_cursor_id(&new_before);
                    let after_root_id = starting_cursor_id(&new_after);
                    before = new_before;
                    after = new_after;
                    app.mapping = human_mapping::load(&name).unwrap_or_default();
                    app.name = name;
                    app.origin = CaseOrigin::Diffs;
                    app.before = PanelState::new(before_root_id);
                    app.after = PanelState::new(after_root_id);
                    app.focus = Focus::Before;
                    app.dirty = false;
                    app.algo_diff = None;
                    app.algo_text_spans = None;
                    app.tree_text_spans = None;
                    app.text_overlay = TextOverlay::default();
                    // The previous case's solution name would start a near-duplicate painting here.
                    app.text_solution = starting_solution(&app.mapping);
                    app.clear_multi_select();
                    app.status = Some(format!("Opened '{}'", app.name));
                }
                Err(err) => {
                    app.status = Some(format!("Error opening '{}': {:#}", name, err));
                }
            },
            SessionEnd::Open(OpenTarget::Sample(name)) => match load_sample(&name) {
                Ok((new_before, new_after, source)) => {
                    let before_root_id = starting_cursor_id(&new_before);
                    let after_root_id = starting_cursor_id(&new_after);
                    before = new_before;
                    after = new_after;
                    app.mapping = HumanMapping::default();
                    app.name = name;
                    app.origin = CaseOrigin::Sample(source);
                    app.before = PanelState::new(before_root_id);
                    app.after = PanelState::new(after_root_id);
                    app.focus = Focus::Before;
                    app.dirty = false;
                    app.algo_diff = None;
                    app.algo_text_spans = None;
                    app.tree_text_spans = None;
                    app.text_overlay = TextOverlay::default();
                    app.text_solution = starting_solution(&app.mapping);
                    app.clear_multi_select();
                    app.status = Some(format!(
                        "Opened sample '{}' (press s to promote it into a test case)",
                        app.name
                    ));
                }
                Err(err) => {
                    app.status = Some(format!("Error opening sample '{}': {:#}", name, err));
                }
            },
            SessionEnd::Open(OpenTarget::GitCommitFile {
                hash,
                summary,
                path,
            }) => match load_git_commit_file(&hash, &path) {
                Ok((new_before, new_after)) => {
                    let before_root_id = starting_cursor_id(&new_before);
                    let after_root_id = starting_cursor_id(&new_after);
                    before = new_before;
                    after = new_after;
                    app.mapping = HumanMapping::default();
                    app.name = format!("{path}@{}", short_hash(&hash));
                    app.status = Some(format!(
                        "Opened '{}' from commit {} \"{}\" (press s to promote it into a test \
                         case)",
                        path,
                        short_hash(&hash),
                        summary
                    ));
                    app.origin = CaseOrigin::GitCommitFile { path };
                    app.before = PanelState::new(before_root_id);
                    app.after = PanelState::new(after_root_id);
                    app.focus = Focus::Before;
                    app.dirty = false;
                    app.algo_diff = None;
                    app.algo_text_spans = None;
                    app.tree_text_spans = None;
                    app.text_overlay = TextOverlay::default();
                    app.text_solution = starting_solution(&app.mapping);
                    app.clear_multi_select();
                }
                Err(err) => {
                    app.status = Some(format!(
                        "Error opening '{}' from commit {}: {:#}",
                        path,
                        short_hash(&hash),
                        err
                    ));
                }
            },
        }
    }
    Ok(())
}

/// The footer line for a pending multi-map selection, after `x` or `X` changes it: the counts,
/// the pairing it will be committed with, and how to commit or clear it.
pub(crate) fn multi_select_status(app: &App) -> String {
    let pairing = match app.multi_select_pairing {
        GroupPairing::AnyOneToOne => "any one-to-one pairing",
        GroupPairing::AllToAll => "ALL-TO-ALL",
    };
    format!(
        "Multi-map selection: {} before, {} after node(s), {pairing} (m/M to commit as a group, X to flip pairing, c to clear)",
        app.before_multi_select.len(),
        app.after_multi_select.len()
    )
}

/// Keys `handle_key` handles without touching `App::mapping`, a `collapsed` set, or
/// `App::hide_solved` - the inputs of the cached `FrameState` - so they skip its rebuild. Browsing
/// is most keystrokes, and a rebuild on a large case is expensive. Keys that only sometimes
/// mutate (`h`/`l`/`a`/`A`) or that are rare (`s`/`R`/`o`/`O`/`C`) are left out on purpose.
pub(crate) fn is_navigation_or_display_key(code: KeyCode) -> bool {
    matches!(
        code,
        KeyCode::Char('q')
            | KeyCode::Esc
            | KeyCode::Char('?')
            | KeyCode::Tab
            | KeyCode::Up
            | KeyCode::Char('k')
            | KeyCode::Down
            | KeyCode::Char('j')
            | KeyCode::Char('g')
            | KeyCode::Char('G')
            | KeyCode::Char('x')
            | KeyCode::Char('X')
            | KeyCode::Char('c')
            | KeyCode::Char('p')
            | KeyCode::Char('n')
            | KeyCode::Char('N')
            | KeyCode::Char('/')
            | KeyCode::Char('t')
            | KeyCode::Char('T')
            | KeyCode::Char('r')
    )
}

/// Whether `code`, delivered in the current `modal` state, is guaranteed not to touch the mapping,
/// either panel's collapsed set, or `hide_solved` -- the inputs of the cached `FrameState`.
/// In a prompt modal only typing and Backspace qualify; its Enter/Esc may act on the case, so
/// they (and any other modal key) conservatively force a rebuild.
pub(crate) fn is_state_preserving_key(modal: Option<&Modal>, code: KeyCode) -> bool {
    match modal {
        None => is_navigation_or_display_key(code),
        Some(Modal::PromptSearch { .. })
        | Some(Modal::PromptPromoteName { .. })
        | Some(Modal::PromptRejectReason { .. })
        | Some(Modal::PromptComment { .. }) => {
            matches!(code, KeyCode::Char(_) | KeyCode::Backspace)
        }
        // Every key here preserves the cache, even the ones that paint: painting writes only
        // `text_mappings`, and the cache derives from `entries`/`groups` alone. This matters
        // because the painting view is where a reader holds down `j` on a large file.
        Some(Modal::TextView { .. })
        | Some(Modal::SolutionPicker { .. })
        | Some(Modal::UnixDiffView { .. }) => true,
        Some(_) => false,
    }
}

/// Runs the event loop for a single case until the user quits or asks to switch to a different
/// one. Separate from `run_event_loop` so the cached `FrameState`, which borrows `before`/`after`,
/// never coexists with their reassignment; a case switch is returned as `SessionEnd::Open`.
pub(crate) fn run_case_session(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    before: &Code,
    after: &Code,
) -> Result<SessionEnd> {
    // An idle poll timeout redraws nothing.
    let mut needs_redraw = true;

    // `None` forces a rebuild; only keys outside `is_state_preserving_key` reset it.
    let mut state: Option<FrameState> = None;

    loop {
        if state.is_none() {
            state = Some(compute_frame_state(before, after, app)?);
        }
        let frame_state = state.as_ref().expect("just populated above if empty");

        if needs_redraw {
            // Cloned: `draw_ui` also takes `app` mutably.
            let current_name = app.name.clone();
            terminal.draw(|f| {
                draw_ui(
                    f,
                    app,
                    &frame_state.before_flat,
                    &frame_state.after_flat,
                    &frame_state.caches,
                    frame_state.before_src,
                    frame_state.after_src,
                    frame_state.before_unmarked,
                    frame_state.after_unmarked,
                    &current_name,
                    frame_state.roots().is_none(),
                )
            })?;
            needs_redraw = false;
        }

        if !event::poll(Duration::from_millis(250))? {
            continue;
        }

        let event = event::read()?;
        let Event::Key(key) = event else {
            needs_redraw = true;
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        let state_preserving = is_state_preserving_key(app.modal.as_ref(), key.code);

        let mut open_request: Option<OpenTarget> = None;

        if app.modal.is_some() {
            open_request = handle_modal_key(
                app,
                key.code,
                &frame_state.before_flat,
                &frame_state.after_flat,
                frame_state.before_root,
                frame_state.after_root,
                &frame_state.caches,
                frame_state.before_src,
                frame_state.after_src,
                before,
                after,
            );
        } else if let Some((before_root, after_root)) = frame_state.roots() {
            // Every loader runs `ensure_parsed` when there is a tree, so the hashes exist here.
            let before_hash = &before
                .metadata
                .ast_metadata
                .as_ref()
                .context("Before code has no AST metadata")?
                .node_to_full_hash;
            let after_hash = &after
                .metadata
                .ast_metadata
                .as_ref()
                .context("After code has no AST metadata")?
                .node_to_full_hash;

            handle_key(
                app,
                key.code,
                &frame_state.before_flat,
                &frame_state.after_flat,
                before_root,
                after_root,
                &frame_state.caches,
                frame_state.before_src,
                frame_state.after_src,
                before_hash,
                after_hash,
                before,
                after,
            );
        } else {
            // Text-only mode (see `FrameState::before_root`).
            handle_tree_independent_key(
                app,
                key.code,
                frame_state.before_src,
                frame_state.after_src,
                before,
                after,
                false,
            );
        }

        needs_redraw = true;
        if !state_preserving {
            state = None;
        }

        if let Some(target) = open_request {
            return Ok(SessionEnd::Open(target));
        }

        if app.should_quit {
            return Ok(SessionEnd::Quit);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_key(
    app: &mut App,
    code: KeyCode,
    before_flat: &FlatIndex,
    after_flat: &FlatIndex,
    before_root: Node,
    after_root: Node,
    caches: &Caches,
    before_src: &[u8],
    after_src: &[u8],
    before_hash: &rustc_hash::FxHashMap<usize, u64>,
    after_hash: &rustc_hash::FxHashMap<usize, u64>,
    before: &Code,
    after: &Code,
) {
    let focus = app.focus;

    let result: Option<Result<String>> = match code {
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
            let outcome = if app.before_multi_select.is_empty() && app.after_multi_select.is_empty()
            {
                action_match(
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
                    before_hash,
                    after_hash,
                )
            } else {
                action_commit_multi_map_group(
                    &mut app.mapping,
                    before_root,
                    after_root,
                    &app.before_multi_select,
                    &app.after_multi_select,
                    before_hash,
                    after_hash,
                    caches,
                    false,
                    app.multi_select_pairing,
                )
            };
            match outcome {
                Ok(ActionOutcome::Done(msg)) => {
                    app.dirty = true;
                    app.status = Some(msg);
                    app.clear_multi_select();
                    advance_both_to_next_unmarked(
                        app,
                        before_flat,
                        after_flat,
                        before_root,
                        after_root,
                    );
                }
                Ok(ActionOutcome::NeedsModal(modal)) => app.modal = Some(*modal),
                Err(err) => app.status = Some(format!("Error: {:#}", err)),
            }
            None
        }
        KeyCode::Char('f') => {
            match action_match_to_end(
                app,
                before_flat,
                after_flat,
                before_root,
                after_root,
                before_src,
                after_src,
                before_hash,
                after_hash,
            ) {
                Ok(ActionOutcome::Done(msg)) => app.status = Some(msg),
                Ok(ActionOutcome::NeedsModal(modal)) => app.modal = Some(*modal),
                Err(err) => app.status = Some(format!("Error: {:#}", err)),
            }
            None
        }
        KeyCode::Char('M') => {
            let outcome = if app.before_multi_select.is_empty() && app.after_multi_select.is_empty()
            {
                action_match_subtree(
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
                    before_hash,
                    after_hash,
                    &mut app.before.collapsed,
                    &mut app.after.collapsed,
                )
            } else if app.multi_select_pairing == GroupPairing::AllToAll {
                // Whole subtrees, not just the roots. No mixed-kinds modal: the walk is stricter
                // than that check and reports a divergence itself.
                action_commit_all_to_all_subtrees(
                    &mut app.mapping,
                    before_root,
                    after_root,
                    &app.before_multi_select,
                    &app.after_multi_select,
                    before_hash,
                    after_hash,
                    caches,
                    before_src,
                    after_src,
                )
                .map(ActionOutcome::Done)
            } else {
                action_commit_multi_map_group(
                    &mut app.mapping,
                    before_root,
                    after_root,
                    &app.before_multi_select,
                    &app.after_multi_select,
                    before_hash,
                    after_hash,
                    caches,
                    true,
                    app.multi_select_pairing,
                )
            };
            match outcome {
                Ok(ActionOutcome::Done(msg)) => {
                    app.dirty = true;
                    app.status = Some(msg);
                    app.clear_multi_select();
                    advance_both_to_next_unmarked(
                        app,
                        before_flat,
                        after_flat,
                        before_root,
                        after_root,
                    );
                }
                Ok(ActionOutcome::NeedsModal(modal)) => app.modal = Some(*modal),
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
                    advance_side_to_next_unmarked(
                        app,
                        Side::Before,
                        before_flat,
                        before_root,
                        after_root,
                    );
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
                    advance_side_to_next_unmarked(
                        app,
                        Side::After,
                        after_flat,
                        before_root,
                        after_root,
                    );
                }
                Some(res)
            }
        }
        KeyCode::Char('a') => Some(action_align(app, focus, before_root, after_root, caches)),
        KeyCode::Char('A') => Some(action_align_algo(app, focus, before_root, after_root)),
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
        KeyCode::Char('x') => {
            let (cursor_id, selected) = match focus {
                Focus::Before => (app.before.cursor_id, &mut app.before_multi_select),
                Focus::After => (app.after.cursor_id, &mut app.after_multi_select),
            };
            if !selected.remove(&cursor_id) {
                selected.insert(cursor_id);
            }
            app.status = Some(multi_select_status(app));
            None
        }
        KeyCode::Char('X') => {
            app.multi_select_pairing = match app.multi_select_pairing {
                GroupPairing::AnyOneToOne => GroupPairing::AllToAll,
                GroupPairing::AllToAll => GroupPairing::AnyOneToOne,
            };
            app.status = Some(multi_select_status(app));
            None
        }
        KeyCode::Char('c') => {
            app.clear_multi_select();
            app.status = Some("Cleared multi-map selection".to_string());
            None
        }
        KeyCode::Char('n') => Some(action_next_mismatch(
            app,
            focus,
            before_flat,
            after_flat,
            caches,
            true,
        )),
        KeyCode::Char('N') => Some(action_next_mismatch(
            app,
            focus,
            before_flat,
            after_flat,
            caches,
            false,
        )),
        KeyCode::Char('/') => {
            app.modal = Some(Modal::PromptSearch {
                input: app.last_search.clone().unwrap_or_default(),
            });
            None
        }
        KeyCode::Char('H') => {
            app.hide_solved = !app.hide_solved;
            app.status = Some(if app.hide_solved {
                "Hiding fully solved subtrees".to_string()
            } else {
                "Showing all nodes".to_string()
            });
            None
        }
        KeyCode::Char('r') => {
            app.show_reason = !app.show_reason;
            app.status = Some(if app.show_reason {
                "Showing ASTMappingReason next to each node's algo verdict".to_string()
            } else {
                "Hiding ASTMappingReason".to_string()
            });
            None
        }
        _ => {
            handle_tree_independent_key(app, code, before_src, after_src, before, after, true);
            return;
        }
    };

    apply_key_result(app, result);
}

/// The keys that read no tree. [`handle_key`] falls through to this, and text-only mode calls it
/// directly, so each key has one implementation in both modes.
///
/// `tree_available` is false in text-only mode, where an unhandled key reports why the tree keys
/// do nothing instead of looking like a hang.
pub(crate) fn handle_tree_independent_key(
    app: &mut App,
    code: KeyCode,
    before_src: &[u8],
    after_src: &[u8],
    before: &Code,
    after: &Code,
    tree_available: bool,
) {
    let result: Option<Result<String>> = match code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.should_quit = true;
            None
        }
        // Shift-1 rather than a letter: this is the one action that cannot be undone, so it
        // should not be one slip away from a harmless key.
        KeyCode::Char('!') => {
            app.modal = Some(Modal::ConfirmResetCase {
                entries: app.mapping.entries.len(),
                groups: app.mapping.groups.len(),
                paintings: app.mapping.text_mappings.len(),
            });
            None
        }
        KeyCode::Char('?') => {
            app.modal = Some(Modal::Help { scroll: 0 });
            None
        }
        KeyCode::Tab => {
            app.focus = app.focus.toggle();
            None
        }
        KeyCode::Char('p') => {
            let diff = diff_code(before, after);
            app.status = Some(match diff.ast {
                Some(ast_diff) => {
                    let msg = format!(
                        "Ran codediff: {} before-node(s), {} after-node(s) mapped",
                        ast_diff.before_node_map.len(),
                        ast_diff.after_node_map.len()
                    );
                    app.algo_diff = Some(ast_diff);
                    msg
                }
                None => "codediff produced no AST diff".to_string(),
            });
            None
        }
        KeyCode::Char('t') => {
            app.modal = Some(Modal::TextView {
                state: TextPaintState::default(),
            });
            None
        }
        KeyCode::Char('V') => {
            match invariant_entries(&app.mapping, before, after) {
                Ok(entries) if entries.is_empty() => {
                    app.status =
                        Some("This case's ground truth breaks none of its invariants".to_string());
                }
                Ok(entries) => {
                    app.status = Some(format!(
                        "{} invariant violation(s) - j/k to move, Enter to jump, Esc to close",
                        entries.len()
                    ));
                    app.modal = Some(Modal::InvariantList {
                        entries,
                        selected: 0,
                    });
                }
                Err(err) => app.status = Some(format!("Could not check invariants: {err:#}")),
            }
            None
        }
        KeyCode::Char('T') => {
            match run_unix_diff(before_src, after_src) {
                Ok(output) => app.modal = Some(Modal::UnixDiffView { output, scroll: 0 }),
                Err(err) => app.status = Some(format!("Error running diff: {:#}", err)),
            }
            None
        }
        KeyCode::Char('s') => match &app.origin {
            CaseOrigin::Diffs => {
                let result = action_save(
                    &mut app.mapping,
                    &mut app.dirty,
                    &app.name,
                    None,
                    is_text_only(before, after),
                );
                if result.is_ok() {
                    refresh_diff_unmarked(app, &app.name.clone());
                    refresh_diff_text_painted(app, &app.name.clone());
                    refresh_diff_disagreement(app, &app.name.clone());
                    refresh_diff_invariants(app, &app.name.clone());
                }
                Some(result)
            }
            CaseOrigin::Sample(source) => {
                app.modal = Some(Modal::PromptPromoteName {
                    input: default_promoted_name(source),
                    error: None,
                });
                None
            }
            CaseOrigin::GitCommitFile { path } => {
                app.modal = Some(Modal::PromptPromoteName {
                    input: default_promoted_name_for_path(path),
                    error: None,
                });
                None
            }
        },
        KeyCode::Char('R') => {
            if matches!(app.origin, CaseOrigin::Sample(_)) {
                app.modal = Some(Modal::PromptRejectReason {
                    input: String::new(),
                    error: None,
                });
            } else {
                app.status = Some("Only an open sample (O) can be rejected".to_string());
            }
            None
        }
        KeyCode::Char('e') => {
            if let CaseOrigin::Diffs = &app.origin {
                // Only the note: `action_promote` moves the sample.csv comment into it.
                let existing = read_note(&app.name).unwrap_or_default();
                app.modal = Some(Modal::PromptComment {
                    input: existing,
                    error: None,
                });
            } else if let CaseOrigin::Sample(source) = &app.origin {
                let existing = read_sample_csv_rows(&sample_csv_path())
                    .ok()
                    .and_then(|rows| find_sample_row(&rows, source).map(|row| row.comment.clone()))
                    .unwrap_or_default();
                app.modal = Some(Modal::PromptComment {
                    input: existing,
                    error: None,
                });
            } else {
                app.status =
                    Some("Only an open diff (o) or sample (O) can have a comment".to_string());
            }
            None
        }
        KeyCode::Char('o') => {
            // Eager, unlike the other per-case maps: notes are displayed, not just sorted by.
            if app.diff_comments.is_none() {
                app.diff_comments = Some(compute_diff_comments());
            }
            match list_available_cases() {
                Ok(options) if !options.is_empty() => {
                    let modal = open_diff_picker_modal(
                        options,
                        &app.name,
                        app.diff_view.clone(),
                        DiffPickerData::from_app(app),
                    );
                    app.modal = Some(modal);
                }
                Ok(_) => {
                    app.status = Some("No test cases found in src/test/data/diffs".to_string());
                }
                Err(err) => {
                    app.status = Some(format!("Error listing cases: {:#}", err));
                }
            }
            None
        }
        KeyCode::Char('O') => {
            match list_sample_rows() {
                Ok(rows) if !rows.is_empty() => {
                    // One external `diff` per sample: scan only the unmeasured ones, in parallel.
                    let missing: Vec<String> = rows
                        .iter()
                        .map(|row| row.name.clone())
                        .filter(|name| !app.sample_diff_sizes.contains_key(name))
                        .collect();
                    if !missing.is_empty() {
                        app.sample_diff_sizes.extend(scan_corpus(&missing, |name| {
                            Some(sample_diff_line_count(name))
                        }));
                    }
                    let rows: Vec<SampleRow> = rows
                        .into_iter()
                        .map(|mut row| {
                            row.size = app
                                .sample_diff_sizes
                                .get(&row.name)
                                .copied()
                                .unwrap_or_default();
                            row
                        })
                        .collect();
                    let view = app.sample_view.clone();
                    app.modal = Some(open_sample_picker_modal(rows, &app.name, view));
                }
                Ok(_) => {
                    app.status = Some("No samples found in src/test/data/samples".to_string());
                }
                Err(err) => {
                    app.status = Some(format!("Error listing samples: {:#}", err));
                }
            }
            None
        }
        KeyCode::Char('C') => {
            match list_repo_commits() {
                Ok(commits) if !commits.is_empty() => {
                    app.modal = Some(Modal::OpenCommitPicker {
                        commits,
                        selected: 0,
                    });
                }
                Ok(_) => {
                    app.status = Some("No commits found in this repository".to_string());
                }
                Err(err) => {
                    app.status = Some(format!("Error listing commits: {:#}", err));
                }
            }
            None
        }
        _ if !tree_available => Some(Ok(
            "No tree-sitter grammar for this file: the tree keys do nothing here, but t (paint) \
             and T (unix diff) work"
                .to_string(),
        )),
        _ => None,
    };

    apply_key_result(app, result);
}

fn apply_key_result(app: &mut App, result: Option<Result<String>>) {
    if let Some(res) = result {
        app.status = Some(match res {
            Ok(msg) => msg,
            Err(err) => format!("Error: {:#}", err),
        });
    }
}

/// The tree-committing modals' answer if reached with no tree. Unreachable (they come from
/// `m`/`M`), but reported rather than unwrapped.
const NO_TREE_TO_MAP: &str = "No tree-sitter grammar for this file: there is no tree to map";

/// Routes a keypress while `app.modal` is `Some`. Returns the case to switch to when the human
/// confirmed one; the caller loads it, because that replaces the `Code`s these `Node`s borrow.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_modal_key(
    app: &mut App,
    code: KeyCode,
    before_flat: &FlatIndex,
    after_flat: &FlatIndex,
    // `None` in text-only mode (see `FrameState::before_root`).
    before_root: Option<Node>,
    after_root: Option<Node>,
    caches: &Caches,
    before_src: &[u8],
    after_src: &[u8],
    before: &Code,
    after: &Code,
) -> Option<OpenTarget> {
    let modal = app.modal.take()?;

    let roots = before_root.zip(after_root);

    match modal {
        Modal::ConfirmResetCase { .. } => match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                app.status = Some(action_reset_case(app));
            }
            // Only `y` confirms, not Enter: Enter confirms everywhere else, so it is hit by reflex.
            _ => {
                app.status = Some("Reset cancelled".to_string());
            }
        },
        Modal::ConfirmKindMismatch {
            before_id,
            after_id,
            before_kind,
            after_kind,
            recursive,
        } => match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let Some((before_root, after_root)) = roots else {
                    app.status = Some(NO_TREE_TO_MAP.to_string());
                    return None;
                };
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
                    &mut app.before.collapsed,
                    &mut app.after.collapsed,
                ));
                advance_both_to_next_unmarked(
                    app,
                    before_flat,
                    after_flat,
                    before_root,
                    after_root,
                );
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
        Modal::ConfirmMultiMapGroup {
            before_ids,
            after_ids,
            operation,
            with_children,
            pairing,
            kinds,
        } => match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let before_set: std::collections::BTreeSet<usize> =
                    before_ids.iter().copied().collect();
                let after_set: std::collections::BTreeSet<usize> =
                    after_ids.iter().copied().collect();
                let Some((before_root, after_root)) = roots else {
                    app.status = Some(NO_TREE_TO_MAP.to_string());
                    return None;
                };
                app.status = Some(
                    match commit_multi_map_group(
                        &mut app.mapping,
                        before_root,
                        after_root,
                        &before_set,
                        &after_set,
                        operation,
                        with_children,
                        pairing,
                    ) {
                        Ok(msg) => {
                            app.dirty = true;
                            msg
                        }
                        Err(err) => format!("Error: {:#}", err),
                    },
                );
                app.clear_multi_select();
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                app.status = Some("Cancelled: multi-map group has mixed node kinds".to_string());
                app.clear_multi_select();
            }
            _ => {
                app.modal = Some(Modal::ConfirmMultiMapGroup {
                    before_ids,
                    after_ids,
                    operation,
                    with_children,
                    pairing,
                    kinds,
                });
            }
        },
        Modal::OpenDiffPicker {
            options,
            selected,
            view,
            name_input,
        } => {
            return handle_open_diff_picker(app, code, options, selected, view, name_input);
        }
        Modal::OpenSamplePicker {
            rows,
            selected,
            view,
            name_input,
        } => {
            return handle_open_sample_picker(app, code, rows, selected, view, name_input);
        }
        Modal::OpenCommitPicker { commits, selected } => match code {
            KeyCode::Up | KeyCode::Char('k') => {
                app.modal = Some(Modal::OpenCommitPicker {
                    selected: selected.saturating_sub(1),
                    commits,
                });
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.modal = Some(Modal::OpenCommitPicker {
                    selected: (selected + 1).min(commits.len().saturating_sub(1)),
                    commits,
                });
            }
            KeyCode::Enter => {
                if let Some((hash, summary)) = commits.get(selected).cloned() {
                    match list_commit_files(&hash) {
                        Ok(files) if !files.is_empty() => {
                            app.modal = Some(Modal::OpenCommitFilePicker {
                                hash,
                                summary,
                                files,
                                selected: 0,
                            });
                        }
                        Ok(_) => {
                            app.status = Some(format!(
                                "No files with a supported language changed in commit {} \
                                 (a merge commit shows no diff here by default - see \
                                 `list_commit_files`)",
                                short_hash(&hash)
                            ));
                            app.modal = Some(Modal::OpenCommitPicker { commits, selected });
                        }
                        Err(err) => {
                            app.status = Some(format!(
                                "Error listing files for commit {}: {:#}",
                                short_hash(&hash),
                                err
                            ));
                            app.modal = Some(Modal::OpenCommitPicker { commits, selected });
                        }
                    }
                } else {
                    app.modal = Some(Modal::OpenCommitPicker { commits, selected });
                }
            }
            KeyCode::Esc => {
                app.status = Some("Cancelled".to_string());
            }
            _ => {
                app.modal = Some(Modal::OpenCommitPicker { commits, selected });
            }
        },
        Modal::OpenCommitFilePicker {
            hash,
            summary,
            files,
            selected,
        } => match code {
            KeyCode::Up | KeyCode::Char('k') => {
                app.modal = Some(Modal::OpenCommitFilePicker {
                    selected: selected.saturating_sub(1),
                    hash,
                    summary,
                    files,
                });
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.modal = Some(Modal::OpenCommitFilePicker {
                    selected: (selected + 1).min(files.len().saturating_sub(1)),
                    hash,
                    summary,
                    files,
                });
            }
            KeyCode::Enter => {
                if let Some(path) = files.get(selected).cloned() {
                    let target = OpenTarget::GitCommitFile {
                        hash,
                        summary,
                        path,
                    };
                    if app.dirty {
                        let can_save = matches!(app.origin, CaseOrigin::Diffs);
                        app.modal = Some(Modal::ConfirmDiscardUnsaved { target, can_save });
                    } else {
                        return Some(target);
                    }
                } else {
                    app.modal = Some(Modal::OpenCommitFilePicker {
                        hash,
                        summary,
                        files,
                        selected,
                    });
                }
            }
            KeyCode::Esc => {
                app.status = Some("Cancelled".to_string());
            }
            _ => {
                app.modal = Some(Modal::OpenCommitFilePicker {
                    hash,
                    summary,
                    files,
                    selected,
                });
            }
        },
        Modal::ConfirmDiscardUnsaved { target, can_save } => match code {
            KeyCode::Char('s') | KeyCode::Char('S') if can_save => {
                match action_save(
                    &mut app.mapping,
                    &mut app.dirty,
                    &app.name,
                    None,
                    is_text_only(before, after),
                ) {
                    Ok(_) => {
                        refresh_diff_unmarked(app, &app.name.clone());
                        refresh_diff_text_painted(app, &app.name.clone());
                        refresh_diff_disagreement(app, &app.name.clone());
                        refresh_diff_invariants(app, &app.name.clone());
                        return Some(target);
                    }
                    Err(err) => {
                        app.status = Some(format!(
                            "Save failed ({:#}); not opening '{}'.",
                            err,
                            target.name()
                        ));
                    }
                }
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                return Some(target);
            }
            KeyCode::Esc | KeyCode::Char('c') | KeyCode::Char('C') => {
                app.status = Some("Cancelled".to_string());
            }
            _ => {
                app.modal = Some(Modal::ConfirmDiscardUnsaved { target, can_save });
            }
        },
        Modal::PromptPromoteName {
            mut input,
            error: _,
        } => match code {
            KeyCode::Enter => {
                let new_name = input.trim().to_string();
                match action_promote(app, &new_name, before_src, after_src) {
                    Ok(msg) => app.status = Some(msg),
                    Err(err) => {
                        app.modal = Some(Modal::PromptPromoteName {
                            input,
                            error: Some(format!("{:#}", err)),
                        });
                    }
                }
            }
            KeyCode::Esc => {
                app.status = Some("Cancelled".to_string());
            }
            KeyCode::Backspace => {
                input.pop();
                app.modal = Some(Modal::PromptPromoteName { input, error: None });
            }
            KeyCode::Char(c) => {
                input.push(c);
                app.modal = Some(Modal::PromptPromoteName { input, error: None });
            }
            _ => {
                app.modal = Some(Modal::PromptPromoteName { input, error: None });
            }
        },
        Modal::PromptRejectReason {
            mut input,
            error: _,
        } => match code {
            KeyCode::Enter => {
                let reason = input.trim().to_string();
                match action_reject(app, &reason) {
                    Ok(msg) => app.status = Some(msg),
                    Err(err) => {
                        app.modal = Some(Modal::PromptRejectReason {
                            input,
                            error: Some(format!("{:#}", err)),
                        });
                    }
                }
            }
            KeyCode::Esc => {
                app.status = Some("Cancelled".to_string());
            }
            KeyCode::Backspace => {
                input.pop();
                app.modal = Some(Modal::PromptRejectReason { input, error: None });
            }
            KeyCode::Char(c) => {
                input.push(c);
                app.modal = Some(Modal::PromptRejectReason { input, error: None });
            }
            _ => {
                app.modal = Some(Modal::PromptRejectReason { input, error: None });
            }
        },
        Modal::PromptComment {
            mut input,
            error: _,
        } => match code {
            KeyCode::Enter => {
                let comment = input.trim().to_string();
                match action_comment(app, &comment) {
                    Ok(msg) => {
                        refresh_diff_comment(app, &app.name.clone());
                        app.status = Some(msg);
                    }
                    Err(err) => {
                        app.modal = Some(Modal::PromptComment {
                            input,
                            error: Some(format!("{:#}", err)),
                        });
                    }
                }
            }
            KeyCode::Esc => {
                app.status = Some("Cancelled".to_string());
            }
            KeyCode::Backspace => {
                input.pop();
                app.modal = Some(Modal::PromptComment { input, error: None });
            }
            KeyCode::Char(c) => {
                input.push(c);
                app.modal = Some(Modal::PromptComment { input, error: None });
            }
            _ => {
                app.modal = Some(Modal::PromptComment { input, error: None });
            }
        },
        Modal::PromptSearch { mut input } => match code {
            KeyCode::Enter => {
                let query = input.trim().to_string();
                if query.is_empty() {
                    app.status = Some("Search cancelled: empty query".to_string());
                } else {
                    app.last_search = Some(query.clone());
                    let focus = app.focus;
                    app.status = Some(
                        match action_search(
                            app,
                            focus,
                            before_flat,
                            after_flat,
                            before_src,
                            after_src,
                            &query,
                        ) {
                            Ok(msg) => msg,
                            Err(err) => format!("{:#}", err),
                        },
                    );
                }
            }
            KeyCode::Esc => {
                app.status = Some("Cancelled".to_string());
            }
            KeyCode::Backspace => {
                input.pop();
                app.modal = Some(Modal::PromptSearch { input });
            }
            KeyCode::Char(c) => {
                input.push(c);
                app.modal = Some(Modal::PromptSearch { input });
            }
            _ => {
                app.modal = Some(Modal::PromptSearch { input });
            }
        },
        Modal::TextView { state } => {
            return handle_text_view(app, code, state, before_src, after_src, before, after);
        }
        Modal::SolutionPicker {
            names,
            selected,
            saving,
            new_name,
            confirm_delete,
            state,
        } => {
            return handle_solution_picker(
                app,
                code,
                names,
                selected,
                saving,
                new_name,
                confirm_delete,
                state,
                before_src,
                after_src,
            );
        }
        Modal::UnixDiffView { output, scroll } => match code {
            KeyCode::Up | KeyCode::Char('k') => {
                app.modal = Some(Modal::UnixDiffView {
                    output,
                    scroll: scroll.saturating_sub(1),
                });
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.modal = Some(Modal::UnixDiffView {
                    output,
                    scroll: scroll.saturating_add(1),
                });
            }
            KeyCode::PageUp => {
                app.modal = Some(Modal::UnixDiffView {
                    output,
                    scroll: scroll.saturating_sub(10),
                });
            }
            KeyCode::PageDown => {
                app.modal = Some(Modal::UnixDiffView {
                    output,
                    scroll: scroll.saturating_add(10),
                });
            }
            KeyCode::Char('t') => {
                app.modal = Some(Modal::TextView {
                    state: TextPaintState::default(),
                });
            }
            KeyCode::Esc => {
                app.status = Some("Closed diff view".to_string());
            }
            _ => {
                app.modal = Some(Modal::UnixDiffView { output, scroll });
            }
        },
        Modal::InvariantList { entries, selected } => {
            let last = entries.len().saturating_sub(1);
            match code {
                KeyCode::Up | KeyCode::Char('k') => {
                    app.modal = Some(Modal::InvariantList {
                        entries,
                        selected: selected.saturating_sub(1),
                    });
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    app.modal = Some(Modal::InvariantList {
                        entries,
                        selected: (selected + 1).min(last),
                    });
                }
                KeyCode::Char('g') => {
                    app.modal = Some(Modal::InvariantList {
                        entries,
                        selected: 0,
                    });
                }
                KeyCode::Char('G') => {
                    app.modal = Some(Modal::InvariantList {
                        entries,
                        selected: last,
                    });
                }
                KeyCode::Enter => {
                    if let Some(entry) = entries.get(selected) {
                        action_focus_violation(app, entry, before, after);
                    }
                }
                KeyCode::Esc | KeyCode::Char('V') | KeyCode::Char('q') => {
                    app.status = Some("Closed the invariant list".to_string());
                }
                _ => {
                    app.modal = Some(Modal::InvariantList { entries, selected });
                }
            }
        }
        Modal::Help { scroll } => match code {
            KeyCode::Up | KeyCode::Char('k') => {
                app.modal = Some(Modal::Help {
                    scroll: scroll.saturating_sub(1),
                });
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.modal = Some(Modal::Help {
                    scroll: scroll.saturating_add(1),
                });
            }
            KeyCode::Esc | KeyCode::Char('?') => {
                app.status = Some("Closed help".to_string());
            }
            _ => {
                app.modal = Some(Modal::Help { scroll });
            }
        },
    }

    None
}

/// Saves the mapping and ensures the fixture's test stubs exist.
///
/// `text_only` (no tree-sitter grammar) changes only what the generated stub asserts - see
/// `ensure_stub_test`. `comment` is a promoted sample's comment; it is written only when the stub
/// file is created, never into an existing one.
pub(crate) fn action_save(
    mapping: &mut HumanMapping,
    dirty: &mut bool,
    name: &str,
    comment: Option<&str>,
    text_only: bool,
) -> Result<String> {
    human_mapping::save(name, mapping)?;
    let created = ensure_stub_test(name, comment, text_only)?;
    // An unpainted fixture has nothing to score, and its painting stub would fail.
    if !mapping.text_mappings.is_empty() {
        ensure_painting_stub_test(name)?;
    }
    // The invariants also cover the tree mapping, which every saved fixture has.
    ensure_invariants_stub_test(name)?;
    *dirty = false;
    Ok(if created {
        format!(
            "Saved human_mapping.json and created fixtures/{}.rs",
            module_name(name)
        )
    } else {
        "Saved human_mapping.json".to_string()
    })
}

/// Rust keywords (2015 through 2024 edition, strict and reserved). A case name becomes a module
/// identifier, so a keyword name would produce a stub that does not compile.
pub(crate) const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while", "abstract", "become", "box", "do", "final", "gen", "macro",
    "override", "priv", "try", "typeof", "unsized", "virtual", "yield",
];

/// A name must be a valid Rust module identifier after `module_name`, and safe as a directory name.
pub(crate) fn validate_new_case_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("Name cannot be empty");
    }
    if !name.chars().next().unwrap().is_ascii_alphabetic() {
        bail!("Name must start with a letter");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("Name may only contain letters, digits, '-' and '_'");
    }
    if RUST_KEYWORDS.contains(&module_name(name).as_str()) {
        bail!(
            "'{}' becomes the Rust keyword '{}' as a module name; pick another name",
            name,
            module_name(name)
        );
    }
    Ok(())
}

/// Promotes the open sample or git-commit-sourced case into
/// `src/test/data/diffs/<dataset>/<new_name>/` (see `promote_target_dataset`), writing
/// `before_src`/`after_src` and saving via `action_save`. A sample also gets `new_name` recorded in
/// its sample.csv row. On success `app` is switched to the new diffs/ case.
pub(crate) fn action_promote(
    app: &mut App,
    new_name: &str,
    before_src: &[u8],
    after_src: &[u8],
) -> Result<String> {
    let origin = app.origin.clone();
    let (path, sample_source): (String, Option<SampleSource>) = match &origin {
        CaseOrigin::Sample(source) => (source.path.clone(), Some(source.clone())),
        CaseOrigin::GitCommitFile { path } => (path.clone(), None),
        CaseOrigin::Diffs => bail!("Current case is not a sample or a git-commit-sourced case"),
    };
    let dataset = promote_target_dataset(&origin)
        .expect("just matched Sample or GitCommitFile above, both of which return Some")
        .to_string();

    validate_new_case_name(new_name)?;

    if let Some(source) = &sample_source {
        // Not trusted outright: a typo in source.json would create a diffs/ folder nothing reads.
        if !DIFF_DATASETS.contains(&source.dataset.as_str()) {
            bail!(
                "sample's recorded dataset '{}' is not one of {:?} - check source.json under \
                 src/test/data/samples/{}/",
                source.dataset,
                DIFF_DATASETS,
                app.name
            );
        }
    }

    // Across every dataset: case names are resolved by flat name, so they must be unique.
    if diffs_case_dir(new_name).is_some() {
        bail!("'{}' already exists in src/test/data/diffs", new_name);
    }
    let dir = diffs_root().join(&dataset).join(new_name);

    let ext = Path::new(&path)
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .ok_or_else(|| anyhow!("path {} has no extension", path))?;

    fs::create_dir_all(&dir).with_context(|| format!("creating {:?}", dir))?;
    fs::write(dir.join(format!("before.{ext}.test")), before_src)?;
    fs::write(dir.join(format!("after.{ext}.test")), after_src)?;

    // The README carries the third-party code's license attribution, which must survive
    // promotion. A git-commit case is this repo's own code and has none.
    let readme_note = if sample_source.is_some() {
        let readme_src = samples_root().join(&app.name).join("README.md");
        match fs::copy(&readme_src, dir.join("README.md")) {
            Ok(_) => String::new(),
            Err(err) => format!(
                " (failed to copy README.md from {:?}: {:#})",
                readme_src, err
            ),
        }
    } else {
        String::new()
    };

    let comment = match &sample_source {
        Some(source) => sample_comment(source)?,
        None => None,
    };
    let save_msg = action_save(
        &mut app.mapping,
        &mut app.dirty,
        new_name,
        comment.as_deref(),
        // Samples and git-commit files load only with a grammar, so they are never text-only.
        false,
    )?;

    // The note moves to `description.md`; `update_sample_csv` clears the sample.csv cell so there
    // is one copy (pinned by `no_promoted_row_carries_a_comment`).
    let note_written = match &comment {
        Some(comment) => write_note(new_name, comment).is_ok(),
        None => false,
    };
    refresh_diff_unmarked(app, new_name);
    refresh_diff_text_painted(app, new_name);
    refresh_diff_disagreement(app, new_name);
    refresh_diff_invariants(app, new_name);
    refresh_diff_comment(app, new_name);

    let csv_note = match &sample_source {
        Some(source) => match update_sample_csv(source, new_name) {
            Ok(true) => String::new(),
            Ok(false) => " (source row not found in sample.csv; not updated)".to_string(),
            Err(err) => format!(" (failed to update sample.csv: {:#})", err),
        },
        None => String::new(),
    };

    app.name = new_name.to_string();
    app.origin = CaseOrigin::Diffs;

    Ok(format!(
        "Promoted to '{}'. {}{}{}{}",
        new_name,
        save_msg,
        csv_note,
        readme_note,
        if note_written {
            " Comment copied to description.md."
        } else {
            ""
        }
    ))
}

/// Rejects the open sample: records `reason` as its sample.csv `comment` with `status` REJECTED.
/// Fails for anything but a sample, and for an empty reason.
pub(crate) fn action_reject(app: &App, reason: &str) -> Result<String> {
    let CaseOrigin::Sample(source) = &app.origin else {
        bail!("Only a sample (opened via O) can be rejected");
    };

    let reason = reason.trim();
    if reason.is_empty() {
        bail!("Rejection reason cannot be empty");
    }

    match reject_sample(source, reason)? {
        true => Ok(format!("Rejected '{}': {}", app.name, reason)),
        false => bail!("source row not found in sample.csv; not updated"),
    }
}

/// Records or clears (empty `comment`) the open case's note: a diffs/ case's or promoted sample's
/// `description.md`, otherwise the sample's sample.csv `comment`, leaving `status` alone.
pub(crate) fn action_comment(app: &App, comment: &str) -> Result<String> {
    let comment = comment.trim();

    // Not the sample.csv row: nothing reads it once the fixture has a `description.md`.
    if let CaseOrigin::Sample(source) = &app.origin
        && let Some(promoted) = promoted_case_name(source)
    {
        write_note(&promoted, comment)?;
        return Ok(if comment.is_empty() {
            format!("Cleared note for '{promoted}' (description.md removed)")
        } else {
            format!("Wrote description.md for '{promoted}' (this sample's promoted case)")
        });
    }

    // Written now, not on save: `app.dirty` tracks human_mapping.json, and a note riding it would
    // be lost to a quit-without-saving.
    if let CaseOrigin::Diffs = &app.origin {
        write_note(&app.name, comment)?;
        return Ok(if comment.is_empty() {
            format!("Cleared note for '{}' (description.md removed)", app.name)
        } else {
            format!("Wrote description.md for '{}'", app.name)
        });
    }

    let CaseOrigin::Sample(source) = &app.origin else {
        bail!("Only a diff (o) or a sample (O) can have a comment");
    };
    match set_sample_comment(source, comment)? {
        true if comment.is_empty() => Ok(format!("Cleared comment for '{}'", app.name)),
        true => Ok(format!("Set comment for '{}': {}", app.name, comment)),
        false => bail!("source row not found in sample.csv; not updated"),
    }
}

pub(crate) fn sample_csv_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("sample.csv")
}

pub(crate) struct SampleCsvRow {
    pub(crate) language: String,
    pub(crate) repository: String,
    pub(crate) commit: String,
    pub(crate) path: String,
    pub(crate) promoted_to: String,
    pub(crate) dataset: String,
    /// One of `SAMPLED`/`PROMOTED`/`REJECTED` - see `sample_test_diffs::Row::status`.
    pub(crate) status: String,
    /// Free-form note, independent of `status`. A rejection reason is stored here too.
    pub(crate) comment: String,
    /// See `sample_test_diffs::Row::size_bucket`. Unused here, but carried because every write
    /// rewrites the whole file, and a dropped column would be erased for every row.
    pub(crate) size_bucket: String,
}

/// The `status` of a row that has none; duplicates `sample_test_diffs::default_status`, as the
/// two binaries share no code.
pub(crate) fn default_sample_status(promoted_to: &str) -> &'static str {
    if promoted_to.is_empty() {
        "SAMPLED"
    } else {
        "PROMOTED"
    }
}

pub(crate) fn read_sample_csv_rows(path: &Path) -> Result<Vec<SampleCsvRow>> {
    let mut reader = csv::Reader::from_path(path).with_context(|| format!("reading {:?}", path))?;
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record?;
        let promoted_to = record.get(4).unwrap_or("").to_string();
        let status = match record.get(6) {
            Some(status) if !status.is_empty() => status.to_string(),
            _ => default_sample_status(&promoted_to).to_string(),
        };
        rows.push(SampleCsvRow {
            language: record[0].to_string(),
            repository: record[1].to_string(),
            commit: record[2].to_string(),
            path: record[3].to_string(),
            promoted_to,
            // Same fallback as `sample_test_diffs::LEGACY_DATASET`.
            dataset: record.get(5).unwrap_or("small").to_string(),
            status,
            comment: record.get(7).unwrap_or("").to_string(),
            size_bucket: record.get(8).unwrap_or("").to_string(),
        });
    }
    Ok(rows)
}

pub(crate) fn write_sample_csv_rows(path: &Path, rows: &[SampleCsvRow]) -> Result<()> {
    let mut writer = csv::Writer::from_path(path).with_context(|| format!("writing {:?}", path))?;
    writer.write_record([
        "language",
        "repository",
        "commit",
        "path",
        "promoted_to",
        "dataset",
        "status",
        "comment",
        "size_bucket",
    ])?;
    for row in rows {
        writer.write_record([
            &row.language,
            &row.repository,
            &row.commit,
            &row.path,
            &row.promoted_to,
            &row.dataset,
            &row.status,
            &row.comment,
            &row.size_bucket,
        ])?;
    }
    writer.flush()?;
    Ok(())
}

/// The fixture name this sample was promoted to, if any.
pub(crate) fn promoted_case_name(source: &SampleSource) -> Option<String> {
    let rows = read_sample_csv_rows(&sample_csv_path()).ok()?;
    find_sample_row(&rows, source)
        .map(|row| row.promoted_to.clone())
        .filter(|name| !name.trim().is_empty())
}

/// The sample.csv row for `source` (language/repository/commit/path). `sample_test_diffs` never
/// writes two rows for one commit+path, so the first match is the only one.
pub(crate) fn find_sample_row<'a>(
    rows: &'a [SampleCsvRow],
    source: &SampleSource,
) -> Option<&'a SampleCsvRow> {
    rows.iter().find(|row| {
        row.language == source.language
            && row.repository == source.repository
            && row.commit == source.commit
            && row.path == source.path
    })
}

pub(crate) fn find_sample_row_mut<'a>(
    rows: &'a mut [SampleCsvRow],
    source: &SampleSource,
) -> Option<&'a mut SampleCsvRow> {
    rows.iter_mut().find(|row| {
        row.language == source.language
            && row.repository == source.repository
            && row.commit == source.commit
            && row.path == source.path
    })
}

pub(crate) fn update_sample_csv(source: &SampleSource, new_name: &str) -> Result<bool> {
    update_sample_csv_at(&sample_csv_path(), source, new_name)
}

/// Marks the sample.csv row matching `source` as promoted to `new_name` and clears its comment.
/// `Ok(false)`, not an error, if no row matches (a hand-placed sample), so a missing row does not
/// fail a promotion that otherwise succeeded.
pub(crate) fn update_sample_csv_at(
    path: &Path,
    source: &SampleSource,
    new_name: &str,
) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    let mut rows = read_sample_csv_rows(path)?;
    let Some(row) = find_sample_row_mut(&mut rows, source) else {
        return Ok(false);
    };
    row.promoted_to = new_name.to_string();
    row.status = "PROMOTED".to_string();
    // The note has moved to the fixture's `description.md`; two copies would drift. A rejection
    // keeps its reason here, as a rejected sample has no directory.
    row.comment.clear();

    write_sample_csv_rows(path, &rows)?;
    Ok(true)
}

pub(crate) fn reject_sample(source: &SampleSource, reason: &str) -> Result<bool> {
    reject_sample_csv_at(&sample_csv_path(), source, reason)
}

/// Marks the sample.csv row matching `source` as rejected with `reason` as its comment, leaving
/// `promoted_to` alone. `Ok(false)` if no row matches.
pub(crate) fn reject_sample_csv_at(
    path: &Path,
    source: &SampleSource,
    reason: &str,
) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    let mut rows = read_sample_csv_rows(path)?;
    let Some(row) = find_sample_row_mut(&mut rows, source) else {
        return Ok(false);
    };
    row.comment = reason.to_string();
    row.status = "REJECTED".to_string();

    write_sample_csv_rows(path, &rows)?;
    Ok(true)
}

pub(crate) fn set_sample_comment(source: &SampleSource, comment: &str) -> Result<bool> {
    set_sample_comment_at(&sample_csv_path(), source, comment)
}

/// Sets the `comment` of the sample.csv row matching `source`, leaving every other column alone.
/// An empty `comment` clears it. `Ok(false)` if no row matches.
pub(crate) fn set_sample_comment_at(
    path: &Path,
    source: &SampleSource,
    comment: &str,
) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    let mut rows = read_sample_csv_rows(path)?;
    let Some(row) = find_sample_row_mut(&mut rows, source) else {
        return Ok(false);
    };
    row.comment = comment.to_string();

    write_sample_csv_rows(path, &rows)?;
    Ok(true)
}

pub(crate) fn sample_comment(source: &SampleSource) -> Result<Option<String>> {
    sample_comment_at(&sample_csv_path(), source)
}

/// The trimmed `comment` of `source`'s sample.csv row; `None` if missing or blank.
pub(crate) fn sample_comment_at(path: &Path, source: &SampleSource) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let rows = read_sample_csv_rows(path)?;
    Ok(find_sample_row(&rows, source)
        .map(|row| row.comment.trim().to_string())
        .filter(|c| !c.is_empty()))
}

/// `handle_modal_key`'s `Modal::SolutionPicker` arm.
#[allow(clippy::too_many_arguments)]
fn handle_solution_picker(
    app: &mut App,
    code: KeyCode,
    names: Vec<String>,
    selected: usize,
    saving: bool,
    new_name: Option<String>,
    confirm_delete: Option<String>,
    state: TextPaintState,
    // Read by save-as, to widen a `Minimal` painting to `Full`
    // (`expand_leading_whitespace_for_full`).
    before_src: &[u8],
    after_src: &[u8],
) -> Option<OpenTarget> {
    let before_text = std::str::from_utf8(before_src).unwrap_or_default();
    let after_text = std::str::from_utf8(after_src).unwrap_or_default();
    let reopen = |app: &mut App, names, selected, new_name, confirm_delete| {
        app.modal = Some(Modal::SolutionPicker {
            names,
            selected,
            saving,
            new_name,
            confirm_delete,
            state: state.clone(),
        });
    };
    // The free-form row always sits one past the named ones.
    let free_form_index = names.len();

    match (new_name, code) {
        // ── typing a new name ────────────────────────────────────────────────────────
        (Some(mut typed), KeyCode::Char(c)) => {
            typed.push(c);
            reopen(app, names, selected, Some(typed), None);
        }
        (Some(mut typed), KeyCode::Backspace) => {
            typed.pop();
            reopen(app, names, selected, Some(typed), None);
        }
        (Some(typed), KeyCode::Enter) => {
            if saving {
                action_save_solution_as(app, &typed, true, before_text, after_text);
            } else {
                action_load_solution(app, typed.trim());
            }
            app.modal = Some(Modal::TextView { state });
        }
        // Back to the list, not closed: a mistyped name costs one key, not the picker.
        (Some(_), KeyCode::Esc) => reopen(app, names, selected, None, None),
        (Some(typed), _) => reopen(app, names, selected, Some(typed), None),

        // ── choosing from the list ───────────────────────────────────────────────────
        (None, KeyCode::Up | KeyCode::Char('k')) => {
            reopen(app, names, selected.saturating_sub(1), None, None)
        }
        (None, KeyCode::Down | KeyCode::Char('j')) => {
            let next = (selected + 1).min(free_form_index);
            reopen(app, names, next, None, None)
        }
        (None, KeyCode::Enter) => {
            if selected == free_form_index {
                reopen(app, names, selected, Some(String::new()), None);
            } else {
                let chosen = names[selected].clone();
                if saving {
                    action_save_solution_as(app, &chosen, true, before_text, after_text);
                } else {
                    action_load_solution(app, &chosen);
                }
                app.modal = Some(Modal::TextView { state });
            }
        }
        // Like Enter, but starts the chosen name empty instead of from a copy of the painting.
        (None, KeyCode::Char('e')) if saving && selected < free_form_index => {
            let chosen = names[selected].clone();
            action_save_solution_as(app, &chosen, false, before_text, after_text);
            app.modal = Some(Modal::TextView { state });
        }
        // Twice, because there is no undo. The second press acts on the name the first one
        // armed, not on whatever row the cursor reached in between.
        (None, KeyCode::Char('D')) if selected < free_form_index => {
            let chosen = names[selected].clone();
            let exists = app
                .mapping
                .text_mappings
                .iter()
                .any(|named| named.name == chosen);
            if !exists {
                app.status = Some(format!(
                    "'{chosen}' is only a suggestion - nothing to delete"
                ));
                reopen(app, names, selected, None, None);
            } else if confirm_delete.as_deref() == Some(chosen.as_str()) {
                action_delete_solution(app, &chosen);
                let names = solution_picker_names(&app.mapping);
                let selected = selected.min(names.len());
                reopen(app, names, selected, None, None);
            } else {
                app.status = Some(format!("Press D again to delete '{chosen}'"));
                reopen(app, names, selected, None, Some(chosen));
            }
        }
        (None, KeyCode::Esc) => {
            app.status = Some("Cancelled".to_string());
            app.modal = Some(Modal::TextView { state });
        }
        (None, _) => reopen(app, names, selected, None, None),
    }

    None
}

/// `handle_modal_key`'s `Modal::TextView` arm.
fn handle_text_view(
    app: &mut App,
    code: KeyCode,
    mut state: TextPaintState,
    before_src: &[u8],
    after_src: &[u8],
    before: &Code,
    after: &Code,
) -> Option<OpenTarget> {
    // Cannot fail (`Code::contents` is a `String`); the fallback avoids a panic mid-session.
    let before_text = std::str::from_utf8(before_src).unwrap_or_default();
    let after_text = std::str::from_utf8(after_src).unwrap_or_default();
    // A conservative stand-in for the popup height, which only the renderer knows: it keeps
    // the cursor on screen on any terminal at least this tall.
    const VIEWPORT_ROWS: usize = 20;
    let focused_source = if state.side == 0 {
        before_text
    } else {
        after_text
    };
    let mut close = false;

    // The `:` prompt takes every keystroke, so a digit is not a movement command.
    if let Some(mut typed) = state.line_prompt.take() {
        match code {
            KeyCode::Char(c) if c.is_ascii_digit() => {
                typed.push(c);
                state.line_prompt = Some(typed);
            }
            KeyCode::Backspace => {
                typed.pop();
                state.line_prompt = Some(typed);
            }
            KeyCode::Enter => match typed.parse::<usize>() {
                // The gutter shows 1-based numbers.
                Ok(line) if line >= 1 => {
                    let last = TextPaintState::row_count(focused_source).saturating_sub(1);
                    let row = (line - 1).min(last);
                    state.cursor[state.side] = (row, 0);
                    app.status = Some(format!("Jumped to line {}", row + 1));
                }
                _ => app.status = Some(format!("Not a line number: {typed:?}")),
            },
            KeyCode::Esc => app.status = Some("Cancelled".to_string()),
            _ => state.line_prompt = Some(typed),
        }
        state.scroll_into_view(VIEWPORT_ROWS);
        app.modal = Some(Modal::TextView { state });
        return None;
    }

    match code {
        KeyCode::Tab => state.side = 1 - state.side,
        KeyCode::Char(':') => {
            state.line_prompt = Some(String::new());
            app.status =
                Some("Jump to line: type a number, Enter to go, Esc to cancel".to_string());
        }
        KeyCode::Up | KeyCode::Char('k') => state.step_row(-1, focused_source),
        KeyCode::Down | KeyCode::Char('j') => state.step_row(1, focused_source),
        KeyCode::Left | KeyCode::Char('h') => state.step_column(false, focused_source),
        KeyCode::Right | KeyCode::Char('l') => state.step_column(true, focused_source),
        KeyCode::PageUp => state.step_row(-(VIEWPORT_ROWS as isize), focused_source),
        KeyCode::PageDown => state.step_row(VIEWPORT_ROWS as isize, focused_source),
        KeyCode::Char('0') | KeyCode::Home => state.cursor[state.side].1 = 0,
        // As in vi. Where a painted range wants to start: `0` would sweep in the indentation.
        KeyCode::Char('^') => {
            let row = state.cursor[state.side].0;
            state.cursor[state.side].1 = TextPaintState::first_code_column(focused_source, row);
        }
        KeyCode::Char('$') | KeyCode::End => {
            let row = state.cursor[state.side].0;
            state.cursor[state.side].1 = TextPaintState::row_text(focused_source, row).len();
        }
        KeyCode::Char('g') => state.cursor[state.side] = (0, 0),
        KeyCode::Char('G') => {
            let last = TextPaintState::row_count(focused_source).saturating_sub(1);
            state.cursor[state.side] = (last, 0);
        }
        KeyCode::Char('v') => {
            state.anchor[state.side] = match state.anchor[state.side] {
                Some(_) => None,
                None => Some(state.cursor[state.side]),
            };
            let mode = if state.vertical {
                "vertical"
            } else {
                "full-line"
            };
            app.status = Some(match state.anchor[state.side] {
                Some(_) => format!("Selecting ({mode}) - move, then d/i/m"),
                None => "Selection cleared".to_string(),
            });
        }
        // Vertical selects the same columns on each row; full-line sweeps rows end to end, which
        // a contiguous multi-line block needs because `m` requires identical text on both sides.
        KeyCode::Char('V') => {
            state.vertical = !state.vertical;
            let mode = if state.vertical {
                "vertical"
            } else {
                "full-line"
            };
            app.status = Some(format!("Selections are now {mode}"));
        }
        // As in the tree panels: banking is what makes an N:M match reachable, since one live
        // selection describes one range.
        KeyCode::Char('x') => {
            let spans = state.selection(state.side, focused_source);
            if spans.is_empty() {
                app.status = Some("Nothing selected to bank - press v first".to_string());
            } else {
                state.pending[state.side].extend(spans);
                state.anchor[state.side] = None;
                let banked = state.pending[state.side].len();
                app.status = Some(format!(
                    "Banked {banked} range(s) on this side - select another, then d/i/m"
                ));
            }
        }
        KeyCode::Char('c') => {
            state.pending = [Vec::new(), Vec::new()];
            state.anchor = [None; 2];
            app.status = Some("Cleared banked ranges on both sides".to_string());
        }
        KeyCode::Char('m') => action_paint_match(app, &mut state, before_text, after_text),
        KeyCode::Char('d') => action_paint_one_sided(
            app,
            &mut state,
            HumanTextOperation::Delete,
            before_text,
            after_text,
        ),
        KeyCode::Char('i') => action_paint_one_sided(
            app,
            &mut state,
            HumanTextOperation::Insert,
            before_text,
            after_text,
        ),
        KeyCode::Char('u') => action_paint_unmark(app, &state, before_text, after_text),
        KeyCode::Char('Z') => action_paint_mark_empty(app),
        KeyCode::Char('n') => action_paint_next_diff(
            app,
            &mut state,
            before_text,
            after_text,
            true,
            VIEWPORT_ROWS,
        ),
        KeyCode::Char('p') => action_paint_next_diff(
            app,
            &mut state,
            before_text,
            after_text,
            false,
            VIEWPORT_ROWS,
        ),
        KeyCode::Char('a') => {
            action_paint_align(app, &mut state, before_text, after_text, VIEWPORT_ROWS)
        }
        // Unlike `a`, aligns this side's *tree* panel with the text cursor.
        KeyCode::Char('A') => action_paint_reveal_node(app, &state, before, after),
        // Adopts codediff's rendering (what `o` shows) as the draft to correct.
        KeyCode::Char('P') => action_paint_seed_from_codediff(app, before, after),
        KeyCode::Char('o') => {
            let next = app.text_overlay.next();
            // Lazy, and kept for the case: running codediff is slow on a large fixture.
            if next != TextOverlay::Human && app.algo_text_spans.is_none() {
                app.algo_text_spans = Some(codediff_text_spans(before, after));
            }
            if next == TextOverlay::TreeDisagreement && app.tree_text_spans.is_none() {
                app.tree_text_spans = Some(tree_mapping_text_spans(&app.mapping, before, after));
            }
            app.text_overlay = next;
            let human_spans_for_status = || {
                [
                    painted_spans(&app.mapping, &app.text_solution, 0, before_text, after_text),
                    painted_spans(&app.mapping, &app.text_solution, 1, before_text, after_text),
                ]
            };
            app.status = Some(match next {
                TextOverlay::Human => "Showing your painting".to_string(),
                TextOverlay::CodeDiff => "Showing codediff's own diff".to_string(),
                TextOverlay::Disagreements => {
                    let differing: usize = app
                        .algo_text_spans
                        .as_ref()
                        .map(|algo| {
                            overlay_disagreement_spans(
                                &human_spans_for_status(),
                                algo,
                                before_text,
                                after_text,
                            )
                            .iter()
                            .map(Vec::len)
                            .sum()
                        })
                        .unwrap_or(0);
                    if differing == 0 {
                        "You and codediff agree everywhere".to_string()
                    } else {
                        format!("Showing {differing} disagreeing range(s)")
                    }
                }
                TextOverlay::TreeDisagreement => {
                    let differing: usize = app
                        .tree_text_spans
                        .as_ref()
                        .map(|tree| {
                            overlay_disagreement_spans(
                                &human_spans_for_status(),
                                tree,
                                before_text,
                                after_text,
                            )
                            .iter()
                            .map(Vec::len)
                            .sum()
                        })
                        .unwrap_or(0);
                    if differing == 0 {
                        "Your painting and your tree mapping agree everywhere".to_string()
                    } else {
                        format!(
                            "Showing {differing} disagreeing range(s) between your painting and your tree mapping"
                        )
                    }
                }
            });
        }
        KeyCode::Char('s') | KeyCode::Char('L') => {
            let saving = matches!(code, KeyCode::Char('s'));
            app.modal = Some(Modal::SolutionPicker {
                names: solution_picker_names(&app.mapping),
                selected: 0,
                saving,
                new_name: None,
                confirm_delete: None,
                state,
            });
            return None;
        }
        KeyCode::Char('T') => match run_unix_diff(before_src, after_src) {
            Ok(output) => {
                app.modal = Some(Modal::UnixDiffView { output, scroll: 0 });
                return None;
            }
            Err(err) => app.status = Some(format!("Error running diff: {:#}", err)),
        },
        KeyCode::Esc => {
            // One step at a time, so an accidental `v` or a half-built N:M group does not
            // cost the whole view.
            if state.anchor[state.side].is_some() {
                state.anchor[state.side] = None;
                app.status = Some("Selection cleared".to_string());
            } else if !state.pending[state.side].is_empty() {
                state.pending[state.side].clear();
                app.status = Some("Banked ranges cleared on this side".to_string());
            } else {
                close = true;
            }
        }
        _ => {}
    }

    if close {
        app.status = Some("Closed text view".to_string());
    } else {
        state.scroll_into_view(VIEWPORT_ROWS);
        app.modal = Some(Modal::TextView { state });
    }

    None
}

/// `handle_modal_key`'s `Modal::OpenSamplePicker` arm. Deliberately parallel to
/// `handle_open_diff_picker`: the same table over a different corpus (see `SampleColumn` on why
/// the types are not shared).
fn handle_open_sample_picker(
    app: &mut App,
    code: KeyCode,
    rows: Vec<SampleRow>,
    selected: usize,
    view: SamplePickerView,
    name_input: Option<String>,
) -> Option<OpenTarget> {
    let mut view = view;
    let mut selected = selected;

    // The `Name` filter prompt takes every keystroke, so `j`/`s`/`f` type rather than act.
    if let Some(mut typed) = name_input {
        match code {
            KeyCode::Char(c) => {
                typed.push(c);
                app.modal = Some(Modal::OpenSamplePicker {
                    rows,
                    selected,
                    view,
                    name_input: Some(typed),
                });
            }
            KeyCode::Backspace => {
                typed.pop();
                app.modal = Some(Modal::OpenSamplePicker {
                    rows,
                    selected,
                    view,
                    name_input: Some(typed),
                });
            }
            KeyCode::Enter => {
                let current = visible_sample_rows(&rows, &view)
                    .get(selected)
                    .map(|row| row.name.clone());
                let needle = typed.trim().to_lowercase();
                view.filters.name = if needle.is_empty() {
                    None
                } else {
                    Some(needle)
                };
                app.sample_view = view.clone();
                let modal =
                    open_sample_picker_modal(rows, current.as_deref().unwrap_or(&app.name), view);
                app.modal = Some(modal);
            }
            KeyCode::Esc => {
                app.status = Some("Name filter cancelled".to_string());
                app.modal = Some(Modal::OpenSamplePicker {
                    rows,
                    selected,
                    view,
                    name_input: None,
                });
            }
            _ => {
                app.modal = Some(Modal::OpenSamplePicker {
                    rows,
                    selected,
                    view,
                    name_input: Some(typed),
                });
            }
        }
        return None;
    }

    let visible = visible_sample_rows(&rows, &view);
    match code {
        KeyCode::Up | KeyCode::Char('k') => {
            selected = selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            selected = (selected + 1).min(visible.len().saturating_sub(1));
        }
        KeyCode::Left | KeyCode::Char('h') => {
            view.column = view.column.left();
        }
        KeyCode::Right | KeyCode::Char('l') => {
            view.column = view.column.right();
        }
        KeyCode::Char('s') => {
            let current = visible.get(selected).map(|row| row.name.clone());
            view.sort = view.sort.toggled(view.column);
            app.sample_view = view.clone();
            let modal =
                open_sample_picker_modal(rows, current.as_deref().unwrap_or(&app.name), view);
            app.modal = Some(modal);
            return None;
        }
        KeyCode::Char('f') => {
            let current = visible.get(selected).map(|row| row.name.clone());
            match view.column {
                SampleColumn::Name => {
                    app.status = Some(
                        "Filter by name: type a substring, Enter to apply, Esc to cancel"
                            .to_string(),
                    );
                    let name_input = view.filters.name.clone().unwrap_or_default();
                    app.modal = Some(Modal::OpenSamplePicker {
                        rows,
                        selected,
                        view,
                        name_input: Some(name_input),
                    });
                    return None;
                }
                SampleColumn::Lang => {
                    let values = sample_language_values(&rows);
                    view.filters.language =
                        next_value_filter(view.filters.language.as_deref(), &values);
                }
                SampleColumn::Bucket => {
                    let values = sample_bucket_values(&rows);
                    view.filters.bucket =
                        next_value_filter(view.filters.bucket.as_deref(), &values);
                }
                SampleColumn::Status => {
                    view.filters.status = next_status_filter(view.filters.status);
                }
                SampleColumn::Size => {
                    view.filters.size = view.filters.size.next();
                }
            }
            app.sample_view = view.clone();
            let modal =
                open_sample_picker_modal(rows, current.as_deref().unwrap_or(&app.name), view);
            app.modal = Some(modal);
            return None;
        }
        KeyCode::Enter => {
            if let Some(row) = visible.get(selected) {
                let target = OpenTarget::Sample(row.name.clone());
                if app.dirty {
                    let can_save = matches!(app.origin, CaseOrigin::Diffs);
                    app.modal = Some(Modal::ConfirmDiscardUnsaved { target, can_save });
                } else {
                    return Some(target);
                }
                return None;
            }
        }
        KeyCode::Esc => {
            app.status = Some("Cancelled".to_string());
            return None;
        }
        _ => {}
    }
    // Persists `h`/`l`'s column across reopening the picker, as the sort and filters do.
    app.sample_view = view.clone();
    app.modal = Some(Modal::OpenSamplePicker {
        rows,
        selected,
        view,
        name_input: None,
    });

    None
}

/// `handle_modal_key`'s `Modal::OpenDiffPicker` arm.
fn handle_open_diff_picker(
    app: &mut App,
    code: KeyCode,
    options: Vec<(String, &'static str)>,
    selected: usize,
    view: DiffPickerView,
    name_input: Option<String>,
) -> Option<OpenTarget> {
    let mut view = view;
    let mut selected = selected;

    // The `Name` filter prompt takes every keystroke, so `j`/`s`/`f` type rather than act.
    if let Some(mut typed) = name_input {
        match code {
            KeyCode::Char(c) => {
                typed.push(c);
                app.modal = Some(Modal::OpenDiffPicker {
                    options,
                    selected,
                    view,
                    name_input: Some(typed),
                });
            }
            KeyCode::Backspace => {
                typed.pop();
                app.modal = Some(Modal::OpenDiffPicker {
                    options,
                    selected,
                    view,
                    name_input: Some(typed),
                });
            }
            KeyCode::Enter => {
                // Taken before the filter narrows the list, so the selection follows its row.
                let current = visible_diff_options(&options, &view, DiffPickerData::from_app(app))
                    .get(selected)
                    .cloned();
                let needle = typed.trim().to_lowercase();
                view.filters.name = if needle.is_empty() {
                    None
                } else {
                    Some(needle)
                };
                app.diff_view = view.clone();
                let modal = open_diff_picker_modal(
                    options,
                    current.as_deref().unwrap_or(&app.name),
                    view,
                    DiffPickerData::from_app(app),
                );
                app.modal = Some(modal);
            }
            KeyCode::Esc => {
                app.status = Some("Name filter cancelled".to_string());
                app.modal = Some(Modal::OpenDiffPicker {
                    options,
                    selected,
                    view,
                    name_input: None,
                });
            }
            _ => {
                app.modal = Some(Modal::OpenDiffPicker {
                    options,
                    selected,
                    view,
                    name_input: Some(typed),
                });
            }
        }
        return None;
    }

    let visible = visible_diff_options(&options, &view, DiffPickerData::from_app(app));
    match code {
        KeyCode::Up | KeyCode::Char('k') => {
            selected = selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            selected = (selected + 1).min(visible.len().saturating_sub(1));
        }
        // Never triggers a scan (`ensure_diff_column_data`): only the deliberate `s`/`f` may stall.
        KeyCode::Left | KeyCode::Char('h') => {
            view.column = view.column.left();
        }
        KeyCode::Right | KeyCode::Char('l') => {
            view.column = view.column.right();
        }
        KeyCode::Char('s') => {
            let current = visible.get(selected).cloned();
            ensure_diff_column_data(app, view.column);
            view.sort = view.sort.toggled(view.column);
            app.diff_view = view.clone();
            let modal = open_diff_picker_modal(
                options,
                current.as_deref().unwrap_or(&app.name),
                view,
                DiffPickerData::from_app(app),
            );
            app.modal = Some(modal);
            return None;
        }
        KeyCode::Char('f') => {
            let current = visible.get(selected).cloned();
            if view.column == DiffColumn::Name {
                app.status = Some(
                    "Filter by name: type a substring, Enter to apply, Esc to cancel".to_string(),
                );
                let name_input = view.filters.name.clone().unwrap_or_default();
                app.modal = Some(Modal::OpenDiffPicker {
                    options,
                    selected,
                    view,
                    name_input: Some(name_input),
                });
                return None;
            }
            ensure_diff_column_data(app, view.column);
            if view.column == DiffColumn::Dataset {
                view.filters.dataset = next_dataset_filter(view.filters.dataset);
            } else if let Some(flag) = view.filters.flag_mut(view.column) {
                *flag = flag.next();
            }
            app.diff_view = view.clone();
            let modal = open_diff_picker_modal(
                options,
                current.as_deref().unwrap_or(&app.name),
                view,
                DiffPickerData::from_app(app),
            );
            app.modal = Some(modal);
            return None;
        }
        KeyCode::Enter => {
            if let Some(name) = visible.get(selected) {
                let target = OpenTarget::Diffs(name.clone());
                if app.dirty {
                    let can_save = matches!(app.origin, CaseOrigin::Diffs);
                    app.modal = Some(Modal::ConfirmDiscardUnsaved { target, can_save });
                } else {
                    return Some(target);
                }
                return None;
            }
        }
        KeyCode::Esc => {
            app.status = Some("Cancelled".to_string());
            return None;
        }
        _ => {}
    }
    // Persists `h`/`l`'s column across reopening the picker, as the sort and filters do.
    app.diff_view = view.clone();
    app.modal = Some(Modal::OpenDiffPicker {
        options,
        selected,
        view,
        name_input: None,
    });

    None
}
