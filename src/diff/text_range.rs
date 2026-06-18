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

    /// Returns true if the range is a zero range.
    pub fn is_zero(&self) -> bool {
        self.start_row == 0 && self.start_column == 0 && self.end_row == 0 && self.end_column == 0
    }

    /// Create a new TextRange from a TreeSitter Range.
    ///
    /// This method adjusts the end position to follow the TextRange convention:
    /// - If the end_row is within the columns_per_row array and end_column equals the last column of that row, it moves to (row+1, 0)
    /// - Otherwise, it increments the column by 1
    ///
    /// # Arguments
    ///
    /// * `ts_range` - The TreeSitter Range to convert
    /// * `columns_per_row` - A slice where each element represents the number of columns in that row
    pub fn from_treesitter_range(ts_range: Range, columns_per_row: &[usize]) -> Self {
        let start_row = ts_range.start_point.row;
        let start_column = ts_range.start_point.column;

        let end_row = ts_range.end_point.row;
        let end_column = ts_range.end_point.column;

        // Adjust the end position
        let (adjusted_end_row, adjusted_end_column) =
            if end_row < columns_per_row.len() && end_column == columns_per_row[end_row] {
                // If end_column is exactly at the end of the row, move to next row, column 0
                (end_row + 1, 0)
            } else if end_row >= columns_per_row.len() && end_column == 0 {
                // If we're past the end of known rows and at column 0, move to next row
                (end_row + 1, 0)
            } else {
                // Otherwise, increment the column
                (end_row, end_column + 1)
            };

        Self {
            start_row,
            start_column,
            end_row: adjusted_end_row,
            end_column: adjusted_end_column,
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
        // When end_column equals the last column of the row, move to next row, column 0
        use tree_sitter::Point;
        use tree_sitter::Range;

        let ts_range = Range {
            start_point: Point { row: 0, column: 0 },
            end_point: Point { row: 0, column: 5 }, // End at column 5 (last column of row 0)
            start_byte: 0,
            end_byte: 5,
        };
        let columns_per_row = vec![5]; // Row 0 has 5 columns

        let result = TextRange::from_treesitter_range(ts_range, &columns_per_row);
        assert_eq!(result.start_row, 0);
        assert_eq!(result.start_column, 0);
        assert_eq!(result.end_row, 1);
        assert_eq!(result.end_column, 0);
    }

    #[test]
    fn from_treesitter_range_end_not_at_line_end() {
        // When end_column is NOT at the last column, increment column
        use tree_sitter::Point;
        use tree_sitter::Range;

        let ts_range = Range {
            start_point: Point { row: 0, column: 0 },
            end_point: Point { row: 0, column: 3 }, // End at column 3 (not last column)
            start_byte: 0,
            end_byte: 3,
        };
        let columns_per_row = vec![5]; // Row 0 has 5 columns

        let result = TextRange::from_treesitter_range(ts_range, &columns_per_row);
        assert_eq!(result.start_row, 0);
        assert_eq!(result.start_column, 0);
        assert_eq!(result.end_row, 0);
        assert_eq!(result.end_column, 4); // 3 + 1
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
        assert_eq!(result.end_column, 4); // 3 + 1
    }

    #[test]
    fn from_treesitter_range_end_at_last_line_end() {
        // Test when end is at the last column of the last line
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
        assert_eq!(result.end_row, 2); // row + 1
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
        assert_eq!(result.end_column, 3); // 2 + 1
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
        assert_eq!(result.end_row, 3); // row + 1 because end_row >= len and end_column == 0
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
        assert_eq!(result.end_column, 4); // column + 1
    }
}
