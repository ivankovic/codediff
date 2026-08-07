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
    diff::{ASTDiff, ASTMappingOperation, NodeCache, nodes, text_range::TextRange},
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

/// The text of `node`'s own span that isn't covered by any of its *direct* children - e.g. for a
/// `line_comment` node with one `//` child, this is everything after the `//` (the comment's
/// actual words), since nothing else claims those bytes. For a typical container node (a `block`,
/// a `module`, ...), this is normally just the whitespace/punctuation between children - real
/// content almost always lives inside a named child, not in the gaps around them.
///
/// This is what makes it possible to tell "this node's own un-decomposed content changed" (a
/// comment) apart from "this node is a container and something changed somewhere inside it" (a
/// `block` that gained/lost/rearranged a child) without hardcoding per-language node-kind rules:
/// a container's own gap text rarely differs at all beyond whitespace, while a comment's does.
pub(crate) fn own_content(node: Node, source: &[u8]) -> String {
    let mut gap_bytes: Vec<u8> = Vec::new();
    let mut pos = node.start_byte();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.start_byte() > pos {
            gap_bytes.extend_from_slice(&source[pos..child.start_byte()]);
        }
        pos = pos.max(child.end_byte());
    }
    if node.end_byte() > pos {
        gap_bytes.extend_from_slice(&source[pos..node.end_byte()]);
    }
    String::from_utf8_lossy(&gap_bytes).into_owned()
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
                        // A node (e.g. a comment) whose *own* un-decomposed content differs from
                        // its match - content not claimed by any of its children (`own_content`) -
                        // beyond whitespace. Without this arm, the difference was invisible in the
                        // rendered diff entirely: `MatchButNotIdentical` had no arm here at all
                        // before, so a changed comment (real case: tree-sitter-rust's
                        // `line_comment` has its own `//`-marker child, so the node carrying the
                        // comment's actual words is this node itself, not a leaf `is_comment`-kind
                        // node the `Update`/childless-`Delete`/childless-`Insert` arms above would
                        // already catch) produced literally no range at all, confirmed by running
                        // `codediff --headless` against a real file pair and seeing no diff
                        // whatsoever for a comment-only text change.
                        //
                        // Deliberately `own_content`, not this node's *whole* text: a container
                        // (e.g. a function `block` that gained a statement) also has
                        // `MatchButNotIdentical` mappings, and its whole text almost always
                        // differs too - but comparing only the gap text between its children
                        // correctly finds nothing (containers rarely have real content outside
                        // their named children), so this arm doesn't fire for them and the
                        // existing descent finds the real, much smaller change instead. Confirmed
                        // as a real, not hypothetical, false positive during development: an
                        // earlier version of this fix compared each child's own `mapping.operation`
                        // instead, which missed a statement moving one level deeper (still mapped
                        // `Identical` at the AST level - only its rendered `TextOperation` becomes
                        // `Move`, from the column shift, see the `Identical` arm above) and wrongly
                        // treated the whole enclosing block as one giant `Update`.
                        ASTMappingOperation::MatchButNotIdentical => {
                            if let Some(&destination_node) = node_cache.get_in_any(&mapped_id) {
                                let source_content = own_content(node, source.contents.as_bytes());
                                let destination_content =
                                    own_content(destination_node, destination.contents.as_bytes());
                                if !whitespace_stripped_equal(&source_content, &destination_content)
                                {
                                    new_range = Some(advance_and_build_range(
                                        &mut last_non_move_range,
                                        node,
                                        &source_columns,
                                        TextOperation::Update,
                                    ));
                                    descend = false;
                                }
                            }
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
}

/// `myers_lcs`'s edit-distance search gives up past this and falls back to treating the whole
/// file as replaced. `myers_lcs` allocates O(max_edit²) `usize`s and does O(max_edit²) work in
/// the worst case (two sides with no common lines at all) - at 10,000 that's ~1.6GB and a
/// genuinely slow search, so this only pays that cost for a file pair that's actually that
/// different; an ordinary edit to a large file (even a 10k-line one) finds its solution and
/// returns long before reaching the cap, since Myers' search terminates as soon as it finds the
/// *actual* edit distance, however small, rather than always running to `max_edit`. Deliberately
/// much higher than `apted::common::FALLBACK_MAX_EDIT` (1000): that one bounds a residual
/// *subtree* forest, typically small even for a large file, while this one bounds whole-file
/// *lines*, where 1000 was too easy to exceed on real config/lockfile-sized files with a
/// legitimately large (but not pathological) number of changed lines.
const PLAIN_TEXT_MAX_EDIT: usize = 10_000;

