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

/// A row in the source document: the index of a `\n`-delimited line.
///
/// Rows have only one unit - a line index is a line index however the bytes on it are encoded -
/// so there is deliberately no byte/character/cell variant of this type. (Source vs. screen row,
/// which differ once long lines wrap, is a distinction the renderers keep locally; no shared
/// newtype for it has had a caller.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct SourceRow(usize);

/// A column in the source document: a **byte** offset within its row.
///
/// Bytes - not characters, and not terminal cells. That is what tree-sitter's `Point::column`
/// reports, what `human_mapping.json` stores for every painted span, what
/// `SourceText::byte_index` adds to a row start, and what `human_solver`'s cursor walks
/// (`step_column` advances by `char::len_utf8`). Re-basing this on characters would silently
/// invalidate every painted span in the corpus that sits on a non-ASCII row.
///
/// The defect this type exists to prevent is a row length measured in *characters* reaching a
/// place that wanted a byte column - which is why [`row_len_of`] is the way to obtain one for a
/// whole row, and why [`SourceColumn::from_raw`] is named to be uncomfortable at a call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct SourceColumn(usize);

/// An absolute **byte** offset into the whole file - the flat form of a
/// (`SourceRow`, `SourceColumn`) pair.
///
/// A separate type from `SourceColumn` because both are byte counts and both were bare `usize`s:
/// nothing distinguished "byte 9 of this row" from "byte 9 of this file", and they are freely
/// mixed at slicing sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct SourceOffset(usize);

/// A column in the rendered viewport: a terminal **cell** offset within its screen row.
///
/// Neither bytes nor characters: a CJK ideograph occupies two cells, a combining mark zero. Only
/// meaningful together with the row's text, so it is derived at the render boundary by
/// [`screen_column_in`] and never stored or persisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ScreenColumn(usize);

