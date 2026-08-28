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
use tree_sitter::{Node, Point, Range};

use crate::{
    code::{Code, metadata::compute_columns_per_row},
    diff::{
        ASTDiff, ASTMappingOperation, NodeCache, nodes,
        text_range::{SourceText, TextRange},
    },
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

/// Like `own_content`, but returns the single contiguous gap's start point and byte range instead
/// of concatenating every gap into a `String` - `None` if the node's own content is split across
/// more than one gap (e.g. a container with content both before its first child and after its
/// last). Precise sub-node positions (see `intra_node_update_ranges`) only make sense for a single
/// contiguous span; a node with multiple gaps keeps reporting the whole node as changed, same as
/// before this existed.
fn own_content_span(node: Node) -> Option<(Point, usize, usize)> {
    let mut pos = node.start_byte();
    let mut gap_start_point = node.start_position();
    let mut gap: Option<(Point, usize, usize)> = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.start_byte() > pos {
            if gap.is_some() {
                return None;
            }
            gap = Some((gap_start_point, pos, child.start_byte()));
        }
        pos = pos.max(child.end_byte());
        gap_start_point = child.end_position();
    }
    if node.end_byte() > pos {
        if gap.is_some() {
            return None;
        }
        gap = Some((gap_start_point, pos, node.end_byte()));
    }
    gap
}

/// Byte length of the longest common prefix between `a` and `b`, respecting UTF-8 character
/// boundaries (never splits a multi-byte character) - the returned length is one past the last
/// matching character, which is guaranteed to be a valid byte-index boundary in *both* strings:
/// matched characters have identical `len_utf8()`, and if the comparison ran out because one
/// string is a character-wise prefix of the other, the returned length is exactly that shorter
/// string's own byte length (also always a valid boundary in itself, and thus in the other string
/// too, since every character up to it matched one-for-one).
fn common_prefix_byte_len(a: &str, b: &str) -> usize {
    let mut len = 0;
    let mut a_chars = a.char_indices();
    let mut b_chars = b.chars();
    loop {
        match (a_chars.next(), b_chars.next()) {
            (Some((i, ca)), Some(cb)) if ca == cb => len = i + ca.len_utf8(),
            _ => break,
        }
    }
    len
}

/// Byte length of the longest common suffix between `a` and `b` - same boundary guarantees as
/// `common_prefix_byte_len`, mirrored from the end. Callers pass strings already trimmed of their
/// common prefix, so the returned length can never overlap it.
fn common_suffix_byte_len(a: &str, b: &str) -> usize {
    let mut len = 0;
    let mut a_chars = a.chars().rev();
    let mut b_chars = b.chars().rev();
    loop {
        match (a_chars.next(), b_chars.next()) {
            (Some(ca), Some(cb)) if ca == cb => len += ca.len_utf8(),
            _ => break,
        }
    }
    len
}

/// The `tree_sitter::Point` reached after advancing `offset` bytes into `text`, starting from
/// `start`. Byte-based, not char-based, to match tree-sitter's own column convention (see
/// `text_range::SourceText::byte_index`'s doc comment) - `offset` must land on a char boundary of
/// `text`, which every call site guarantees (see `common_prefix_byte_len`/`common_suffix_byte_len`
/// above).
fn point_at_byte_offset(text: &str, start: Point, offset: usize) -> Point {
    let mut row = start.row;
    let mut column = start.column;
    for &b in &text.as_bytes()[..offset] {
        if b == b'\n' {
            row += 1;
            column = 0;
        } else {
            column += 1;
        }
    }
    Point { row, column }
}

/// Builds a `TextRange` from two `tree_sitter::Point`s directly, for sub-node spans that were
/// never a real tree-sitter node's own range (e.g. the changed middle portion of an updated
/// string/comment). `TextRange::from_treesitter_range` only reads the point fields of its
/// `tree_sitter::Range` argument, not the byte fields (it recomputes end-of-row normalization from
/// `columns_per_row` itself), so the synthetic range's byte fields are never read and left at 0.
fn text_range_from_points(start: Point, end: Point, columns_per_row: &[usize]) -> TextRange {
    let ts_range = Range {
        start_byte: 0,
        end_byte: 0,
        start_point: start,
        end_point: end,
    };
    TextRange::from_treesitter_range(ts_range, columns_per_row)
}

/// One side's own text plus what's needed to turn a byte offset into it back into a `TextRange`:
/// the point it starts at in the full file, and that file's per-row column counts (for
/// `text_range_from_points`'s end-of-row normalization). Bundled together purely to keep
/// `intra_node_update_ranges` under clippy's argument-count limit - `source`/`destination` are
/// otherwise completely independent, never compared against each other structurally.
struct TextSpan<'a> {
    text: &'a str,
    start: Point,
    columns: &'a [usize],
}