/// A plain line-level diff (Myers LCS over hashed lines, no AST) for files with no tree-sitter
/// grammar - `app::compute_diff`'s fallback when either side's `Code::ast` is `None` (an
/// unrecognized extension, e.g. a `Makefile`). Returns `(before_ranges, after_ranges)`, the same
/// shape `TextDiff::all(0)`/`all(1)` produce, so every downstream consumer (the TUI's overlay
/// rendering, `headless::render_text_diff`, `json_output::build_side`, `change_counts`,
/// `DiffSummary`) works unchanged - none of them actually require an AST, only a `RangeMatch`
/// list.
///
/// Only ever produces `Identical`/`Insert`/`Delete` - there is no AST-level node identity to
/// detect an `Update` (a changed line one side, at the same position) or a `Move` (a line
/// relocated elsewhere) from, so a changed line renders as an adjacent delete+insert pair instead
/// of a single `Update`, same as a plain `diff -u`.
pub fn plain_text_line_diff(before: &str, after: &str) -> (Vec<RangeMatch>, Vec<RangeMatch>) {
    plain_text_line_diff_with_max_edit(before, after, PLAIN_TEXT_MAX_EDIT)
}

/// `plain_text_line_diff`'s actual implementation, parameterized on the edit-distance cap so
/// tests can exercise the "gave up" path with a small cap instead of paying `PLAIN_TEXT_MAX_EDIT`
/// squared (10,000² ≈ 1.6GB and genuinely slow) just to prove that path exists.
fn plain_text_line_diff_with_max_edit(
    before: &str,
    after: &str,
    max_edit: usize,
) -> (Vec<RangeMatch>, Vec<RangeMatch>) {
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();

    let before_hashes = hash_lines(&before_lines);
    let after_hashes = hash_lines(&after_lines);

    match crate::diff::apted::myers_lcs(&before_hashes, &after_hashes, max_edit) {
        Some(pairs) => build_line_ranges(before_lines.len(), after_lines.len(), &pairs),
        None => whole_file_replaced(before_lines.len(), after_lines.len()),
    }
}

fn hash_lines(lines: &[&str]) -> Vec<u64> {
    use std::hash::{Hash, Hasher};
    lines
        .iter()
        .map(|line| {
            let mut hasher = rustc_hash::FxHasher::default();
            line.hash(&mut hasher);
            hasher.finish()
        })
        .collect()
}

/// One whole line as a `TextRange`: `(row, 0)` to `(row + 1, 0)` - this module's convention for
/// "all of row, including its own line break" (see `text_range.rs`'s doc comment on referring to
/// a full row via `(row + 1, 0)`).
fn whole_line_range(row: usize) -> TextRange {
    TextRange::new(row, 0, row + 1, 0)
}

/// Walks `pairs` (matched `(before_row, after_row)`, ascending in both - an LCS matching
/// preserves relative order on both sides) once, emitting one `Identical` `RangeMatch` per match
/// and one merged `Delete`/`Insert` `RangeMatch` per *gap* between matches (so a multi-line
/// block reads, and n/p-navigates, as a single change rather than one per line).
///
/// Every unmatched run's `destination` is anchored at the other side's most recently confirmed
/// match, advanced past it via `right_limit` - the exact convention `diff::text::
/// advance_and_build_range` uses for the AST path's own plain Insert/Delete ranges, so the
/// cross-panel cursor lands at "where this content would be if it existed on the other side"
/// instead of at a coordinate-space-confused row (before-side and after-side rows generally
/// diverge once there's been any earlier insert/delete).
fn build_line_ranges(
    before_line_count: usize,
    after_line_count: usize,
    pairs: &[(usize, usize)],
) -> (Vec<RangeMatch>, Vec<RangeMatch>) {
    let mut before_ranges = Vec::new();
    let mut after_ranges = Vec::new();

    let mut next_before_row = 0;
    let mut next_after_row = 0;
    let mut last_before_match = TextRange::zero();
    let mut last_after_match = TextRange::zero();

    for &(bi, ai) in pairs {
        if bi > next_before_row {
            before_ranges.push(RangeMatch {
                source: TextRange::new(next_before_row, 0, bi, 0),
                destination: last_after_match.right_limit(),
                operation: TextOperation::Delete,
            });
        }
        if ai > next_after_row {
            after_ranges.push(RangeMatch {
                source: TextRange::new(next_after_row, 0, ai, 0),
                destination: last_before_match.right_limit(),
                operation: TextOperation::Insert,
            });
        }

        let before_line = whole_line_range(bi);
        let after_line = whole_line_range(ai);
        before_ranges.push(RangeMatch {
            source: before_line.clone(),
            destination: after_line.clone(),
            operation: TextOperation::Identical,
        });
        after_ranges.push(RangeMatch {
            source: after_line.clone(),
            destination: before_line.clone(),
            operation: TextOperation::Identical,
        });

        last_before_match = before_line;
        last_after_match = after_line;
        next_before_row = bi + 1;
        next_after_row = ai + 1;
    }

    if before_line_count > next_before_row {
        before_ranges.push(RangeMatch {
            source: TextRange::new(next_before_row, 0, before_line_count, 0),
            destination: last_after_match.right_limit(),
            operation: TextOperation::Delete,
        });
    }
    if after_line_count > next_after_row {
        after_ranges.push(RangeMatch {
            source: TextRange::new(next_after_row, 0, after_line_count, 0),
            destination: last_before_match.right_limit(),
            operation: TextOperation::Insert,
        });
    }

    (before_ranges, after_ranges)
}

