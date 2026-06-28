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
    code::{Code, metadata::compute_columns_per_row},
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
#[derive(Debug, Clone, Default)]
pub struct TextDiff {
    // TODO: A much more complex tree-based structure for very large files.
    before_ranges: Vec<RangeMatch>,
    after_ranges: Vec<RangeMatch>,
}

/// Returns the RangeMatches from source to destination.
fn ranges(
    source: &Code,
    destination: &Code,
    diff: &ASTDiff,
    node_cache: &NodeCache,
) -> Vec<RangeMatch> {
    let mut ranges = Vec::new();

    // Compute columns per row for source and destination
    let source_columns = compute_columns_per_row(&source.contents);
    let destination_columns = compute_columns_per_row(&destination.contents);

    match (&source.ast, &destination.ast) {
        (None, None) => {
            // If there is no code on either side, there is no diff.
            // We simply leave ranges empty and let the match complete.
        }
        (Some(source_tree), None) => {
            let source_root = source_tree.root_node();
            let source_range =
                TextRange::from_treesitter_range(source_root.range(), &source_columns);

            ranges.push(RangeMatch {
                source: source_range.clone(),
                destination: TextRange::zero(),
                operation: TextOperation::Delete,
            });
        }
        (None, Some(destination_tree)) => {
            let destination_root = destination_tree.root_node();
            let destination_range =
                TextRange::from_treesitter_range(destination_root.range(), &destination_columns);

            ranges.push(RangeMatch {
                source: TextRange::zero(),
                destination: destination_range.clone(),
                operation: TextOperation::Insert,
            });
        }
        (Some(source_tree), Some(_destination_tree)) => {
            let root_node = source_tree.root_node();

            // We perform a pre-order traversal of the source tree and look for nodes with known
            // TextRanges.
            let mut stack = Vec::new();
            stack.push(root_node);

            let mut last_non_move_range = TextRange::zero();

            let mut current_range = RangeMatch::zero();

            while let Some(node) = stack.pop() {
                if let Some((mapped_id, mapping)) = diff.mapping_for_node(&node.id()) {
                    let mut new_range = None;
                    let mut descend = true;

                    match mapping.operation {
                        ASTMappingOperation::Identical => {
                            if let Some(destination_node) = node_cache.get_in_any(&mapped_id) {
                                let s =
                                    TextRange::from_treesitter_range(node.range(), &source_columns);
                                let d = TextRange::from_treesitter_range(
                                    destination_node.range(),
                                    &destination_columns,
                                );

                                // A matched node whose column changed wasn't just shifted down by
                                // unrelated insertions/deletions elsewhere in the file (which
                                // leaves its column untouched) - it was actually relocated (e.g.
                                // reindented because it's now nested inside a new block). That's
                                // a Move, not an Identical range, and its destination must not
                                // become the new `last_non_move_range` anchor since its position
                                // is out of the normal sequential flow.
                                if s.start_column == d.start_column {
                                    last_non_move_range = d.clone();

                                    new_range = Some(RangeMatch {
                                        source: s,
                                        destination: d,
                                        operation: TextOperation::Identical,
                                    });
                                } else {
                                    new_range = Some(RangeMatch {
                                        source: s,
                                        destination: d,
                                        operation: TextOperation::Move,
                                    });
                                }

                                descend = false;
                            }
                        }
                        ASTMappingOperation::DeleteWithChildren => {
                            // We are adding to the "end" of the last used range, so move it to the
                            // right limit.
                            last_non_move_range = last_non_move_range.right_limit();

                            new_range = Some(RangeMatch {
                                source: TextRange::from_treesitter_range(
                                    node.range(),
                                    &source_columns,
                                ),
                                destination: last_non_move_range.clone(),
                                operation: TextOperation::Delete,
                            });

                            descend = false;
                        }
                        ASTMappingOperation::InsertWithChildren => {
                            // We are adding to the "end" of the last used range, so move it to the
                            // right limit.
                            last_non_move_range = last_non_move_range.right_limit();

                            new_range = Some(RangeMatch {
                                source: TextRange::from_treesitter_range(
                                    node.range(),
                                    &source_columns,
                                ),
                                destination: last_non_move_range.clone(),
                                operation: TextOperation::Insert,
                            });

                            descend = false;
                        }
                        ASTMappingOperation::Delete => {
                            if node.child_count() == 0 {
                                last_non_move_range = last_non_move_range.right_limit();

                                new_range = Some(RangeMatch {
                                    source: TextRange::from_treesitter_range(
                                        node.range(),
                                        &source_columns,
                                    ),
                                    destination: last_non_move_range.clone(),
                                    operation: TextOperation::Delete,
                                });
                            }
                        }
                        ASTMappingOperation::Insert => {
                            if node.child_count() == 0 {
                                last_non_move_range = last_non_move_range.right_limit();

                                new_range = Some(RangeMatch {
                                    source: TextRange::from_treesitter_range(
                                        node.range(),
                                        &source_columns,
                                    ),
                                    destination: last_non_move_range.clone(),
                                    operation: TextOperation::Insert,
                                });
                            }
                        }
                        ASTMappingOperation::Update => {
                            // We are adding to the "end" of the last used range, so move it to the
                            // right limit.
                            last_non_move_range = last_non_move_range.right_limit();

                            new_range = Some(RangeMatch {
                                source: TextRange::from_treesitter_range(
                                    node.range(),
                                    &source_columns,
                                ),
                                destination: last_non_move_range.clone(),
                                operation: TextOperation::Update,
                            });
                        }
                        _ => {
                            // For other operations, just allow the descent into the tree
                        }
                    }

                    if let Some(new_range) = new_range {
                        if new_range.extends(
                            &current_range,
                            &source.contents,
                            &destination.contents,
                        ) {
                            current_range.extend_into(&new_range);
                        } else {
                            if !current_range.is_zero() {
                                ranges.push(current_range);
                            }
                            current_range = new_range;
                        }

                        if !descend {
                            continue;
                        }
                    }
                }

                // Reverse order to ensure the stack is in tree pre-order.
                let mut child_cursor = node.walk();
                let children: Vec<_> = node.children(&mut child_cursor).collect();
                for child in children.into_iter().rev() {
                    stack.push(child);
                }
            }

            if !current_range.is_zero() {
                ranges.push(current_range);
            }
        }
    }

    ranges
}

