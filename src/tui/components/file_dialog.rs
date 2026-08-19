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
    /// Index into `visible_entries()`'s output, not `entries` - the selection moves within
    /// whatever the filter and hidden-file toggle currently show.
    selected: usize,
    /// Type-ahead filter: printable keys narrow the listing to entries whose name contains the
    /// typed text (case-insensitive), Backspace widens it again (and only falls back to "go to
    /// parent directory" once the filter is empty). Cleared on every directory change.
    filter: String,
    /// Whether dotfiles are listed (`Ctrl-h` toggles). Off by default - the common case for
    /// picking a source file is that dotfiles are noise.
    show_hidden: bool,
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
            filter: String::new(),
            show_hidden: false,
        }
    }

    /// The entries the filter and hidden-file toggle currently allow, in listing order. `..` is
    /// always visible: filtering must never take away the way back out of a directory.
    fn visible_entries(&self) -> Vec<&DirEntryInfo> {
        let filter_lower = self.filter.to_lowercase();
        self.entries
            .iter()
            .filter(|entry| {
                if entry.name == ".." {
                    return true;
                }
                if !self.show_hidden && entry.name.starts_with('.') {
                    return false;
                }
                filter_lower.is_empty() || entry.name.to_lowercase().contains(&filter_lower)
            })
            .collect()
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
        use crossterm::event::KeyModifiers;
        let visible_len = self.visible_entries().len();
        match key.code {
            KeyCode::Up => {
                move_selection(&mut self.selected, -1, visible_len);
                Ok(Some(Action::Render))
            }
            KeyCode::Down => {
                move_selection(&mut self.selected, 1, visible_len);
                Ok(Some(Action::Render))
            }
            KeyCode::Enter => {
                let entry = self.visible_entries().get(self.selected).cloned().cloned();
                match entry {
                    Some(entry) if entry.is_dir => {
                        self.request_listing(entry.path);
                        Ok(Some(Action::Render))
                    }
                    Some(entry) => Ok(Some(Action::FileSelected(entry.path))),
                    None => Ok(None),
                }
            }
            // Ctrl-h toggles dotfiles. Checked before the plain-character filter arm below, which
            // would otherwise swallow the 'h'.
            KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.show_hidden = !self.show_hidden;
                self.selected = 0;
                Ok(Some(Action::Render))
            }
            // Type-ahead: printable characters narrow the listing instead of doing nothing.
            KeyCode::Char(c) => {
                self.filter.push(c);
                self.selected = 0;
                Ok(Some(Action::Render))
            }
            // Backspace widens the filter first; only with nothing left to widen does it keep
            // its original meaning of "go to the parent directory".
            KeyCode::Backspace => {
                if self.filter.pop().is_none()
                    && let Some(parent) = self.current_dir.parent()
                {
                    self.request_listing(parent.to_path_buf());
                }
                self.selected = 0;
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
            self.filter.clear();
            self.pending_dir = None;
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let mut title_parts = vec![
            self.title.clone().bold().fg(Color::Cyan),
            " - ".into(),
            self.current_dir.display().to_string().into(),
        ];
        if !self.filter.is_empty() {
            title_parts.push(
                format!("  filter: {}", self.filter)
                    .bold()
                    .fg(Color::Yellow),
            );
        }
        let title = Line::from(title_parts);

        let items: Vec<ListItem> = self
            .visible_entries()
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
            " type: filter | Enter: select/open dir | Backspace: unfilter/parent | Ctrl-h: hidden | Esc: cancel ",
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn entry(name: &str, is_dir: bool) -> DirEntryInfo {
        DirEntryInfo {
            name: name.to_string(),
            path: PathBuf::from(format!("/tmp/{name}")),
            is_dir,
        }
    }

    fn dialog_with_entries(entries: Vec<DirEntryInfo>) -> FileDialog {
        let mut dialog = FileDialog::new("test");
        dialog.entries = entries;
        dialog
    }

    #[test]
    fn typing_narrows_the_listing_and_dotfiles_are_hidden_by_default() {
        let mut dialog = dialog_with_entries(vec![
            entry("..", true),
            entry(".git", true),
            entry("main.rs", false),
            entry("Makefile", false),
        ]);
        let names = |d: &FileDialog| -> Vec<String> {
            d.visible_entries().iter().map(|e| e.name.clone()).collect()
        };

        assert_eq!(
            names(&dialog),
            vec!["..", "main.rs", "Makefile"],
            ".git hidden by default, .. always visible"
        );

        dialog.handle_key_event(key(KeyCode::Char('m'))).unwrap();
        dialog.handle_key_event(key(KeyCode::Char('a'))).unwrap();
        assert_eq!(
            names(&dialog),
            vec!["..", "main.rs", "Makefile"],
            "the filter is case-insensitive and .. survives it"
        );

        dialog.handle_key_event(key(KeyCode::Char('i'))).unwrap();
        assert_eq!(
            names(&dialog),
            vec!["..", "main.rs"],
            "\"mai\" should keep only main.rs"
        );
    }

    #[test]
    fn ctrl_h_toggles_hidden_files() {
        let mut dialog = dialog_with_entries(vec![entry(".git", true), entry("main.rs", false)]);
        assert_eq!(dialog.visible_entries().len(), 1);

        dialog.handle_key_event(ctrl('h')).unwrap();
        assert_eq!(
            dialog.visible_entries().len(),
            2,
            "Ctrl-h should reveal dotfiles"
        );

        dialog.handle_key_event(ctrl('h')).unwrap();
        assert_eq!(dialog.visible_entries().len(), 1);
    }

    #[test]
    fn backspace_widens_the_filter_before_falling_back_to_parent_navigation() {
        let mut dialog = dialog_with_entries(vec![entry("main.rs", false)]);
        dialog.handle_key_event(key(KeyCode::Char('z'))).unwrap();
        assert!(dialog.visible_entries().is_empty(), "no entry contains 'z'");

        dialog.handle_key_event(key(KeyCode::Backspace)).unwrap();
        assert_eq!(
            dialog.visible_entries().len(),
            1,
            "Backspace should first delete the filter character"
        );
        assert!(
            dialog.pending_dir.is_none(),
            "no parent-directory listing should have been requested while a filter was active"
        );
    }

    #[test]
    fn enter_selects_from_the_filtered_view_not_the_raw_listing() {
        let mut dialog = dialog_with_entries(vec![entry("aaa.rs", false), entry("bbb.rs", false)]);
        dialog.handle_key_event(key(KeyCode::Char('b'))).unwrap();

        let action = dialog.handle_key_event(key(KeyCode::Enter)).unwrap();
        assert_eq!(
            action,
            Some(Action::FileSelected(PathBuf::from("/tmp/bbb.rs"))),
            "selection index 0 must resolve within the filtered view"
        );
    }
}