/// `myers_lcs` gave up (edit distance past `PLAIN_TEXT_MAX_EDIT`): treat the whole file as
/// replaced rather than paying for an unbounded search - same fallback-of-a-fallback
/// `apted::common::resolve_residual_forest_via_myers_lcs` already uses for the same reason. No
/// range at all for an empty side, matching `diff::text::ranges`'s own `(None, None)` "no code on
/// either side" case.
fn whole_file_replaced(
    before_line_count: usize,
    after_line_count: usize,
) -> (Vec<RangeMatch>, Vec<RangeMatch>) {
    let before_ranges = if before_line_count == 0 {
        Vec::new()
    } else {
        vec![RangeMatch {
            source: TextRange::new(0, 0, before_line_count, 0),
            destination: TextRange::zero(),
            operation: TextOperation::Delete,
        }]
    };
    let after_ranges = if after_line_count == 0 {
        Vec::new()
    } else {
        vec![RangeMatch {
            source: TextRange::new(0, 0, after_line_count, 0),
            destination: TextRange::zero(),
            operation: TextOperation::Insert,
        }]
    };
    (before_ranges, after_ranges)
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

/// Line-level +/-/~ counts for a completed diff - e.g. for a compact status-bar summary like
/// `+12 -4 ~2`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChangeCounts {
    pub insertions: usize,
    pub deletions: usize,
    pub updates: usize,
}

/// Counts each side's own [`line_operations`] output independently: `Insert` from the after side
/// (a line that exists only in after), `Delete` from the before side (a line that exists only in
/// before). `Update` is counted once, from the after side only - an updated line exists on both
/// sides at the same row, so counting it from both sides would double it.
pub fn change_counts(
    before_contents: &str,
    after_contents: &str,
    before_ranges: &[RangeMatch],
    after_ranges: &[RangeMatch],
) -> ChangeCounts {
    let before_ops = line_operations(before_ranges, before_contents.split('\n').count());
    let after_ops = line_operations(after_ranges, after_contents.split('\n').count());

    ChangeCounts {
        insertions: after_ops
            .iter()
            .filter(|op| **op == TextOperation::Insert)
            .count(),
        deletions: before_ops
            .iter()
            .filter(|op| **op == TextOperation::Delete)
            .count(),
        updates: after_ops
            .iter()
            .filter(|op| **op == TextOperation::Update)
            .count(),
    }
}

/// A quick, common-case classification of a diff's overall shape. Most variants are cheap enough
/// to compute on every completed diff (see `summarize_diff`) from data `TextDiff` already
/// produces, no extra tree-sitter/AST work needed - `CommentOnly` is the one exception, which
/// needs AST-level node-kind access (`is_comment_only_diff`) and so is only ever added on by
/// `summarize_diff_with_comment_check`, not `summarize_diff` itself. Deliberately
/// presentation-agnostic (a label, not a color or an icon): callers like `tui::app` map each
/// variant to their own styling, the same separation `tui::headless`'s `ansi_color`/`marker`
/// already draw around `TextOperation`.
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
    /// Every "real" change (an `Insert`, `Delete`, `DeleteWithChildren`, `InsertWithChildren`, or
    /// `Update` at the AST-mapping level) touches only comment nodes (`nodes::is_comment`) - see
    /// `is_comment_only_diff`. Checked after `NewFile`/`DeletedFile` (a wholly new file that
    /// happens to be all comments is still more usefully reported as `NewFile`), but before
    /// `RefactorMovedOnly`/no classification at all, since "only comments changed" is the more
    /// specific and more useful claim of those two.
    CommentOnly,
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
            DiffSummary::CommentOnly => "Comment changes only",
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
    let mut has_identical = false;
    for range in before_ranges.iter().chain(after_ranges.iter()) {
        match range.operation {
            TextOperation::Insert => has_insert = true,
            TextOperation::Delete => has_delete = true,
            TextOperation::Update => has_update = true,
            TextOperation::Move => has_move = true,
            TextOperation::Identical => has_identical = true,
            TextOperation::NotYetSet => {}
        }
    }

    // `!has_identical` matters here, not just "only Insert/Delete present": without it, adding one
    // line to an otherwise-untouched large file would also match "only Insert present" and get
    // mislabeled NewFile - confirmed as a real, not hypothetical, misclassification by running the
    // actual pipeline on exactly that case. A genuinely new/deleted file has nothing on the other
    // side to have matched anything against, so no Identical range can exist for it either.
    if has_insert && !has_delete && !has_update && !has_move && !has_identical {
        return Some(DiffSummary::NewFile);
    }
    if has_delete && !has_insert && !has_update && !has_move && !has_identical {
        return Some(DiffSummary::DeletedFile);
    }
    if has_move && !has_insert && !has_delete && !has_update {
        return Some(DiffSummary::RefactorMovedOnly);
    }

    None
}

