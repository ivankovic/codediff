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
//! The `G` picker: the repository's unstaged, staged and recently committed changes as one list,
//! commits folding open to their files. Picking a file hands `App` a `ReviewTarget`, which it
//! materializes (`review::Workspace`) and opens like any other pair.

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{Frame, layout::Rect, text::Line, widgets::ListItem};

use super::{Component, render_list_dialog};
use crate::review::{ChangeSet, ChangedFile, Review, ReviewTarget};
use crate::tui::actions::Action;

const HINT: &str =
    " ↑/↓ or j/k: move | Enter: open file / fold commit | ←/→: fold/unfold | Esc or G: close ";

/// One line of the picker. Only files and commits can be selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Header(String),
    /// A file, with its index within its set - what `]`/`[` step from.
    File {
        target: ReviewTarget,
        index: usize,
    },
    Commit(usize),
    Note(&'static str),
}

pub struct ReviewDialog {
    review: Review,
    expanded: Vec<bool>,
    selected: usize,
}

impl ReviewDialog {
    /// Starts with the newest commit unfolded, since "what did the last commit touch" is the
    /// question this answers most often; the selection sits on the first selectable row.
    pub fn new(review: Review) -> Self {
        let mut expanded = vec![false; review.commits.len()];
        if let Some(first) = expanded.first_mut() {
            *first = true;
        }
        let mut dialog = Self {
            review,
            expanded,
            selected: 0,
        };
        dialog.selected = dialog.rows().iter().position(Self::selectable).unwrap_or(0);
        dialog
    }

    pub fn review(&self) -> &Review {
        &self.review
    }

    fn file_rows(rows: &mut Vec<Row>, set: ChangeSet, files: &[ChangedFile]) {
        for (index, file) in files.iter().enumerate() {
            rows.push(Row::File {
                target: ReviewTarget {
                    set: set.clone(),
                    file: file.clone(),
                },
                index,
            });
        }
    }

    pub fn rows(&self) -> Vec<Row> {
        let review = &self.review;
        let mut rows = Vec::new();
        rows.push(Row::Header(format!(
            "Working tree ({})",
            review.working_tree.len()
        )));
        if review.working_tree.is_empty() {
            rows.push(Row::Note("  (clean)"));
        }
        Self::file_rows(&mut rows, ChangeSet::WorkingTree, &review.working_tree);
        rows.push(Row::Header(format!("Staged ({})", review.staged.len())));
        if review.staged.is_empty() {
            rows.push(Row::Note("  (nothing staged)"));
        }
        Self::file_rows(&mut rows, ChangeSet::Staged, &review.staged);
        rows.push(Row::Header(format!(
            "Recent commits ({})",
            review.commits.len()
        )));
        if review.commits.is_empty() {
            rows.push(Row::Note("  (no commits yet)"));
        }
        for (index, commit) in review.commits.iter().enumerate() {
            rows.push(Row::Commit(index));
            if self.expanded[index] {
                Self::file_rows(
                    &mut rows,
                    ChangeSet::Commit {
                        hash: commit.hash.clone(),
                    },
                    &commit.files,
                );
            }
        }
        rows
    }

    fn selectable(row: &Row) -> bool {
        matches!(row, Row::File { .. } | Row::Commit(_))
    }

    /// Moves the selection to the next selectable row in `direction`, staying put at the ends.
    fn move_selection(&mut self, direction: i32) {
        let rows = self.rows();
        let mut index = self.selected;
        loop {
            let next = if direction < 0 {
                index.checked_sub(1)
            } else {
                Some(index + 1).filter(|&i| i < rows.len())
            };
            let Some(next) = next else {
                return;
            };
            index = next;
            if Self::selectable(&rows[index]) {
                self.selected = index;
                return;
            }
        }
    }

    fn set_expanded(&mut self, commit: usize, expanded: bool) {
        if let Some(flag) = self.expanded.get_mut(commit) {
            *flag = expanded;
        }
    }

    pub fn selected_row(&self) -> Option<Row> {
        self.rows().get(self.selected).cloned()
    }

