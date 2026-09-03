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
//!
//! Split out of `main.rs` along the section banner that already marked this boundary.

#[allow(unused_imports)]
use crate::*;

// ---------------------------------------------------------------------------------------------
// Event loop
// ---------------------------------------------------------------------------------------------

/// Everything derived from the current trees, mapping, and collapse/hide state that drawing a
/// frame or interpreting a keystroke needs. Rebuilding this is the expensive part of the loop (a
/// handful of whole-tree passes); see `run_event_loop`'s `needs_redraw` for why it only happens
/// once per keystroke rather than on every idle poll timeout.
pub(crate) struct FrameState<'a> {
    pub(crate) before_root: Node<'a>,
    pub(crate) after_root: Node<'a>,
    pub(crate) before_src: &'a [u8],
    pub(crate) after_src: &'a [u8],
    pub(crate) caches: Caches,
    pub(crate) before_flat: FlatIndex<'a>,
    pub(crate) after_flat: FlatIndex<'a>,
    /// Counts of `Unmarked` nodes in `before_flat`/`after_flat`, for `render_panel`'s "N unmarked"
    /// header. Computed once here rather than by scanning all of `flat` on every single draw call
    /// (see `render_panel`), since on a large case that scan -- calling `status_before`/
    /// `status_after` on every node, not just the visible ones -- was itself a real cost paid every
    /// frame for no reason: it only changes when this `FrameState` does.
    pub(crate) before_unmarked: usize,
    pub(crate) after_unmarked: usize,
}

/// The number of `flat`'s nodes with `NodeStatus::Unmarked`, per `status_fn`.
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
    let before_root = before
        .ast
        .as_ref()
        .context("Before code has no AST")?
        .root_node();
    let after_root = after
        .ast
        .as_ref()
        .context("After code has no AST")?
        .root_node();
    let before_src = before.contents.as_bytes();
    let after_src = after.contents.as_bytes();

    let caches = rebuild_caches_for_mapping(&app.mapping, before_root, after_root);

    // Recomputed fresh whenever frame state is rebuilt, so `H` can't show a subtree as hidden
    // after it's actually been un-marked, or vice versa.
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
        before_root,
        after_root,
        before_src,
        after_src,
        caches,
        before_flat,
        after_flat,
        before_unmarked,
        after_unmarked,
    })
}

