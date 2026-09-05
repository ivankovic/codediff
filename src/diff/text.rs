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
    code::{Code, metadata::compute_row_byte_lengths},
    diff::{
        ASTDiff, ASTMappingOperation, ASTMappingReason, NodeCache, nodes,
        text_range::{SourceText, TextRange},
    },
};

mod plain_text_diff;
mod render_options;
mod summary;

pub use plain_text_diff::{
    LineDiffCore, WholeFileClass, line_diff_core, plain_text_line_diff, whole_file_text_class,
};
pub use render_options::{
    RangeMatch, RenderOptions, TextOperation, is_structural_only, ranges_for_options,
};
pub use summary::{
    ChangeCounts, DiffSummary, change_counts, is_comment_only_diff, line_operations,
    summarize_diff, summarize_diff_with_comment_check,
};

// `ranges` (below) is the one piece of this file's own remaining code that reaches into a
// submodule for something not already part of the public API above.
use summary::whitespace_stripped_equal;

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
/// common Identical prefix, the differing middle, and a common Identical suffix - instead of
/// reporting the node's entire text as changed. This is what lets a small edit inside a long
/// string, comment, or identifier highlight only the part that actually changed.
///
/// **The middle is not always an `Update`.** When one side's middle is empty, nothing was
/// replaced: text was purely added or purely removed, and calling that an update paints an
/// insertion yellow. `"""Fetch user data from API"""` becoming
/// `"""Fetch user data from API with improved error handling"""` has an empty before-middle, and
/// every human painting of that shape in the corpus calls the added words an insert. So the middle
/// takes its operation from which side actually holds text, which is why `source_is_before` has to
/// reach here: an insertion is an `Insert` on the after side and a zero-width "added here" marker
/// on the before side, and the two texts alone cannot tell those apart.
///
/// **Content nodes only**, and the corpus is unanimous about why. Applying the insert/delete
/// reading everywhere improves three fixtures and worsens three, and the losing three are all the
/// same shape: `IntBox` -> `Box` (`cpp-add-templates`) and `<=` -> `<` (`cpp-fix-segfault`,
/// `java-fix-array-index`), where the painter called the whole identifier or operator *updated*
/// rather than calling the dropped affix a deletion. That is the right reading: `IntBox` -> `Box`
/// is a rename, not the deletion of an `Int`. The winning three are all string literals and
/// docstrings gaining a phrase, where the added words genuinely are an insertion. So the split is
/// by what the node's text *is* - content a reader reads, versus a name or an operator - not by
/// any threshold on how much of it changed.
///
/// Only ever one middle, deliberately. A node whose own text contained two separate edits would
/// need a real sequence diff inside it; measured against the painted corpus on 2026-08-28, **0 of
/// 31** `Update` node extents carry more than one separately painted run, so that generality has no
/// customer and the affix split is the whole of what the ground truth asks for.
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
/// Symmetric in shape, mirrored in operation. `common_prefix_byte_len`/`common_suffix_byte_len`
/// only compare characters pairwise for equality, which doesn't depend on which string is "source"
/// and which is "destination" - so calling this with the two texts swapped (as `ranges` does, once
/// for before->after and once for after->before) always produces the same number of sub-ranges, in
/// the same order. The *operations* are mirrored rather than identical, and have to be: an `Insert`
/// on the after side states the same fact as a zero-width "added here" marker on the before side.
/// Only the middle can differ that way - prefix and suffix are `Identical` from both directions. `ranges`'s caller is responsible for the other half of
/// this guarantee: pushing a multi-range result straight into `ranges` rather than through the
/// usual same-operation-neighbor-merging accumulator, since that merging depends on each side's own
/// (possibly different) surrounding text and could otherwise make the two sides' sub-range counts
/// diverge after accumulation even though this function itself is symmetric.
/// Whether a node's own text is content a reader reads, rather than a name or grammar glue.
///
/// Literals and comments are content: adding a phrase to a docstring is an insertion. Identifiers,
/// keywords and operators are not: `IntBox` becoming `Box` is a rename, and `<=` becoming `<` is a
/// changed operator - in both, the painter calls the node updated rather than calling the dropped
/// characters a deletion. See `intra_node_update_ranges` (the original caller, carrying the six
/// fixtures this was measured against) and `ranges`'s own `Insert`/`Delete`-with-children arm
/// (added later, for the same content-vs-glue split on a *whole* new/removed node rather than a
/// changed one).
fn is_content_node(kind: &str) -> bool {
    // `nodes::is_literal_kind` is not enough on its own and deliberately not widened here: it is
    // shared with the APTED rename-cost model and `code::hash`, and it lists only the kinds those
    // need (`string_literal`, `integer_literal`, ...). Python spells its docstrings `string` and
    // HTML its content `raw_text`, so the substring tests below carry the cases this decision
    // actually turns on. Kept local for that reason - widening the shared list to serve a
    // rendering choice would change matching and hashing too.
    nodes::is_literal_kind(kind)
        || nodes::is_comment(kind)
        || kind.contains("string")
        || kind.contains("comment")
        || kind.contains("raw_text")
}

