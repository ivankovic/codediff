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
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{Frame, layout::Rect, prelude::Stylize, style::Color, widgets::ListItem};

use super::{Component, move_selection, render_list_dialog};
use crate::diff::DiffMode;
use crate::tui::actions::Action;

/// Popup shown when `PendingDiff::looks_expensive()` trips mid-diff (see
/// `tui::app::compute_diff_interactive`): lets the user choose `DiffMode::Fast` (cheaper, less
/// precise - pre-selected, matching `DiffMode::default()`) or `DiffMode::Exact` (full tree-edit-
/// distance, potentially several seconds - the `rust-completely-unrelated-main-files` fixture
/// measured 14.5s before this feature existed).
pub struct DiffModeDialog {
    options: Vec<DiffMode>,
    selected: usize,
    /// The counts `PendingDiff::unmatched_counts()` reported, for the "why am I being asked this"
    /// context shown in the title. Regression fix (2026-07 code-health pass): `Action::
    /// DiffModeChoiceNeeded`'s own doc comment always promised these counts were "just enough
    /// context to render 'this diff looks big'," but `handle_actions` discarded them with `{ .. }`
    /// and this dialog never had anywhere to put them even if it hadn't.
    unmatched_before: usize,
    unmatched_after: usize,
}

impl DiffModeDialog {
    /// Fast is always first (and pre-selected) - both because it's `DiffMode::default()`, and so
    /// an inattentive Enter press (or Esc, which this dialog treats the same as confirming the
    /// current selection - see `handle_key_event`) can't accidentally trigger the slow path.
    pub fn new(unmatched_before: usize, unmatched_after: usize) -> Self {
        Self {
            options: vec![DiffMode::Fast, DiffMode::Exact],
            selected: 0,
            unmatched_before,
            unmatched_after,
        }
    }

    /// The area the popup itself should occupy, centered within `area` - same shape as
    /// `ThemeDialog::popup_area`.
    pub fn popup_area(&self, area: Rect) -> Rect {
        let width = 60.min(area.width);
        let height = (self.options.len() as u16 + 5).min(area.height);
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        Rect::new(x, y, width, height)
    }

    fn label(mode: DiffMode) -> &'static str {
        match mode {
            DiffMode::Fast => "Fast (approximate fallback - this diff is unusually large)",
            DiffMode::Exact => "Exact (full analysis - may take several seconds)",
        }
    }
}

impl Default for DiffModeDialog {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

impl Component for DiffModeDialog {
    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        match key.code {
            KeyCode::Up => {
                move_selection(&mut self.selected, -1, self.options.len());
                Ok(Some(Action::Render))
            }
            KeyCode::Down => {
                move_selection(&mut self.selected, 1, self.options.len());
                Ok(Some(Action::Render))
            }
            // Esc resolves to the current selection rather than cancelling - there's no
            // "cancel" to fall back to once a diff is already in flight waiting on this answer.
            KeyCode::Enter | KeyCode::Esc => {
                Ok(Some(Action::DiffModeSelected(self.options[self.selected])))
            }
            _ => Ok(None),
        }
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let items: Vec<ListItem> = self
            .options
            .iter()
            .map(|&mode| ListItem::new(Self::label(mode)))
            .collect();

        let title = format!(
            " This diff is unusually large ({} before-side / {} after-side nodes still unmatched) ",
            self.unmatched_before, self.unmatched_after
        );
        render_list_dialog(
            frame,
            area,
            title.bold().fg(Color::Cyan).into(),
            items,
            self.selected,
            " Enter: select (default: Fast) ",
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

    #[test]
    fn new_preselects_fast() {
        let dialog = DiffModeDialog::new(3, 5);
        assert_eq!(dialog.options[dialog.selected], DiffMode::Fast);
    }

    #[test]
    fn enter_selects_the_highlighted_mode() {
        let mut dialog = DiffModeDialog::new(3, 5);
        dialog.handle_key_event(key(KeyCode::Down)).unwrap();
        let action = dialog.handle_key_event(key(KeyCode::Enter)).unwrap();
        assert_eq!(action, Some(Action::DiffModeSelected(DiffMode::Exact)));
    }

    #[test]
    fn esc_confirms_the_current_selection_instead_of_cancelling() {
        let mut dialog = DiffModeDialog::new(3, 5);
        let action = dialog.handle_key_event(key(KeyCode::Esc)).unwrap();
        assert_eq!(action, Some(Action::DiffModeSelected(DiffMode::Fast)));
    }

    #[test]
    fn down_does_not_run_past_the_last_option() {
        let mut dialog = DiffModeDialog::new(3, 5);
        for _ in 0..10 {
            dialog.handle_key_event(key(KeyCode::Down)).unwrap();
        }
        assert_eq!(dialog.selected, dialog.options.len() - 1);
    }

    #[test]
    fn up_does_not_run_before_the_first_option() {
        let mut dialog = DiffModeDialog::new(3, 5);
        let action = dialog.handle_key_event(key(KeyCode::Up)).unwrap();
        assert_eq!(action, Some(Action::Render));
        assert_eq!(dialog.selected, 0);
    }

    /// Regression test: `Action::DiffModeChoiceNeeded`'s own doc comment always promised
    /// `unmatched_before`/`unmatched_after` were "just enough context to render 'this diff looks
    /// big'," but `handle_actions` discarded them with `{ .. }` and this dialog had nowhere to
    /// put them even if it hadn't - the prompt never actually showed why it was asking.
    #[test]
    fn draw_shows_the_unmatched_counts() {
        use ratatui::{Terminal, backend::TestBackend};

        let area = ratatui::layout::Rect::new(0, 0, 80, 20);
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut dialog = DiffModeDialog::new(729, 7309);

        terminal.draw(|f| dialog.draw(f, area).unwrap()).unwrap();

        let buffer = terminal.backend().buffer().clone();
        let rendered: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(rendered.contains("729"), "should show the before-side count: {rendered}");
        assert!(rendered.contains("7309"), "should show the after-side count: {rendered}");
    }
}
