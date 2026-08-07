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
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error};

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::code::{Code, Language};
use crate::diff::{
    Diff, DiffMode, NodeCache,
    text::{
        ChangeCounts, DiffSummary, TextDiff, change_counts, is_comment_only_diff,
        summarize_diff_with_comment_check,
    },
};
use crate::tui::actions::{Action, DiffSessionData};
use crate::tui::components::{
    Component,
    diff_mode_dialog::DiffModeDialog,
    diff_viewer::{DiffViewer, Panel},
    file_dialog::FileDialog,
    help_modal::HelpModal,
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
    /// The background diff computation is running.
    Diffing,
    /// The background diff computation is paused, waiting for the user to answer
    /// `Action::DiffModeChoiceNeeded` (the phase-1-5 residual was too large for `DiffMode::Fast`
    /// to auto-resolve silently - see `PendingDiff::looks_expensive`).
    SelectDiffMode,
    /// The `?` keybinding reference is open, drawn over the (still-visible) viewer.
    Help,
    /// The `/` search modal is open, drawn over the (still-visible) viewer.
    Search,
}

/// Whether pressing Esc on `screen` should quit the app, rather than being handled by that
/// screen's own dialog (via `Action::DialogCancelled`, or - for `SelectDiffMode` - resolving to
/// its current default selection instead of "cancel," which has no obvious meaning once a diff is
/// already in flight and the background thread is blocked waiting for *some* answer).
///
/// Deliberately an exhaustive match, not a list of `!=` exclusions: that list was extended twice
/// before (for `SelectDiffMode`, then `Help`) and `SelectTheme` was missed *both* times, letting
/// Esc silently quit the whole app instead of closing the theme picker - the only dialog that
/// happened to (found in a 2026-07 code-health pass). An exhaustive match means the compiler
/// itself forces every future `AppScreen` variant to be considered here, instead of relying on
/// someone remembering to extend a growing exclusion list.
fn esc_should_quit(screen: AppScreen) -> bool {
    match screen {
        AppScreen::Viewer | AppScreen::Diffing => true,
        AppScreen::SelectFile
        | AppScreen::SelectTheme
        | AppScreen::SelectDiffMode
        | AppScreen::Help
        | AppScreen::Search => false,
    }
}

/// The codediff application. The state, but not the state machine or the UI, of the TUI.
pub struct App {
    tick_rate: f64,
    frame_rate: f64,

    diff_viewer: DiffViewer,
    file_dialog: Option<FileDialog>,
    theme_dialog: Option<ThemeDialog>,
    diff_mode_dialog: Option<DiffModeDialog>,
    help_modal: Option<HelpModal>,
    search_modal: Option<SearchModal>,

    action_tx: mpsc::UnboundedSender<Action>,
    action_rx: mpsc::UnboundedReceiver<Action>,
    /// Shared with whichever background diff thread is currently running (if any): once
    /// `PendingDiff::looks_expensive()` trips, `compute_diff_interactive` stashes the
    /// `oneshot::Sender` half here, and `Action::DiffModeSelected`'s handler (running on the main
    /// task - a *different* OS thread than the `spawn_blocking` closure) retrieves it to unblock
    /// that closure's `blocking_recv()`. There is never more than one diff in flight at a time
    /// (`AppScreen::Diffing`/`SelectDiffMode` both make `dispatch_event_to_active_screen` ignore
    /// new `StartDiff`s), so a single-slot cell is sufficient. Needs `Arc<Mutex<_>>`, not a bare
    /// field, because it's written to from the `spawn_blocking` closure's own OS thread.
    pending_diff_mode_tx: Arc<Mutex<Option<oneshot::Sender<DiffMode>>>>,

    screen: AppScreen,
    /// Which panel the open file dialog is selecting a file for.
    dialog_target: Option<Panel>,
    /// The currently active overlay color theme, persisted across runs (see `tui::theme`).
    current_theme: OverlayTheme,
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
    /// Which `DiffMode` produced the currently loaded diff (see `DiffSessionData::mode`), shown in
    /// the footer so a user can't forget whether they're looking at the fast, approximate result
    /// or the exact one. Same lifecycle as `diff_summary`/`change_counts`.
    diff_mode: Option<DiffMode>,

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
            diff_mode_dialog: None,
            help_modal: None,
            search_modal: None,
            action_tx,
            action_rx,
            pending_diff_mode_tx: Arc::new(Mutex::new(None)),
            screen: AppScreen::default(),
            dialog_target: None,
            current_theme: OverlayTheme::default(),
            before_path: None,
            after_path: None,
            last_error: None,
            diff_summary: None,
            change_counts: None,
            diff_mode: None,
            should_exit: false,
            should_suspend: false,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        // Loaded here rather than in `new` so constructing an `App` (e.g. in tests) never
        // touches disk; only actually running the TUI reads (and may create) the config file.
        self.current_theme = theme::load_overlay_theme();
        self.diff_viewer.set_overlay_theme(self.current_theme);