/// Whether every "real" change between `before` and `after` (per `diff`) touches only comment
/// nodes (`nodes::is_comment`) - the check behind `DiffSummary::CommentOnly`. `false`, not an
/// error, if either side has no AST (nothing to walk).
///
/// "Real" means the same handful of AST-mapping operations `ranges` itself treats as producing a
/// visible change - `DeleteWithChildren`, `InsertWithChildren`, a childless `Delete`/`Insert`, an
/// `Update`, or a `MatchButNotIdentical` whose `own_content` differs - checked with the exact same
/// criteria `ranges` itself uses (mirrored deliberately, not reused: `ranges` also tracks positions
/// and merges the two sides' ranges, work this doesn't need for a yes/no answer). Everything else -
/// `Identical`, `Move`, a non-childless plain `Delete`/`Insert`, a `MatchButNotIdentical` whose
/// `own_content` doesn't differ - is bookkeeping for an ancestor/container of the real change, not
/// the change itself, so it's skipped by descending into it rather than required to be a comment.
/// Returns `false`, not `true`, if no qualifying operation exists at all (e.g. a diff that's only
/// `Move`s): "comment-only" is a claim about *what* changed, and is meaningless to assert about a
/// diff where nothing did.
///
/// Checks comment-ness via `is_comment_or_inside_comment`, not a bare `nodes::is_comment(node.
/// kind())`: at least one grammar (Rust's `line_comment`) represents a comment as a small node
/// tree of its own rather than one opaque token (confirmed empirically - its `//` marker is a
/// separate child node), so the specific node actually carrying the mapping can be a non-
/// comment-kind piece *of* a comment.
pub fn is_comment_only_diff(
    before: &Code,
    after: &Code,
    diff: &ASTDiff,
    node_cache: &NodeCache,
) -> bool {
    let (Some(before_ast), Some(after_ast)) = (before.ast.as_ref(), after.ast.as_ref()) else {
        return false;
    };

    // `node` itself, or an ancestor of it, is a comment. Not just `nodes::is_comment(node.kind())`
    // directly: at least one grammar (Rust's `line_comment`) represents a comment as a small node
    // tree of its own (the `//` marker is its own child, confirmed empirically), so the specific
    // node carrying an Insert/Delete/Update mapping can be a non-comment-kind piece *of* a
    // comment, not the comment node itself. Walking up finds the enclosing comment either way.
    fn is_comment_or_inside_comment(node: Node) -> bool {
        let mut current = Some(node);
        while let Some(n) = current {
            if nodes::is_comment(n.kind()) {
                return true;
            }
            current = n.parent();
        }
        false
    }

    // Returns (found_any_qualifying_operation, every_one_of_them_was_a_comment). `own_bytes` is
    // `root`'s own source, `other_bytes` its mapped counterpart's - i.e. (before, after) when
    // `root` is the before-tree root, (after, before) when it's the after-tree root.
    fn scan(
        root: Node,
        diff: &ASTDiff,
        node_cache: &NodeCache,
        own_bytes: &[u8],
        other_bytes: &[u8],
    ) -> (bool, bool) {
        let mut found_any = false;
        let mut all_comments = true;
        let mut stack = vec![root];

        while let Some(node) = stack.pop() {
            let mut descend = true;
            let mut mark_found = || {
                found_any = true;
                if !is_comment_or_inside_comment(node) {
                    all_comments = false;
                }
            };

            if let Some((mapped_id, mapping)) = diff.mapping_for_node(&node.id()) {
                match mapping.operation {
                    ASTMappingOperation::Identical => descend = false,
                    ASTMappingOperation::DeleteWithChildren
                    | ASTMappingOperation::InsertWithChildren => {
                        descend = false;
                        mark_found();
                    }
                    ASTMappingOperation::Delete | ASTMappingOperation::Insert
                        if node.child_count() == 0 =>
                    {
                        mark_found();
                    }
                    ASTMappingOperation::Update => mark_found(),
                    // Same criterion as `ranges`'s own `MatchButNotIdentical` arm - see that
                    // arm's doc comment for why `own_content`, not the node's whole text.
                    ASTMappingOperation::MatchButNotIdentical => {
                        if let Some(&other_node) = node_cache.get_in_any(&mapped_id)
                            && !whitespace_stripped_equal(
                                &own_content(node, own_bytes),
                                &own_content(other_node, other_bytes),
                            )
                        {
                            descend = false;
                            mark_found();
                        }
                    }
                    _ => {}
                }
            }

            if descend {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    stack.push(child);
                }
            }
        }

        (found_any, all_comments)
    }

    let before_bytes = before.contents.as_bytes();
    let after_bytes = after.contents.as_bytes();
    let (before_found, before_all_comments) = scan(
        before_ast.root_node(),
        diff,
        node_cache,
        before_bytes,
        after_bytes,
    );
    let (after_found, after_all_comments) = scan(
        after_ast.root_node(),
        diff,
        node_cache,
        after_bytes,
        before_bytes,
    );

    // At least one side must have actually found a qualifying operation - "comment-only" is
    // meaningless to assert about a diff where nothing changed at all (e.g. a diff that's only
    // Moves) - and whichever side(s) did find one must all agree it was comment-only.
    (before_found || after_found)
        && (!before_found || before_all_comments)
        && (!after_found || after_all_comments)
}

