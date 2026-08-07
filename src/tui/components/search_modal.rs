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
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Stylize,
    style::{Color, Style},
    symbols::border,
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

use super::Component;
use crate::tui::actions::Action;

/// A popup that reads a search query (the `/` key), same visual scaffold as `ThemeDialog`/
/// `FileDialog` (bordered, cyan title, dim hint line) but text entry rather than a list, since
/// there's nothing to list yet - matches only exist once a query has been typed.
#[derive(Default)]
pub struct SearchModal {
    query: String,
}

impl SearchModal {
    pub fn new() -> Self {
        Self::default()
    }

    /// The query typed so far. Exposed at crate visibility only for `app.rs`'s own tests (e.g. the
    /// double-dispatch regression guard) - nothing outside this module needs to read live input.
    #[cfg(test)]
    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    /// The area the popup itself should occupy, centered within `area` - same shape as
    /// `ThemeDialog::popup_area`, sized for a single line of input rather than a list.
    pub fn popup_area(&self, area: Rect) -> Rect {
        let width = 50.min(area.width);
        let height = 4.min(area.height);
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        Rect::new(x, y, width, height)
    }

    /// Where the text cursor should be drawn on screen within `area` (the same area passed to
    /// `draw`), right after the typed query.
    pub fn cursor_screen_position(&self, area: Rect) -> (u16, u16) {
        // +1 for the border, +1 for the leading "/" the input line is prefixed with.
        let col = area.x + 2 + self.query.chars().count() as u16;
        (col, area.y + 1)
    }
}

impl Component for SearchModal {
    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        match key.code {
            KeyCode::Char(c) => {
                self.query.push(c);
                Ok(Some(Action::Render))
            }
            KeyCode::Backspace => {
                self.query.pop();
                Ok(Some(Action::Render))
            }
            KeyCode::Enter => Ok(Some(Action::SearchSubmitted(self.query.clone()))),
            KeyCode::Esc => Ok(Some(Action::DialogCancelled)),
            _ => Ok(None),
        }
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);

        let block = Block::default()
            .title(" Search ".bold().fg(Color::Cyan))
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .border_style(Style::new().fg(Color::Cyan));

        frame.render_widget(
            Paragraph::new(format!("/{}", self.query)).block(block),
            layout[0],
        );
        frame.render_widget(
            Line::from(" Enter: jump to match | Esc: cancel ").dim(),
            layout[1],
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
    fn typing_characters_builds_up_the_query() {
        let mut modal = SearchModal::new();
        modal.handle_key_event(key(KeyCode::Char('f'))).unwrap();
        modal.handle_key_event(key(KeyCode::Char('o'))).unwrap();
        modal.handle_key_event(key(KeyCode::Char('o'))).unwrap();
        assert_eq!(modal.query, "foo");
    }

    #[test]
    fn backspace_removes_the_last_character() {
        let mut modal = SearchModal::new();
        modal.handle_key_event(key(KeyCode::Char('f'))).unwrap();
        modal.handle_key_event(key(KeyCode::Char('o'))).unwrap();
        modal.handle_key_event(key(KeyCode::Backspace)).unwrap();
        assert_eq!(modal.query, "f");
    }

    #[test]
    fn backspace_on_an_empty_query_is_a_no_op() {
        let mut modal = SearchModal::new();
        modal.handle_key_event(key(KeyCode::Backspace)).unwrap();
        assert_eq!(modal.query, "");
    }

    #[test]
    fn enter_submits_the_typed_query() {
        let mut modal = SearchModal::new();
        modal.handle_key_event(key(KeyCode::Char('x'))).unwrap();
        let action = modal.handle_key_event(key(KeyCode::Enter)).unwrap();
        assert_eq!(action, Some(Action::SearchSubmitted("x".to_string())));
    }

    #[test]
    fn esc_cancels_without_submitting() {
        let mut modal = SearchModal::new();
        let action = modal.handle_key_event(key(KeyCode::Esc)).unwrap();
        assert_eq!(action, Some(Action::DialogCancelled));
    }

    #[test]
    fn cursor_screen_position_follows_the_typed_query() {
        let modal = SearchModal::new();
        let area = Rect::new(10, 5, 50, 4);
        assert_eq!(modal.cursor_screen_position(area), (12, 6));

        let mut modal = SearchModal::new();
        modal.handle_key_event(key(KeyCode::Char('a'))).unwrap();
        modal.handle_key_event(key(KeyCode::Char('b'))).unwrap();
        assert_eq!(modal.cursor_screen_position(area), (14, 6));
    }
}
