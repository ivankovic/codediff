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
use tree_sitter::Range;

/// A row in the source document: the index of a `\n`-delimited line. Rows have one unit only,
/// so there is no byte/character/cell variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct SourceRow(usize);

/// A column in the source document: a **byte** offset within its row, not characters and not
/// terminal cells. Tree-sitter and every painted span in `human_mapping.json` use bytes, so
/// re-basing on characters would invalidate every span on a non-ASCII row. Get a row's length
/// from [`row_len_of`], never from a character count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct SourceColumn(usize);

/// An absolute **byte** offset into the whole file. Distinct from `SourceColumn`, which is also a
/// byte count, so "byte 9 of this row" cannot be mistaken for "byte 9 of this file".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct SourceOffset(usize);

/// A terminal **cell** offset within a screen row: a CJK ideograph is two cells, a combining mark
/// zero. Derived at the render boundary by [`screen_column_in`], never stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ScreenColumn(usize);

macro_rules! position_newtype {
    ($name:ident) => {
        impl $name {
            /// Verbose on purpose: each call asserts a unit rather than carrying one, and should
            /// be visible in review.
            pub const fn from_raw(value: usize) -> Self {
                Self(value)
            }

            pub const fn get(self) -> usize {
                self.0
            }
        }
    };
}

position_newtype!(SourceRow);
position_newtype!(SourceColumn);
position_newtype!(SourceOffset);
position_newtype!(ScreenColumn);

/// `line`'s length in **bytes**, the column one past its last character. Use this, never
/// `line.chars().count()`, which mis-highlights every non-ASCII row.
pub fn row_len_of(line: &str) -> SourceColumn {
    SourceColumn(line.len())
}

/// [`row_len_of`] without trailing whitespace: how far a renderer paints a row a range covers
/// without ending on, since end-of-line whitespace is never shown painted.
pub fn paint_row_len(line: &str) -> SourceColumn {
    row_len_of(line.trim_end())
}

/// `column` clamped to `line`'s length and rounded down to a character boundary, so a bad column
/// (a malformed painting, a stale range) can slice `line` without panicking. Every renderer that
/// slices a row by column should go through this.
pub fn floor_char_boundary(line: &str, column: usize) -> usize {
    let mut column = column.min(line.len());
    while column > 0 && !line.is_char_boundary(column) {
        column -= 1;
    }
    column
}

/// How many terminal cells the first `column` bytes of `line` occupy; the only conversion from
/// `SourceColumn` to `ScreenColumn`. A character the column lands inside counts for nothing, which
/// keeps the result monotonic.
pub fn screen_column_in(line: &str, column: SourceColumn) -> ScreenColumn {
    let mut cells = 0usize;
    for (index, ch) in line.char_indices() {
        if index + ch.len_utf8() > column.get() {
            break;
        }
        cells += cell_width_of(ch).get();
    }
    ScreenColumn(cells)
}

/// How many terminal cells `ch` occupies. All width arithmetic, wrapping included, goes through
/// this.
pub fn cell_width_of(ch: char) -> ScreenColumn {
    use unicode_width::UnicodeWidthChar;
    ScreenColumn(ch.width().unwrap_or(0))
}

pub fn row_cells_of(line: &str) -> ScreenColumn {
    screen_column_in(line, row_len_of(line))
}

/// A right-open range of (row, column) points, columns in bytes.
///
/// A range that ends at the end of a row is written `(row + 1, 0)`, never `(row, row_len)`, even
/// when that row does not exist (end of file); one form means fewer off-by-one errors.
///
/// A range with `start == end` selects nothing. It marks where an insert or delete sits on the
/// side that has nothing to show, so both sides always have the same number of ranges.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRange {
    pub start_row: usize,
    pub start_column: usize,
    pub end_row: usize,
    pub end_column: usize,
}

impl TextRange {
    pub fn new(start_row: usize, start_column: usize, end_row: usize, end_column: usize) -> Self {
        Self {
            start_row,
            start_column,
            end_row,
            end_column,
        }
    }

    /// The empty range at this range's end, for placing a delete after it.
    pub fn right_limit(&self) -> Self {
        Self {
            start_row: self.end_row,
            start_column: self.end_column,
            end_row: self.end_row,
            end_column: self.end_column,
        }
    }