/// Splits an `Update`/`MatchButNotIdentical` node's own text into up to three sub-ranges - a
/// common Identical prefix, the differing Update middle, and a common Identical suffix - instead
/// of reporting the node's entire text as changed. This is what lets a small edit inside a long
/// string, comment, or identifier highlight only the part that actually changed.
///
/// Falls back to a single whole-span `Update` range (`whole_source_range`, anchored via
/// `last_non_move_range` exactly like the pre-existing behavior this replaces) when there's no
/// common prefix or suffix at all - the two texts differ from their very first to their very last
/// character, so there's nothing more precise to report.
///
/// The prefix/suffix `Identical` sub-ranges get *real* destination positions (derived from
/// `destination_start`, the actual matched node/span's own start point) rather than the usual
/// placeholder `last_non_move_range` anchor: unlike a plain Delete/Insert, an Update's matched
/// counterpart really does exist at a real position, and giving `Identical` ranges fabricated
/// destinations would risk corrupting `extend_into` accumulation if one ever merged with a
/// genuinely-Identical neighbor range that does carry a real destination. The Update middle still
/// uses the placeholder anchor, matching the pre-existing convention that only `Identical`
/// ranges carry cross-file-accurate destinations.
///
/// Symmetric by construction: `common_prefix_byte_len`/`common_suffix_byte_len` only compare
/// characters pairwise for equality, which doesn't depend on which string is "source" and which is
/// "destination" - so calling this with the two texts swapped (as `ranges` does, once for
/// before->after and once for after->before) always produces the same number of sub-ranges, with
/// the same operations, in the same order. `ranges`'s caller is responsible for the other half of
/// this guarantee: pushing a multi-range result straight into `ranges` rather than through the
/// usual same-operation-neighbor-merging accumulator, since that merging depends on each side's own
/// (possibly different) surrounding text and could otherwise make the two sides' sub-range counts
/// diverge after accumulation even though this function itself is symmetric.
fn intra_node_update_ranges(
    last_non_move_range: &mut TextRange,
    whole_source_range: TextRange,
    source: TextSpan,
    destination: TextSpan,
) -> Vec<RangeMatch> {
    let prefix_len = common_prefix_byte_len(source.text, destination.text);
    let suffix_len =
        common_suffix_byte_len(&source.text[prefix_len..], &destination.text[prefix_len..]);

    if prefix_len == 0 && suffix_len == 0 {
        return vec![advance_and_build_range_with_source(
            last_non_move_range,
            whole_source_range,
            TextOperation::Update,
        )];
    }

    let mut result = Vec::with_capacity(3);

    if prefix_len > 0 {
        let source_end = point_at_byte_offset(source.text, source.start, prefix_len);
        let destination_end = point_at_byte_offset(destination.text, destination.start, prefix_len);
        result.push(RangeMatch {
            source: text_range_from_points(source.start, source_end, source.columns),
            destination: text_range_from_points(
                destination.start,
                destination_end,
                destination.columns,
            ),
            operation: TextOperation::Identical,
        });
    }

    let source_mid_len = source.text.len() - prefix_len - suffix_len;
    let destination_mid_len = destination.text.len() - prefix_len - suffix_len;
    if source_mid_len > 0 || destination_mid_len > 0 {
        let source_mid_start = point_at_byte_offset(source.text, source.start, prefix_len);
        let source_mid_end =
            point_at_byte_offset(source.text, source.start, source.text.len() - suffix_len);
        result.push(advance_and_build_range_with_source(
            last_non_move_range,
            text_range_from_points(source_mid_start, source_mid_end, source.columns),
            TextOperation::Update,
        ));
    }

    if suffix_len > 0 {
        let source_start_point =
            point_at_byte_offset(source.text, source.start, source.text.len() - suffix_len);
        let source_end_point = point_at_byte_offset(source.text, source.start, source.text.len());
        let destination_start_point = point_at_byte_offset(
            destination.text,
            destination.start,
            destination.text.len() - suffix_len,
        );
        let destination_end_point =
            point_at_byte_offset(destination.text, destination.start, destination.text.len());
        result.push(RangeMatch {
            source: text_range_from_points(source_start_point, source_end_point, source.columns),
            destination: text_range_from_points(
                destination_start_point,
                destination_end_point,
                destination.columns,
            ),
            operation: TextOperation::Identical,
        });
    }

    result
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
    // Row offsets for both sides, built once for the whole traversal. `RangeMatch::extends` below
    // needs them for every merge decision it makes, and rebuilding them per decision is exactly the
    // cost this replaced.
    let source_text = SourceText::new(&source.contents);
    let destination_text = SourceText::new(&destination.contents);

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
                    let mut new_ranges: Vec<RangeMatch> = Vec::new();
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
                                //
                                // A second, column-preserving relocation also counts: a node whose
                                // destination starts *before* the last sequential anchor crossed
                                // over earlier content - siblings reordered (e.g. two top-level
                                // functions swapped). Same-column row shifts from unrelated edits
                                // elsewhere never run backwards, so this can't fire for them.
                                // Restricted to nodes spanning a row boundary: sub-line tokens
                                // (a `}`, an operator) can legitimately match an earlier identical
                                // occurrence when matching is imperfect, and flagging those would
                                // paint noise, while a multi-row (or full-line) match landing
                                // backwards is a real reorder. Before this check, a pure sibling
                                // reorder produced no non-Identical range at all - the diff
                                // rendered as completely unchanged (the gap `DiffSummary::
                                // RefactorMovedOnly`'s doc comment used to describe).
                                let crossed_backwards = s.end_row > s.start_row
                                    && (d.start_row, d.start_column)
                                        < (
                                            last_non_move_range.start_row,
                                            last_non_move_range.start_column,
                                        );
                                // A column change means the node was relocated - *unless* it is a
                                // multi-row node that stayed on its own starting row, in which
                                // case the shift is text inserted earlier on that line pushing it
                                // rightwards, and only its first row moved at all.
                                //
                                // Marking the whole subtree moved on that evidence over-reports by
                                // the size of the subtree: adding `const ` to one parameter used
                                // to paint an entire function body as moved
                                // (`cpp-add-const-correctness`, where the human paints only the
                                // inserted `const`).
                                //
                                // The two exclusions are both load-bearing, and each was put here
                                // by a measurement:
                                //
                                // * single-row nodes keep the old treatment - the painted corpus
                                //   holds 16 human-painted moves that are column-only on one row,
                                //   across six fixtures, so the rule is right there;
                                // * a multi-row node that *also* changed rows keeps it too - that
                                //   is a real relocation, and excluding it regressed
                                //   `rust-add-if` (a block genuinely moved into a new `if`) from
                                //   0.7% to 56.5% disagreement with its painting.
                                let shifted_within_its_own_line =
                                    s.start_row == d.start_row && s.end_row > s.start_row;
                                let column_shift_is_meaningful = s.start_column != d.start_column
                                    && !shifted_within_its_own_line;
                                if !column_shift_is_meaningful && !crossed_backwards {
                                    last_non_move_range = d.clone();

                                    new_ranges.push(RangeMatch {
                                        source: s,
                                        destination: d,
                                        operation: TextOperation::Identical,
                                    });
                                } else {
                                    new_ranges.push(RangeMatch {
                                        source: s,
                                        destination: d,
                                        operation: TextOperation::Move,
                                    });
                                }

                                descend = false;
                            }
                        }
                        ASTMappingOperation::DeleteWithChildren => {
                            new_ranges.push(advance_and_build_range(
                                &mut last_non_move_range,
                                node,
                                &source_columns,
                                TextOperation::Delete,
                            ));
                            descend = false;
                        }
                        ASTMappingOperation::InsertWithChildren => {
                            new_ranges.push(advance_and_build_range(
                                &mut last_non_move_range,
                                node,
                                &source_columns,
                                TextOperation::Insert,
                            ));
                            descend = false;
                        }
                        ASTMappingOperation::Delete if node.child_count() == 0 => {
                            new_ranges.push(advance_and_build_range(
                                &mut last_non_move_range,
                                node,
                                &source_columns,
                                TextOperation::Delete,
                            ));
                        }
                        ASTMappingOperation::Insert if node.child_count() == 0 => {
                            new_ranges.push(advance_and_build_range(
                                &mut last_non_move_range,
                                node,
                                &source_columns,
                                TextOperation::Insert,
                            ));
                        }
                        // See `intra_node_update_ranges`'s own doc comment for the sub-range
                        // split (common prefix/middle/suffix) and why the prefix/suffix
                        // `Identical` pieces get `destination_node`'s real position instead of
                        // the placeholder `last_non_move_range` anchor every other arm here uses.
                        ASTMappingOperation::Update => {
                            if let Some(&destination_node) = node_cache.get_in_any(&mapped_id) {
                                let source_text =
                                    node.utf8_text(source.contents.as_bytes()).unwrap_or("");
                                let destination_text = destination_node
                                    .utf8_text(destination.contents.as_bytes())
                                    .unwrap_or("");
                                new_ranges = intra_node_update_ranges(
                                    &mut last_non_move_range,
                                    TextRange::from_treesitter_range(node.range(), &source_columns),
                                    TextSpan {
                                        text: source_text,
                                        start: node.start_position(),
                                        columns: &source_columns,
                                    },
                                    TextSpan {
                                        text: destination_text,
                                        start: destination_node.start_position(),
                                        columns: &destination_columns,
                                    },
                                );
                            } else {
                                new_ranges.push(advance_and_build_range(
                                    &mut last_non_move_range,
                                    node,
                                    &source_columns,
                                    TextOperation::Update,
                                ));
                            }
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
                        // See `intra_node_update_ranges`'s own doc comment for the sub-range
                        // split. `own_content_span` (unlike `own_content`) needs the node's
                        // content to sit in a single contiguous gap to report precise positions -
                        // when it doesn't (multiple gaps), fall back to the pre-existing
                        // whole-node placeholder behavior below.
                        ASTMappingOperation::MatchButNotIdentical => {
                            if let Some(&destination_node) = node_cache.get_in_any(&mapped_id) {
                                let source_content = own_content(node, source.contents.as_bytes());
                                let destination_content =
                                    own_content(destination_node, destination.contents.as_bytes());
                                if !whitespace_stripped_equal(&source_content, &destination_content)
                                {
                                    new_ranges = match (
                                        own_content_span(node),
                                        own_content_span(destination_node),
                                    ) {
                                        (
                                            Some((s_start, s_from, s_to)),
                                            Some((d_start, d_from, d_to)),
                                        ) => intra_node_update_ranges(
                                            &mut last_non_move_range,
                                            TextRange::from_treesitter_range(
                                                node.range(),
                                                &source_columns,
                                            ),
                                            TextSpan {
                                                text: &source.contents[s_from..s_to],
                                                start: s_start,
                                                columns: &source_columns,
                                            },
                                            TextSpan {
                                                text: &destination.contents[d_from..d_to],
                                                start: d_start,
                                                columns: &destination_columns,
                                            },
                                        ),
                                        _ => vec![advance_and_build_range(
                                            &mut last_non_move_range,
                                            node,
                                            &source_columns,
                                            TextOperation::Update,
                                        )],
                                    };
                                    descend = false;
                                }
                            }
                        }
                        _ => {
                            // For other operations, just allow the descent into the tree
                        }
                    }

                    // A node that decomposed into more than one sub-range (see
                    // `intra_node_update_ranges`) is pushed straight into `ranges`, bypassing the
                    // usual same-operation-neighbor-merging accumulator below: that merging
                    // depends on each side's own surrounding text via `can_extend_with_whitespace`,
                    // which can differ between the before->after and after->before traversals and
                    // would risk making the two sides' final range counts diverge even though
                    // `intra_node_update_ranges` itself always produces a symmetric sub-range
                    // count - see that function's doc comment. A single-range result (every other
                    // arm, plus `intra_node_update_ranges`'s own no-common-affix fallback) is
                    // exactly the pre-existing behavior and keeps going through the accumulator.
                    if new_ranges.len() > 1 {
                        if !current_range.is_zero() {
                            ranges.push(current_range);
                        }
                        ranges.extend(new_ranges);
                        current_range = RangeMatch::zero();
                    } else {
                        for new_range in new_ranges {
                            if new_range.extends(&current_range, &source_text, &destination_text) {
                                current_range.extend_into(&new_range);
                            } else {
                                if !current_range.is_zero() {
                                    ranges.push(current_range);
                                }
                                current_range = new_range;
                            }
                        }
                    }

                    if !descend {
                        continue;
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
    advance_and_build_range_with_source(
        last_non_move_range,
        TextRange::from_treesitter_range(node.range(), columns),
        operation,
    )
}

