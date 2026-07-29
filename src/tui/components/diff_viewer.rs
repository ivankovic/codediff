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
use std::path::PathBuf;

use anyhow::Result;
use ratatui::{
    prelude::*,
    symbols::border,
    text::Line,
    widgets::{Block, Borders},
};
use tokio::sync::mpsc::UnboundedSender;

use super::{Component, code_viewer::CodeViewer};
use crate::tui::actions::{Action, DiffSessionData};
use crate::tui::theme::OverlayTheme;

/// Below this terminal width, two side-by-side panels would each be too narrow to read code in,
/// so callers fall back to showing a single panel at full width. Shared with `human_solver`'s
/// own before/after panel layout, which has the same readability constraint.
pub const SINGLE_PANEL_THRESHOLD: u16 = 220;

/// A component that displays the before/after files side by side for diffing
#[derive(Default)]
pub struct DiffViewer {
    /// The "Before" code viewer
    left_viewer: CodeViewer,
    /// The "After" code viewer
    right_viewer: CodeViewer,
    /// Action sender
    command_tx: Option<UnboundedSender<Action>>,
    /// Current display mode: dual panel or single panel
    display_mode: DisplayMode,
    /// Which panel is shown in single panel mode, and which panel's cursor drives navigation
    /// (and which panel `o` opens a file selector for) in dual panel mode.
    active_panel: Panel,
}

/// Display mode for the diff viewer
#[derive(Default, Clone, Copy, PartialEq)]
enum DisplayMode {
    /// Show both panels side by side
    #[default]
    Dual,
    /// Show only one panel at a time
    Single,
}

/// Which of the two panels is active.
#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub enum Panel {
    #[default]
    Before,
    After,
}

impl DiffViewer {
    /// Create a new DiffViewer
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a DiffViewer with specific files
    pub fn with_files(left_path: PathBuf, right_path: PathBuf) -> Self {
        Self {
            left_viewer: CodeViewer::with_file(left_path),
            right_viewer: CodeViewer::with_file(right_path),
            ..Default::default()
        }
    }

    /// Load files into the viewers
    pub fn load_files(&mut self, left_path: PathBuf, right_path: PathBuf) -> Result<()> {
        self.left_viewer.load_file(left_path)?;
        self.right_viewer.load_file(right_path)?;
        Ok(())
    }

    /// Load a completed diff: the file contents (already read, no further disk I/O here) plus
    /// the before/after diff ranges, and reset the cursor/cross-highlight to the start.
    pub fn load_diff(&mut self, data: &DiffSessionData) {
        self.left_viewer
            .load_contents(data.before_path.clone(), data.before_contents.clone());
        self.right_viewer
            .load_contents(data.after_path.clone(), data.after_contents.clone());
        self.left_viewer.set_ranges(data.before_ranges.clone());
        self.right_viewer.set_ranges(data.after_ranges.clone());
        self.active_panel = Panel::Before;
        self.sync_focus();
        self.sync_cross_highlight();
    }

    /// Move the focused panel's cursor vertically (by one line) and push the resulting matched
    /// node onto the other panel's cross-highlight, scrolling it to keep the destination visible.
    pub fn move_cursor_vertical(&mut self, direction: i32) {
        self.focused_viewer().move_cursor_vertical(direction);
        self.sync_cross_highlight();
        self.sync_scroll();
    }

    /// Move the focused panel's cursor horizontally (by one character) and push the resulting
    /// matched node onto the other panel's cross-highlight, scrolling it to keep the destination visible.
    pub fn move_cursor_horizontal(&mut self, direction: i32) {
        self.focused_viewer().move_cursor_horizontal(direction);
        self.sync_cross_highlight();
        self.sync_scroll();
    }

