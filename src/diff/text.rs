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

use crate::diff::{ASTDiff, ASTMappingOperation};

/**
* The API that can be used to transform the AST Diff, which has no inherent visualization, into a
* textual 2D visualization, commonly used in IDEs to show textual code.
*
* This is a datastructure with an API instead of simply being a vector of ranges because we want
* the ability to partially look up ranges for large files efficiently. To enable this, most
* information in this datastructure is lazy loaded on demand. Only very efficient computations are
* pre-computed. The goal is to be able to generate this datastructure from a complete AST Diff of
* twenty thousand nodes for a file of about two thousands lines in 20 ms. Each cold query should
* complete in 5 ms, and subsequent queries that ask for ranges that overlap with already computed
* ranges should complete in 1 ms or less.
*/
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextDiff {}

impl TextDiff {
    /// Construct the PointDiff from an ASTDiff.
    ///
    /// An ASTDiff must exist to create the PointDiff. There is no algorithm currently
    /// implemented that can construct the PointDiff directly from code.
    pub fn from(_diff: &ASTDiff) -> Self {
        unimplemented!("PointDiff::from is not yet implemented")
    }

    /// For the given side of the diff, return all Ranges.
    ///
    /// The result is a vector of (Range, Operation, Option<Range>) tuples.
    pub fn all(&self, _side: usize) -> Vec<RangeMatch> {
        unimplemented!("To be vibecoded")
    }
}

/**
* A textual range match. For a given source match, it provides the operation for that range and
* optionally the matching range on the destination side.
*
* Note that it doesn't use before or after terms on purpose.
*/
pub struct RangeMatch {
    pub source: TextRange,
    pub operation: TextOperation,
    pub destination: TextRange,
}

/**
* The diff operation.
*
* Why not re-use ASTMappingOperation struct? It's not a 1:1 match. For example "InsertWithChildren"
* is not a valid textual operation.
*/
#[derive(Debug, Clone, Default, PartialEq)]
pub enum TextOperation {
    #[default]
    /// Sentinel value.
    NotYetSet,
    /// The ranges are identical.
    Identical,
    /// The range was moved somewhere else.
    Move,
    /// The text in the range differs.
    Update,
    /// The range was inserted.
    Insert,
    /// The range was deleted.
    Delete,
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
pub struct TextRange {
    start_row: usize,
    start_column: usize,
    end_row: usize,
    end_column: usize,
}

#[cfg(test)]
mod tests {
    use crate::test;
    use anyhow::Result;
    use tree_sitter::Point;

    use super::*;

