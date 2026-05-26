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

use tree_sitter::{Node, Tree};

use crate::{code::Code, diff::ASTDiff, diff::text_range::TextRange};

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
#[derive(Debug, Clone)]
pub struct TextDiff {
    /// Cache of already computed ranges.
    cache: Vec<RangeMatch>,
    /// The before AST tree.
    before_tree: Option<Tree>,
    /// The after AST tree.
    after_tree: Option<Tree>,
}

impl Default for TextDiff {
    fn default() -> Self {
        Self {
            cache: Vec::new(),
            before_tree: None,
            after_tree: None,
        }
    }
}

impl TextDiff {
    /// Construct the TextDiff from an ASTDiff.
    ///
    /// An ASTDiff must exist to create the TextDiff. There is no algorithm currently
    /// implemented that can construct the TextDiff directly from code.
    pub fn from(before: &Code, after: &Code, _diff: &ASTDiff) -> Self {
        Self {
            cache: Vec::new(),
            before_tree: before.ast.clone(),
            after_tree: after.ast.clone(),
        }
    }

    /// For the given side of the diff, return all Ranges.
    ///
    /// The result is a vector of (Range, Operation, Option<Range>) tuples.
    pub fn all(&self, _side: usize) -> Vec<RangeMatch> {
        // Simply calling the ranged version for the entire file will do.
        unimplemented!("To be vibecoded")
    }

    /// For the given range and side of the diff, return all RangeMatches.
    ///
    /// Note that the union of the resulting matches will cover the input range, but it **can**
    /// be bigger than the input range. In other words, we will not return partial ranges, but
    /// rather the biggest range possible for the first and last operation in the result.
    pub fn for_range(&self, range: &TextRange, side: usize) -> Vec<RangeMatch> {
        // Get the tree for the specified side
        let tree = match side {
            0 => self.before_tree.as_ref(),
            1 => self.after_tree.as_ref(),
            _ => return Vec::new(),
        };

        let Some(tree) = tree else {
            return Vec::new();
        };

        let root_node = tree.root_node();

        // Stack-based pre-order traversal for n-ary trees.
        // Pre-order for n-ary: visit node, then child[0..n] subtrees left to right.
        let mut stack: Vec<Node> = Vec::new();
        let mut result: Vec<RangeMatch> = Vec::new();

        // Start with root node
        stack.push(root_node);

        while let Some(node) = stack.pop() {
            // Check if this node has children
            let mut cursor = node.walk();
            let children: Vec<Node> = node.children(&mut cursor).collect();

            if children.is_empty() {
                // Leaf node - create a range and check intersection
                let node_range = TextRange::from_treesitter_range(node.range());
                if node_range.intersects(range) {
                    result.push(RangeMatch {
                        source: node_range,
                        operation: TextOperation::NotYetSet,
                        destination: TextRange::new(0, 0, 0, 0),
                    });
                }
            } else {
                // Push children in reverse order so leftmost is processed first
                for child in children.into_iter().rev() {
                    stack.push(child);
                }
            }
        }

        result
    }
}

/**
* A textual range match. For a given source match, it provides the operation for that range and
* optionally the matching range on the destination side.
*
* Note that it doesn't use before or after terms on purpose.
*/
#[derive(Debug, Clone, PartialEq)]
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

#[cfg(test)]
mod tests {
    use crate::test;
    use anyhow::Result;

    use super::*;

    #[test]
    fn no_change_all_ranges() -> Result<()> {
        let diffs = test::helper::handmade_test_diffs(true, false)?;
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
        let diffs = test::helper::handmade_test_diffs(true, false)?;

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
        assert_eq!(before_ranges[1].destination.start_row, 2);
        assert_eq!(before_ranges[1].destination.start_column, 0);
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
        assert_eq!(after_ranges[1].source.start_column, 0);
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
        let diffs = test::helper::handmade_test_diffs(true, false)?;

        let (before, after, diff) = diffs.get("python-added-if-block").unwrap().clone();

        let text_diff = TextDiff::from(&before, &after, &diff.ast.unwrap());

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

        // Another empty range for the added whitespace.
        // Note that this is a very interesting use case, since whitespace "disappears" from the AST
        // in some sense, but in Python whitespace is extremely important and highlighting it in the
        // textual diff is very important.
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

        // The added identation.
        assert_eq!(after_ranges[3].operation, TextOperation::Insert);
        assert_eq!(after_ranges[3].source.start_row, 22);
        assert_eq!(after_ranges[3].source.start_column, 5);
        assert_eq!(after_ranges[3].source.end_row, 22);
        assert_eq!(after_ranges[3].source.end_column, 10);
        assert_eq!(after_ranges[3].destination.start_row, 21);
        assert_eq!(after_ranges[3].destination.start_column, 5);
        assert_eq!(after_ranges[3].destination.end_row, 21);
        assert_eq!(after_ranges[3].destination.end_column, 5);

        // The matched existing implementation.
        assert_eq!(after_ranges[4].operation, TextOperation::Move);
        assert_eq!(after_ranges[4].source.start_row, 22);
        assert_eq!(after_ranges[4].source.start_column, 5);
        assert_eq!(after_ranges[4].source.end_row, 23);
        assert_eq!(after_ranges[4].source.end_column, 0);
        assert_eq!(after_ranges[4].destination.start_row, 21);
        assert_eq!(after_ranges[4].destination.start_column, 5);
        assert_eq!(after_ranges[4].destination.end_row, 22);
        assert_eq!(after_ranges[4].destination.end_column, 0);

        Ok(())
    }
}
