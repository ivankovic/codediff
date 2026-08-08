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
use ratatui::{Frame, layout::Rect};
use tokio::sync::mpsc::UnboundedSender;

use super::Component;
use crate::diff::text::RangeMatch;
use crate::diff::text_range::TextRange;
use crate::tui::actions::Action;
use crate::tui::theme::OverlayTheme;

/// A component that displays source code
///
/// This component wraps the CodeViewerWidget and manages its state.
/// It implements the Component trait for integration with the TUI architecture.
#[derive(Default)]
pub struct CodeViewer {
    /// The underlying widget
    widget: crate::tui::widgets::code_viewer::CodeViewerWidget,
    /// The widget state (scroll position and viewport)
    state: crate::tui::widgets::code_viewer::CodeViewerState,
    /// Action sender
    command_tx: Option<UnboundedSender<Action>>,
}

impl CodeViewer {
    /// Create a new CodeViewer
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a CodeViewer for a specific file
    pub fn with_file(path: PathBuf) -> Self {
        Self {
            widget: crate::tui::widgets::code_viewer::CodeViewerWidget::with_file(path),
            ..Default::default()
        }
    }

    /// Load a file into the viewer
    pub fn load_file(&mut self, path: PathBuf) -> Result<()> {
        self.widget.load_file(path)?;
        self.reset_state();
        Ok(())
    }

    /// Load already-read contents into the viewer (no filesystem access).
    pub fn load_contents(&mut self, path: PathBuf, contents: String) {
        self.widget.load_contents(path, contents);
        self.reset_state();
    }

    /// Reset scroll/diff/cursor state, e.g. after loading a new file.
    fn reset_state(&mut self) {
        self.state.scroll = 0;
        self.state.load_ranges(Vec::new());
    }

    /// Get the total number of lines
    pub fn line_count(&self) -> usize {
        self.widget.line_count()
    }

    /// Scroll up by one line
    pub fn scroll_up(&mut self) {
        if self.state.scroll > 0 {
            self.state.scroll = self.state.scroll.saturating_sub(1);
        }
    }

    /// Scroll down by one line
    pub fn scroll_down(&mut self) {
        let total = self.line_count();
        if self.state.scroll < total.saturating_sub(1) {
            self.state.scroll = self.state.scroll.saturating_add(1);
        }
    }

    /// Scroll to a specific line
    pub fn scroll_to(&mut self, line: usize) {
        self.state.scroll = line.min(self.line_count().saturating_sub(1));
    }

    /// Set the diff ranges for this side (as returned by `TextDiff::all`), and place the cursor
    /// on the first navigable (non zero-width) range.
    pub fn set_ranges(&mut self, ranges: Vec<RangeMatch>) {
        self.state.load_ranges(ranges);
        self.scroll_to_cursor();
    }

    /// The destination range matched to wherever the cursor currently sits, i.e. the range to
    /// cross-highlight on the other panel.
    pub fn cursor_destination(&self) -> Option<TextRange> {
        self.state.cursor_destination()
    }

    /// Set (or clear) the cross-highlighted range coming from the other panel's cursor.
    pub fn set_highlight_destination(&mut self, destination: Option<TextRange>) {
        self.state.highlight_destination = destination;
    }

    /// Mark whether this side's cursor is the one currently driving navigation; see
    /// `CodeViewerState::is_focused`.
    pub fn set_focused(&mut self, focused: bool) {
        self.state.is_focused = focused;
    }

    /// Set the palette used to paint the diff/cursor overlay, picked via the `c` theme picker.
    pub fn set_overlay_theme(&mut self, theme: OverlayTheme) {
        self.widget.set_overlay_theme(theme);
    }

    /// Enable or disable the cross-highlight blue paint, toggled via `x` - see
    /// `CodeViewerWidget::set_cross_highlight_enabled`'s doc comment.
    pub fn set_cross_highlight_enabled(&mut self, enabled: bool) {
        self.widget.set_cross_highlight_enabled(enabled);
    }

    /// See `CodeViewerWidget`'s `hide_border` field.
    pub fn set_hide_border(&mut self, hide: bool) {
        self.widget.set_hide_border(hide);
    }