    #[test]
    fn python_leetcode_1_added_if_block() -> Result<()> {
        let test_diffs = test::helper::handmade_test_diffs(true, false)?;

        let diff = test_diffs.get("python-added-if-block").unwrap().clone();

        let text_diff = TextDiff::from(&diff.ast.unwrap());

        let before_ranges = text_diff.all(0);
        assert_eq!(before_ranges.len(), 5);

        assert_eq!(before_ranges[0].operation, TextOperation::Identical);
        assert_eq!(before_ranges[0].source.start_row, 0);
        assert_eq!(before_ranges[0].source.start_column, 0);
        assert_eq!(before_ranges[0].source.end_row, 21);
        assert_eq!(before_ranges[0].source.end_column, 0);
        assert_eq!(before_ranges[0].destination.start_row, 0);
        assert_eq!(before_ranges[0].destination.start_column, 0);
        assert_eq!(before_ranges[0].destination.end_row, 21);
        assert_eq!(before_ranges[0].destination.end_column, 0);

        // This is a "empty range" that indicates something exists here in the other side.
        assert_eq!(before_ranges[1].source.operation, TextOperation::Delete);
        assert_eq!(before_ranges[1].source.start_row, 21);
        assert_eq!(before_ranges[1].source.start_column, 0);
        assert_eq!(before_ranges[1].source.end_row, 21);
        assert_eq!(before_ranges[1].source.end_column, 0);
        assert_eq!(before_ranges[1].destination.start_row, 21);
        assert_eq!(before_ranges[1].destination.start_column, 0);
        assert_eq!(before_ranges[1].destination.end_row, 22);
        assert_eq!(before_ranges[1].destination.end_column, 0);

        // Note the order between the empty range and the actual range that exists. The empty range
        // must always be before an actual existing range, even if their start point is equal.
        assert_eq!(before_ranges[2].operation, TextOperation::Identical);
        assert_eq!(before_ranges[2].source.start_row, 21);
        assert_eq!(before_ranges[2].source.start_column, 0);
        assert_eq!(before_ranges[2].source.end_row, 21);
        assert_eq!(before_ranges[2].source.end_column, 5);
        assert_eq!(before_ranges[2].destination.start_row, 22);
        assert_eq!(before_ranges[2].destination.start_column, 0);
        assert_eq!(before_ranges[2].destination.end_row, 22);
        assert_eq!(before_ranges[2].destination.end_column, 5);

        // Another empty range for the added whitespace.
        assert_eq!(before_ranges[3].operation, TextOperation::Delete);
        assert_eq!(before_ranges[3].source.start_row, 21);
        assert_eq!(before_ranges[3].source.start_column, 5);
        assert_eq!(before_ranges[3].source.end_row, 21);
        assert_eq!(before_ranges[3].source.end_column, 5);
        assert_eq!(before_ranges[3].destination.start_row, 22);
        assert_eq!(before_ranges[3].destination.start_column, 5);
        assert_eq!(before_ranges[3].destination.end_row, 22);
        assert_eq!(before_ranges[3].destination.end_column, 10);

        // Again, the ordering is well defined and mandatory.
        // This is the line that was idented, so it is moved but otherwise identical.
        assert_eq!(before_ranges[4].operation, TextOperation::Move);
        assert_eq!(before_ranges[4].source.start_row, 21);
        assert_eq!(before_ranges[4].source.start_column, 5);
        assert_eq!(before_ranges[4].source.end_row, 22);
        assert_eq!(before_ranges[4].source.end_column, 0);
        assert_eq!(before_ranges[4].destination.start_row, 22);
        assert_eq!(before_ranges[4].destination.start_column, 5);
        assert_eq!(before_ranges[4].destination.end_row, 23);
        assert_eq!(before_ranges[4].destination.end_column, 0);

        let after_ranges = text_diff.all(1);
        // Note the symetric relationships between source and destination ranges in the
        // before_ranges and after_ranges vectors.
        assert_eq!(after_ranges.len(), 5);

        assert_eq!(before_ranges[0].operation, TextOperation::Identical);
        assert_eq!(before_ranges[0].source.start_row, 0);
        assert_eq!(before_ranges[0].source.start_column, 0);
        assert_eq!(before_ranges[0].source.end_row, 21);
        assert_eq!(before_ranges[0].source.end_column, 0);
        assert_eq!(before_ranges[0].destination.start_row, 0);
        assert_eq!(before_ranges[0].destination.start_column, 0);
        assert_eq!(before_ranges[0].destination.end_row, 21);
        assert_eq!(before_ranges[0].destination.end_column, 0);

        // The added "if" conditional.
        assert_eq!(before_ranges[1].operation, TextOperation::Insert);
        assert_eq!(before_ranges[1].source.start_row, 21);
        assert_eq!(before_ranges[1].source.start_column, 0);
        assert_eq!(before_ranges[1].source.end_row, 22);
        assert_eq!(before_ranges[1].source.end_column, 0);
        assert_eq!(before_ranges[1].destination.start_row, 21);
        assert_eq!(before_ranges[1].destination.start_column, 0);
        assert_eq!(before_ranges[1].destination.end_row, 21);
        assert_eq!(before_ranges[1].destination.end_column, 0);

        // The spaces that are the same on both sides.
        assert_eq!(before_ranges[2].operation, TextOperation::Identical);
        assert_eq!(before_ranges[2].source.start_row, 22);
        assert_eq!(before_ranges[2].source.start_column, 0);
        assert_eq!(before_ranges[2].source.end_row, 22);
        assert_eq!(before_ranges[2].source.end_column, 5);
        assert_eq!(before_ranges[2].destination.start_row, 21);
        assert_eq!(before_ranges[2].destination.start_column, 0);
        assert_eq!(before_ranges[2].destination.end_row, 21);
        assert_eq!(before_ranges[2].destination.end_column, 5);

        // The added identation.
        assert_eq!(before_ranges[3].operation, TextOperation::Insert);
        assert_eq!(before_ranges[3].source.start_row, 22);
        assert_eq!(before_ranges[3].source.start_column, 5);
        assert_eq!(before_ranges[3].source.end_row, 22);
        assert_eq!(before_ranges[3].source.end_column, 10);
        assert_eq!(before_ranges[3].destination.start_row, 21);
        assert_eq!(before_ranges[3].destination.start_column, 5);
        assert_eq!(before_ranges[3].destination.end_row, 21);
        assert_eq!(before_ranges[3].destination.end_column, 5);

        // The matched existing implementation.
        assert_eq!(before_ranges[4].operation, TextOperation::Move);
        assert_eq!(before_ranges[4].source.start_row, 22);
        assert_eq!(before_ranges[4].source.start_column, 5);
        assert_eq!(before_ranges[4].source.end_row, 23);
        assert_eq!(before_ranges[4].source.end_column, 0);
        assert_eq!(before_ranges[4].destination.start_row, 21);
        assert_eq!(before_ranges[4].destination.start_column, 5);
        assert_eq!(before_ranges[4].destination.end_row, 22);
        assert_eq!(before_ranges[4].destination.end_column, 0);

        Ok(())
    }
}
