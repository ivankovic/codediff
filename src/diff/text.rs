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
use tree_sitter::Node;

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
                            new_range = Some(advance_and_build_range(
                                &mut last_non_move_range,
                                node,
                                &source_columns,
                                TextOperation::Delete,
                            ));
                            descend = false;
                        }
                        ASTMappingOperation::InsertWithChildren => {
                            new_range = Some(advance_and_build_range(
                                &mut last_non_move_range,
                                node,
                                &source_columns,
                                TextOperation::Insert,
                            ));
                            descend = false;
                        }
                        ASTMappingOperation::Delete if node.child_count() == 0 => {
                            new_range = Some(advance_and_build_range(
                                &mut last_non_move_range,
                                node,
                                &source_columns,
                                TextOperation::Delete,
                            ));
                        }
                        ASTMappingOperation::Insert if node.child_count() == 0 => {
                            new_range = Some(advance_and_build_range(
                                &mut last_non_move_range,
                                node,
                                &source_columns,
                                TextOperation::Insert,
                            ));
                        }
                        ASTMappingOperation::Update => {
                            new_range = Some(advance_and_build_range(
                                &mut last_non_move_range,
                                node,
                                &source_columns,
                                TextOperation::Update,
                            ));
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

/// Build the `RangeMatch` for a non-Identical, non-Move node: advances `last_non_move_range` to
/// its own right limit (we're appending after whatever was last placed) and anchors the new
/// range's destination there, since the node has no real destination-side counterpart to point at.
fn advance_and_build_range(
    last_non_move_range: &mut TextRange,
    node: Node,
    columns: &[usize],
    operation: TextOperation,
) -> RangeMatch {
    *last_non_move_range = last_non_move_range.right_limit();
    RangeMatch {
        source: TextRange::from_treesitter_range(node.range(), columns),
        destination: last_non_move_range.clone(),
        operation,
    }
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
    pub fn for_range(&self, range: &TextRange, side: usize) -> Vec<RangeMatch> {
        let all_ranges = if side == 0 {
            &self.before_ranges
        } else {
            &self.after_ranges
        };
        all_ranges
            .iter()
            .filter(|rm| rm.source.intersects(range))
            .cloned()
            .collect()
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

/// Assigns one `TextOperation` to each of `line_count` lines, from one side's `RangeMatch` list
/// (`TextDiff::all`).
///
/// Deliberately row-granular, not column-precise: a range's column bounds are only used to decide
/// whether it's a zero-width placeholder, never to split a single line between two operations.
/// `diff::text` ranges are whitespace-insensitive and can leave small gaps (e.g. leading
/// indentation - see `python_leetcode_1_added_if_block_all_ranges` below), so lining up exact
/// sub-line spans for a plain-text consumer would be fragile; whole-line coloring instead picks,
/// for each row, the *most specific* operation among all the ranges that touch it (see the
/// precedence comment below). Used by both `tui::headless` (its plain-text fallback renderer -
/// the TUI itself stays column-precise) and `benchmark_other` (reducing any `ASTDiff` - codediff's
/// own or a synthetic one built from a human mapping - to a per-line signal comparable against an
/// external line-based tool like Unix `diff`).
pub fn line_operations(ranges: &[RangeMatch], line_count: usize) -> Vec<TextOperation> {
    let mut ops = vec![TextOperation::Identical; line_count];
    for rm in ranges {
        let r = &rm.source;
        if r.is_empty() {
            // Zero-width placeholder: nothing on this side for this diff unit (see
            // `TextRange`'s doc comment on symmetric insert/delete placeholders).
            continue;
        }
        // `TextRange`'s convention: an end column of 0 already means "up to, not including, this
        // row", so only a genuinely mid-row end column needs the extra +1.
        let end_row = if r.end_column == 0 {
            r.end_row
        } else {
            r.end_row + 1
        };
        for row_op in ops
            .iter_mut()
            .take(end_row.min(line_count))
            .skip(r.start_row)
        {
            // A row can legitimately be touched by more than one range (e.g. a changed token
            // shares its row with the identical whitespace/punctuation around it). Whichever
            // range for that row is *not* Identical wins, regardless of iteration order -
            // otherwise an Identical range for the same row ordered after the real change would
            // silently overwrite it back to plain, hiding the change entirely. Two non-Identical
            // ranges touching the same row is not expected to happen in practice (ranges are
            // built from a non-overlapping tree traversal, see `diff/text.rs`), so last-wins
            // between two of those is an arbitrary but harmless tiebreak.
            if rm.operation != TextOperation::Identical || *row_op == TextOperation::Identical {
                *row_op = rm.operation.clone();
            }
        }
    }
    ops
}

/// A quick, common-case classification of a diff's overall shape, cheap enough to compute on every
/// completed diff (see `summarize_diff`) from data `TextDiff` already produces - no extra
/// tree-sitter/AST work needed. Deliberately presentation-agnostic (a label, not a color or an
/// icon): callers like `tui::app` map each variant to their own styling, the same separation
/// `tui::headless`'s `ansi_color`/`marker` already draw around `TextOperation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffSummary {
    /// The two sides are byte-for-byte identical. Deliberately *not* "no operations were
    /// produced" - a pure reformat can also produce zero operations (see `summarize_diff`'s own
    /// doc comment on why), but that case is `WhitespaceOnly`, not this.
    NoChanges,
    /// Every changed range is an `Insert` - nothing on the before side was touched or survived.
    NewFile,
    /// Every changed range is a `Delete` - nothing on the after side existed before.
    DeletedFile,
    /// The before/after content is identical once every whitespace character is stripped out,
    /// though the two sides are not byte-identical (that's `NoChanges`) - regardless of what
    /// operations, if any, actually resulted. Checked before any operation-based case: a pure
    /// reformat can produce many `Move` ranges (a node's column shifting is by itself enough to
    /// reclassify an otherwise-`Identical` range as `Move` - see `ranges`'s own Identical/Move
    /// branch), *or* zero ranges at all if the shifted node's own position happens to be
    /// unaffected (see `summarize_diff`). Checking operations alone can't tell either of those
    /// apart from a real reordering, but comparing whitespace-stripped content can, since real
    /// reordering changes token order and a pure reformat never does.
    WhitespaceOnly,
    /// Every changed range is a `Move` (on top of whatever's `Identical`) - code relocated without
    /// a single `Insert`, `Delete`, or `Update` anywhere. Checked after `WhitespaceOnly`, so a pure
    /// reformat (which can also produce only `Move` ranges) is reported as that instead, since
    /// "reformatted" is the more specific and more useful claim of the two.
    ///
    /// Narrower than "moved code" might suggest: `TextOperation::Move` only fires when a matched
    /// node's own *column* shifts (a re-indent), not generally whenever a node's position in the
    /// file changes - reordering two top-level items (same column, different row) produces no
    /// operations at all today, not `Move` (confirmed empirically: `codediff --headless` on two
    /// files differing only by a swapped pair of top-level functions shows no diff whatsoever).
    /// That case currently falls through to `None` if it also fails `WhitespaceOnly` (it usually
    /// will, since token order really did change) - a real gap, not a bug: catching genuine
    /// cross-position reordering needs the AST-level `ASTMappingOperation::Move`/
    /// `ASTMappingReason::MovedSubtree` (`solve_moved_subtrees`), not `TextOperation`, which
    /// `summarize_diff` doesn't have access to (it only sees `TextDiff`'s already-flattened
    /// ranges). Worth revisiting if this proves too narrow in practice.
    RefactorMovedOnly,
}

impl DiffSummary {
    /// A short, human-readable label - presentation-agnostic (see this type's own doc comment).
    pub fn label(self) -> &'static str {
        match self {
            DiffSummary::NoChanges => "No changes - files are identical",
            DiffSummary::NewFile => "New file - everything inserted",
            DiffSummary::DeletedFile => "Deleted file - everything removed",
            DiffSummary::WhitespaceOnly => "Whitespace changes only",
            DiffSummary::RefactorMovedOnly => "Refactor - code moved, no content changes",
        }
    }
}

/// Whether `a` and `b` contain the same characters once every whitespace character is removed from
/// each - the check behind `DiffSummary::WhitespaceOnly`. Compares via iterators rather than
/// building two new `String`s, since this runs on full file contents on every completed diff.
fn whitespace_stripped_equal(a: &str, b: &str) -> bool {
    a.chars()
        .filter(|c| !c.is_whitespace())
        .eq(b.chars().filter(|c| !c.is_whitespace()))
}

/// Classifies a diff's overall shape into one of `DiffSummary`'s common cases, or `None` if it
/// doesn't cleanly fit any of them (the ordinary case - most diffs are a genuine mix of edits).
/// `before_ranges`/`after_ranges` are `TextDiff::all(0)`/`TextDiff::all(1)` (or the equivalent -
/// `DiffSessionData`'s own fields, in `tui::app`), and `before_contents`/`after_contents` the full
/// raw source each side was parsed from.
///
/// Checked in order from most to least specific, returning the first match - see each
/// `DiffSummary` variant's own doc comment for why that particular order matters
/// (`WhitespaceOnly` before `RefactorMovedOnly` in particular).
pub fn summarize_diff(
    before_contents: &str,
    after_contents: &str,
    before_ranges: &[RangeMatch],
    after_ranges: &[RangeMatch],
) -> Option<DiffSummary> {
    // Checked before anything operation-based, not after: a pure reformat can legitimately
    // produce *zero* `TextOperation`s at all, not just `Move`s. Hash-based matching pairs the
    // largest identical subtree it can (ignoring position), and `ranges` only checks a matched
    // node's own start column against its match - once matched, it never descends into that
    // node's children (`descend = false` in the `Identical` branch above). So a whole-file
    // reformat that happens to match as one big subtree, whose own start position is unchanged
    // (e.g. the file root, or a top-level item still at column 0), produces a single `Identical`
    // range covering everything, with no `Move` anywhere - confirmed empirically against the real
    // pipeline, not just reasoned about. Checking operation presence first would misreport that
    // case as `NoChanges`, which is wrong: the files are not byte-identical, only AST-identical
    // modulo whitespace. Only a literal content match earns `NoChanges`; everything else that's
    // whitespace-stripped-equal is `WhitespaceOnly`, regardless of what operations (if any) resulted.
    if before_contents == after_contents {
        return Some(DiffSummary::NoChanges);
    }
    if whitespace_stripped_equal(before_contents, after_contents) {
        return Some(DiffSummary::WhitespaceOnly);
    }

    let mut has_insert = false;
    let mut has_delete = false;
    let mut has_update = false;
    let mut has_move = false;
    for range in before_ranges.iter().chain(after_ranges.iter()) {
        match range.operation {
            TextOperation::Insert => has_insert = true,
            TextOperation::Delete => has_delete = true,
            TextOperation::Update => has_update = true,
            TextOperation::Move => has_move = true,
            TextOperation::Identical | TextOperation::NotYetSet => {}
        }
    }

    if has_insert && !has_delete && !has_update && !has_move {
        return Some(DiffSummary::NewFile);
    }
    if has_delete && !has_insert && !has_update && !has_move {
        return Some(DiffSummary::DeletedFile);
    }
    if has_move && !has_insert && !has_delete && !has_update {
        return Some(DiffSummary::RefactorMovedOnly);
    }

    None
}

#[cfg(test)]
mod tests {
    use crate::test;
    use anyhow::Result;

    use super::*;

    /// Regression guard: a real one-token change (e.g. renaming a call inside an otherwise
    /// unchanged statement) can leave the same row covered by both an Update range (the token)
    /// and an Identical range (the rest of the line/its surrounding punctuation). If the Identical
    /// range for that row happens to come *after* the Update range in `ranges`' order, a naive
    /// last-write-wins would silently overwrite the row back to `Identical`, hiding the change -
    /// this is exactly what a real end-to-end smoke test against the built binary caught.
    #[test]
    fn line_operations_does_not_let_a_same_row_identical_range_hide_a_real_change() {
        let ranges = vec![
            RangeMatch {
                source: TextRange::new(0, 4, 0, 12),
                destination: TextRange::new(0, 4, 0, 12),
                operation: TextOperation::Update,
            },
            // Ordered *after* the Update above on purpose - this is the ordering that triggered
            // the bug.
            RangeMatch {
                source: TextRange::new(0, 12, 1, 0),
                destination: TextRange::new(0, 12, 1, 0),
                operation: TextOperation::Identical,
            },
        ];
        assert_eq!(line_operations(&ranges, 1), vec![TextOperation::Update]);
    }

    #[test]
    fn line_operations_treats_a_zero_width_range_as_a_placeholder_not_a_real_row() {
        let ranges = vec![RangeMatch {
            source: TextRange::new(1, 0, 1, 0),
            destination: TextRange::new(1, 0, 2, 0),
            operation: TextOperation::Delete,
        }];
        let ops = line_operations(&ranges, 3);
        assert_eq!(ops, vec![TextOperation::Identical; 3]);
    }

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

    fn range(operation: TextOperation) -> RangeMatch {
        RangeMatch {
            source: TextRange::new(0, 0, 1, 0),
            destination: TextRange::new(0, 0, 1, 0),
            operation,
        }
    }

    #[test]
    fn whitespace_stripped_equal_ignores_all_whitespace_differences() {
        assert!(whitespace_stripped_equal(
            "fn main() {\n    foo();\n}\n",
            "fn main(){foo();}"
        ));
        assert!(!whitespace_stripped_equal("fn main() {}", "fn other() {}"));
    }

    #[test]
    fn summarize_diff_is_no_changes_when_every_range_is_identical() {
        let ranges = vec![range(TextOperation::Identical)];
        assert_eq!(
            summarize_diff("same", "same", &ranges, &ranges),
            Some(DiffSummary::NoChanges)
        );
    }

    #[test]
    fn summarize_diff_is_no_changes_for_two_empty_files() {
        assert_eq!(
            summarize_diff("", "", &[], &[]),
            Some(DiffSummary::NoChanges)
        );
    }

    #[test]
    fn summarize_diff_is_new_file_when_only_inserts_are_present() {
        let before_ranges: Vec<RangeMatch> = vec![];
        let after_ranges = vec![range(TextOperation::Insert)];
        assert_eq!(
            summarize_diff("", "fn main() {}", &before_ranges, &after_ranges),
            Some(DiffSummary::NewFile)
        );
    }

    #[test]
    fn summarize_diff_is_deleted_file_when_only_deletes_are_present() {
        let before_ranges = vec![range(TextOperation::Delete)];
        let after_ranges: Vec<RangeMatch> = vec![];
        assert_eq!(
            summarize_diff("fn main() {}", "", &before_ranges, &after_ranges),
            Some(DiffSummary::DeletedFile)
        );
    }

    #[test]
    fn summarize_diff_is_whitespace_only_when_stripped_content_matches_despite_move_ranges() {
        // A pure re-indent: codediff sees the reindented block as Moved (column shifted), even
        // though nothing about the code itself changed - see DiffSummary::WhitespaceOnly's own
        // doc comment for why the operation set alone can't distinguish this from a real move.
        let ranges = vec![range(TextOperation::Move)];
        assert_eq!(
            summarize_diff(
                "fn main() {\nfoo();\n}",
                "fn main() {\n    foo();\n}",
                &ranges,
                &ranges
            ),
            Some(DiffSummary::WhitespaceOnly)
        );
    }

    #[test]
    fn summarize_diff_is_refactor_moved_only_when_only_moves_are_present_and_content_really_differs()
     {
        let ranges = vec![range(TextOperation::Move)];
        assert_eq!(
            summarize_diff(
                "fn a() {}\nfn b() {}",
                "fn b() {}\nfn a() {}",
                &ranges,
                &ranges
            ),
            Some(DiffSummary::RefactorMovedOnly)
        );
    }

    #[test]
    fn summarize_diff_is_none_for_a_genuine_mixed_edit() {
        let ranges = vec![range(TextOperation::Update), range(TextOperation::Insert)];
        assert_eq!(summarize_diff("a", "b", &ranges, &ranges), None);
    }

    #[test]
    fn summarize_diff_prefers_whitespace_only_over_refactor_when_both_could_apply() {
        // Both conditions are structurally satisfiable at once (only Move ranges present, and the
        // content is whitespace-stripped-equal but *not* byte-identical - "same" vs " same " here,
        // not "same" vs "same", which would instead hit NoChanges); the whitespace-stripped content
        // check must win, since "reformatted" is the more specific and more useful claim - see
        // DiffSummary::RefactorMovedOnly's own doc comment on the order.
        let ranges = vec![range(TextOperation::Move)];
        assert_eq!(
            summarize_diff("same", " same ", &ranges, &ranges),
            Some(DiffSummary::WhitespaceOnly)
        );
    }

    /// Regression guard for a real finding, not a hypothetical: a whole-file reformat can produce
    /// *zero* `TextOperation`s at all (not even `Move`), when the single matched subtree covering
    /// the reformatted content happens to have an unchanged start position itself - confirmed
    /// against the real pipeline (`codediff --headless` on a file reindented inside an unchanged
    /// top-level item showed no diff whatsoever, not a `Move`-marked one). Checking "no operations"
    /// before "content differs only in whitespace" would have misreported this as `NoChanges`
    /// (implying the files are identical, which they are not) instead of `WhitespaceOnly`.
    #[test]
    fn summarize_diff_is_whitespace_only_even_with_zero_operations_when_content_is_not_byte_identical()
     {
        let no_ranges: Vec<RangeMatch> = vec![];
        assert_eq!(
            summarize_diff(
                "fn main() {\nfoo();\n}",
                "fn main() {\n    foo();\n}",
                &no_ranges,
                &no_ranges,
            ),
            Some(DiffSummary::WhitespaceOnly)
        );
    }
}
