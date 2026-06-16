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

use crate::{
    code::Code,
    diff::{ASTDiff, ASTMappingOperation, NodeCache, text_range::TextRange},
};

/**
* The API that can be used to transform the AST Diff, which has no inherent visualization, into a
* textual 2D visualization, commonly used in IDEs to show textual code.
*
* One crucial design choice with TextDiff is how it handles whitespace. In principle, whitespace is
* completely ignored, except when it causes differences in the parsed AST, notably as part of
* constants.
*
* This is a datastructure with an API instead of simply being a vector of ranges because we want
* the ability to partially look up ranges for large files efficiently.
*/
#[derive(Debug, Clone)]
pub struct TextDiff {
    // TODO: A much more complex tree-based structure for very large files.
    before_ranges: Vec<RangeMatch>,
    after_ranges: Vec<RangeMatch>,
}

impl Default for TextDiff {
    fn default() -> Self {
        Self {
            before_ranges: Vec::new(),
            after_ranges: Vec::new(),
        }
    }
}

/// Returns the RangeMatches from source to destination.
fn ranges(
    source: &Code,
    destination: &Code,
    diff: &ASTDiff,
    node_cache: &NodeCache,
) -> Vec<RangeMatch> {
    let mut ranges = Vec::new();

    match (&source.ast, &destination.ast) {
        (None, None) => {
            // If there is no code on either side, there is no diff.
            // We simply leave ranges empty and let the match complete.
        }
        (Some(source_tree), None) => {
            let source_root = source_tree.root_node();
            let source_range = TextRange::from_treesitter_range(source_root.range());

            ranges.push(RangeMatch {
                source: source_range.clone(),
                destination: TextRange::zero(),
                operation: TextOperation::Delete,
            });
        }
        (None, Some(destination_tree)) => {
            let destination_root = destination_tree.root_node();
            let destination_range = TextRange::from_treesitter_range(destination_root.range());

            ranges.push(RangeMatch {
                source: TextRange::zero(),
                destination: destination_range.clone(),
                operation: TextOperation::Insert,
            });
        }
        (Some(source_tree), Some(_)) => {
            let root_node = source_tree.root_node();

            // We perform a pre-order traversal of the source tree and look for nodes with known
            // TextRanges.
            let mut stack = Vec::new();
            stack.push(root_node);

            while let Some(node) = stack.pop() {
                if let Some((mapped_id, mapping)) = diff.mapping_for_node(&node.id()) {
                    match mapping.operation {
                        ASTMappingOperation::Identical => {
                            if let Some(destination_node) = node_cache.get_in_any(&mapped_id) {
                                ranges.push(RangeMatch {
                                    source: TextRange::from_treesitter_range(node.range()),
                                    destination: TextRange::from_treesitter_range(
                                        destination_node.range(),
                                    ),
                                    operation: TextOperation::Identical,
                                });

                                // Don't descend
                                continue;
                            }
                        }
                        _ => {
                            // For other operations, just allow the descent into the tree
                        }
                    }
                }

                let mut child_cursor = node.walk();
                for child in node.children(&mut child_cursor) {
                    stack.push(child);
                }
            }
        }
    }

    ranges
}

impl TextDiff {
    /// Construct the TextDiff from an ASTDiff.
    ///
    /// An ASTDiff must exist to create the TextDiff. There is no algorithm currently
    /// implemented that can construct the TextDiff directly from code.
    pub fn from(before: &Code, after: &Code, diff: &ASTDiff, node_cache: &NodeCache) -> Self {
        Self {
            before_ranges: ranges(before, after, diff, node_cache),
            after_ranges: ranges(after, before, diff, node_cache),
        }
    }

    /// For the given side of the diff, return all Ranges.
    ///
    /// The result is a vector of (Range, Operation, Option<Range>) tuples.
    pub fn all(&self, side: usize) -> Vec<RangeMatch> {
        if side == 0 {
            return self.before_ranges.clone();
        }
        self.after_ranges.clone()
    }