/// What a case session (`run_case_session`) ended on: either the user quit, or a modal asked to
/// switch to a different case (`o`/`O`'s pickers, or a discard-unsaved confirmation).
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
        // `run_case_session` only ever reads `before`/`after` (never reassigns them), so it's free
        // to cache state that borrows from them for as long as the whole session runs, with none of
        // the self-referential-across-a-mutation problem that caching across *this* loop's own
        // iterations would run into: those iterations are exactly the ones that reassign `before`/
        // `after` below.
        match run_case_session(terminal, app, &before, &after)? {
            SessionEnd::Quit => break,
            SessionEnd::Open(OpenTarget::Diffs(name)) => match load_case(&name) {
                Ok((new_before, new_after)) => {
                    let before_root_id = new_before.ast.as_ref().unwrap().root_node().id();
                    let after_root_id = new_after.ast.as_ref().unwrap().root_node().id();
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
                    // Follow the newly-opened case's own paintings rather than carrying the last
                    // case's solution name into it, which would silently start a second, near-
                    // duplicate painting under a name that means nothing here.
                    app.text_solution = starting_solution(&app.mapping);
                    app.before_multi_select.clear();
                    app.after_multi_select.clear();
                    app.status = Some(format!("Opened '{}'", app.name));
                }
                Err(err) => {
                    app.status = Some(format!("Error opening '{}': {:#}", name, err));
                }
            },
            SessionEnd::Open(OpenTarget::Sample(name)) => match load_sample(&name) {
                Ok((new_before, new_after, source)) => {
                    let before_root_id = new_before.ast.as_ref().unwrap().root_node().id();
                    let after_root_id = new_after.ast.as_ref().unwrap().root_node().id();
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
                    // Follow the newly-opened case's own paintings rather than carrying the last
                    // case's solution name into it, which would silently start a second, near-
                    // duplicate painting under a name that means nothing here.
                    app.text_solution = starting_solution(&app.mapping);
                    app.before_multi_select.clear();
                    app.after_multi_select.clear();
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
                    let before_root_id = new_before.ast.as_ref().unwrap().root_node().id();
                    let after_root_id = new_after.ast.as_ref().unwrap().root_node().id();
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
                    // Follow the newly-opened case's own paintings rather than carrying the last
                    // case's solution name into it, which would silently start a second, near-
                    // duplicate painting under a name that means nothing here.
                    app.text_solution = starting_solution(&app.mapping);
                    app.before_multi_select.clear();
                    app.after_multi_select.clear();
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

/// Keys `handle_key` (only reachable when no modal is open -- see `is_state_preserving_key`)
/// processes without touching `App::mapping`, either panel's `collapsed` set, or
/// `App::hide_solved`: the three things `run_case_session`'s cached `FrameState` depends on. Pure
/// cursor movement (`j`/`k`/arrows/`Tab`/`g`/`G`/`n`/`N`), the multi-map selection (`x`/`c`, read
/// directly off `App` by `render_panel`, not through `FrameState`), and view/display toggles
/// (`p`/`r`/`t`/`T`/`/`/`?`) whose own state (`algo_diff`, `show_reason`) is likewise read
/// straight off `App`. On a large case, rebuilding `FrameState` for every one of these -- which is
/// what browsing a case mostly consists of -- used to mean paying `rebuild_caches_for_mapping`
/// (documented up to ~2s on a heavily-annotated fixture), two `fully_solved_nodes` walks, and two
/// `flatten_visible` walks on every single keystroke, whether or not anything `FrameState` derives
/// from had actually changed.
///
/// Deliberately conservative: `h`/`l`/`a`/`A` (which sometimes mutate a `collapsed` set, depending
/// on where the cursor already is) and `s`/`R`/`o`/`O`/`C` (which open a modal or save, and are
/// rare enough that the existing full-rebuild cost isn't worth the extra classification surface)
/// are NOT included here, even though some of their branches don't actually need a rebuild either
/// -- see `handle_key` for the exact effect of every key this list omits.
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
/// either panel's collapsed set, or `hide_solved` -- the three things `run_case_session`'s cached
/// `FrameState` depends on.
///
/// With no modal open, this is `is_navigation_or_display_key` (`handle_key`'s own pure-navigation
/// keys). With a modal open, only typing into (or backspacing out of) `PromptSearch`'s,
/// `PromptPromoteName`'s, `PromptRejectReason`'s, or `PromptComment`'s own `input` string
/// qualifies: those modals only ever mutate that string in response to these keys, everything else
/// about the case is untouched. Every other key while a modal is open -- including Enter/Esc on
/// these same four modals, which can search-and-move-the-cursor, promote/reject/comment/save, or
/// close the modal -- is treated conservatively as "might have changed something", so the cache is
/// thrown away and rebuilt fresh, exactly as if this function didn't exist.
pub(crate) fn is_state_preserving_key(modal: Option<&Modal>, code: KeyCode) -> bool {
    match modal {
        None => is_navigation_or_display_key(code),
        Some(Modal::PromptSearch { .. })
        | Some(Modal::PromptPromoteName { .. })
        | Some(Modal::PromptRejectReason { .. })
        | Some(Modal::PromptComment { .. }) => {
            matches!(code, KeyCode::Char(_) | KeyCode::Backspace)
        }
        // The text-painting views cannot invalidate the cached `FrameState`, so *every* key in
        // them preserves it - including the ones that write.
        //
        // `FrameState` caches the flattened trees and the `Caches` built from them, and
        // `rebuild_caches_for_mapping` reads only `entries`/`groups`. Painting writes
        // `text_mappings`, a separate ground truth that no tree state is derived from (see
        // `HumanTextMapping`), so nothing the `t` view does can make the cache wrong.
        //
        // This is a correctness observation with a large performance consequence. Falling through
        // to `false` re-flattened both ASTs and rebuilt every cache on each cursor keystroke; on a
        // ~900 KB fixture that is a full walk of a few hundred thousand nodes per keypress, which
        // is what made the view unusable on big files. The painting view is exactly where a reader
        // holds down `j`.
        Some(Modal::TextView { .. })
        | Some(Modal::SolutionPicker { .. })
        | Some(Modal::UnixDiffView { .. }) => true,
        Some(_) => false,
    }
}

/// Runs the event loop for a single case (before/after AST pair) until the user quits or asks to
/// switch to a different one. Split out from `run_event_loop` specifically so the `state` cache
/// below -- which borrows from `before`/`after` -- never has to coexist with a reassignment of
/// them: `before`/`after` are `&Code` here, immutable for this whole call, so the cache is free to
/// survive across as many keystrokes as it likes with no lifetime conflict. A case switch is
/// reported back to the caller as a `SessionEnd::Open` instead of being handled in place.
pub(crate) fn run_case_session(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    before: &Code,
    after: &Code,
) -> Result<SessionEnd> {
    // Whether the on-screen frame reflects the current state. Set whenever something might have
    // changed (a key was handled, the terminal was resized) and cleared right after redrawing. On
    // a pure idle poll timeout -- the common case, since the TUI just sits there most of the time
    // -- nothing is redrawn at all.
    let mut needs_redraw = true;

    // Cached result of `compute_frame_state` -- rebuilding both caches and re-flattening both
    // (possibly multi-thousand-node) trees is real work, so it's only redone when something that
    // could actually change it happens, not on every idle tick or every keystroke. `None` forces a
    // fresh (potentially expensive) recompute; set back to `None` only by a key that could
    // plausibly touch the mapping, a collapsed set, or `hide_solved` -- see
    // `is_state_preserving_key`/`is_navigation_or_display_key` for the exact set that's exempted.
    // That set is deliberately generous: browsing a case (moving the cursor, jumping between
    // mismatches, toggling `p`/`r`/`t`/`T` display) is the overwhelming majority of keys pressed in
    // a session, and none of it needs a rebuild, so those keys reuse this `FrameState` untouched.
    // Only the keys that actually mutate the mapping or a collapsed set (`m`/`M`/`f`/`d`/`D`/`i`/
    // `I`/`u`/`h`/`l`/`a`/`A`/`H`, plus text-modal Enter/Esc) still pay the full recompute.
    let mut state: Option<FrameState> = None;

    loop {
        if state.is_none() {
            state = Some(compute_frame_state(before, after, app)?);
        }
        let frame_state = state.as_ref().expect("just populated above if empty");

        if needs_redraw {
            // Cloned rather than borrowed from `app`: draw_ui also takes `app: &mut App`, and
            // passing both `app` and `&app.name` as separate arguments to the same call would
            // conflict.
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
                )
            })?;
            needs_redraw = false;
        }

        if !event::poll(Duration::from_millis(250))? {
            continue;
        }

        let event = event::read()?;
        let Event::Key(key) = event else {
            // e.g. a resize: nothing to recompute, just redraw at the (possibly new) size.
            needs_redraw = true;
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        // `load_case` runs `ensure_parsed` on both sides, so full-content hashes are always
        // available here; used by `m`/`M` to decide Identical vs MatchButNotIdentical for nodes
        // with children without asking (see `subtree_match_operation`).
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
        } else {
            handle_key(
                app,
                key.code,
                &frame_state.before_flat,
                &frame_state.after_flat,
                frame_state.before_root,
                frame_state.after_root,
                &frame_state.caches,
                frame_state.before_src,
                frame_state.after_src,
                before_hash,
                after_hash,
                before,
                after,
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
        KeyCode::Char('q') | KeyCode::Esc => {
            app.should_quit = true;
            None
        }
        KeyCode::Char('?') => {
            app.modal = Some(Modal::Help { scroll: 0 });
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
                )
            };
            match outcome {
                Ok(ActionOutcome::Done(msg)) => {
                    app.dirty = true;
                    app.status = Some(msg);
                    app.before_multi_select.clear();
                    app.after_multi_select.clear();
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
                )
            };
            match outcome {
                Ok(ActionOutcome::Done(msg)) => {
                    app.dirty = true;
                    app.status = Some(msg);
                    app.before_multi_select.clear();
                    app.after_multi_select.clear();
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
                    advance_before_to_next_unmarked(app, before_flat, before_root, after_root);
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
                    advance_after_to_next_unmarked(app, after_flat, before_root, after_root);
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
            app.status = Some(format!(
                "Multi-map selection: {} before, {} after node(s) (m/M to commit as a group, c to clear)",
                app.before_multi_select.len(),
                app.after_multi_select.len()
            ));
            None
        }
        KeyCode::Char('c') => {
            app.before_multi_select.clear();
            app.after_multi_select.clear();
            app.status = Some("Cleared multi-map selection".to_string());
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
        KeyCode::Char('t') => {
            app.modal = Some(Modal::TextView {
                state: TextPaintState::default(),
            });
            None
        }
        KeyCode::Char('T') => {
            match run_unix_diff(before_src, after_src) {
                Ok(output) => app.modal = Some(Modal::UnixDiffView { output, scroll: 0 }),
                Err(err) => app.status = Some(format!("Error running diff: {:#}", err)),
            }
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
        KeyCode::Char('s') => match &app.origin {
            CaseOrigin::Diffs => {
                let result = action_save(&mut app.mapping, &mut app.dirty, &app.name, None);
                if result.is_ok() {
                    refresh_diff_unmarked(app, &app.name.clone());
                    refresh_diff_text_painted(app, &app.name.clone());
                    refresh_diff_disagreement(app, &app.name.clone());
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
                // Just the note: a promoted fixture's sample.csv row no longer carries a comment
                // to fall back to, because `action_promote` moves it into `description.md` and
                // clears the cell rather than leaving a second copy behind.
                let existing = read_note(&app.name).unwrap_or_default();
                app.modal = Some(Modal::PromptComment {
                    input: existing,
                    error: None,
                });
            } else if let CaseOrigin::Sample(source) = &app.origin {
                // Pre-fill with whatever's already recorded, so this is an edit, not a blind
                // overwrite - same idea as `PromptPromoteName`'s pre-filled default name.
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
            // Loaded here, unlike the unmarked/painted/disagreement maps which wait for the key
            // that sorts or filters by them: notes are *displayed*, so they have to be present the
            // first time the list is drawn. Affordable exactly because this scan is the cheap one
            // - a stat and a short read per fixture, and most have no note at all.
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
                    // Only the names we have not already measured, scanned in parallel: this is
                    // one external `diff` per sample, so all 1489 of them serially cost 3.9s of
                    // frozen picker on every `O` (1.9s across the scan threads, and nothing at
                    // all on the presses after the first - see `sample_diff_sizes`).
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
        _ => None,
    };

    if let Some(res) = result {
        app.status = Some(match res {
            Ok(msg) => msg,
            Err(err) => format!("Error: {:#}", err),
        });
    }
}

/// Routes a keypress while `app.modal` is `Some`. Returns `Some(name)` when the human just
/// confirmed switching to a different test case (via the open picker, possibly after a save/
/// discard decision): the caller is responsible for actually loading it, since that needs
/// mutable access to the owned `Code` values that `run_event_loop` holds, which can't be threaded
/// down here alongside `Node`s borrowed from them.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_modal_key(
    app: &mut App,
    code: KeyCode,
    before_flat: &FlatIndex,
    after_flat: &FlatIndex,
    before_root: Node,
    after_root: Node,
    caches: &Caches,
    before_src: &[u8],
    after_src: &[u8],
    // Needed only by the text view's `p` (running codediff to show its own rendering), but the
    // modal handler is one function, so it takes the pair the same way `handle_key` does.
    before: &Code,
    after: &Code,
) -> Option<OpenTarget> {
    let modal = app.modal.take()?;

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
            kinds,
        } => match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let before_set: std::collections::BTreeSet<usize> =
                    before_ids.iter().copied().collect();
                let after_set: std::collections::BTreeSet<usize> =
                    after_ids.iter().copied().collect();
                app.status = Some(
                    match commit_multi_map_group(
                        &mut app.mapping,
                        before_root,
                        after_root,
                        &before_set,
                        &after_set,
                        operation,
                        with_children,
                    ) {
                        Ok(msg) => {
                            app.dirty = true;
                            msg
                        }
                        Err(err) => format!("Error: {:#}", err),
                    },
                );
                app.before_multi_select.clear();
                app.after_multi_select.clear();
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                app.status = Some("Cancelled: multi-map group has mixed node kinds".to_string());
                app.before_multi_select.clear();
                app.after_multi_select.clear();
            }
            _ => {
                app.modal = Some(Modal::ConfirmMultiMapGroup {
                    before_ids,
                    after_ids,
                    operation,
                    with_children,
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
                match action_save(&mut app.mapping, &mut app.dirty, &app.name, None) {
                    Ok(_) => {
                        refresh_diff_unmarked(app, &app.name.clone());
                        refresh_diff_text_painted(app, &app.name.clone());
                        refresh_diff_disagreement(app, &app.name.clone());
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

/// `comment` is only ever `Some` from `action_promote` (a sample's recorded `Modal::PromptComment`
/// text, if any) - the plain save path (`s` on an already-real `CaseOrigin::Diffs` case) always
/// passes `None`, since only samples have a comment to carry forward. Only takes effect when
/// `ensure_stub_test` is *creating* the stub file for the first time; a comment added or edited
/// after promotion has no generated file left to write into.
pub(crate) fn action_save(
    mapping: &mut HumanMapping,
    dirty: &mut bool,
    name: &str,
    comment: Option<&str>,
) -> Result<String> {
    human_mapping::save(name, mapping)?;
    let created = ensure_stub_test(name, comment)?;
    // Only once there is something to score: an unpainted fixture has no painting for the test to
    // compare against, and a stub for it would fail rather than report a distance.
    if !mapping.text_mappings.is_empty() {
        ensure_painting_stub_test(name)?;
    }
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

/// Rust keywords (2015 through 2024 edition, strict and reserved). `module_name` turns a case
/// name directly into a module identifier (`-` -> `_`), so a name that collides with one of these
/// would produce a stub that fails to compile -- caught here instead, before anything is written.
pub(crate) const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while", "abstract", "become", "box", "do", "final", "gen", "macro",
    "override", "priv", "try", "typeof", "unsized", "virtual", "yield",
];

/// A name must be non-empty, start with a letter (so `module_name` -- which just swaps `-` for
/// `_` -- produces a valid Rust identifier) and contain only characters safe to use directly as
/// a directory name.
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

/// Promotes the currently open sample or git-commit-sourced case (`app.origin` must be
/// `CaseOrigin::Sample` or `CaseOrigin::GitCommitFile`) into a real test case under
/// `src/test/data/diffs/<dataset>/<new_name>/` (`dataset` per `promote_target_dataset`: a
/// sample's own recorded `source.dataset`, or always `"handmade"` for a git-commit-sourced case):
/// copies the before/after content sitting in `before_src`/`after_src` (the same bytes currently
/// on screen), saves human_mapping.json and the optimal_solutions stub via the normal
/// `action_save` path, and -- for a sample only, since a git-commit-sourced case has no
/// sample.csv row to update -- records `new_name` against the matching row in sample.csv. On
/// success, `app` is switched over to the new diffs/ case so subsequent `s` presses behave like a
/// normal save.
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
        // The sample's own recorded provenance decides the target dataset folder, not a hardcoded
        // guess - see `SampleSource::dataset`. Checked against `DIFF_DATASETS` rather than trusted
        // outright: a bad value here (e.g. a hand-edited source.json, or a `--dataset` typo when
        // the sample was originally materialized) would otherwise silently create a fourth diffs/
        // folder that nothing else in this codebase knows to look in.
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

    // Collision check spans every dataset folder (`diffs_case_dir` searches `DIFF_DATASETS`) - the
    // flat-name lookup every other case name resolution in this file relies on breaks the moment
    // two different datasets can hold the same name.
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

    // Carries the sample's provenance/license attribution (written by `materialize_test_diffs`,
    // via `codediff::stats::license`) forward into `diffs/` - the before/after content is
    // someone else's code, not codediff's own, so that attribution needs to survive promotion,
    // not just live in `samples/` until this directory gets cleaned up once triage finishes (see
    // the `d285097` commit that deleted the small dataset's fully-triaged `samples/`). Only a
    // `Sample` origin has a README.md to copy; `GitCommitFile` promotions are sourced from this
    // repo's own commits, not a third-party one.
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

    // A sample's recorded comment (if any) rides along into the generated stub test - see
    // `action_save`'s doc comment for why this is fetched here rather than threaded further down.
    let comment = match &sample_source {
        Some(source) => sample_comment(source)?,
        None => None,
    };
    let save_msg = action_save(
        &mut app.mapping,
        &mut app.dirty,
        new_name,
        comment.as_deref(),
    )?;

    // ...and into the fixture's own `description.md`, which is where `diff_inventory` reads a note
    // from and where `e` on the promoted case writes one. The sample.csv cell is cleared below:
    // the note **moves** here rather than being copied, so a promoted fixture has exactly one
    // note and there is no second copy to drift. `no_promoted_row_carries_a_comment` pins that.
    let note_written = match &comment {
        Some(comment) => write_note(new_name, comment).is_ok(),
        None => false,
    };
    refresh_diff_unmarked(app, new_name);
    refresh_diff_text_painted(app, new_name);
    refresh_diff_disagreement(app, new_name);
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

/// Rejects the currently open sample instead of promoting it: records `reason` verbatim in its
/// sample.csv row (`comment`, with `status` set to `REJECTED`) and leaves everything else -- the
/// sample directory, `promoted_to` -- untouched. Only a sample has a sample.csv row to update; a
/// git-commit-sourced case (`CaseOrigin::GitCommitFile`) has nothing to reject.
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

/// Records or clears the currently open sample's sample.csv `comment` column, without touching
/// `status`/`promoted_to` -- unlike `R`'s reject flow (`action_reject`), this works regardless of
/// whether the sample is still unreviewed, already promoted, or already rejected, and an empty
/// `comment` is valid (clears any previously-recorded one, unlike a rejection reason, which can't
/// be empty). Only a sample has a sample.csv row to update; a git-commit-sourced case
/// (`CaseOrigin::GitCommitFile`) has nothing to comment on.
pub(crate) fn action_comment(app: &App, comment: &str) -> Result<String> {
    let comment = comment.trim();

    // An already-promoted sample edits its *fixture's* `description.md`, not its own sample.csv
    // row. That row is no longer what anything reads - `diff_inventory` prefers the file - so
    // writing to it would be an edit that appears to work and shows up nowhere. One note per
    // fixture, one writer.
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

    // A promoted or handmade fixture keeps its note in its own directory, as `description.md`,
    // because there is no sample.csv row to hold one - a handmade case was never sampled at all.
    // Written here and now rather than on the next `w`: `app.dirty` means "human_mapping.json has
    // unsaved edits", and letting one keystroke ride a flag that saves a different file is how a
    // comment gets lost to a quit-without-saving. The sample branch below has always written
    // immediately too, so this keeps `e` meaning one thing.
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
    /// Free-form note about this sample, verbatim from `Modal::PromptComment`'s input - independent
    /// of `status`: settable (and editable) whether the row is still SAMPLED, already PROMOTED, or
    /// REJECTED. `action_reject` also writes here (the rejection reason *is* the comment, not a
    /// separate column) - see that function and `Modal::PromptRejectReason`. Empty if never set.
    pub(crate) comment: String,
    /// The `stats::sampling::loc_bucket` this row was drawn for, or empty for a row that predates
    /// bucket tracking or was not sampled with `sample_test_diffs --stratified` - see
    /// `sample_test_diffs::Row::size_bucket`. Read and written purely so a round-trip through this
    /// tool preserves it: promoting or rejecting one sample rewrites the whole file, so a column
    /// this reader dropped would be erased for every other row at the same time.
    pub(crate) size_bucket: String,
}

/// Same backfill `sample_test_diffs::default_status` uses for a sample.csv row written before
/// `status` existed: duplicated rather than shared across the two binaries, same as the
/// `dataset` fallback ("small") a few lines below already is.
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
            // Same historical fallback as `legacy_dataset()`/`sample_test_diffs::LEGACY_DATASET`.
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

/// Finds the sample.csv row matching `source`'s identity (language/repository/commit/path) -
/// shared by every write path that needs to locate exactly one row to update
/// (`update_sample_csv_at`, `reject_sample_csv_at`, `set_sample_comment_at`), plus the read-only
/// `sample_comment_at`/the `e` keybinding's prefill lookup. `source` uniquely identifies a row by
/// construction (`sample_test_diffs` never writes two rows for the same commit+path), so "first
/// match" is never ambiguous in practice.
/// The fixture name this sample was promoted to, if it has been promoted at all.
///
/// The inverse of `promoted_sample_comment`'s join, and the check that tells `action_comment`
/// whether a sample still owns its own note or whether the promoted fixture's `description.md`
/// has taken over.
pub(crate) fn promoted_case_name(source: &SampleSource) -> Option<String> {
    let rows = read_sample_csv_rows(&sample_csv_path()).ok()?;
    find_sample_row(&rows, source)
        .map(|row| row.promoted_to.clone())
        .filter(|name| !name.trim().is_empty())
}

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

/// Mutable counterpart of `find_sample_row`, for the write paths.
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

/// Marks the sample.csv row matching `source` as promoted to `new_name`, preserving every other
/// row and column (including `comment`) untouched. Returns `Ok(false)` (not an error) if no row
/// matches -- e.g. the sample was placed under samples/ by hand rather than by
/// `sample_test_diffs` -- since that shouldn't undo a promotion that has already otherwise
/// succeeded.
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
    // The note **moves** to the fixture's own `description.md` (written by `action_promote` just
    // before this call) rather than being copied there. A promoted row keeping its comment is how
    // one fixture ended up with two notes that could drift - and one pair had, by the time the
    // duplication was found. `no_promoted_row_carries_a_comment` pins the invariant; a rejection
    // keeps its reason here, because a rejected sample has no directory to hold one.
    row.comment.clear();

    write_sample_csv_rows(path, &rows)?;
    Ok(true)
}

pub(crate) fn reject_sample(source: &SampleSource, reason: &str) -> Result<bool> {
    reject_sample_csv_at(&sample_csv_path(), source, reason)
}

/// Marks the sample.csv row matching `source` as rejected, recording `reason` in its `comment`
/// column -- the reject counterpart of `update_sample_csv_at`. `promoted_to` is deliberately left
/// as-is (empty, in practice: `action_reject` only ever runs against a case that's still
/// `CaseOrigin::Sample`, which a promotion would have already moved past) since a rejected sample
/// was never promoted. Returns `Ok(false)` (not an error) if no row matches, same reasoning as
/// `update_sample_csv_at`.
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

/// Records `comment` verbatim in the sample.csv row matching `source`'s `comment` column,
/// preserving every other column -- including `status`/`promoted_to` -- untouched. Unlike
/// `reject_sample_csv_at`, this never changes `status`, so it works the same whether the row is
/// still SAMPLED, already PROMOTED, or REJECTED; an empty `comment` is valid and clears any
/// previously-recorded one. Returns `Ok(false)` (not an error) if no row matches, same reasoning
/// as `update_sample_csv_at`.
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

/// The `comment` column value for `source`'s row in sample.csv, if the row exists and its comment
/// is non-empty (after trimming) - `None` either way otherwise. `action_promote`'s own way of
/// asking "should the generated stub test get a leading explanatory comment".
pub(crate) fn sample_comment_at(path: &Path, source: &SampleSource) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let rows = read_sample_csv_rows(path)?;
    Ok(find_sample_row(&rows, source)
        .map(|row| row.comment.trim().to_string())
        .filter(|c| !c.is_empty()))
}

/// `handle_modal_key`'s `Modal::SolutionPicker` arm. Split out at 105 lines: an arm that
/// long stops being readable as one case of a match, and taking only the 0 of
/// nine threaded values it actually uses makes its real dependencies visible.
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
) -> Option<OpenTarget> {
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
                action_save_solution_as(app, &typed, true);
            } else {
                action_load_solution(app, typed.trim());
            }
            app.modal = Some(Modal::TextView { state });
        }
        // Esc backs out of typing to the list rather than closing outright, so a mistyped
        // name costs one key, not the whole picker.
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
                    action_save_solution_as(app, &chosen, true);
                } else {
                    action_load_solution(app, &chosen);
                }
                app.modal = Some(Modal::TextView { state });
            }
        }
        // `e` is the one-key alternative to Enter: start the chosen name from nothing
        // instead of from a copy of what is currently painted.
        (None, KeyCode::Char('e')) if saving && selected < free_form_index => {
            let chosen = names[selected].clone();
            action_save_solution_as(app, &chosen, false);
            app.modal = Some(Modal::TextView { state });
        }
        // `D` twice deletes the highlighted painting. Capital, and twice, because there
        // is no undo: the first press names what is about to go in the picker's title, the
        // second acts on that name rather than on whatever row the cursor reached in
        // between. Any other key clears the pending confirmation.
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

/// `handle_modal_key`'s `Modal::TextView` arm. Split out at 270 lines: an arm that
/// long stops being readable as one case of a match, and taking only the 4 of
/// nine threaded values it actually uses makes its real dependencies visible.
fn handle_text_view(
    app: &mut App,
    code: KeyCode,
    mut state: TextPaintState,
    before_src: &[u8],
    after_src: &[u8],
    before: &Code,
    after: &Code,
) -> Option<OpenTarget> {
    // `before_src`/`after_src` are the bytes of a `String` (`Code::contents`), so these
    // conversions cannot fail; the fallback exists so a hypothetical non-UTF-8 source
    // degrades to an empty painting surface rather than panicking mid-session.
    let before_text = std::str::from_utf8(before_src).unwrap_or_default();
    let after_text = std::str::from_utf8(after_src).unwrap_or_default();
    // The viewport height the cursor has to stay inside. The real popup height isn't known
    // outside `render_text_view_modal`, and threading it back here would couple the key
    // handler to the layout for one number - a conservative constant keeps the cursor on
    // screen for any terminal at least this tall and merely over-scrolls on a shorter one.
    const VIEWPORT_ROWS: usize = 20;
    let focused_source = if state.side == 0 {
        before_text
    } else {
        after_text
    };
    let mut close = false;

    // While the `:` prompt is open it takes every keystroke, so a digit is a digit rather
    // than a movement command.
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
                // 1-based in, 0-based out: the gutter shows 1-based numbers, so that is
                // what a reader will type.
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
        // Swaps how a selection spanning several rows reads: vertical picks the same
        // columns down each row (a stack of squares), full-line sweeps every row end to
        // end - what a single contiguous multi-line block (e.g. a whole moved function)
        // still needs, since `m` requires every span on a side to read identical text.
        KeyCode::Char('V') => {
            state.vertical = !state.vertical;
            let mode = if state.vertical {
                "vertical"
            } else {
                "full-line"
            };
            app.status = Some(format!("Selections are now {mode}"));
        }
        // Same pair the tree panels use for their own multi-map selection: `x` banks what
        // is selected so another range can be selected on the same side, `c` clears both
        // sides' banks. This is what makes an N:M match reachable - one live selection can
        // only ever describe one range.
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
        // Shift-`p`, next to the `p` that shows codediff's rendering: `p` looks at it, `P` adopts
        // it as the draft to correct.
        KeyCode::Char('P') => action_paint_seed_from_codediff(app, before, after),
        KeyCode::Char('p') => {
            let next = app.text_overlay.next();
            // Computed on the first cycle away from `Human` and kept for the rest of the
            // case: running codediff is real work on a large fixture, and the default view
            // never needs it.
            if next != TextOverlay::Human && app.algo_text_spans.is_none() {
                app.algo_text_spans = Some(codediff_text_spans(before, after));
            }
            // Same lazy contract, for the tree-mapping side `TreeDisagreement` needs -
            // built independently of `algo_text_spans` since a case might be cycled
            // straight past `CodeDiff`/`Disagreements` without ever needing it.
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
        // `s` and `L` both raise the same picker; `saving` is the only difference, and it
        // decides only what Enter does with the chosen name.
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
            // Esc backs out one step at a time, so an accidental `v` - or a half-built
            // N:M group - doesn't cost the whole view. Only an Esc with nothing pending
            // closes.
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

/// `handle_modal_key`'s `Modal::OpenSamplePicker` arm, split out for the same reason as
/// `handle_open_diff_picker` just below: it works purely on `App` and its own payload, and is too
/// long to read as one arm of a match. The two are deliberately parallel - same keys, same
/// re-anchoring, same persistence back onto `App` - since they are the same table over two
/// different corpora (see `SampleColumn` on why the *types* are not shared).
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

    // The `Name` filter's prompt takes every keystroke while it is open, so a name containing
    // `j`, `s` or `f` types those characters instead of moving the selection and re-sorting the
    // table mid-word - same posture as `handle_open_diff_picker`.
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
        // `s` takes the sort over to the cursor column, or flips the direction if it already owns
        // it. The selection follows the row it was on rather than jumping to the top.
        KeyCode::Char('s') => {
            let current = visible.get(selected).map(|row| row.name.clone());
            view.sort = view.sort.toggled(view.column);
            app.sample_view = view.clone();
            let modal =
                open_sample_picker_modal(rows, current.as_deref().unwrap_or(&app.name), view);
            app.modal = Some(modal);
            return None;
        }
        // `f` cycles the cursor column's own filter: a value from the column for `Lang`/`Bucket`,
        // a triage state for `Status`, off/yes/no for `Size`, and a typed substring for `Name`
        // (which opens `name_input` above rather than taking effect immediately).
        KeyCode::Char('f') => {
            let current = visible.get(selected).map(|row| row.name.clone());
            match view.column {
                SampleColumn::Name => {
                    app.status = Some(
                        "Filter by name: type a substring, Enter to apply, Esc to cancel"
                            .to_string(),
                    );
                    // Pre-filled with the filter already in force, so `f` edits rather than
                    // blindly overwrites.
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
    // Covers `h`/`l`: the cursor column persists across closing and reopening the picker, the same
    // way the sort and filters do. The other keys reaching here leave `view` untouched.
    app.sample_view = view.clone();
    app.modal = Some(Modal::OpenSamplePicker {
        rows,
        selected,
        view,
        name_input: None,
    });

    None
}

/// `handle_modal_key`'s `Modal::OpenDiffPicker` arm. Split out at 182 lines: an arm that
/// long stops being readable as one case of a match. It needs none of the nine values
/// `handle_modal_key` threads through - it works purely on `App` and its own payload.
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

    // The `Name` filter's prompt takes every keystroke while it is open, so a name
    // containing `j`, `s` or `f` types those characters instead of moving the selection
    // and re-sorting the table mid-word. Same posture as the text view's `:` line prompt.
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
                // Which row the selection was on *before* the new filter narrows the list,
                // so it can follow that row rather than resetting to the top - same
                // re-anchoring every other filter/sort key here does.
                let current = visible_diff_options(&options, &view, DiffPickerData::from_app(app))
                    .get(selected)
                    .cloned();
                // Lowercased once here rather than per row per frame; blank clears the
                // filter rather than being stored as a needle that matches everything
                // while still reading as "filtered" in the header.
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
        // Column movement never triggers a scan: `s`/`f` are deliberate presses that can
        // afford to stall for several seconds the first time (see
        // `ensure_diff_column_data`), but walking the cursor across the header to reach
        // one of them must stay instant.
        KeyCode::Left | KeyCode::Char('h') => {
            view.column = view.column.left();
        }
        KeyCode::Right | KeyCode::Char('l') => {
            view.column = view.column.right();
        }
        // `s` takes the sort over to the cursor column, or flips the direction if it is
        // already the sorted one. The selection follows the row it was on rather than
        // jumping to the top, which is the whole point of re-sorting while looking at a
        // particular fixture.
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
        // `f` cycles the cursor column's own filter - a dataset for `Dataset`, a
        // three-state off/yes/no for each yes-no column, and a typed substring for `Name`
        // (which opens `name_input` above instead of taking effect immediately).
        KeyCode::Char('f') => {
            let current = visible.get(selected).cloned();
            if view.column == DiffColumn::Name {
                app.status = Some(
                    "Filter by name: type a substring, Enter to apply, Esc to cancel".to_string(),
                );
                // Pre-filled with the filter already in force, so `f` is an edit rather
                // than a blind overwrite - same idea as `PromptComment`.
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
    // Covers `h`/`l`: the cursor column persists across closing and reopening the picker
    // the same way the sort and filters `s`/`f` set do, so reopening lands back on the
    // column that was being worked with. The other keys reaching here leave `view`
    // untouched, so this is a no-op for them.
    app.diff_view = view.clone();
    app.modal = Some(Modal::OpenDiffPicker {
        options,
        selected,
        view,
        name_input: None,
    });

    None
}
