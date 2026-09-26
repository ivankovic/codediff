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
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */
use std::path::PathBuf;

use anyhow::Result;
use ratatui::{Frame, layout::Rect};
use tokio::sync::mpsc::UnboundedSender;

use super::Component;
use crate::diff::text::{RangeMatch, TextOperation};
use crate::diff::text_range::{TextRange, floor_char_boundary};
use crate::tui::actions::Action;
use crate::tui::display_columns;
use crate::tui::theme::OverlayTheme;

/// One code panel: the `CodeViewerWidget` plus its cursor, scroll and range state.
#[derive(Default)]
pub struct CodeViewer {
    widget: crate::tui::widgets::code_viewer::CodeViewerWidget,
    state: crate::tui::widgets::code_viewer::CodeViewerState,
    command_tx: Option<UnboundedSender<Action>>,
    /// The editor-style "sticky column" `move_cursor_vertical` returns to across a run of
    /// vertical moves, so `k k k j j j` does not drift left. A display column, so the cursor keeps
    /// its place on screen across rows indented differently. Any other cursor movement clears it.
    desired_col: Option<usize>,
}

impl CodeViewer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_file(&mut self, path: PathBuf) -> Result<()> {
        self.widget.load_file(path)?;
        self.reset_state();
        Ok(())
    }

    /// Load already-read contents, without filesystem access.
    pub fn load_contents(&mut self, path: PathBuf, contents: String) {
        self.widget.load_contents(path, contents);
        self.reset_state();
    }

    fn reset_state(&mut self) {
        self.state.scroll = 0;
        self.state.scroll_col = 0;
        self.state.load_ranges(Vec::new());
        self.desired_col = None;
    }

    pub fn line_count(&self) -> usize {
        self.widget.line_count()
    }

    pub fn scroll_up(&mut self) {
        if self.state.scroll > 0 {
            self.state.scroll = self.state.scroll.saturating_sub(1);
        }
    }

    pub fn scroll_down(&mut self) {
        let total = self.line_count();
        if self.state.scroll < total.saturating_sub(1) {
            self.state.scroll = self.state.scroll.saturating_add(1);
        }
    }

    pub fn scroll_to(&mut self, line: usize) {
        self.state.scroll = line.min(self.line_count().saturating_sub(1));
    }

    /// Set this side's ranges (as returned by `TextDiff::all`) and place the cursor on the first
    /// non-zero-width range.
    pub fn set_ranges(&mut self, ranges: Vec<RangeMatch>) {
        self.state.load_ranges(ranges);
        self.desired_col = None;
        self.scroll_to_cursor();
    }

    /// Swap the ranges, keeping the cursor and scroll where they are - see
    /// `CodeViewerState::replace_ranges`.
    pub fn replace_ranges(&mut self, ranges: Vec<RangeMatch>) {
        let line_count = self.line_count();
        self.state.replace_ranges(ranges, line_count);
        self.desired_col = None;
    }

    pub fn ranges(&self) -> &[RangeMatch] {
        &self.state.ranges
    }

    /// The destination range matched to the cursor: where the other panel's cursor follows.
    pub fn cursor_destination(&self) -> Option<TextRange> {
        self.state.cursor_destination()
    }

    /// Like `cursor_destination`, but `None` for an `Identical` match, which is not
    /// cross-highlighted.
    pub fn cursor_destination_for_highlight(&self) -> Option<TextRange> {
        self.state.cursor_destination_for_highlight()
    }

    pub fn set_highlight_destination(&mut self, destination: Option<TextRange>) {
        self.state.highlight_destination = destination;
    }

    pub fn set_syntax_theme(&mut self, name: String) {
        self.widget.set_theme(name);
    }

    /// The `H` toggle; see `CodeViewerState::node_highlight` for why it defaults to off.
    pub fn is_node_highlight_enabled(&self) -> bool {
        self.state.node_highlight
    }

    /// Painting only: the cursor follows its counterpart either way.
    pub fn set_node_highlight(&mut self, enable: bool) {
        self.state.node_highlight = enable;
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.state.is_focused = focused;
    }

    pub fn set_overlay_theme(&mut self, theme: OverlayTheme) {
        self.widget.set_overlay_theme(theme);
    }

    pub fn is_syntax_highlighting_enabled(&self) -> bool {
        self.widget.is_syntax_highlighting_enabled()
    }

    pub fn set_syntax_highlighting(&mut self, enable: bool) {
        if enable {
            self.widget.enable_syntax_highlighting();
        } else {
            self.widget.disable_syntax_highlighting();
        }
    }

    /// Move one line up (`direction < 0`) or down, on the sticky `desired_col` clamped into the
    /// new line's non-whitespace content.
    pub fn move_cursor_vertical(&mut self, direction: i32) {
        let total_lines = self.line_count();
        if total_lines == 0 || direction == 0 {
            return;
        }
        let cursor_display_col = self.cursor_display_col();
        let target_col = *self.desired_col.get_or_insert(cursor_display_col);

        let new_row = (self.state.cursor_row as isize + direction.signum() as isize)
            .clamp(0, total_lines as isize - 1) as usize;
        let line = self.widget.line_text(new_row);
        self.state.cursor_row = new_row;
        self.state.cursor_col = clamp_to_non_whitespace(
            display_columns::byte_column_at_display(&line, target_col),
            &line,
        );
        self.scroll_to_cursor();
    }

    /// Move one character left (`direction < 0`) or right, wrapping across line ends. Resets
    /// `desired_col`.
    pub fn move_cursor_horizontal(&mut self, direction: i32) {
        if direction == 0 {
            return;
        }
        self.desired_col = None;
        let line = self.widget.line_text(self.state.cursor_row);
        let col = floor_char_boundary(&line, self.state.cursor_col);
        if direction < 0 {
            if let Some(previous) = line[..col].chars().next_back() {
                self.state.cursor_col = col - previous.len_utf8();
            } else if self.state.cursor_row > 0 {
                self.state.cursor_row -= 1;
                self.state.cursor_col = self.widget.line_len(self.state.cursor_row);
            }
        } else if let Some(next) = line[col..].chars().next() {
            self.state.cursor_col = col + next.len_utf8();
        } else if self.state.cursor_row + 1 < self.line_count() {
            self.state.cursor_row += 1;
            self.state.cursor_col = 0;
        }
        self.scroll_to_cursor();
    }

    /// Replace the search with case-insensitive `query` and jump to the nearest match at or
    /// after the cursor, wrapping. No match leaves the cursor where it is.
    pub fn search(&mut self, query: &str) {
        self.state.search_matches = self.widget.find_matches(query);
        if let Some((row, col)) = self.state.nearest_search_match_position() {
            self.set_cursor_position(row, col);
        }
    }

    /// Highlight `query`'s matches without moving the cursor; returns the match count.
    pub fn preview_search(&mut self, query: &str) -> usize {
        self.state.search_matches = self.widget.find_matches(query);
        self.state.search_matches.len()
    }

    pub fn jump_to_search_match(&mut self, forward: bool) {
        if let Some((row, col)) = self.state.next_search_match_position(forward) {
            self.set_cursor_position(row, col);
        }
    }

    pub fn search_match_count_and_index(&self) -> Option<(usize, usize)> {
        self.state.search_match_count_and_index()
    }

    /// Clamp and move the cursor without scrolling; `false` on an empty file. `col` is a byte
    /// column, rounded down to a character boundary. Resets `desired_col`, since every caller is
    /// an explicit reposition.
    fn clamp_and_set_cursor(&mut self, row: usize, col: usize) -> bool {
        let total_lines = self.line_count();
        if total_lines == 0 {
            return false;
        }
        let clamped_row = row.min(total_lines.saturating_sub(1));
        let line = self.widget.line_text(clamped_row);
        self.state.cursor_row = clamped_row;
        self.state.cursor_col = floor_char_boundary(&line, col);
        self.desired_col = None;
        true
    }

    /// Clamp and move the cursor to byte column `col` of `row` - a range's or a search hit's
    /// column - scrolling only as far as needed to show it.
    pub fn set_cursor_position(&mut self, row: usize, col: usize) {
        if self.clamp_and_set_cursor(row, col) {
            self.scroll_to_cursor();
        }
    }

    /// Like `set_cursor_position`, for a display column: the character drawn there, as for a
    /// mouse click.
    pub fn set_cursor_at_display_col(&mut self, row: usize, display_col: usize) {
        let row = row.min(self.line_count().saturating_sub(1));
        let line = self.widget.line_text(row);
        let col = display_columns::byte_column_at_display(&line, display_col);
        self.set_cursor_position(row, col);
    }

    /// The cursor's display column: where on its row it is drawn.
    fn cursor_display_col(&self) -> usize {
        let line = self.widget.line_text(self.state.cursor_row);
        display_columns::display_column(&line, self.state.cursor_col)
    }

    /// The cursor's `(row, character column)`, as the footer's `Ln`/`Col` counts.
    pub fn cursor_character_position(&self) -> (usize, usize) {
        let line = self.widget.line_text(self.state.cursor_row);
        (
            self.state.cursor_row,
            display_columns::char_column(&line, self.state.cursor_col),
        )
    }

    /// The cursor's screen cell within `area` (the area passed to `draw`), or `None` when it is
    /// scrolled out of view.
    pub fn cursor_screen_position(&self, area: Rect) -> Option<(u16, u16)> {
        let row_in_viewport = self.state.cursor_row.checked_sub(self.state.scroll)?;
        let col_in_viewport = self
            .cursor_display_col()
            .checked_sub(self.state.scroll_col)?;
        let gutter = self.widget.gutter_width();
        if row_in_viewport >= area.height as usize
            || gutter + col_in_viewport >= area.width as usize
        {
            return None;
        }
        Some((
            area.x + (gutter + col_in_viewport) as u16,
            area.y + row_in_viewport as u16,
        ))
    }

    fn scroll_to_cursor(&mut self) {
        self.scroll_to_show_row(self.state.cursor_row);
        self.scroll_to_show_col(self.cursor_display_col());
    }

    /// Scroll horizontally to show display column `col`. A no-op until the first frame sets
    /// `viewport_width`.
    fn scroll_to_show_col(&mut self, col: usize) {
        let width = self.state.viewport_width;
        if width == 0 {
            return;
        }
        if col < self.state.scroll_col {
            self.state.scroll_col = col;
        } else if col >= self.state.scroll_col + width {
            self.state.scroll_col = col + 1 - width;
        }
    }

    pub fn filename_or_hint(&self) -> String {
        if self.widget.has_file() {
            self.widget.filename()
        } else {
            "(press 'o' to open a file)".to_string()
        }
    }

    pub fn language_name(&self) -> String {
        self.widget.language_name()
    }

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

    /// Center `row` without moving the cursor, clamped so no dead space shows past either end
    /// of the file.
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

    pub fn viewport_height(&self) -> usize {
        self.state.viewport_height
    }

    pub fn set_viewport_height(&mut self, height: usize) {
        self.state.viewport_height = height;
    }

    pub fn state(&self) -> &crate::tui::widgets::code_viewer::CodeViewerState {
        &self.state
    }

    pub fn gutter_width(&self) -> usize {
        self.widget.gutter_width()
    }

    /// The minimap cells: the file's rows split into `bands` equal slices, each holding the
    /// highest-`band_priority` change touching it, or `None`.
    pub fn change_bands(&self, bands: usize) -> Vec<Option<TextOperation>> {
        let total = self.line_count();
        let mut out = vec![None; bands];
        if bands == 0 || total == 0 {
            return out;
        }
        for rm in &self.state.ranges {
            if matches!(
                rm.operation,
                TextOperation::Identical | TextOperation::NotYetSet
            ) || rm.source.is_empty()
            {
                continue;
            }
            let last_row = if rm.source.end_column == 0 {
                rm.source.end_row.saturating_sub(1)
            } else {
                rm.source.end_row
            }
            .min(total - 1);
            for row in rm.source.start_row..=last_row {
                let band = (row * bands / total).min(bands - 1);
                let current = &mut out[band];
                if current
                    .as_ref()
                    .is_none_or(|existing| band_priority(&rm.operation) > band_priority(existing))
                {
                    *current = Some(rm.operation.clone());
                }
            }
        }
        out
    }
}