/// Take the destination range, and merge it into the source range to recover insertions/deletions.
///
/// Inserted node in the destination are invisible in the source AST. This function restores their
/// ranges and makes the range vectors symetric.
fn merge_ranges(
    source_ranges: &[RangeMatch],
    destination_ranges: &[RangeMatch],
) -> Vec<RangeMatch> {
    let mut result = Vec::new();

    let mut i = 0;
    let mut j = 0;

    while i < source_ranges.len() {
        while j < destination_ranges.len()
            && destination_ranges[j].operation == TextOperation::Insert
        {
            result.push(RangeMatch {
                source: destination_ranges[j].destination.clone(),
                destination: destination_ranges[j].source.clone(),
                operation: TextOperation::Delete,
            });
            j += 1;
        }

        result.push(source_ranges[i].clone());

        i += 1;
        j += 1;
    }

    result
}

impl TextDiff {
    /// Construct the TextDiff from an ASTDiff.
    ///
    /// An ASTDiff must exist to create the TextDiff. There is no algorithm currently
    /// implemented that can construct the TextDiff directly from code.
    pub fn from(before: &Code, after: &Code, diff: &ASTDiff, node_cache: &NodeCache) -> Self {
        let before_ranges_plain = ranges(before, after, diff, node_cache);
        let after_ranges_plain = ranges(after, before, diff, node_cache);

        let before_ranges = merge_ranges(&before_ranges_plain, &after_ranges_plain);
        let after_ranges = merge_ranges(&after_ranges_plain, &before_ranges_plain);

        Self {
            before_ranges,
            after_ranges,
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

impl RangeMatch {
    pub fn zero() -> Self {
        RangeMatch {
            source: TextRange::zero(),
            destination: TextRange::zero(),
            operation: TextOperation::NotYetSet,
        }
    }

    pub fn is_zero(&self) -> bool {
        self.source.is_zero()
            && self.destination.is_zero()
            && self.operation == TextOperation::NotYetSet
    }

    pub fn extends(&self, other: &RangeMatch, source_code: &str, dest_code: &str) -> bool {
        if self.operation != other.operation {
            return false;
        }
        self.source
            .can_extend_with_whitespace(&other.source, source_code)
            && self
                .destination
                .can_extend_with_whitespace(&other.destination, dest_code)
    }

    pub fn extend_into(&mut self, other: &RangeMatch) {
        self.source.extend_to_end(&other.source);
        self.destination.extend_to_end(&other.destination);
    }
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
        let code_pairs = test::helper::handmade_test_code_pairs()?;
        let (before, after) = code_pairs.get("rust-no-change").unwrap().clone();
        let node_cache = NodeCache::build(&before, &after);
        let diff = crate::diff::Diff::from_code(&before, &after);

        let text_diff = TextDiff::from(&before, &after, &diff.ast.unwrap(), &node_cache);

        let before_ranges = text_diff.all(0);
        assert_eq!(before_ranges.len(), 1, "Wrong number of before ranges");

        let after_ranges = text_diff.all(1);
        assert_eq!(after_ranges.len(), 1, "Wrong number of after ranges");

        assert_eq!(
            before_ranges[0].operation,
            TextOperation::Identical,
            "The identical part has wrong operation"
        );
        assert_eq!(
            before_ranges[0].source.start_row, 0,
            "The identical part has wrong source start row"
        );
        assert_eq!(
            before_ranges[0].source.start_column, 0,
            "The identical part has wrong source start column"
        );
        assert_eq!(
            before_ranges[0].source.end_row, 49,
            "The identical part has wrong source end row"
        );
        assert_eq!(
            before_ranges[0].source.end_column, 0,
            "The identical part has wrong source end column"
        );
        assert_eq!(
            before_ranges[0].destination.start_row, 0,
            "The identical part has wrong destination start row"
        );
        assert_eq!(
            before_ranges[0].destination.start_column, 0,
            "The identical part has wrong destination start column"
        );
        assert_eq!(
            before_ranges[0].destination.end_row, 49,
            "The identical part has wrong destination end row"
        );
        assert_eq!(
            before_ranges[0].destination.end_column, 0,
            "The identical part has wrong destination end column"
        );

        assert_eq!(
            after_ranges[0].operation,
            TextOperation::Identical,
            "When looking from after to before: The identical part has wrong operation"
        );
        assert_eq!(
            after_ranges[0].source.start_row, 0,
            "When looking from after to before: The identical part has wrong source start row"
        );
        assert_eq!(
            after_ranges[0].source.start_column, 0,
            "When looking from after to before: The identical part has wrong source start column"
        );
        assert_eq!(
            after_ranges[0].source.end_row, 49,
            "When looking from after to before: The identical part has wrong source end row"
        );
        assert_eq!(
            after_ranges[0].source.end_column, 0,
            "When looking from after to before: The identical part has wrong source end column"
        );
        assert_eq!(
            after_ranges[0].destination.start_row, 0,
            "When looking from after to before: The identical part has wrong destination start row"
        );
        assert_eq!(
            after_ranges[0].destination.start_column, 0,
            "When looking from after to before: The identical part has wrong destination start column"
        );
        assert_eq!(
            after_ranges[0].destination.end_row, 49,
            "When looking from after to before: The identical part has wrong destination end row"
        );
        assert_eq!(
            after_ranges[0].destination.end_column, 0,
            "When looking from after to before: The identical part has wrong destination end column"
        );

        Ok(())
    }

    #[test]
    fn hello_world_added_message_all_ranges() -> Result<()> {
        let code_pairs = test::helper::handmade_test_code_pairs()?;
        let (before, after) = code_pairs
            .get("rust-hello-world-added-message")
            .unwrap()
            .clone();
        let node_cache = NodeCache::build(&before, &after);
        let diff = crate::diff::Diff::from_code(&before, &after);

        let text_diff = TextDiff::from(&before, &after, &diff.ast.unwrap(), &node_cache);

        let before_ranges = text_diff.all(0);
        assert_eq!(before_ranges.len(), 3, "Wrong number of before ranges");

        assert_eq!(
            before_ranges[0].operation,
            TextOperation::Identical,
            "The initial identical part has wrong operation"
        );
        assert_eq!(
            before_ranges[0].source.start_row, 0,
            "The initial identical part has wrong source start row"
        );
        assert_eq!(
            before_ranges[0].source.start_column, 0,
            "The initial identical part has wrong source start column"
        );
        assert_eq!(
            before_ranges[0].source.end_row, 2,
            "The initial identical part has wrong source end row"
        );
        assert_eq!(
            before_ranges[0].source.end_column, 0,
            "The initial identical part has wrong source end column"
        );
        assert_eq!(
            before_ranges[0].destination.start_row, 0,
            "The initial identical part has wrong destination start row"
        );
        assert_eq!(
            before_ranges[0].destination.start_column, 0,
            "The initial identical part has wrong destination start column"
        );
        assert_eq!(
            before_ranges[0].destination.end_row, 2,
            "The initial identical part has wrong destination end row"
        );
        assert_eq!(
            before_ranges[0].destination.end_column, 0,
            "The initial identical part has wrong destination end column"
        );

        assert_eq!(
            before_ranges[1].operation,
            TextOperation::Delete,
            "The virtual delete, that marks the 'insert' on the after side, has wrong operation"
        );
        assert_eq!(
            before_ranges[1].source.start_row, 2,
            "The virtual delete, that marks the 'insert' on the after side, has wrong source start row"
        );
        assert_eq!(
            before_ranges[1].source.start_column, 0,
            "The virtual delete, that marks the 'insert' on the after side, has wrong source start column"
        );
        assert_eq!(
            before_ranges[1].source.end_row, 2,
            "The virtual delete, that marks the 'insert' on the after side, has wrong source end row"
        );
        assert_eq!(
            before_ranges[1].source.end_column, 0,
            "The virtual delete, that marks the 'insert' on the after side, has wrong source end column"
        );
        // Note that because we ignore whitespace, the [(2, 0), (2, 2)> range is simply missing from
        // the result.
        assert_eq!(
            before_ranges[1].destination.start_row, 2,
            "The virtual delete, that marks the 'insert' on the after side, has wrong destination start row"
        );
        assert_eq!(
            before_ranges[1].destination.start_column, 2,
            "The virtual delete, that marks the 'insert' on the after side, has wrong destination start column"
        );
        assert_eq!(
            before_ranges[1].destination.end_row, 3,
            "The virtual delete, that marks the 'insert' on the after side, has wrong destination end row"
        );
        assert_eq!(
            before_ranges[1].destination.end_column, 0,
            "The virtual delete, that marks the 'insert' on the after side, has wrong destination end column"
        );

        assert_eq!(
            before_ranges[2].operation,
            TextOperation::Identical,
            "The final identical part has wrong operation"
        );
        assert_eq!(
            before_ranges[2].source.start_row, 2,
            "The final identical part has wrong source start row"
        );
        assert_eq!(
            before_ranges[2].source.start_column, 0,
            "The final identical part has wrong source start column"
        );
        assert_eq!(
            before_ranges[2].source.end_row, 3,
            "The final identical part has wrong source end row"
        );
        assert_eq!(
            before_ranges[2].source.end_column, 0,
            "The final identical part has wrong source end column"
        );
        assert_eq!(
            before_ranges[2].destination.start_row, 3,
            "The final identical part has wrong destination start row"
        );
        assert_eq!(
            before_ranges[2].destination.start_column, 0,
            "The final identical part has wrong destination start column"
        );
        assert_eq!(
            before_ranges[2].destination.end_row, 4,
            "The final identical part has wrong destination end row"
        );
        assert_eq!(
            before_ranges[2].destination.end_column, 0,
            "The final identical part has wrong destination end column"
        );

        let after_ranges = text_diff.all(1);
        assert_eq!(
            after_ranges.len(),
            3,
            "When looking from after to before: Wrong number of after ranges"
        );

        assert_eq!(
            after_ranges[0].operation,
            TextOperation::Identical,
            "When looking from after to before: The initial identical part has wrong operation"
        );
        assert_eq!(
            after_ranges[0].source.start_row, 0,
            "When looking from after to before: The initial identical part has wrong source start row"
        );
        assert_eq!(
            after_ranges[0].source.start_column, 0,
            "When looking from after to before: The initial identical part has wrong source start column"
        );
        assert_eq!(
            after_ranges[0].source.end_row, 2,
            "When looking from after to before: The initial identical part has wrong source end row"
        );
        assert_eq!(
            after_ranges[0].source.end_column, 0,
            "When looking from after to before: The initial identical part has wrong source end column"
        );
        assert_eq!(
            after_ranges[0].destination.start_row, 0,
            "When looking from after to before: The initial identical part has wrong destination start row"
        );
        assert_eq!(
            after_ranges[0].destination.start_column, 0,
            "When looking from after to before: The initial identical part has wrong destination start column"
        );
        assert_eq!(
            after_ranges[0].destination.end_row, 2,
            "When looking from after to before: The initial identical part has wrong destination end row"
        );
        assert_eq!(
            after_ranges[0].destination.end_column, 0,
            "When looking from after to before: The initial identical part has wrong destination end column"
        );

        assert_eq!(
            after_ranges[1].operation,
            TextOperation::Insert,
            "When looking from after to before: The insert has the wrong operation"
        );
        assert_eq!(
            after_ranges[1].source.start_row, 2,
            "When looking from after to before: The insert has wrong source start row"
        );
        assert_eq!(
            after_ranges[1].source.start_column, 2,
            "When looking from after to before: The insert has wrong source start column"
        );
        assert_eq!(
            after_ranges[1].source.end_row, 3,
            "When looking from after to before: The insert has wrong source end row"
        );
        assert_eq!(
            after_ranges[1].source.end_column, 0,
            "When looking from after to before: The insert has wrong source end column"
        );
        assert_eq!(
            after_ranges[1].destination.start_row, 2,
            "When looking from after to before: The insert has wrong destination start row"
        );
        assert_eq!(
            after_ranges[1].destination.start_column, 0,
            "When looking from after to before: The insert has wrong destination start column"
        );
        assert_eq!(
            after_ranges[1].destination.end_row, 2,
            "When looking from after to before: The insert has wrong destination end row"
        );
        assert_eq!(
            after_ranges[1].destination.end_column, 0,
            "When looking from after to before: The insert has wrong destination end column"
        );

        assert_eq!(
            after_ranges[2].operation,
            TextOperation::Identical,
            "When looking from after to before: The final identical part has wrong operation"
        );
        assert_eq!(
            after_ranges[2].source.start_row, 3,
            "When looking from after to before: The final identical part has wrong source start row"
        );
        assert_eq!(
            after_ranges[2].source.start_column, 0,
            "When looking from after to before: The final identical part has wrong source start column"
        );
        assert_eq!(
            after_ranges[2].source.end_row, 4,
            "When looking from after to before: The final identical part has wrong source end row"
        );
        assert_eq!(
            after_ranges[2].source.end_column, 0,
            "When looking from after to before: The final identical part has wrong source end column"
        );
        assert_eq!(
            after_ranges[2].destination.start_row, 2,
            "When looking from after to before: The final identical part has wrong destination start row"
        );
        assert_eq!(
            after_ranges[2].destination.start_column, 0,
            "When looking from after to before: The final identical part has wrong destination start column"
        );
        assert_eq!(
            after_ranges[2].destination.end_row, 3,
            "When looking from after to before: The final identical part has wrong destination end row"
        );
        assert_eq!(
            after_ranges[2].destination.end_column, 0,
            "When looking from after to before: The final identical part has wrong destination end column"
        );

        Ok(())
    }

    #[test]
    fn python_leetcode_1_added_if_block_all_ranges() -> Result<()> {
        let code_pairs = test::helper::handmade_test_code_pairs()?;
        let (before, after) = code_pairs.get("python-added-if-block").unwrap().clone();
        let node_cache = NodeCache::build(&before, &after);
        let diff = crate::diff::Diff::from_code(&before, &after);

        let text_diff = TextDiff::from(&before, &after, &diff.ast.unwrap(), &node_cache);

        let before_ranges = text_diff.all(0);
        assert_eq!(before_ranges.len(), 3);

        assert_eq!(before_ranges[0].operation, TextOperation::Identical);
        assert_eq!(before_ranges[0].source.start_row, 0);
        assert_eq!(before_ranges[0].source.start_column, 0);
        assert_eq!(before_ranges[0].source.end_row, 20);
        assert_eq!(before_ranges[0].source.end_column, 0);
        assert_eq!(before_ranges[0].destination.start_row, 0);
        assert_eq!(before_ranges[0].destination.start_column, 0);
        assert_eq!(before_ranges[0].destination.end_row, 20);
        assert_eq!(before_ranges[0].destination.end_column, 0);

        // This is a "empty range" that indicates something exists here in the other side.
        // Note that because we ignore whitespace, the leading 4-space indentation of the new
        // "if" line is simply missing from the result, and the destination starts at column 4.
        assert_eq!(before_ranges[1].operation, TextOperation::Delete);
        assert_eq!(before_ranges[1].source.start_row, 20);
        assert_eq!(before_ranges[1].source.start_column, 0);
        assert_eq!(before_ranges[1].source.end_row, 20);
        assert_eq!(before_ranges[1].source.end_column, 0);
        assert_eq!(before_ranges[1].destination.start_row, 20);
        assert_eq!(before_ranges[1].destination.start_column, 4);
        assert_eq!(before_ranges[1].destination.end_row, 21);
        assert_eq!(before_ranges[1].destination.end_column, 0);

        // Note the order between the empty range and the actual range that exists. The empty range
        // must always be before an actual existing range, even if their start point is equal.
        // This is the print statement that was re-indented (column 4 -> column 8) because it now
        // lives one level deeper inside the new "if" block. Its text is identical, but its
        // position moved, so it's a Move rather than an Identical range.
        assert_eq!(before_ranges[2].operation, TextOperation::Move);
        assert_eq!(before_ranges[2].source.start_row, 20);
        assert_eq!(before_ranges[2].source.start_column, 4);
        assert_eq!(before_ranges[2].source.end_row, 21);
        assert_eq!(before_ranges[2].source.end_column, 0);
        assert_eq!(before_ranges[2].destination.start_row, 21);
        assert_eq!(before_ranges[2].destination.start_column, 8);
        assert_eq!(before_ranges[2].destination.end_row, 22);
        assert_eq!(before_ranges[2].destination.end_column, 0);

        let after_ranges = text_diff.all(1);
        // Note the symetric relationships between source and destination ranges in the
        // before_ranges and after_ranges vectors.
        assert_eq!(after_ranges.len(), before_ranges.len());

        assert_eq!(after_ranges[0].operation, TextOperation::Identical);
        assert_eq!(after_ranges[0].source.start_row, 0);
        assert_eq!(after_ranges[0].source.start_column, 0);
        assert_eq!(after_ranges[0].source.end_row, 20);
        assert_eq!(after_ranges[0].source.end_column, 0);
        assert_eq!(after_ranges[0].destination.start_row, 0);
        assert_eq!(after_ranges[0].destination.start_column, 0);
        assert_eq!(after_ranges[0].destination.end_row, 20);
        assert_eq!(after_ranges[0].destination.end_column, 0);

        // The added "if" conditional (leading 4-space indentation ignored, same as above).
        assert_eq!(after_ranges[1].operation, TextOperation::Insert);
        assert_eq!(after_ranges[1].source.start_row, 20);
        assert_eq!(after_ranges[1].source.start_column, 4);
        assert_eq!(after_ranges[1].source.end_row, 21);
        assert_eq!(after_ranges[1].source.end_column, 0);
        assert_eq!(after_ranges[1].destination.start_row, 20);
        assert_eq!(after_ranges[1].destination.start_column, 0);
        assert_eq!(after_ranges[1].destination.end_row, 20);
        assert_eq!(after_ranges[1].destination.end_column, 0);

        // The matched existing implementation, moved one level deeper.
        assert_eq!(after_ranges[2].operation, TextOperation::Move);
        assert_eq!(after_ranges[2].source.start_row, 21);
        assert_eq!(after_ranges[2].source.start_column, 8);
        assert_eq!(after_ranges[2].source.end_row, 22);
        assert_eq!(after_ranges[2].source.end_column, 0);
        assert_eq!(after_ranges[2].destination.start_row, 20);
        assert_eq!(after_ranges[2].destination.start_column, 4);
        assert_eq!(after_ranges[2].destination.end_row, 21);
        assert_eq!(after_ranges[2].destination.end_column, 0);

        Ok(())
    }
}
