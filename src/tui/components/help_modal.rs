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
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use super::Component;
use crate::tui::actions::Action;
use crate::tui::theme::OverlayTheme;

/// Static reference sheet of every keybinding plus an About section (copyright, license,
/// repository), kept in one place so the keybindings can't drift out of sync with individual
/// handlers the way a comment scattered across several files could. Mirrors (and should be kept
/// in sync with) README.md's "Using the TUI" section and `src/bin/human_solver.rs`'s own
/// `HELP_TEXT`/`?` modal, which this is modeled on. The diff-color legend is *not* part of this
/// static text: the actual colors are theme-dependent, so `draw` renders it from the live
/// `OverlayTheme` palette instead (see `legend_lines`) - a fixed "Green means inserted"
/// description would be wrong for most non-default themes.
const HELP_TEXT: &str = "\
Navigation
  Tab              Switch the active panel (Before/After)
  h/j/k/l          Move the cursor left/down/up/right
  Arrow keys       Same as h/j/k/l
  Enter            Jump to the counterpart of the range under the cursor (again: jump back)
  n/p              Jump to the next/previous change, skipping unchanged lines.
                   Walks both panels as one ordered sequence, switching sides when
                   that is where the next change is - so an insertion with nothing
                   on the before side is reached like any other change.
  g                Go to a line number
  /                Search the focused panel (smart-case; Enter jumps to nearest, Esc cancels)
                   A bare Enter repeats the last search; an empty query clears the highlights
  >/<              Jump to the next/previous search match
  Ctrl-d/Ctrl-u    Move the cursor half a page down/up
  Ctrl-e/Ctrl-y    Scroll the view one line without moving the cursor
  Page Up/Down     Scroll by a page
  Home/End         Jump to the top/bottom of the file

Files and diffing
  o                Open a file selector for the active panel
  r                Reload both files from disk and re-diff (keeps the cursor position)
  e                Open the focused panel's file in $VISUAL/$EDITOR at the cursor line
  Esc              While a diff is computing: cancel it and keep the previous result

Appearance
  c                Open the theme editor: a Theme dropdown, a syntax-highlighting
                   dropdown, and one editable color per diff operation, the cursor
                   counterpart, search matches, and the Before/After titles.
                   Up/Down moves, Left/Right changes a dropdown, Enter edits a color
                   (type #rrggbb) or accepts, Esc cancels. Editing any color forks the
                   selection to Custom, leaving the presets untouched.
  v                Cycle the panel layout: auto / dual / single (persisted)
  S                Toggle syntax highlighting
  H                Toggle the node highlight (off by default): the range under the
                   cursor, and its match on the other panel, highlight when part of a
                   real change. Unchanged content is never highlighted either way.
  M                Open the render-options panel: independent checkboxes for which
                   parts of the diff get painted (leading whitespace, standalone
                   punctuation), plus m/f shortcuts for the Minimal/Full presets.
                   Every choice applies immediately and persists. Trailing whitespace
                   is never painted, regardless of any option. Up/Down moves,
                   Enter/Space toggles, Esc closes.

Other
  ?                Toggle this help
  q or Esc         Quit (Esc closes an open dialog instead, while one is open)

About
  codediff - fast, syntax-aware code diffing using tree-sitter ASTs
  Copyright (C) 2026 Marko Ivankovic
  License: GNU Affero General Public License v3 or later
           https://www.gnu.org/licenses/
  Repository: https://github.com/ivankovic/codediff
";

/// The `?` popup: a scrollable keybinding reference plus a color legend rendered from the live
/// theme. Modeled on `ThemeDialog`'s shape, but has no selection state of its own - the only
/// state worth keeping is how far it's scrolled and which theme to render the legend with.
#[derive(Default)]
pub struct HelpModal {
    scroll: u16,
    theme: OverlayTheme,
}

impl HelpModal {
    pub fn new(theme: OverlayTheme) -> Self {
        Self { scroll: 0, theme }
    }

    /// The color legend, built from the live palette rather than hardcoded color names - each
    /// entry is a swatch painted in the actual background the viewer uses for that signal, so
    /// the legend is correct for whichever of the eight themes is active.
    fn legend_lines(&self) -> Vec<Line<'static>> {
        let palette = self.theme.palette();
        let swatch = |label: &str, bg: Color| -> Span<'static> {
            Span::styled(
                format!(" {label} "),
                Style::new().fg(palette.overlay_fg).bg(bg),
            )
        };
        vec![
            Line::from("Colors (current theme)"),
            Line::from(vec![
                Span::raw("  "),
                swatch("inserted", palette.insert_bg),
                Span::raw(" "),
                swatch("deleted", palette.delete_bg),
                Span::raw(" "),
                swatch("moved", palette.move_bg),
                Span::raw(" "),
                swatch("updated", palette.update_bg),
                Span::raw(" "),
                swatch("cursor/counterpart", palette.cross_highlight_bg),
                Span::raw(" "),
                swatch("search match", palette.search_bg),
            ]),
            Line::from(""),
        ]
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

        // The live color legend first (it's what a first-time user most needs decoded), then
        // the static keybinding reference.
        let mut lines = self.legend_lines();
        lines.extend(HELP_TEXT.lines().map(|l| Line::from(l.to_string())));

        frame.render_widget(
            Paragraph::new(lines)
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
        let mut modal = HelpModal::new(OverlayTheme::default());
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
        let mut modal = HelpModal::new(OverlayTheme::default());
        modal.handle_key_event(key(KeyCode::Char('j'))).unwrap();
        modal.handle_key_event(key(KeyCode::Char('j'))).unwrap();
        assert_eq!(modal.scroll, 2);
        modal.handle_key_event(key(KeyCode::Char('k'))).unwrap();
        assert_eq!(modal.scroll, 1);
    }

    #[test]
    fn scroll_does_not_go_negative() {
        let mut modal = HelpModal::new(OverlayTheme::default());
        modal.handle_key_event(key(KeyCode::Up)).unwrap();
        assert_eq!(modal.scroll, 0);
    }

    /// Sized generously (well past `HELP_TEXT`'s longest line and line count) so nothing about
    /// this test depends on the real terminal size - it's only checking that `draw` doesn't
    /// panic and that the keybinding reference text actually ends up on screen somewhere.
    #[test]
    fn help_modal_renders_keybindings() {
        let backend = TestBackend::new(120, 70);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut modal = HelpModal::new(OverlayTheme::default());

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

    /// The color legend renders from the live palette - swatch cells carry the theme's actual
    /// diff backgrounds, so "what does this color mean" is answered correctly per theme rather
    /// than by a hardcoded color-name list.
    #[test]
    fn help_modal_renders_a_legend_with_the_current_themes_backgrounds() {
        let backend = TestBackend::new(120, 70);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut modal = HelpModal::new(OverlayTheme::Nord);

        terminal
            .draw(|f| {
                let area = f.size();
                modal.draw(f, modal.popup_area(area)).unwrap();
            })
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        let rendered: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(rendered.contains("inserted"));
        assert!(rendered.contains("search match"));

        let palette = OverlayTheme::Nord.palette();
        let backgrounds: Vec<_> = buffer
            .content()
            .iter()
            .filter_map(|cell| cell.bg.into())
            .collect();
        assert!(
            backgrounds.contains(&palette.insert_bg),
            "a legend swatch should be painted in Nord's own insert background"
        );
        assert!(
            backgrounds.contains(&palette.search_bg),
            "a legend swatch should be painted in Nord's own search background"
        );
    }
}
