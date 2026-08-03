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
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{Frame, layout::Rect, prelude::Stylize, style::Color, widgets::ListItem};
use strum::IntoEnumIterator;

use super::{Component, move_selection, render_list_dialog};
use crate::tui::actions::Action;
use crate::tui::theme::OverlayTheme;

/// A popup that lets the user pick the diff/cursor overlay color theme (the `c` key).
pub struct ThemeDialog {
    themes: Vec<OverlayTheme>,
    selected: usize,
}

impl ThemeDialog {
    /// Create a dialog listing every theme, with `current` pre-selected.
    pub fn new(current: OverlayTheme) -> Self {
        let themes: Vec<OverlayTheme> = OverlayTheme::iter().collect();
        let selected = themes.iter().position(|&t| t == current).unwrap_or(0);
        Self { themes, selected }
    }

    /// The area the popup itself should occupy, centered within `area` (e.g. the full terminal).
    /// Sized to fit every theme plus the dialog's own border and hint line, but never wider/
    /// taller than `area`.
    pub fn popup_area(&self, area: Rect) -> Rect {
        let width = 30.min(area.width);
        let height = (self.themes.len() as u16 + 4).min(area.height);
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        Rect::new(x, y, width, height)
    }
}

impl Component for ThemeDialog {
    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        match key.code {
            KeyCode::Up => {
                move_selection(&mut self.selected, -1, self.themes.len());
                Ok(Some(Action::Render))
            }
            KeyCode::Down => {
                move_selection(&mut self.selected, 1, self.themes.len());
                Ok(Some(Action::Render))
            }
            KeyCode::Enter => Ok(Some(Action::ThemeSelected(self.themes[self.selected]))),
            KeyCode::Esc => Ok(Some(Action::DialogCancelled)),
            _ => Ok(None),
        }
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let items: Vec<ListItem> = self
            .themes
            .iter()
            .map(|theme| ListItem::new(theme.to_string()))
            .collect();

        render_list_dialog(
            frame,
            area,
            " Color Theme ".bold().fg(Color::Cyan).into(),
            items,
            self.selected,
            " Enter: select | Esc: cancel ",
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
    fn new_preselects_the_current_theme() {
        let dialog = ThemeDialog::new(OverlayTheme::SolarizedDark);
        assert_eq!(dialog.themes[dialog.selected], OverlayTheme::SolarizedDark);
    }

    #[test]
    fn enter_selects_the_highlighted_theme() {
        let mut dialog = ThemeDialog::new(OverlayTheme::Dark);
        dialog.handle_key_event(key(KeyCode::Down)).unwrap();
        let action = dialog.handle_key_event(key(KeyCode::Enter)).unwrap();
        assert_eq!(
            action,
            Some(Action::ThemeSelected(dialog.themes[dialog.selected]))
        );
        assert_ne!(dialog.themes[dialog.selected], OverlayTheme::Dark);
    }

    #[test]
    fn down_does_not_run_past_the_last_theme() {
        let mut dialog = ThemeDialog::new(OverlayTheme::Dark);
        for _ in 0..10 {
            dialog.handle_key_event(key(KeyCode::Down)).unwrap();
        }
        assert_eq!(dialog.selected, dialog.themes.len() - 1);
    }

    #[test]
    fn up_does_not_run_before_the_first_theme() {
        let mut dialog = ThemeDialog::new(OverlayTheme::Dark);
        let action = dialog.handle_key_event(key(KeyCode::Up)).unwrap();
        assert_eq!(action, Some(Action::Render));
        assert_eq!(dialog.selected, 0);
    }

    #[test]
    fn esc_cancels_without_selecting() {
        let mut dialog = ThemeDialog::new(OverlayTheme::Dark);
        let action = dialog.handle_key_event(key(KeyCode::Esc)).unwrap();
        assert_eq!(action, Some(Action::DialogCancelled));
    }
}