    /// Move the focused panel's cursor to the next (`forward = true`) or previous (`forward =
    /// false`) actual change (`n`/`p`), and push the resulting matched node onto the other
    /// panel's cross-highlight, same as any other cursor movement.
    pub fn jump_to_change(&mut self, forward: bool) {
        self.focused_viewer().jump_to_change(forward);
        self.sync_cross_highlight();
        self.sync_scroll();
    }

    /// Push the focused panel's current cursor destination onto the other panel's
    /// cross-highlight, and move the other panel's cursor to follow the matched leaf node;
    /// call after anything that can change the cursor or the focused panel.
    fn sync_cross_highlight(&mut self) {
        let destination = self.focused_viewer().cursor_destination();
        self.other_viewer()
            .set_highlight_destination(destination.clone());

        // Also move the inactive side's cursor to follow the matched leaf node
        if let Some(dest_range) = destination {
            self.other_viewer()
                .set_cursor_position(dest_range.start_row, dest_range.start_column);
        }
    }

    /// Scroll the inactive panel's viewport so the destination row of the focused panel's cursor
    /// stays visible; call after cursor movement.
    fn sync_scroll(&mut self) {
        if let Some(dest) = self.focused_viewer().cursor_destination() {
            self.other_viewer().scroll_to_show_row(dest.start_row);
        }
    }

    /// Make sure exactly one side is marked focused, matching `active_panel`: the focused side
    /// then shows its own live cursor, and the other side shows only the pushed cross-highlight
    /// (see `CodeViewerState::is_focused`). Call whenever `active_panel` changes.
    fn sync_focus(&mut self) {
        self.left_viewer
            .set_focused(self.active_panel == Panel::Before);
        self.right_viewer
            .set_focused(self.active_panel == Panel::After);
    }

    /// Set the palette used to paint the diff/cursor overlay on both panels, picked via the `c`
    /// theme picker.
    pub fn set_overlay_theme(&mut self, theme: OverlayTheme) {
        self.left_viewer.set_overlay_theme(theme);
        self.right_viewer.set_overlay_theme(theme);
    }

    /// The panel whose cursor currently drives navigation.
    fn focused_viewer(&mut self) -> &mut CodeViewer {
        match self.active_panel {
            Panel::Before => &mut self.left_viewer,
            Panel::After => &mut self.right_viewer,
        }
    }

    /// The panel that is cross-highlighted from the focused panel's cursor.
    fn other_viewer(&mut self) -> &mut CodeViewer {
        match self.active_panel {
            Panel::Before => &mut self.right_viewer,
            Panel::After => &mut self.left_viewer,
        }
    }

    /// Which panel is currently active, i.e. which one `Tab` last selected.
    pub fn active_panel(&self) -> Panel {
        self.active_panel
    }

    /// Load a single file (no diff overlay yet) into the "Before" panel.
    pub fn set_before_file(&mut self, path: PathBuf) -> Result<()> {
        self.left_viewer.load_file(path)
    }

    /// Load a single file (no diff overlay yet) into the "After" panel.
    pub fn set_after_file(&mut self, path: PathBuf) -> Result<()> {
        self.right_viewer.load_file(path)
    }

    /// Update display mode based on available width
    pub fn update_display_mode(&mut self, width: u16) {
        self.display_mode = if width < SINGLE_PANEL_THRESHOLD {
            DisplayMode::Single
        } else {
            DisplayMode::Dual
        };
    }

    /// Toggle between the "Before" and "After" panel.
    pub fn toggle_active_panel(&mut self) {
        self.active_panel = match self.active_panel {
            Panel::Before => Panel::After,
            Panel::After => Panel::Before,
        };
        self.sync_focus();
    }

    /// Get the filename of the active viewer
    fn active_filename(&self) -> String {
        match self.active_panel {
            Panel::Before => self.left_viewer.filename(),
            Panel::After => self.right_viewer.filename(),
        }
    }

    /// Get the language name of the active viewer
    fn active_language(&self) -> String {
        match self.active_panel {
            Panel::Before => self.left_viewer.language_name(),
            Panel::After => self.right_viewer.language_name(),
        }
    }
}