/// Same as `advance_and_build_range`, but for a caller that already has the exact `TextRange` to
/// use as the source (e.g. `intra_node_update_ranges`'s middle sub-range, a byte span within a
/// node rather than a whole node's own `node.range()`).
fn advance_and_build_range_with_source(
    last_non_move_range: &mut TextRange,
    source_range: TextRange,
    operation: TextOperation,
) -> RangeMatch {
    *last_non_move_range = last_non_move_range.right_limit();
    RangeMatch {
        source: source_range,
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

/// How many rows a range covers, in `line_operations`' convention: an end column of 0 already
/// means "up to, not including, this row", so only a genuinely mid-row end needs the extra one.
/// Never zero - a sub-line range still occupies the row it sits on.
fn rows_covered(range: &TextRange) -> usize {
    let end_row = if range.end_column == 0 {
        range.end_row
    } else {
        range.end_row + 1
    };
    end_row.saturating_sub(range.start_row).max(1)
}

/// Makes the two sides agree on *which* matched pair relocated, by believing whichever side
/// blames fewer rows.
///
/// `TextDiff::from` builds each side by walking that side's tree in its own order, and `ranges`'
/// `crossed_backwards` test asks whether a node's destination lands behind the walk's running
/// anchor. That is a question about walk order, not about the pair - so on a reorder the two
/// walks name *different* pairs. Moving one import below a block of five, the before walk passes
/// the mover first (advancing its anchor past the block) and then sees all five land behind it, so
/// it flags the five; the after walk passes the five in order and sees only the mover land behind,
/// so it flags one. Both are self-consistent and only one can be right.
///
/// Neither the union nor the intersection is the answer. The intersection is empty here - each
/// pair is flagged by exactly one walk - which loses the reorder entirely, the very regression
/// `crossed_backwards` exists to prevent. The union paints all six, which is worse than either
/// walk alone. What separates them is size: relocating one line past five is one move, not five,
/// and the walk that says "one" is the one that matched a reader's account of the edit.
///
/// So: sum the rows each side blames *where the two disagree*, and keep the smaller account,
/// rewriting the other side to match it. Agreed pairs are never touched, so a file whose walks
/// already agree comes out byte-identical.
///
/// Two limits worth stating rather than discovering later. The comparison is **per file, not per
/// reorder**: a file containing two independent reorders that disagree in opposite directions gets
/// one global verdict, and the minority one is decided wrongly. Grouping disagreements into
/// clusters needs a notion of which reorder a pair belongs to that nothing here has. And promoting
/// the winner's counterpart needs to *find* it - an exact extent lookup into the other side's
/// range list, which is not quite total (measured at 99.2% for the analogous lookup in
/// `generate_mapping_site`), so a counterpart that isn't found stays unpainted rather than being
/// invented.
fn reconcile_moves(before: &mut [RangeMatch], after: &mut [RangeMatch]) {
    use std::collections::HashMap;

    let key = |r: &TextRange| (r.start_row, r.start_column, r.end_row, r.end_column);
    let index_of = |ranges: &[RangeMatch]| -> HashMap<(usize, usize, usize, usize), usize> {
        let mut map = HashMap::new();
        for (index, range_match) in ranges.iter().enumerate() {
            if range_match.source.is_empty() {
                continue;
            }
            map.entry(key(&range_match.source)).or_insert(index);
        }
        map
    };
    let before_index = index_of(before);
    let after_index = index_of(after);

    // `(this side's index, the other side's index for the same pair)` for every `Move` whose
    // counterpart is *not* also a `Move` - i.e. exactly the pairs the two walks disagree about.
    let disagreements = |side: &[RangeMatch],
                         other: &[RangeMatch],
                         other_index: &HashMap<(usize, usize, usize, usize), usize>|
     -> Vec<(usize, Option<usize>)> {
        side.iter()
            .enumerate()
            .filter(|(_, range_match)| {
                range_match.operation == TextOperation::Move && !range_match.destination.is_empty()
            })
            .filter_map(|(index, range_match)| {
                match other_index.get(&key(&range_match.destination)).copied() {
                    Some(other_index) if other[other_index].operation == TextOperation::Move => {
                        None
                    }
                    counterpart => Some((index, counterpart)),
                }
            })
            .collect()
    };

    let before_disagreed = disagreements(before, after, &after_index);
    let after_disagreed = disagreements(after, before, &before_index);
    if before_disagreed.is_empty() && after_disagreed.is_empty() {
        return;
    }

    let blamed_rows = |ranges: &[RangeMatch], disagreed: &[(usize, Option<usize>)]| -> usize {
        disagreed
            .iter()
            .map(|&(index, _)| rows_covered(&ranges[index].source))
            .sum()
    };
    let before_rows = blamed_rows(before, &before_disagreed);
    let after_rows = blamed_rows(after, &after_disagreed);

    // Ties go to the before side. Arbitrary, but it has to be *some* fixed side or the result
    // stops being a function of the input; the symmetric case this reaches is two accounts of
    // equal size, where neither is more economical than the other.
    let (winner, loser) = if before_rows <= after_rows {
        (&before_disagreed, &mut *after)
    } else {
        (&after_disagreed, &mut *before)
    };

    // The loser's own claims are withdrawn: their counterparts on the winning side are already
    // `Identical`, which is what makes this the half of the pair that has to change.
    for &(index, _) in if before_rows <= after_rows {
        &after_disagreed
    } else {
        &before_disagreed
    } {
        loser[index].operation = TextOperation::Identical;
    }
    // ...and the winner's counterparts are painted, which is the whole point: a `Move` the reader
    // can follow to a highlighted node on the other side rather than to an unmarked one.
    //
    // Safe against the loop above: a counterpart found here was by definition *not* a `Move`, so
    // it is never one of the indices just withdrawn.
    for &(_, counterpart) in winner {
        if let Some(counterpart) = counterpart {
            loser[counterpart].operation = TextOperation::Move;
        }
    }
}

impl TextDiff {
    /// Construct the TextDiff from an ASTDiff.
    ///
    /// An ASTDiff must exist to create the TextDiff. There is no algorithm currently
    /// implemented that can construct the TextDiff directly from code.
    pub fn from(before: &Code, after: &Code, diff: &ASTDiff, node_cache: &NodeCache) -> Self {
        let mut before_ranges_plain = ranges(before, after, diff, node_cache);
        let mut after_ranges_plain = ranges(after, before, diff, node_cache);

        // Each `ranges` call above decided `Move` from its own walk order, so the two can name
        // different pairs for the same reorder - see `reconcile_moves`.
        reconcile_moves(&mut before_ranges_plain, &mut after_ranges_plain);

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
/// Never produces `Move`: detecting one needs a notion of identity that survives relocation, and
/// hashed lines give none - a line that moved elsewhere is a delete plus an unrelated insert here,
/// same as a plain `diff -u`.
///
/// It *does* produce `Update`, with sub-line columns, which `diff -u` cannot. `plan_gap` pairs
/// rows inside a hunk by [`shared_affix`] rather than positionally, and each paired row goes
/// through [`intra_line_ranges`], which splits it into an identical prefix, an `Update` over the
/// differing middle, and an identical suffix. So a rewritten line renders as one changed line with
/// the changed characters marked, not as an adjacent delete+insert pair.
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
    match line_diff_core(before, after, max_edit) {
        // Re-split rather than widening `LineDiffCore`: that struct is also the matching
        // pipeline's own entry point (see its doc comment), which needs only counts and pairs, and
        // `str::lines` is a cheap linear scan next to the `myers_lcs` that just ran.
        Some(core) => {
            let before_lines: Vec<&str> = before.lines().collect();
            let after_lines: Vec<&str> = after.lines().collect();
            debug_assert_eq!(before_lines.len(), core.before_line_count);
            debug_assert_eq!(after_lines.len(), core.after_line_count);
            build_line_ranges(&before_lines, &after_lines, &core.pairs)
        }
        None => {
            let before_line_count = before.lines().count();
            let after_line_count = after.lines().count();
            whole_file_replaced(before_line_count, after_line_count)
        }
    }
}

/// The parser-independent line-diff core, shared between `plain_text_line_diff`'s visualization
/// path (via `plain_text_line_diff_with_max_edit`) and the matching pipeline's phases-4-7
/// rearchitecture (`TODO.md`, `~/.claude/plans/iterative-herding-panda.md`, Phase 3a). `None` means
/// `myers_lcs` gave up past `max_edit` - callers fall back to treating the whole file as replaced
/// (`whole_file_replaced`) rather than trusting a partial/nonexistent match set.
pub struct LineDiffCore {
    /// Matched `(before_row, after_row)` pairs, ascending in both (an LCS matching preserves
    /// relative order on both sides).
    pub pairs: Vec<(usize, usize)>,
    pub before_line_count: usize,
    pub after_line_count: usize,
}

/// Classification of a whole-file line diff, used to license (or refuse to license) a
/// constrained, delete-free/insert-free resolver downstream - see the phases-4-7 rearchitecture
/// plan's "Step 2 - text-diff-first classification" section. Corpus-census-validated at whole-file
/// granularity (72/338 fixtures `InsertOnly`/`DeleteOnly`, zero ground-truth counterexamples); not
/// yet validated at hunk granularity within `Mixed` files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WholeFileClass {
    /// No lines changed at all.
    Identical,
    /// Every before-line is matched (nothing deleted); at least one after-line is new.
    InsertOnly,
    /// Every after-line is matched (nothing inserted); at least one before-line is gone.
    DeleteOnly,
    /// Both insertions and deletions present, or `myers_lcs` gave up past `max_edit` (treated as
    /// `Mixed` since no license can be safely granted without knowing what actually changed).
    Mixed,
}

impl LineDiffCore {
    /// See `WholeFileClass`'s doc comment. `pairs.len() < before_line_count` means some
    /// before-line has no match (a delete happened somewhere); symmetric for inserts.
    pub fn whole_file_class(&self) -> WholeFileClass {
        let has_delete = self.pairs.len() < self.before_line_count;
        let has_insert = self.pairs.len() < self.after_line_count;
        match (has_delete, has_insert) {
            (false, false) => WholeFileClass::Identical,
            (true, false) => WholeFileClass::DeleteOnly,
            (false, true) => WholeFileClass::InsertOnly,
            (true, true) => WholeFileClass::Mixed,
        }
    }
}

/// Whole-file classification at the pipeline's default edit-distance cap (`PLAIN_TEXT_MAX_EDIT`) -
/// the entry point Phase 3a's dispatcher uses. A `myers_lcs` give-up is treated as `Mixed`: no
/// license should ever be granted from an edit distance too large to have actually been measured.
pub fn whole_file_text_class(before: &str, after: &str) -> WholeFileClass {
    match line_diff_core(before, after, PLAIN_TEXT_MAX_EDIT) {
        Some(core) => core.whole_file_class(),
        None => WholeFileClass::Mixed,
    }
}

/// Runs `myers_lcs` over hashed lines. Returns `None` if it gave up past `max_edit` (see
/// `PLAIN_TEXT_MAX_EDIT`'s doc comment) - callers must not treat a `None` as "no changes."
pub fn line_diff_core(before: &str, after: &str, max_edit: usize) -> Option<LineDiffCore> {
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();

    let before_hashes = hash_lines(&before_lines);
    let after_hashes = hash_lines(&after_lines);

    let pairs = crate::diff::apted::myers_lcs(&before_hashes, &after_hashes, max_edit)?;
    Some(LineDiffCore {
        pairs,
        before_line_count: before_lines.len(),
        after_line_count: after_lines.len(),
    })
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

/// How much of the longer of two unmatched lines their common prefix and suffix must cover before
/// [`intra_line_ranges`] will decompose them into "same, changed, same" instead of leaving them as
/// a whole-line delete + insert.
///
/// A *ratio* on the longer line, not an absolute length: structured text shares long affixes by
/// coincidence all the time (two unrelated rows of a wide CSV both ending `,0,0,0,0,0` clear any
/// absolute bar trivially), and decomposing an unrelated pair is worse than not decomposing it at
/// all - it invents a "common prefix" out of two rows that merely share a column layout, and hides
/// the real change inside a span labelled `Identical`. 50% was chosen against
/// `research/data/quality/optimal_solutions_benchmark.csv`, where a regenerated row differs only in
/// its `elapsed_ms` field (~97% shared affix) while two unrelated fixture rows sit far below half.
const MIN_SHARED_AFFIX_PERCENT: usize = 50;

/// Byte lengths of the common prefix and suffix of two unmatched lines, or `None` if they are too
/// dissimilar to be treated as one line rewritten (see [`MIN_SHARED_AFFIX_PERCENT`]). Split out
/// from [`intra_line_ranges`] because [`plan_gap`] needs the *decision* while it is still choosing
/// which lines pair with which, and only pays for the ranges once that is settled.
///
/// The suffix is measured on the already-prefix-trimmed remainders (exactly as
/// `intra_node_update_ranges` does), so prefix + suffix can never overlap on the shorter line.
fn shared_affix(before_line: &str, after_line: &str) -> Option<(usize, usize)> {
    let prefix = common_prefix_byte_len(before_line, after_line);
    let suffix = common_suffix_byte_len(&before_line[prefix..], &after_line[prefix..]);

    let longer = before_line.len().max(after_line.len());
    if longer == 0 || (prefix + suffix) * 100 < longer * MIN_SHARED_AFFIX_PERCENT {
        return None;
    }
    // Both middles empty means the two lines are byte-identical. `myers_lcs` can leave such a pair
    // unmatched when ordering constraints prevent it (reordered duplicate lines), and claiming a
    // match it deliberately didn't make is not this function's call - the caller's whole-line
    // treatment is the honest answer.
    if before_line.len() - suffix == prefix && after_line.len() - suffix == prefix {
        return None;
    }
    Some((prefix, suffix))
}

/// Sub-line ranges for one *changed* line pair: the common prefix and suffix render as `Identical`
/// and only the differing middle as `Update`, so a one-field edit in a wide line highlights that
/// field instead of the whole row. `None` under the same condition as [`shared_affix`].
///
/// Columns are byte offsets within the row, matching tree-sitter's own `Point` convention that
/// `TextRange` inherits - see `text_range::SourceText::byte_index`, whose doc comment records the
/// multi-byte-character crash that established it. `common_prefix_byte_len`/`common_suffix_byte_len`
/// both guarantee char boundaries, and the suffix is measured on the already-prefix-trimmed
/// remainders (exactly as `intra_node_update_ranges` does) so prefix + suffix can never overlap on
/// the shorter line.
///
/// Both sides always get the same number of ranges. That symmetry is load-bearing: the two
/// vectors are consumed index-comparably downstream (see `merge_ranges`, and the AST path's own
/// note in `ranges` about bypassing the merging accumulator to keep the counts from diverging).
fn intra_line_ranges(
    before_row: usize,
    before_line: &str,
    after_row: usize,
    after_line: &str,
) -> Option<(Vec<RangeMatch>, Vec<RangeMatch>)> {
    let (prefix, suffix) = shared_affix(before_line, after_line)?;
    let before_middle_end = before_line.len() - suffix;
    let after_middle_end = after_line.len() - suffix;

    let mut before_ranges = Vec::with_capacity(3);
    let mut after_ranges = Vec::with_capacity(3);

    let mut push = |b: TextRange, a: TextRange, operation: TextOperation| {
        before_ranges.push(RangeMatch {
            source: b.clone(),
            destination: a.clone(),
            operation: operation.clone(),
        });
        after_ranges.push(RangeMatch {
            source: a,
            destination: b,
            operation,
        });
    };

    if prefix > 0 {
        push(
            TextRange::new(before_row, 0, before_row, prefix),
            TextRange::new(after_row, 0, after_row, prefix),
            TextOperation::Identical,
        );
    }
    push(
        TextRange::new(before_row, prefix, before_row, before_middle_end),
        TextRange::new(after_row, prefix, after_row, after_middle_end),
        TextOperation::Update,
    );
    if suffix > 0 {
        push(
            TextRange::new(before_row, before_middle_end, before_row, before_line.len()),
            TextRange::new(after_row, after_middle_end, after_row, after_line.len()),
            TextOperation::Identical,
        );
    }

    Some((before_ranges, after_ranges))
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
    before_lines: &[&str],
    after_lines: &[&str],
    pairs: &[(usize, usize)],
) -> (Vec<RangeMatch>, Vec<RangeMatch>) {
    let (before_line_count, after_line_count) = (before_lines.len(), after_lines.len());
    let mut before_ranges = Vec::new();
    let mut after_ranges = Vec::new();

    let mut next_before_row = 0;
    let mut next_after_row = 0;
    let mut last_before_match = TextRange::zero();
    let mut last_after_match = TextRange::zero();

    for &(bi, ai) in pairs {
        emit_gap(
            before_lines,
            after_lines,
            next_before_row..bi,
            next_after_row..ai,
            &last_before_match,
            &last_after_match,
            &mut before_ranges,
            &mut after_ranges,
        );

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

    emit_gap(
        before_lines,
        after_lines,
        next_before_row..before_line_count,
        next_after_row..after_line_count,
        &last_before_match,
        &last_after_match,
        &mut before_ranges,
        &mut after_ranges,
    );

    (before_ranges, after_ranges)
}

/// One unmatched run - the before-rows and after-rows between two consecutive LCS matches (or
/// after the last one).
///
/// Default behaviour is one merged `Delete` and one merged `Insert` for the whole run, which is
/// what makes a block insert/delete read, and `n`/`p`-navigate, as a single change rather than one
/// step per line. That is preserved exactly whenever the run is a genuine block: if *no* line pairs
/// off similarly enough for [`intra_line_ranges`], this emits byte-for-byte what it always did.
///
/// When lines do pair off - the same row rewritten rather than removed, e.g. a wide CSV row where
/// one field changed - the run is emitted per [`plan_gap`] instead, so each changed row narrows to
/// the part that actually differs. Consecutive unpaired rows are still merged into one range, so a
/// block sitting inside an otherwise-rewritten run keeps reading as a block.
#[allow(clippy::too_many_arguments)]
fn emit_gap(
    before_lines: &[&str],
    after_lines: &[&str],
    before_rows: std::ops::Range<usize>,
    after_rows: std::ops::Range<usize>,
    last_before_match: &TextRange,
    last_after_match: &TextRange,
    before_ranges: &mut Vec<RangeMatch>,
    after_ranges: &mut Vec<RangeMatch>,
) {
    if before_rows.is_empty() && after_rows.is_empty() {
        return;
    }

    let plan = plan_gap(
        before_lines,
        after_lines,
        before_rows.clone(),
        after_rows.clone(),
    );

    if !plan.iter().any(|op| matches!(op, GapOp::Pair(..))) {
        if !before_rows.is_empty() {
            before_ranges.push(RangeMatch {
                source: TextRange::new(before_rows.start, 0, before_rows.end, 0),
                destination: last_after_match.right_limit(),
                operation: TextOperation::Delete,
            });
        }
        if !after_rows.is_empty() {
            after_ranges.push(RangeMatch {
                source: TextRange::new(after_rows.start, 0, after_rows.end, 0),
                destination: last_before_match.right_limit(),
                operation: TextOperation::Insert,
            });
        }
        return;
    }

    // Consecutive `Delete`s (and `Insert`s) coalesce into one range, for the same
    // reads-as-one-change reason the whole-gap merge above exists. `plan_gap` emits each side's
    // rows in ascending order, so a run of them is always contiguous.
    let mut pending_delete: Option<std::ops::Range<usize>> = None;
    let mut pending_insert: Option<std::ops::Range<usize>> = None;
    let flush_delete = |pending: &mut Option<std::ops::Range<usize>>, out: &mut Vec<RangeMatch>| {
        if let Some(rows) = pending.take() {
            out.push(RangeMatch {
                source: TextRange::new(rows.start, 0, rows.end, 0),
                destination: last_after_match.right_limit(),
                operation: TextOperation::Delete,
            });
        }
    };
    let flush_insert = |pending: &mut Option<std::ops::Range<usize>>, out: &mut Vec<RangeMatch>| {
        if let Some(rows) = pending.take() {
            out.push(RangeMatch {
                source: TextRange::new(rows.start, 0, rows.end, 0),
                destination: last_before_match.right_limit(),
                operation: TextOperation::Insert,
            });
        }
    };

    for op in plan {
        match op {
            GapOp::Pair(b, a) => {
                flush_delete(&mut pending_delete, before_ranges);
                flush_insert(&mut pending_insert, after_ranges);
                let (before_parts, after_parts) =
                    intra_line_ranges(b, before_lines[b], a, after_lines[a])
                        .expect("plan_gap only emits Pair for rows shared_affix accepted");
                before_ranges.extend(before_parts);
                after_ranges.extend(after_parts);
            }
            GapOp::Delete(b) => match &mut pending_delete {
                Some(rows) if rows.end == b => rows.end = b + 1,
                _ => {
                    flush_delete(&mut pending_delete, before_ranges);
                    pending_delete = Some(b..b + 1);
                }
            },
            GapOp::Insert(a) => match &mut pending_insert {
                Some(rows) if rows.end == a => rows.end = a + 1,
                _ => {
                    flush_insert(&mut pending_insert, after_ranges);
                    pending_insert = Some(a..a + 1);
                }
            },
        }
    }
    flush_delete(&mut pending_delete, before_ranges);
    flush_insert(&mut pending_insert, after_ranges);
}

/// One decision in a gap's line-by-line plan. `Pair` means "the same line, rewritten" and gets
/// [`intra_line_ranges`]; the other two are ordinary whole-line deletes and inserts.
enum GapOp {
    Pair(usize, usize),
    Delete(usize),
    Insert(usize),
}

/// How far [`plan_gap`] will look ahead on one side to resynchronise after the two sides fall out
/// of step, i.e. the longest run of pure insertions or deletions it can step over and still
/// recognise the rows after it as pairs.
///
/// Needed because a gap's two sides are only aligned at their ends, not throughout: a regenerated
/// `research/data/quality/optimal_solutions_benchmark.csv` gained 18 rows, and being sorted by
/// mismatch count, those rows land *among* the existing ones. Strict k-th-with-k-th pairing
/// recognised the first 33 rows and then silently gave up on all 400+ below the first inserted
/// row - the whole file downstream of it read as one block rewrite again.
///
/// Bounded rather than unbounded so the scan stays O(gap x window) instead of quadratic, and so a
/// genuinely unrelated pair of blocks can't find a spurious partner far away.
const GAP_RESYNC_WINDOW: usize = 16;

/// Decides, for one unmatched run, which before-rows are rewrites of which after-rows.
///
/// Walks both sides together. Where the current pair clears [`shared_affix`] it is a rewrite; where
/// it doesn't, the run has fallen out of step, so this looks ahead up to [`GAP_RESYNC_WINDOW`] rows
/// on each side for the nearest position that does clear it, and emits the rows stepped over as
/// plain inserts or deletes. Nearest wins, and insertions are preferred on a tie, purely so the
/// result is deterministic.
///
/// The walk is monotonic on both sides - pairs never cross - which the renderer requires: both
/// range vectors have to come out in ascending row order for `merge_ranges` and the cursor-follow
/// logic to line up.
fn plan_gap(
    before_lines: &[&str],
    after_lines: &[&str],
    before_rows: std::ops::Range<usize>,
    after_rows: std::ops::Range<usize>,
) -> Vec<GapOp> {
    let pairs_up = |b: usize, a: usize| shared_affix(before_lines[b], after_lines[a]).is_some();

    let mut plan = Vec::new();
    let (mut b, mut a) = (before_rows.start, after_rows.start);

    while b < before_rows.end && a < after_rows.end {
        if pairs_up(b, a) {
            plan.push(GapOp::Pair(b, a));
            b += 1;
            a += 1;
            continue;
        }

        let resync = (1..=GAP_RESYNC_WINDOW).find_map(|d| {
            if a + d < after_rows.end && pairs_up(b, a + d) {
                Some((d, true))
            } else if b + d < before_rows.end && pairs_up(b + d, a) {
                Some((d, false))
            } else {
                None
            }
        });

        match resync {
            Some((d, true)) => {
                plan.extend((a..a + d).map(GapOp::Insert));
                a += d;
            }
            Some((d, false)) => {
                plan.extend((b..b + d).map(GapOp::Delete));
                b += d;
            }
            // Neither side resynchronises within the window: this row really was replaced rather
            // than rewritten, so both sides advance and it renders as a delete plus an insert.
            None => {
                plan.push(GapOp::Delete(b));
                plan.push(GapOp::Insert(a));
                b += 1;
                a += 1;
            }
        }
    }

    plan.extend((b..before_rows.end).map(GapOp::Delete));
    plan.extend((a..after_rows.end).map(GapOp::Insert));
    plan
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

    /// Takes a [`SourceText`] per side rather than a `&str`, because deciding this needs to look
    /// at the text *between* two ranges, which means turning row/column positions into byte
    /// offsets - and doing that by walking the file was 90% of the corpus's worst fixture. See
    /// `SourceText`'s own doc comment for the measurement. Callers build one per side per call to
    /// `ranges`, not one per comparison.
    pub fn extends(
        &self,
        other: &RangeMatch,
        source_code: &SourceText,
        dest_code: &SourceText,
    ) -> bool {
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

/// How much of a diff to actually paint.
///
/// Not a difference in what was computed - the mapping is identical either way - but in how much
/// of it is worth showing. Both readings of the same diff are defensible, which is why the
/// human-authored ground truth records them as two separate paintings rather than one answer plus
/// a mistake (see `HumanTextMapping`, and `research/data/quality/text_painting_findings.md` for
/// the measurements this is built from).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RenderMode {
    /// Paint every changed range, brackets and separators included. Nothing is dropped, so this is
    /// exactly `TextDiff::all`.
    #[default]
    Full,
    /// Drop ranges that consist solely of structural punctuation, keeping the content that
    /// carries meaning.
    ///
    /// Measured against the corpus's hand-painted ground truth: across the ten fixtures painted
    /// both ways, `Full` carries 27 entries `Minimal` does not, and **16 of those 27 are a single
    /// punctuation token** - eight `(`, five `)`, two `):` and one `;`. So the difference between
    /// the two styles is more than half brackets, and dropping standalone punctuation is what
    /// most of the distance between them consists of.
    Minimal,
}

/// The characters [`RenderMode::Minimal`] treats as structural - brackets, separators and
/// whitespace.
///
/// **Operators are deliberately absent.** `+`, `=`, `<`, `&&` are punctuation to a tokenizer but
/// carry the entire meaning of a change to a reader: an edit from `<` to `<=` is the whole edit,
/// and a mode that hid it would be reporting a different diff rather than a tighter one. Every
/// token this actually drops was observed being dropped by hand in the painted corpus; nothing
/// here is included on the grounds that it merely looks like punctuation.
const STRUCTURAL_PUNCTUATION: &[char] = &['(', ')', '[', ']', '{', '}', ',', ';', ':'];

/// Whether `text` is nothing but structural punctuation and whitespace - i.e. whether a range
/// covering it says anything a reader needs in [`RenderMode::Minimal`].
///
/// Empty text is *not* structural: a zero-width range is an insert/delete placeholder marking a
/// position, and dropping those would remove the only mark one side has for what the other side
/// gained or lost.
pub fn is_structural_only(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_whitespace() || STRUCTURAL_PUNCTUATION.contains(&c))
}

/// One side's ranges as `mode` would paint them.
///
/// `Full` returns them unchanged. `Minimal` drops the ranges whose own text is structural only,
/// and trims whitespace off the ends of the ones that survive.
///
/// **Trimming, not just dropping.** A range that starts or ends in whitespace paints a highlight
/// running out to the indentation or off the end of the line, which reads as though the blank
/// space were part of the change. Painting by hand, nobody does that: leading and trailing
/// whitespace is never marked. Interior whitespace is left alone - it sits *between* two things
/// the range is genuinely about, and cutting the range in two there would report one edit as two.
///
/// A range is never merged, split or re-operated, only narrowed, so a range that survives still
/// describes the same edit `Full` describes - just without the padding.
pub fn ranges_for_mode(ranges: &[RangeMatch], source: &str, mode: RenderMode) -> Vec<RangeMatch> {
    if mode == RenderMode::Full {
        return ranges.to_vec();
    }
    // Built once for the whole call rather than rescanning the file per range: `range_text` walked
    // from the top of the source for every range it was asked about, which on a large file with
    // many ranges is the dominant cost of painting a frame.
    let lines: Vec<&str> = source.split('\n').collect();

    ranges
        .iter()
        .filter_map(|range_match| {
            // An `Identical` range is the unpainted background in every consumer, so whether it
            // survives changes nothing on screen - but keeping it keeps the two modes' range lists
            // structurally comparable, and `line_operations` relies on the identical ranges being
            // present to colour a row it would otherwise leave blank.
            if range_match.operation == TextOperation::Identical
                || range_match.operation == TextOperation::NotYetSet
            {
                return Some(range_match.clone());
            }
            match range_is_structural_only(&lines, &range_match.source) {
                Some(true) => return None,
                Some(false) => {}
                // A range that doesn't read back is left alone rather than silently dropped: this
                // is a display filter, and it has no business deciding that a range it could not
                // interpret is uninteresting.
                None => return Some(range_match.clone()),
            }
            let mut trimmed = range_match.clone();
            trimmed.source = trim_whitespace(&lines, &range_match.source)?;
            Some(trimmed)
        })
        .collect()
}

/// `range` with leading and trailing whitespace removed, or `None` if nothing but whitespace is
/// left (which `is_structural_only` will already have caught for any caller that checks it first).
///
/// Only the `source` side is narrowed. The `destination` is a position in the *other* file, whose
/// text this function cannot see, and each side's ranges are filtered independently against their
/// own source - so trimming here and there happens separately and correctly, while `destination`
/// keeps pointing at the untrimmed counterpart region that cross-panel navigation jumps to.
fn trim_whitespace(lines: &[&str], range: &TextRange) -> Option<TextRange> {
    let (mut start_row, mut start_column) = (range.start_row, range.start_column);
    let (mut end_row, mut end_column) = (range.end_row, range.end_column);

    // Forward from the start, over whitespace, wrapping to the next row at end of line.
    loop {
        if (start_row, start_column) >= (end_row, end_column) {
            return None;
        }
        let line = *lines.get(start_row)?;
        match line[start_column.min(line.len())..].chars().next() {
            Some(c) if c.is_whitespace() => start_column += c.len_utf8(),
            // Past the last character of this row: the newline itself is whitespace, so step over
            // it to the start of the next row.
            None => {
                start_row += 1;
                start_column = 0;
            }
            Some(_) => break,
        }
    }
    // Backward from the end, symmetrically.
    loop {
        if (start_row, start_column) >= (end_row, end_column) {
            return None;
        }
        let line = *lines.get(end_row)?;
        let column = end_column.min(line.len());
        match line[..column].chars().next_back() {
            Some(c) if c.is_whitespace() => end_column = column - c.len_utf8(),
            // At column 0 the previous character is the previous row's newline.
            None => {
                end_row = end_row.checked_sub(1)?;
                end_column = lines.get(end_row)?.len();
            }
            Some(_) => break,
        }
    }
    Some(TextRange::new(start_row, start_column, end_row, end_column))
}

/// Whether every character `range` covers is structural punctuation or whitespace, given the
/// source already split into rows. `None` if the range falls outside the source.
///
/// Walks the rows rather than materializing the covered text: a multi-row range's text would have
/// to be joined into a fresh `String` just to be scanned once and dropped, and the covered rows
/// are exactly what needs checking either way.
fn range_is_structural_only(lines: &[&str], range: &TextRange) -> Option<bool> {
    if range.start_row > range.end_row {
        return None;
    }
    let mut saw_any = false;
    for row in range.start_row..=range.end_row {
        let line = *lines.get(row)?;
        let start = if row == range.start_row {
            range.start_column
        } else {
            0
        };
        let end = if row == range.end_row {
            range.end_column
        } else {
            line.len()
        };
        if start > line.len() || end > line.len() || start > end {
            return None;
        }
        let covered = line.get(start..end)?;
        if !covered.is_empty() {
            saw_any = true;
            if !is_structural_only(covered) {
                return Some(false);
            }
        }
        // The newline joining this row to the next is itself whitespace, so a multi-row range that
        // is blank on every row stays structural-only.
        if row < range.end_row {
            saw_any = true;
        }
    }
    // An empty range covers nothing, and `is_structural_only` deliberately says a zero-width
    // placeholder is not structural - it marks a position the other side changed.
    Some(saw_any)
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
    pub moves: usize,
}

/// Counts each side's own [`line_operations`] output independently: `Insert` from the after side
/// (a line that exists only in after), `Delete` from the before side (a line that exists only in
/// before). `Update` is counted once, from the after side only - an updated line exists on both
/// sides at the same row, so counting it from both sides would double it. `Move` is counted from
/// the after side only for the same reason - a moved line exists on both sides (at different
/// rows), so its destination is the single representative.
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
        moves: after_ops
            .iter()
            .filter(|op| **op == TextOperation::Move)
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
    /// `TextOperation::Move` fires when a matched node's own *column* shifts (a re-indent), or
    /// when a multi-row matched node's destination lands *before* the last sequential anchor (a
    /// sibling reorder - see `ranges`'s `crossed_backwards`). A same-column row shift caused by
    /// unrelated edits elsewhere in the file is deliberately not a `Move`. Before the
    /// `crossed_backwards` check existed, a pure reorder of two top-level functions produced no
    /// operations at all - the diff rendered as completely unchanged and this variant never fired
    /// for the very case it names.
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

    fn text_range_on(row: usize, start: usize, end: usize) -> TextRange {
        TextRange::new(row, start, row, end)
    }

    fn changed(row: usize, start: usize, end: usize) -> RangeMatch {
        RangeMatch {
            source: text_range_on(row, start, end),
            destination: text_range_on(row, start, end),
            operation: TextOperation::Update,
        }
    }

    /// The exact tokens the painted corpus showed `Full` adding over `Minimal` - eight `(`, five
    /// `)`, two `):` and one `;` across ten fixtures. If `Minimal` does not drop these, it is not
    /// modelling the style it is named after.
    #[test]
    fn minimal_drops_the_punctuation_the_painted_corpus_drops() {
        for text in ["(", ")", "):", ";", "{", "}", "[]", "  ", " ,\n", "( )"] {
            assert!(
                is_structural_only(text),
                "{text:?} should be structural-only"
            );
        }
    }

    /// Operators look like punctuation and are the entire content of the change they appear in.
    /// Dropping them would report a different diff, not a tighter one.
    #[test]
    fn minimal_keeps_operators_and_anything_carrying_meaning() {
        for text in ["+", "=", "<=", "&&", "=>", "foo", "1", "_", "a,", "->"] {
            assert!(!is_structural_only(text), "{text:?} should survive Minimal");
        }
    }

    /// A zero-width range marks where the other side gained or lost text - the only mark that side
    /// has. Treating "no characters" as "only structural characters" would erase it.
    #[test]
    fn an_empty_range_is_not_structural() {
        assert!(!is_structural_only(""));
    }

    #[test]
    fn full_returns_every_range_unchanged() {
        let source = "foo(bar);\n";
        let ranges = vec![changed(0, 3, 4), changed(0, 4, 7)];

        assert_eq!(
            ranges_for_mode(&ranges, source, RenderMode::Full),
            ranges,
            "Full is exactly TextDiff::all"
        );
    }

    #[test]
    fn minimal_drops_a_standalone_bracket_but_keeps_its_neighbour() {
        let source = "foo(bar);\n";
        // `(` alone, then `bar`.
        let ranges = vec![changed(0, 3, 4), changed(0, 4, 7)];

        let minimal = ranges_for_mode(&ranges, source, RenderMode::Minimal);

        assert_eq!(minimal.len(), 1, "got {minimal:?}");
        assert_eq!(minimal[0].source, text_range_on(0, 4, 7));
    }

    /// A bracket *inside* a larger range is not a standalone bracket - only whole ranges are
    /// dropped, never parts of one, so a surviving range is byte-for-byte what `Full` shows.
    #[test]
    fn minimal_keeps_a_range_that_merely_contains_punctuation() {
        let source = "foo(bar);\n";
        let ranges = vec![changed(0, 0, 9)];

        assert_eq!(
            ranges_for_mode(&ranges, source, RenderMode::Minimal),
            ranges
        );
    }

    /// `Identical` ranges are the unpainted background, so dropping them changes nothing on
    /// screen - but `line_operations` reads them to colour rows, and the two modes stay
    /// comparable only if their range lists differ by content ranges alone.
    #[test]
    fn minimal_keeps_identical_ranges_even_when_they_are_pure_punctuation() {
        let source = "foo(bar);\n";
        let identical = RangeMatch {
            source: text_range_on(0, 3, 4),
            destination: text_range_on(0, 3, 4),
            operation: TextOperation::Identical,
        };

        assert_eq!(
            ranges_for_mode(
                std::slice::from_ref(&identical),
                source,
                RenderMode::Minimal
            ),
            vec![identical]
        );
    }

    /// Painting by hand, nobody marks the indentation in front of a change or the blank running
    /// off the end of the line - a highlight that includes them reads as though the blank space
    /// were part of the edit.
    #[test]
    fn minimal_trims_leading_and_trailing_whitespace_off_a_range() {
        let source = "    foo   \n";
        let ranges = vec![changed(0, 0, 10)];

        let minimal = ranges_for_mode(&ranges, source, RenderMode::Minimal);

        assert_eq!(minimal.len(), 1);
        assert_eq!(
            minimal[0].source,
            text_range_on(0, 4, 7),
            "the range should cover exactly `foo`"
        );
    }

    /// Interior whitespace sits *between* two things the range is genuinely about; cutting there
    /// would report one edit as two.
    #[test]
    fn minimal_keeps_whitespace_inside_a_range() {
        let source = "a   b\n";
        let ranges = vec![changed(0, 0, 5)];

        assert_eq!(
            ranges_for_mode(&ranges, source, RenderMode::Minimal)[0].source,
            text_range_on(0, 0, 5)
        );
    }

    /// `Full` is untouched by any of this - it is exactly `TextDiff::all`.
    #[test]
    fn full_does_not_trim() {
        let source = "    foo   \n";
        let ranges = vec![changed(0, 0, 10)];

        assert_eq!(ranges_for_mode(&ranges, source, RenderMode::Full), ranges);
    }

    /// Trimming narrows the source only. The destination is a position in the *other* file, whose
    /// text this side cannot see, and cross-panel navigation jumps to it.
    #[test]
    fn trimming_leaves_the_destination_alone() {
        let source = "  foo  \n";
        let ranges = vec![RangeMatch {
            source: text_range_on(0, 0, 7),
            destination: text_range_on(3, 0, 7),
            operation: TextOperation::Update,
        }];

        let minimal = ranges_for_mode(&ranges, source, RenderMode::Minimal);

        assert_eq!(minimal[0].source, text_range_on(0, 2, 5));
        assert_eq!(minimal[0].destination, text_range_on(3, 0, 7));
    }

    /// A multi-row range must not be judged by its first row alone: whitespace there says nothing
    /// about the rows below it.
    #[test]
    fn a_multi_row_range_with_content_below_a_blank_first_row_survives() {
        let source = "   \n  keep\n";
        let ranges = vec![RangeMatch {
            source: TextRange::new(0, 0, 1, 6),
            destination: TextRange::new(0, 0, 1, 6),
            operation: TextOperation::Update,
        }];

        let minimal = ranges_for_mode(&ranges, source, RenderMode::Minimal);

        assert_eq!(minimal.len(), 1, "got {minimal:?}");
        assert_eq!(
            minimal[0].source,
            TextRange::new(1, 2, 1, 6),
            "and it should trim down to `keep`"
        );
    }

    /// A range that is nothing but blank rows is still dropped.
    #[test]
    fn a_multi_row_range_of_only_whitespace_is_dropped() {
        let source = "   \n   \n";
        let ranges = vec![RangeMatch {
            source: TextRange::new(0, 0, 1, 3),
            destination: TextRange::new(0, 0, 1, 3),
            operation: TextOperation::Update,
        }];

        assert!(ranges_for_mode(&ranges, source, RenderMode::Minimal).is_empty());
    }

    /// A range that doesn't read back is left alone. This is a display filter; deciding that
    /// something it could not interpret is uninteresting is not its call to make.
    #[test]
    fn minimal_keeps_a_range_it_cannot_read() {
        let source = "short\n";
        let ranges = vec![changed(99, 0, 4)];

        assert_eq!(
            ranges_for_mode(&ranges, source, RenderMode::Minimal),
            ranges
        );
    }

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

    /// Two lines that share no common prefix or suffix at all are not a rewrite of each other in
    /// any useful sense, so they stay an adjacent delete+insert pair, same as a plain `diff -u`.
    /// The similar-lines case is `plain_text_line_diff_narrows_a_changed_line_to_the_changed_part`
    /// below.
    #[test]
    fn plain_text_line_diff_treats_a_dissimilar_changed_line_as_delete_plus_insert() {
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

    /// Every range on one row, as `(operation, start_column, end_column)` - byte columns, this
    /// module's convention (see `text_range::SourceText::byte_index`). The assertion shape the
    /// intra-line tests below need, where `changed_row_spans` deliberately drops columns.
    fn row_column_spans(ranges: &[RangeMatch], row: usize) -> Vec<(TextOperation, usize, usize)> {
        ranges
            .iter()
            .filter(|r| r.source.start_row == row && r.source.end_row == row)
            .map(|r| {
                (
                    r.operation.clone(),
                    r.source.start_column,
                    r.source.end_column,
                )
            })
            .collect()
    }

    /// The point of the whole intra-line path: a wide line whose one field changed highlights that
    /// field, not the row. Modeled on a regenerated CSV row (the case that motivated this - see
    /// `MIN_SHARED_AFFIX_PERCENT`), where only a timing column differs.
    #[test]
    fn plain_text_line_diff_narrows_a_changed_line_to_the_changed_part() {
        let before = "h\nname,alpha,beta,gamma,delta,37.282,epsilon,zeta\n";
        let after = "h\nname,alpha,beta,gamma,delta,37.860,epsilon,zeta\n";
        let (before_ranges, after_ranges) = plain_text_line_diff(before, after);

        // "37." is common prefix, "epsilon,zeta" common suffix; only "282"/"860" differ.
        let prefix_end = "name,alpha,beta,gamma,delta,37.".len();
        let middle_end = prefix_end + "282".len();
        assert_eq!(
            row_column_spans(&before_ranges, 1),
            vec![
                (TextOperation::Identical, 0, prefix_end),
                (TextOperation::Update, prefix_end, middle_end),
                (
                    TextOperation::Identical,
                    middle_end,
                    "name,alpha,beta,gamma,delta,37.282,epsilon,zeta".len()
                ),
            ]
        );
        assert_eq!(
            row_column_spans(&after_ranges, 1).len(),
            3,
            "both sides must produce the same number of sub-ranges - see intra_line_ranges"
        );
    }

    /// A pure block insert must keep reading as one change, not become one range per line. This is
    /// the regression the gap-level gate in `emit_gap` exists for: if it ever leaks, `n`/`p`
    /// navigation degrades for every grammar-less file.
    #[test]
    fn plain_text_line_diff_keeps_a_block_insert_merged() {
        let before = "same\n";
        let after = "same\nwholly different one\nwholly different two\nwholly different three\n";
        let (_, after_ranges) = plain_text_line_diff(before, after);
        assert_eq!(
            changed_row_spans(&after_ranges),
            vec![(TextOperation::Insert, 1, 4)],
            "three unrelated inserted lines must stay one merged range"
        );
    }

    /// Rows inserted *among* changed rows knock the two sides out of step. Without the bounded
    /// resynchronisation in `plan_gap` the first mismatch ends refinement for the entire rest of
    /// the run - which is exactly what happened on the CSV that motivated this (33 rows narrowed,
    /// 400+ below the first inserted row fell back to whole-line).
    #[test]
    fn plain_text_line_diff_resynchronises_after_an_inserted_line() {
        let before = "row-a,1\nrow-b,1\nrow-c,1\n";
        let after = "row-a,2\nBRAND NEW UNRELATED LINE\nrow-b,2\nrow-c,2\n";
        let (before_ranges, after_ranges) = plain_text_line_diff(before, after);

        for (before_row, after_row) in [(0usize, 0usize), (1, 2), (2, 3)] {
            assert_eq!(
                row_column_spans(&before_ranges, before_row),
                vec![
                    (TextOperation::Identical, 0, "row-a,".len()),
                    (TextOperation::Update, "row-a,".len(), "row-a,1".len()),
                ],
                "before row {before_row} should narrow to its trailing digit"
            );
            assert_eq!(
                row_column_spans(&after_ranges, after_row).len(),
                2,
                "after row {after_row} must stay symmetric with its partner"
            );
        }
        assert!(
            changed_row_spans(&after_ranges).contains(&(TextOperation::Insert, 1, 2)),
            "the genuinely new line must still read as an insert: {:?}",
            changed_row_spans(&after_ranges)
        );
    }

    /// Byte columns, not character counts. A multi-byte character before the changed part shifts
    /// every later byte offset, and getting this wrong slices mid-character downstream - the crash
    /// `text_range::row_col_to_byte_index`'s doc comment records.
    #[test]
    fn plain_text_line_diff_intra_line_columns_are_byte_offsets() {
        let before = "x\nprefix — value 1 suffix\n";
        let after = "x\nprefix — value 2 suffix\n";
        let (before_ranges, _) = plain_text_line_diff(before, after);

        let prefix_end = "prefix — value ".len(); // 17 bytes, 15 chars - the em dash is 3 bytes
        assert_eq!(
            row_column_spans(&before_ranges, 1),
            vec![
                (TextOperation::Identical, 0, prefix_end),
                (TextOperation::Update, prefix_end, prefix_end + 1),
                (
                    TextOperation::Identical,
                    prefix_end + 1,
                    "prefix — value 1 suffix".len()
                ),
            ]
        );
        assert_eq!(
            prefix_end, 17,
            "sanity: the em dash must count as 3 bytes, not 1 char"
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

    #[test]
    fn whole_file_class_identical_when_no_lines_changed() {
        assert_eq!(
            whole_file_text_class("a\nb\nc\n", "a\nb\nc\n"),
            WholeFileClass::Identical
        );
    }

    #[test]
    fn whole_file_class_insert_only_when_nothing_deleted() {
        assert_eq!(
            whole_file_text_class("a\nc\n", "a\nb\nc\n"),
            WholeFileClass::InsertOnly
        );
    }

    #[test]
    fn whole_file_class_delete_only_when_nothing_inserted() {
        assert_eq!(
            whole_file_text_class("a\nb\nc\n", "a\nc\n"),
            WholeFileClass::DeleteOnly
        );
    }

    #[test]
    fn whole_file_class_mixed_when_both_inserted_and_deleted() {
        assert_eq!(
            whole_file_text_class("a\nb\nc\n", "a\nx\nc\n"),
            WholeFileClass::Mixed,
            "a changed line is a delete+insert pair, not an Update - so it's Mixed, not licensed"
        );
    }

    #[test]
    fn whole_file_class_mixed_when_myers_lcs_gives_up() {
        const SMALL_CAP: usize = 20;
        let before: String = (0..SMALL_CAP + 10)
            .map(|i| format!("before-unique-line-{i}\n"))
            .collect();
        let after: String = (0..SMALL_CAP + 10)
            .map(|i| format!("after-unique-line-{i}\n"))
            .collect();
        let class = line_diff_core(&before, &after, SMALL_CAP)
            .map(|core| core.whole_file_class())
            .unwrap_or(WholeFileClass::Mixed);
        assert_eq!(
            class,
            WholeFileClass::Mixed,
            "a give-up must never be reported as a licensable class - no license should be \
             granted from an edit distance too large to have actually been measured"
        );
    }

    /// Cross-checks `whole_file_text_class` (Myers LCS, this module's own algorithm) against an
    /// independently computed classification (Python `difflib.SequenceMatcher`, `autojunk=False`)
    /// over the same 338-fixture corpus used for Phase 0's hunk-level census (`TODO.md`'s
    /// "Phase 0 findings" section) - `src/test/data/whole_file_text_classification_census.csv`.
    /// This is the load-bearing primitive Phase 3a's dispatcher licenses a delete-free/insert-free
    /// resolver from, so a wiring bug here (not just a logic bug within this module) needs an
    /// external ground truth to catch, per the phases-4-7 rearchitecture plan's Phase 3a doc
    /// comment ("give this newly load-bearing primitive focused test coverage beyond its existing
    /// viz-oriented tests").
    #[test]
    #[ignore = "slow"]
    fn whole_file_text_class_matches_independent_census() -> Result<()> {
        let census_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("test")
            .join("data")
            .join("whole_file_text_classification_census.csv");
        let census_csv = std::fs::read_to_string(&census_path)?;

        let pairs = test::helper::handmade_test_code_pairs()?;
        let mut mismatches = Vec::new();
        let mut checked = 0;
        for line in census_csv.lines().skip(1) {
            let (fixture, expected_str) = line
                .split_once(',')
                .expect("census CSV row must be `fixture,classification`");
            let expected = match expected_str {
                "Identical" => WholeFileClass::Identical,
                "InsertOnly" => WholeFileClass::InsertOnly,
                "DeleteOnly" => WholeFileClass::DeleteOnly,
                "Mixed" => WholeFileClass::Mixed,
                other => panic!("unknown census classification `{other}` for `{fixture}`"),
            };
            let Some((before, after)) = pairs.get(fixture) else {
                continue;
            };
            checked += 1;
            let actual = whole_file_text_class(&before.contents, &after.contents);
            if actual != expected {
                mismatches.push(format!("{fixture}: census={expected:?} rust={actual:?}"));
            }
        }

        assert!(
            checked > 300,
            "expected to check the vast majority of the 338-fixture corpus, only checked {checked}"
        );
        assert!(
            mismatches.is_empty(),
            "{} whole-file classification mismatches vs the independent census:\n{}",
            mismatches.len(),
            mismatches.join("\n")
        );
        Ok(())
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
                moves: 0,
            }
        );
    }

    /// `Move` is counted from the after side only, mirroring `Update`'s single-count rule - a
    /// moved line exists on both sides at different rows.
    #[test]
    fn change_counts_tallies_moves_once_from_the_after_side() {
        let move_range = |row: usize, dest_row: usize| RangeMatch {
            source: TextRange::new(row, 0, row, 1),
            destination: TextRange::new(dest_row, 0, dest_row, 1),
            operation: TextOperation::Move,
        };
        let before_ranges = vec![move_range(0, 2)];
        let after_ranges = vec![move_range(2, 0)];

        let counts = change_counts("a\nb\nc", "b\nc\na", &before_ranges, &after_ranges);
        assert_eq!(
            counts,
            ChangeCounts {
                insertions: 0,
                deletions: 0,
                updates: 0,
                moves: 1,
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

    /// Was `#[ignore]`d during the `phases-4-7-rearchitecture` branch's Phase 1 (see `TODO.md`):
    /// `python-added-if-block` briefly had 5 mismatches (was 0) from replacing whole-residual full
    /// APTED with the cheaper Myers-LCS fallback. Passes again as of the `maximal_unmatched_roots`
    /// traversal fix (`TODO.md`'s "Bug fix" entry) - un-ignored.
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

    /// Regression test for the `crossed_backwards` check in `ranges`: before it, a pure reorder
    /// of two sibling functions (same column, different rows) produced no non-Identical range at
    /// all - the diff rendered as completely unchanged and `summarize_diff` returned `None`,
    /// silence for two byte-different files (found via `codediff --headless` smoke test,
    /// 2026-08-19).
    #[test]
    fn sibling_reorder_produces_move_ranges_and_a_refactor_moved_summary() {
        let before_src = "fn main() {\n    let a = 1;\n    println!(\"{}\", a);\n}\n\nfn helper(x: i32) -> i32 {\n    x * 2\n}\n";
        let after_src = "fn helper(x: i32) -> i32 {\n    x * 2\n}\n\nfn main() {\n    let a = 1;\n    println!(\"{}\", a);\n}\n";
        let (before, after, ast, node_cache) = diff_ast(before_src, after_src);
        let text_diff = TextDiff::from(&before, &after, &ast, &node_cache);
        let before_ranges = text_diff.all(0);
        let after_ranges = text_diff.all(1);

        for (side, ranges) in [("before", &before_ranges), ("after", &after_ranges)] {
            assert!(
                ranges.iter().any(|r| r.operation == TextOperation::Move),
                "{side} side should contain a Move range for a sibling reorder, got {ranges:?}"
            );
            assert!(
                ranges
                    .iter()
                    .all(|r| matches!(r.operation, TextOperation::Move | TextOperation::Identical)),
                "{side} side of a pure reorder should only have Move/Identical ranges, got {ranges:?}"
            );
        }
        assert_eq!(
            summarize_diff(before_src, after_src, &before_ranges, &after_ranges),
            Some(DiffSummary::RefactorMovedOnly)
        );
    }

    /// The other half of the `crossed_backwards` contract: content shifted *down* by an unrelated
    /// insertion above it keeps its column and its relative order, so it must stay `Identical` -
    /// flagging everything below an inserted line as "moved" would be noise.
    #[test]
    fn unrelated_insertion_does_not_flag_shifted_content_as_moved() {
        let before_src = "fn main() {\n    foo();\n}\n\nfn helper() {\n    bar();\n}\n";
        let after_src =
            "fn added() {}\n\nfn main() {\n    foo();\n}\n\nfn helper() {\n    bar();\n}\n";
        let (before, after, ast, node_cache) = diff_ast(before_src, after_src);
        let text_diff = TextDiff::from(&before, &after, &ast, &node_cache);
        for (side, ranges) in [("before", &text_diff.all(0)), ("after", &text_diff.all(1))] {
            assert!(
                !ranges.iter().any(|r| r.operation == TextOperation::Move),
                "{side} side should have no Move ranges when content merely shifted down, got {ranges:?}"
            );
        }
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

    /// A single-character edit inside a 20-character identifier ("long_identifier_**n**ame" ->
    /// "long_identifier_**n**ome": common prefix "long_identifier_n", common suffix "me", one
    /// changed character in between) must produce exactly one narrow `Update` range - not one
    /// `Update` spanning the whole identifier, which is the bug this feature fixes.
    #[test]
    fn ranges_decomposes_a_small_change_inside_a_long_identifier() {
        let (before, after, ast, node_cache) = diff_ast(
            "fn main() {\n    let long_identifier_name = 5;\n}",
            "fn main() {\n    let long_identifier_nome = 5;\n}",
        );
        let before_ranges = ranges(&before, &after, &ast, &node_cache);

        let updates: Vec<_> = before_ranges
            .iter()
            .filter(|r| r.operation == TextOperation::Update)
            .collect();
        assert_eq!(
            updates.len(),
            1,
            "expected exactly one Update sub-range, got {before_ranges:?}"
        );
        let update = updates[0];
        assert_eq!(update.source.start_row, 1);
        assert_eq!(update.source.end_row, 1);
        assert_eq!(
            update.source.end_column - update.source.start_column,
            1,
            "the Update range should cover only the single changed character, not the whole \
             20-character identifier"
        );

        let after_ranges = ranges(&after, &before, &ast, &node_cache);
        let after_updates: Vec<_> = after_ranges
            .iter()
            .filter(|r| r.operation == TextOperation::Update)
            .collect();
        assert_eq!(after_updates.len(), 1);
        assert_eq!(
            after_updates[0].source.end_column - after_updates[0].source.start_column,
            1,
            "the after->before direction must independently find the same narrow width"
        );
    }

    /// When the two texts share no common prefix or suffix at all, there's nothing more precise
    /// to report than the whole span - same as the pre-existing whole-node behavior.
    #[test]
    fn ranges_falls_back_to_a_whole_span_update_when_there_is_no_common_affix() {
        let (before, after, ast, node_cache) = diff_ast(
            "fn main() {\n    let foo = 5;\n}",
            "fn main() {\n    let bar = 5;\n}",
        );
        let before_ranges = ranges(&before, &after, &ast, &node_cache);

        let updates: Vec<_> = before_ranges
            .iter()
            .filter(|r| r.operation == TextOperation::Update)
            .collect();
        assert_eq!(updates.len(), 1);
        assert_eq!(
            updates[0].source.end_column - updates[0].source.start_column,
            3,
            "\"foo\" and \"bar\" share no common prefix/suffix, so the whole 3-character \
             identifier should be reported as changed"
        );
    }

    /// A comment's own content sits in a single gap after its `//` marker child
    /// (`own_content_span` succeeds), so a localized change inside a comment should decompose the
    /// same way a leaf identifier does, not report the whole comment as changed.
    #[test]
    fn ranges_decomposes_a_small_change_inside_a_comment() {
        let (before, after, ast, node_cache) = diff_ast(
            "// hello world!\nfn main() {}",
            "// hello universe!\nfn main() {}",
        );
        let before_ranges = ranges(&before, &after, &ast, &node_cache);

        let updates: Vec<_> = before_ranges
            .iter()
            .filter(|r| r.operation == TextOperation::Update)
            .collect();
        assert_eq!(
            updates.len(),
            1,
            "expected exactly one Update sub-range inside the comment, got {before_ranges:?}"
        );
        assert_eq!(
            updates[0].source.end_column - updates[0].source.start_column,
            5,
            "\"world\" (5 chars) should be the only part marked changed, not the whole comment"
        );

        let identical_in_comment: Vec<_> = before_ranges
            .iter()
            .filter(|r| {
                r.operation == TextOperation::Identical
                    && r.source.start_row == 0
                    && r.source.end_row == 0
            })
            .collect();
        assert!(
            !identical_in_comment.is_empty(),
            "the common \"// hello \" prefix should be reported as Identical, not swallowed into \
             the Update: {before_ranges:?}"
        );
    }

    /// Regression guard for the risk that motivated bypassing the range-merging accumulator for
    /// decomposed nodes (see `ranges`'s own comment on why): an unrelated insertion earlier in the
    /// file shifts the changed identifier to a different column on each side, which is exactly the
    /// kind of before/after asymmetry that could make accumulator-based merging diverge between
    /// the two independently-computed range lists. `TextDiff::from` (which calls `merge_ranges`)
    /// must not panic or misalign, and the narrow Update must still be found on both sides.
    #[test]
    fn ranges_decomposition_survives_an_unrelated_earlier_insertion() {
        let (before, after, ast, node_cache) = diff_ast(
            "fn main() {\n    let short = 1;\n    let long_identifier_name = 5;\n}",
            "fn main() {\n    let inserted_line = 0;\n    let short = 1;\n    \
             let long_identifier_nome = 5;\n}",
        );

        let text_diff = TextDiff::from(&before, &after, &ast, &node_cache);
        let before_ranges = text_diff.all(0);
        let after_ranges = text_diff.all(1);

        let before_updates: Vec<_> = before_ranges
            .iter()
            .filter(|r| r.operation == TextOperation::Update)
            .collect();
        let after_updates: Vec<_> = after_ranges
            .iter()
            .filter(|r| r.operation == TextOperation::Update)
            .collect();
        assert_eq!(before_updates.len(), 1, "{before_ranges:?}");
        assert_eq!(after_updates.len(), 1, "{after_ranges:?}");
        assert_eq!(
            before_updates[0].source.end_column - before_updates[0].source.start_column,
            1
        );
        assert_eq!(
            after_updates[0].source.end_column - after_updates[0].source.start_column,
            1
        );
    }
}