    /// Move the cursor up (`direction < 0`) or down (`direction > 0`) by one line, clamping the
    /// column to the new line's length, and scroll to keep it visible.
    pub fn move_cursor_vertical(&mut self, direction: i32) {
        let total_lines = self.line_count();
        if total_lines == 0 || direction == 0 {
            return;
        }
        let new_row = (self.state.cursor_row as isize + direction.signum() as isize)
            .clamp(0, total_lines as isize - 1) as usize;
        self.state.cursor_row = new_row;
        self.state.cursor_col = self.state.cursor_col.min(self.widget.line_len(new_row));
        self.scroll_to_cursor();
    }

    /// Move the cursor left (`direction < 0`) or right (`direction > 0`) by one character,
    /// wrapping to the end of the previous line / start of the next line at row boundaries, like
    /// a normal text cursor.
    pub fn move_cursor_horizontal(&mut self, direction: i32) {
        if direction == 0 {
            return;
        }
        if direction < 0 {
            if self.state.cursor_col > 0 {
                self.state.cursor_col -= 1;
            } else if self.state.cursor_row > 0 {
                self.state.cursor_row -= 1;
                self.state.cursor_col = self.widget.line_len(self.state.cursor_row);
            }
        } else {
            let line_len = self.widget.line_len(self.state.cursor_row);
            if self.state.cursor_col < line_len {
                self.state.cursor_col += 1;
            } else if self.state.cursor_row + 1 < self.line_count() {
                self.state.cursor_row += 1;
                self.state.cursor_col = 0;
            }
        }
        self.scroll_to_cursor();
    }

    /// Move the cursor to the start of the next (`forward = true`) or previous (`forward =
    /// false`) actual change, wrapping around at the ends - see `CodeViewerState::
    /// next_change_position`. A no-op if this side has no changes at all.
    pub fn jump_to_change(&mut self, forward: bool) {
        if let Some((row, col)) = self.state.next_change_position(forward) {
            self.set_cursor_position(row, col);
        }
    }

    /// Total distinct changes and how many are at or before the cursor - see
    /// `CodeViewerState::change_count_and_index`.
    pub fn change_count_and_index(&self) -> Option<(usize, usize)> {
        self.state.change_count_and_index()
    }

    /// Search this file for case-insensitive occurrences of `query`, replacing any previous
    /// search, and jump the cursor to the nearest match at or after the current position (wrapping
    /// to the first match otherwise) - what pressing Enter in the search modal does. Clears any
    /// existing highlighted matches with no jump if `query` matches nothing (including an empty
    /// query).
    pub fn search(&mut self, query: &str) {
        self.state.search_matches = self.widget.find_matches(query);
        if let Some((row, col)) = self.state.nearest_search_match_position() {
            self.set_cursor_position(row, col);
        }
    }

    /// Move the cursor to the next (`forward = true`) or previous (`forward = false`) search
    /// match (`>`/`<`) - see `CodeViewerState::next_search_match_position`. A no-op if there's no
    /// active search.
    pub fn jump_to_search_match(&mut self, forward: bool) {
        if let Some((row, col)) = self.state.next_search_match_position(forward) {
            self.set_cursor_position(row, col);
        }
    }

    /// Total current search matches and how many are at or before the cursor - see
    /// `CodeViewerState::search_match_count_and_index`.
    pub fn search_match_count_and_index(&self) -> Option<(usize, usize)> {
        self.state.search_match_count_and_index()
    }

    /// Set the cursor to a specific (row, column) position, clamping to valid bounds,
    /// and scroll to keep it visible. Used to synchronize the inactive panel's cursor
    /// to match the active panel's cursor destination.
    pub fn set_cursor_position(&mut self, row: usize, col: usize) {
        let total_lines = self.line_count();
        if total_lines == 0 {
            return;
        }
        let clamped_row = row.min(total_lines.saturating_sub(1));
        let line_len = self.widget.line_len(clamped_row);
        let clamped_col = col.min(line_len);
        self.state.cursor_row = clamped_row;
        self.state.cursor_col = clamped_col;
        self.scroll_to_cursor();
    }