    /// For the given range and side of the diff, return all RangeMatches.
    ///
    /// Note that the union of the resulting matches will cover the input range, but it **can**
    /// be bigger than the input range. In other words, we will not return partial ranges, but
    /// rather the biggest range possible for the first and last operation in the result.
    pub fn for_range(&self, _range: &TextRange, _side: usize) -> Vec<RangeMatch> {
        unimplemented!("TODO: Implement fast, tree based storage and implement this method")
    }
}

/**
* A textual range match. For a given source match, it provides the operation for that range and
* optionally the matching range on the destination side.
*
* Note that it doesn't use before or after terms on purpose, because it is used for both
* before-to-after and after-to-before ranges.
*/
#[derive(Debug, Clone, PartialEq)]
pub struct RangeMatch {
    pub source: TextRange,
    pub destination: TextRange,
    pub operation: TextOperation,
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

#[cfg(test)]
mod tests {
    use crate::test;
    use anyhow::Result;

    use super::*;

    #[test]
    fn no_change_all_ranges() -> Result<()> {
        let diffs = test::helper::handmade_test_diffs(true, false, "no-change")?;
        let (before, after, diff) = diffs.get("no-change").unwrap().clone();

        let text_diff = TextDiff::from(&before, &after, &diff.ast.unwrap());

        let before_ranges = text_diff.all(0);
        assert_eq!(before_ranges.len(), 1);

        let after_ranges = text_diff.all(1);
        assert_eq!(after_ranges.len(), 1);

        assert_eq!(before_ranges[0].operation, TextOperation::Identical);
        assert_eq!(before_ranges[0].source.start_row, 0);
        assert_eq!(before_ranges[0].source.start_column, 0);
        assert_eq!(before_ranges[0].source.end_row, 50);
        assert_eq!(before_ranges[0].source.end_column, 0);
        assert_eq!(before_ranges[0].destination.start_row, 0);
        assert_eq!(before_ranges[0].destination.start_column, 0);
        assert_eq!(before_ranges[0].destination.end_row, 50);
        assert_eq!(before_ranges[0].destination.end_column, 0);

        assert_eq!(after_ranges[0].operation, TextOperation::Identical);
        assert_eq!(after_ranges[0].source.start_row, 0);
        assert_eq!(after_ranges[0].source.start_column, 0);
        assert_eq!(after_ranges[0].source.end_row, 50);
        assert_eq!(after_ranges[0].source.end_column, 0);
        assert_eq!(after_ranges[0].destination.start_row, 0);
        assert_eq!(after_ranges[0].destination.start_column, 0);
        assert_eq!(after_ranges[0].destination.end_row, 50);
        assert_eq!(after_ranges[0].destination.end_column, 0);

        Ok(())
    }