/// Same as `summarize_diff`, but folds in `DiffSummary::CommentOnly` too - checked with lower
/// precedence than every other case (see that variant's own doc comment for the exact order).
/// A separate function, not an extra parameter on `summarize_diff` itself: `is_comment_only` needs
/// AST-level node-kind access (`is_comment_only_diff`, over `ASTDiff`+`Code`), which
/// `summarize_diff`'s own inputs (`TextDiff`'s already-flattened `RangeMatch`es) don't carry -
/// callers without that access, or that don't need it, can keep using `summarize_diff` directly.
pub fn summarize_diff_with_comment_check(
    before_contents: &str,
    after_contents: &str,
    before_ranges: &[RangeMatch],
    after_ranges: &[RangeMatch],
    is_comment_only: bool,
) -> Option<DiffSummary> {
    let summary = summarize_diff(before_contents, after_contents, before_ranges, after_ranges);
    if is_comment_only && matches!(summary, None | Some(DiffSummary::RefactorMovedOnly)) {
        return Some(DiffSummary::CommentOnly);
    }
    summary
}

#[cfg(test)]
mod tests {
    use crate::test;
    use anyhow::Result;

    use super::*;

    /// Every non-Identical range's source row span, in order - the shape of assertion these tests
    /// care about (what changed, and in what order), without pinning down destination anchors
    /// line by line.
    fn changed_row_spans(ranges: &[RangeMatch]) -> Vec<(TextOperation, usize, usize)> {
        ranges
            .iter()
            .filter(|r| r.operation != TextOperation::Identical)
            .map(|r| (r.operation.clone(), r.source.start_row, r.source.end_row))
            .collect()
    }

    #[test]
    fn plain_text_line_diff_matches_identical_lines() {
        let (before, after) = plain_text_line_diff("a\nb\nc\n", "a\nb\nc\n");
        assert!(changed_row_spans(&before).is_empty());
        assert!(changed_row_spans(&after).is_empty());
        assert_eq!(before.len(), 3, "every line should get an Identical range");
        assert!(
            before
                .iter()
                .all(|r| r.operation == TextOperation::Identical)
        );
    }

    #[test]
    fn plain_text_line_diff_finds_a_pure_insertion() {
        let (before, after) = plain_text_line_diff("a\nc\n", "a\nb\nc\n");
        assert_eq!(changed_row_spans(&before), vec![]);
        assert_eq!(
            changed_row_spans(&after),
            vec![(TextOperation::Insert, 1, 2)]
        );
    }

    #[test]
    fn plain_text_line_diff_finds_a_pure_deletion() {
        let (before, after) = plain_text_line_diff("a\nb\nc\n", "a\nc\n");
        assert_eq!(
            changed_row_spans(&before),
            vec![(TextOperation::Delete, 1, 2)]
        );
        assert_eq!(changed_row_spans(&after), vec![]);
    }

    /// A changed line has no AST-level identity to recognize as an `Update` - it renders as an
    /// adjacent delete+insert pair instead, same as a plain `diff -u`.
    #[test]
    fn plain_text_line_diff_treats_a_changed_line_as_delete_plus_insert() {
        let (before, after) = plain_text_line_diff("a\nOLD\nc\n", "a\nNEW\nc\n");
        assert_eq!(
            changed_row_spans(&before),
            vec![(TextOperation::Delete, 1, 2)]
        );
        assert_eq!(
            changed_row_spans(&after),
            vec![(TextOperation::Insert, 1, 2)]
        );
    }

    /// Non-contiguous matches (matched, gap on both sides, matched, gap on both sides, matched) -
    /// the case most likely to expose a grouping bug, since a naive implementation might zip the
    /// two sides' gaps together instead of walking each side's own row space independently.
    #[test]
    fn plain_text_line_diff_handles_non_contiguous_matches() {
        let before = "same0\nDEL_A\nsame1\nDEL_B\nDEL_C\nsame2\n";
        let after = "same0\nINS_A\nINS_B\nsame1\nsame2\n";
        let (before_ranges, after_ranges) = plain_text_line_diff(before, after);

        assert_eq!(
            changed_row_spans(&before_ranges),
            vec![(TextOperation::Delete, 1, 2), (TextOperation::Delete, 3, 5)],
            "before's two unmatched runs (rows 1 and 3-4) must stay separate, not merge across \
             the row-2 match"
        );
        assert_eq!(
            changed_row_spans(&after_ranges),
            vec![(TextOperation::Insert, 1, 3)],
            "after's contiguous unmatched run (rows 1-2) must merge into one range"
        );

        // same0 (before row 0) matches after row 0; same1 (before row 2) matches after row 3;
        // same2 (before row 5) matches after row 4.
        let matches: Vec<_> = before_ranges
            .iter()
            .filter(|r| r.operation == TextOperation::Identical)
            .map(|r| (r.source.start_row, r.destination.start_row))
            .collect();
        assert_eq!(matches, vec![(0, 0), (2, 3), (5, 4)]);
    }

