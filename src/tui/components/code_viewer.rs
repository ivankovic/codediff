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
    /// The column `move_cursor_vertical` (up/down) tries to return to on each move, remembered
    /// across a run of vertical moves even while an intervening short/indented line forces
    /// `cursor_col` somewhere else - the "sticky column" most text editors implement, so
    /// `k k k j j j` lands back where it started instead of drifting left. `None` means "not
    /// sticky yet" (any other cursor movement resets it - see `clamp_and_set_cursor`).
    desired_col: Option<usize>,
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
        self.desired_col = None;
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
        self.desired_col = None;
        self.scroll_to_cursor();
    }

    /// The destination range matched to wherever the cursor currently sits, i.e. the range the
    /// other panel's cursor should follow.
    pub fn cursor_destination(&self) -> Option<TextRange> {
        self.state.cursor_destination()
    }

    /// Same as `cursor_destination`, but `None` for an `Identical` match - the range to
    /// cross-highlight on the other panel (unchanged content isn't highlighted there).
    pub fn cursor_destination_for_highlight(&self) -> Option<TextRange> {
        self.state.cursor_destination_for_highlight()
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

    /// See `CodeViewerWidget`'s `hide_border` field.
    pub fn set_hide_border(&mut self, hide: bool) {
        self.widget.set_hide_border(hide);
    }

    /// Move the cursor up (`direction < 0`) or down (`direction > 0`) by one line, keeping it on
    /// the same "sticky" column across a run of vertical moves (see `desired_col`'s doc comment)
    /// rather than the destination row's actual column - except where that column doesn't land
    /// on real content on the new line: past the last non-whitespace character, it's pulled back
    /// to it; before the first (i.e. in the line's own indentation), it's pushed forward to it.
    /// Scrolls to keep the cursor visible afterward.
    pub fn move_cursor_vertical(&mut self, direction: i32) {
        let total_lines = self.line_count();
        if total_lines == 0 || direction == 0 {
            return;
        }
        let target_col = *self.desired_col.get_or_insert(self.state.cursor_col);

        let new_row = (self.state.cursor_row as isize + direction.signum() as isize)
            .clamp(0, total_lines as isize - 1) as usize;
        self.state.cursor_row = new_row;
        self.state.cursor_col =
            clamp_to_non_whitespace(target_col, &self.widget.line_text(new_row));
        self.scroll_to_cursor();
    }

    /// Move the cursor left (`direction < 0`) or right (`direction > 0`) by one character,
    /// wrapping to the end of the previous line / start of the next line at row boundaries, like
    /// a normal text cursor. Resets `desired_col` - an explicit horizontal move always means the
    /// user wants *this* column from now on, not whatever `move_cursor_vertical` was last sticky
    /// on.
    pub fn move_cursor_horizontal(&mut self, direction: i32) {
        if direction == 0 {
            return;
        }
        self.desired_col = None;
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
    /// next_change_position`. Unlike other cursor movement (which only scrolls the minimum
    /// needed to keep the cursor visible - see `scroll_to_cursor`), this centers the destination
    /// row in the viewport, clamped to the start/end of the file when centering would scroll
    /// past either edge: jumping between scattered changes across a large file benefits from
    /// full context around the target, not just "barely on screen" at whichever edge it
    /// happened to enter from. A no-op if this side has no changes at all.
    pub fn jump_to_change(&mut self, forward: bool) {
        if let Some((row, col)) = self.state.next_change_position(forward)
            && self.clamp_and_set_cursor(row, col)
        {
            self.scroll_to_center_row(self.state.cursor_row);
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

    /// Clamp `(row, col)` to valid bounds and move the cursor there, without touching scroll.
    /// Returns `false` (and does nothing else) if the file is empty. Shared by
    /// `set_cursor_position` (keeps the cursor merely visible afterward) and `jump_to_change`
    /// (centers it - see that method's own doc comment for why the two need different scroll
    /// behavior). Resets `desired_col` - every caller of this is an explicit reposition (search,
    /// change navigation, cross-panel cursor sync), not `move_cursor_vertical`'s own sticky
    /// column, so a later vertical move should start fresh from wherever this lands.
    fn clamp_and_set_cursor(&mut self, row: usize, col: usize) -> bool {
        let total_lines = self.line_count();
        if total_lines == 0 {
            return false;
        }
        let clamped_row = row.min(total_lines.saturating_sub(1));
        let line_len = self.widget.line_len(clamped_row);
        let clamped_col = col.min(line_len);
        self.state.cursor_row = clamped_row;
        self.state.cursor_col = clamped_col;
        self.desired_col = None;
        true
    }

    /// Set the cursor to a specific (row, column) position, clamping to valid bounds,
    /// and scroll to keep it visible. Used to synchronize the inactive panel's cursor
    /// to match the active panel's cursor destination.
    pub fn set_cursor_position(&mut self, row: usize, col: usize) {
        if self.clamp_and_set_cursor(row, col) {
            self.scroll_to_cursor();
        }
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

    /// Scroll the viewport so `row` is vertically centered, without moving the cursor - unlike
    /// `scroll_to_show_row`, this always re-centers rather than only scrolling when `row` would
    /// otherwise fall out of view. Clamped to the start/end of the file (`max_scroll`) when
    /// centering `row` would scroll past either edge, so a change near the very first or last
    /// line still pins the viewport there instead of leaving dead space above/below the file.
    /// Used by `jump_to_change` (`n`/`p`) and `DiffViewer::sync_scroll_centered`, which centers
    /// the *other* panel on its matched destination the same way.
    pub fn scroll_to_center_row(&mut self, row: usize) {
        let total_lines = self.line_count();
        if total_lines == 0 || self.state.viewport_height == 0 {
            return;
        }
        let row = row.min(total_lines.saturating_sub(1));
        let half_viewport = self.state.viewport_height / 2;
        let max_scroll = total_lines.saturating_sub(self.state.viewport_height);
        self.state.scroll = row.saturating_sub(half_viewport).min(max_scroll);
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

/// The `[first, last)` non-whitespace column bounds of `line` (character indices; `last` is one
/// past the last non-whitespace character, i.e. itself a valid cursor column - a cursor there
/// sits immediately after that character, same convention as `line_len`/`cursor_col`), or `None`
/// if `line` is empty or entirely whitespace, in which case there's no content to snap to.
fn non_whitespace_bounds(line: &str) -> Option<(usize, usize)> {
    let mut first = None;
    let mut last = None;
    for (i, c) in line.chars().enumerate() {
        if !c.is_whitespace() {
            first.get_or_insert(i);
            last = Some(i + 1);
        }
    }
    first.zip(last)
}

/// Clamp `col` into `line`'s non-whitespace bounds (see `non_whitespace_bounds`) - used by
/// `move_cursor_vertical` so a "sticky" column never lands inside a line's leading indentation
/// or past its actual content into trailing whitespace. Falls back to the plain `[0, line_len]`
/// clamp on an empty/all-whitespace line, where there's nothing to snap to.
fn clamp_to_non_whitespace(col: usize, line: &str) -> usize {
    match non_whitespace_bounds(line) {
        Some((first, last)) => col.clamp(first, last),
        None => col.min(line.chars().count()),
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
    fn non_whitespace_bounds_finds_the_first_and_last_non_whitespace_columns() {
        assert_eq!(non_whitespace_bounds("  foo bar  "), Some((2, 9)));
        assert_eq!(non_whitespace_bounds("foo"), Some((0, 3)));
        assert_eq!(non_whitespace_bounds(""), None);
        assert_eq!(non_whitespace_bounds("    "), None, "all-whitespace line");
    }

    #[test]
    fn clamp_to_non_whitespace_pulls_back_from_trailing_whitespace() {
        assert_eq!(clamp_to_non_whitespace(20, "  foo  "), 5);
    }

    #[test]
    fn clamp_to_non_whitespace_pushes_forward_from_leading_whitespace() {
        assert_eq!(clamp_to_non_whitespace(0, "    foo"), 4);
    }

    #[test]
    fn clamp_to_non_whitespace_leaves_a_column_within_bounds_untouched() {
        assert_eq!(clamp_to_non_whitespace(5, "  foo bar  "), 5);
    }

    #[test]
    fn clamp_to_non_whitespace_falls_back_to_the_plain_clamp_on_an_all_whitespace_line() {
        assert_eq!(clamp_to_non_whitespace(10, "     "), 5);
        assert_eq!(clamp_to_non_whitespace(10, ""), 0);
    }

    /// The defining "sticky column" case: a column that survives an intervening short/indented
    /// line and reappears once a line wide enough for it comes back around - not just "clamp to
    /// whatever the immediately previous line allowed".
    #[test]
    fn move_cursor_vertical_remembers_the_desired_column_across_a_shorter_line() {
        let mut viewer = viewer_with("hello world\nhi\nhello again\n");
        viewer.set_cursor_position(0, 8); // inside "world"

        viewer.move_cursor_vertical(1); // onto "hi" (len 2) - column must clamp down
        assert_eq!(viewer.state.cursor_col, 2);

        viewer.move_cursor_vertical(1); // onto "hello again" - long enough for column 8 again
        assert_eq!(
            viewer.state.cursor_col, 8,
            "should return to the original desired column, not stay clamped at 2"
        );
    }

    #[test]
    fn move_cursor_vertical_clamps_past_the_end_of_line_to_the_last_non_whitespace_column() {
        let mut viewer = viewer_with("hello world\nfoo   \n");
        viewer.set_cursor_position(0, 10); // near the end of "hello world"

        viewer.move_cursor_vertical(1); // onto "foo   " (trailing whitespace)
        assert_eq!(
            viewer.state.cursor_col, 3,
            "should land right after 'foo', not on the trailing whitespace"
        );
    }

    #[test]
    fn move_cursor_vertical_clamps_before_the_first_non_whitespace_column() {
        let mut viewer = viewer_with("x\n    indented\n");
        viewer.set_cursor_position(0, 0);

        viewer.move_cursor_vertical(1); // onto "    indented"
        assert_eq!(
            viewer.state.cursor_col, 4,
            "should land on the 'i' of 'indented', not inside the leading whitespace"
        );
    }

    #[test]
    fn move_cursor_vertical_does_not_panic_on_an_all_whitespace_line() {
        let mut viewer = viewer_with("hello\n   \nworld\n");
        viewer.set_cursor_position(0, 4);

        viewer.move_cursor_vertical(1); // onto the all-whitespace line
        assert_eq!(
            viewer.state.cursor_col, 3,
            "plain clamp: min(4, line_len=3)"
        );

        viewer.move_cursor_vertical(1); // onto "world" - the original desired column (4) returns
        assert_eq!(viewer.state.cursor_col, 4);
    }

    #[test]
    fn move_cursor_horizontal_resets_the_sticky_column() {
        let mut viewer = viewer_with("hello world\nhi\nhello again\n");
        viewer.set_cursor_position(0, 8);
        viewer.move_cursor_vertical(1); // sticky column now 8, clamped to 2 (end of "hi")
        viewer.move_cursor_horizontal(-1); // explicit horizontal move: now col 1 on "hi"

        viewer.move_cursor_vertical(1); // onto "hello again" - should use the new column (1)
        assert_eq!(
            viewer.state.cursor_col, 1,
            "an explicit horizontal move should replace the old sticky column, not the reverse"
        );
    }

    #[test]
    fn set_cursor_position_resets_the_sticky_column() {
        let mut viewer = viewer_with("hello world\nhi\nhello again\n");
        viewer.set_cursor_position(0, 8);
        viewer.move_cursor_vertical(1); // sticky column now 8, clamped to 2 on "hi"
        viewer.set_cursor_position(1, 1); // explicit reposition, still on "hi"

        viewer.move_cursor_vertical(1); // onto "hello again" - should use the new column (1)
        assert_eq!(viewer.state.cursor_col, 1);
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

    /// Builds a `line_count`-line file with an `Update` range (a "change") at each row in
    /// `change_rows`, `Identical` elsewhere - just enough range structure for
    /// `next_change_position`/`jump_to_change` to navigate between changes, for the centering
    /// tests below.
    fn viewer_with_changes_at(line_count: usize, change_rows: &[usize]) -> CodeViewer {
        use crate::diff::text::TextOperation;

        let contents: String = (0..line_count).map(|i| format!("line{i}\n")).collect();
        let mut viewer = viewer_with(&contents);

        let mut ranges = Vec::new();
        let mut row = 0;
        for &change_row in change_rows {
            if change_row > row {
                ranges.push(RangeMatch {
                    source: TextRange::new(row, 0, change_row, 0),
                    destination: TextRange::new(row, 0, change_row, 0),
                    operation: TextOperation::Identical,
                });
            }
            ranges.push(RangeMatch {
                source: TextRange::new(change_row, 0, change_row, 4),
                destination: TextRange::new(change_row, 0, change_row, 4),
                operation: TextOperation::Update,
            });
            row = change_row + 1;
        }
        if row < line_count {
            ranges.push(RangeMatch {
                source: TextRange::new(row, 0, line_count, 0),
                destination: TextRange::new(row, 0, line_count, 0),
                operation: TextOperation::Identical,
            });
        }
        viewer.set_ranges(ranges);
        viewer
    }

    /// `n`/`p` (`jump_to_change`) must *center* the destination row, not just scroll the minimum
    /// needed to keep it visible: a change already inside the viewport should still cause a
    /// re-center, unlike `move_cursor_vertical`/other movement (`scroll_to_show_row`).
    #[test]
    fn jump_to_change_centers_the_destination_row() {
        let mut viewer = viewer_with_changes_at(100, &[50]);
        viewer.set_viewport_height(10);
        viewer.scroll_to_show_row(0); // destination (50) is already technically "visible"-adjacent

        viewer.jump_to_change(true);

        assert_eq!(viewer.state.cursor_row, 50);
        assert_eq!(
            viewer.state.scroll, 45,
            "row 50 centered in a 10-row viewport should scroll to 50 - 10/2 = 45"
        );
    }

    /// A change near the very start of the file can't be centered without scrolling above line
    /// 0 - the viewport should pin to the start instead of leaving dead space above the file.
    #[test]
    fn jump_to_change_clamps_to_the_start_of_the_file() {
        let mut viewer = viewer_with_changes_at(100, &[2]);
        viewer.set_viewport_height(10);
        viewer.scroll_to_show_row(50); // start far from the destination

        viewer.jump_to_change(true);

        assert_eq!(viewer.state.cursor_row, 2);
        assert_eq!(viewer.state.scroll, 0);
    }

    /// A change near the very end of the file can't be centered without scrolling past the last
    /// line - the viewport should pin to the end instead of leaving dead space below the file.
    #[test]
    fn jump_to_change_clamps_to_the_end_of_the_file() {
        let mut viewer = viewer_with_changes_at(100, &[97]);
        viewer.set_viewport_height(10);

        viewer.jump_to_change(true);

        assert_eq!(viewer.state.cursor_row, 97);
        assert_eq!(
            viewer.state.scroll, 90,
            "a 100-line file with a 10-row viewport can scroll no further than row 90"
        );
    }
}