    #[test]
    fn hellow_world_added_message_all_ranges() -> Result<()> {
        let diffs = test::helper::handmade_test_diffs(true, false, "hello-world-added-message")?;

        let (before, after, diff) = diffs.get("hello-world-added-message").unwrap().clone();

        let text_diff = TextDiff::from(&before, &after, &diff.ast.unwrap());

        let before_ranges = text_diff.all(0);
        assert_eq!(before_ranges.len(), 3);

        assert_eq!(before_ranges[0].operation, TextOperation::Identical);
        assert_eq!(before_ranges[0].source.start_row, 0);
        assert_eq!(before_ranges[0].source.start_column, 0);
        assert_eq!(before_ranges[0].source.end_row, 2);
        assert_eq!(before_ranges[0].source.end_column, 0);
        assert_eq!(before_ranges[0].destination.start_row, 0);
        assert_eq!(before_ranges[0].destination.start_column, 0);
        assert_eq!(before_ranges[0].destination.end_row, 2);
        assert_eq!(before_ranges[0].destination.end_column, 0);

        assert_eq!(before_ranges[1].operation, TextOperation::Delete);
        assert_eq!(before_ranges[1].source.start_row, 2);
        assert_eq!(before_ranges[1].source.start_column, 0);
        assert_eq!(before_ranges[1].source.end_row, 2);
        assert_eq!(before_ranges[1].source.end_column, 0);
        // Note that because we ignore whitespace, the [(2, 0), (2, 2)> range is simply missing from
        // the result.
        assert_eq!(before_ranges[1].destination.start_row, 2);
        assert_eq!(before_ranges[1].destination.start_column, 2);
        assert_eq!(before_ranges[1].destination.end_row, 3);
        assert_eq!(before_ranges[1].destination.end_column, 0);

        assert_eq!(before_ranges[2].operation, TextOperation::Identical);
        assert_eq!(before_ranges[2].source.start_row, 2);
        assert_eq!(before_ranges[2].source.start_column, 0);
        assert_eq!(before_ranges[2].source.end_row, 3);
        assert_eq!(before_ranges[2].source.end_column, 0);
        assert_eq!(before_ranges[2].destination.start_row, 3);
        assert_eq!(before_ranges[2].destination.start_column, 0);
        assert_eq!(before_ranges[2].destination.end_row, 4);
        assert_eq!(before_ranges[2].destination.end_column, 0);

        let after_ranges = text_diff.all(1);
        assert_eq!(after_ranges.len(), 3);

        assert_eq!(after_ranges[0].operation, TextOperation::Identical);
        assert_eq!(after_ranges[0].source.start_row, 0);
        assert_eq!(after_ranges[0].source.start_column, 0);
        assert_eq!(after_ranges[0].source.end_row, 2);
        assert_eq!(after_ranges[0].source.end_column, 0);
        assert_eq!(after_ranges[0].destination.start_row, 0);
        assert_eq!(after_ranges[0].destination.start_column, 0);
        assert_eq!(after_ranges[0].destination.end_row, 2);
        assert_eq!(after_ranges[0].destination.end_column, 0);

        assert_eq!(after_ranges[1].operation, TextOperation::Insert);
        assert_eq!(after_ranges[1].source.start_row, 2);
        assert_eq!(after_ranges[1].source.start_column, 2);
        assert_eq!(after_ranges[1].source.end_row, 3);
        assert_eq!(after_ranges[1].source.end_column, 0);
        assert_eq!(after_ranges[1].destination.start_row, 2);
        assert_eq!(after_ranges[1].destination.start_column, 0);
        assert_eq!(after_ranges[1].destination.end_row, 2);
        assert_eq!(after_ranges[1].destination.end_column, 0);

        assert_eq!(after_ranges[2].operation, TextOperation::Identical);
        assert_eq!(after_ranges[2].source.start_row, 3);
        assert_eq!(after_ranges[2].source.start_column, 0);
        assert_eq!(after_ranges[2].source.end_row, 4);
        assert_eq!(after_ranges[2].source.end_column, 0);
        assert_eq!(after_ranges[2].destination.start_row, 3);
        assert_eq!(after_ranges[2].destination.start_column, 0);
        assert_eq!(after_ranges[2].destination.end_row, 3);
        assert_eq!(after_ranges[2].destination.end_column, 0);

        Ok(())
    }

