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
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */
use anyhow::{Context, Result};
use crossterm::event::KeyCode;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Clear, Paragraph},
};
use tokio::sync::mpsc;
use tracing::{debug, error};

use std::path::{Path, PathBuf};

use crate::code::{Code, Language};
use crate::diff::{
    Diff, DiffMode, NodeCache,
    text::{
        ChangeCounts, DiffSummary, RenderOptions, TextDiff, change_counts, is_comment_only_diff,
        plain_text_line_diff, summarize_diff_with_comment_check,
    },
};
use crate::tui::actions::{Action, DiffOutcome, DiffSessionData};
use crate::tui::components::{
    Component,
    diff_viewer::{DiffViewer, Panel},
    file_dialog::FileDialog,
    help_modal::HelpModal,
    line_prompt::LinePrompt,
    render_options_dialog::RenderOptionsDialog,
    search_modal::SearchModal,
    theme_dialog::ThemeDialog,
};
use crate::tui::events::Event;
use crate::tui::theme::{self, OverlayTheme};
use crate::tui::ui::UI;

/// Which top-level screen is currently shown.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum AppScreen {
    /// The Before/After panels, populated or not, with the normal diff-viewer key bindings live.
    #[default]
    Viewer,
    /// A file dialog is open, picking a file for `dialog_target`.
    SelectFile,
    /// The theme picker popup is open, drawn over the (still-visible) viewer.
    SelectTheme,
    /// The render-options panel (the `M` key) is open, drawn over the (still-visible) viewer.
    RenderOptions,
    /// The background diff computation is running.
    Diffing,
    /// The `?` keybinding reference is open, drawn over the (still-visible) viewer.
    Help,
    /// The `/` search modal is open, drawn over the (still-visible) viewer.
    Search,
    /// The `g` jump-to-line prompt is open, drawn over the (still-visible) viewer.
    JumpToLine,
}

/// Whether pressing Esc on `screen` should quit the app, rather than being handled by that
/// screen's own dialog (via `Action::DialogCancelled`).
///
/// `Diffing` is deliberately *not* a quit screen: Esc there cancels the in-flight computation
/// and returns to the viewer (see the Esc arm in `handle_events`) - previously it quit the whole
/// app, making a slow diff the one situation where a reflexive Esc lost the entire session.
///
/// Deliberately an exhaustive match, not a list of `!=` exclusions: that list was extended twice
/// before and `SelectTheme` was missed *both* times, letting Esc silently quit the whole app
/// instead of closing the theme picker - the only dialog that happened to (found in a 2026-07
/// code-health pass). An exhaustive match means the compiler itself forces every future
/// `AppScreen` variant to be considered here, instead of relying on someone remembering to
/// extend a growing exclusion list.
fn esc_should_quit(screen: AppScreen) -> bool {
    match screen {
        AppScreen::Viewer => true,
        AppScreen::Diffing
        | AppScreen::SelectFile
        | AppScreen::SelectTheme
        | AppScreen::RenderOptions
        | AppScreen::Help
        | AppScreen::Search
        | AppScreen::JumpToLine => false,
    }
}

/// The codediff application. The state, but not the state machine or the UI, of the TUI.
pub struct App {
    tick_rate: f64,
    frame_rate: f64,

    diff_viewer: DiffViewer,
    file_dialog: Option<FileDialog>,
    theme_dialog: Option<ThemeDialog>,
    render_options_dialog: Option<RenderOptionsDialog>,
    help_modal: Option<HelpModal>,
    search_modal: Option<SearchModal>,
    line_prompt: Option<LinePrompt>,

    action_tx: mpsc::UnboundedSender<Action>,
    action_rx: mpsc::UnboundedReceiver<Action>,
    /// Incremented for every launched background diff; `start_diff` captures the current value
    /// and tags its `Action::DiffComputed` with it. A result whose generation no longer matches
    /// is silently dropped - which is all "cancelling" a `spawn_blocking` computation can mean
    /// (the thread itself runs to completion; its answer just doesn't land). Bumped by Esc during
    /// `Diffing` and by every new `StartDiff`, so a slow older diff can never overwrite a newer
    /// one's result either.
    diff_generation: u64,
    /// Where to put the cursor back after the next `DiffReady` - set by the `r` reload and the
    /// `e` editor round-trip so recomputing the same pair doesn't dump the cursor back at the
    /// first change. `(panel, row, col)`, applied clamped (the file may have changed under a
    /// reload).
    restore_after_reload: Option<(Panel, usize, usize)>,
    /// The last submitted search query - `/` then a bare Enter repeats it (see `SearchModal`).
    last_search_query: Option<String>,
    /// Set by the `e` key: leave the TUI, run `$VISUAL`/`$EDITOR` on this `(path, 1-indexed
    /// line)`, then re-enter and re-diff - serviced by the `run` loop between event batches,
    /// since only it holds the `UI` needed to release and re-acquire the terminal.
    pending_editor: Option<(PathBuf, usize)>,
    /// Recently diffed pairs (most recent first), loaded from the config in `run` and offered on
    /// the empty-start screen as digit shortcuts - see `draw_recent_pairs`/the digit-key arm.
    recent_pairs: Vec<(PathBuf, PathBuf)>,

    screen: AppScreen,
    /// Which panel the open file dialog is selecting a file for.
    dialog_target: Option<Panel>,
    /// The currently active overlay color theme, persisted across runs (see `tui::theme`).
    current_theme: OverlayTheme,
    /// The chosen syntax-highlighting theme name, persisted alongside the overlay theme. `None`
    /// until the user picks one in the theme dialog, in which case the code viewer keeps its own
    /// built-in default.
    syntax_theme: Option<String>,
    /// The "Before" file path, once a file has been picked for that panel.
    before_path: Option<PathBuf>,
    /// The "After" file path, once a file has been picked for that panel.
    after_path: Option<PathBuf>,
    /// The most recent diff failure, shown as a one-line banner until the next file pick.
    last_error: Option<String>,
    /// A quick, common-case classification of the currently loaded diff (see
    /// `diff::text::DiffSummary`), shown as a one-line status bar above the panels when it's
    /// `Some`. Computed once, when `Action::DiffReady` arrives (`summarize_diff`), not on every
    /// frame - the panels themselves don't change without a fresh diff, so neither does this.
    /// Cleared on `StartDiff`/`DiffFailed` so a stale summary from the *previous* diff never shows
    /// while a new one is loading or after it fails.
    diff_summary: Option<DiffSummary>,
    /// Line-level +/-/~ counts for the currently loaded diff (see `diff::text::change_counts`),
    /// shown in the footer. Same lifecycle as `diff_summary` - computed once on `Action::DiffReady`,
    /// cleared on `StartDiff`/`DiffFailed`.
    change_counts: Option<ChangeCounts>,
    /// Whether the currently loaded diff came from `diff::text::plain_text_line_diff` (see
    /// `DiffSessionData::plain_text_fallback`) rather than the AST pipeline - shown in the footer
    /// as `[plain text]`, since no AST algorithm ran at all. Same lifecycle as
    /// `diff_summary`/`change_counts`.
    plain_text_fallback: bool,

    should_exit: bool,
    should_suspend: bool,
}