impl Component for DiffViewer {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.command_tx = Some(tx.clone());
        self.left_viewer.register_action_handler(tx.clone())?;
        self.right_viewer.register_action_handler(tx)?;
        Ok(())
    }

    fn init(&mut self, area: Rect) -> Result<()> {
        // Update display mode based on available width
        self.update_display_mode(area.width);

        if self.display_mode == DisplayMode::Dual {
            // Split the area into two halves
            let divider = area.width / 2;
            let left_area = Rect::new(area.x, area.y, divider, area.height);
            let right_area = Rect::new(
                area.x + divider + 1,
                area.y,
                area.width - divider - 1,
                area.height,
            );

            self.left_viewer.init(left_area)?;
            self.right_viewer.init(right_area)?;
        } else {
            // Single panel mode: init both viewers with full area
            // They'll be switched based on active_panel
            self.left_viewer.init(area)?;
            self.right_viewer.init(area)?;
        }

        Ok(())
    }

    fn handle_key_event(&mut self, key: crossterm::event::KeyEvent) -> Result<Option<Action>> {
        // Handle key events
        match key.code {
            // Tab switches which panel's cursor drives navigation (and, in single panel mode,
            // which panel is shown).
            crossterm::event::KeyCode::Tab => {
                self.toggle_active_panel();
                self.sync_cross_highlight();
                Ok(Some(Action::Render))
            }
            // The cursor is a real (row, column) position (see SPECS.md), so arrows and vim
            // h/j/k/l map to literal left/down/up/right movement, same as any text editor.
            crossterm::event::KeyCode::Up | crossterm::event::KeyCode::Char('k') => {
                self.move_cursor_vertical(-1);
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Down | crossterm::event::KeyCode::Char('j') => {
                self.move_cursor_vertical(1);
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Left | crossterm::event::KeyCode::Char('h') => {
                self.move_cursor_horizontal(-1);
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Right | crossterm::event::KeyCode::Char('l') => {
                self.move_cursor_horizontal(1);
                Ok(Some(Action::Render))
            }
            // n/p jump the cursor straight to the next/previous actual change, skipping over
            // unchanged content entirely - unlike h/j/k/l, which move one character/line at a
            // time regardless of what's there.
            crossterm::event::KeyCode::Char('n') => {
                self.jump_to_change(true);
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Char('p') => {
                self.jump_to_change(false);
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::PageUp => {
                if self.display_mode == DisplayMode::Dual {
                    let left_lines = self.left_viewer.viewport_height();
                    let right_lines = self.right_viewer.viewport_height();
                    for _ in 0..left_lines {
                        self.left_viewer.scroll_up();
                    }
                    for _ in 0..right_lines {
                        self.right_viewer.scroll_up();
                    }
                } else {
                    let lines = self.focused_viewer().viewport_height();
                    for _ in 0..lines {
                        self.focused_viewer().scroll_up();
                    }
                }
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::PageDown => {
                if self.display_mode == DisplayMode::Dual {
                    let left_lines = self.left_viewer.viewport_height();
                    let right_lines = self.right_viewer.viewport_height();
                    for _ in 0..left_lines {
                        self.left_viewer.scroll_down();
                    }
                    for _ in 0..right_lines {
                        self.right_viewer.scroll_down();
                    }
                } else {
                    let lines = self.focused_viewer().viewport_height();
                    for _ in 0..lines {
                        self.focused_viewer().scroll_down();
                    }
                }
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Home => {
                if self.display_mode == DisplayMode::Dual {
                    self.left_viewer.scroll_to(0);
                    self.right_viewer.scroll_to(0);
                } else {
                    self.focused_viewer().scroll_to(0);
                }
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::End => {
                if self.display_mode == DisplayMode::Dual {
                    let left_lines = self.left_viewer.line_count();
                    let right_lines = self.right_viewer.line_count();
                    let max_lines = std::cmp::max(left_lines, right_lines);
                    self.left_viewer.scroll_to(max_lines.saturating_sub(1));
                    self.right_viewer.scroll_to(max_lines.saturating_sub(1));
                } else {
                    let lines = self.focused_viewer().line_count();
                    self.focused_viewer().scroll_to(lines.saturating_sub(1));
                }
                Ok(Some(Action::Render))
            }
            _ => {
                // Let individual viewers handle other keys
                // For now, just forward to both
                let _ = self.left_viewer.handle_key_event(key)?;
                let _ = self.right_viewer.handle_key_event(key)?;
                Ok(None)
            }
        }
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match &action {
            Action::Resize(w, _h) => {
                // Update display mode based on new width
                self.update_display_mode(*w);
            }
            Action::DiffReady(data) => self.load_diff(data),
            _ => {}
        }

        // Forward action to both viewers
        let _ = self.left_viewer.update(action.clone())?;
        let _ = self.right_viewer.update(action)?;

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        // Update display mode based on current width
        self.update_display_mode(area.width);

        // In single panel mode, the outer block drawn below already shows the panel name,
        // filename, and language in one header line - the inner CodeViewer's own border would
        // just repeat the filename and language a second time. Dual mode keeps each side's own
        // border (it's the only place the language shows at all there).
        let hide_inner_border = self.display_mode == DisplayMode::Single;
        self.left_viewer.set_hide_border(hide_inner_border);
        self.right_viewer.set_hide_border(hide_inner_border);

        // Update viewport heights based on current area. Dual mode nests two borders (the
        // "Before"/"After" panel_block, plus each CodeViewer's own) - 2 rows (top+bottom) each,
        // -4 total. Single mode now hides the inner border above, leaving only the outer block's
        // own 2 rows.
        let viewport_height =
            area.height
                .saturating_sub(if hide_inner_border { 2 } else { 4 }) as usize;
        self.left_viewer.set_viewport_height(viewport_height);
        self.right_viewer.set_viewport_height(viewport_height);

        if self.display_mode == DisplayMode::Dual {
            // Dual panel mode: show both side by side
            let divider_x = area.width / 2;
            let left_area = Rect::new(area.x, area.y, divider_x, area.height);
            let right_area = Rect::new(
                area.x + divider_x + 1,
                area.y,
                area.width - divider_x - 1,
                area.height,
            );

            let left_filename = self.left_viewer.filename_or_hint();
            let left_block = panel_block(
                " Before ",
                Color::Red,
                &left_filename,
                self.active_panel == Panel::Before,
            );
            let left_inner = left_block.inner(left_area);
            frame.render_widget(left_block, left_area);
            self.left_viewer.draw(frame, left_inner)?;

            let right_filename = self.right_viewer.filename_or_hint();
            let right_block = panel_block(
                " After ",
                Color::Green,
                &right_filename,
                self.active_panel == Panel::After,
            );
            let right_inner = right_block.inner(right_area);
            frame.render_widget(right_block, right_area);
            self.right_viewer.draw(frame, right_inner)?;

            let focused_inner = match self.active_panel {
                Panel::Before => left_inner,
                Panel::After => right_inner,
            };
            if let Some((x, y)) = self.focused_viewer().cursor_screen_position(focused_inner) {
                frame.set_cursor(x, y);
            }
        } else {
            // Single panel mode: show only one panel at a time
            // Determine border color based on active panel
            let border_color = match self.active_panel {
                Panel::Before => Color::Red,
                Panel::After => Color::Green,
            };

            let panel_name = match self.active_panel {
                Panel::Before => " Before ",
                Panel::After => " After ",
            };

            let filename = self.active_filename();
            let language = self.active_language();

            // Draw active panel with title showing filename and language
            let block = Block::default()
                .title(Line::from(vec![
                    Span::styled(panel_name, Style::new().bold().fg(border_color)),
                    Span::raw(" - "),
                    Span::styled(&filename, Style::new().bold().fg(Color::Cyan)),
                    Span::raw(" - "),
                    Span::styled(&language, Style::new().fg(Color::Gray)),
                    Span::raw(" (Tab to switch)"),
                ]))
                .borders(Borders::ALL)
                .border_set(border::ROUNDED)
                .border_style(Style::new().fg(border_color));

            let inner = block.inner(area);
            frame.render_widget(block, area);
            self.focused_viewer().draw(frame, inner)?;

            if let Some((x, y)) = self.focused_viewer().cursor_screen_position(inner) {
                frame.set_cursor(x, y);
            }
        }

        Ok(())
    }
}

/// Build a dual-mode panel's border block, showing a thicker border and a bold title on
/// whichever side is active so `Tab` (and therefore `o`'s file-selector target) is visible.
fn panel_block<'a>(name: &'static str, color: Color, filename: &str, active: bool) -> Block<'a> {
    let title_style = if active {
        Style::new().bold().fg(Color::Black).bg(color)
    } else {
        Style::new().bold().fg(color)
    };
    Block::default()
        .title(Line::from(vec![
            Span::styled(name, title_style),
            Span::raw(format!(" {filename}")),
        ]))
        .borders(Borders::ALL)
        .border_set(if active {
            border::THICK
        } else {
            border::ROUNDED
        })
        .border_style(Style::new().fg(color))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_diff_data() -> DiffSessionData {
        DiffSessionData {
            before_path: PathBuf::from("before.txt"),
            after_path: PathBuf::from("after.txt"),
            before_contents: "before".to_string(),
            after_contents: "after".to_string(),
            before_ranges: Vec::new(),
            after_ranges: Vec::new(),
        }
    }

    /// Regression test for the exploratory-testing bug: after `Tab` moves focus to "After",
    /// exactly the "After" side shows its own cursor highlight and the "Before" side shows only
    /// the pushed cross-highlight - never the other way around, and never both blues on one
    /// panel at once.
    #[test]
    fn tab_moves_focus_exclusively_to_the_other_panel() {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&sample_diff_data());

        assert!(viewer.left_viewer.state().is_focused);
        assert!(!viewer.right_viewer.state().is_focused);

        viewer.toggle_active_panel();

        assert!(
            !viewer.left_viewer.state().is_focused,
            "Before must lose focus once Tab moves it to After"
        );
        assert!(
            viewer.right_viewer.state().is_focused,
            "After must gain focus after Tab"
        );

        // Toggling back must restore focus to "Before" alone.
        viewer.toggle_active_panel();
        assert!(viewer.left_viewer.state().is_focused);
        assert!(!viewer.right_viewer.state().is_focused);
    }

    /// Test that moving the cursor on the active side moves the inactive side's cursor to
    /// follow the matched leaf node.
    #[test]
    fn moving_cursor_on_active_side_moves_inactive_side_cursor_to_matched_node() {
        use crate::diff::text::{RangeMatch, TextOperation};
        use crate::diff::text_range::TextRange;

        let mut viewer = DiffViewer::new();

        // Create sample diff data with two ranges
        let data = DiffSessionData {
            before_path: PathBuf::from("before.txt"),
            after_path: PathBuf::from("after.txt"),
            before_contents: "abc\ndef\nghi".to_string(),
            after_contents: "ABC\nDEF\nGHI".to_string(),
            before_ranges: vec![
                RangeMatch {
                    source: TextRange::new(0, 0, 0, 3),
                    destination: TextRange::new(0, 0, 0, 3),
                    operation: TextOperation::Update,
                },
                RangeMatch {
                    source: TextRange::new(1, 0, 1, 3),
                    destination: TextRange::new(1, 0, 1, 3),
                    operation: TextOperation::Update,
                },
            ],
            after_ranges: vec![
                RangeMatch {
                    source: TextRange::new(0, 0, 0, 3),
                    destination: TextRange::new(0, 0, 0, 3),
                    operation: TextOperation::Update,
                },
                RangeMatch {
                    source: TextRange::new(1, 0, 1, 3),
                    destination: TextRange::new(1, 0, 1, 3),
                    operation: TextOperation::Update,
                },
            ],
        };

        viewer.load_diff(&data);

        // Initially, cursor should be on first range (0, 0)
        assert_eq!(viewer.left_viewer.state().cursor_row, 0);
        assert_eq!(viewer.left_viewer.state().cursor_col, 0);

        // The right side's cursor should also be at (0, 0) to follow the matched node
        assert_eq!(viewer.right_viewer.state().cursor_row, 0);
        assert_eq!(viewer.right_viewer.state().cursor_col, 0);

        // Move cursor down on left side
        viewer.move_cursor_vertical(1);

        // Left cursor should now be on row 1
        assert_eq!(viewer.left_viewer.state().cursor_row, 1);

        // Right cursor should follow to the matched destination (row 1, col 0)
        assert_eq!(viewer.right_viewer.state().cursor_row, 1);
        assert_eq!(viewer.right_viewer.state().cursor_col, 0);
    }

    /// `n`/`p` (`jump_to_change`) must skip straight over unchanged lines to the next/previous
    /// real change, and - like every other cursor movement - push the result onto the other
    /// panel's cross-highlight so both sides stay in sync.
    #[test]
    fn jump_to_change_skips_unchanged_lines_and_syncs_the_other_panel() {
        use crate::diff::text::{RangeMatch, TextOperation};
        use crate::diff::text_range::TextRange;

        let mut viewer = DiffViewer::new();
        let data = DiffSessionData {
            before_path: PathBuf::from("before.txt"),
            after_path: PathBuf::from("after.txt"),
            before_contents: "same0\nsame1\nchanged\nsame3\nsame4".to_string(),
            after_contents: "same0\nsame1\nCHANGED\nsame3\nsame4".to_string(),
            before_ranges: vec![
                RangeMatch {
                    source: TextRange::new(0, 0, 2, 0),
                    destination: TextRange::new(0, 0, 2, 0),
                    operation: TextOperation::Identical,
                },
                RangeMatch {
                    source: TextRange::new(2, 0, 2, 7),
                    destination: TextRange::new(2, 0, 2, 7),
                    operation: TextOperation::Update,
                },
                RangeMatch {
                    source: TextRange::new(2, 7, 5, 0),
                    destination: TextRange::new(2, 7, 5, 0),
                    operation: TextOperation::Identical,
                },
            ],
            after_ranges: vec![
                RangeMatch {
                    source: TextRange::new(0, 0, 2, 0),
                    destination: TextRange::new(0, 0, 2, 0),
                    operation: TextOperation::Identical,
                },
                RangeMatch {
                    source: TextRange::new(2, 0, 2, 7),
                    destination: TextRange::new(2, 0, 2, 7),
                    operation: TextOperation::Update,
                },
                RangeMatch {
                    source: TextRange::new(2, 7, 5, 0),
                    destination: TextRange::new(2, 7, 5, 0),
                    operation: TextOperation::Identical,
                },
            ],
        };
        viewer.load_diff(&data);

        // load_diff already places the cursor on the first (only) change, so back it off first.
        viewer.left_viewer.set_cursor_position(0, 0);

        viewer.jump_to_change(true);
        assert_eq!(
            viewer.left_viewer.state().cursor_row,
            2,
            "should land on the changed line"
        );
        assert_eq!(
            viewer.right_viewer.state().cursor_row,
            2,
            "the other panel's cursor should follow to the matched destination"
        );

        // Only one change exists, so jumping forward again must wrap back to the same spot.
        viewer.jump_to_change(true);
        assert_eq!(viewer.left_viewer.state().cursor_row, 2);

        viewer.jump_to_change(false);
        assert_eq!(viewer.left_viewer.state().cursor_row, 2);
    }
}
