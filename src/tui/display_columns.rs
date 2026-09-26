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

//! The one mapping between the byte columns the viewer stores and the display columns it draws.
//!
//! A file's text reaches every front end with its tabs intact, so every `TextRange` column, every
//! search hit and the TUI's cursor are byte offsets into the text as it is on disk
//! (`diff::text_range::SourceColumn`). Tabs become spaces only where a row is drawn: a tab reaches
//! the next multiple of [`TAB_WIDTH`], every other character its terminal cell width. Expanding
//! the text itself instead would move every byte after the first tab and invalidate the ranges.
//!
//! What is in which unit:
//!
//! * **bytes** - ranges, search hits, the cursor. Comparing any two of them needs no conversion.
//! * **display columns** - horizontal scroll, the viewport width, the terminal cursor's x, a mouse
//!   click, the sticky column of vertical movement: everything measured on screen. Converted from
//!   and to bytes here, and nowhere else.
//! * **characters** - only the footer's `Col`, via [`char_column`].
//!
//! The showcase's browser viewer (`assets/viewer/model.js`) keeps the same split in UTF-16 code
//! units, the unit a JavaScript string is indexed by, and receives `TAB_WIDTH` in its state.

use crate::diff::text_range::{cell_width_of, floor_char_boundary};

/// Columns between tab stops, everywhere a line is drawn: the TUI, headless output and the
/// showcase's browser viewer.
pub const TAB_WIDTH: usize = 4;

/// How many display columns `ch` takes when it starts at display column `column`.
pub fn char_display_width(ch: char, column: usize) -> usize {
    if ch == '\t' {
        TAB_WIDTH - column % TAB_WIDTH
    } else {
        cell_width_of(ch).get()
    }
}

/// The display column at which byte `byte_column` of `line` is drawn. A column inside a
/// multi-byte character, or past the end, is rounded down to a character boundary first.
pub fn display_column(line: &str, byte_column: usize) -> usize {
    let end = floor_char_boundary(line, byte_column);
    line[..end]
        .chars()
        .fold(0, |column, ch| column + char_display_width(ch, column))
}

/// How many display columns `line` takes.
pub fn display_width(line: &str) -> usize {
    display_column(line, line.len())
}

/// The byte column of the character drawn over display column `target` - a tab or a wide
/// character covers more than one - or `line.len()` past the end of the row.
pub fn byte_column_at_display(line: &str, target: usize) -> usize {
    let mut column = 0;
    for (index, ch) in line.char_indices() {
        let width = char_display_width(ch, column);
        if target < column + width {
            return index;
        }
        column += width;
    }
    line.len()
}

/// How many characters come before byte `byte_column` of `line`.
pub fn char_column(line: &str, byte_column: usize) -> usize {
    line[..floor_char_boundary(line, byte_column)]
        .chars()
        .count()
}

/// Appends `text` to `out` with every tab expanded, where `*column` is the display column `text`
/// starts at; advances `*column` past it. Pieces of one row appended in order line up with the
/// row expanded whole.
pub fn push_expanded(out: &mut String, text: &str, column: &mut usize) {
    for ch in text.chars() {
        let width = char_display_width(ch, *column);
        if ch == '\t' {
            out.extend(std::iter::repeat_n(' ', width));
        } else {
            out.push(ch);
        }
        *column += width;
    }
}

/// `line` with every tab expanded, for a row whose first character is at a tab stop.
pub fn expand_tabs(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    push_expanded(&mut out, line, &mut 0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tab_reaches_the_next_tab_stop() {
        assert_eq!(expand_tabs("\tx"), "    x");
        assert_eq!(expand_tabs("ab\tx"), "ab  x");
        assert_eq!(
            expand_tabs("abcd\tx"),
            "abcd    x",
            "at a stop, a tab is a whole stop"
        );
        assert_eq!(expand_tabs("\t\tx"), "        x");
    }

    #[test]
    fn display_column_counts_tab_stops_and_cell_widths_not_bytes() {
        let line = "\té漢x";
        assert_eq!(display_column(line, 0), 0);
        assert_eq!(display_column(line, 1), 4, "after the tab");
        assert_eq!(display_column(line, 3), 5, "after the two-byte é, one cell");
        assert_eq!(
            display_column(line, 6),
            7,
            "after the three-byte 漢, two cells"
        );
        assert_eq!(display_column(line, 7), 8);
        assert_eq!(
            display_column(line, 2),
            4,
            "inside é rounds down to its start"
        );
        assert_eq!(display_column(line, 99), 8, "past the end clamps");
        assert_eq!(display_width(line), 8);
    }

    #[test]
    fn byte_column_at_display_inverts_display_column_on_every_covered_cell() {
        let line = "\té漢x";
        for (display, byte) in [(0, 0), (3, 0), (4, 1), (5, 3), (6, 3), (7, 6), (8, 7)] {
            assert_eq!(
                byte_column_at_display(line, display),
                byte,
                "display column {display}"
            );
        }
        for byte in [0, 1, 3, 6, 7] {
            assert_eq!(
                byte_column_at_display(line, display_column(line, byte)),
                byte
            );
        }
    }

    #[test]
    fn char_column_counts_characters_before_a_byte_column() {
        assert_eq!(char_column("é = x", 5), 4);
        assert_eq!(char_column("\tx", 1), 1);
        assert_eq!(char_column("é", 1), 0, "inside é rounds down");
    }

    #[test]
    fn pieces_expanded_in_order_line_up_with_the_whole_row() {
        let line = "a\tbc\td";
        let mut out = String::new();
        let mut column = 0;
        push_expanded(&mut out, &line[..2], &mut column);
        push_expanded(&mut out, &line[2..], &mut column);
        assert_eq!(out, expand_tabs(line));
        assert_eq!(column, display_width(line));
    }
}