impl App {
    /// Construct the App.
    pub fn new(tick_rate: f64, frame_rate: f64) -> Result<Self> {
        let (action_tx, action_rx) = mpsc::unbounded_channel();

        Ok(Self {
            tick_rate,
            frame_rate,
            diff_viewer: DiffViewer::new(),
            file_dialog: None,
            theme_dialog: None,
            render_options_dialog: None,
            help_modal: None,
            search_modal: None,
            line_prompt: None,
            action_tx,
            action_rx,
            diff_generation: 0,
            restore_after_reload: None,
            last_search_query: None,
            pending_editor: None,
            recent_pairs: Vec::new(),
            screen: AppScreen::default(),
            syntax_theme: None,
            dialog_target: None,
            current_theme: OverlayTheme::default(),
            before_path: None,
            after_path: None,
            last_error: None,
            diff_summary: None,
            change_counts: None,
            plain_text_fallback: false,
            should_exit: false,
            should_suspend: false,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        // Loaded here rather than in `new` so constructing an `App` (e.g. in tests) never
        // touches disk; only actually running the TUI reads (and may create) the config file.
        self.current_theme = theme::load_overlay_theme();
        self.diff_viewer.set_overlay_theme(self.current_theme);
        self.diff_viewer
            .set_layout_override(theme::load_panel_layout());
        self.diff_viewer
            .set_node_highlight(theme::load_node_highlight());
        self.diff_viewer
            .set_render_options(theme::load_render_options());
        // The custom palette has to be installed before anything renders: `OverlayTheme::Custom`
        // resolves through the process-global one, so a user whose saved theme is Custom would
        // otherwise see Dracula's defaults for the first frame.
        theme::set_custom_palette(theme::load_custom_palette());
        self.syntax_theme = theme::load_syntax_theme();
        if let Some(name) = self.syntax_theme.clone() {
            self.diff_viewer.set_syntax_theme(name);
        }
        self.recent_pairs = theme::load_recent_pairs();

        let mut ui = UI::new()?
            .tick_rate(self.tick_rate)
            .frame_rate(self.frame_rate);
        // Mouse capture on by default: the wheel scrolls the panel under the pointer and a
        // click focuses/places the cursor (`DiffViewer::handle_mouse_event`). Terminal-native
        // text selection remains available via the terminal's own modifier (usually Shift-drag).
        ui.mouse = true;
        ui.enter()?;

        self.diff_viewer
            .register_action_handler(self.action_tx.clone())?;
        self.diff_viewer.init(ui.size()?)?;

        let action_tx = self.action_tx.clone();
        loop {
            self.handle_events(&mut ui).await?;
            self.handle_actions(&mut ui)?;
            if let Some((path, line)) = self.pending_editor.take() {
                self.run_editor(&mut ui, &path, line)?;
            }
            if self.should_suspend {
                ui.suspend()?;
                action_tx.send(Action::Resume)?;
                action_tx.send(Action::ClearScreen)?;
                ui.enter()?;
            } else if self.should_exit {
                break;
            }
        }
        ui.exit()?;
        Ok(())
    }

    async fn handle_events(&mut self, ui: &mut UI) -> Result<()> {
        let Some(event) = ui.next_event().await else {
            return Ok(());
        };
        let action_tx = self.action_tx.clone();

        // Set by the o/c/? arms below when they open a new screen - see the comment at this
        // function's `dispatch_event_to_active_screen` call site for why that matters.
        let mut globally_handled = false;

        // Global key handling that doesn't depend on which component is focused.
        if let Event::Key(key) = &event {
            match key.code {
                KeyCode::Char('q') => {
                    action_tx.send(Action::Quit)?;
                }
                KeyCode::Esc if esc_should_quit(self.screen) => {
                    action_tx.send(Action::Quit)?;
                }
                // Esc during an in-flight diff cancels it instead of quitting: bump the
                // generation so the eventual `DiffComputed` is recognized as stale and dropped,
                // and return to the viewer (still showing whatever diff was loaded before).
                KeyCode::Esc if self.screen == AppScreen::Diffing => {
                    self.diff_generation += 1;
                    self.restore_after_reload = None;
                    self.screen = AppScreen::Viewer;
                    action_tx.send(Action::Render)?;
                    globally_handled = true;
                }
                // Re-diff the current pair from disk - the edit-in-another-terminal loop. Keeps
                // the cursor where it is (clamped) instead of resetting to the first change.
                KeyCode::Char('r') if self.screen == AppScreen::Viewer => {
                    if let (Some(before), Some(after)) =
                        (self.before_path.clone(), self.after_path.clone())
                    {
                        self.remember_cursor_for_restore();
                        action_tx.send(Action::StartDiff(before, after, DiffMode::Fast))?;
                    }
                    globally_handled = true;
                }
                KeyCode::Char('g') if self.screen == AppScreen::Viewer => {
                    self.line_prompt = Some(LinePrompt::new());
                    self.screen = AppScreen::JumpToLine;
                    action_tx.send(Action::Render)?;
                    globally_handled = true;
                }
                // On the empty-start screen, the digit keys reopen a recent pair (see
                // `draw_recent_pairs`). Only with no files loaded: once a diff is up, digits
                // stay free for future use.
                KeyCode::Char(c @ '1'..='9')
                    if self.screen == AppScreen::Viewer
                        && self.before_path.is_none()
                        && self.after_path.is_none() =>
                {
                    let index = (c as usize) - ('1' as usize);
                    if let Some((before, after)) = self.recent_pairs.get(index).cloned() {
                        self.open_files(before, after)?;
                        action_tx.send(Action::Render)?;
                    }
                    globally_handled = true;
                }
                // Open the focused panel's file in $VISUAL/$EDITOR at the cursor line - serviced
                // by the run loop (`run_editor`), which is the only place the terminal can be
                // released and re-acquired.
                KeyCode::Char('e') if self.screen == AppScreen::Viewer => {
                    let path = match self.diff_viewer.active_panel() {
                        Panel::Before => self.before_path.clone(),
                        Panel::After => self.after_path.clone(),
                    };
                    if let (Some(path), Some((row, _col))) =
                        (path, self.diff_viewer.focused_cursor_position())
                    {
                        self.pending_editor = Some((path, row + 1));
                    }
                    globally_handled = true;
                }
                KeyCode::Char('o') if self.screen == AppScreen::Viewer => {
                    let panel = self.diff_viewer.active_panel();
                    self.dialog_target = Some(panel);
                    self.last_error = None;
                    self.screen = AppScreen::SelectFile;
                    let title = match panel {
                        Panel::Before => "Select the BEFORE file",
                        Panel::After => "Select the AFTER file",
                    };
                    self.open_file_dialog(title, ui)?;
                    action_tx.send(Action::Render)?;
                    globally_handled = true;
                }
                KeyCode::Char('c') if self.screen == AppScreen::Viewer => {
                    self.theme_dialog = Some(ThemeDialog::with_syntax_theme(
                        self.current_theme,
                        self.syntax_theme.as_deref(),
                    ));
                    self.screen = AppScreen::SelectTheme;
                    action_tx.send(Action::Render)?;
                    globally_handled = true;
                }
                // Open the render-options panel - which parts of the diff get painted, not a
                // blind Full/Minimal flip any more (see `RenderOptionsDialog`).
                KeyCode::Char('M') if self.screen == AppScreen::Viewer => {
                    self.render_options_dialog =
                        Some(RenderOptionsDialog::new(self.diff_viewer.render_options()));
                    self.screen = AppScreen::RenderOptions;
                    action_tx.send(Action::Render)?;
                    globally_handled = true;
                }
                KeyCode::Char('?') if self.screen == AppScreen::Viewer => {
                    self.help_modal = Some(HelpModal::new(self.current_theme));
                    self.screen = AppScreen::Help;
                    action_tx.send(Action::Render)?;
                    globally_handled = true;
                }
                KeyCode::Char('/') if self.screen == AppScreen::Viewer => {
                    self.search_modal = Some(SearchModal::new(self.last_search_query.clone()));
                    self.screen = AppScreen::Search;
                    action_tx.send(Action::Render)?;
                    globally_handled = true;
                }
                _ => {}
            }
        }

        match event {
            Event::Tick => action_tx.send(Action::Tick)?,
            Event::Render => action_tx.send(Action::Render)?,
            Event::Resize(x, y) => action_tx.send(Action::Resize(x, y))?,
            Event::Key(_) | Event::Mouse(_) => {}
        }

        // Skip forwarding this same keystroke to whatever screen it just opened above - `o`/`c`
        // happened to get away with the double-dispatch (neither FileDialog nor ThemeDialog treats
        // `o`/`c` as one of its own keys, so the stray extra delivery was a harmless no-op), but
        // `?` doesn't: HelpModal treats `?` as its own close key too, so without this guard the
        // keystroke that opens it would immediately reach the just-created modal and close it
        // again in the same event cycle - it would open and close within a single frame, i.e.
        // never visibly show up at all.
        if !globally_handled {
            if let Some(action) = self.dispatch_event_to_active_screen(event)? {
                action_tx.send(action)?;
            }
        }
        Ok(())
    }

    /// Forward an event only to the component backing the currently active screen, so e.g. a
    /// file dialog being open doesn't also feed arrow keys into the (hidden) diff viewer.
    fn dispatch_event_to_active_screen(&mut self, event: Event) -> Result<Option<Action>> {
        match self.screen {
            AppScreen::Viewer => self.diff_viewer.handle_events(Some(event)),
            AppScreen::SelectFile => match self.file_dialog.as_mut() {
                Some(dialog) => dialog.handle_events(Some(event)),
                None => Ok(None),
            },
            AppScreen::SelectTheme => match self.theme_dialog.as_mut() {
                Some(dialog) => dialog.handle_events(Some(event)),
                None => Ok(None),
            },
            AppScreen::RenderOptions => match self.render_options_dialog.as_mut() {
                Some(dialog) => dialog.handle_events(Some(event)),
                None => Ok(None),
            },
            AppScreen::Diffing => Ok(None),
            AppScreen::JumpToLine => match self.line_prompt.as_mut() {
                Some(prompt) => prompt.handle_events(Some(event)),
                None => Ok(None),
            },
            AppScreen::Help => match self.help_modal.as_mut() {
                Some(modal) => modal.handle_events(Some(event)),
                None => Ok(None),
            },
            AppScreen::Search => match self.search_modal.as_mut() {
                Some(modal) => modal.handle_events(Some(event)),
                None => Ok(None),
            },
        }
    }

    /// Create, register and initialize a fresh file dialog, replacing any previous one.
    fn open_file_dialog(&mut self, title: &str, ui: &mut UI) -> Result<()> {
        let mut dialog = FileDialog::new(title);
        dialog.register_action_handler(self.action_tx.clone())?;
        dialog.init(ui.size()?)?;
        self.file_dialog = Some(dialog);
        Ok(())
    }

    fn handle_actions(&mut self, ui: &mut UI) -> Result<()> {
        while let Ok(action) = self.action_rx.try_recv() {
            if action != Action::Tick && action != Action::Render {
                debug!("{action:?}");
            }
            match &action {
                Action::Tick => {}
                Action::Quit => self.should_exit = true,
                Action::Suspend => self.should_suspend = true,
                Action::Resume => self.should_suspend = false,
                Action::ClearScreen => ui.terminal.clear()?,
                Action::Resize(w, h) => self.handle_resize(ui, *w, *h)?,
                Action::Render => self.render(ui)?,
                Action::FileSelected(path) => self.handle_file_selected(path.clone())?,
                Action::DialogCancelled => self.handle_dialog_cancelled(),
                Action::StartDiff(before, after, mode) => {
                    self.screen = AppScreen::Diffing;
                    self.diff_summary = None;
                    self.change_counts = None;
                    self.plain_text_fallback = false;
                    self.start_diff(before.clone(), after.clone(), *mode);
                }
                // Every background diff reports back through here; only the result matching the
                // current generation is re-dispatched as `DiffReady`/`DiffFailed` (which is what
                // the components consume) - a stale one (cancelled, or superseded by a newer
                // `StartDiff`) is dropped without a trace.
                Action::DiffComputed {
                    generation,
                    outcome,
                } => {
                    if *generation == self.diff_generation {
                        match outcome {
                            DiffOutcome::Ready { data, .. } => {
                                self.action_tx.send(Action::DiffReady(data.clone()))?;
                            }
                            DiffOutcome::Failed(message) => {
                                self.action_tx.send(Action::DiffFailed(message.clone()))?;
                            }
                        }
                    }
                }
                Action::DiffReady(data) => {
                    self.screen = AppScreen::Viewer;
                    self.last_error = None;
                    self.file_dialog = None;
                    // A pair that diffed successfully is worth remembering for the empty-start
                    // screen's digit shortcuts (deduplicated and capped in `record_recent_pair`).
                    theme::record_recent_pair(&data.before_path, &data.after_path);
                    let pair = (data.before_path.clone(), data.after_path.clone());
                    self.recent_pairs.retain(|existing| existing != &pair);
                    self.recent_pairs.insert(0, pair);
                    self.diff_summary = summarize_diff_with_comment_check(
                        &data.before_contents,
                        &data.after_contents,
                        &data.before_ranges,
                        &data.after_ranges,
                        data.comment_only,
                    );
                    self.change_counts = Some(change_counts(
                        &data.before_contents,
                        &data.after_contents,
                        &data.before_ranges,
                        &data.after_ranges,
                    ));
                    self.plain_text_fallback = data.plain_text_fallback;
                }
                Action::DiffFailed(message) => {
                    error!("diff failed: {message}");
                    self.last_error = Some(message.clone());
                    self.screen = AppScreen::Viewer;
                    self.file_dialog = None;
                    self.diff_summary = None;
                    self.change_counts = None;
                    self.plain_text_fallback = false;
                }
                Action::Error(message) => {
                    error!("{message}");
                    self.last_error = Some(message.clone());
                }
                Action::ThemeSelected(selected_theme) => {
                    self.apply_theme_selection(*selected_theme)
                }
                // A live preview while the theme dialog's selection moves - applied to the
                // viewer behind the dialog, but not persisted and not recorded as
                // `current_theme`; Esc (`DialogCancelled`) reverts to `current_theme`.
                Action::ThemePreviewed(previewed) => {
                    self.diff_viewer.set_overlay_theme(*previewed);
                }
                Action::SyntaxThemePreviewed(name) => {
                    self.diff_viewer.set_syntax_theme(name.clone());
                }
                Action::SearchSubmitted(query) => self.handle_search_submitted(query.clone()),
                Action::SearchQueryChanged(query) => self.handle_search_query_changed(query),
                Action::JumpToLineSubmitted(line) => {
                    self.diff_viewer.jump_to_line(*line);
                    self.line_prompt = None;
                    self.screen = AppScreen::Viewer;
                }
                // Every toggle in the render-options panel is already final (see the action's own
                // doc comment) - apply it to the live viewer and persist it in the same step,
                // rather than waiting for the dialog to close.
                Action::RenderOptionsChanged(options) => self.apply_render_options(*options)?,
                _ => {}
            }

            self.diff_viewer.update(action.clone())?;
            if let Some(dialog) = self.file_dialog.as_mut() {
                dialog.update(action.clone())?;
            }
            // `DiffViewer::load_diff` (triggered by the update call above) resets the cursor to
            // the first change; a reload/exact re-run of the same pair should instead put it back
            // where the user had it (clamped - the file may have changed under a reload).
            if matches!(action, Action::DiffReady(_))
                && let Some((panel, row, col)) = self.restore_after_reload.take()
            {
                self.diff_viewer.restore_cursor(panel, row, col);
            }
        }
        Ok(())
    }

    /// Record the focused panel's cursor so the next `DiffReady` puts it back - see
    /// `restore_after_reload`.
    fn remember_cursor_for_restore(&mut self) {
        if let Some((row, col)) = self.diff_viewer.focused_cursor_position() {
            self.restore_after_reload = Some((self.diff_viewer.active_panel(), row, col));
        }
    }

    /// `Action::RenderOptionsChanged`'s handler: apply and persist `options` immediately (every
    /// toggle in the render-options panel is already final, see the action's own doc comment),
    /// and - the one field that needs more than a re-filter -  reload the diff if
    /// `whole_pair_updates` changed.
    ///
    /// `whole_pair_updates` changes which ranges the diff itself has, not just how much of an
    /// already-built range list gets painted - `DiffViewer::set_render_options` alone re-filters
    /// the cached list and would leave this one option looking like it did nothing until the next
    /// unrelated reload. The other two fields are real post-filters and stay instant, so the
    /// reload is gated on this one field specifically rather than firing on every call.
    ///
    /// A method of its own, not inlined into `handle_actions`, so it can be unit tested without a
    /// real `UI` - nothing here touches one.
    fn apply_render_options(&mut self, options: RenderOptions) -> Result<()> {
        let previous = self.diff_viewer.render_options();
        let needs_reload = previous.whole_pair_updates != options.whole_pair_updates
            || previous.paint_reindent_only_moves != options.paint_reindent_only_moves;
        self.diff_viewer.set_render_options(options);
        theme::save_render_options(options);
        if needs_reload
            && let (Some(before), Some(after)) = (self.before_path.clone(), self.after_path.clone())
        {
            self.remember_cursor_for_restore();
            self.action_tx
                .send(Action::StartDiff(before, after, DiffMode::Fast))?;
        }
        Ok(())
    }

    /// The `e` key's other half: release the terminal, run the user's editor on `path` at
    /// `line` (the `+N` line convention vi/vim/nano/emacs/micro all accept), re-acquire the
    /// terminal, and re-diff so the edit shows up immediately - closing the read-diff/fix-code
    /// loop without ever leaving the session. `$VISUAL` beats `$EDITOR`, the POSIX convention;
    /// `vi` is the last resort. Blocking the async loop here is the *point*: the TUI has no
    /// terminal to draw on until the editor exits.
    fn run_editor(&mut self, ui: &mut UI, path: &Path, line: usize) -> Result<()> {
        let editor = std::env::var("VISUAL")
            .or_else(|_| std::env::var("EDITOR"))
            .unwrap_or_else(|_| "vi".to_string());
        ui.exit()?;
        let status = std::process::Command::new(&editor)
            .arg(format!("+{line}"))
            .arg(path)
            .status();
        ui.enter()?;
        ui.terminal.clear()?;
        if let Err(err) = status {
            self.last_error = Some(format!("failed to launch editor '{editor}': {err}"));
        }
        if let (Some(before), Some(after)) = (self.before_path.clone(), self.after_path.clone()) {
            self.remember_cursor_for_restore();
            self.action_tx
                .send(Action::StartDiff(before, after, DiffMode::Fast))?;
        }
        self.action_tx.send(Action::Render)?;
        Ok(())
    }

    /// A file was confirmed in the dialog: load it into the panel that was active when `o` was
    /// pressed, then kick off the diff once both panels have a file.
    fn handle_file_selected(&mut self, path: PathBuf) -> Result<()> {
        if let Some(panel) = self.dialog_target.take() {
            self.select_file_for_panel(panel, path)?;
        }
        self.file_dialog = None;
        self.screen = AppScreen::Viewer;
        Ok(())
    }

    /// Load `before` and `after` straight into their panels, bypassing the file dialog. Used to
    /// support starting the TUI with both file paths already given on the command line.
    pub fn open_files(&mut self, before: PathBuf, after: PathBuf) -> Result<()> {
        self.select_file_for_panel(Panel::Before, before)?;
        self.select_file_for_panel(Panel::After, after)
    }

    /// Load `path` into `panel`, remember it, and kick off the diff once both panels have a file.
    fn select_file_for_panel(&mut self, panel: Panel, path: PathBuf) -> Result<()> {
        match panel {
            Panel::Before => {
                self.before_path = Some(path.clone());
                self.diff_viewer.set_before_file(path)?;
            }
            Panel::After => {
                self.after_path = Some(path.clone());
                self.diff_viewer.set_after_file(path)?;
            }
        }

        if let (Some(before), Some(after)) = (self.before_path.clone(), self.after_path.clone()) {
            self.action_tx
                .send(Action::StartDiff(before, after, DiffMode::Fast))?;
        }
        Ok(())
    }

    fn handle_dialog_cancelled(&mut self) {
        // Cancelling the theme dialog must undo any live preview it applied (see
        // `Action::ThemePreviewed`); a no-op when no preview happened.
        if self.theme_dialog.is_some() {
            self.diff_viewer.set_overlay_theme(self.current_theme);
        }
        // Cancelling the search modal reverts its live highlight preview to the last actually
        // submitted query's matches (or none).
        if self.search_modal.is_some() {
            let last = self.last_search_query.clone().unwrap_or_default();
            self.diff_viewer.preview_search(&last);
        }
        self.file_dialog = None;
        self.theme_dialog = None;
        // Nothing to revert here, unlike the theme dialog above: every toggle in the
        // render-options panel already applied and persisted itself (see
        // `Action::RenderOptionsChanged`), so closing it is just discarding the dialog's own
        // cursor state.
        self.render_options_dialog = None;
        self.help_modal = None;
        self.search_modal = None;
        self.line_prompt = None;
        self.dialog_target = None;
        self.screen = AppScreen::Viewer;
    }

    /// Apply a search query from the search modal: jump the focused panel's cursor to the nearest
    /// match and highlight every match, then return to the normal viewer screen. A bare Enter
    /// (empty query) repeats the last submitted search, if any - the modal advertises this in its
    /// hint line.
    fn handle_search_submitted(&mut self, query: String) {
        let query = if query.is_empty() {
            self.last_search_query.clone().unwrap_or_default()
        } else {
            query
        };
        if !query.is_empty() {
            self.last_search_query = Some(query.clone());
        }
        self.diff_viewer.search(&query);
        self.search_modal = None;
        self.screen = AppScreen::Viewer;
    }

    /// Live feedback while typing in the search modal: preview the highlights (no cursor
    /// movement) and feed the match count back into the modal's `N matches` readout.
    fn handle_search_query_changed(&mut self, query: &str) {
        let count = self.diff_viewer.preview_search(query);
        if let Some(modal) = self.search_modal.as_mut() {
            modal.set_live_match_count(count);
        }
    }

    /// Apply a theme choice from the theme dialog: update the live viewer, persist it for future
    /// runs, and return to the normal viewer screen.
    fn apply_theme_selection(&mut self, selected_theme: OverlayTheme) {
        self.current_theme = selected_theme;
        self.diff_viewer.set_overlay_theme(selected_theme);
        theme::save_overlay_theme(selected_theme);
        self.theme_dialog = None;
        self.screen = AppScreen::Viewer;
    }

    /// Run the (CPU-bound) parse+diff pipeline on a blocking thread so it never stalls the
    /// render loop, then report the result back as `Action::DiffComputed`, tagged with a fresh
    /// generation - see `diff_generation` for how that makes cancellation/supersession work.
    ///
    /// The diff pipeline isn't guaranteed panic-free for arbitrary/unsupported input (e.g. it
    /// assumes a parsed AST further down the call chain), and a panic on a `spawn_blocking`
    /// thread would otherwise just vanish, leaving the UI stuck on "Diffing…" forever. Catching
    /// it here turns that into a reported failure instead.
    fn start_diff(&mut self, before: PathBuf, after: PathBuf, mode: DiffMode) {
        self.diff_generation += 1;
        let generation = self.diff_generation;
        let tx = self.action_tx.clone();
        // Read off the live viewer rather than threaded through `Action::StartDiff`: every caller
        // of `start_diff` (file selection, `x`'s exact-mode re-run, the external-editor reload,
        // and `Action::RenderOptionsChanged` below) should compute with whatever
        // `whole_pair_updates`/`paint_reindent_only_moves` currently are, not have to know to pass
        // them along individually.
        let render_options = self.diff_viewer.render_options();
        let whole_pair_updates = render_options.whole_pair_updates;
        let paint_reindent_only_moves = render_options.paint_reindent_only_moves;
        tokio::task::spawn_blocking(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                compute_diff_with_update_style(
                    &before,
                    &after,
                    mode,
                    whole_pair_updates,
                    paint_reindent_only_moves,
                )
            }));
            let outcome = match result {
                Ok(Ok((data, fallback_used))) => DiffOutcome::Ready {
                    data,
                    fallback_used,
                },
                Ok(Err(err)) => DiffOutcome::Failed(err.to_string()),
                Err(panic) => DiffOutcome::Failed(format!(
                    "internal error while diffing: {}",
                    panic_message(&panic)
                )),
            };
            // The receiver only goes away when the app is shutting down.
            let _ = tx.send(Action::DiffComputed {
                generation,
                outcome,
            });
        });
    }

    fn handle_resize(&mut self, ui: &mut UI, w: u16, h: u16) -> Result<()> {
        ui.resize(Rect::new(0, 0, w, h))?;
        self.render(ui)?;
        Ok(())
    }

    fn render(&mut self, ui: &mut UI) -> Result<()> {
        ui.draw(|frame| {
            let area = frame.size();
            let result = match self.screen {
                AppScreen::Viewer => self.draw_viewer(frame, area),
                AppScreen::SelectFile => match self.file_dialog.as_mut() {
                    Some(dialog) => dialog.draw(frame, area),
                    None => Ok(()),
                },
                AppScreen::SelectTheme => self.draw_theme_dialog(frame, area),
                AppScreen::RenderOptions => self.draw_render_options_dialog(frame, area),
                AppScreen::Diffing => {
                    frame.render_widget(diffing_status_paragraph(), area);
                    Ok(())
                }
                AppScreen::Help => self.draw_help_modal(frame, area),
                AppScreen::Search => self.draw_search_modal(frame, area),
                AppScreen::JumpToLine => self.draw_line_prompt(frame, area),
            };
            if let Err(err) = result {
                let _ = self
                    .action_tx
                    .send(Action::Error(format!("Failed to draw: {:?}", err)));
            }
        })?;
        Ok(())
    }

    /// Draw the Before/After panels, with an optional one-line diff-summary status bar above them
    /// (`self.diff_summary` - see `Action::DiffReady`'s handler), an optional one-line error
    /// banner below them (`self.last_error`, e.g. an unsupported file type on the most recent file
    /// pick), and an always-visible footer line below everything (cursor position plus a compact
    /// key-hint reference - see `draw_footer`). The status bar and error banner are each present
    /// or absent independently; the layout only reserves space for whichever actually has
    /// something to show. The footer's row is always reserved, unlike those two: it's the primary
    /// discoverability aid for a user who hasn't yet thought to press `?`, so unlike the status
    /// bar/error banner it can't be conditionally absent without defeating its own purpose.
    fn draw_viewer(&mut self, frame: &mut ratatui::Frame, area: Rect) -> Result<()> {
        let mut constraints = Vec::with_capacity(4);
        if self.diff_summary.is_some() {
            constraints.push(Constraint::Length(1));
        }
        constraints.push(Constraint::Min(1));
        if self.last_error.is_some() {
            constraints.push(Constraint::Length(1));
        }
        constraints.push(Constraint::Length(1));
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(area);

        let mut next = 0;
        if let Some(summary) = self.diff_summary {
            frame.render_widget(status_bar_paragraph(summary), layout[next]);
            next += 1;
        }
        self.diff_viewer.draw(frame, layout[next])?;
        self.draw_recent_pairs(frame, layout[next]);
        next += 1;
        if let Some(message) = &self.last_error {
            frame.render_widget(
                Paragraph::new(message.as_str()).style(Style::new().fg(Color::Red)),
                layout[next],
            );
            next += 1;
        }
        self.draw_footer(frame, layout[next]);
        Ok(())
    }

    /// The always-visible footer: the focused panel's cursor position, the diff's +/-/~ line
    /// counts, and its progress through `n`/`p` navigation on the left; a compact key-hint
    /// reference on the right. Pressing `?` still shows the full keybinding/color reference
    /// (`help_modal.rs`) - this is deliberately just the handful of most-used keys, so a
    /// first-time user has *some* signal that keybindings exist at all without having to already
    /// know to press `?` first.
    fn draw_footer(&self, frame: &mut ratatui::Frame, area: Rect) {
        let mut left_parts = Vec::with_capacity(3);
        if let Some((row, col)) = self.diff_viewer.focused_cursor_position() {
            left_parts.push(format!("Ln {}, Col {}", row + 1, col + 1));
        }
        if let Some(counts) = self.change_counts {
            let counts_text = format_change_counts(counts);
            if !counts_text.is_empty() {
                left_parts.push(counts_text);
            }
        }
        // While a search is active, its progress replaces "change N/M" rather than sitting
        // alongside it - both convey "progress through a list of positions," and showing both at
        // once would risk overflowing the footer's fixed-width left column on a narrow terminal.
        if let Some((index, total)) = self.diff_viewer.focused_search_match_count_and_index() {
            left_parts.push(format!("match {index}/{total}"));
        } else if let Some((index, total)) = self.diff_viewer.merged_change_count_and_index() {
            left_parts.push(format!("change {index}/{total}"));
        }
        // `[plain text]` flags that no AST algorithm ran at all (unrecognized language on one
        // side). The old `[fast]`/`[exact]` mode label is gone along with the Fast/Exact prompt:
        // since the phases-4-7 rearchitecture, `PendingDiff::finish` runs the same pipeline
        // regardless of `DiffMode` (see its phase-6 comment), so a mode label described a
        // distinction that no longer exists.
        if self.plain_text_fallback {
            left_parts.push("[plain text]".to_string());
        }
        // The layout override only earns footer space when it's actually overriding something.
        let layout = self.diff_viewer.layout_override();
        if layout != crate::tui::theme::PanelLayout::Auto {
            left_parts.push(format!("[layout: {}]", layout.label()));
        }
        // Same rule as the layout override above, and it matters more here: turning an option off
        // deliberately leaves something unpainted (standalone brackets, leading whitespace), so
        // without a badge a reader who forgot they pressed `M` - or inherited the setting from a
        // previous run, since it persists - would read the missing highlights as codediff having
        // missed them. `FULL` is the default and what every release before this setting existed
        // rendered, so labelling it would put a permanent badge on a screen that has nothing to
        // report.
        let render_options = self.diff_viewer.render_options();
        if render_options != RenderOptions::FULL {
            if render_options == RenderOptions::MINIMAL {
                left_parts.push("[minimal]".to_string());
            } else {
                let off: Vec<&str> = render_options
                    .options()
                    .into_iter()
                    .filter(|(_, on)| !on)
                    .map(|(label, _)| label)
                    .collect();
                left_parts.push(format!("[{} off]", off.join(", ")));
            }
        }
        let left = left_parts.join("   ");

        let layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(70), Constraint::Min(1)])
            .split(area);
        frame.render_widget(
            Paragraph::new(left).style(Style::new().fg(Color::DarkGray)),
            layout[0],
        );
        frame.render_widget(
            Paragraph::new(FOOTER_HINTS)
                .style(Style::new().fg(Color::DarkGray))
                .alignment(Alignment::Right),
            layout[1],
        );
    }

    /// On the empty-start screen only (no file picked for either panel), overlay a small
    /// centered list of recently diffed pairs with their digit shortcuts - so a returning user
    /// can reopen yesterday's comparison with one keypress instead of two file-dialog trips.
    /// Draws nothing once any file is loaded, or when there's no history to offer.
    fn draw_recent_pairs(&self, frame: &mut ratatui::Frame, area: Rect) {
        if self.before_path.is_some() || self.after_path.is_some() || self.recent_pairs.is_empty() {
            return;
        }
        let shown = self.recent_pairs.len().min(9);
        let mut lines = vec![ratatui::text::Line::from("Recent pairs".to_string())];
        for (i, (before, after)) in self.recent_pairs.iter().take(shown).enumerate() {
            lines.push(ratatui::text::Line::from(format!(
                "  {}  {}  \u{2194}  {}",
                i + 1,
                before.display(),
                after.display()
            )));
        }
        lines.push(ratatui::text::Line::from(
            "  press a digit to reopen, or 'o' to browse".to_string(),
        ));

        let width = (lines
            .iter()
            .map(|l| l.width() as u16)
            .max()
            .unwrap_or(20)
            .saturating_add(4))
        .min(area.width);
        let height = (lines.len() as u16 + 2).min(area.height);
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let popup = Rect::new(x, y, width, height);
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Paragraph::new(lines).block(
                ratatui::widgets::Block::default()
                    .borders(ratatui::widgets::Borders::ALL)
                    .border_style(Style::new().fg(Color::Cyan)),
            ),
            popup,
        );
    }

    /// Draw the theme picker as a popup over the (still-visible) viewer behind it.
    fn draw_theme_dialog(&mut self, frame: &mut ratatui::Frame, area: Rect) -> Result<()> {
        self.draw_viewer(frame, area)?;
        let Some(dialog) = self.theme_dialog.as_mut() else {
            return Ok(());
        };
        let popup = dialog.popup_area(area);
        frame.render_widget(Clear, popup);
        dialog.draw(frame, popup)
    }

    /// Draw the `M` render-options panel as a popup over the (still-visible) viewer behind it -
    /// same convention as the theme dialog, and for the same reason: every toggle applies
    /// immediately, so the reader should see the diff repaint live as they check/uncheck options.
    fn draw_render_options_dialog(&mut self, frame: &mut ratatui::Frame, area: Rect) -> Result<()> {
        self.draw_viewer(frame, area)?;
        let Some(dialog) = self.render_options_dialog.as_mut() else {
            return Ok(());
        };
        let popup = dialog.popup_area(area);
        frame.render_widget(Clear, popup);
        dialog.draw(frame, popup)
    }

    /// Draw the `g` jump-to-line prompt as a popup over the (still-visible) viewer behind it,
    /// with a real terminal cursor at the end of the typed number - same convention as the
    /// search modal.
    fn draw_line_prompt(&mut self, frame: &mut ratatui::Frame, area: Rect) -> Result<()> {
        self.draw_viewer(frame, area)?;
        let Some(prompt) = self.line_prompt.as_mut() else {
            return Ok(());
        };
        let popup = prompt.popup_area(area);
        frame.render_widget(Clear, popup);
        prompt.draw(frame, popup)?;
        let (x, y) = prompt.cursor_screen_position(popup);
        frame.set_cursor(x, y);
        Ok(())
    }

    /// Draw the `?` keybinding reference as a popup over the (still-visible) viewer behind it.
    fn draw_help_modal(&mut self, frame: &mut ratatui::Frame, area: Rect) -> Result<()> {
        self.draw_viewer(frame, area)?;
        let Some(modal) = self.help_modal.as_mut() else {
            return Ok(());
        };
        let popup = modal.popup_area(area);
        frame.render_widget(Clear, popup);
        modal.draw(frame, popup)
    }

    /// Draw the `/` search input as a popup over the (still-visible) viewer behind it, with a real
    /// blinking terminal cursor at the end of the typed query - same convention as the focused
    /// code panel's own cursor (`CodeViewer::cursor_screen_position`).
    fn draw_search_modal(&mut self, frame: &mut ratatui::Frame, area: Rect) -> Result<()> {
        self.draw_viewer(frame, area)?;
        let Some(modal) = self.search_modal.as_mut() else {
            return Ok(());
        };
        let popup = modal.popup_area(area);
        frame.render_widget(Clear, popup);
        modal.draw(frame, popup)?;
        let (x, y) = modal.cursor_screen_position(popup);
        frame.set_cursor(x, y);
        Ok(())
    }
}

