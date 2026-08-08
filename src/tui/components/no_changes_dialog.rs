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
    widgets::{Block, Borders, Paragraph, Wrap},
};

use super::Component;
use super::diff_viewer::Panel;
use crate::tui::actions::Action;

/// Popup shown when `n`/`p` is pressed on a panel with no navigable changes while the *other*
/// panel has some (e.g. the "before" side of a diff that's pure insertions - there is nothing on
/// that side to ever jump to). Before this existed, that key press was a silent no-op; this
/// offers to switch panels and jump there instead. Same visual scaffold as `SearchModal`
/// (bordered, cyan title, dim hint line) - a message rather than a list or text entry, since
/// there's nothing to choose between beyond confirm/cancel.
pub struct NoChangesDialog {
    /// Which direction (`n` = forward, `p` = backward) triggered this - replayed on confirmation
    /// via `Action::NoChangesPromptConfirmed`.
    forward: bool,
    /// The panel that has no changes (the one focused when `n`/`p` was pressed) - only used for
    /// the message; confirming always switches to *the other* panel; see `other_panel_label`.
    empty_panel: Panel,
}

impl NoChangesDialog {
    pub fn new(forward: bool, empty_panel: Panel) -> Self {
        Self {
            forward,
            empty_panel,
        }
    }

    /// The area the popup itself should occupy, centered within `area` - same shape as
    /// `SearchModal::popup_area`, sized for a couple of lines of message plus the hint line.
    pub fn popup_area(&self, area: Rect) -> Rect {
        let width = 60.min(area.width);
        let height = 5.min(area.height);
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        Rect::new(x, y, width, height)
    }

    fn empty_panel_label(&self) -> &'static str {
        panel_label(self.empty_panel)
    }

    fn other_panel_label(&self) -> &'static str {
        panel_label(other_panel(self.empty_panel))
    }
}

fn panel_label(panel: Panel) -> &'static str {
    match panel {
        Panel::Before => "Before",
        Panel::After => "After",
    }
}

fn other_panel(panel: Panel) -> Panel {
    match panel {
        Panel::Before => Panel::After,
        Panel::After => Panel::Before,
    }
}

impl Component for NoChangesDialog {
    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        match key.code {
            KeyCode::Enter => Ok(Some(Action::NoChangesPromptConfirmed {
                forward: self.forward,
            })),
            KeyCode::Esc => Ok(Some(Action::DialogCancelled)),
            _ => Ok(None),
        }
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(2), Constraint::Length(1)])
            .split(area);

        let block = Block::default()
            .title(" No changes ".bold().fg(Color::Cyan))
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .border_style(Style::new().fg(Color::Cyan));

        let message = format!(
            "The {} panel has no changes to jump to. Switch to {} and jump there?",
            self.empty_panel_label(),
            self.other_panel_label(),
        );
        frame.render_widget(
            Paragraph::new(message)
                .block(block)
                .wrap(Wrap { trim: true }),
            layout[0],
        );
        frame.render_widget(
            Line::from(" Enter: switch and jump | Esc: cancel ").dim(),
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
    fn enter_confirms_with_the_original_direction() {
        let mut dialog = NoChangesDialog::new(true, Panel::Before);
        assert_eq!(
            dialog.handle_key_event(key(KeyCode::Enter)).unwrap(),
            Some(Action::NoChangesPromptConfirmed { forward: true })
        );

        let mut dialog = NoChangesDialog::new(false, Panel::After);
        assert_eq!(
            dialog.handle_key_event(key(KeyCode::Enter)).unwrap(),
            Some(Action::NoChangesPromptConfirmed { forward: false })
        );
    }

    #[test]
    fn esc_cancels_without_confirming() {
        let mut dialog = NoChangesDialog::new(true, Panel::Before);
        assert_eq!(
            dialog.handle_key_event(key(KeyCode::Esc)).unwrap(),
            Some(Action::DialogCancelled)
        );
    }

    #[test]
    fn other_key_is_ignored() {
        let mut dialog = NoChangesDialog::new(true, Panel::Before);
        assert_eq!(
            dialog.handle_key_event(key(KeyCode::Char('x'))).unwrap(),
            None
        );
    }

    #[test]
    fn labels_name_the_empty_panel_and_its_opposite() {
        let dialog = NoChangesDialog::new(true, Panel::Before);
        assert_eq!(dialog.empty_panel_label(), "Before");
        assert_eq!(dialog.other_panel_label(), "After");

        let dialog = NoChangesDialog::new(true, Panel::After);
        assert_eq!(dialog.empty_panel_label(), "After");
        assert_eq!(dialog.other_panel_label(), "Before");
    }
}