    /// The empty range at (0, 0), used as a "nothing yet" sentinel.
    pub fn zero() -> Self {
        Self {
            start_row: 0,
            start_column: 0,
            end_row: 0,
            end_column: 0,
        }
    }

    /// Whether this is the `zero()` sentinel. Not an emptiness check; see [`Self::is_empty`].
    pub fn is_zero(&self) -> bool {
        self.start_row == 0 && self.start_column == 0 && self.end_row == 0 && self.end_column == 0
    }

    /// Whether `start == end`, at any position.
    pub fn is_empty(&self) -> bool {
        self.start_row == self.end_row && self.start_column == self.end_column
    }

    /// Converts a tree-sitter range, normalizing an end at the end of a row to `(row + 1, 0)`.
    /// `columns_per_row` holds each row's length in **bytes**.
    pub fn from_treesitter_range(ts_range: Range, columns_per_row: &[usize]) -> Self {
        let mut end_row = ts_range.end_point.row;
        let mut end_column = ts_range.end_point.column;

        // An `end_row` past the last row is already normalized.
        if end_row < columns_per_row.len() && columns_per_row[end_row] == end_column {
            end_row += 1;
            end_column = 0;
        }

        Self {
            start_row: ts_range.start_point.row,
            start_column: ts_range.start_point.column,
            end_row,
            end_column,
        }
    }

    /// Whether the two right-open ranges overlap. Two empty ranges at the same point intersect.
    pub fn intersects(&self, other: &TextRange) -> bool {
        let self_start = (self.start_row, self.start_column);
        let self_end = (self.end_row, self.end_column);
        let other_start = (other.start_row, other.start_column);
        let other_end = (other.end_row, other.end_column);

        (self_start == self_end && other_start == other_end && self_start == other_start)
            || (self_start < other_end && other_start < self_end)
    }

    /// Whether this range starts exactly where `other` ends.
    pub fn extends(&self, other: &TextRange) -> bool {
        self.start_row == other.end_row && self.start_column == other.end_column
    }

    pub fn extend_to_end(&mut self, other: &TextRange) {
        self.end_row = other.end_row;
        self.end_column = other.end_column;
    }

    /// Whether the two ranges, in either order, touch or are separated only by whitespace. False
    /// for overlapping ranges and for positions that do not exist in `code`.
    pub fn can_extend_with_whitespace(&self, other: &TextRange, code: &SourceText) -> bool {
        is_whitespace_between(self, other, code)
    }

    /// The `[start, end)` byte columns of this range on `row`, whose length is `row_len`, or
    /// `None` if it covers nothing there.
    pub fn columns_on_row(&self, row: usize, row_len: SourceColumn) -> Option<(usize, usize)> {
        let row_len = row_len.get();
        if row < self.start_row || row > self.end_row {
            return None;
        }
        let start_col = if row == self.start_row {
            self.start_column
        } else {
            0
        };
        let end_col = if row == self.end_row {
            self.end_column
        } else {
            row_len
        };
        if start_col >= end_col {
            return None;
        }
        Some((start_col, end_col))
    }
}

fn is_whitespace_between(a: &TextRange, b: &TextRange, code: &SourceText) -> bool {
    if a.extends(b) {
        return true;
    }
    if b.extends(a) {
        return true;
    }

    let a_end_pos = (a.end_row, a.end_column);
    let b_start_pos = (b.start_row, b.start_column);
    let b_end_pos = (b.end_row, b.end_column);
    let a_start_pos = (a.start_row, a.start_column);

    let (first, second) = if a_end_pos <= b_start_pos {
        (a, b)
    } else if b_end_pos <= a_start_pos {
        (b, a)
    } else {
        return false;
    };

    // An unaddressable position refuses to extend, the safe direction; it must not read as "no
    // gap here".
    let (Some(first_end_idx), Some(second_start_idx)) = (
        code.byte_index(
            SourceRow::from_raw(first.end_row),
            SourceColumn::from_raw(first.end_column),
        ),
        code.byte_index(
            SourceRow::from_raw(second.start_row),
            SourceColumn::from_raw(second.start_column),
        ),
    ) else {
        return false;
    };

    if first_end_idx >= second_start_idx {
        return true;
    }

    let gap_text = &code.text()[first_end_idx.get()..second_start_idx.get()];
    gap_text.chars().all(|c| c.is_whitespace())
}