/// The footer's compact key-hint reference - deliberately just the handful of most-used keys, not
/// a full reference (that's `?`/`help_modal.rs`'s job).
const FOOTER_HINTS: &str =
    "?:help  o:open  r:reload  n/p:next/prev  /:search  M:options  Tab:switch  q:quit";

/// Formats a `ChangeCounts` as a compact `+12 -4 ~2` summary for the footer - omits any category
/// that's zero, and returns an empty string if every category is (e.g. a `NoChanges` diff, already
/// covered by the status bar above).
fn format_change_counts(counts: ChangeCounts) -> String {
    let mut parts = Vec::with_capacity(4);
    if counts.insertions > 0 {
        parts.push(format!("+{}", counts.insertions));
    }
    if counts.deletions > 0 {
        parts.push(format!("-{}", counts.deletions));
    }
    if counts.updates > 0 {
        parts.push(format!("~{}", counts.updates));
    }
    if counts.moves > 0 {
        parts.push(format!("M{}", counts.moves));
    }
    parts.join(" ")
}

/// The centered status shown while a blocking (screen-owning) diff computation is in flight.
/// Mentions the Esc cancel because it's only discoverable here - the footer isn't drawn on the
/// `Diffing` screen.
fn diffing_status_paragraph() -> Paragraph<'static> {
    Paragraph::new("Diffing\u{2026} (Esc cancels)").alignment(Alignment::Center)
}