    /// The cross-panel cursor anchor for an unmatched run must land right after the nearest
    /// *preceding* match in the other side's coordinate space - not at the unmatched run's own
    /// row number, which is a different (and generally diverging) coordinate space once earlier
    /// insertions/deletions have shifted the two sides out of alignment.
    #[test]
    fn plain_text_line_diff_anchors_unmatched_runs_at_the_preceding_matchs_destination() {
        // before: same0, DEL, same1        (3 lines)
        // after:  same0, INS_A, INS_B, same1  (4 lines) - "same1" sits at a different row on
        // each side (before row 2, after row 3), so a correct anchor must use the *destination*
        // coordinate space, not reuse the source row number.
        let before = "same0\nDEL\nsame1\n";
        let after = "same0\nINS_A\nINS_B\nsame1\n";
        let (before_ranges, after_ranges) = plain_text_line_diff(before, after);

        let delete = before_ranges
            .iter()
            .find(|r| r.operation == TextOperation::Delete)
            .expect("before should have one Delete range");
        assert_eq!(
            delete.destination.start_row, 1,
            "the deleted before-row-1 line has no real counterpart, so its cross-highlight \
             anchor should sit right after same0's match (after row 0), i.e. after row 1"
        );

        let insert = after_ranges
            .iter()
            .find(|r| r.operation == TextOperation::Insert)
            .expect("after should have one Insert range");
        assert_eq!(
            insert.destination.start_row, 1,
            "the inserted after-rows have no real counterpart, so its cross-highlight anchor \
             should sit right after same0's match (before row 0), i.e. before row 1"
        );
    }

    #[test]
    fn plain_text_line_diff_handles_empty_before_as_a_pure_insertion() {
        let (before, after) = plain_text_line_diff("", "a\nb\n");
        assert!(before.is_empty());
        assert_eq!(
            changed_row_spans(&after),
            vec![(TextOperation::Insert, 0, 2)]
        );
    }

    #[test]
    fn plain_text_line_diff_handles_empty_after_as_a_pure_deletion() {
        let (before, after) = plain_text_line_diff("a\nb\n", "");
        assert_eq!(
            changed_row_spans(&before),
            vec![(TextOperation::Delete, 0, 2)]
        );
        assert!(after.is_empty());
    }

    #[test]
    fn plain_text_line_diff_treats_two_empty_files_as_no_changes() {
        let (before, after) = plain_text_line_diff("", "");
        assert!(before.is_empty());
        assert!(after.is_empty());
    }

    /// Past the edit-distance cap, `myers_lcs` gives up and the whole file is treated as replaced
    /// - one Delete covering all of before, one Insert covering all of after - rather than paying
    ///   for an unbounded search. Exercises `plain_text_line_diff_with_max_edit` with a small cap
    ///   rather than the real `PLAIN_TEXT_MAX_EDIT` (10,000): actually reaching that cap costs
    ///   O(10,000²) - ~1.6GB and genuinely slow - which the "gave up" *logic* doesn't need paying
    ///   for just to verify it fires and produces the right ranges.
    #[test]
    fn plain_text_line_diff_replaces_the_whole_file_past_the_edit_cap() {
        const SMALL_CAP: usize = 20;
        let before: String = (0..SMALL_CAP + 10)
            .map(|i| format!("before-unique-line-{i}\n"))
            .collect();
        let after: String = (0..SMALL_CAP + 10)
            .map(|i| format!("after-unique-line-{i}\n"))
            .collect();
        let (before_ranges, after_ranges) =
            plain_text_line_diff_with_max_edit(&before, &after, SMALL_CAP);

        assert_eq!(before_ranges.len(), 1);
        assert_eq!(before_ranges[0].operation, TextOperation::Delete);
        assert_eq!(
            before_ranges[0].source.end_row,
            SMALL_CAP + 10,
            "the single Delete range should cover every line, not just part of the file"
        );

        assert_eq!(after_ranges.len(), 1);
        assert_eq!(after_ranges[0].operation, TextOperation::Insert);
        assert_eq!(after_ranges[0].source.end_row, SMALL_CAP + 10);
    }