/// A file's text plus the byte offset every row starts at, so a (row, column) lookup is O(1)
/// rather than a walk from the start of the file, which dominates on large single-line files.
pub struct SourceText<'a> {
    text: &'a str,
    /// Never empty: even an empty file has row 0.
    row_starts: Vec<usize>,
}

impl<'a> SourceText<'a> {
    pub fn new(text: &'a str) -> Self {
        let mut row_starts = vec![0usize];
        row_starts.extend(
            text.bytes()
                .enumerate()
                .filter(|(_, b)| *b == b'\n')
                .map(|(i, _)| i + 1),
        );
        SourceText { text, row_starts }
    }

    pub fn text(&self) -> &'a str {
        self.text
    }

    /// Byte offset of a (row, byte column) position. `None` for a column past the row's end or
    /// inside a multi-byte character, which callers rely on so slicing cannot panic. The
    /// end-of-file position `(row_count, 0)` is valid.
    pub fn byte_index(&self, row: SourceRow, column: SourceColumn) -> Option<SourceOffset> {
        // The `(row + 1, 0)` end-of-file form has no `row_starts` entry but is addressable.
        if row.get() == self.row_starts.len() && column.get() == 0 {
            return Some(SourceOffset::from_raw(self.text.len()));
        }
        let &start = self.row_starts.get(row.get())?;
        // A column exactly at the row's end (its newline) is valid.
        let row_end = self
            .row_starts
            .get(row.get() + 1)
            .map(|next| next - 1)
            .unwrap_or(self.text.len());
        let index = start + column.get();
        if index > row_end || !self.text.is_char_boundary(index) {
            return None;
        }
        Some(SourceOffset::from_raw(index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A linear-walk reference for `SourceText::byte_index`; its answers, including the `None`s,
    /// are the contract.
    fn row_col_to_byte_index(row: usize, col: usize, code: &str) -> Option<usize> {
        // The `(row + 1, 0)` end-of-file form, which the walk cannot reach.
        if row == code.split('\n').count() && col == 0 {
            return Some(code.len());
        }

        let mut current_row = 0;
        let mut current_col = 0; // byte offset within the current row
        let mut byte_index = 0; // byte offset from the start of the string

        for ch in code.chars() {
            if current_row == row && current_col == col {
                return Some(byte_index);
            }
            let len = ch.len_utf8();
            if ch == '\n' {
                current_row += 1;
                current_col = 0;
            } else {
                current_col += len;
            }
            byte_index += len;
        }
        // Out of text: addressable only if exactly at the end.
        (current_row == row && current_col == col).then_some(byte_index)
    }

    /// The same equivalence over the corpus. Run it after touching `byte_index`.
    #[test]
    #[ignore = "slow: reads the whole fixture corpus"]
    fn byte_index_agrees_with_a_linear_walk_on_the_corpus() {
        let pairs = crate::test::helper::handmade_test_code_pairs().expect("corpus");
        let mut checked = 0usize;
        for (name, (before, after)) in pairs.iter() {
            for code in [&before.contents, &after.contents] {
                let index = SourceText::new(code);
                let rows = code.split('\n').count();
                for row in 0..=rows {
                    let width = code.split('\n').nth(row).map(str::len).unwrap_or(0);
                    for column in 0..=(width + 2) {
                        assert_eq!(
                            index
                                .byte_index(
                                    SourceRow::from_raw(row),
                                    SourceColumn::from_raw(column)
                                )
                                .map(SourceOffset::get),
                            row_col_to_byte_index(row, column, code),
                            "'{name}' ({row}, {column})"
                        );
                    }
                }
                checked += 1;
            }
        }
        assert!(checked > 100, "only checked {checked} files");
    }

    /// Includes a margin past the end: the out-of-range answers matter as much, because callers
    /// slice with them.
    #[test]
    fn byte_index_agrees_with_a_linear_walk_everywhere() {
        let samples = [
            "",
            "\n",
            "a",
            "a\n",
            "one\ntwo\nthree\n",
            "no trailing newline\nsecond",
            "a\u{2014}b   c", // em dash: byte columns 1..4 are inside one char
            "\u{1F600}x\n\u{00E9}\u{00E9}\n\n  tail", // emoji, accents, an empty row
            "  \t \n\t\n   ", // whitespace-only rows
        ];
        for code in samples {
            let index = SourceText::new(code);
            let rows = code.split('\n').count() + 2;
            for row in 0..rows {
                for column in 0..(code.len() + 3) {
                    assert_eq!(
                        index
                            .byte_index(SourceRow::from_raw(row), SourceColumn::from_raw(column))
                            .map(SourceOffset::get),
                        row_col_to_byte_index(row, column, code),
                        "({row}, {column}) in {code:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn is_zero_recognizes_only_the_origin_sentinel_while_is_empty_holds_anywhere() {
        let elsewhere = TextRange::new(3, 4, 3, 4);
        assert!(elsewhere.is_empty());
        assert!(!elsewhere.is_zero());
        assert!(TextRange::zero().is_zero());
        assert!(TextRange::zero().is_empty());
    }

    #[test]
    fn columns_on_row_spans_a_middle_row_whole_and_skips_rows_outside() {
        let range = TextRange::new(1, 4, 3, 2);
        assert_eq!(range.columns_on_row(0, SourceColumn::from_raw(9)), None);
        assert_eq!(
            range.columns_on_row(1, SourceColumn::from_raw(9)),
            Some((4, 9))
        );
        assert_eq!(
            range.columns_on_row(2, SourceColumn::from_raw(7)),
            Some((0, 7))
        );
        assert_eq!(
            range.columns_on_row(3, SourceColumn::from_raw(9)),
            Some((0, 2))
        );
        assert_eq!(range.columns_on_row(4, SourceColumn::from_raw(9)), None);
        // An end normalized to `(row + 1, 0)` paints nothing on that next row.
        assert_eq!(
            TextRange::new(0, 0, 1, 0).columns_on_row(1, SourceColumn::from_raw(5)),
            None
        );
    }

    #[test]
    fn text_range_intersects_overlapping() {
        let a = TextRange::new(0, 0, 5, 0);
        let b = TextRange::new(3, 0, 8, 0);
        assert!(a.intersects(&b));
        assert!(b.intersects(&a));
    }

    #[test]
    fn text_range_intersects_touching() {
        let a = TextRange::new(0, 0, 5, 0);
        let b = TextRange::new(5, 0, 10, 0);
        assert!(!a.intersects(&b));
        assert!(!b.intersects(&a));
    }

    #[test]
    fn text_range_intersects_identical() {
        let a = TextRange::new(0, 0, 5, 0);
        let b = TextRange::new(0, 0, 5, 0);
        assert!(a.intersects(&b));
    }

    #[test]
    fn text_range_intersects_contains() {
        let a = TextRange::new(0, 0, 10, 0);
        let b = TextRange::new(2, 0, 5, 0);
        assert!(a.intersects(&b));
        assert!(b.intersects(&a));
    }

    #[test]
    fn text_range_intersects_disjoint() {
        let a = TextRange::new(0, 0, 2, 0);
        let b = TextRange::new(5, 0, 8, 0);
        assert!(!a.intersects(&b));
        assert!(!b.intersects(&a));
    }

    #[test]
    fn text_range_intersects_same_row_different_columns() {
        let a = TextRange::new(0, 0, 0, 10);
        let b = TextRange::new(0, 5, 0, 15);
        assert!(a.intersects(&b));
        assert!(b.intersects(&a));
    }

    #[test]
    fn text_range_intersects_same_row_touching_columns() {
        let a = TextRange::new(0, 0, 0, 10);
        let b = TextRange::new(0, 10, 0, 20);
        assert!(!a.intersects(&b));
        assert!(!b.intersects(&a));
    }

    #[test]
    fn text_range_intersects_empty_range() {
        let a = TextRange::new(0, 0, 5, 0);
        let b = TextRange::new(2, 0, 2, 0);
        assert!(a.intersects(&b));
        assert!(b.intersects(&a));
    }

    #[test]
    fn text_range_intersects_empty_ranges_same_point() {
        let a = TextRange::new(0, 0, 0, 0);
        let b = TextRange::new(0, 0, 0, 0);
        assert!(a.intersects(&b));
    }

    #[test]
    fn text_range_intersects_crossing_rows() {
        let a = TextRange::new(0, 5, 2, 5);
        let b = TextRange::new(1, 0, 3, 0);
        assert!(a.intersects(&b));
        assert!(b.intersects(&a));
    }

    /// With a character count as row length, a mid-row byte column equal to that count would be
    /// taken for the end of the row. `let 漢 = "yy";` is 15 bytes and 13 characters.
    #[test]
    fn from_treesitter_range_does_not_normalize_a_mid_row_column_that_matches_a_character_count() {
        use tree_sitter::{Point, Range};

        let row = "let 漢 = \"yy\";";
        assert_eq!(row.len(), 15, "fifteen bytes");
        assert_eq!(row.chars().count(), 13, "thirteen characters");
        // The real producer, so the unit is checked at both ends.
        let row_byte_lengths = crate::code::metadata::compute_row_byte_lengths(row);

        let mid_row = TextRange::from_treesitter_range(
            Range {
                start_point: Point { row: 0, column: 11 },
                end_point: Point { row: 0, column: 13 },
                start_byte: 11,
                end_byte: 13,
            },
            &row_byte_lengths,
        );
        assert_eq!(
            (mid_row.end_row, mid_row.end_column),
            (0, 13),
            "byte column 13 is mid-row here and must stay where it is"
        );

        let end_of_row = TextRange::from_treesitter_range(
            Range {
                start_point: Point { row: 0, column: 11 },
                end_point: Point { row: 0, column: 15 },
                start_byte: 11,
                end_byte: 15,
            },
            &row_byte_lengths,
        );
        assert_eq!(
            (end_of_row.end_row, end_of_row.end_column),
            (1, 0),
            "the row's real end still normalises"
        );
    }

    #[test]
    fn from_treesitter_range_end_at_line_end() {
        use tree_sitter::Point;
        use tree_sitter::Range;

        let ts_range = Range {
            start_point: Point { row: 0, column: 0 },
            end_point: Point { row: 0, column: 5 },
            start_byte: 0,
            end_byte: 5,
        };
        let columns_per_row = vec![5];

        let result = TextRange::from_treesitter_range(ts_range, &columns_per_row);
        assert_eq!(result.start_row, 0);
        assert_eq!(result.start_column, 0);
        assert_eq!(result.end_row, 1);
        assert_eq!(result.end_column, 0);
    }

    #[test]
    fn from_treesitter_range_end_not_at_line_end() {
        use tree_sitter::Point;
        use tree_sitter::Range;

        let ts_range = Range {
            start_point: Point { row: 0, column: 0 },
            end_point: Point { row: 0, column: 3 },
            start_byte: 0,
            end_byte: 3,
        };
        let columns_per_row = vec![5]; // Row 0 has 5 columns

        let result = TextRange::from_treesitter_range(ts_range, &columns_per_row);
        assert_eq!(result.start_row, 0);
        assert_eq!(result.start_column, 0);
        assert_eq!(result.end_row, 0);
        assert_eq!(result.end_column, 3);
    }

    #[test]
    fn from_treesitter_range_multiline() {
        use tree_sitter::Point;
        use tree_sitter::Range;

        let ts_range = Range {
            start_point: Point { row: 0, column: 0 },
            end_point: Point { row: 1, column: 3 }, // End at row 1, column 3
            start_byte: 0,
            end_byte: 7,
        };
        let columns_per_row = vec![5, 5]; // Both rows have 5 columns

        let result = TextRange::from_treesitter_range(ts_range, &columns_per_row);
        assert_eq!(result.start_row, 0);
        assert_eq!(result.start_column, 0);
        assert_eq!(result.end_row, 1);
        assert_eq!(result.end_column, 3);
    }

    #[test]
    fn from_treesitter_range_end_at_last_line_end() {
        use tree_sitter::Point;
        use tree_sitter::Range;

        let ts_range = Range {
            start_point: Point { row: 0, column: 0 },
            end_point: Point { row: 1, column: 5 }, // End at last column of row 1
            start_byte: 0,
            end_byte: 11,
        };
        let columns_per_row = vec![5, 5];

        let result = TextRange::from_treesitter_range(ts_range, &columns_per_row);
        assert_eq!(result.start_row, 0);
        assert_eq!(result.start_column, 0);
        assert_eq!(result.end_row, 2);
        assert_eq!(result.end_column, 0);
    }

    #[test]
    fn from_treesitter_range_empty_range() {
        use tree_sitter::Point;
        use tree_sitter::Range;

        let ts_range = Range {
            start_point: Point { row: 0, column: 2 },
            end_point: Point { row: 0, column: 2 },
            start_byte: 2,
            end_byte: 2,
        };
        let columns_per_row = vec![5];

        let result = TextRange::from_treesitter_range(ts_range, &columns_per_row);
        assert_eq!(result.start_row, 0);
        assert_eq!(result.start_column, 2);
        assert_eq!(result.end_row, 0);
        assert_eq!(result.end_column, 2);
    }

    #[test]
    fn from_treesitter_range_end_row_beyond_columns() {
        use tree_sitter::Point;
        use tree_sitter::Range;

        let ts_range = Range {
            start_point: Point { row: 0, column: 0 },
            end_point: Point { row: 2, column: 0 }, // Row 2 doesn't exist in columns_per_row
            start_byte: 0,
            end_byte: 0,
        };
        let columns_per_row = vec![5, 5];

        let result = TextRange::from_treesitter_range(ts_range, &columns_per_row);
        assert_eq!(result.start_row, 0);
        assert_eq!(result.start_column, 0);
        assert_eq!(result.end_row, 2);
        assert_eq!(result.end_column, 0);
    }

    #[test]
    fn from_treesitter_range_end_row_beyond_columns_nonzero_column() {
        use tree_sitter::Point;
        use tree_sitter::Range;

        let ts_range = Range {
            start_point: Point { row: 0, column: 0 },
            end_point: Point { row: 2, column: 3 }, // Row 2 doesn't exist in columns_per_row
            start_byte: 0,
            end_byte: 0,
        };
        let columns_per_row = vec![5, 5];

        let result = TextRange::from_treesitter_range(ts_range, &columns_per_row);
        assert_eq!(result.start_row, 0);
        assert_eq!(result.start_column, 0);
        assert_eq!(result.end_row, 2);
        assert_eq!(result.end_column, 3);
    }

    #[test]
    fn text_range_can_extend_exact_touch() {
        let a = TextRange::new(0, 0, 1, 0);
        let b = TextRange::new(1, 0, 2, 0);
        let code = "line1\nline2\nline3";

        assert!(a.can_extend_with_whitespace(&b, &SourceText::new(code)));
    }

    #[test]
    fn text_range_can_extend_with_whitespace_only() {
        let a = TextRange::new(0, 0, 1, 0);
        let b = TextRange::new(1, 0, 2, 0);
        let code = "line1\n   \nline3"; // Two newlines with spaces in between

        assert!(a.can_extend_with_whitespace(&b, &SourceText::new(code)));
    }

    #[test]
    fn text_range_cannot_extend_with_non_whitespace() {
        let a = TextRange::new(0, 0, 0, 5);
        let b = TextRange::new(0, 7, 0, 10);
        let code = "helloXworld"; // Non-whitespace between

        assert!(!a.can_extend_with_whitespace(&b, &SourceText::new(code)));
    }

    #[test]
    fn text_range_can_extend_same_line_with_spaces() {
        let a = TextRange::new(0, 0, 0, 5);
        let b = TextRange::new(0, 7, 0, 10);
        let code = "hello   world"; // spaces between

        assert!(a.can_extend_with_whitespace(&b, &SourceText::new(code)));
    }

    #[test]
    fn text_range_cannot_extend_same_line_with_text() {
        let a = TextRange::new(0, 0, 0, 5);
        let b = TextRange::new(0, 7, 0, 10);
        let code = "helloXworld"; // 'X' is non-whitespace between

        assert!(!a.can_extend_with_whitespace(&b, &SourceText::new(code)));
    }

    #[test]
    fn text_range_cannot_extend_multi_line_with_non_whitespace() {
        let a = TextRange::new(0, 0, 1, 0);
        let b = TextRange::new(2, 0, 3, 0);
        let code = "line1\ntext\nline3"; // Non-whitespace line between

        assert!(!a.can_extend_with_whitespace(&b, &SourceText::new(code)));
    }

    #[test]
    fn row_col_to_byte_index_lands_after_a_multi_byte_character() {
        // "—" is 3 bytes, so byte column 4 is 'b'.
        let code = "a—bc";
        assert_eq!(row_col_to_byte_index(0, 0, code), Some(0)); // 'a'
        assert_eq!(row_col_to_byte_index(0, 1, code), Some(1)); // '—', right after 'a'
        assert_eq!(row_col_to_byte_index(0, 4, code), Some(4)); // 'b', right after '—'
        assert_eq!(row_col_to_byte_index(0, 5, code), Some(5)); // 'c', right after 'b'
    }

    /// With character counts, the em dash would put the gap slice mid-character and panic.
    #[test]
    fn text_range_can_extend_with_whitespace_after_a_multi_byte_character_earlier_in_the_line() {
        let code = "a—b   c"; // "a—b", 3 spaces, "c" - byte columns: a=0, —=1, b=4, c=8
        let a = TextRange::new(0, 0, 0, 5); // ends right after "a—b" (byte 5)
        let b = TextRange::new(0, 8, 0, 9); // starts at "c" (byte 8)

        assert!(a.can_extend_with_whitespace(&b, &SourceText::new(code)));
    }
}

#[cfg(test)]
mod position_tests {
    use super::*;

    #[test]
    fn floor_char_boundary_clamps_to_the_line_and_rounds_down_inside_a_character() {
        assert_eq!(floor_char_boundary("a漢b", 2), 1);
        assert_eq!(floor_char_boundary("a漢b", 4), 4);
        assert_eq!(floor_char_boundary("ab", 99), 2);
    }

    #[test]
    fn paint_row_len_stops_before_trailing_whitespace() {
        assert_eq!(paint_row_len("let x;  \t"), SourceColumn::from_raw(6));
        assert_eq!(paint_row_len("   "), SourceColumn::from_raw(0));
    }

    #[test]
    fn row_len_is_bytes_not_characters() {
        assert_eq!(row_len_of("abc"), SourceColumn::from_raw(3));
        // Two characters, three bytes.
        assert_eq!(row_len_of("é]"), SourceColumn::from_raw(3));
        assert_eq!(row_len_of(""), SourceColumn::from_raw(0));
    }

    #[test]
    fn screen_column_counts_cells_not_bytes_or_characters() {
        // ASCII: all three units agree.
        assert_eq!(
            screen_column_in("let a", SourceColumn::from_raw(5)),
            ScreenColumn::from_raw(5)
        );
        // 'é' is two bytes but one cell, so byte column 3 is screen column 2.
        assert_eq!(
            screen_column_in("aéb", SourceColumn::from_raw(3)),
            ScreenColumn::from_raw(2)
        );
        // A CJK ideograph is three bytes and two cells.
        assert_eq!(
            screen_column_in("a漢b", SourceColumn::from_raw(4)),
            ScreenColumn::from_raw(3)
        );
        // A combining mark occupies no cell of its own.
        assert_eq!(
            screen_column_in("e\u{0301}x", SourceColumn::from_raw(3)),
            ScreenColumn::from_raw(1)
        );
    }

    #[test]
    fn screen_column_rounds_down_inside_a_multi_byte_character() {
        // Byte column 2 lands inside '漢'.
        assert_eq!(
            screen_column_in("a漢", SourceColumn::from_raw(2)),
            ScreenColumn::from_raw(1)
        );
        assert_eq!(
            screen_column_in("a漢", SourceColumn::from_raw(4)),
            ScreenColumn::from_raw(3)
        );
    }

    #[test]
    fn screen_column_is_monotonic_across_every_byte_column_of_a_mixed_row() {
        let line = "a漢é\u{0301}b—c";
        let mut previous = ScreenColumn::from_raw(0);
        for column in 0..=line.len() {
            let cells = screen_column_in(line, SourceColumn::from_raw(column));
            assert!(
                cells >= previous,
                "screen column went backwards at byte {column} of {line:?}"
            );
            previous = cells;
        }
        // The whole row's width, for reference: a(1) 漢(2) é(1) combining(0) b(1) —(1) c(1).
        assert_eq!(previous, ScreenColumn::from_raw(7));
    }
}

#[cfg(test)]
mod corpus_position_invariants {
    /// Every emitted position is a byte column on a character boundary and addressable as an
    /// offset, and two ranges on one side share at most a line terminator. One pass so each
    /// fixture is diffed once.
    ///
    /// This does not catch a character-count row length in `compute_row_byte_lengths`, since a
    /// mis-normalized `(row + 1, 0)` is still legal; that is
    /// `from_treesitter_range_does_not_normalize_a_mid_row_column_that_matches_a_character_count`'s
    /// job.
    ///
    /// Sharing a `\n` is expected: an end normalized to `(row + 1, 0)` takes in the newline the
    /// next range starts at, and every consumer excludes newlines. Sharing a character would be a
    /// real disagreement, which the renderers and the scorer resolve by different rules.
    #[test]
    fn corpus_ranges_are_addressable_byte_columns_sharing_nothing_but_line_terminators() {
        let cases = crate::test::helper::handmade_test_case_dirs().expect("corpus");
        let checked = std::sync::atomic::AtomicUsize::new(0);

        // `scope` re-raises a panicking thread's assertion on join.
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
        let chunk = cases.len().div_ceil(threads).max(1);
        std::thread::scope(|scope| {
            for slice in cases.chunks(chunk) {
                scope.spawn(|| check_cases(slice, &checked));
            }
        });

        assert!(
            checked.load(std::sync::atomic::Ordering::Relaxed) > 0,
            "the corpus should have produced ranges to check"
        );
    }

    fn check_cases(
        cases: &[(String, std::path::PathBuf)],
        checked: &std::sync::atomic::AtomicUsize,
    ) {
        use crate::diff::text_range::{SourceColumn, SourceOffset, SourceRow, SourceText};

        for (name, dir) in cases {
            let Some((before, after)) =
                crate::test::helper::code_pair_from_dir(dir).expect("fixture loads")
            else {
                continue;
            };
            let diff = crate::diff::diff_code(&before, &after);
            let Some(ast) = diff.ast.as_ref() else {
                continue;
            };
            let cache = crate::diff::NodeCache::build(&before, &after);
            let text_diff = crate::diff::text::TextDiff::from(&before, &after, ast, &cache);

            for (side, contents) in [(0usize, &before.contents), (1usize, &after.contents)] {
                let rows: Vec<&str> = contents.split('\n').collect();
                let text = SourceText::new(contents);
                let mut spans: Vec<(usize, usize)> = Vec::new();

                for range in text_diff.all(side) {
                    for (label, row, column) in [
                        ("start", range.source.start_row, range.source.start_column),
                        ("end", range.source.end_row, range.source.end_column),
                    ] {
                        // One past the last row is end-of-file, legal only at column 0.
                        let Some(text) = rows.get(row) else {
                            assert_eq!(
                                column, 0,
                                "{name} side{side}: {label} past the last row must be column 0"
                            );
                            continue;
                        };
                        assert!(
                            column <= text.len(),
                            "{name} side{side}: {label} column {column} exceeds row {row}'s \
                             {} bytes - a character count cannot stand in for a byte column",
                            text.len()
                        );
                        assert!(
                            text.is_char_boundary(column),
                            "{name} side{side}: {label} column {column} falls inside a multi-byte \
                             character on row {row}"
                        );
                        checked.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }

                    let offset = |row: usize, column: usize| -> usize {
                        text.byte_index(SourceRow::from_raw(row), SourceColumn::from_raw(column))
                            .map(SourceOffset::get)
                            .unwrap_or_else(|| {
                                panic!(
                                    "{name} side{side}: r{row}c{column} is not addressable as a \
                                     byte offset"
                                )
                            })
                    };
                    let start = offset(range.source.start_row, range.source.start_column);
                    let end = offset(range.source.end_row, range.source.end_column);
                    assert!(
                        end >= start,
                        "{name} side{side}: range ends before it starts ({start}..{end})"
                    );
                    if end > start {
                        spans.push((start, end));
                    }
                }

                for i in 0..spans.len() {
                    for j in (i + 1)..spans.len() {
                        let lo = spans[i].0.max(spans[j].0);
                        let hi = spans[i].1.min(spans[j].1);
                        if lo >= hi {
                            continue;
                        }
                        assert_eq!(
                            &contents[lo..hi],
                            "\n",
                            "{name} side{side}: ranges {:?} and {:?} both claim {:?}, which is not \
                             a line terminator",
                            spans[i],
                            spans[j],
                            &contents[lo..hi]
                        );
                    }
                }
            }
        }
    }
}
