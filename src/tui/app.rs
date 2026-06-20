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
    layout::{Alignment, Rect},
    widgets::Paragraph,
};
use tokio::sync::mpsc;
use tracing::{debug, error};

use std::path::{Path, PathBuf};

use crate::code::Code;
use crate::diff::{Diff, NodeCache, text::TextDiff};
use crate::tui::actions::{Action, DiffSessionData};
use crate::tui::components::{
    Component, diff_viewer::DiffViewer, file_dialog::FileDialog, overview::Overview,
};
use crate::tui::events::Event;
use crate::tui::ui::UI;

/// Which top-level screen is currently shown.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum AppScreen {
    #[default]
    Overview,
    SelectBeforeFile,
    SelectAfterFile,
    Diffing,
    ShowDiff,
}

/// The codediff application. The state, but not the state machine or the UI, of the TUI.
pub struct App {
    tick_rate: f64,
    frame_rate: f64,

    overview: Overview,
    diff_viewer: DiffViewer,
    file_dialog: Option<FileDialog>,

    action_tx: mpsc::UnboundedSender<Action>,
    action_rx: mpsc::UnboundedReceiver<Action>,

    screen: AppScreen,
    /// The before-file path, set once the first file dialog confirms a selection.
    before_path: Option<PathBuf>,
    /// Whether a diff has ever successfully completed, so cancelling a re-diff returns to
    /// `ShowDiff` instead of `Overview`.
    has_diff: bool,

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
            overview: Overview::new(),
            diff_viewer: DiffViewer::new(),
            file_dialog: None,
            action_tx,
            action_rx,
            screen: AppScreen::default(),
            before_path: None,
            has_diff: false,
            should_exit: false,
            should_suspend: false,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut ui = UI::new()?
            .tick_rate(self.tick_rate)
            .frame_rate(self.frame_rate);
        ui.enter()?;

        self.overview
            .register_action_handler(self.action_tx.clone())?;
        self.diff_viewer
            .register_action_handler(self.action_tx.clone())?;
        self.overview.init(ui.size()?)?;
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
                ui.stop()?;
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
                    // itself via Action::DialogCancelled) rather than quitting the app.
                    if !matches!(
                        self.screen,
                        AppScreen::SelectBeforeFile | AppScreen::SelectAfterFile
                    ) {
                        action_tx.send(Action::Quit)?;
                    }
                }
                KeyCode::Char('o') if self.screen == AppScreen::ShowDiff => {
                    self.screen = AppScreen::Overview;
                    action_tx.send(Action::Render)?;
                }
                KeyCode::Char('d')
                    if matches!(self.screen, AppScreen::Overview | AppScreen::ShowDiff) =>
                {
                    self.before_path = None;
                    self.screen = AppScreen::SelectBeforeFile;
                    self.open_file_dialog("Select the BEFORE file", ui)?;
                    action_tx.send(Action::Render)?;
                }
                _ => {}
            }
        }

        match event {
            Event::Quit => action_tx.send(Action::Quit)?,
            Event::Tick => action_tx.send(Action::Tick)?,
            Event::Render => action_tx.send(Action::Render)?,
            Event::Resize(x, y) => action_tx.send(Action::Resize(x, y))?,
            Event::Key(_) => {}
            _ => {}
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
            AppScreen::Overview => self.overview.handle_events(Some(event)),
            AppScreen::SelectBeforeFile | AppScreen::SelectAfterFile => {
                match self.file_dialog.as_mut() {
                    Some(dialog) => dialog.handle_events(Some(event)),
                    None => Ok(None),
                }
            }
            AppScreen::Diffing => Ok(None),
            AppScreen::ShowDiff => self.diff_viewer.handle_events(Some(event)),
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
                Action::FileSelected(path) => self.handle_file_selected(path.clone(), ui)?,
                Action::DialogCancelled => self.handle_dialog_cancelled(),
                Action::StartDiff(before, after) => {
                    self.screen = AppScreen::Diffing;
                    self.start_diff(before.clone(), after.clone());
                }
                Action::DiffReady(_) => {
                    self.screen = AppScreen::ShowDiff;
                    self.has_diff = true;
                    self.file_dialog = None;
                }
                Action::DiffFailed(message) => {
                    error!("diff failed: {message}");
                    self.screen = if self.has_diff {
                        AppScreen::ShowDiff
                    } else {
                        AppScreen::Overview
                    };
                    self.file_dialog = None;
                }
                _ => {}
            }

            self.overview.update(action.clone())?;
            self.diff_viewer.update(action.clone())?;
            if let Some(dialog) = self.file_dialog.as_mut() {
                dialog.update(action)?;
            }
        }
        Ok(())
    }

    /// A file was confirmed in the dialog: advance from before->after->kick off the diff.
    fn handle_file_selected(&mut self, path: PathBuf, ui: &mut UI) -> Result<()> {
        match self.screen {
            AppScreen::SelectBeforeFile => {
                self.before_path = Some(path);
                self.screen = AppScreen::SelectAfterFile;
                self.open_file_dialog("Select the AFTER file", ui)?;
            }
            AppScreen::SelectAfterFile => {
                if let Some(before) = self.before_path.take() {
                    self.file_dialog = None;
                    self.action_tx.send(Action::StartDiff(before, path))?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_dialog_cancelled(&mut self) {
        self.file_dialog = None;
        self.before_path = None;
        self.screen = if self.has_diff {
            AppScreen::ShowDiff
        } else {
            AppScreen::Overview
        };
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
        tokio::task::spawn_blocking(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                compute_diff(&before, &after)
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
                AppScreen::Overview => self.overview.draw(frame, area),
                AppScreen::SelectBeforeFile | AppScreen::SelectAfterFile => {
                    match self.file_dialog.as_mut() {
                        Some(dialog) => dialog.draw(frame, area),
                        None => Ok(()),
                    }
                }
                AppScreen::Diffing => {
                    let status = Paragraph::new("Diffing\u{2026}").alignment(Alignment::Center);
                    frame.render_widget(status, area);
                    Ok(())
                }
                AppScreen::ShowDiff => self.diff_viewer.draw(frame, area),
            };
            if let Err(err) = result {
                let _ = self
                    .action_tx
                    .send(Action::Error(format!("Failed to draw: {:?}", err)));
            }
        })?;
        Ok(())
    }
}

/// Parse both files, diff them, and compute the textual ranges needed to display the result.
fn compute_diff(before: &Path, after: &Path) -> Result<DiffSessionData> {
    let before_code = Code::from_file(before)?;
    let after_code = Code::from_file(after)?;
    if before_code.ast.is_none() {
        anyhow::bail!(
            "unsupported or unrecognized file type: {}",
            before.display()
        );
    }
    if after_code.ast.is_none() {
        anyhow::bail!("unsupported or unrecognized file type: {}", after.display());
    }
    let node_cache = NodeCache::build(&before_code, &after_code);
    let diff = Diff::from_code(&before_code, &after_code);
    let ast = diff.ast.as_ref().context("diff produced no AST mapping")?;
    let text_diff = TextDiff::from(&before_code, &after_code, ast, &node_cache);

    Ok(DiffSessionData {
        before_path: before.to_path_buf(),
        after_path: after.to_path_buf(),
        before_contents: before_code.contents,
        after_contents: after_code.contents,
        before_ranges: text_diff.all(0),
        after_ranges: text_diff.all(1),
    })
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
