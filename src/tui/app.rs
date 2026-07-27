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
    style::{Color, Style},
    widgets::{Clear, Paragraph},
};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error};

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::code::{Code, Language};
use crate::diff::{Diff, DiffMode, NodeCache, text::TextDiff};
use crate::tui::actions::{Action, DiffSessionData};
use crate::tui::components::{
    Component,
    diff_mode_dialog::DiffModeDialog,
    diff_viewer::{DiffViewer, Panel},
    file_dialog::FileDialog,
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
}

/// The codediff application. The state, but not the state machine or the UI, of the TUI.
pub struct App {
    tick_rate: f64,
    frame_rate: f64,

    diff_viewer: DiffViewer,
    file_dialog: Option<FileDialog>,
    theme_dialog: Option<ThemeDialog>,
    diff_mode_dialog: Option<DiffModeDialog>,

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
            action_tx,
            action_rx,
            pending_diff_mode_tx: Arc::new(Mutex::new(None)),
            screen: AppScreen::default(),
            dialog_target: None,
            current_theme: OverlayTheme::default(),
            before_path: None,
            after_path: None,
            last_error: None,
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

        // Global key handling that doesn't depend on which component is focused.
        if let Event::Key(key) = &event {
            match key.code {
                KeyCode::Char('q') => {
                    action_tx.send(Action::Quit)?;
                }
                KeyCode::Esc => {
                    // Inside a file dialog, Esc cancels the dialog (handled by the dialog
                    // itself via Action::DialogCancelled) rather than quitting the app. Inside
                    // the diff-mode prompt, Esc resolves to the dialog's current (default Fast)
                    // selection instead - "cancel" has no obvious meaning once a diff is already
                    // in flight, and the background thread is blocked waiting for *some* answer.
                    if self.screen != AppScreen::SelectFile && self.screen != AppScreen::SelectDiffMode {
                        action_tx.send(Action::Quit)?;
                    }
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
                }
                KeyCode::Char('c') if self.screen == AppScreen::Viewer => {
                    self.theme_dialog = Some(ThemeDialog::new(self.current_theme));
                    self.screen = AppScreen::SelectTheme;
                    action_tx.send(Action::Render)?;
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

        if let Some(action) = self.dispatch_event_to_active_screen(event)? {
            action_tx.send(action)?;
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
                    self.start_diff(before.clone(), after.clone());
                }
                Action::DiffReady(_) => {
                    self.screen = AppScreen::Viewer;
                    self.last_error = None;
                    self.file_dialog = None;
                }
                Action::DiffFailed(message) => {
                    error!("diff failed: {message}");
                    self.last_error = Some(message.clone());
                    self.screen = AppScreen::Viewer;
                    self.file_dialog = None;
                }
                Action::ThemeSelected(selected_theme) => {
                    self.apply_theme_selection(*selected_theme)
                }
                Action::DiffModeChoiceNeeded { .. } => {
                    self.diff_mode_dialog = Some(DiffModeDialog::new());
                    self.screen = AppScreen::SelectDiffMode;
                }
                Action::DiffModeSelected(mode) => {
                    if let Some(tx) = self.pending_diff_mode_tx.lock().unwrap().take() {
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
        self.dialog_target = None;
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
                    let status = Paragraph::new("Diffing\u{2026}").alignment(Alignment::Center);
                    frame.render_widget(status, area);
                    Ok(())
                }
                AppScreen::SelectDiffMode => self.draw_diff_mode_dialog(frame, area),
            };
            if let Err(err) = result {
                let _ = self
                    .action_tx
                    .send(Action::Error(format!("Failed to draw: {:?}", err)));
            }
        })?;
        Ok(())
    }

    /// Draw the Before/After panels, plus a one-line error banner under them if the most recent
    /// file pick failed to diff (e.g. an unsupported file type).
    fn draw_viewer(&mut self, frame: &mut ratatui::Frame, area: Rect) -> Result<()> {
        let Some(message) = &self.last_error else {
            return self.diff_viewer.draw(frame, area);
        };
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);
        self.diff_viewer.draw(frame, layout[0])?;
        frame.render_widget(
            Paragraph::new(message.as_str()).style(Style::new().fg(Color::Red)),
            layout[1],
        );
        Ok(())
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
        let status = Paragraph::new("Diffing\u{2026}").alignment(Alignment::Center);
        frame.render_widget(status, area);
        let Some(dialog) = self.diff_mode_dialog.as_mut() else {
            return Ok(());
        };
        let popup = dialog.popup_area(area);
        frame.render_widget(Clear, popup);
        dialog.draw(frame, popup)
    }
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
) -> Result<DiffSessionData> {
    let node_cache = NodeCache::build(before_code, after_code);
    let ast = diff.ast.as_ref().context("diff produced no AST mapping")?;
    let text_diff = TextDiff::from(before_code, after_code, ast, &node_cache);

    Ok(DiffSessionData {
        before_path: before_path.to_path_buf(),
        after_path: after_path.to_path_buf(),
        before_contents: before_code.contents.clone(),
        after_contents: after_code.contents.clone(),
        before_ranges: text_diff.all(0),
        after_ranges: text_diff.all(1),
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
pub(crate) fn compute_diff(before: &Path, after: &Path, mode: DiffMode) -> Result<(DiffSessionData, bool)> {
    let (before_code, after_code) = parse_before_after(before, after)?;
    let pending = Diff::pending(&before_code, &after_code);
    let fallback_used = mode == DiffMode::Fast && pending.looks_expensive();
    let diff = pending.finish(mode);
    let data = assemble_diff_session_data(before, after, &before_code, &after_code, &diff)?;
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
        *pending_diff_mode_tx.lock().unwrap() = Some(tx);
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
    assemble_diff_session_data(before, after, &before_code, &after_code, &diff)
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

        let (data, _fallback_used) = compute_diff(Path::new("/dev/null"), after.path(), DiffMode::Fast)?;

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

        let (data, _fallback_used) = compute_diff(before.path(), Path::new("/dev/null"), DiffMode::Fast)?;

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
}