        let mut ui = UI::new()?
            .tick_rate(self.tick_rate)
            .frame_rate(self.frame_rate);
        ui.enter()?;

        self.diff_viewer
            .register_action_handler(self.action_tx.clone())?;
        self.diff_viewer.init(ui.size()?)?;

        let action_tx = self.action_tx.clone();
        loop {
            self.handle_events(&mut ui).await?;
            self.handle_actions(&mut ui)?;
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
                    self.theme_dialog = Some(ThemeDialog::new(self.current_theme));
                    self.screen = AppScreen::SelectTheme;
                    action_tx.send(Action::Render)?;
                    globally_handled = true;
                }
                KeyCode::Char('?') if self.screen == AppScreen::Viewer => {
                    self.help_modal = Some(HelpModal::new());
                    self.screen = AppScreen::Help;
                    action_tx.send(Action::Render)?;
                    globally_handled = true;
                }
                KeyCode::Char('/') if self.screen == AppScreen::Viewer => {
                    self.search_modal = Some(SearchModal::new());
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
            AppScreen::Diffing => Ok(None),
            AppScreen::SelectDiffMode => match self.diff_mode_dialog.as_mut() {
                Some(dialog) => dialog.handle_events(Some(event)),
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
                Action::StartDiff(before, after) => {
                    self.screen = AppScreen::Diffing;
                    self.diff_summary = None;
                    self.change_counts = None;
                    self.diff_mode = None;
                    self.start_diff(before.clone(), after.clone());
                }
                Action::DiffReady(data) => {
                    self.screen = AppScreen::Viewer;
                    self.last_error = None;
                    self.file_dialog = None;
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
                    self.diff_mode = Some(data.mode);
                }
                Action::DiffFailed(message) => {
                    error!("diff failed: {message}");
                    self.last_error = Some(message.clone());
                    self.screen = AppScreen::Viewer;
                    self.file_dialog = None;
                    self.diff_summary = None;
                    self.change_counts = None;
                    self.diff_mode = None;
                }
                Action::Error(message) => {
                    error!("{message}");
                    self.last_error = Some(message.clone());
                }
                Action::ThemeSelected(selected_theme) => {
                    self.apply_theme_selection(*selected_theme)
                }
                Action::DiffModeChoiceNeeded {
                    unmatched_before,
                    unmatched_after,
                } => {
                    self.diff_mode_dialog =
                        Some(DiffModeDialog::new(*unmatched_before, *unmatched_after));
                    self.screen = AppScreen::SelectDiffMode;
                }
                Action::SearchSubmitted(query) => self.handle_search_submitted(query.clone()),
                Action::DiffModeSelected(mode) => {
                    if let Some(tx) = self
                        .pending_diff_mode_tx
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .take()
                    {
                        let _ = tx.send(*mode);
                    }
                    self.diff_mode_dialog = None;
                    self.screen = AppScreen::Diffing;
                }
                _ => {}
            }

            self.diff_viewer.update(action.clone())?;
            if let Some(dialog) = self.file_dialog.as_mut() {
                dialog.update(action)?;
            }
        }
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
            self.action_tx.send(Action::StartDiff(before, after))?;
        }
        Ok(())
    }

    fn handle_dialog_cancelled(&mut self) {
        self.file_dialog = None;
        self.theme_dialog = None;
        self.help_modal = None;
        self.search_modal = None;
        self.dialog_target = None;
        self.screen = AppScreen::Viewer;
    }

    /// Apply a search query from the search modal: jump the focused panel's cursor to the nearest
    /// match and highlight every match, then return to the normal viewer screen.
    fn handle_search_submitted(&mut self, query: String) {
        self.diff_viewer.search(&query);
        self.search_modal = None;
        self.screen = AppScreen::Viewer;
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
    /// render loop, then report the result back as an action.
    ///
    /// The diff pipeline isn't guaranteed panic-free for arbitrary/unsupported input (e.g. it
    /// assumes a parsed AST further down the call chain), and a panic on a `spawn_blocking`
    /// thread would otherwise just vanish, leaving the UI stuck on "Diffing…" forever. Catching
    /// it here turns that into a reported `Action::DiffFailed` instead.
    fn start_diff(&self, before: PathBuf, after: PathBuf) {
        let tx = self.action_tx.clone();
        let pending_diff_mode_tx = self.pending_diff_mode_tx.clone();
        tokio::task::spawn_blocking(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                compute_diff_interactive(&before, &after, &tx, &pending_diff_mode_tx)
            }));
            let action = match result {
                Ok(Ok(data)) => Action::DiffReady(data),
                Ok(Err(err)) => Action::DiffFailed(err.to_string()),
                Err(panic) => Action::DiffFailed(format!(
                    "internal error while diffing: {}",
                    panic_message(&panic)
                )),
            };
            // The receiver only goes away when the app is shutting down.
            let _ = tx.send(action);
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
                AppScreen::Diffing => {
                    frame.render_widget(diffing_status_paragraph(), area);
                    Ok(())
                }
                AppScreen::SelectDiffMode => self.draw_diff_mode_dialog(frame, area),
                AppScreen::Help => self.draw_help_modal(frame, area),
                AppScreen::Search => self.draw_search_modal(frame, area),
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
        } else if let Some((index, total)) = self.diff_viewer.focused_change_count_and_index() {
            left_parts.push(format!("change {index}/{total}"));
        }
        if let Some(mode) = self.diff_mode {
            left_parts.push(format_diff_mode(mode));
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

    /// Draw the Fast/Exact prompt as a popup over the "Diffing…" status behind it.
    fn draw_diff_mode_dialog(&mut self, frame: &mut ratatui::Frame, area: Rect) -> Result<()> {
        frame.render_widget(diffing_status_paragraph(), area);
        let Some(dialog) = self.diff_mode_dialog.as_mut() else {
            return Ok(());
        };
        let popup = dialog.popup_area(area);
        frame.render_widget(Clear, popup);
        dialog.draw(frame, popup)
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
const FOOTER_HINTS: &str = "?:help  o:open  n/p:next/prev  /:search  Tab:switch  q:quit";

/// Formats a `ChangeCounts` as a compact `+12 -4 ~2` summary for the footer - omits any category
/// that's zero, and returns an empty string if every category is (e.g. a `NoChanges` diff, already
/// covered by the status bar above).
fn format_change_counts(counts: ChangeCounts) -> String {
    let mut parts = Vec::with_capacity(3);
    if counts.insertions > 0 {
        parts.push(format!("+{}", counts.insertions));
    }
    if counts.deletions > 0 {
        parts.push(format!("-{}", counts.deletions));
    }
    if counts.updates > 0 {
        parts.push(format!("~{}", counts.updates));
    }
    parts.join(" ")
}

/// Formats the currently loaded diff's `DiffMode` as a compact footer label - `[fast]`/`[exact]`,
/// so a user can't forget which one they're looking at once the initial prompt (`draw_diff_mode_
/// dialog`, only shown when `PendingDiff::looks_expensive()` trips) is long gone.
fn format_diff_mode(mode: DiffMode) -> String {
    match mode {
        DiffMode::Fast => "[fast]".to_string(),
        DiffMode::Exact => "[exact]".to_string(),
    }
}

/// The centered "Diffing…" status shown while a background diff computation is in flight -
/// shared by `render`'s `AppScreen::Diffing` arm and `draw_diff_mode_dialog` (the Fast/Exact
/// prompt's own background), which previously each built the identical `Paragraph` themselves.
fn diffing_status_paragraph() -> Paragraph<'static> {
    Paragraph::new("Diffing\u{2026}").alignment(Alignment::Center)
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
fn parse_before_after(before: &Path, after: &Path) -> Result<(Code, Code)> {
    let mut before_code = Code::from_file(before)?;
    let mut after_code = Code::from_file(after)?;

    // git's `difftool`/`GIT_EXTERNAL_DIFF` integration represents an added or deleted file by
    // handing us `/dev/null` for the missing side (see README's "Git integration" section).
    // `/dev/null` reads back as empty content with no extension, so `Code::from_file` can't
    // detect a language for it and leaves `ast` unset - re-parse that side as empty content in
    // the *other* side's language instead of failing outright, so the diff shows a normal
    // whole-file insert/delete rather than an "unrecognized file type" error. This is the only
    // situation where the two sides may legitimately disagree on detected language, so it's safe
    // to only kick in when the empty side's own language genuinely couldn't be determined.
    let before_language = before_code.metadata.language;
    let after_language = after_code.metadata.language;
    substitute_missing_language(&mut before_code, after_language, before);
    substitute_missing_language(&mut after_code, before_language, after);

    if before_code.ast.is_none() {
        anyhow::bail!(
            "unsupported or unrecognized file type: {}",
            before.display()
        );
    }
    if after_code.ast.is_none() {
        anyhow::bail!("unsupported or unrecognized file type: {}", after.display());
    }

    Ok((before_code, after_code))
}

/// Builds the textual ranges needed to display an already-`finish`ed `Diff`.
fn assemble_diff_session_data(
    before_path: &Path,
    after_path: &Path,
    before_code: &Code,
    after_code: &Code,
    diff: &Diff,
    mode: DiffMode,
) -> Result<DiffSessionData> {
    let node_cache = NodeCache::build(before_code, after_code);
    let ast = diff.ast.as_ref().context("diff produced no AST mapping")?;
    let text_diff = TextDiff::from(before_code, after_code, ast, &node_cache);
    // Computed here, not later from DiffSessionData's own fields: needs AST-level node-kind
    // access (is_comment_only_diff), which is gone by the time DiffSessionData exists - see that
    // field's own doc comment.
    let comment_only = is_comment_only_diff(before_code, after_code, ast, &node_cache);

    Ok(DiffSessionData {
        before_path: before_path.to_path_buf(),
        after_path: after_path.to_path_buf(),
        before_contents: before_code.contents.clone(),
        after_contents: after_code.contents.clone(),
        before_ranges: text_diff.all(0),
        after_ranges: text_diff.all(1),
        comment_only,
        mode,
    })
}

/// Parse, diff and compute the textual ranges needed to display the result, using `mode`
/// unconditionally - no prompting, for callers with no interactive UI to ask (headless mode, and
/// any caller that just wants a diff). Returns whether `DiffMode::Fast`'s guard silently
/// substituted the cheaper fallback for phase 6 (always `false` under `DiffMode::Exact`), so
/// headless mode can warn about it.
///
/// `pub(crate)` rather than private: `tui::headless` calls this directly too, since it's the same
/// terminal-independent diff computation either way - only what happens to the result (draw it
/// interactively vs. print it as text) differs between the two modes.
pub(crate) fn compute_diff(
    before: &Path,
    after: &Path,
    mode: DiffMode,
) -> Result<(DiffSessionData, bool)> {
    let (before_code, after_code) = parse_before_after(before, after)?;
    let pending = Diff::pending(&before_code, &after_code);
    let fallback_used = mode == DiffMode::Fast && pending.looks_expensive();
    let diff = pending.finish(mode);
    let data = assemble_diff_session_data(before, after, &before_code, &after_code, &diff, mode)?;
    Ok((data, fallback_used))
}

/// Interactive counterpart to `compute_diff`, only ever called from `start_diff`'s
/// `spawn_blocking` closure: if `PendingDiff::looks_expensive()` trips, sends
/// `Action::DiffModeChoiceNeeded`, stashes the `oneshot::Sender` in `pending_diff_mode_tx`, and
/// blocks *this* worker thread on `Receiver::blocking_recv()` until `App` (running on the main
/// task, a different OS thread) answers via `Action::DiffModeSelected`. Safe to block here
/// specifically because this runs inside `spawn_blocking`, on tokio's dedicated blocking-pool
/// thread, not the async executor that drives rendering/input.
fn compute_diff_interactive(
    before: &Path,
    after: &Path,
    action_tx: &mpsc::UnboundedSender<Action>,
    pending_diff_mode_tx: &Arc<Mutex<Option<oneshot::Sender<DiffMode>>>>,
) -> Result<DiffSessionData> {
    let (before_code, after_code) = parse_before_after(before, after)?;
    let pending = Diff::pending(&before_code, &after_code);

    let mode = if pending.looks_expensive() {
        let (tx, rx) = oneshot::channel::<DiffMode>();
        *pending_diff_mode_tx
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(tx);
        let (unmatched_before, unmatched_after) = pending.unmatched_counts();
        let _ = action_tx.send(Action::DiffModeChoiceNeeded {
            unmatched_before,
            unmatched_after,
        });
        // `App` is shutting down (receiver dropped) => just proceed with the default.
        rx.blocking_recv().unwrap_or_default()
    } else {
        DiffMode::Fast
    };

    let diff = pending.finish(mode);
    assemble_diff_session_data(before, after, &before_code, &after_code, &diff, mode)
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
            Action::StartDiff(before.path().to_path_buf(), after.path().to_path_buf())
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

    /// The `/dev/null` fallback only kicks in for a genuinely empty, language-less side - it must
    /// not paper over the case where neither side has a recognizable language at all.
    #[test]
    fn compute_diff_still_fails_when_neither_side_has_a_recognizable_language() {
        let before = write_temp_file("hello");
        let after = write_temp_file("world");

        let result = compute_diff(before.path(), after.path(), DiffMode::Fast);
        assert!(result.is_err(), "both sides unrecognized should still bail");
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
        assert!(!esc_should_quit(AppScreen::SelectDiffMode));
        assert!(!esc_should_quit(AppScreen::Help));
        assert!(!esc_should_quit(AppScreen::Search));
    }

    #[test]
    fn esc_should_quit_is_true_with_no_dialog_open() {
        assert!(esc_should_quit(AppScreen::Viewer));
        assert!(esc_should_quit(AppScreen::Diffing));
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

    /// Regression test for the exact mechanism `handle_events`'s `globally_handled` guard exists
    /// to prevent: without it, the `?` keystroke that opens the help modal would *also* reach
    /// `dispatch_event_to_active_screen` in the same event cycle (since `self.screen` is already
    /// `Help` by the time that call happens), and `HelpModal` treats `?` as its own close key too
    /// - so the same keypress that opens it would immediately close it again, meaning it would
    /// never visibly show up at all. `handle_events` itself can't be unit-tested directly (it
    /// owns a real `UI`/terminal, which every other test in this module also avoids), so this
    /// pins the hazard at the one layer that is testable: simulating exactly the state
    /// `handle_events` leaves behind right after opening the modal, and confirming that
    /// re-delivering the same keystroke to it would indeed cancel it.
    #[test]
    fn redelivering_the_opening_keystroke_would_immediately_close_the_help_modal() {
        use crossterm::event::{KeyEvent, KeyModifiers};

        let mut app = App::new(4.0, 60.0).expect("construct App");
        app.help_modal = Some(crate::tui::components::help_modal::HelpModal::new());
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
        app.search_modal = Some(SearchModal::new());
        app.diff_viewer.load_diff(&DiffSessionData {
            before_path: PathBuf::from("before.rs"),
            after_path: PathBuf::from("after.rs"),
            before_contents: "foo\nbar\n".to_string(),
            after_contents: "foo\nbar\n".to_string(),
            before_ranges: Vec::new(),
            after_ranges: Vec::new(),
            comment_only: false,
            mode: DiffMode::Fast,
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
        app.search_modal = Some(SearchModal::new());
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
            }),
            "+12 -4 ~2"
        );
        assert_eq!(
            format_change_counts(ChangeCounts {
                insertions: 3,
                deletions: 0,
                updates: 0,
            }),
            "+3"
        );
        assert_eq!(
            format_change_counts(ChangeCounts {
                insertions: 0,
                deletions: 0,
                updates: 0,
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
    fn format_diff_mode_labels_each_mode() {
        assert_eq!(format_diff_mode(DiffMode::Fast), "[fast]");
        assert_eq!(format_diff_mode(DiffMode::Exact), "[exact]");
    }

    #[test]
    fn draw_viewer_shows_the_active_diff_mode_in_the_footer_when_set() -> Result<()> {
        let mut app = App::new(4.0, 60.0)?;
        app.diff_mode = Some(DiffMode::Exact);

        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend)?;
        terminal.draw(|f| {
            let area = f.size();
            app.draw_viewer(f, area).unwrap();
        })?;

        assert!(
            rendered_text(&terminal).contains("[exact]"),
            "expected the active diff mode to be drawn in the footer"
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
}