/// Maps a `DiffSummary` to its status-bar styling - color-coded consistently with this TUI's
/// existing insert/delete/move conventions (`tui::headless::ansi_color` draws the same mapping for
/// the headless renderer): green for a pure addition, red for a pure removal, cyan for a pure
/// reformat, blue for a comment-only change, yellow for a pure reorganization, gray when there's
/// nothing to report at all. `DiffSummary` itself stays presentation-agnostic (just a label) - see
/// its own doc comment.
fn status_bar_paragraph(summary: DiffSummary) -> Paragraph<'static> {
    let color = match summary {
        DiffSummary::NoChanges => Color::DarkGray,
        DiffSummary::NewFile => Color::Green,
        DiffSummary::DeletedFile => Color::Red,
        DiffSummary::WhitespaceOnly => Color::Cyan,
        DiffSummary::CommentOnly => Color::Blue,
        DiffSummary::RefactorMovedOnly => Color::Yellow,
    };
    Paragraph::new(summary.label())
        .style(Style::new().fg(color).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
}

/// Parse both files, applying the `/dev/null` fallback below. Shared prefix of `compute_diff`
/// (headless/no-prompt callers) and `compute_diff_interactive` (the TUI's pause-capable path).
///
/// Does not require either side to have parsed successfully: a side with no tree-sitter grammar
/// (unrecognized extension, e.g. a `Makefile`) comes back with `ast: None` rather than an error -
/// `compute_diff`/`compute_diff_interactive` check for that themselves and route to
/// `diff::text::plain_text_line_diff` instead of the AST pipeline, rather than failing outright.
/// Before that fallback existed, this bailed with "unsupported or unrecognized file type" here,
/// which - via `headless`/`json_output`'s non-interactive callers - propagated as a fatal exit;
/// under `GIT_EXTERNAL_DIFF`, a non-zero exit for any one file aborts the *entire* multi-file
/// `git diff`, not just that file's view.
fn parse_before_after(before: &Path, after: &Path) -> Result<(Code, Code)> {
    let mut before_code = Code::from_file(before)?;
    let mut after_code = Code::from_file(after)?;

    // git's `difftool`/`GIT_EXTERNAL_DIFF` integration represents an added or deleted file by
    // handing us `/dev/null` for the missing side (see README's "Git integration" section).
    // `/dev/null` reads back as empty content with no extension, so `Code::from_file` can't
    // detect a language for it and leaves `ast` unset - re-parse that side as empty content in
    // the *other* side's language instead of leaving it unsupported, so the diff shows a normal
    // whole-file insert/delete rather than falling back to a plain-text diff for no reason. This
    // is the only situation where the two sides may legitimately disagree on detected language,
    // so it's safe to only kick in when the empty side's own language genuinely couldn't be
    // determined.
    let before_language = before_code.metadata.language;
    let after_language = after_code.metadata.language;
    substitute_missing_language(&mut before_code, after_language, before);
    substitute_missing_language(&mut after_code, before_language, after);

    Ok((before_code, after_code))
}