    #[test]
    fn python_leetcode_1_added_if_block_all_ranges() -> Result<()> {
        let diffs = test::helper::handmade_test_diffs(true, false, "python-added-if-block")?;

        let (before, after, diff) = diffs.get("python-added-if-block").unwrap().clone();

        let text_diff = TextDiff::from(&before, &after, &diff.ast.unwrap());

        let before_ranges = text_diff.all(0);
        assert_eq!(before_ranges.len(), 4);

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
        assert_eq!(before_ranges[1].operation, TextOperation::Delete);
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

        // This is where whitespace WOULD be, but we ignore it on purpose.
        // assert_eq!(before_ranges[X].operation, TextOperation::Delete);
        // assert_eq!(before_ranges[X].source.start_row, 21);
        // assert_eq!(before_ranges[X].source.start_column, 5);
        // assert_eq!(before_ranges[X].source.end_row, 21);
        // assert_eq!(before_ranges[X].source.end_column, 5);
        // assert_eq!(before_ranges[X].destination.start_row, 22);
        // assert_eq!(before_ranges[X].destination.start_column, 5);
        // assert_eq!(before_ranges[X].destination.end_row, 22);
        // assert_eq!(before_ranges[X].destination.end_column, 10);

        // Again, the ordering is well defined and mandatory.
        // This is the line that was idented, so it is moved but otherwise identical.
        assert_eq!(before_ranges[3].operation, TextOperation::Move);
        assert_eq!(before_ranges[3].source.start_row, 21);
        assert_eq!(before_ranges[3].source.start_column, 5);
        assert_eq!(before_ranges[3].source.end_row, 22);
        assert_eq!(before_ranges[3].source.end_column, 0);
        assert_eq!(before_ranges[3].destination.start_row, 22);
        assert_eq!(before_ranges[3].destination.start_column, 5);
        assert_eq!(before_ranges[3].destination.end_row, 23);
        assert_eq!(before_ranges[3].destination.end_column, 0);

        let after_ranges = text_diff.all(1);
        // Note the symetric relationships between source and destination ranges in the
        // before_ranges and after_ranges vectors.
        assert_eq!(after_ranges.len(), before_ranges.len());

        assert_eq!(after_ranges[0].operation, TextOperation::Identical);
        assert_eq!(after_ranges[0].source.start_row, 0);
        assert_eq!(after_ranges[0].source.start_column, 0);
        assert_eq!(after_ranges[0].source.end_row, 21);
        assert_eq!(after_ranges[0].source.end_column, 0);
        assert_eq!(after_ranges[0].destination.start_row, 0);
        assert_eq!(after_ranges[0].destination.start_column, 0);
        assert_eq!(after_ranges[0].destination.end_row, 21);
        assert_eq!(after_ranges[0].destination.end_column, 0);

        // The added "if" conditional.
        assert_eq!(after_ranges[1].operation, TextOperation::Insert);
        assert_eq!(after_ranges[1].source.start_row, 21);
        assert_eq!(after_ranges[1].source.start_column, 0);
        assert_eq!(after_ranges[1].source.end_row, 22);
        assert_eq!(after_ranges[1].source.end_column, 0);
        assert_eq!(after_ranges[1].destination.start_row, 21);
        assert_eq!(after_ranges[1].destination.start_column, 0);
        assert_eq!(after_ranges[1].destination.end_row, 21);
        assert_eq!(after_ranges[1].destination.end_column, 0);

        // The spaces that are the same on both sides.
        assert_eq!(after_ranges[2].operation, TextOperation::Identical);
        assert_eq!(after_ranges[2].source.start_row, 22);
        assert_eq!(after_ranges[2].source.start_column, 0);
        assert_eq!(after_ranges[2].source.end_row, 22);
        assert_eq!(after_ranges[2].source.end_column, 5);
        assert_eq!(after_ranges[2].destination.start_row, 21);
        assert_eq!(after_ranges[2].destination.start_column, 0);
        assert_eq!(after_ranges[2].destination.end_row, 21);
        assert_eq!(after_ranges[2].destination.end_column, 5);

        // This is where whitespace WOULD be, but we ignore it on purpose.
        // assert_eq!(after_ranges[X].operation, TextOperation::Insert);
        // assert_eq!(after_ranges[X].source.start_row, 22);
        // assert_eq!(after_ranges[X].source.start_column, 5);
        // assert_eq!(after_ranges[X].source.end_row, 22);
        // assert_eq!(after_ranges[X].source.end_column, 10);
        // assert_eq!(after_ranges[X].destination.start_row, 21);
        // assert_eq!(after_ranges[X].destination.start_column, 5);
        // assert_eq!(after_ranges[X].destination.end_row, 21);
        // assert_eq!(after_ranges[X].destination.end_column, 5);

        // The matched existing implementation.
        assert_eq!(after_ranges[3].operation, TextOperation::Move);
        assert_eq!(after_ranges[3].source.start_row, 22);
        assert_eq!(after_ranges[3].source.start_column, 5);
        assert_eq!(after_ranges[3].source.end_row, 23);
        assert_eq!(after_ranges[3].source.end_column, 0);
        assert_eq!(after_ranges[3].destination.start_row, 21);
        assert_eq!(after_ranges[3].destination.start_column, 5);
        assert_eq!(after_ranges[3].destination.end_row, 22);
        assert_eq!(after_ranges[3].destination.end_column, 0);

        Ok(())
    }
}
