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
    layout::Rect,
    prelude::Stylize,
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use super::Component;
use crate::tui::actions::Action;

/// Static reference sheet of every keybinding plus an About section (copyright, license,
/// repository), kept in one place so the keybindings can't drift out of sync with individual
/// handlers the way a comment scattered across several files could. Mirrors (and should be kept
/// in sync with) README.md's "Using the TUI" section and `src/bin/human_solver.rs`'s own
/// `HELP_TEXT`/`?` modal, which this is modeled on. Deliberately has no diff-color legend: the
/// actual colors are theme-dependent (see `tui::theme::OverlayTheme`), so a fixed "Green means
/// inserted" description would be wrong for most non-default themes.
const HELP_TEXT: &str = "\
Navigation
  Tab              Switch the active panel (Before/After)
  h/j/k/l          Move the cursor left/down/up/right
  Arrow keys       Same as h/j/k/l
                   The range under the cursor, and its match on the other panel, highlight in
                   blue when part of a real change; unchanged content is never highlighted
  n/p              Jump to the next/previous change, skipping unchanged lines
  /                Search the focused panel (Enter jumps to nearest, Esc cancels)
                   An empty query clears the current search's highlights
  >/<              Jump to the next/previous search match
  Page Up/Down     Scroll by a page
  Home/End         Jump to the top/bottom of the file

Files and appearance
  o                Open a file selector for the active panel
  c                Open the color theme picker

Other
  ?                Toggle this help
  q or Esc         Quit (Esc cancels an open dialog instead, while one is open)

About
  codediff - fast, syntax-aware code diffing using tree-sitter ASTs
  Copyright (C) 2026 Marko Ivankovic
  License: GNU Affero General Public License v3 or later
           https://www.gnu.org/licenses/
  Repository: https://github.com/ivankovic/codediff
";

/// The `?` popup: a static, scrollable keybinding/color-legend reference. Modeled on
/// `ThemeDialog`/`DiffModeDialog`'s shape, but has no selection state of its own - `HELP_TEXT` is
/// display-only, so the only state worth keeping is how far it's scrolled.
#[derive(Default)]
pub struct HelpModal {
    scroll: u16,
}

impl HelpModal {
    pub fn new() -> Self {
        Self::default()
    }

    /// The area the popup itself should occupy, centered within `area` - same shape as
    /// `ThemeDialog::popup_area`, just generously sized (90%) since this is a full reference
    /// sheet rather than a single-purpose picker.
    pub fn popup_area(&self, area: Rect) -> Rect {
        let width = (area.width * 9 / 10).min(area.width);
        let height = (area.height * 9 / 10).min(area.height);
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        Rect::new(x, y, width, height)
    }
}

impl Component for HelpModal {
    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        match key.code {
            KeyCode::Char('?') | KeyCode::Esc => Ok(Some(Action::DialogCancelled)),
            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll = self.scroll.saturating_add(1);
                Ok(Some(Action::Render))
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll = self.scroll.saturating_sub(1);
                Ok(Some(Action::Render))
            }
            _ => Ok(None),
        }
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        // `area` is already cleared by the caller (`app::draw_help_modal`, same as every other
        // popup's own draw site) before this runs - clearing it a second time here would be a
        // no-op, not a correctness issue, so this was only ever wasted work, not a visible bug.
        let block = Block::default()
            .title(
                " Help - j/k scroll, ? or Esc to close "
                    .bold()
                    .fg(Color::Cyan),
            )
            .borders(Borders::ALL)
            .border_style(Style::new().fg(Color::Cyan));

        frame.render_widget(
            Paragraph::new(HELP_TEXT)
                .block(block)
                .wrap(Wrap { trim: false })
                .scroll((self.scroll, 0)),
            area,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use ratatui::{Terminal, backend::TestBackend};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn question_mark_and_esc_both_close_the_modal() {
        let mut modal = HelpModal::new();
        assert_eq!(
            modal.handle_key_event(key(KeyCode::Char('?'))).unwrap(),
            Some(Action::DialogCancelled)
        );
        assert_eq!(
            modal.handle_key_event(key(KeyCode::Esc)).unwrap(),
            Some(Action::DialogCancelled)
        );
    }

    #[test]
    fn j_and_k_scroll_down_and_up() {
        let mut modal = HelpModal::new();
        modal.handle_key_event(key(KeyCode::Char('j'))).unwrap();
        modal.handle_key_event(key(KeyCode::Char('j'))).unwrap();
        assert_eq!(modal.scroll, 2);
        modal.handle_key_event(key(KeyCode::Char('k'))).unwrap();
        assert_eq!(modal.scroll, 1);
    }

    #[test]
    fn scroll_does_not_go_negative() {
        let mut modal = HelpModal::new();
        modal.handle_key_event(key(KeyCode::Up)).unwrap();
        assert_eq!(modal.scroll, 0);
    }

    /// Sized generously (well past `HELP_TEXT`'s longest line and line count) so nothing about
    /// this test depends on the real terminal size - it's only checking that `draw` doesn't
    /// panic and that the keybinding reference text actually ends up on screen somewhere.
    #[test]
    fn help_modal_renders_keybindings() {
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut modal = HelpModal::new();

        terminal
            .draw(|f| {
                let area = f.size();
                modal.draw(f, modal.popup_area(area)).unwrap();
            })
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        let rendered: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(rendered.contains("Navigation"));
        assert!(rendered.contains("About"));
        assert!(rendered.contains("Copyright"));
    }
}