    /// Confirms the real, production `PLAIN_TEXT_MAX_EDIT` actually is large enough to cover a
    /// realistic large-file edit - a 10,000-line file with a change scattered across it (not just
    /// a handful of lines) - without falling back to "whole file replaced". Cheap despite the
    /// large line count: Myers' search terminates at the *actual* edit distance, not the cap, so
    /// this only costs O(changed_lines²), not O(PLAIN_TEXT_MAX_EDIT²).
    #[test]
    fn plain_text_line_diff_handles_a_ten_thousand_line_file_with_scattered_changes() {
        let before: String = (0..10_000).map(|i| format!("line-{i}\n")).collect();
        let after: String = (0..10_000)
            .map(|i| {
                if i % 137 == 0 {
                    format!("changed-line-{i}\n")
                } else {
                    format!("line-{i}\n")
                }
            })
            .collect();
        let (before_ranges, after_ranges) = plain_text_line_diff(&before, &after);

        assert!(
            before_ranges
                .iter()
                .any(|r| r.operation == TextOperation::Delete),
            "a real per-line diff should find the scattered deletes, not give up and replace the \
             whole file: got {} before ranges",
            before_ranges.len()
        );
        assert!(
            after_ranges
                .iter()
                .any(|r| r.operation == TextOperation::Insert),
            "a real per-line diff should find the scattered inserts, not give up and replace the \
             whole file: got {} after ranges",
            after_ranges.len()
        );
        assert!(
            before_ranges.len() > 10,
            "10,000/137 ≈ 73 scattered changes should produce many small ranges, not one giant \
             replaced-whole-file range: got {} before ranges",
            before_ranges.len()
        );
    }

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

    /// `before` is `"a\nb\nc"` (3 lines), `after` is `"a\nX\nc\nd"` (4 lines): row 1 updated
    /// (`b` -> `X`, present as an Update range on both sides), row 3 inserted (`d`, only on the
    /// after side). Update must be counted once, not twice, even though both sides carry an Update
    /// range for the same row.
    #[test]
    fn change_counts_tallies_insertions_deletions_and_updates_without_double_counting() {
        let update_range = |row: usize| RangeMatch {
            source: TextRange::new(row, 0, row, 1),
            destination: TextRange::new(row, 0, row, 1),
            operation: TextOperation::Update,
        };
        let before_ranges = vec![update_range(1)];
        let after_ranges = vec![
            update_range(1),
            RangeMatch {
                source: TextRange::new(3, 0, 3, 1),
                destination: TextRange::new(3, 0, 3, 1),
                operation: TextOperation::Insert,
            },
        ];

        let counts = change_counts("a\nb\nc", "a\nX\nc\nd", &before_ranges, &after_ranges);
        assert_eq!(
            counts,
            ChangeCounts {
                insertions: 1,
                deletions: 0,
                updates: 1,
            }
        );
    }

