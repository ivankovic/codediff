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
 *  You should have received a copy of the GNU Affero General License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */
use std::cmp::Ordering;
use std::path::PathBuf;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{Frame, layout::Rect, prelude::Stylize, style::Color, text::Line, widgets::ListItem};
use tokio::sync::mpsc::UnboundedSender;

use super::{Component, move_selection, render_list_dialog};
use crate::tui::actions::{Action, DirEntryInfo};

/// A minimal navigable directory/file picker.
///
/// Directory listings are fetched asynchronously via `tokio::fs::read_dir` so opening a (large)
/// directory never blocks the render loop; the result comes back through the normal action
/// channel as [`Action::DirectoryListed`].
pub struct FileDialog {
    command_tx: Option<UnboundedSender<Action>>,
    /// Title shown in the dialog border, e.g. "Select before file".
    title: String,
    /// The directory currently being displayed.
    current_dir: PathBuf,
    /// The directory a listing has been requested for but not yet received.
    pending_dir: Option<PathBuf>,
    entries: Vec<DirEntryInfo>,
    selected: usize,
}

impl FileDialog {
    /// Create a new FileDialog rooted at the current working directory.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            command_tx: None,
            title: title.into(),
            current_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            pending_dir: None,
            entries: Vec::new(),
            selected: 0,
        }
    }

    /// Kick off an async listing of `dir`; the result arrives later as `Action::DirectoryListed`.
    fn request_listing(&mut self, dir: PathBuf) {
        self.pending_dir = Some(dir.clone());
        let Some(tx) = self.command_tx.clone() else {
            return;
        };
        tokio::spawn(async move {
            let mut entries = Vec::new();
            if let Ok(mut read_dir) = tokio::fs::read_dir(&dir).await {
                while let Ok(Some(entry)) = read_dir.next_entry().await {
                    let is_dir = entry
                        .file_type()
                        .await
                        .map(|file_type| file_type.is_dir())
                        .unwrap_or(false);
                    entries.push(DirEntryInfo {
                        name: entry.file_name().to_string_lossy().into_owned(),
                        path: entry.path(),
                        is_dir,
                    });
                }
            }
            entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            });
            if let Some(parent) = dir.parent() {
                entries.insert(
                    0,
                    DirEntryInfo {
                        name: "..".to_string(),
                        path: parent.to_path_buf(),
                        is_dir: true,
                    },
                );
            }
            // The receiving end only goes away when the app is shutting down.
            let _ = tx.send(Action::DirectoryListed(dir, entries));
        });
    }
}

impl Component for FileDialog {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.command_tx = Some(tx);
        Ok(())
    }

    fn init(&mut self, _area: Rect) -> Result<()> {
        let dir = self.current_dir.clone();
        self.request_listing(dir);
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        match key.code {
            KeyCode::Up => {
                move_selection(&mut self.selected, -1, self.entries.len());
                Ok(Some(Action::Render))
            }
            KeyCode::Down => {
                move_selection(&mut self.selected, 1, self.entries.len());
                Ok(Some(Action::Render))
            }
            KeyCode::Enter => match self.entries.get(self.selected).cloned() {
                Some(entry) if entry.is_dir => {
                    self.request_listing(entry.path);
                    Ok(Some(Action::Render))
                }
                Some(entry) => Ok(Some(Action::FileSelected(entry.path))),
                None => Ok(None),
            },
            KeyCode::Backspace => {
                if let Some(parent) = self.current_dir.parent() {
                    self.request_listing(parent.to_path_buf());
                }
                Ok(Some(Action::Render))
            }
            KeyCode::Esc => Ok(Some(Action::DialogCancelled)),
            _ => Ok(None),
        }
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        if let Action::DirectoryListed(dir, entries) = action
            && self.pending_dir.as_ref() == Some(&dir)
        {
            self.current_dir = dir;
            self.entries = entries;
            self.selected = 0;
            self.pending_dir = None;
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let title = Line::from(vec![
            self.title.clone().bold().fg(Color::Cyan),
            " - ".into(),
            self.current_dir.display().to_string().into(),
        ]);

        let items: Vec<ListItem> = self
            .entries
            .iter()
            .map(|entry| {
                let label = if entry.is_dir {
                    format!("{}/", entry.name)
                } else {
                    entry.name.clone()
                };
                ListItem::new(label)
            })
            .collect();

        render_list_dialog(
            frame,
            area,
            title,
            items,
            self.selected,
            " Enter: select/open dir | Backspace: parent dir | Esc: cancel ",
        );

        Ok(())
    }
}
