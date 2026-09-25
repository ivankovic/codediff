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
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{Frame, layout::Rect, text::Line, widgets::ListItem};

use super::{Component, move_selection, render_list_dialog};
use crate::diff::text::RenderOptions;
use crate::tui::actions::Action;

const HINT: &str = "↑/↓ move  Space toggle  1/2 presets  Enter or Esc close";

/// The `M` key's settings panel: one checkbox row per [`RenderOptions`] field, plus two presets.
///
/// Every toggle applies and persists the moment it is pressed, and the diff behind the panel
/// shows it. There is nothing to accept or cancel, so `Enter` and `Esc` both just close; unlike
/// `ThemeDialog`, this panel has no preview state to revert. The presets are on `1`/`2`, not
/// `m`/`f`: the panel opens on `M`, and a doubled opening key must not wipe every field and
/// persist that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderOptionsDialog {
    options: RenderOptions,
    selected: usize,
}

impl RenderOptionsDialog {
    pub fn new(options: RenderOptions) -> Self {
        Self {
            options,
            selected: 0,
        }
    }

    fn row_count(&self) -> usize {
        self.options.options().len()
    }

    /// Centered, sized to every option row and the whole of `HINT`.
    pub fn popup_area(&self, area: Rect) -> Rect {
        let width = (HINT.chars().count() as u16 + 2).max(56).min(area.width);
        let height = (self.row_count() as u16 + 3).min(area.height);
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        Rect::new(x, y, width, height)
    }
}

impl Component for RenderOptionsDialog {
    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        match key.code {
            KeyCode::Up => {
                let len = self.row_count();
                move_selection(&mut self.selected, -1, len);
                Ok(Some(Action::Render))
            }
            KeyCode::Down => {
                let len = self.row_count();
                move_selection(&mut self.selected, 1, len);
                Ok(Some(Action::Render))
            }
            KeyCode::Char(' ') => {
                self.options.toggle(self.selected);
                Ok(Some(Action::RenderOptionsChanged(self.options)))
            }
            KeyCode::Enter | KeyCode::Esc => Ok(Some(Action::RenderOptionsAccepted)),
            KeyCode::Char('1') => {
                self.options = RenderOptions::MINIMAL;
                Ok(Some(Action::RenderOptionsChanged(self.options)))
            }
            KeyCode::Char('2') => {
                self.options = RenderOptions::FULL;
                Ok(Some(Action::RenderOptionsChanged(self.options)))
            }
            _ => Ok(None),
        }
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let items = self
            .options
            .options()
            .into_iter()
            .map(|(label, on)| ListItem::new(format!("[{}] {label}", if on { 'x' } else { ' ' })))
            .collect();
        render_list_dialog(
            frame,
            area,
            Line::from("Render options"),
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
    use crossterm::event::{KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn space_toggles_the_selected_option_and_reports_it() {
        let mut dialog = RenderOptionsDialog::new(RenderOptions::MINIMAL);

        let action = dialog.handle_key_event(key(KeyCode::Char(' '))).unwrap();

        assert_eq!(
            action,
            Some(Action::RenderOptionsChanged(RenderOptions {
                leading_whitespace: true,
                structural_punctuation: false,
                whole_pair_updates: false,
                paint_reindent_only_moves: false,
                paint_displaced_moves: false,
                paint_resized_moves: false,
                whole_identifier_updates: false,
            }))
        );
    }

    #[test]
    fn down_then_space_toggles_the_second_option() {
        let mut dialog = RenderOptionsDialog::new(RenderOptions::MINIMAL);
        dialog.handle_key_event(key(KeyCode::Down)).unwrap();

        let action = dialog.handle_key_event(key(KeyCode::Char(' '))).unwrap();

        assert_eq!(
            action,
            Some(Action::RenderOptionsChanged(RenderOptions {
                leading_whitespace: false,
                structural_punctuation: true,
                whole_pair_updates: false,
                paint_reindent_only_moves: false,
                paint_displaced_moves: false,
                paint_resized_moves: false,
                whole_identifier_updates: false,
            }))
        );
    }

    #[test]
    fn digits_jump_straight_to_the_named_presets() {
        let mut dialog = RenderOptionsDialog::new(RenderOptions::FULL);

        let to_minimal = dialog.handle_key_event(key(KeyCode::Char('1'))).unwrap();
        assert_eq!(
            to_minimal,
            Some(Action::RenderOptionsChanged(RenderOptions::MINIMAL))
        );

        let to_full = dialog.handle_key_event(key(KeyCode::Char('2'))).unwrap();
        assert_eq!(
            to_full,
            Some(Action::RenderOptionsChanged(RenderOptions::FULL))
        );
    }

    /// Every toggle has already applied and persisted itself, so closing has nothing left to
    /// change - and nothing to revert: `Esc` is not a cancel here.
    #[test]
    fn enter_and_esc_both_close_and_keep_every_toggle() {
        for close in [KeyCode::Enter, KeyCode::Esc] {
            let mut dialog = RenderOptionsDialog::new(RenderOptions::MINIMAL);
            dialog.handle_key_event(key(KeyCode::Char(' '))).unwrap();
            let toggled = dialog.options;

            let action = dialog.handle_key_event(key(close)).unwrap();

            assert_eq!(action, Some(Action::RenderOptionsAccepted), "{close:?}");
            assert_eq!(
                dialog.options, toggled,
                "{close:?} must not edit the options it closes on"
            );
        }
    }

    /// Bound to MINIMAL, a doubled `M`/`m` would turn every field off and persist it.
    #[test]
    fn the_key_that_opens_the_panel_is_inert_inside_it() {
        let mut dialog = RenderOptionsDialog::new(RenderOptions::FULL);

        assert_eq!(
            dialog.handle_key_event(key(KeyCode::Char('m'))).unwrap(),
            None
        );
        assert_eq!(
            dialog.handle_key_event(key(KeyCode::Char('f'))).unwrap(),
            None
        );
        assert_eq!(dialog.options, RenderOptions::FULL);
    }

    /// A preset pressed and then `Esc`: the preset stands, since it was applied and saved when
    /// it was pressed. `Esc` never sends a cancel from this panel.
    #[test]
    fn esc_after_a_preset_keeps_the_preset() {
        let mut dialog = RenderOptionsDialog::new(RenderOptions::FULL);

        dialog.handle_key_event(key(KeyCode::Char('1'))).unwrap();
        assert_eq!(dialog.options, RenderOptions::MINIMAL);

        let action = dialog.handle_key_event(key(KeyCode::Esc)).unwrap();

        assert_eq!(action, Some(Action::RenderOptionsAccepted));
        assert_eq!(dialog.options, RenderOptions::MINIMAL);
    }

    #[test]
    fn selection_does_not_move_past_the_last_row() {
        let mut dialog = RenderOptionsDialog::new(RenderOptions::FULL);
        for _ in 0..10 {
            dialog.handle_key_event(key(KeyCode::Down)).unwrap();
        }
        assert_eq!(dialog.selected, dialog.row_count() - 1);
    }

    #[test]
    fn popup_is_wide_enough_for_the_whole_hint() {
        let dialog = RenderOptionsDialog::new(RenderOptions::FULL);
        let popup = dialog.popup_area(Rect::new(0, 0, 200, 50));
        assert!(popup.width as usize >= HINT.chars().count() + 2);
    }
}
