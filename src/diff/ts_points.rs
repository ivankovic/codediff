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

use tree_sitter::{Point, Range};

use crate::diff::{ASTDiff, ASTMappingOperation};

/*
* Holds the API and necessary data to query an AST based diff using TreeSitter Point structures as
* reference points. TreeSitter Points are row-column based points in the textual representation of
* the code. When the diff is displayed to humans in textual form, this structure helps keep the
* translation logic from the tree based diff, which has no inherent visual form, to the two
* dimensions of text.
*
* This is very useful in text editors or TUI apps.
*/
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TSPointDiff {}

impl TSPointDiff {
    /// Construct the PointDiff from an ASTDiff.
    ///
    /// An ASTDiff must exist to create the PointDiff. There is no algorithm currently
    /// implemented that can construct the PointDiff directly from code.
    pub fn from(_diff: &ASTDiff) -> Self {
        unimplemented!("PointDiff::from is not yet implemented")
    }

    /// For a given side of the diff and a Point, return the diff information. The side is 0 for
    /// before and 1 for after.
    ///
    /// Returns the operation, the range this point belongs to. The range is the widest consecutive
    /// sequence of points that contains the given input point and has the same operation. We
    /// return the range because in practice, it would be the "same colour" area on the screen.
    ///
    /// If the operation is not Insert or Delete (i.e. it has a matching range on the other side)
    /// the matching range in the other code is also returned.
    pub fn for_point(
        &self,
        _side: usize,
        _point: &Point,
    ) -> (Range, ASTMappingOperation, Option<Range>) {
        unimplemented!("PointDiff::for_point is not yet implemented")
    }

    /// For the given side of the diff and Range, return the diff information.
    ///
    /// The result is a vector of (Range, Operation, Option<Range>) tuples. The results are a vector because the
    /// input range can span multiple AST nodes and multiple operations. However, the union of all
    /// returned ranges will always exactly match the input range.
    pub fn for_range(
        &self,
        _side: usize,
        _range: &Range,
    ) -> Vec<(Range, ASTMappingOperation, Option<Range>)> {
        unimplemented!("To be vibecoded")
    }
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

        let point_diff = TSPointDiff::from(&diff.ast.unwrap());

        let (range, operation, matching_range) = point_diff.for_point(0, &Point::new(0, 0));
        assert_eq!(operation, ASTMappingOperation::Identical);
        assert_eq!(range.start_point.row, 0);
        assert_eq!(range.start_point.column, 0);
        assert_eq!(range.end_point.row, 20);
        assert_eq!(range.end_point.column, 0);
        assert!(matching_range.is_some());
        let matching_range = matching_range.unwrap();
        assert_eq!(matching_range.start_point.row, 0);
        assert_eq!(matching_range.start_point.column, 0);
        assert_eq!(matching_range.end_point.row, 20);
        assert_eq!(matching_range.end_point.column, 0);

        let (range, operation, matching_range) = point_diff.for_point(0, &Point::new(21, 5));
        assert_eq!(operation, ASTMappingOperation::Insert);
        assert_eq!(range.start_point.row, 21);
        assert_eq!(range.start_point.column, 0);
        assert_eq!(range.end_point.row, 22);
        assert_eq!(range.end_point.column, 0);
        assert!(matching_range.is_none());

        let (range, operation, matching_range) = point_diff.for_point(0, &Point::new(22, 9));
        assert_eq!(operation, ASTMappingOperation::Identical);
        assert_eq!(range.start_point.row, 22);
        assert_eq!(range.start_point.column, 0);
        assert_eq!(range.end_point.row, 23);
        assert_eq!(range.end_point.column, 0);
        assert!(matching_range.is_some());
        let matching_range = matching_range.unwrap();
        assert_eq!(matching_range.start_point.row, 21);
        assert_eq!(matching_range.start_point.column, 5);
        assert_eq!(matching_range.end_point.row, 22);
        assert_eq!(matching_range.end_point.column, 0);

        Ok(())
    }
}
