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
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Stylize,
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

use super::Component;
use crate::tui::actions::Action;

/// The `g` jump-to-line prompt: same visual scaffold as `SearchModal` (bordered, cyan title, a
/// hint line, text entry), but digits-only input - non-digit characters are silently ignored
/// rather than accepted-then-rejected on submit, so the field can never hold something that
/// wouldn't parse. Submitting an empty field just cancels (there is no "line nothing").
#[derive(Default)]
pub struct LinePrompt {
    digits: String,
}

impl LinePrompt {
    pub fn new() -> Self {
        Self::default()
    }

    /// The area the popup itself should occupy, centered within `area` - same shape as
    /// `SearchModal::popup_area`.
    pub fn popup_area(&self, area: Rect) -> Rect {
        let width = 40.min(area.width);
        let height = 4.min(area.height);
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        Rect::new(x, y, width, height)
    }

    /// Where the text cursor should be drawn on screen within `area` (the same area passed to
    /// `draw`), right after the typed digits.
    pub fn cursor_screen_position(&self, area: Rect) -> (u16, u16) {
        // +1 for the border, +1 for the leading ":" the input line is prefixed with.
        let col = area.x + 2 + self.digits.chars().count() as u16;
        (col, area.y + 1)
    }
}

impl Component for LinePrompt {
    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        match key.code {
            KeyCode::Char(c) if c.is_ascii_digit() => {
                self.digits.push(c);
                Ok(Some(Action::Render))
            }
            KeyCode::Backspace => {
                self.digits.pop();
                Ok(Some(Action::Render))
            }
            KeyCode::Enter => match self.digits.parse::<usize>() {
                Ok(line) if line > 0 => Ok(Some(Action::JumpToLineSubmitted(line))),
                // Empty (or a pathological "0"/overflow) - nothing meaningful to jump to.
                _ => Ok(Some(Action::DialogCancelled)),
            },
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
            .title(" Go to line ".bold().fg(Color::Cyan))
            .borders(Borders::ALL)
            .border_style(Style::new().fg(Color::Cyan));
        frame.render_widget(
            Paragraph::new(Line::from(format!(":{}", self.digits))).block(block),
            layout[0],
        );
        frame.render_widget(
            Paragraph::new(" Enter: jump | Esc: cancel ").style(Style::new().fg(Color::DarkGray)),
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
    fn digits_accumulate_and_submit_as_a_line_number() {
        let mut prompt = LinePrompt::new();
        prompt.handle_key_event(key(KeyCode::Char('4'))).unwrap();
        prompt.handle_key_event(key(KeyCode::Char('2'))).unwrap();
        assert_eq!(
            prompt.handle_key_event(key(KeyCode::Enter)).unwrap(),
            Some(Action::JumpToLineSubmitted(42))
        );
    }

    #[test]
    fn non_digit_characters_are_ignored_entirely() {
        let mut prompt = LinePrompt::new();
        assert_eq!(
            prompt.handle_key_event(key(KeyCode::Char('x'))).unwrap(),
            None
        );
        prompt.handle_key_event(key(KeyCode::Char('7'))).unwrap();
        assert_eq!(
            prompt.handle_key_event(key(KeyCode::Enter)).unwrap(),
            Some(Action::JumpToLineSubmitted(7))
        );
    }

    #[test]
    fn backspace_edits_and_empty_enter_cancels() {
        let mut prompt = LinePrompt::new();
        prompt.handle_key_event(key(KeyCode::Char('9'))).unwrap();
        prompt.handle_key_event(key(KeyCode::Backspace)).unwrap();
        assert_eq!(
            prompt.handle_key_event(key(KeyCode::Enter)).unwrap(),
            Some(Action::DialogCancelled)
        );
    }

    #[test]
    fn esc_cancels() {
        let mut prompt = LinePrompt::new();
        assert_eq!(
            prompt.handle_key_event(key(KeyCode::Esc)).unwrap(),
            Some(Action::DialogCancelled)
        );
    }

    #[test]
    fn zero_is_rejected_as_a_cancel_not_a_jump() {
        let mut prompt = LinePrompt::new();
        prompt.handle_key_event(key(KeyCode::Char('0'))).unwrap();
        assert_eq!(
            prompt.handle_key_event(key(KeyCode::Enter)).unwrap(),
            Some(Action::DialogCancelled)
        );
    }
}
