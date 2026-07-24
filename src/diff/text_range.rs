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

/**
* A range of text. The range is a right-open interval, i.e. the end point is NOT part of the range.
* Each point in the range is a (row, column) pair.
*
* Note that there are some interesting corner cases when dealing with non-printable characters and
* text ranges. Each textual row, unless the file is completely empty, ends with a newline
* character, or two in case of '\r\n'. This raises the interesting question of how to refer to a
* full row:
*   - We could refer to the row, but refer to a non-existing column one column after the last
*     printed character.
*   - We could refer to the next row (which could also be non-existing in case of end of file) and
*     always refer to column 0.
*
* Note that either way, we need to support refering to technically non-existing rows or columns.
* With this in mind, all algorithms should ideally be implemented in such way that they support
* either of the two. However, in this codebase, all code should actually use the second approach
* and use the (row + 1, 0) pair. This is based on engineering intuition that this leads to fewer
* potential off-by-one errors.
*
* Another interesting corner case are ranges with no size. If the start and end point are exactly
* the same, we could interpret this either as "only start" or "empty range that doesn't actually
* select anything". We choose the second, because "only start" can already be represented as
* [start, start+1> right-open interval. This gives us an interesting ability to represent
* "infinitely small ranges" that are still strictly well ordered. This is useful because it allows
* us a neat property: when code is inserted or deleted, the other side of the comparison will not
* have a matching range at all. However, if we insert a null-range in the appropriate place, we can
* allow the editor to display a red/green line indicating that something exists on the other side
* in this place. This also leads to symetric diffs: both sides will always have the same number of
* ranges, or in case of multi file diffs the sum total of ranges will always be an even number and
* each range will always have a matching range somewhere.
*
* One-side open intervals have the useful property that they can easily implement union,
* subtraction and intersection with not corner cases.
*/
#[derive(Debug, Clone, PartialEq)]
pub struct TextRange {
    pub start_row: usize,
    pub start_column: usize,
    pub end_row: usize,
    pub end_column: usize,
}

impl TextRange {
    /// Create a new TextRange from start and end points.
    pub fn new(start_row: usize, start_column: usize, end_row: usize, end_column: usize) -> Self {
        Self {
            start_row,
            start_column,
            end_row,
            end_column,
        }
    }

    /// Returns a new empty range that starts and ends exactly at the right-open limit of the
    /// current range.
    ///
    /// Useful when you want to add a "non existing" thing like a delete to the end of the current
    /// range.
    pub fn right_limit(&self) -> Self {
        Self {
            start_row: self.end_row,
            start_column: self.end_column,
            end_row: self.end_row,
            end_column: self.end_column,
        }
    }

    /// Creats a "zero" range. An empty range starting at (0,0) and ending at (0, 0).
    pub fn zero() -> Self {
        Self {
            start_row: 0,
            start_column: 0,
            end_row: 0,
            end_column: 0,
        }
    }

    /// Returns true if this is exactly the `(0,0)-(0,0)` range produced by `zero()`.
    ///
    /// This is *not* a general emptiness check (a range can be empty, i.e. `start == end`, at any
    /// position) - callers use it to detect the `zero()` sentinel, e.g. "no range accumulated yet"
    /// in `text.rs`'s range-building loop.
    pub fn is_zero(&self) -> bool {
        self.start_row == 0 && self.start_column == 0 && self.end_row == 0 && self.end_column == 0
    }

    /// Returns true if this range spans no text at all (`start == end`), regardless of position -
    /// unlike [`Self::is_zero`], which only recognizes the `(0,0)-(0,0)` sentinel specifically.
    /// Used to detect the symmetric insert/delete placeholder ranges `diff::text` emits for the
    /// side with nothing to show.
    pub fn is_empty(&self) -> bool {
        self.start_row == self.end_row && self.start_column == self.end_column
    }