/// Which operation wins a minimap band touched by several kinds of change. The order is
/// arbitrary but stable: a band is one cell, so something has to win.
fn band_priority(op: &TextOperation) -> u8 {
    match op {
        TextOperation::Delete => 4,
        TextOperation::Insert => 3,
        TextOperation::Update => 2,
        TextOperation::Move => 1,
        TextOperation::Identical | TextOperation::NotYetSet => 0,
    }
}

/// The `[first, last)` byte columns of `line`'s non-whitespace content, or `None` for a blank
/// line. `last` is itself a valid cursor column.
fn non_whitespace_bounds(line: &str) -> Option<(usize, usize)> {
    let mut first = None;
    let mut last = None;
    for (i, c) in line.char_indices() {
        if !c.is_whitespace() {
            first.get_or_insert(i);
            last = Some(i + c.len_utf8());
        }
    }
    first.zip(last)
}

/// Byte column `col`, a character boundary, pulled into `line`'s non-whitespace content.
fn clamp_to_non_whitespace(col: usize, line: &str) -> usize {
    match non_whitespace_bounds(line) {
        Some((first, last)) => col.clamp(first, last),
        None => col.min(line.len()),
    }
}

impl Component for CodeViewer {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.command_tx = Some(tx);
        Ok(())
    }

    fn init(&mut self, area: Rect) -> Result<()> {
        // -1 for `DiffViewer`'s title row. Only the first frame uses this; `draw` resets it.
        self.state.viewport_height = area.height.saturating_sub(1) as usize;
        Ok(())
    }

    // No `handle_key_event`: `DiffViewer`, the only owner, handles every key itself.

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Tick => {}
            Action::Render => {}
            Action::Resize(_w, h) => {
                self.state.viewport_height = h.saturating_sub(2) as usize;
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
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
    fn move_cursor_horizontal_steps_over_a_multi_byte_character_in_one_press() {
        let mut viewer = viewer_with("é漢x\n");
        viewer.move_cursor_horizontal(1);
        assert_eq!(viewer.state.cursor_col, 2, "past the two-byte é");
        viewer.move_cursor_horizontal(1);
        assert_eq!(viewer.state.cursor_col, 5, "past the three-byte 漢");
        viewer.move_cursor_horizontal(-1);
        viewer.move_cursor_horizontal(-1);
        assert_eq!(viewer.state.cursor_col, 0);
    }

    /// The sticky column is where the cursor is drawn, so it keeps its place on screen across a
    /// tab-indented and a space-indented row.
    #[test]
    fn move_cursor_vertical_keeps_the_display_column_across_tab_and_space_indentation() {
        let mut viewer = viewer_with("\tfoo\n    bar\n");
        viewer.set_cursor_position(0, 2); // the second `o`, drawn in column 5

        viewer.move_cursor_vertical(1);
        assert_eq!(
            viewer.state.cursor_col, 5,
            "the `a` of `bar`, also column 5"
        );

        viewer.move_cursor_vertical(-1);
        assert_eq!(viewer.state.cursor_col, 2);
    }

    #[test]
    fn cursor_screen_position_is_in_display_columns_on_a_tab_indented_row() {
        let mut viewer = viewer_with("\t\tx\n");
        viewer.set_viewport_height(10);
        viewer.set_cursor_position(0, 2);
        let area = Rect::new(3, 1, 40, 10);
        let gutter = viewer.gutter_width() as u16;
        assert_eq!(
            viewer.cursor_screen_position(area),
            Some((3 + gutter + 8, 1))
        );
    }

    #[test]
    fn horizontal_scroll_follows_the_cursor_in_display_columns() {
        let mut viewer = viewer_with("\t\t\t\tx\n");
        viewer.state.viewport_width = 5;
        viewer.set_cursor_position(0, 4); // `x`, drawn in column 16
        assert_eq!(viewer.state.scroll_col, 12);

        viewer.set_cursor_position(0, 1); // the second tab, from column 4
        assert_eq!(viewer.state.scroll_col, 4);
    }

    #[test]
    fn set_cursor_at_display_col_lands_on_the_character_drawn_there() {
        let mut viewer = viewer_with("\tab\n");
        viewer.set_cursor_at_display_col(0, 2);
        assert_eq!(viewer.state.cursor_col, 0, "inside the tab");
        viewer.set_cursor_at_display_col(0, 5);
        assert_eq!(viewer.state.cursor_col, 2, "the `b`");
        viewer.set_cursor_at_display_col(0, 40);
        assert_eq!(viewer.state.cursor_col, 3, "past the end");
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

    fn viewer_with_lines(line_count: usize) -> CodeViewer {
        viewer_with(
            &(0..line_count)
                .map(|i| format!("line{i}\n"))
                .collect::<String>(),
        )
    }

    #[test]
    fn scroll_to_center_row_centers_the_row() {
        let mut viewer = viewer_with_lines(100);
        viewer.set_viewport_height(10);

        viewer.scroll_to_center_row(50);

        assert_eq!(
            viewer.state.scroll, 45,
            "row 50 centered in a 10-row viewport should scroll to 50 - 10/2 = 45"
        );
    }

    #[test]
    fn scroll_to_center_row_clamps_to_the_start_of_the_file() {
        let mut viewer = viewer_with_lines(100);
        viewer.set_viewport_height(10);
        viewer.scroll_to_show_row(50); // start far from the destination

        viewer.scroll_to_center_row(2);

        assert_eq!(viewer.state.scroll, 0);
    }

    #[test]
    fn scroll_to_center_row_clamps_to_the_end_of_the_file() {
        let mut viewer = viewer_with_lines(100);
        viewer.set_viewport_height(10);

        viewer.scroll_to_center_row(97);

        assert_eq!(
            viewer.state.scroll, 90,
            "a 100-line file with a 10-row viewport can scroll no further than row 90"
        );
    }

    fn range(start_row: usize, end_row: usize, end_col: usize, op: TextOperation) -> RangeMatch {
        RangeMatch {
            source: TextRange::new(start_row, 0, end_row, end_col),
            destination: TextRange::new(start_row, 0, end_row, end_col),
            operation: op,
        }
    }

    #[test]
    fn change_bands_marks_each_band_a_change_touches_and_skips_identical_ranges() {
        let mut viewer = viewer_with(&(0..10).map(|i| format!("l{i}\n")).collect::<String>());
        viewer.set_ranges(vec![
            range(0, 2, 0, TextOperation::Identical),
            range(2, 2, 2, TextOperation::Update),
            range(3, 8, 0, TextOperation::Identical),
            range(8, 8, 2, TextOperation::Insert),
        ]);
        assert_eq!(
            viewer.change_bands(5),
            vec![
                None,
                Some(TextOperation::Update),
                None,
                None,
                Some(TextOperation::Insert)
            ]
        );
    }

    #[test]
    fn change_bands_does_not_count_the_row_a_range_ends_on_at_column_zero() {
        let mut viewer = viewer_with(&(0..4).map(|i| format!("l{i}\n")).collect::<String>());
        viewer.set_ranges(vec![range(0, 2, 0, TextOperation::Update)]);
        assert_eq!(
            viewer.change_bands(4),
            vec![
                Some(TextOperation::Update),
                Some(TextOperation::Update),
                None,
                None
            ]
        );
    }

    #[test]
    fn change_bands_shows_the_highest_priority_operation_in_a_shared_band() {
        let mut viewer = viewer_with("a\nb\nc\nd\n");
        viewer.set_ranges(vec![
            range(0, 0, 1, TextOperation::Move),
            range(1, 1, 1, TextOperation::Delete),
            range(2, 2, 1, TextOperation::Update),
        ]);
        assert_eq!(viewer.change_bands(1), vec![Some(TextOperation::Delete)]);
    }

    #[test]
    fn cursor_screen_position_is_none_once_the_cursor_scrolls_out_of_view() {
        let mut viewer = viewer_with(&(0..50).map(|i| format!("l{i}\n")).collect::<String>());
        viewer.set_viewport_height(10);
        let area = Rect::new(0, 0, 40, 10);
        assert!(viewer.cursor_screen_position(area).is_some());
        viewer.scroll_to_show_row(30);
        assert_eq!(viewer.cursor_screen_position(area), None);
    }
}