    fn label(&self, row: &Row) -> String {
        match row {
            Row::Header(text) => text.clone(),
            Row::Note(text) => text.to_string(),
            Row::File { target, .. } => format!("    {}", target.file.label()),
            Row::Commit(index) => {
                let marker = if self.expanded[*index] { "▾" } else { "▸" };
                format!("  {marker} {}", self.review.commits[*index].label())
            }
        }
    }

    pub fn popup_area(&self, area: Rect) -> Rect {
        let width = (area.width * 9 / 10).min(area.width);
        let height = (area.height * 9 / 10).min(area.height);
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        Rect::new(x, y, width, height)
    }
}

impl Component for ReviewDialog {
    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_selection(-1);
                Ok(Some(Action::Render))
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_selection(1);
                Ok(Some(Action::Render))
            }
            KeyCode::Enter | KeyCode::Char(' ') => match self.selected_row() {
                Some(Row::File { target, index }) => {
                    Ok(Some(Action::ReviewFileSelected { target, index }))
                }
                Some(Row::Commit(commit)) => {
                    let expanded = self.expanded[commit];
                    self.set_expanded(commit, !expanded);
                    Ok(Some(Action::Render))
                }
                _ => Ok(None),
            },
            KeyCode::Right | KeyCode::Left => {
                if let Some(Row::Commit(commit)) = self.selected_row() {
                    self.set_expanded(commit, key.code == KeyCode::Right);
                }
                Ok(Some(Action::Render))
            }
            KeyCode::Esc | KeyCode::Char('G') => Ok(Some(Action::DialogCancelled)),
            _ => Ok(None),
        }
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let items: Vec<ListItem> = self
            .rows()
            .iter()
            .map(|row| ListItem::new(self.label(row)))
            .collect();
        render_list_dialog(
            frame,
            area,
            Line::from(format!(" Git review - {} ", self.review.root.display())),
            items,
            self.selected,
            HINT,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::{Commit, FileStatus};
    use crossterm::event::KeyModifiers;
    use std::path::PathBuf;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn file(status: FileStatus, path: &str) -> ChangedFile {
        ChangedFile {
            status,
            path: path.to_string(),
            old_path: None,
        }
    }

    fn sample_review() -> Review {
        Review {
            root: PathBuf::from("/repo"),
            working_tree: vec![file(FileStatus::Modified, "a.rs")],
            staged: Vec::new(),
            commits: vec![
                Commit {
                    hash: "aaaa".into(),
                    short: "aaaa".into(),
                    author: "Ada".into(),
                    date: "2026-09-10".into(),
                    subject: "newest".into(),
                    files: vec![
                        file(FileStatus::Added, "n.rs"),
                        file(FileStatus::Deleted, "o.rs"),
                    ],
                },
                Commit {
                    hash: "bbbb".into(),
                    short: "bbbb".into(),
                    author: "Bob".into(),
                    date: "2026-09-09".into(),
                    subject: "older".into(),
                    files: vec![file(FileStatus::Modified, "p.rs")],
                },
            ],
        }
    }

    #[test]
    fn rows_list_the_three_sections_with_the_newest_commit_unfolded() {
        let dialog = ReviewDialog::new(sample_review());
        let labels: Vec<String> = dialog.rows().iter().map(|row| dialog.label(row)).collect();
        assert_eq!(
            labels,
            vec![
                "Working tree (1)",
                "    M a.rs",
                "Staged (0)",
                "  (nothing staged)",
                "Recent commits (2)",
                "  ▾ aaaa 2026-09-10 newest (Ada)",
                "    A n.rs",
                "    D o.rs",
                "  ▸ bbbb 2026-09-09 older (Bob)",
            ]
        );
        assert_eq!(dialog.selected, 1, "the first file, not the header");
    }

    #[test]
    fn navigation_skips_headers_and_notes_and_stops_at_the_ends() {
        let mut dialog = ReviewDialog::new(sample_review());
        dialog.handle_key_event(key(KeyCode::Up)).unwrap();
        assert_eq!(
            dialog.selected, 1,
            "nothing selectable above the first file"
        );
        dialog.handle_key_event(key(KeyCode::Down)).unwrap();
        assert!(
            matches!(dialog.selected_row(), Some(Row::Commit(0))),
            "straight over the empty Staged section"
        );
        dialog.handle_key_event(key(KeyCode::Char('j'))).unwrap();
        dialog.handle_key_event(key(KeyCode::Char('j'))).unwrap();
        dialog.handle_key_event(key(KeyCode::Char('j'))).unwrap();
        assert!(matches!(dialog.selected_row(), Some(Row::Commit(1))));
        dialog.handle_key_event(key(KeyCode::Down)).unwrap();
        assert!(
            matches!(dialog.selected_row(), Some(Row::Commit(1))),
            "stays at the end"
        );
    }

    #[test]
    fn enter_on_a_file_reports_the_target_and_its_index_within_its_set() {
        let mut dialog = ReviewDialog::new(sample_review());
        for _ in 0..3 {
            dialog.handle_key_event(key(KeyCode::Down)).unwrap();
        }
        let action = dialog.handle_key_event(key(KeyCode::Enter)).unwrap();
        assert_eq!(
            action,
            Some(Action::ReviewFileSelected {
                target: ReviewTarget {
                    set: ChangeSet::Commit {
                        hash: "aaaa".into()
                    },
                    file: file(FileStatus::Deleted, "o.rs"),
                },
                index: 1,
            })
        );
    }

    #[test]
    fn enter_and_arrows_fold_and_unfold_a_commit() {
        let mut dialog = ReviewDialog::new(sample_review());
        dialog.handle_key_event(key(KeyCode::Down)).unwrap();
        assert_eq!(dialog.rows().len(), 9);
        dialog.handle_key_event(key(KeyCode::Enter)).unwrap();
        assert_eq!(dialog.rows().len(), 7, "folded: the two files are gone");
        dialog.handle_key_event(key(KeyCode::Right)).unwrap();
        assert_eq!(dialog.rows().len(), 9);
        dialog.handle_key_event(key(KeyCode::Left)).unwrap();
        assert_eq!(dialog.rows().len(), 7);
        dialog.handle_key_event(key(KeyCode::Down)).unwrap();
        dialog.handle_key_event(key(KeyCode::Char(' '))).unwrap();
        assert_eq!(dialog.rows().len(), 8, "the older commit unfolds too");
    }

    #[test]
    fn esc_and_g_close_the_dialog() {
        let mut dialog = ReviewDialog::new(sample_review());
        assert_eq!(
            dialog.handle_key_event(key(KeyCode::Esc)).unwrap(),
            Some(Action::DialogCancelled)
        );
        assert_eq!(
            dialog.handle_key_event(key(KeyCode::Char('G'))).unwrap(),
            Some(Action::DialogCancelled)
        );
    }

    #[test]
    fn an_empty_repository_shows_its_notes_and_selects_nothing_harmful() {
        let review = Review {
            root: PathBuf::from("/repo"),
            working_tree: Vec::new(),
            staged: Vec::new(),
            commits: Vec::new(),
        };
        let mut dialog = ReviewDialog::new(review);
        assert_eq!(dialog.handle_key_event(key(KeyCode::Enter)).unwrap(), None);
        dialog.handle_key_event(key(KeyCode::Down)).unwrap();
        assert_eq!(dialog.selected, 0);
    }

    #[test]
    fn draws_the_root_and_every_row() {
        use ratatui::{Terminal, backend::TestBackend};
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut dialog = ReviewDialog::new(sample_review());
        terminal
            .draw(|f| {
                let area = f.size();
                dialog.draw(f, dialog.popup_area(area)).unwrap();
            })
            .unwrap();
        let rendered: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(rendered.contains("Git review - /repo"));
        assert!(rendered.contains("M a.rs"));
        assert!(rendered.contains("newest (Ada)"));
    }
}