    /// Where the cursor should be drawn on screen within `area` (the same area passed to
    /// `draw`), accounting for the widget's own border and the current scroll position. `None`
    /// if the cursor's row is scrolled out of view, or its column is past the (not horizontally
    /// scrollable) visible width.
    pub fn cursor_screen_position(&self, area: Rect) -> Option<(u16, u16)> {
        let inner = self.widget.inner_area(area);
        let row_in_viewport = self.state.cursor_row.checked_sub(self.state.scroll)?;
        if row_in_viewport >= inner.height as usize || self.state.cursor_col >= inner.width as usize
        {
            return None;
        }
        Some((
            inner.x + self.state.cursor_col as u16,
            inner.y + row_in_viewport as u16,
        ))
    }

    /// Scroll the viewport so the cursor's row is visible.
    fn scroll_to_cursor(&mut self) {
        self.scroll_to_show_row(self.state.cursor_row);
    }

    /// Get the filename for display
    pub fn filename(&self) -> String {
        self.widget.filename()
    }

    /// The filename, or a hint to press `o` if no file is loaded into this panel yet.
    pub fn filename_or_hint(&self) -> String {
        if self.widget.has_file() {
            self.widget.filename()
        } else {
            "(press 'o' to open a file)".to_string()
        }
    }

    /// Get the language name for display
    pub fn language_name(&self) -> String {
        self.widget.language_name()
    }

    /// Scroll the viewport to keep `row` visible, without moving the cursor.
    pub fn scroll_to_show_row(&mut self, row: usize) {
        let row = row.min(self.line_count().saturating_sub(1));
        if row < self.state.scroll {
            self.state.scroll = row;
        } else if self.state.viewport_height > 0
            && row >= self.state.scroll + self.state.viewport_height
        {
            self.state.scroll = row.saturating_sub(self.state.viewport_height - 1);
        }
    }

    /// Get the viewport height
    pub fn viewport_height(&self) -> usize {
        self.state.viewport_height
    }

    /// Set the viewport height
    pub fn set_viewport_height(&mut self, height: usize) {
        self.state.viewport_height = height;
    }

    /// Get reference to the state
    pub fn state(&self) -> &crate::tui::widgets::code_viewer::CodeViewerState {
        &self.state
    }
}