/// Builds the textual ranges needed to display an already-`finish`ed `Diff`.
#[allow(clippy::too_many_arguments)]
fn assemble_diff_session_data(
    before_path: &Path,
    after_path: &Path,
    before_code: &Code,
    after_code: &Code,
    diff: &Diff,
    mode: DiffMode,
    // [`RenderOptions::whole_pair_updates`]/[`RenderOptions::paint_reindent_only_moves`] - unlike
    // the rest of `RenderOptions`, these change which ranges `TextDiff` itself builds rather than
    // which of an already-built list get painted, so they have to reach in here rather than
    // `ranges_for_options`'s later filter pass.
    whole_pair_updates: bool,
    paint_reindent_only_moves: bool,
) -> Result<DiffSessionData> {
    let node_cache = NodeCache::build(before_code, after_code);
    let ast = diff.ast.as_ref().context("diff produced no AST mapping")?;
    let text_diff = TextDiff::from_with_options(
        before_code,
        after_code,
        ast,
        &node_cache,
        whole_pair_updates,
        paint_reindent_only_moves,
    );
    // Computed here, not later from DiffSessionData's own fields: needs AST-level node-kind
    // access (is_comment_only_diff), which is gone by the time DiffSessionData exists - see that
    // field's own doc comment.
    let comment_only = is_comment_only_diff(before_code, after_code, ast, &node_cache);

    Ok(DiffSessionData {
        before_path: before_path.to_path_buf(),
        after_path: after_path.to_path_buf(),
        before_contents: display_safe(&before_code.contents),
        after_contents: display_safe(&after_code.contents),
        before_ranges: text_diff.all(0),
        after_ranges: text_diff.all(1),
        comment_only,
        mode,
        plain_text_fallback: false,
    })
}

/// Replaces every literal tab with a single space, for text that's about to be stored as
/// `DiffSessionData::before_contents`/`after_contents` - i.e. handed to `ratatui` for rendering,
/// not to the diff engine (which already finished computing `before_ranges`/`after_ranges` against
/// the *original* text by the time this runs).
///
/// `ratatui::buffer::Buffer` treats every character as exactly one cell wide, `\t` included - but
/// a real terminal receiving a raw `\t` byte jumps its cursor to the next hardware tab stop
/// instead, desyncing the terminal's actual cursor column from the column `ratatui`'s own Buffer
/// model believes it's at. Every subsequent write on that line (and, since `ratatui` only
/// redraws cells it believes changed, on later frames too) lands at the wrong screen position -
/// exactly the "artifacts/repeated text that `?` doesn't clear" failure mode this fixes, confirmed
/// against a real tab-indented fixture (html-gohugoio-hugo-enclose-table-with-div-and-add-thead-tbody).
///
/// A single space, not an N-column tab-stop expansion: `\t` and `' '` are both exactly one UTF-8
/// byte, so this is a strict length-preserving substitution - every `RangeMatch`/`TextRange`
/// offset computed against the original tab-containing text upstream of this function stays
/// exactly as valid against the space-substituted text it returns.
fn display_safe(text: &str) -> String {
    text.replace('\t', " ")
}

/// `assemble_diff_session_data`'s counterpart for the plain-text fallback (see
/// `DiffSessionData::plain_text_fallback`'s doc comment): no `Diff`/`ASTDiff` exists, so this
/// builds `before_ranges`/`after_ranges` from `diff::text::plain_text_line_diff` directly instead
/// of `TextDiff::from`. `comment_only` is always `false` - there's no AST, so no concept of a
/// comment node to check.
fn assemble_plain_text_diff_session_data(
    before_path: &Path,
    after_path: &Path,
    before_code: &Code,
    after_code: &Code,
    mode: DiffMode,
) -> DiffSessionData {
    let (before_ranges, after_ranges) =
        plain_text_line_diff(&before_code.contents, &after_code.contents);
    DiffSessionData {
        before_path: before_path.to_path_buf(),
        after_path: after_path.to_path_buf(),
        before_contents: display_safe(&before_code.contents),
        after_contents: display_safe(&after_code.contents),
        before_ranges,
        after_ranges,
        comment_only: false,
        mode,
        plain_text_fallback: true,
    }
}

/// Parse, diff and compute the textual ranges needed to display the result, using `mode`
/// unconditionally - no prompting, for callers with no interactive UI to ask (headless mode, and
/// any caller that just wants a diff). Returns whether `DiffMode::Fast`'s guard silently
/// substituted the cheaper fallback for phase 6 (always `false` under `DiffMode::Exact`, and
/// under the plain-text fallback below - no AST algorithm ran at all, so the guard never had a
/// chance to trip), so headless mode can warn about it.
///
/// `pub(crate)` rather than private: `tui::headless` calls this directly too, since it's the same
/// terminal-independent diff computation either way - only what happens to the result (draw it
/// interactively vs. print it as text) differs between the two modes.
///
/// Test-only convenience: [`RenderOptions::whole_pair_updates`] off, unconditionally. Every real
/// (non-test) caller - `App::start_diff`, `tui::headless::run`, `tui::json_output::run` - resolves
/// that option from the live viewer or from CLI/config and calls
/// [`compute_diff_with_update_style`] directly, so as of that option's own toggle reaching the `M`
/// panel there is no production caller left that has no opinion on it. Kept anyway, `#[cfg(test)]`
/// gated, purely so the many tests across this module and `headless`/`json_output` that predate
/// that option don't all need updating just to keep passing `false`.
#[cfg(test)]
pub(crate) fn compute_diff(
    before: &Path,
    after: &Path,
    mode: DiffMode,
) -> Result<(DiffSessionData, bool)> {
    compute_diff_with_update_style(before, after, mode, false, true)
}

/// The real diff computation every production caller uses -
/// [`RenderOptions::whole_pair_updates`]/[`RenderOptions::paint_reindent_only_moves`] threaded
/// through to the AST-backed path rather than hardcoded to their legacy defaults.
/// `App::start_diff` reads them off the live `DiffViewer`; `tui::headless::run`/
/// `tui::json_output::run` read them off the `RenderOptions` CLI/config already resolved before a
/// diff is computed.
pub(crate) fn compute_diff_with_update_style(
    before: &Path,
    after: &Path,
    mode: DiffMode,
    whole_pair_updates: bool,
    paint_reindent_only_moves: bool,
) -> Result<(DiffSessionData, bool)> {
    let (before_code, after_code) = parse_before_after(before, after)?;
    if before_code.ast.is_none() || after_code.ast.is_none() {
        let data =
            assemble_plain_text_diff_session_data(before, after, &before_code, &after_code, mode);
        return Ok((data, false));
    }
    let pending = Diff::pending(&before_code, &after_code);
    let fallback_used = mode == DiffMode::Fast && pending.looks_expensive();
    let diff = pending.finish(mode);
    let data = assemble_diff_session_data(
        before,
        after,
        &before_code,
        &after_code,
        &diff,
        mode,
        whole_pair_updates,
        paint_reindent_only_moves,
    )?;
    Ok((data, fallback_used))
}

/// If `code` has no detected language and no content (the `/dev/null` case handled by
/// `compute_diff`), re-parse it as empty content in `fallback_language` instead of leaving it
/// unsupported.
fn substitute_missing_language(code: &mut Code, fallback_language: Option<Language>, path: &Path) {
    if code.ast.is_none() && code.contents.is_empty() {
        if let Some(language) = fallback_language {
            *code = Code::from_string("", &language);
            code.metadata.path = Some(path.to_path_buf());
        }
    }
}