    /// Create a new TextRange from a TreeSitter Range.
    ///
    /// TreeSitter ranges are already right-open, so no adjustment is needed.
    ///
    /// # Arguments
    ///
    /// * `ts_range` - The TreeSitter Range to convert
    /// * `columns_per_row` - A slice where each element represents the number of columns in that row (not used currently, kept for API compatibility)
    pub fn from_treesitter_range(ts_range: Range, columns_per_row: &[usize]) -> Self {
        let mut end_row = ts_range.end_point.row;
        let mut end_column = ts_range.end_point.column;

        // If the end point lands exactly at the end of its row, normalize it to (next row, 0) per
        // this module's convention (see the doc comment above). `end_row < columns_per_row.len()`
        // both guards the index below and skips normalization when `end_row` is already one past
        // the last real row - i.e. already in normalized form, nothing to do.
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

    /// Check if this range intersects with another range.
    /// Two ranges intersect if they overlap (as right-open intervals).
    /// Empty ranges (start == end) at the same point are considered to intersect.
    pub fn intersects(&self, other: &TextRange) -> bool {
        let self_start = (self.start_row, self.start_column);
        let self_end = (self.end_row, self.end_column);
        let other_start = (other.start_row, other.start_column);
        let other_end = (other.end_row, other.end_column);

        // Right-open interval intersection: self starts before other ends AND other starts before self ends
        // Special case: both are empty ranges at the same point
        (self_start == self_end && other_start == other_end && self_start == other_start)
            || (self_start < other_end && other_start < self_end)
    }

    /// Check if this range exactly extends the other range.
    ///
    /// A range exactly extends another range if the end of one is exactly the start of the other.
    /// I.e., if they do NOT intersect but there are no elements between the two.
    pub fn extends(&self, other: &TextRange) -> bool {
        self.start_row == other.end_row && self.start_column == other.end_column
    }

    /// Extend the end to match the end of the other range.
    pub fn extend_to_end(&mut self, other: &TextRange) {
        self.end_row = other.end_row;
        self.end_column = other.end_column;
    }

    /// Check if this range can extend the other range, allowing for whitespace-only gaps.
    ///
    /// Returns true if the ranges touch exactly (see `extends`) OR if all text between
    /// the end of this range and the start of the other range consists of whitespace characters.
    pub fn can_extend_with_whitespace(&self, other: &TextRange, code: &str) -> bool {
        is_whitespace_between(self, other, code)
    }
}

/// Helper function to check if all characters between the end of range `a` and start of range `b` are whitespace.
/// Returns true if the ranges touch exactly (a.end == b.start or b.end == a.start),
/// or if there is a gap containing only whitespace.
/// Returns false if the ranges overlap or are in the wrong order for extension.
fn is_whitespace_between(a: &TextRange, b: &TextRange, code: &str) -> bool {
    // Check if a extends b exactly (a.end touches b.start)
    if a.extends(b) {
        return true;
    }

    // Check if b extends a exactly (b.end touches a.start)
    if b.extends(a) {
        return true;
    }

    // Determine which range comes first
    // a comes before b if a.end < b.start (in row-major order)
    let a_end_pos = (a.end_row, a.end_column);
    let b_start_pos = (b.start_row, b.start_column);
    let b_end_pos = (b.end_row, b.end_column);
    let a_start_pos = (a.start_row, a.start_column);

    let (first, second) = if a_end_pos <= b_start_pos {
        // a ends at or before b starts, so a comes first
        (a, b)
    } else if b_end_pos <= a_start_pos {
        // b ends at or before a starts, so b comes first
        (b, a)
    } else {
        // Ranges overlap or are otherwise incomparable - cannot extend
        return false;
    };

    // Convert positions to character indices
    let first_end_idx = row_col_to_char_index(first.end_row, first.end_column, code);
    let second_start_idx = row_col_to_char_index(second.start_row, second.start_column, code);

    if first_end_idx >= second_start_idx {
        // No gap or overlapping
        return true;
    }

    // Extract the gap text and check if all characters are whitespace
    let gap_text = &code[first_end_idx..second_start_idx];
    gap_text.chars().all(|c| c.is_whitespace())
}

/// Convert a (row, column) position to a character index in the code string.
/// This counts characters (not bytes) from the start of the string.
fn row_col_to_char_index(row: usize, col: usize, code: &str) -> usize {
    let mut current_row = 0;
    let mut current_col = 0;
    let mut char_index = 0;

    for ch in code.chars() {
        // Check if we've reached the target position
        if current_row == row && current_col == col {
            return char_index;
        }

        if ch == '\n' {
            current_row += 1;
            current_col = 0;
        } else {
            current_col += 1;
        }

        char_index += 1;
    }

    // Position is at or beyond the end of the string - clamp to the string length.
    char_index
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests for TextRange::intersects()
    #[test]
    fn text_range_intersects_overlapping() {
        let a = TextRange::new(0, 0, 5, 0);
        let b = TextRange::new(3, 0, 8, 0);
        assert!(a.intersects(&b));
        assert!(b.intersects(&a));
    }

    #[test]
    fn text_range_intersects_touching() {
        // Right-open intervals: [0,5) and [5,10) do NOT intersect
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
        // [0,10) and [10,20) do NOT intersect
        let a = TextRange::new(0, 0, 0, 10);
        let b = TextRange::new(0, 10, 0, 20);
        assert!(!a.intersects(&b));
        assert!(!b.intersects(&a));
    }

    #[test]
    fn text_range_intersects_empty_range() {
        // Empty ranges (start == end) still intersect with ranges that contain them
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

    // Tests for TextRange::from_treesitter_range()
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
        // TreeSitter ranges are already right-open, so no adjustment needed
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
        // Test with multiple lines
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
        // Test with an empty range (start == end)
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
        // Test when end_row is beyond the columns_per_row array with end_column = 0
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
        // Test when end_row is beyond the columns_per_row array with end_column != 0
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

    // Tests for TextRange::can_extend_with_whitespace()
    #[test]
    fn text_range_can_extend_exact_touch() {
        let a = TextRange::new(0, 0, 1, 0);
        let b = TextRange::new(1, 0, 2, 0);
        let code = "line1\nline2\nline3";

        assert!(a.can_extend_with_whitespace(&b, code));
    }

    #[test]
    fn text_range_can_extend_with_whitespace_only() {
        let a = TextRange::new(0, 0, 1, 0);
        let b = TextRange::new(1, 0, 2, 0);
        let code = "line1\n   \nline3"; // Two newlines with spaces in between

        assert!(a.can_extend_with_whitespace(&b, code));
    }

    #[test]
    fn text_range_cannot_extend_with_non_whitespace() {
        let a = TextRange::new(0, 0, 0, 5);
        let b = TextRange::new(0, 7, 0, 10);
        let code = "helloXworld"; // Non-whitespace between

        assert!(!a.can_extend_with_whitespace(&b, code));
    }

    #[test]
    fn text_range_can_extend_same_line_with_spaces() {
        let a = TextRange::new(0, 0, 0, 5);
        let b = TextRange::new(0, 7, 0, 10);
        let code = "hello   world"; // spaces between

        assert!(a.can_extend_with_whitespace(&b, code));
    }

    #[test]
    fn text_range_cannot_extend_same_line_with_text() {
        let a = TextRange::new(0, 0, 0, 5);
        let b = TextRange::new(0, 7, 0, 10);
        let code = "helloXworld"; // 'X' is non-whitespace between

        assert!(!a.can_extend_with_whitespace(&b, code));
    }

    #[test]
    fn text_range_cannot_extend_multi_line_with_non_whitespace() {
        let a = TextRange::new(0, 0, 1, 0);
        let b = TextRange::new(2, 0, 3, 0);
        let code = "line1\ntext\nline3"; // Non-whitespace line between

        assert!(!a.can_extend_with_whitespace(&b, code));
    }
}