impl Component for CodeViewer {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.command_tx = Some(tx);
        Ok(())
    }

    fn init(&mut self, area: Rect) -> Result<()> {
        self.state.viewport_height = area.height.saturating_sub(2) as usize; // -2 for borders
        Ok(())
    }

    // No `handle_key_event` override: `DiffViewer::handle_key_event` (the only place a
    // `CodeViewer` is ever constructed) intercepts every key this component used to handle
    // itself - arrows/hjkl via `move_cursor_vertical`/`move_cursor_horizontal`, PageUp/PageDown/
    // Home/End via `scroll_up`/`scroll_down`/`scroll_to` - before ever reaching the fallback that
    // forwards to `left_viewer`/`right_viewer`. Those arms were dead code (verified: nothing
    // constructs a bare `CodeViewer` outside `diff_viewer.rs`, and no test called this method
    // directly), so this now just falls through to `Component`'s default `Ok(None)`.

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Tick => {}
            Action::Render => {}
            Action::Resize(_w, h) => {
                // Update viewport height
                self.state.viewport_height = h.saturating_sub(2) as usize;
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        // Use the StatefulWidget render method via frame.render_stateful_widget
        // We need to pass self.widget by value, but we can't move out of self
        // So we use a reference and implement StatefulWidget for &CodeViewerWidget
        frame.render_stateful_widget(&self.widget, area, &mut self.state);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewer_with(contents: &str) -> CodeViewer {
        let mut viewer = CodeViewer::new();
        viewer.load_contents(PathBuf::from("test.txt"), contents.to_string());
        viewer
    }

    #[test]
    fn move_cursor_horizontal_right_advances_within_a_line() {
        let mut viewer = viewer_with("abc\ndef\n");
        viewer.move_cursor_horizontal(1);
        assert_eq!((viewer.state.cursor_row, viewer.state.cursor_col), (0, 1));
    }

    #[test]
    fn move_cursor_horizontal_right_wraps_to_the_start_of_the_next_line() {
        let mut viewer = viewer_with("abc\ndef\n");
        viewer.set_cursor_position(0, 3); // end of "abc"
        viewer.move_cursor_horizontal(1);
        assert_eq!(
            (viewer.state.cursor_row, viewer.state.cursor_col),
            (1, 0),
            "moving right past the end of a line must land on column 0 of the next line"
        );
    }

    #[test]
    fn move_cursor_horizontal_right_is_a_no_op_at_the_very_end_of_the_file() {
        let mut viewer = viewer_with("abc\ndef");
        let last_row = viewer.line_count() - 1;
        let last_col = viewer.widget.line_len(last_row);
        viewer.set_cursor_position(last_row, last_col);
        viewer.move_cursor_horizontal(1);
        assert_eq!(
            (viewer.state.cursor_row, viewer.state.cursor_col),
            (last_row, last_col),
            "there is no next line to wrap to at the end of the file"
        );
    }

    #[test]
    fn move_cursor_horizontal_left_retreats_within_a_line() {
        let mut viewer = viewer_with("abc\ndef\n");
        viewer.set_cursor_position(0, 2);
        viewer.move_cursor_horizontal(-1);
        assert_eq!((viewer.state.cursor_row, viewer.state.cursor_col), (0, 1));
    }

    #[test]
    fn move_cursor_horizontal_left_wraps_to_the_end_of_the_previous_line() {
        let mut viewer = viewer_with("abc\ndef\n");
        viewer.set_cursor_position(1, 0);
        viewer.move_cursor_horizontal(-1);
        assert_eq!(
            (viewer.state.cursor_row, viewer.state.cursor_col),
            (0, 3),
            "moving left past column 0 must land at the end of the previous line"
        );
    }

    #[test]
    fn move_cursor_horizontal_left_is_a_no_op_at_the_very_start_of_the_file() {
        let mut viewer = viewer_with("abc\ndef\n");
        viewer.move_cursor_horizontal(-1);
        assert_eq!(
            (viewer.state.cursor_row, viewer.state.cursor_col),
            (0, 0),
            "there is no previous line to wrap to at the start of the file"
        );
    }

    #[test]
    fn move_cursor_horizontal_with_zero_direction_is_a_no_op() {
        let mut viewer = viewer_with("abc\ndef\n");
        viewer.set_cursor_position(0, 1);
        viewer.move_cursor_horizontal(0);
        assert_eq!((viewer.state.cursor_row, viewer.state.cursor_col), (0, 1));
    }

    #[test]
    fn search_jumps_the_cursor_to_the_first_match_at_or_after_the_cursor() {
        let mut viewer = viewer_with("foo\nbar\nfoo bar\n");
        viewer.search("bar");
        assert_eq!((viewer.state.cursor_row, viewer.state.cursor_col), (1, 0));
    }

    #[test]
    fn search_with_no_matches_leaves_the_cursor_untouched() {
        let mut viewer = viewer_with("foo\nbar\n");
        viewer.set_cursor_position(1, 1);
        viewer.search("xyz");
        assert_eq!((viewer.state.cursor_row, viewer.state.cursor_col), (1, 1));
        assert_eq!(viewer.search_match_count_and_index(), None);
    }

    #[test]
    fn jump_to_search_match_steps_forward_and_wraps() {
        let mut viewer = viewer_with("bar\nfoo\nbar\n");
        viewer.search("bar");
        assert_eq!((viewer.state.cursor_row, viewer.state.cursor_col), (0, 0));

        viewer.jump_to_search_match(true);
        assert_eq!((viewer.state.cursor_row, viewer.state.cursor_col), (2, 0));

        viewer.jump_to_search_match(true);
        assert_eq!(
            (viewer.state.cursor_row, viewer.state.cursor_col),
            (0, 0),
            "forward past the last match should wrap to the first"
        );
    }

    #[test]
    fn search_match_count_and_index_reflects_the_active_search() {
        let mut viewer = viewer_with("bar\nfoo\nbar\n");
        assert_eq!(
            viewer.search_match_count_and_index(),
            None,
            "no active search yet"
        );

        viewer.search("bar");
        assert_eq!(viewer.search_match_count_and_index(), Some((1, 2)));

        viewer.jump_to_search_match(true);
        assert_eq!(viewer.search_match_count_and_index(), Some((2, 2)));
    }
}