macro_rules! position_newtype {
    ($name:ident) => {
        impl $name {
            /// Wrap a bare `usize`. Deliberately verbose: every call is a place where the unit is
            /// being asserted rather than carried, so it should be visible in review and greppable.
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

/// The extent of `line` as a source column - its length in **bytes**, which is the column one past
/// its last character and the value `TextRange::columns_on_row` wants as a row length.
///
/// This exists so that call sites cannot reach for `line.chars().count()`, which is the exact
/// substitution that mis-highlighted every non-ASCII row: a character count is not a byte column.
pub fn row_len_of(line: &str) -> SourceColumn {
    SourceColumn(line.len())
}

/// The extent of `line` a renderer should paint up to when a range covers the row without ending
/// on it: [`row_len_of`] with trailing whitespace excluded, so a painted range never covers
/// trailing whitespace or, past it, the newline (RULES_AND_PREFERENCES.md, "never show end of
/// line whitespace"). Renderers still print the trimmed tail; they just do not colour it.
pub fn paint_row_len(line: &str) -> SourceColumn {
    row_len_of(line.trim_end())
}

/// `column` clamped to `line`'s length and rounded down to a character boundary, so a byte
/// column that is off - a malformed painting, a stale range, an offset derived from a hash - can
/// slice `line` without panicking inside a multi-byte character.
///
/// The one home for this clamp. Every renderer that slices a row by column used to carry its own
/// copy, and the byte-versus-character defect behind it was found and fixed separately in two of
/// them (see [`SourceColumn`]).
pub fn floor_char_boundary(line: &str, column: usize) -> usize {
    let mut column = column.min(line.len());
    while column > 0 && !line.is_char_boundary(column) {
        column -= 1;
    }
    column
}

/// How many terminal cells the first `column` bytes of `line` occupy - the source-to-screen
/// conversion, and the only place a `SourceColumn` may become a `ScreenColumn`.
///
/// A column landing inside a multi-byte character counts that character not at all rather than
/// partially: cells are indivisible, and rounding down keeps the result monotonic in `column`.
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

/// How many terminal cells `ch` occupies: two for a CJK ideograph, none for a combining mark, one
/// for most things. Neither a byte count nor a character count answers this, which is why wrapping
/// and any other width arithmetic has to go through it.
pub fn cell_width_of(ch: char) -> ScreenColumn {
    use unicode_width::UnicodeWidthChar;
    ScreenColumn(ch.width().unwrap_or(0))
}

/// The width of a whole row in terminal cells.
pub fn row_cells_of(line: &str) -> ScreenColumn {
    screen_column_in(line, row_len_of(line))
}

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
* in this place. This also leads to symmetric diffs: both sides will always have the same number of
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
    /// * `columns_per_row` - A slice where each element represents the number of columns in that
    ///   row, used to detect an end point landing exactly at end-of-row and normalize it to
    ///   `(next row, 0)` per this module's convention (see the normalization step below)
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
    pub fn can_extend_with_whitespace(&self, other: &TextRange, code: &SourceText) -> bool {
        is_whitespace_between(self, other, code)
    }

    /// Returns the `[start_column, end_column)` portion of this range that falls on `row`, given
    /// the number of characters on that row, or `None` if this range doesn't cover any part of
    /// `row`. Shared by `tui::widgets::code_viewer` (column-precise overlay painting) and
    /// `tui::headless` (column-precise inline ANSI highlighting) - both walk the same per-row
    /// span math, just onto different rendering targets (a ratatui `Line` vs. a plain string).
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

/// Helper function to check if all characters between the end of range `a` and start of range `b` are whitespace.
/// Returns true if the ranges touch exactly (a.end == b.start or b.end == a.start),
/// or if there is a gap containing only whitespace.
/// Returns false if the ranges overlap or are in the wrong order for extension.
fn is_whitespace_between(a: &TextRange, b: &TextRange, code: &SourceText) -> bool {
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

    // Convert positions to byte indices (see `SourceText::byte_index` - the slice below needs byte
    // offsets, not character counts).
    // `None` means the position is not addressable - a row past the end of the file, or a column
    // inside a multi-byte character. Refusing to extend is the safe direction and the one the
    // arm above already takes for incomparable ranges; the previous in-band `text.len()` sentinel
    // made both cases satisfy `first_end_idx >= second_start_idx` instead, so an unaddressable
    // position silently read as "no gap here, merge them".
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
        // No gap or overlapping
        return true;
    }

    // Extract the gap text and check if all characters are whitespace
    let gap_text = &code.text()[first_end_idx.get()..second_start_idx.get()];
    gap_text.chars().all(|c| c.is_whitespace())
}

/// A file's text plus the byte offset every row starts at.
///
/// **Why this exists.** `is_whitespace_between` needs to turn two (row, column) positions into byte
/// offsets so it can look at the text between them. It used to do that by walking `code.chars()`
/// from byte 0 for each position - O(file) per call, and `RangeMatch::extends` makes two of those
/// calls per side. Profiling the corpus on 2026-08-28 found the result: on
/// `json-ipfs-ipfs-desktop-only-update-version-strings` (924KB per side), 16,848 calls cost 148
/// billion instructions - **90% of the entire run** - at 8.8 million instructions each, which is
/// exactly the cost of walking that file twice. The same fixture's diff took 0.7s and its range
/// merging took 10.4s.
///
/// With the row offsets computed once, the same lookup is an add and a bounds check. This is the
/// second instance of this shape in this module; see `ranges_for_options`'s own "built once for the
/// whole call rather than rescanning the file per range" note.
pub struct SourceText<'a> {
    text: &'a str,
    /// Byte offset where each row begins. Always at least one entry (row 0 starts at 0), since
    /// even an empty file has one empty row.
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

    /// Byte index of a (row, column) position, or the text's byte length if that position doesn't
    /// exist.
    ///
    /// `TextRange`'s `column` fields come straight from tree-sitter's `Point` (see
    /// `TextRange::from`), which - like all of tree-sitter's offsets - counts *bytes* within the
    /// row, not Unicode characters. So the lookup is `row_starts[row] + column`, with two
    /// rejections that the linear walk this replaced performed implicitly by never matching, and
    /// that callers depend on:
    ///
    /// * A column past the row's own end. The walk carried on into later rows with its row counter
    ///   already advanced, so it never matched and fell through to the end of the file.
    /// * A column that lands *inside* a multi-byte character. The walk only ever tested positions
    ///   at character boundaries, so it skipped straight past such a column - which is what kept
    ///   the `&code[a..b]` slice at the call site from panicking with "byte index N is not a char
    ///   boundary". Reproducing that is not defensive: `byte_index_lands_correctly_past_a_multi_
    ///   byte_character` below is modeled on a real crash from a file containing an em dash.
    ///
    /// `byte_index_agrees_with_a_linear_walk_everywhere` checks the equivalence exhaustively
    /// against the original implementation rather than trusting this description.
    pub fn byte_index(&self, row: SourceRow, column: SourceColumn) -> Option<SourceOffset> {
        // The end-of-file position, in this module's own `(row + 1, 0)` convention for "past the
        // end of the last row" (see the doc comment at the top of this file). There is no
        // `row_starts` entry for it - it is one past the last row - but it is a real, addressable
        // position that 79 ranges in the corpus end at, so it is answered here rather than
        // rejected. The pre-`Option` version happened to return `text.len()` for it via its
        // failure sentinel, which was the right answer reached by the wrong route.
        if row.get() == self.row_starts.len() && column.get() == 0 {
            return Some(SourceOffset::from_raw(self.text.len()));
        }
        let &start = self.row_starts.get(row.get())?;
        // The row ends just before the next row's first byte - that is, at its newline - or at the
        // end of the text for the last row. A column exactly *at* the row's end is valid: it is the
        // position the walk reached before consuming the newline.
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

    /// The linear walk `SourceText::byte_index` replaced, kept verbatim as the reference the
    /// equivalence test checks against. Its semantics - including what it does with a column past
    /// the row's end or inside a multi-byte character - are the contract, and they were implicit in
    /// the walk rather than written down anywhere.
    fn row_col_to_byte_index(row: usize, col: usize, code: &str) -> Option<usize> {
        // This module's `(row + 1, 0)` end-of-file form (see the doc comment at the top of the
        // file): one past the last row, which the walk below cannot reach because it only ever
        // visits positions that real characters sit at. `byte_index` answers it directly, so the
        // reference has to as well or the two disagree on 79 of the corpus's ranges.
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
        // The walk ran out of text: the position is addressable only if it is exactly the end.
        // The previous version returned `byte_index` unconditionally here, which is the same
        // in-band sentinel `byte_index` itself used to have - it made "one past the end" and "not
        // a real position" indistinguishable in the very test meant to pin the behaviour.
        (current_row == row && current_col == col).then_some(byte_index)
    }

    /// The same equivalence, over the real corpus rather than hand-picked samples. Kept
    /// `#[ignore]`d because it parses every fixture; run it deliberately after touching
    /// `byte_index`.
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

    /// The whole justification for the rewrite: same answers, without the O(file) walk.
    ///
    /// Exhaustive over every (row, column) pair in range for each sample, plus a margin past the
    /// end of both - the out-of-range answers are as load-bearing as the in-range ones, because
    /// the call site slices with them.
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
    /// The end-of-row normalisation must key on the row's **byte** length, which is the unit
    /// tree-sitter reports columns in. Feeding it a character count fires the rule on the wrong
    /// rows, in both directions, and this pins the direction that is hardest to notice: a *mid-row*
    /// column being mistaken for the end of the row and rewritten to the next one, silently
    /// widening the range to the end of the line.
    ///
    /// `let 漢 = "yy";` is 15 bytes and 13 characters, so byte column 13 - where the string's
    /// content ends - is exactly the coincidence that used to trigger it.
    #[test]
    fn from_treesitter_range_does_not_normalize_a_mid_row_column_that_matches_a_character_count() {
        use tree_sitter::{Point, Range};

        let row = "let 漢 = \"yy\";";
        assert_eq!(row.len(), 15, "fifteen bytes");
        assert_eq!(row.chars().count(), 13, "thirteen characters");
        // Through the real producer, not a hand-written `vec![row.len()]`: the unit has to be
        // right at both ends of the pipeline, and a literal here would only test this function's
        // half of it.
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
        // "—" (em dash) is 3 bytes but 1 character. Columns are byte-based (tree-sitter's `Point`
        // convention - see `row_col_to_byte_index`'s doc comment), so column 4 is right after it
        // ('a' = 1 byte + '—' = 3 bytes = byte 4), not "the 4th character" (which would be 'c').
        let code = "a—bc";
        assert_eq!(row_col_to_byte_index(0, 0, code), Some(0)); // 'a'
        assert_eq!(row_col_to_byte_index(0, 1, code), Some(1)); // '—', right after 'a'
        assert_eq!(row_col_to_byte_index(0, 4, code), Some(4)); // 'b', right after '—'
        assert_eq!(row_col_to_byte_index(0, 5, code), Some(5)); // 'c', right after 'b'
    }

    /// Regression test for a real crash: `is_whitespace_between` used to slice `code` with
    /// character counts instead of byte offsets, so any multi-byte character earlier in the file
    /// (this reproduces one seen in the wild: an em dash, "—") could land a slice mid-character,
    /// panicking with "byte index N is not a char boundary".
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
    fn row_len_is_bytes_not_characters() {
        assert_eq!(row_len_of("abc"), SourceColumn::from_raw(3));
        // Two characters, three bytes. A character count here is the defect these types exist to
        // prevent: tree-sitter would report column 3 for the end of this row, not column 2.
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
        // A CJK ideograph is three bytes and *two* cells - the case a character count also gets
        // wrong, in the other direction.
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
        // Byte column 2 lands inside the three-byte '漢'. A cell is indivisible, so the character
        // counts for nothing rather than partially - and the result stays monotonic in `column`.
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
    /// Every position codediff emits over the whole corpus is a byte column on a character
    /// boundary of its own row, is addressable as a byte offset, and the only text two ranges on
    /// one side may both claim is a line terminator.
    ///
    /// One pass, not one test per invariant: each fixture is loaded and diffed once here, and
    /// that is what this test costs - it was the suite's two slowest tests when the invariants
    /// were checked separately, each diffing all 597 fixtures on its own. Streamed through
    /// `handmade_test_case_dirs` rather than `handmade_test_code_pairs` for the same reason the
    /// benchmark is: nothing here needs two fixtures in memory at once.
    ///
    /// **Byte columns** (see [`super::SourceColumn`]): a character count masquerading as a column
    /// produces one that is either past the row's byte length or inside a multi-byte character,
    /// and both are invisible until something slices there. What this does **not** catch,
    /// checked rather than assumed: reintroducing the character count in
    /// `compute_row_byte_lengths` leaves this passing, because a mis-normalised end is still a
    /// legal position - `(row + 1, 0)` always is. The guard for the *unit* is
    /// `from_treesitter_range_does_not_normalize_a_mid_row_column_that_matches_a_character_count`
    /// (fast, and it does fail on that regression) together with
    /// `headless::tests::a_highlight_covers_the_same_text_on_ascii_and_non_ascii_rows`, which
    /// catches it where a user would see it. This covers the neighbouring class those two do not.
    ///
    /// **Overlaps**: a measurement turned into an invariant. Across the corpus there are 7620
    /// overlapping pairs on 59 fixtures, and *all 7620* share exactly one byte, always `\n`:
    /// `from_treesitter_range` normalises an end landing at the end of a row to `(row + 1, 0)`,
    /// which in byte terms is one past `(row, row_len)` and so takes in the newline, while the
    /// next range starts at that newline. Every consumer excludes newlines already - `label_bytes`
    /// nulls them outright, `columns_on_row` stops at the row's trimmed content - which is why
    /// this has never mis-rendered anything. Two ranges sharing a *character* would be a genuine
    /// disagreement about real code, which the renderers and the scorer resolve by different rules
    /// (highest verdict vs. last writer). This fails if one ever appears.
    ///
    /// Deliberately not `#[ignore]`d, unlike `byte_index_agrees_with_a_linear_walk_on_the_corpus`
    /// next door - that guards an optimisation against a reference implementation and is only
    /// interesting when someone touches it, whereas any change to position handling anywhere can
    /// break this one.
    #[test]
    fn corpus_ranges_are_addressable_byte_columns_sharing_nothing_but_line_terminators() {
        let cases = crate::test::helper::handmade_test_case_dirs().expect("corpus");
        let checked = std::sync::atomic::AtomicUsize::new(0);

        // One thread per core over slices of the corpus: this is the suite's slowest test, and
        // the fixtures are independent. A failing assertion panics its thread, which `scope`
        // re-raises on join, so a failure is still a failure.
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
                        // A row one past the last is the normalized "end of file" form, and a
                        // column of 0 on it is the only legal position there.
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