fn intra_node_update_ranges(
    last_non_move_range: &mut TextRange,
    whole_source_range: TextRange,
    source: TextSpan,
    destination: TextSpan,
    source_is_before: bool,
    // Whether the node's text is content a reader reads - a literal or a comment - as opposed to
    // an identifier, keyword or operator. Decides whether a one-sided middle reads as an
    // insertion/deletion or stays an update.
    content_node: bool,
    // [`RenderOptions::whole_pair_updates`]. `true` skips the affix split below entirely and
    // reports the node's whole extent as one `Update`, the same single-range shape the "no common
    // affix at all" case below already produces - painting the matched pair whole is exactly that
    // case, forced rather than discovered.
    whole_pair_updates: bool,
) -> Vec<RangeMatch> {
    let prefix_len = common_prefix_byte_len(source.text, destination.text);
    let suffix_len =
        common_suffix_byte_len(&source.text[prefix_len..], &destination.text[prefix_len..]);

    if whole_pair_updates || (prefix_len == 0 && suffix_len == 0) {
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
    // `source` is the side being painted, so "no text here, text there" reads as an insertion from
    // the before side and as a deletion from the after side. Only for content nodes - see this
    // function's doc comment for the six fixtures that draw that line.
    let middle_operation = match (source_mid_len, destination_mid_len, source_is_before) {
        _ if !content_node => TextOperation::Update,
        (0, _, true) | (_, 0, false) => TextOperation::Insert,
        (0, _, false) | (_, 0, true) => TextOperation::Delete,
        _ => TextOperation::Update,
    };
    if source_mid_len > 0 || destination_mid_len > 0 {
        let source_mid_start = point_at_byte_offset(source.text, source.start, prefix_len);
        let source_mid_end =
            point_at_byte_offset(source.text, source.start, source.text.len() - suffix_len);
        result.push(advance_and_build_range_with_source(
            last_non_move_range,
            text_range_from_points(source_mid_start, source_mid_end, source.columns),
            middle_operation,
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
    // Which file `source` is. Only `intra_node_update_ranges` needs it, and only to tell an
    // insertion from a deletion - see its doc comment. Everything else in here is symmetric.
    source_is_before: bool,
    // [`RenderOptions::whole_pair_updates`], forwarded straight through to every
    // `intra_node_update_ranges` call below - see that parameter's own doc comment.
    whole_pair_updates: bool,
    // [`RenderOptions::paint_reindent_only_moves`] - see that field's own doc comment and
    // `known_pure_reindent` below.
    paint_reindent_only_moves: bool,
) -> Vec<RangeMatch> {
    let mut ranges = Vec::new();

    // Compute columns per row for source and destination
    let source_columns = compute_row_byte_lengths(&source.contents);
    let destination_columns = compute_row_byte_lengths(&destination.contents);
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
                                // A single-row node pushed sideways by an edit *elsewhere on its
                                // own row* is not a Move either: `void process(int x)` ->
                                // `void process(const int x)` shifts `int x` by six columns, and
                                // the human paints only the inserted `const`. The test is that
                                // the node lies wholly inside the common prefix or common suffix
                                // of its source row and destination row - its own text is
                                // untouched and the row's one edit is beside it. A node inside
                                // the *rewritten* part of the row keeps the Move treatment:
                                // `function fetchData(callback: ...): void {` ->
                                // `async function fetchData(): Promise<string> {` shifts
                                // `function fetchData(` the same way, but the row was rewritten
                                // around it and its painter calls the surviving fragments moved
                                // (`typescript-async-await`, which a plain same-row-same-indent
                                // rule regressed 27.5% -> 36.1% on 2026-09-01, the fourth attempt
                                // at this shape; the three before it are in the fix log).
                                // Restricted to nodes that stayed on their own row and were not
                                // reindented, so a reindent-only move keeps reaching
                                // `paint_reindent_only_moves` below.
                                let shifted_by_an_edit_beside_it = s.end_row == s.start_row
                                    && s.start_row == d.start_row
                                    && node_untouched_on_its_row(
                                        &source.contents,
                                        &destination.contents,
                                        &s,
                                        &d,
                                    );
                                let column_shift_is_meaningful = s.start_column != d.start_column
                                    && !shifted_within_its_own_line
                                    && !shifted_by_an_edit_beside_it;
                                // `NestedConditionCollapse`/`WrapGrowth` mark a node whose
                                // relocation is *known*, by construction, to be a pure reindent -
                                // see `solve_nested_condition_collapse`'s and `solve_wrap_growth`'s
                                // own doc comments. Deliberately narrow (only these specific,
                                // pre-verified reasons, never a bare column-shift check): the
                                // general heuristic above cannot tell a pure reindent from a genuine
                                // relocation by position alone - `rust-add-if`'s own shape is
                                // exactly the counter-example that ruled that out, which is why it's
                                // included here by *verified reason* (`WrapGrowth`) rather than by
                                // loosening the position-based heuristic itself.
                                let known_pure_reindent = !paint_reindent_only_moves
                                    && matches!(
                                        mapping.reason,
                                        ASTMappingReason::NestedConditionCollapse
                                            | ASTMappingReason::WrapGrowth
                                    );
                                // Unlike `NestedConditionCollapse` above, not gated by
                                // `paint_reindent_only_moves`: this tag only fires when a class's
                                // or interface's body is byte-identical and its shift is verified
                                // to come entirely from a newly-inserted heritage clause (see
                                // `solve_heritage_clause_growth`'s doc comment), and both `MINIMAL`
                                // and `FULL` ground truth agree it should never paint `Move`
                                // (measured on `typescript-refactor-interface`) - a correctness
                                // fix, not a preference.
                                let known_pure_relocation =
                                    mapping.reason == ASTMappingReason::HeritageClauseGrowth;
                                if (!column_shift_is_meaningful && !crossed_backwards)
                                    || known_pure_reindent
                                    || known_pure_relocation
                                {
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
                        // A wholly-new-or-removed content node (a comment, e.g. `// Early
                        // termination optimization`) whose grammar splits it into a marker leaf
                        // (`//`) plus un-decomposed trailing text - the same shape
                        // `own_content`/`own_content_span` exist for on the `MatchButNotIdentical`
                        // arm below, but for a *whole* node insertion/deletion rather than a
                        // changed one. Without this arm, the childless-leaf arms above only ever
                        // painted the marker (`//`), and the comment's actual words - not covered
                        // by any child - were silently dropped: confirmed on
                        // `rust-cost-optimization`, where a brand-new `// Early termination
                        // optimization` comment rendered with only its `//` highlighted and the
                        // rest in plain, unpainted text.
                        //
                        // Scoped tightly to avoid the ordering hazard a naive fix would hit: this
                        // node's own gap text sits *after* its children in the file, but the
                        // surrounding pre-order stack walk would visit those children in a later
                        // iteration, so pushing the gap range here (during the parent's own turn)
                        // and the children's ranges later (in theirs) would insert the gap into
                        // `ranges` out of byte order - corrupting every `last_non_move_range`
                        // anchor computed afterward. Instead, when every direct child is itself a
                        // leaf (`child_count() == 0` - true for a marker token like `//` or a
                        // quote character, false for anything with real internal structure this
                        // arm has no business re-deciding), this computes the *whole* ordered
                        // range list - each child's own range interleaved with any real (non-
                        // whitespace) gap text around it - in one pass here, and skips the normal
                        // per-child descent (`descend = false`) so those children aren't visited
                        // (and painted) a second time.
                        //
                        // `is_content_node` gates this the same way it gates
                        // `intra_node_update_ranges`'s affix split: only comments/literals/strings
                        // have meaningful text living in a node's own gaps rather than in a named
                        // child, so this never fires for a container (a `block`, a `class_body`)
                        // gaining or losing a whole child - that shape already renders correctly
                        // via the childless-leaf arms recursing normally.
                        //
                        // `own_content_span(node).is_some_and(...)` in the guard - not just inside
                        // the body - matters: it's what keeps this arm from firing on a node whose
                        // children already fully reconstruct it with no real gap at all (a Java
                        // `string_literal` node made of a `"` / `string_fragment` / `"` triple with
                        // nothing between them, extremely common, unlike a genuinely gappy comment).
                        // Confirmed as a real regression, not a hypothetical one: an earlier version
                        // of this arm fired there too and, despite painting every one of those
                        // three children correctly on their own, its `new_ranges.len() > 1` bypass
                        // (below) skips the normal same-operation-neighbor merge accumulator that
                        // silently absorbs a *whitespace-only* gap into an adjacent Insert/Delete
                        // range (see the comment where `new_ranges.len() > 1` is checked, further
                        // down) - so cutting a no-gap node like this over to the bypass path dropped
                        // whitespace at its *boundary* with a sibling (a single space between a
                        // string literal and a `+` in `"Dividing " + a`) that isn't part of this
                        // node at all. Regressed `java-add-logging` from exact agreement to six
                        // dropped bytes before this guard was added.
                        ASTMappingOperation::Insert | ASTMappingOperation::Delete
                            if node.child_count() > 0
                                && is_content_node(node.kind())
                                && node
                                    .children(&mut node.walk())
                                    .all(|c| c.child_count() == 0)
                                && own_content_span(node).is_some_and(|(_, from, to)| {
                                    !source.contents[from..to].trim().is_empty()
                                }) =>
                        {
                            let operation = match mapping.operation {
                                ASTMappingOperation::Insert => TextOperation::Insert,
                                _ => TextOperation::Delete,
                            };
                            let mut pos = node.start_byte();
                            let mut gap_start_point = node.start_position();
                            let mut child_cursor = node.walk();
                            for child in node.children(&mut child_cursor) {
                                if child.start_byte() > pos
                                    && !source.contents[pos..child.start_byte()].trim().is_empty()
                                {
                                    let gap_text = &source.contents[pos..child.start_byte()];
                                    let gap_end = point_at_byte_offset(
                                        gap_text,
                                        gap_start_point,
                                        gap_text.len(),
                                    );
                                    new_ranges.push(advance_and_build_range_with_source(
                                        &mut last_non_move_range,
                                        text_range_from_points(
                                            gap_start_point,
                                            gap_end,
                                            &source_columns,
                                        ),
                                        operation.clone(),
                                    ));
                                }
                                new_ranges.push(advance_and_build_range(
                                    &mut last_non_move_range,
                                    child,
                                    &source_columns,
                                    operation.clone(),
                                ));
                                pos = pos.max(child.end_byte());
                                gap_start_point = child.end_position();
                            }
                            if node.end_byte() > pos
                                && !source.contents[pos..node.end_byte()].trim().is_empty()
                            {
                                let gap_text = &source.contents[pos..node.end_byte()];
                                let gap_end =
                                    point_at_byte_offset(gap_text, gap_start_point, gap_text.len());
                                new_ranges.push(advance_and_build_range_with_source(
                                    &mut last_non_move_range,
                                    text_range_from_points(
                                        gap_start_point,
                                        gap_end,
                                        &source_columns,
                                    ),
                                    operation,
                                ));
                            }
                            descend = false;
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
                                    source_is_before,
                                    is_content_node(node.kind()),
                                    whole_pair_updates,
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
                                            source_is_before,
                                            is_content_node(node.kind()),
                                            whole_pair_updates,
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

/// True if the single-row node at `s` (in `source`) / `d` (in `destination`) lies wholly inside
/// the common prefix or the common suffix of its two rows - i.e. the rows differ only in one
/// stretch that does not overlap the node, and the node merely slid sideways. Columns are the
/// character columns `TextRange` carries; a row past either text's end counts as empty. See the
/// `shifted_by_an_edit_beside_it` call site in `ranges`.
fn node_untouched_on_its_row(
    source: &str,
    destination: &str,
    s: &TextRange,
    d: &TextRange,
) -> bool {
    let row = |text: &str, row: usize| -> Vec<char> {
        text.split('\n')
            .nth(row)
            .map(|line| line.chars().collect())
            .unwrap_or_default()
    };
    let source_row = row(source, s.start_row);
    let destination_row = row(destination, d.start_row);
    let prefix = source_row
        .iter()
        .zip(destination_row.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = source_row
        .iter()
        .rev()
        .zip(destination_row.iter().rev())
        .take_while(|(a, b)| a == b)
        .count()
        .min(source_row.len() - prefix)
        .min(destination_row.len() - prefix);
    let in_prefix = s.end_column <= prefix && d.end_column <= prefix;
    let in_suffix = s.start_column + suffix >= source_row.len()
        && d.start_column + suffix >= destination_row.len();
    in_prefix || in_suffix
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
/// ranges and makes the range vectors symmetric.
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
    /// Construct the TextDiff from an ASTDiff, with [`RenderOptions::whole_pair_updates`] off and
    /// [`RenderOptions::paint_reindent_only_moves`] on - the readings every caller but the ones
    /// that resolve those options from a real `RenderOptions` wants (both match every release
    /// before either option existed). See [`Self::from_with_options`] for the parameterized
    /// version and why this stays the zero-argument default rather than growing parameters of its
    /// own: touching every call site's signature for options almost none of them resolve is
    /// exactly the "risky `ranges()` logic" churn that isn't worth it just to type two literals in
    /// one place instead of here.
    pub fn from(before: &Code, after: &Code, diff: &ASTDiff, node_cache: &NodeCache) -> Self {
        Self::from_with_options(before, after, diff, node_cache, false, true)
    }

    /// [`Self::from_with_update_style`], with [`RenderOptions::paint_reindent_only_moves`] also
    /// threaded through - kept as a thin wrapper (rather than renaming callers) so the one
    /// existing caller that only ever resolved `whole_pair_updates` doesn't need to also decide
    /// an opinion on the newer option; new callers that need both should call
    /// [`Self::from_with_options`] directly.
    pub fn from_with_update_style(
        before: &Code,
        after: &Code,
        diff: &ASTDiff,
        node_cache: &NodeCache,
        whole_pair_updates: bool,
    ) -> Self {
        Self::from_with_options(before, after, diff, node_cache, whole_pair_updates, true)
    }

    /// [`Self::from`], with both [`RenderOptions::whole_pair_updates`] and
    /// [`RenderOptions::paint_reindent_only_moves`] threaded through to every node that needs
    /// them, instead of hardcoded to their legacy defaults.
    ///
    /// A separate method rather than parameters on `from` itself: `from` has real call sites in
    /// `human_solver`, `generate_mapping_site` and the test harness that have no opinion on either
    /// option and would otherwise all need updating (and re-reviewing) just to keep passing the
    /// same two literals - the cost `RenderOptions::whole_pair_updates`'s own doc comment says
    /// this design avoids. Callers that resolve a real `RenderOptions` (`tui::app::
    /// assemble_diff_session_data`, and `test::helper::human_mapping::compare_painting` - the
    /// latter now builds one `TextDiff` per mode instead of one shared between `Minimal`/`Full`,
    /// since `paint_reindent_only_moves` genuinely differs between the two presets where
    /// `whole_pair_updates` never did) call this one.
    pub fn from_with_options(
        before: &Code,
        after: &Code,
        diff: &ASTDiff,
        node_cache: &NodeCache,
        whole_pair_updates: bool,
        paint_reindent_only_moves: bool,
    ) -> Self {
        let mut before_ranges_plain = ranges(
            before,
            after,
            diff,
            node_cache,
            true,
            whole_pair_updates,
            paint_reindent_only_moves,
        );
        let mut after_ranges_plain = ranges(
            after,
            before,
            diff,
            node_cache,
            false,
            whole_pair_updates,
            paint_reindent_only_moves,
        );

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

#[cfg(test)]
mod tests;