/// Extract a human-readable message from a caught panic payload.
fn panic_message(panic: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        message.to_string()
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_file(contents: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().expect("create temp file");
        file.write_all(contents.as_bytes())
            .expect("write temp file");
        file
    }

    #[test]
    fn panic_message_extracts_str_payload() {
        let panic: Box<dyn std::any::Any + Send> = Box::new("boom");
        assert_eq!(panic_message(&panic), "boom");
    }

    #[test]
    fn panic_message_extracts_string_payload() {
        let panic: Box<dyn std::any::Any + Send> = Box::new("boom".to_string());
        assert_eq!(panic_message(&panic), "boom");
    }

    #[test]
    fn panic_message_falls_back_for_unknown_payload_type() {
        let panic: Box<dyn std::any::Any + Send> = Box::new(42i32);
        assert_eq!(panic_message(&panic), "unknown panic");
    }

    /// The diff must not start until *both* panels have a file, even if one panel is reselected
    /// after the other was already set.
    #[test]
    fn select_file_for_panel_only_starts_diff_once_both_sides_are_set() -> Result<()> {
        let mut app = App::new(4.0, 60.0)?;
        let before = write_temp_file("before contents");
        let after = write_temp_file("after contents");

        app.select_file_for_panel(Panel::Before, before.path().to_path_buf())?;
        assert_eq!(app.before_path, Some(before.path().to_path_buf()));
        assert!(
            app.action_rx.try_recv().is_err(),
            "StartDiff must not fire with only one side set"
        );

        app.select_file_for_panel(Panel::After, after.path().to_path_buf())?;
        assert_eq!(app.after_path, Some(after.path().to_path_buf()));
        let action = app
            .action_rx
            .try_recv()
            .expect("StartDiff must fire once both sides are set");
        assert_eq!(
            action,
            Action::StartDiff(
                before.path().to_path_buf(),
                after.path().to_path_buf(),
                DiffMode::Fast
            )
        );
        Ok(())
    }

    /// Regression guard for `git`'s add/delete convention (`difftool`/`GIT_EXTERNAL_DIFF` both
    /// hand codediff `/dev/null` for the missing side of an added or deleted file): before this
    /// fallback, `Code::from_file("/dev/null")` detected no language (no extension) and left
    /// `ast` unset, so `compute_diff` bailed with "unsupported or unrecognized file type" even
    /// though the other side parsed fine.
    #[test]
    fn compute_diff_treats_dev_null_before_as_an_empty_file_in_the_afters_language() -> Result<()> {
        let after = tempfile::Builder::new()
            .suffix(".rs")
            .tempfile()
            .expect("create temp file");
        std::fs::write(after.path(), "fn main() {}\n").expect("write temp file");

        let (data, _fallback_used) =
            compute_diff(Path::new("/dev/null"), after.path(), DiffMode::Fast)?;

        assert_eq!(data.before_contents, "");
        assert_eq!(data.after_contents, "fn main() {}\n");
        assert!(
            !data.after_ranges.is_empty(),
            "the whole after-file should show up as inserted"
        );
        Ok(())
    }

    /// Same as above, mirrored: the *after* side is `/dev/null` (a deleted file).
    #[test]
    fn compute_diff_treats_dev_null_after_as_an_empty_file_in_the_befores_language() -> Result<()> {
        let before = tempfile::Builder::new()
            .suffix(".rs")
            .tempfile()
            .expect("create temp file");
        std::fs::write(before.path(), "fn main() {}\n").expect("write temp file");

        let (data, _fallback_used) =
            compute_diff(before.path(), Path::new("/dev/null"), DiffMode::Fast)?;

        assert_eq!(data.before_contents, "fn main() {}\n");
        assert_eq!(data.after_contents, "");
        assert!(
            !data.before_ranges.is_empty(),
            "the whole before-file should show up as deleted"
        );
        Ok(())
    }

    /// The `/dev/null` fallback only kicks in for a genuinely empty, language-less side - a pair
    /// where neither side is empty (so there's real content on both sides, just no recognizable
    /// language) must fall through to the plain-text diff instead, not bail. Before that fallback
    /// existed, this bailed with "unsupported or unrecognized file type" - see
    /// `parse_before_after`'s own doc comment on why that used to be worse than it sounds
    /// (`GIT_EXTERNAL_DIFF` treats any non-zero exit as aborting the *whole* multi-file
    /// `git diff`, not just this one file).
    #[test]
    fn compute_diff_falls_back_to_plain_text_when_neither_side_has_a_recognizable_language()
    -> Result<()> {
        let before = write_temp_file("hello");
        let after = write_temp_file("world");

        let (data, fallback_used) = compute_diff(before.path(), after.path(), DiffMode::Fast)?;
        assert!(
            data.plain_text_fallback,
            "an unrecognized language on both sides should route through plain_text_line_diff"
        );
        assert!(
            !fallback_used,
            "fallback_used means DiffMode::Fast's phase-6 guard tripped, which never applies \
             here - no AST algorithm ran at all"
        );
        assert_eq!(data.before_contents, "hello");
        assert_eq!(data.after_contents, "world");
        assert!(!data.before_ranges.is_empty());
        assert!(!data.after_ranges.is_empty());
        Ok(())
    }

    #[test]
    fn open_files_loads_both_panels_and_starts_diff() -> Result<()> {
        let mut app = App::new(4.0, 60.0)?;
        let before = write_temp_file("before contents");
        let after = write_temp_file("after contents");

        app.open_files(before.path().to_path_buf(), after.path().to_path_buf())?;

        assert_eq!(app.before_path, Some(before.path().to_path_buf()));
        assert_eq!(app.after_path, Some(after.path().to_path_buf()));
        assert!(app.action_rx.try_recv().is_ok());
        Ok(())
    }

    /// Regression test: Esc used to quit the whole app instead of closing the theme picker,
    /// because `SelectTheme` was missing from a hand-maintained exclusion list (twice-extended,
    /// missed both times). Every screen with its own dialog must resolve Esc itself, not quit.
    #[test]
    fn esc_should_quit_is_false_for_every_screen_with_its_own_dialog() {
        assert!(!esc_should_quit(AppScreen::SelectFile));
        assert!(!esc_should_quit(AppScreen::SelectTheme));
        assert!(!esc_should_quit(AppScreen::Help));
        assert!(!esc_should_quit(AppScreen::Search));
        assert!(!esc_should_quit(AppScreen::JumpToLine));
    }

    #[test]
    fn esc_should_quit_is_true_only_on_the_bare_viewer() {
        assert!(esc_should_quit(AppScreen::Viewer));
        // Esc during Diffing cancels the computation instead of quitting - see the dedicated
        // Esc arm in `handle_events`.
        assert!(!esc_should_quit(AppScreen::Diffing));
    }

    /// Esc during `Diffing` bumps the generation, so the eventually arriving `DiffComputed` is
    /// recognized as stale and never re-dispatched as `DiffReady` - i.e. the cancel actually
    /// discards the result instead of just hiding the screen.
    #[test]
    fn a_stale_diff_computed_result_is_dropped_after_cancel() -> Result<()> {
        let mut app = App::new(4.0, 60.0)?;
        app.screen = AppScreen::Diffing;
        app.diff_generation = 1; // as if start_diff had launched generation 1

        // The cancel: what the Esc arm in handle_events does.
        app.diff_generation += 1;
        app.screen = AppScreen::Viewer;

        // The stale result arrives afterwards.
        app.action_tx.send(Action::DiffComputed {
            generation: 1,
            outcome: DiffOutcome::Failed("too late".to_string()),
        })?;
        let mut ui_free_actions = Vec::new();
        while let Ok(action) = app.action_rx.try_recv() {
            match &action {
                Action::DiffComputed { .. } => {
                    // Simulate handle_actions' arm without needing a real UI.
                    if let Action::DiffComputed { generation, .. } = &action
                        && *generation == app.diff_generation
                    {
                        ui_free_actions.push(action.clone());
                    }
                }
                other => ui_free_actions.push(other.clone()),
            }
        }
        assert!(
            ui_free_actions.is_empty(),
            "a stale generation must produce no follow-up actions: {ui_free_actions:?}"
        );
        assert!(app.last_error.is_none());
        Ok(())
    }

    #[test]
    fn handle_dialog_cancelled_resets_dialog_state() {
        let mut app = App::new(4.0, 60.0).expect("construct App");
        app.screen = AppScreen::SelectFile;
        app.dialog_target = Some(Panel::Before);

        app.handle_dialog_cancelled();

        assert_eq!(app.screen, AppScreen::Viewer);
        assert_eq!(app.dialog_target, None);
        assert!(app.file_dialog.is_none());
    }

    /// `whole_pair_updates` changes which ranges the diff itself has - a plain re-filter can't
    /// reach it, so this is the one field whose toggle must reload the diff.
    #[test]
    fn apply_render_options_reloads_when_whole_pair_updates_changes() -> Result<()> {
        let mut app = App::new(4.0, 60.0)?;
        app.before_path = Some(PathBuf::from("before.rs"));
        app.after_path = Some(PathBuf::from("after.rs"));
        assert!(!app.diff_viewer.render_options().whole_pair_updates);

        app.apply_render_options(RenderOptions {
            whole_pair_updates: true,
            ..RenderOptions::default()
        })?;

        assert!(app.diff_viewer.render_options().whole_pair_updates);
        let queued: Vec<_> = std::iter::from_fn(|| app.action_rx.try_recv().ok()).collect();
        assert!(
            matches!(
                queued.as_slice(),
                [Action::StartDiff(before, after, DiffMode::Fast)]
                    if before == Path::new("before.rs") && after == Path::new("after.rs")
            ),
            "expected exactly one StartDiff reload, got {queued:?}"
        );
        Ok(())
    }

    /// The other two fields are real post-filters - toggling only them must stay instant, with no
    /// reload queued.
    #[test]
    fn apply_render_options_does_not_reload_for_the_other_fields() -> Result<()> {
        let mut app = App::new(4.0, 60.0)?;
        app.before_path = Some(PathBuf::from("before.rs"));
        app.after_path = Some(PathBuf::from("after.rs"));

        app.apply_render_options(RenderOptions {
            leading_whitespace: false,
            structural_punctuation: false,
            whole_pair_updates: false,
            paint_reindent_only_moves: true,
        })?;

        let queued: Vec<_> = std::iter::from_fn(|| app.action_rx.try_recv().ok()).collect();
        assert!(
            queued.is_empty(),
            "leading_whitespace/structural_punctuation must not trigger a reload: {queued:?}"
        );
        Ok(())
    }

    /// Nothing is open yet (the empty-start screen) - there is no pair to reload, so the change
    /// must apply to the (empty) viewer without trying to queue a diff for a path that doesn't
    /// exist.
    #[test]
    fn apply_render_options_with_no_open_pair_does_not_queue_a_reload() -> Result<()> {
        let mut app = App::new(4.0, 60.0)?;
        assert!(app.before_path.is_none());
        assert!(app.after_path.is_none());

        app.apply_render_options(RenderOptions {
            whole_pair_updates: true,
            ..RenderOptions::default()
        })?;

        assert!(app.diff_viewer.render_options().whole_pair_updates);
        let queued: Vec<_> = std::iter::from_fn(|| app.action_rx.try_recv().ok()).collect();
        assert!(queued.is_empty(), "nothing to reload: {queued:?}");
        Ok(())
    }

    /// Regression test for the exact mechanism `handle_events`'s `globally_handled` guard exists
    /// to prevent: without it, the `?` keystroke that opens the help modal would *also* reach
    /// `dispatch_event_to_active_screen` in the same event cycle (since `self.screen` is already
    /// `Help` by the time that call happens), and `HelpModal` treats `?` as its own close key too
    /// - so the same keypress that opens it would immediately close it again, meaning it would
    ///   never visibly show up at all. `handle_events` itself can't be unit-tested directly (it
    ///   owns a real `UI`/terminal, which every other test in this module also avoids), so this
    ///   pins the hazard at the one layer that is testable: simulating exactly the state
    ///   `handle_events` leaves behind right after opening the modal, and confirming that
    ///   re-delivering the same keystroke to it would indeed cancel it.
    #[test]
    fn redelivering_the_opening_keystroke_would_immediately_close_the_help_modal() {
        use crossterm::event::{KeyEvent, KeyModifiers};

        let mut app = App::new(4.0, 60.0).expect("construct App");
        app.help_modal = Some(crate::tui::components::help_modal::HelpModal::new(
            OverlayTheme::default(),
        ));
        app.screen = AppScreen::Help;

        let event = Event::Key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        let action = app.dispatch_event_to_active_screen(event).unwrap();

        assert_eq!(
            action,
            Some(Action::DialogCancelled),
            "confirms why handle_events must skip re-dispatching the keystroke that just opened \
             this screen, not just document it"
        );
    }

    #[test]
    fn handle_search_submitted_jumps_the_focused_panel_and_returns_to_the_viewer_screen() {
        let mut app = App::new(4.0, 60.0).expect("construct App");
        app.screen = AppScreen::Search;
        app.search_modal = Some(SearchModal::new(None));
        app.diff_viewer.load_diff(&DiffSessionData {
            before_path: PathBuf::from("before.rs"),
            after_path: PathBuf::from("after.rs"),
            before_contents: "foo\nbar\n".to_string(),
            after_contents: "foo\nbar\n".to_string(),
            before_ranges: Vec::new(),
            after_ranges: Vec::new(),
            comment_only: false,
            mode: DiffMode::Fast,
            plain_text_fallback: false,
        });

        app.handle_search_submitted("bar".to_string());

        assert_eq!(app.screen, AppScreen::Viewer);
        assert!(app.search_modal.is_none());
        assert_eq!(app.diff_viewer.focused_cursor_position(), Some((1, 0)));
    }

    /// Same hazard as `redelivering_the_opening_keystroke_would_immediately_close_the_help_modal`,
    /// but for `/`: `SearchModal` treats every `Char` key as "append to the query," so without
    /// `handle_events`'s `globally_handled` guard, the very keystroke that opens the modal would
    /// also reach it in the same event cycle and seed the query with a stray `/`.
    #[test]
    fn redelivering_the_opening_keystroke_would_seed_the_search_query_with_a_stray_slash() {
        use crossterm::event::{KeyEvent, KeyModifiers};

        let mut app = App::new(4.0, 60.0).expect("construct App");
        app.search_modal = Some(SearchModal::new(None));
        app.screen = AppScreen::Search;

        let event = Event::Key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        app.dispatch_event_to_active_screen(event).unwrap();

        assert_eq!(
            app.search_modal.unwrap().query(),
            "/",
            "confirms why handle_events must skip re-dispatching the keystroke that just opened \
             this screen, not just document it"
        );
    }

    #[test]
    fn handle_file_selected_loads_into_the_dialogs_target_panel() -> Result<()> {
        let mut app = App::new(4.0, 60.0)?;
        let before = write_temp_file("before contents");
        app.dialog_target = Some(Panel::Before);

        app.handle_file_selected(before.path().to_path_buf())?;

        assert_eq!(app.before_path, Some(before.path().to_path_buf()));
        assert_eq!(app.screen, AppScreen::Viewer);
        assert!(app.dialog_target.is_none());
        Ok(())
    }

    /// `apply_theme_selection` is the only place that calls `theme::save_overlay_theme`, which
    /// writes to the real process cwd (there's no per-test path injection for it, since the
    /// production code intentionally always targets the cwd it's run from). Clean up the file
    /// this test causes to be written so repeated runs don't see a stale leftover.
    #[test]
    fn apply_theme_selection_updates_viewer_and_returns_to_the_viewer_screen() {
        let mut app = App::new(4.0, 60.0).expect("construct App");
        app.screen = AppScreen::SelectTheme;
        app.theme_dialog = Some(ThemeDialog::new(app.current_theme));

        app.apply_theme_selection(OverlayTheme::SolarizedLight);

        assert_eq!(app.current_theme, OverlayTheme::SolarizedLight);
        assert_eq!(app.screen, AppScreen::Viewer);
        assert!(app.theme_dialog.is_none());

        let _ = std::fs::remove_file(theme::config_path());
    }

    fn rendered_text(terminal: &ratatui::Terminal<ratatui::backend::TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    /// `MINIMAL` deliberately leaves brackets, separators and leading whitespace unpainted, and
    /// the setting persists across runs - so without this badge, missing highlights read as
    /// codediff having missed them rather than as a preset the reader chose (possibly in a
    /// previous session).
    #[test]
    fn draw_viewer_badges_minimal_options_in_the_footer() -> Result<()> {
        let mut app = App::new(4.0, 60.0)?;
        app.diff_viewer.set_render_options(RenderOptions::MINIMAL);

        let backend = ratatui::backend::TestBackend::new(120, 24);
        let mut terminal = ratatui::Terminal::new(backend)?;
        terminal.draw(|f| {
            let area = f.size();
            app.draw_viewer(f, area).unwrap();
        })?;

        assert!(
            rendered_text(&terminal).contains("[minimal]"),
            "expected the minimal badge in the footer"
        );
        Ok(())
    }

    /// `FULL` is the default and what every release before this setting existed rendered, so a
    /// badge for it would sit permanently on a screen with nothing to report.
    #[test]
    fn draw_viewer_does_not_badge_full_options() -> Result<()> {
        let mut app = App::new(4.0, 60.0)?;
        app.diff_viewer.set_render_options(RenderOptions::FULL);

        let backend = ratatui::backend::TestBackend::new(120, 24);
        let mut terminal = ratatui::Terminal::new(backend)?;
        terminal.draw(|f| {
            let area = f.size();
            app.draw_viewer(f, area).unwrap();
        })?;

        let text = rendered_text(&terminal);
        assert!(!text.contains("[minimal]"), "got: {text}");
        assert!(!text.contains("[full]"), "got: {text}");
        Ok(())
    }

    /// Neither preset - only one option off. The badge must name what's missing, not just
    /// collapse to the `[minimal]` label, or a reader would misread this as "everything off".
    #[test]
    fn draw_viewer_badges_a_single_disabled_option_by_name() -> Result<()> {
        let mut app = App::new(4.0, 60.0)?;
        app.diff_viewer.set_render_options(RenderOptions {
            leading_whitespace: false,
            structural_punctuation: true,
            whole_pair_updates: false,
            paint_reindent_only_moves: true,
        });

        let backend = ratatui::backend::TestBackend::new(120, 24);
        let mut terminal = ratatui::Terminal::new(backend)?;
        terminal.draw(|f| {
            let area = f.size();
            app.draw_viewer(f, area).unwrap();
        })?;

        let text = rendered_text(&terminal);
        assert!(!text.contains("[minimal]"), "got: {text}");
        assert!(text.contains("Leading whitespace"), "got: {text}");
        Ok(())
    }

    #[test]
    fn draw_viewer_shows_the_diff_summary_status_bar_when_set() -> Result<()> {
        let mut app = App::new(4.0, 60.0)?;
        app.diff_summary = Some(DiffSummary::NewFile);

        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend)?;
        terminal.draw(|f| {
            let area = f.size();
            app.draw_viewer(f, area).unwrap();
        })?;

        assert!(
            rendered_text(&terminal).contains(DiffSummary::NewFile.label()),
            "expected the New file status bar to be drawn"
        );
        Ok(())
    }

    #[test]
    fn draw_viewer_shows_no_status_bar_when_diff_summary_is_none() -> Result<()> {
        let mut app = App::new(4.0, 60.0)?;
        assert!(app.diff_summary.is_none());

        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend)?;
        terminal.draw(|f| {
            let area = f.size();
            app.draw_viewer(f, area).unwrap();
        })?;

        let text = rendered_text(&terminal);
        for summary in [
            DiffSummary::NoChanges,
            DiffSummary::NewFile,
            DiffSummary::DeletedFile,
            DiffSummary::WhitespaceOnly,
            DiffSummary::RefactorMovedOnly,
        ] {
            assert!(
                !text.contains(summary.label()),
                "no summary label should render when diff_summary is None: {}",
                summary.label()
            );
        }
        Ok(())
    }

    /// The footer's key hints must render regardless of whether the status bar or error banner
    /// are present - it's the primary discoverability aid for `?`, so it can't be conditionally
    /// absent the way those two are (see `draw_viewer`'s own doc comment).
    #[test]
    fn draw_viewer_always_shows_the_footer_key_hints() -> Result<()> {
        let mut app = App::new(4.0, 60.0)?;
        assert!(app.diff_summary.is_none());
        assert!(app.last_error.is_none());

        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend)?;
        terminal.draw(|f| {
            let area = f.size();
            app.draw_viewer(f, area).unwrap();
        })?;

        assert!(
            rendered_text(&terminal).contains("?:help"),
            "expected the footer's key hints to be drawn even with no status bar or error banner"
        );
        Ok(())
    }

    #[test]
    fn format_change_counts_omits_zero_categories() {
        assert_eq!(
            format_change_counts(ChangeCounts {
                insertions: 12,
                deletions: 4,
                updates: 2,
                moves: 3,
            }),
            "+12 -4 ~2 M3"
        );
        assert_eq!(
            format_change_counts(ChangeCounts {
                insertions: 3,
                deletions: 0,
                updates: 0,
                moves: 0,
            }),
            "+3"
        );
        assert_eq!(
            format_change_counts(ChangeCounts {
                insertions: 0,
                deletions: 0,
                updates: 0,
                moves: 0,
            }),
            ""
        );
    }

    #[test]
    fn draw_viewer_shows_change_counts_in_the_footer_when_set() -> Result<()> {
        let mut app = App::new(4.0, 60.0)?;
        app.change_counts = Some(ChangeCounts {
            insertions: 12,
            deletions: 4,
            updates: 2,
            moves: 0,
        });

        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend)?;
        terminal.draw(|f| {
            let area = f.size();
            app.draw_viewer(f, area).unwrap();
        })?;

        assert!(
            rendered_text(&terminal).contains("+12 -4 ~2"),
            "expected the change counts to be drawn in the footer"
        );
        Ok(())
    }

    #[test]
    fn draw_viewer_shows_change_progress_in_the_footer_once_the_focused_panel_has_changes()
    -> Result<()> {
        let mut app = App::new(4.0, 60.0)?;
        app.diff_viewer.load_diff(&DiffSessionData {
            before_path: PathBuf::from("before.rs"),
            after_path: PathBuf::from("after.rs"),
            before_contents: "a\nb\nc".to_string(),
            after_contents: "a\nb\nc".to_string(),
            before_ranges: vec![crate::diff::text::RangeMatch {
                source: crate::diff::text_range::TextRange::new(1, 0, 1, 1),
                destination: crate::diff::text_range::TextRange::new(1, 0, 1, 1),
                operation: crate::diff::text::TextOperation::Update,
            }],
            after_ranges: vec![crate::diff::text::RangeMatch {
                source: crate::diff::text_range::TextRange::new(1, 0, 1, 1),
                destination: crate::diff::text_range::TextRange::new(1, 0, 1, 1),
                operation: crate::diff::text::TextOperation::Update,
            }],
            comment_only: false,
            mode: DiffMode::Fast,
            plain_text_fallback: false,
        });

        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend)?;
        terminal.draw(|f| {
            let area = f.size();
            app.draw_viewer(f, area).unwrap();
        })?;

        assert!(
            rendered_text(&terminal).contains("change 1/1"),
            "expected the footer to show change progress once the focused panel has a change"
        );
        Ok(())
    }

    /// While a search is active, the footer's progress indicator must show "match N/M" - not
    /// "change N/M" - even when the diff itself also has changes, since the two would otherwise
    /// compete for the same fixed-width footer column (see `draw_footer`'s own comment).
    #[test]
    fn draw_viewer_shows_search_match_progress_in_the_footer_in_place_of_change_progress()
    -> Result<()> {
        let mut app = App::new(4.0, 60.0)?;
        app.diff_viewer.load_diff(&DiffSessionData {
            before_path: PathBuf::from("before.rs"),
            after_path: PathBuf::from("after.rs"),
            before_contents: "foo\nbar\nfoo bar\n".to_string(),
            after_contents: "foo\nbar\nfoo bar\n".to_string(),
            before_ranges: vec![crate::diff::text::RangeMatch {
                source: crate::diff::text_range::TextRange::new(1, 0, 1, 1),
                destination: crate::diff::text_range::TextRange::new(1, 0, 1, 1),
                operation: crate::diff::text::TextOperation::Update,
            }],
            after_ranges: vec![crate::diff::text::RangeMatch {
                source: crate::diff::text_range::TextRange::new(1, 0, 1, 1),
                destination: crate::diff::text_range::TextRange::new(1, 0, 1, 1),
                operation: crate::diff::text::TextOperation::Update,
            }],
            comment_only: false,
            mode: DiffMode::Fast,
            plain_text_fallback: false,
        });
        app.diff_viewer.search("bar");

        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend)?;
        terminal.draw(|f| {
            let area = f.size();
            app.draw_viewer(f, area).unwrap();
        })?;

        let text = rendered_text(&terminal);
        assert!(
            text.contains("match 1/2"),
            "expected the footer to show search-match progress"
        );
        assert!(
            !text.contains("change 1/"),
            "change progress must be replaced, not shown alongside, search-match progress"
        );
        Ok(())
    }

    #[test]
    fn draw_viewer_shows_plain_text_fallback_in_the_footer() -> Result<()> {
        let mut app = App::new(4.0, 60.0)?;
        app.plain_text_fallback = true;

        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend)?;
        terminal.draw(|f| {
            let area = f.size();
            app.draw_viewer(f, area).unwrap();
        })?;

        let text = rendered_text(&terminal);
        assert!(
            text.contains("[plain text]"),
            "expected the plain-text fallback indicator to be drawn in the footer"
        );
        Ok(())
    }

    /// Regression guard for the layout math in `draw_viewer`: both a summary bar (top) and an
    /// error banner (bottom) must be able to show at once, around the panels in the middle -
    /// dynamic `Vec<Constraint>` indexing is exactly the kind of code an off-by-one silently
    /// swallows one of the two rows without a test actually rendering both together.
    #[test]
    fn draw_viewer_shows_both_the_status_bar_and_the_error_banner_at_once() -> Result<()> {
        let mut app = App::new(4.0, 60.0)?;
        app.diff_summary = Some(DiffSummary::RefactorMovedOnly);
        app.last_error = Some("unsupported file type".to_string());

        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend)?;
        terminal.draw(|f| {
            let area = f.size();
            app.draw_viewer(f, area).unwrap();
        })?;

        let text = rendered_text(&terminal);
        assert!(text.contains(DiffSummary::RefactorMovedOnly.label()));
        assert!(text.contains("unsupported file type"));
        Ok(())
    }

    /// `handle_actions` itself needs a real `UI` (wraps a live terminal backend), which no other
    /// test in this module constructs - see those tests' own comments on why they exercise the
    /// smaller, directly-callable handlers instead. This checks the one part of the
    /// `Action::DiffReady` handler that's actually novel here (the rest - `self.screen`,
    /// `self.last_error`, `self.file_dialog` - already existed and is untouched): that
    /// `summarize_diff_with_comment_check`'s result is what ends up in `self.diff_summary`, using
    /// the exact same `DiffSessionData` fields the real handler reads.
    #[test]
    fn diff_ready_summary_matches_summarize_diff_on_the_same_session_data() {
        let data = DiffSessionData {
            before_path: PathBuf::from("before.rs"),
            after_path: PathBuf::from("after.rs"),
            before_contents: String::new(),
            after_contents: "fn main() {}".to_string(),
            before_ranges: vec![],
            after_ranges: vec![crate::diff::text::RangeMatch {
                source: crate::diff::text_range::TextRange::new(0, 0, 1, 0),
                destination: crate::diff::text_range::TextRange::new(0, 0, 1, 0),
                operation: crate::diff::text::TextOperation::Insert,
            }],
            comment_only: false,
            mode: DiffMode::Fast,
            plain_text_fallback: false,
        };

        let summary = summarize_diff_with_comment_check(
            &data.before_contents,
            &data.after_contents,
            &data.before_ranges,
            &data.after_ranges,
            data.comment_only,
        );

        assert_eq!(summary, Some(DiffSummary::NewFile));
    }

    /// Regression guard for `summarize_diff_with_comment_check`'s wiring specifically (not
    /// `is_comment_only_diff`'s own logic, already covered in `diff::text`'s tests): the
    /// `comment_only` flag on `DiffSessionData` must actually reach the final `DiffSummary`, not
    /// just get carried around unused.
    #[test]
    fn diff_ready_summary_reports_comment_only_when_the_session_data_says_so() {
        let ranges = vec![crate::diff::text::RangeMatch {
            source: crate::diff::text_range::TextRange::new(0, 0, 1, 0),
            destination: crate::diff::text_range::TextRange::new(0, 0, 1, 0),
            operation: crate::diff::text::TextOperation::Update,
        }];
        let data = DiffSessionData {
            before_path: PathBuf::from("before.rs"),
            after_path: PathBuf::from("after.rs"),
            before_contents: "// old comment".to_string(),
            after_contents: "// new comment".to_string(),
            before_ranges: ranges.clone(),
            after_ranges: ranges,
            comment_only: true,
            mode: DiffMode::Fast,
            plain_text_fallback: false,
        };

        let summary = summarize_diff_with_comment_check(
            &data.before_contents,
            &data.after_contents,
            &data.before_ranges,
            &data.after_ranges,
            data.comment_only,
        );

        assert_eq!(summary, Some(DiffSummary::CommentOnly));
    }

    /// Full pipeline, not synthetic data: `compute_diff` -> `assemble_diff_session_data` (where
    /// `comment_only` actually gets computed, from real AST access) -> `summarize_diff_with_
    /// comment_check`. Regression guard for the real end-to-end wiring, since
    /// `diff_ready_summary_reports_comment_only_when_the_session_data_says_so` only proves the
    /// combinator itself is correct given a `comment_only` flag handed to it directly, not that
    /// the real pipeline ever sets that flag to `true` for a genuine comment-only diff.
    #[test]
    fn compute_diff_reports_comment_only_for_a_real_inserted_comment() -> Result<()> {
        let before = tempfile::Builder::new()
            .suffix(".rs")
            .tempfile()
            .expect("create temp file");
        std::fs::write(before.path(), "fn main() {}\n").expect("write temp file");
        let after = tempfile::Builder::new()
            .suffix(".rs")
            .tempfile()
            .expect("create temp file");
        std::fs::write(after.path(), "// a comment\nfn main() {}\n").expect("write temp file");

        let (data, _fallback_used) = compute_diff(before.path(), after.path(), DiffMode::Exact)?;
        assert!(
            data.comment_only,
            "inserting only a comment should set comment_only"
        );

        let summary = summarize_diff_with_comment_check(
            &data.before_contents,
            &data.after_contents,
            &data.before_ranges,
            &data.after_ranges,
            data.comment_only,
        );
        assert_eq!(summary, Some(DiffSummary::CommentOnly));
        Ok(())
    }

    /// Regression test for the "artifacts / repeated text that `?` doesn't clear" TUI corruption
    /// bug: `ratatui::buffer::Buffer` treats every character (`\t` included) as exactly one cell
    /// wide, but a real terminal receiving a raw tab byte jumps its actual cursor to the next
    /// hardware tab stop instead - desyncing the terminal's real cursor column from the column
    /// `ratatui`'s own Buffer model believes it's at, corrupting everything drawn afterward.
    /// `display_safe` fixes this by replacing every `\t` with a single space (`\t` and `' '` are
    /// both exactly one UTF-8 byte, so this can't shift any `RangeMatch`/`TextRange` offset
    /// computed against the original text). Confirmed against the real tab-indented fixture that
    /// exposed the bug during manual TUI testing, not just a synthetic string.
    #[test]
    fn compute_diff_never_puts_a_raw_tab_into_diff_session_data_contents() -> Result<()> {
        let before = Path::new(
            "src/test/data/diffs/small/html-gohugoio-hugo-enclose-table-with-div-and-add-thead-tbody/before.html.test",
        );
        let after = Path::new(
            "src/test/data/diffs/small/html-gohugoio-hugo-enclose-table-with-div-and-add-thead-tbody/after.html.test",
        );
        // The fixture must actually contain a raw tab for this test to mean anything.
        assert!(std::fs::read_to_string(before)?.contains('\t'));

        let (data, _fallback) = compute_diff(before, after, DiffMode::Fast)?;

        assert!(
            !data.before_contents.contains('\t'),
            "before_contents still has a raw tab byte"
        );
        assert!(
            !data.after_contents.contains('\t'),
            "after_contents still has a raw tab byte"
        );
        Ok(())
    }
}
