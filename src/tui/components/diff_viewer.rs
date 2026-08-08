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
    /// panel's cross-highlight, same as any other cursor movement - but centers *both* panels'
    /// viewports on the destination (see `CodeViewer::jump_to_change`/`sync_scroll_centered`)
    /// rather than the usual minimal "keep visible" scroll: jumping between scattered changes
    /// benefits from full context around the target on both sides, not just the focused one.
    pub fn jump_to_change(&mut self, forward: bool) {
        self.focused_viewer().jump_to_change(forward);
        self.sync_cross_highlight();
        self.sync_scroll_centered();
    }

    /// `n`/`p`'s actual key handler: jumps normally if the focused panel has a change to jump
    /// to. If it doesn't, but the *other* panel does (e.g. this is the "before" side of a diff
    /// that's pure insertions), that's `Action::NoChangesPromptNeeded` instead of a silent no-op,
    /// so `App` shows `NoChangesDialog` (see its own doc comment) - confirming it calls
    /// `jump_to_change_on_other_panel` below. If neither panel has any changes at all, there is
    /// genuinely nothing to offer, so this stays a no-op exactly as before.
    fn jump_to_change_or_prompt(&mut self, forward: bool) -> Option<Action> {
        if self.focused_viewer().change_count_and_index().is_some() {
            self.jump_to_change(forward);
            return Some(Action::Render);
        }
        if self.other_viewer().change_count_and_index().is_some() {
            return Some(Action::NoChangesPromptNeeded {
                forward,
                empty_panel: self.active_panel,
            });
        }
        Some(Action::Render)
    }

    /// Confirmed via `NoChangesDialog`: switch to the other panel (which has changes - the
    /// dialog wouldn't have been offered otherwise) and jump `forward`/backward on it, same as a
    /// normal `n`/`p` jump once there.
    pub fn jump_to_change_on_other_panel(&mut self, forward: bool) {
        self.toggle_active_panel();
        self.jump_to_change(forward);
    }

    /// Search the focused panel for `query`, replacing any previous search, and jump its cursor to
    /// the nearest match - what pressing Enter in the search modal does, then push the resulting
    /// matched node onto the other panel's cross-highlight like any other cursor movement.
    pub fn search(&mut self, query: &str) {
        self.focused_viewer().search(query);
        self.sync_cross_highlight();
        self.sync_scroll();
    }

    /// Move the focused panel's cursor to the next (`forward = true`) or previous (`forward =
    /// false`) search match (`>`/`<`), and sync the cross-highlight/scroll same as any other
    /// cursor movement.
    pub fn jump_to_search_match(&mut self, forward: bool) {
        self.focused_viewer().jump_to_search_match(forward);
        self.sync_cross_highlight();
        self.sync_scroll();
    }

    /// The focused panel's total search matches and how many are at or before the cursor - shown
    /// in `app.rs`'s footer, in place of `change N/M`, while a search is active.
    pub fn focused_search_match_count_and_index(&self) -> Option<(usize, usize)> {
        let viewer = match self.active_panel {
            Panel::Before => &self.left_viewer,
            Panel::After => &self.right_viewer,
        };
        viewer.search_match_count_and_index()
    }

    /// Push the focused panel's current cursor destination onto the other panel's
    /// cross-highlight, and move the other panel's cursor to follow the matched leaf node;
    /// call after anything that can change the cursor or the focused panel. The cursor always
    /// follows, but the highlight itself is suppressed when the focused side sits on an
    /// `Identical` match - see `cursor_destination_for_highlight`.
    fn sync_cross_highlight(&mut self) {
        let destination = self.focused_viewer().cursor_destination();
        let highlight_destination = self.focused_viewer().cursor_destination_for_highlight();
        self.other_viewer()
            .set_highlight_destination(highlight_destination);

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

    /// Same as `sync_scroll`, but centers the inactive panel's viewport on the destination row
    /// instead of just keeping it minimally visible - see `CodeViewer::scroll_to_center_row` and
    /// `jump_to_change` (`n`/`p`), this method's only caller.
    fn sync_scroll_centered(&mut self) {
        if let Some(dest) = self.focused_viewer().cursor_destination() {
            self.other_viewer().scroll_to_center_row(dest.start_row);
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

    /// The focused panel's cursor position (0-indexed row, col), or `None` if that panel has no
    /// file loaded yet - shown in `app.rs`'s footer line.
    pub fn focused_cursor_position(&self) -> Option<(usize, usize)> {
        let viewer = match self.active_panel {
            Panel::Before => &self.left_viewer,
            Panel::After => &self.right_viewer,
        };
        if viewer.line_count() == 0 {
            return None;
        }
        let state = viewer.state();
        Some((state.cursor_row, state.cursor_col))
    }

    /// The focused panel's total distinct changes and how many are at or before the cursor - see
    /// `CodeViewer::change_count_and_index` - shown in `app.rs`'s footer line after `n`/`p`.
    pub fn focused_change_count_and_index(&self) -> Option<(usize, usize)> {
        let viewer = match self.active_panel {
            Panel::Before => &self.left_viewer,
            Panel::After => &self.right_viewer,
        };
        viewer.change_count_and_index()
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

    /// Get the filename of the active viewer, or a hint to press `o` if it has no file loaded yet
    /// - matches dual-panel mode's own title bar, which uses the same fallback (see `draw`).
    fn active_filename(&self) -> String {
        match self.active_panel {
            Panel::Before => self.left_viewer.filename_or_hint(),
            Panel::After => self.right_viewer.filename_or_hint(),
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
            let (left_area, right_area) = split_panels(area);

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
            // time regardless of what's there. See `jump_to_change_or_prompt`'s doc comment for
            // what happens when the focused panel has no changes to jump to at all.
            crossterm::event::KeyCode::Char('n') => Ok(self.jump_to_change_or_prompt(true)),
            crossterm::event::KeyCode::Char('p') => Ok(self.jump_to_change_or_prompt(false)),
            // >/< jump the cursor between search matches (see the `/` search modal), the same
            // wrap-around convention as n/p - a distinct pair rather than overloading n/p, since
            // help_modal.rs already documents those as change-navigation specifically.
            crossterm::event::KeyCode::Char('>') => {
                self.jump_to_search_match(true);
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Char('<') => {
                self.jump_to_search_match(false);
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
            _ => Ok(None),
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
            let (left_area, right_area) = split_panels(area);

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

/// Splits a dual-panel area into (left, right) halves with a one-column gap for the divider,
/// shared by [`DiffViewer::init`] (sizing viewports before anything is drawn) and
/// [`DiffViewer::draw`] (the actual layout) so the two can never silently diverge.
fn split_panels(area: Rect) -> (Rect, Rect) {
    let divider = area.width / 2;
    let left_area = Rect::new(area.x, area.y, divider, area.height);
    let right_area = Rect::new(
        area.x + divider + 1,
        area.y,
        area.width - divider - 1,
        area.height,
    );
    (left_area, right_area)
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
            comment_only: false,
            mode: crate::diff::DiffMode::Fast,
            plain_text_fallback: false,
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

    #[test]
    fn focused_cursor_position_is_none_before_any_file_is_loaded() {
        let viewer = DiffViewer::new();
        assert_eq!(viewer.focused_cursor_position(), None);
    }

    #[test]
    fn focused_cursor_position_reflects_the_active_panels_cursor() {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&sample_diff_data());

        viewer.focused_viewer().set_cursor_position(0, 3);
        assert_eq!(viewer.focused_cursor_position(), Some((0, 3)));

        // Tab moves focus to "After" - the footer should now follow that panel's cursor instead.
        viewer.toggle_active_panel();
        viewer.focused_viewer().set_cursor_position(0, 2);
        assert_eq!(viewer.focused_cursor_position(), Some((0, 2)));
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
            comment_only: false,
            mode: crate::diff::DiffMode::Fast,
            plain_text_fallback: false,
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
            comment_only: false,
            mode: crate::diff::DiffMode::Fast,
            plain_text_fallback: false,
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

    /// `n`/`p` must center *both* panels' viewports on the matched change, not just the focused
    /// one (`sync_scroll_centered`) - otherwise the other panel could show its matched content
    /// pinned awkwardly at the very top/bottom of the viewport even though the focused side is
    /// nicely centered.
    #[test]
    fn jump_to_change_centers_both_panels() {
        use crate::diff::text::{RangeMatch, TextOperation};
        use crate::diff::text_range::TextRange;

        let line_count = 100;
        let change_row = 50;
        let contents: String = (0..line_count).map(|i| format!("line{i}\n")).collect();
        let ranges = vec![
            RangeMatch {
                source: TextRange::new(0, 0, change_row, 0),
                destination: TextRange::new(0, 0, change_row, 0),
                operation: TextOperation::Identical,
            },
            RangeMatch {
                source: TextRange::new(change_row, 0, change_row, 4),
                destination: TextRange::new(change_row, 0, change_row, 4),
                operation: TextOperation::Update,
            },
            RangeMatch {
                source: TextRange::new(change_row + 1, 0, line_count, 0),
                destination: TextRange::new(change_row + 1, 0, line_count, 0),
                operation: TextOperation::Identical,
            },
        ];

        let mut viewer = DiffViewer::new();
        let data = DiffSessionData {
            before_path: PathBuf::from("before.txt"),
            after_path: PathBuf::from("after.txt"),
            before_contents: contents.clone(),
            after_contents: contents,
            before_ranges: ranges.clone(),
            after_ranges: ranges,
            comment_only: false,
            mode: crate::diff::DiffMode::Fast,
            plain_text_fallback: false,
        };
        viewer.load_diff(&data);
        viewer.left_viewer.set_viewport_height(10);
        viewer.right_viewer.set_viewport_height(10);
        viewer.left_viewer.set_cursor_position(0, 0);

        viewer.jump_to_change(true);

        assert_eq!(viewer.left_viewer.state().scroll, 45);
        assert_eq!(
            viewer.right_viewer.state().scroll,
            45,
            "the other panel should center on its matched destination the same way"
        );
    }

    /// A diff of pure insertions: the "before" side is 100% `Identical` (nothing was removed or
    /// changed there), so its `n`/`p` history is empty even though the "after" side has a real
    /// change. This is the exact scenario `Action::NoChangesPromptNeeded` exists for.
    fn pure_insertion_diff_data() -> DiffSessionData {
        use crate::diff::text::{RangeMatch, TextOperation};
        use crate::diff::text_range::TextRange;

        DiffSessionData {
            before_path: PathBuf::from("before.txt"),
            after_path: PathBuf::from("after.txt"),
            before_contents: "a\nb\nc\n".to_string(),
            after_contents: "a\nb\nc\nd\n".to_string(),
            before_ranges: vec![
                RangeMatch {
                    source: TextRange::new(0, 0, 3, 0),
                    destination: TextRange::new(0, 0, 3, 0),
                    operation: TextOperation::Identical,
                },
                RangeMatch {
                    // `source` is *this side's own* position - zero-width here since there is no
                    // "before" location for an insertion. `change_positions` filters zero-width
                    // ranges out via `source.is_empty()`, which is exactly why the "before" panel
                    // has zero navigable changes despite this entry existing at all.
                    source: TextRange::new(3, 0, 3, 0),
                    destination: TextRange::new(3, 0, 4, 0),
                    operation: TextOperation::Insert,
                },
            ],
            after_ranges: vec![
                RangeMatch {
                    source: TextRange::new(0, 0, 3, 0),
                    destination: TextRange::new(0, 0, 3, 0),
                    operation: TextOperation::Identical,
                },
                RangeMatch {
                    // `source` is *this side's own* position - real and non-zero-width here
                    // (the new "d" line genuinely exists in `after_contents`), so this one *is* a
                    // navigable change on the "after" side.
                    source: TextRange::new(3, 0, 4, 0),
                    destination: TextRange::new(3, 0, 3, 0),
                    operation: TextOperation::Insert,
                },
            ],
            comment_only: false,
            mode: crate::diff::DiffMode::Fast,
            plain_text_fallback: false,
        }
    }

    /// `n`/`p` on the empty "before" side of a pure-insertion diff must not silently do nothing
    /// (the old, confusing behavior) - it should ask `App` to offer switching to "after" instead.
    #[test]
    fn jump_to_change_prompts_when_focused_panel_has_no_changes_but_the_other_does() {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&pure_insertion_diff_data());
        assert_eq!(viewer.active_panel, Panel::Before);

        let action = viewer.jump_to_change_or_prompt(true);
        assert_eq!(
            action,
            Some(Action::NoChangesPromptNeeded {
                forward: true,
                empty_panel: Panel::Before,
            })
        );
        // Declining (or not yet confirming) must not have moved anything.
        assert_eq!(viewer.active_panel, Panel::Before);
    }

    /// The same prompt, triggered by `p` instead of `n` - the direction must be carried through
    /// unchanged so confirming replays the right one.
    #[test]
    fn jump_to_change_prompt_carries_the_backward_direction_too() {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&pure_insertion_diff_data());

        let action = viewer.jump_to_change_or_prompt(false);
        assert_eq!(
            action,
            Some(Action::NoChangesPromptNeeded {
                forward: false,
                empty_panel: Panel::Before,
            })
        );
    }

    /// Once the *other* panel is focused (e.g. after `Tab`), it's the one with the real changes,
    /// so `n`/`p` there must jump normally - no prompt.
    #[test]
    fn jump_to_change_does_not_prompt_when_the_focused_panel_has_changes() {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&pure_insertion_diff_data());
        viewer.toggle_active_panel(); // focus "after", which has the real change

        let action = viewer.jump_to_change_or_prompt(true);
        assert_eq!(action, Some(Action::Render));
        assert_eq!(
            viewer.right_viewer.state().cursor_row,
            3,
            "should have jumped normally"
        );
    }

    /// Neither side has any changes at all (e.g. two identical files) - genuinely nothing to
    /// offer, so this must stay the same silent no-op it always was, not a pointless prompt.
    #[test]
    fn jump_to_change_does_not_prompt_when_neither_panel_has_changes() {
        let mut viewer = DiffViewer::new();
        let mut data = sample_diff_data();
        data.before_contents = "same\n".to_string();
        data.after_contents = "same\n".to_string();
        viewer.load_diff(&data);

        let action = viewer.jump_to_change_or_prompt(true);
        assert_eq!(action, Some(Action::Render));
    }

    /// Confirming the prompt (`Action::NoChangesPromptConfirmed`, handled by `App` calling this)
    /// must switch focus to the other panel and land on its change, in one step.
    #[test]
    fn jump_to_change_on_other_panel_switches_focus_and_jumps() {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&pure_insertion_diff_data());
        assert_eq!(viewer.active_panel, Panel::Before);

        viewer.jump_to_change_on_other_panel(true);

        assert_eq!(viewer.active_panel, Panel::After);
        assert_eq!(viewer.right_viewer.state().cursor_row, 3);
        assert!(viewer.right_viewer.state().is_focused);
        assert!(!viewer.left_viewer.state().is_focused);
    }

    /// `search` (the `/` modal's Enter) must operate on the focused panel and, like every other
    /// cursor movement, sync the resulting position onto the other panel's cross-highlight.
    #[test]
    fn search_jumps_the_focused_panel_and_syncs_the_other_panel() {
        let mut viewer = DiffViewer::new();
        let mut data = sample_diff_data();
        data.before_contents = "foo\nbar\nfoo bar\n".to_string();
        data.after_contents = "xyz\n".to_string();
        viewer.load_diff(&data);

        viewer.search("bar");
        assert_eq!(viewer.left_viewer.state().cursor_row, 1);
        assert_eq!(viewer.focused_search_match_count_and_index(), Some((1, 2)));
    }

    /// `>`/`<` (`jump_to_search_match`) step through matches on the focused panel and wrap around,
    /// same convention as `n`/`p`.
    #[test]
    fn jump_to_search_match_steps_through_matches_on_the_focused_panel() {
        let mut viewer = DiffViewer::new();
        let mut data = sample_diff_data();
        data.before_contents = "bar\nfoo\nbar\n".to_string();
        viewer.load_diff(&data);

        viewer.search("bar");
        assert_eq!(viewer.left_viewer.state().cursor_row, 0);

        viewer.jump_to_search_match(true);
        assert_eq!(viewer.left_viewer.state().cursor_row, 2);

        viewer.jump_to_search_match(true);
        assert_eq!(
            viewer.left_viewer.state().cursor_row,
            0,
            "forward past the last match should wrap to the first"
        );
    }

    #[test]
    fn focused_search_match_count_and_index_is_none_before_any_search() {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&sample_diff_data());
        assert_eq!(viewer.focused_search_match_count_and_index(), None);
    }
}