    #[test]
    fn no_change_all_ranges() -> Result<()> {
        let (before, after) = test::helper::handmade_test_code_pair("rust-no-change")?;
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
        let (before, after) =
            test::helper::handmade_test_code_pair("rust-hello-world-added-message")?;
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
        let (before, after) = test::helper::handmade_test_code_pair("python-added-if-block")?;
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

    /// Regression guard for a real finding, not a hypothetical: confirmed via the actual pipeline
    /// (adding one line to an otherwise-unchanged file) that "only Insert operations present"
    /// alone is not enough to mean NewFile - the rest of the file being untouched shows up as
    /// Identical ranges, which must disqualify NewFile just as much as a Delete/Update/Move would.
    #[test]
    fn summarize_diff_is_not_new_file_when_inserts_are_mixed_with_identical_content() {
        let ranges = vec![
            range(TextOperation::Identical),
            range(TextOperation::Insert),
        ];
        assert_eq!(
            summarize_diff(
                "fn main() {\n    foo();\n}",
                "fn main() {\n    foo();\n    bar();\n}",
                &ranges,
                &ranges
            ),
            None
        );
    }

    #[test]
    fn summarize_diff_is_not_deleted_file_when_deletes_are_mixed_with_identical_content() {
        let ranges = vec![
            range(TextOperation::Identical),
            range(TextOperation::Delete),
        ];
        assert_eq!(
            summarize_diff(
                "fn main() {\n    foo();\n    bar();\n}",
                "fn main() {\n    foo();\n}",
                &ranges,
                &ranges
            ),
            None
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

    /// Real `Code`/`diff_code` pairs, not hand-built `ASTDiff`s - `is_comment_only_diff` needs
    /// genuine node kinds (`nodes::is_comment` reads `node.kind()`), which only a real parse
    /// provides.
    fn diff_ast(
        before_src: &str,
        after_src: &str,
    ) -> (crate::code::Code, crate::code::Code, ASTDiff, NodeCache) {
        let before = crate::code::Code::from_string(before_src, &crate::code::Language::Rust);
        let after = crate::code::Code::from_string(after_src, &crate::code::Language::Rust);
        let node_cache = NodeCache::build(&before, &after);
        let diff = crate::diff::diff_code(&before, &after);
        let ast = diff
            .ast
            .expect("diff_code should always produce an AST for valid Rust");
        (before, after, ast, node_cache)
    }

    /// A comment whose text changed (as opposed to being wholly inserted/deleted) is tagged
    /// `MatchButNotIdentical` by the pipeline, not `Update` - confirmed via a real parse. This
    /// used to be a real, separate gap shared with `ranges` (which had no arm for
    /// `MatchButNotIdentical` at all, so a changed comment produced no visible diff whatsoever -
    /// confirmed via `codediff --headless` against the real binary): `is_comment_only_diff`
    /// deliberately mirrored that blind spot rather than "fixing" it unilaterally, since the
    /// status bar must never claim something changed when the diff below it shows nothing. Now
    /// that `ranges` handles `MatchButNotIdentical` (via `own_content`), this must too, and does.
    #[test]
    fn is_comment_only_diff_is_true_when_only_a_comments_text_changed() {
        let (before, after, ast, node_cache) = diff_ast(
            "// old comment\nfn main() {}",
            "// new comment\nfn main() {}",
        );
        assert!(is_comment_only_diff(&before, &after, &ast, &node_cache));
    }

    #[test]
    fn is_comment_only_diff_is_true_when_a_comment_was_inserted() {
        let (before, after, ast, node_cache) =
            diff_ast("fn main() {}", "// a comment\nfn main() {}");
        assert!(is_comment_only_diff(&before, &after, &ast, &node_cache));
    }

    #[test]
    fn is_comment_only_diff_is_true_when_a_comment_was_deleted() {
        let (before, after, ast, node_cache) =
            diff_ast("// a comment\nfn main() {}", "fn main() {}");
        assert!(is_comment_only_diff(&before, &after, &ast, &node_cache));
    }

    #[test]
    fn is_comment_only_diff_is_false_for_a_real_code_change() {
        let (before, after, ast, node_cache) =
            diff_ast("fn main() { old(); }", "fn main() { new(); }");
        assert!(!is_comment_only_diff(&before, &after, &ast, &node_cache));
    }

    #[test]
    fn is_comment_only_diff_is_false_when_a_comment_and_real_code_both_changed() {
        let (before, after, ast, node_cache) = diff_ast(
            "// old comment\nfn main() { old(); }",
            "// new comment\nfn main() { new(); }",
        );
        assert!(!is_comment_only_diff(&before, &after, &ast, &node_cache));
    }

    #[test]
    fn is_comment_only_diff_is_false_when_nothing_changed_at_all() {
        // Vacuous case: no qualifying operation exists anywhere, so there is nothing to claim is
        // "comment-only" about - see this function's own doc comment on why this must be false,
        // not true.
        let (before, after, ast, node_cache) = diff_ast("fn main() {}", "fn main() {}");
        assert!(!is_comment_only_diff(&before, &after, &ast, &node_cache));
    }

    /// A container whose text differs everywhere purely because a statement got wrapped one level
    /// deeper - not because any comment changed - must not spuriously look "comment-only" just
    /// because `MatchButNotIdentical` is now checked. Regression guard tied directly to the same
    /// real bug `ranges`' own `own_content`-vs-child-operation development history hit (see
    /// `ranges`'s `MatchButNotIdentical` arm doc comment): a statement moving one level deeper
    /// stays `Identical` at the AST level, so a naive check could have missed that the enclosing
    /// block's real content changed.
    #[test]
    fn is_comment_only_diff_is_false_when_a_statement_moves_one_level_deeper() {
        let (before, after, ast, node_cache) = diff_ast(
            "fn main() {\n    foo();\n    bar();\n}",
            "fn main() {\n    foo();\n    if true {\n        bar();\n    }\n}",
        );
        assert!(!is_comment_only_diff(&before, &after, &ast, &node_cache));
    }

    #[test]
    fn summarize_diff_with_comment_check_reports_comment_only_over_no_classification() {
        let ranges = vec![range(TextOperation::Update)];
        assert_eq!(
            summarize_diff_with_comment_check("// a", "// b", &ranges, &ranges, true),
            Some(DiffSummary::CommentOnly)
        );
    }

    #[test]
    fn summarize_diff_with_comment_check_reports_comment_only_over_refactor_moved_only() {
        let ranges = vec![range(TextOperation::Move)];
        assert_eq!(
            summarize_diff_with_comment_check(
                "fn a() {}\nfn b() {}",
                "fn b() {}\nfn a() {}",
                &ranges,
                &ranges,
                true,
            ),
            Some(DiffSummary::CommentOnly)
        );
    }

    #[test]
    fn summarize_diff_with_comment_check_does_not_override_new_file() {
        let before_ranges: Vec<RangeMatch> = vec![];
        let after_ranges = vec![range(TextOperation::Insert)];
        assert_eq!(
            summarize_diff_with_comment_check(
                "",
                "// just a comment",
                &before_ranges,
                &after_ranges,
                true,
            ),
            Some(DiffSummary::NewFile),
            "a wholly new file should stay NewFile even if it's all comments"
        );
    }

    #[test]
    fn summarize_diff_with_comment_check_ignores_the_flag_when_false() {
        let ranges = vec![range(TextOperation::Move)];
        assert_eq!(
            summarize_diff_with_comment_check(
                "fn a() {}\nfn b() {}",
                "fn b() {}\nfn a() {}",
                &ranges,
                &ranges,
                false,
            ),
            Some(DiffSummary::RefactorMovedOnly)
        );
    }
}
