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

use summary::whitespace_stripped_equal;

/// The painted ranges of an [`ASTDiff`], one list per side, as a text editor would show them.
///
/// Whitespace is ignored except where it changes the parsed AST (e.g. inside a string constant).
#[derive(Debug, Clone, Default)]
pub struct TextDiff {
    // TODO: a tree-based structure, for partial lookups in very large files.
    before_ranges: Vec<RangeMatch>,
    after_ranges: Vec<RangeMatch>,
}

/// The text of `node` not covered by any *direct* child, concatenated: a `line_comment`'s words
/// after its `//` child, or a container's whitespace and punctuation between children.
///
/// Comparing this, not the whole text, separates "this node's own content changed" (a comment)
/// from "something inside this container changed" without per-language node-kind rules.
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

/// The start point and byte range of `node`'s own content when it is one contiguous gap; `None`
/// when it is split across several gaps (or there is none), since a sub-node range can only be
/// placed on one span.
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

/// Byte length of the longest common prefix of `a` and `b`, compared by character, so the result
/// is a char boundary in *both* strings.
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

/// [`common_prefix_byte_len`] from the end. Callers pass strings already trimmed of their common
/// prefix, so the two never overlap.
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

/// The point `offset` bytes into `text`, which starts at `start`. Columns are bytes, tree-sitter's
/// convention. `offset` must be a char boundary of `text`.
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

/// A `TextRange` for a span that is not a node's own range (e.g. the changed middle of a string).
/// The byte fields are left 0 because `TextRange::from_treesitter_range` reads only the points.
fn text_range_from_points(start: Point, end: Point, columns_per_row: &[usize]) -> TextRange {
    let ts_range = Range {
        start_byte: 0,
        end_byte: 0,
        start_point: start,
        end_point: end,
    };
    TextRange::from_treesitter_range(ts_range, columns_per_row)
}

/// One side's text, where it starts in its file, and that file's row lengths. Bundled only to keep
/// `intra_node_update_ranges` under clippy's argument-count limit.
struct TextSpan<'a> {
    text: &'a str,
    start: Point,
    columns: &'a [usize],
}

/// Whether a node's own text is content a reader reads (a literal or comment) rather than a name
/// or grammar glue. Adding a phrase to a docstring is an insertion; `IntBox` becoming `Box` is a
/// rename and `<=` becoming `<` a changed operator, which painters call updated.
fn is_content_node(kind: &str) -> bool {
    // Not folded into `nodes::is_literal_kind`: that list also drives the APTED rename cost and
    // `code::hash`, and widening it for a rendering choice would change matching too. Python's
    // docstring is `string` and HTML's content is `raw_text`, hence the substring tests.
    nodes::is_literal_kind(kind)
        || nodes::is_comment(kind)
        || kind.contains("string")
        || kind.contains("comment")
        || kind.contains("raw_text")
}

/// The tokens a painting always marks whole, under both presets. Shared with the ground-truth
/// invariant `tokens_are_painted_whole` so the two cannot disagree.
///
/// A list rather than a character class so what counts as one symbol is written down. `.=` is `=`
/// grown at its front; `true`/`false` and `private`/`protected` share letters no reader sees as
/// surviving.
pub(crate) const WHOLE_TOKENS: &[&str] = &[
    "<",
    ">",
    "=",
    "==",
    "===",
    "!=",
    "!==",
    "<=",
    ">=",
    "+",
    "-",
    "++",
    "--",
    "+=",
    "-=",
    "*=",
    ">>",
    "<<",
    "/>",
    ".=",
    "true",
    "false",
    "private",
    "protected",
];

/// Whether a changed node of `kind` reading `a` and `b` is a renamed identifier, the shape
/// [`RenderOptions::whole_identifier_updates`] paints whole. Content nodes keep their word-level
/// reading, and so does HTML's `text` node: `office` there is prose without spaces.
fn is_renamed_identifier(kind: &str, a: &str, b: &str) -> bool {
    let identifier = |text: &str| {
        let mut chars = text.chars();
        chars
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
            && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
    };
    !is_content_node(kind) && kind != "text" && identifier(a) && identifier(b)
}

/// Whether an affix split of `a` against `b` would cut through one of [`WHOLE_TOKENS`]: `<=` to
/// `<` is a different comparison, not a kept `<`.
fn splits_a_whole_token(a: &str, b: &str) -> bool {
    [a, b]
        .iter()
        .any(|text| text.len() > 1 && WHOLE_TOKENS.contains(text))
}

/// Splits a changed node's own text into an `Identical` common prefix, a changed middle, and an
/// `Identical` common suffix, so a small edit inside a long string or name paints only what
/// changed. One whole-span `Update` when there is no common affix or `whole_pair_updates` is set.
///
/// When one side's middle is empty on a content node, nothing was replaced: the middle is an
/// `Insert`/`Delete`, chosen by `source_is_before`, since the texts alone cannot say which. On a
/// name or operator it stays `Update`.
///
/// Only ever one middle: the painted corpus never marks two separate runs inside one updated node.
///
/// The prefix and suffix carry the counterpart's real positions; the middle is anchored at
/// `last_non_move_range` like every other placed range. A fabricated destination on an
/// `Identical` range could corrupt `extend_into` when it merges with a real neighbour.
///
/// The sub-range count and order are the same whichever side is `source` (only the middle's
/// operation mirrors), which is why [`RangeWalk::push`] must not merge a multi-range result.
fn intra_node_update_ranges(
    last_non_move_range: &mut TextRange,
    whole_source_range: TextRange,
    source: TextSpan,
    destination: TextSpan,
    source_is_before: bool,
    content_node: bool,
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
    // `source` is the painted side: "no text here, text there" is an insertion seen from before
    // and a deletion seen from after.
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

/// How one source node renders, decided from its mapping alone. Shared by [`ranges`] (which turns
/// it into painted ranges) and `summary::scan` (which only asks whether it is a visible change),
/// so the two cannot disagree about what counts as one.
pub(crate) enum NodeChange<'c> {
    /// Mapped `Identical` - the counterpart, when the cache knows it. [`ranges`] still has to
    /// decide whether the node's *position* makes it a `Move`.
    Identical(Option<Node<'c>>),
    /// The whole subtree was deleted or inserted: one range, never descended.
    PrunedSubtree(TextOperation),
    /// A childless node deleted or inserted; the walk still descends (there is nothing below).
    Leaf(TextOperation),
    /// Mapped `Update` - the counterpart, when the cache knows it.
    Update(Option<Node<'c>>),
    /// Mapped `MatchButNotIdentical` with its *own* gap text changed beyond whitespace (see
    /// [`own_content`]): the counterpart. A container whose children changed but whose own gaps
    /// did not is [`NodeChange::Descend`] instead, so the walk finds the smaller real change.
    OwnContentChanged(Node<'c>),
    /// Nothing to paint at this node; its children decide.
    Descend,
}

/// Classifies `node` by its mapping `operation` to `mapped_id`. `own_bytes`/`other_bytes` are
/// the two files' contents.
///
/// `MatchButNotIdentical` compares [`own_content`], not the whole text: a container that gained a
/// statement differs as a whole but not in its own gaps, so it descends to the smaller change.
/// Comparing the children's mapping operations instead misses a statement moved one level deeper
/// (still `Identical` in the AST) and paints the whole block `Update`.
///
/// `OwnContentChanged` also requires each side's own content to be one contiguous gap, the only
/// shape it can paint narrowly. A `\`-continued vim `dictionnary` or shell `command` has a marker
/// in every gap, so gaining a child changes its own content; it must descend to that child rather
/// than paint the whole container (`vimscript-neovim-neovim-add-one-dict-entry`).
pub(crate) fn classify_node<'c>(
    node: Node,
    mapped_id: usize,
    operation: &ASTMappingOperation,
    node_cache: &'c NodeCache,
    own_bytes: &[u8],
    other_bytes: &[u8],
) -> NodeChange<'c> {
    let counterpart = || node_cache.get_in_any(&mapped_id).copied();
    match operation {
        ASTMappingOperation::Identical => NodeChange::Identical(counterpart()),
        ASTMappingOperation::DeleteWithChildren => NodeChange::PrunedSubtree(TextOperation::Delete),
        ASTMappingOperation::InsertWithChildren => NodeChange::PrunedSubtree(TextOperation::Insert),
        ASTMappingOperation::Delete if node.child_count() == 0 => {
            NodeChange::Leaf(TextOperation::Delete)
        }
        ASTMappingOperation::Insert if node.child_count() == 0 => {
            NodeChange::Leaf(TextOperation::Insert)
        }
        ASTMappingOperation::Update => NodeChange::Update(counterpart()),
        ASTMappingOperation::MatchButNotIdentical => match counterpart() {
            Some(other)
                if own_content_span(node).is_some()
                    && own_content_span(other).is_some()
                    && !whitespace_stripped_equal(
                        &own_content(node, own_bytes),
                        &own_content(other, other_bytes),
                    ) =>
            {
                NodeChange::OwnContentChanged(other)
            }
            _ => NodeChange::Descend,
        },
        _ => NodeChange::Descend,
    }
}

/// The state of one [`ranges`] traversal.
struct RangeWalk<'a> {
    source: &'a Code,
    destination: &'a Code,
    diff: &'a ASTDiff,
    node_cache: &'a NodeCache<'a>,
    source_columns: Vec<usize>,
    destination_columns: Vec<usize>,
    // Built once per walk: `RangeMatch::extends` reads them on every merge decision.
    source_text: SourceText<'a>,
    destination_text: SourceText<'a>,
    // Only `intra_node_update_ranges` reads it, to tell an insertion from a deletion; everything
    // else here is symmetric.
    source_is_before: bool,
    // Only the construction-time options are read; see [`TextDiff::from_with_options`].
    options: RenderOptions,
    /// Where the last in-sequence range ended on the destination side. A deleted, inserted or
    /// updated node has no destination counterpart and is anchored here; a `Move`'s destination is
    /// out of sequence and never becomes the anchor.
    last_non_move_range: TextRange,
    ranges: Vec<RangeMatch>,
    current_range: RangeMatch,
}

/// The ranges of `source`, painted against `destination`, before merging in the other side's
/// insertions (see [`merge_ranges`]).
fn ranges(
    source: &Code,
    destination: &Code,
    diff: &ASTDiff,
    node_cache: &NodeCache,
    source_is_before: bool,
    options: RenderOptions,
) -> Vec<RangeMatch> {
    let mut walk = RangeWalk {
        source,
        destination,
        diff,
        node_cache,
        source_columns: compute_row_byte_lengths(&source.contents),
        destination_columns: compute_row_byte_lengths(&destination.contents),
        source_text: SourceText::new(&source.contents),
        destination_text: SourceText::new(&destination.contents),
        source_is_before,
        options,
        last_non_move_range: TextRange::zero(),
        ranges: Vec::new(),
        current_range: RangeMatch::zero(),
    };

    match (&source.ast, &destination.ast) {
        (None, None) => {}
        (Some(source_tree), None) => {
            let source_root = source_tree.root_node();
            walk.ranges.push(RangeMatch {
                source: TextRange::from_treesitter_range(source_root.range(), &walk.source_columns),
                destination: TextRange::zero(),
                operation: TextOperation::Delete,
            });
        }
        (None, Some(destination_tree)) => {
            let destination_root = destination_tree.root_node();
            walk.ranges.push(RangeMatch {
                source: TextRange::zero(),
                destination: TextRange::from_treesitter_range(
                    destination_root.range(),
                    &walk.destination_columns,
                ),
                operation: TextOperation::Insert,
            });
        }
        (Some(source_tree), Some(_destination_tree)) => {
            walk.visit_tree(source_tree.root_node());
        }
    }

    walk.ranges
}

impl RangeWalk<'_> {
    /// Pre-order traversal of the source tree, painting every node with a known mapping and
    /// descending only where [`Self::visit`] says the children still have to decide.
    fn visit_tree(&mut self, root: Node) {
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if !self.visit(node) {
                continue;
            }
            let mut child_cursor = node.walk();
            let children: Vec<_> = node.children(&mut child_cursor).collect();
            for child in children.into_iter().rev() {
                stack.push(child);
            }
        }
        if !self.current_range.is_zero() {
            let finished = std::mem::replace(&mut self.current_range, RangeMatch::zero());
            self.ranges.push(finished);
        }
    }

    /// Paints one node. Returns whether the traversal should descend into its children.
    fn visit(&mut self, node: Node) -> bool {
        let Some((mapped_id, mapping)) = self.diff.mapping_for_node(&node.id()) else {
            return true;
        };
        let change = classify_node(
            node,
            mapped_id,
            &mapping.operation,
            self.node_cache,
            self.source.contents.as_bytes(),
            self.destination.contents.as_bytes(),
        );
        let (new_ranges, descend) = match change {
            NodeChange::Identical(Some(destination_node)) => (
                vec![self.identical_or_move(node, destination_node, mapping.reason)],
                false,
            ),
            NodeChange::Identical(None) => (Vec::new(), true),
            NodeChange::PrunedSubtree(operation) => (vec![self.placed(node, operation)], false),
            NodeChange::Leaf(operation) => (vec![self.placed(node, operation)], true),
            NodeChange::Update(Some(destination_node)) => {
                (self.update_ranges(node, destination_node), true)
            }
            NodeChange::Update(None) => (vec![self.placed(node, TextOperation::Update)], true),
            NodeChange::OwnContentChanged(destination_node) => (
                self.own_content_update_ranges(node, destination_node),
                false,
            ),
            NodeChange::Descend => match self.whole_content_prune(node, &mapping.operation) {
                Some(operation) => (self.own_gap_ranges(node, operation), false),
                None => (Vec::new(), true),
            },
        };
        self.push(new_ranges);
        descend
    }

    /// The range for a node with no destination counterpart; see [`advance_and_build_range`].
    fn placed(&mut self, node: Node, operation: TextOperation) -> RangeMatch {
        advance_and_build_range(
            &mut self.last_non_move_range,
            node,
            &self.source_columns,
            operation,
        )
    }

    /// Appends a node's ranges to the output, merging same-operation neighbours.
    ///
    /// A multi-range result bypasses merging. Merging reads each side's own surrounding
    /// whitespace, which differs between the two walks, and would break the matching sub-range
    /// counts [`intra_node_update_ranges`] guarantees.
    fn push(&mut self, new_ranges: Vec<RangeMatch>) {
        if new_ranges.len() > 1 {
            if !self.current_range.is_zero() {
                let finished = std::mem::replace(&mut self.current_range, RangeMatch::zero());
                self.ranges.push(finished);
            }
            self.ranges.extend(new_ranges);
            return;
        }
        for new_range in new_ranges {
            if new_range.extends(
                &self.current_range,
                &self.source_text,
                &self.destination_text,
            ) {
                self.current_range.extend_into(&new_range);
            } else {
                if !self.current_range.is_zero() {
                    let finished = std::mem::replace(&mut self.current_range, RangeMatch::zero());
                    self.ranges.push(finished);
                }
                self.current_range = new_range;
            }
        }
    }

    /// An `Identical`-mapped node paints `Identical` where the sequential flow puts it and `Move`
    /// where its position says it relocated. Only an `Identical` result advances the anchor.
    fn identical_or_move(
        &mut self,
        node: Node,
        destination_node: Node,
        reason: ASTMappingReason,
    ) -> RangeMatch {
        let s = TextRange::from_treesitter_range(node.range(), &self.source_columns);
        let d =
            TextRange::from_treesitter_range(destination_node.range(), &self.destination_columns);

        // A column change means a relocation (e.g. reindented into a new block); a row-only shift
        // is an unrelated edit above. A multi-row node landing before the anchor crossed earlier
        // content: a sibling reorder, which otherwise paints nothing at all. Sub-line tokens are
        // exempt because imperfect matching can pair a `}` with an earlier twin.
        let crossed_backwards = s.end_row > s.start_row
            && (d.start_row, d.start_column)
                < (
                    self.last_non_move_range.start_row,
                    self.last_non_move_range.start_column,
                );
        // A multi-row node still on its own starting row was pushed right by an edit earlier on
        // that row; only its first row moved (`cpp-add-const-correctness`). Single-row nodes, and
        // multi-row nodes that also changed rows (a block moved into a new `if`), still count.
        //
        // "Its own starting row" means content, not index: an insertion above can also push it
        // down (`rust-adding-a-variable-and-test-with-comments`). This disjunct accepts that when
        // every row shifts equally, the end column holds, and the first row's tail is unchanged.
        // `rust-add-if` has the same geometry but is decided by `known_pure_reindent` below.
        let displaced_beside_an_edit_on_its_first_row = !self.options.paint_displaced_moves
            && d.end_row as i64 - s.end_row as i64 == d.start_row as i64 - s.start_row as i64
            && s.end_column == d.end_column
            && node_first_row_tail_untouched(
                &self.source.contents,
                &self.destination.contents,
                &s,
                &d,
            );
        let shifted_within_its_own_line = s.end_row > s.start_row
            && (s.start_row == d.start_row || displaced_beside_an_edit_on_its_first_row);
        // Likewise a single-row node slid sideways by an edit beside it on its row (`int x` in
        // `process(int x)` -> `process(const int x)`). A node inside the rewritten part of the row
        // keeps `Move` (`function fetchData(` in `typescript-async-await`). Reindented nodes are
        // excluded so they still reach `paint_reindent_only_moves`. Under `MINIMAL` the row index
        // may differ, since `node_untouched_on_its_row` compares the rows' content.
        let shifted_by_an_edit_beside_it = s.end_row == s.start_row
            && (s.start_row == d.start_row || !self.options.paint_displaced_moves)
            && node_untouched_on_its_row(
                &self.source.contents,
                &self.destination.contents,
                &s,
                &d,
                !self.options.paint_displaced_moves,
            );
        let column_shift_is_meaningful = s.start_column != d.start_column
            && !shifted_within_its_own_line
            && !shifted_by_an_edit_beside_it;
        // These reasons are verified pure reindents (see their solvers). Trusted by reason only:
        // position alone cannot tell a reindent from a relocation (`rust-add-if`).
        let known_pure_reindent = !self.options.paint_reindent_only_moves
            && matches!(
                reason,
                ASTMappingReason::NestedConditionCollapse | ASTMappingReason::WrapGrowth
            );
        // Ungated: a byte-identical body shifted only by a new heritage clause is never a `Move`
        // under either preset (`typescript-refactor-interface`).
        let known_pure_relocation = reason == ASTMappingReason::HeritageClauseGrowth;
        let operation = if (!column_shift_is_meaningful && !crossed_backwards)
            || known_pure_reindent
            || known_pure_relocation
        {
            self.last_non_move_range = d.clone();
            TextOperation::Identical
        } else {
            TextOperation::Move
        };
        RangeMatch {
            source: s,
            destination: d,
            operation,
        }
    }

    /// An `Update` node, split by [`intra_node_update_ranges`].
    fn update_ranges(&mut self, node: Node, destination_node: Node) -> Vec<RangeMatch> {
        let source_text = node
            .utf8_text(self.source.contents.as_bytes())
            .unwrap_or("");
        let destination_text = destination_node
            .utf8_text(self.destination.contents.as_bytes())
            .unwrap_or("");
        intra_node_update_ranges(
            &mut self.last_non_move_range,
            TextRange::from_treesitter_range(node.range(), &self.source_columns),
            TextSpan {
                text: source_text,
                start: node.start_position(),
                columns: &self.source_columns,
            },
            TextSpan {
                text: destination_text,
                start: destination_node.start_position(),
                columns: &self.destination_columns,
            },
            self.source_is_before,
            is_content_node(node.kind()),
            self.options.whole_pair_updates
                || splits_a_whole_token(source_text, destination_text)
                || (self.options.whole_identifier_updates
                    && is_renamed_identifier(node.kind(), source_text, destination_text)),
        )
    }

    /// A node whose own content ([`own_content`]) changed, e.g. a Rust `line_comment`, whose words
    /// belong to the node itself rather than to a leaf child. Split like an update when both sides'
    /// content is one gap; otherwise the whole node is an `Update`.
    fn own_content_update_ranges(&mut self, node: Node, destination_node: Node) -> Vec<RangeMatch> {
        match (own_content_span(node), own_content_span(destination_node)) {
            (Some((s_start, s_from, s_to)), Some((d_start, d_from, d_to))) => {
                intra_node_update_ranges(
                    &mut self.last_non_move_range,
                    TextRange::from_treesitter_range(node.range(), &self.source_columns),
                    TextSpan {
                        text: &self.source.contents[s_from..s_to],
                        start: s_start,
                        columns: &self.source_columns,
                    },
                    TextSpan {
                        text: &self.destination.contents[d_from..d_to],
                        start: d_start,
                        columns: &self.destination_columns,
                    },
                    self.source_is_before,
                    is_content_node(node.kind()),
                    self.options.whole_pair_updates
                        || splits_a_whole_token(
                            &self.source.contents[s_from..s_to],
                            &self.destination.contents[d_from..d_to],
                        )
                        || (self.options.whole_identifier_updates
                            && is_renamed_identifier(
                                node.kind(),
                                &self.source.contents[s_from..s_to],
                                &self.destination.contents[d_from..d_to],
                            )),
                )
            }
            _ => vec![self.placed(node, TextOperation::Update)],
        }
    }

    /// Whether a non-leaf `Insert`/`Delete` node is a wholly new or removed content node, such as a
    /// comment split into a `//` leaf plus its own words, and so is painted by
    /// [`Self::own_gap_ranges`]. Descending would paint only the `//` (`rust-cost-optimization`).
    ///
    /// Three guards:
    ///
    /// * every child is a leaf, so nothing with real structure is re-decided here;
    /// * `is_content_node`, so a container gaining a child still descends;
    /// * a non-blank own gap. A node its children fully cover (a `"` / fragment / `"` string)
    ///   would take the multi-range bypass in [`Self::push`] and lose the whitespace merge with
    ///   its sibling, as in the spaces around `+` in `"Dividing " + a`.
    fn whole_content_prune(
        &self,
        node: Node,
        operation: &ASTMappingOperation,
    ) -> Option<TextOperation> {
        let operation = match operation {
            ASTMappingOperation::Insert => TextOperation::Insert,
            ASTMappingOperation::Delete => TextOperation::Delete,
            _ => return None,
        };
        let fires = node.child_count() > 0
            && is_content_node(node.kind())
            && node
                .children(&mut node.walk())
                .all(|c| c.child_count() == 0)
            && own_content_span(node)
                .is_some_and(|(_, from, to)| !self.source.contents[from..to].trim().is_empty());
        fires.then_some(operation)
    }

    /// The ranges of a [`Self::whole_content_prune`] node: its children and its non-blank gaps, in
    /// byte order. Built in one pass because the pre-order walk would visit the children after the
    /// gap was pushed, out of order, corrupting every later `last_non_move_range` anchor.
    fn own_gap_ranges(&mut self, node: Node, operation: TextOperation) -> Vec<RangeMatch> {
        let mut new_ranges = Vec::new();
        let mut pos = node.start_byte();
        let mut gap_start_point = node.start_position();
        let mut child_cursor = node.walk();
        for child in node.children(&mut child_cursor) {
            if child.start_byte() > pos {
                new_ranges.extend(self.gap_range(
                    pos,
                    child.start_byte(),
                    gap_start_point,
                    &operation,
                ));
            }
            new_ranges.push(self.placed(child, operation.clone()));
            pos = pos.max(child.end_byte());
            gap_start_point = child.end_position();
        }
        if node.end_byte() > pos {
            new_ranges.extend(self.gap_range(pos, node.end_byte(), gap_start_point, &operation));
        }
        new_ranges
    }

    /// The range for the gap text `from..to` starting at `start`, if it holds anything but
    /// whitespace.
    fn gap_range(
        &mut self,
        from: usize,
        to: usize,
        start: Point,
        operation: &TextOperation,
    ) -> Option<RangeMatch> {
        let gap_text = &self.source.contents[from..to];
        if gap_text.trim().is_empty() {
            return None;
        }
        let gap_end = point_at_byte_offset(gap_text, start, gap_text.len());
        Some(advance_and_build_range_with_source(
            &mut self.last_non_move_range,
            text_range_from_points(start, gap_end, &self.source_columns),
            operation.clone(),
        ))
    }
}

/// True if the single-row node at `s`/`d` lies wholly inside the common prefix or suffix of its two
/// rows, so the row's one edit is beside it. Columns are characters; a missing row reads as empty.
fn node_untouched_on_its_row(
    source: &str,
    destination: &str,
    s: &TextRange,
    d: &TextRange,
    // `!paint_displaced_moves`.
    read_rows_carrying_several_edits: bool,
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
    in_prefix
        || in_suffix
        // Same row index required: uniqueness places the node within each row but cannot say the
        // two rows are one row. A statement split in two whose unique call lands rows below is a
        // real `Move` (`java-defects4j-chart-25-statisticalbarrenderer`).
        || (read_rows_carrying_several_edits
            && s.start_row == d.start_row
            && row_was_edited_rather_than_rewritten(&source_row, &destination_row, prefix, suffix)
            && node_uniquely_placed_on_its_row(&source_row, &destination_row, s, d))
}

/// True if the two rows read as one edited row rather than one row replaced by another: the
/// changes are bounded on both sides by unchanged text, and the unchanged head is more than
/// indentation. Uniqueness alone would call `function fetchData(` untouched in
/// `typescript-async-await`, where nothing else on the row survived.
///
/// A row whose middle was rewritten between a real head and tail still passes
/// (`javascript-add-event-listener`, which paints the displaced `handleClick` moved). Separating
/// it would take a share-of-the-row threshold, a tuned sweep rather than a structural rule.
fn row_was_edited_rather_than_rewritten(
    source_row: &[char],
    destination_row: &[char],
    prefix: usize,
    suffix: usize,
) -> bool {
    let indentation = |row: &[char]| row.iter().take_while(|c| c.is_whitespace()).count();
    suffix > 0 && prefix > indentation(source_row).max(indentation(destination_row))
}

/// The longest row [`node_uniquely_placed_on_its_row`] scans; the scan is quadratic, and rows
/// tens of thousands of characters wide are minified or broken files, not code a reader diffs.
const MAX_ROW_FOR_UNIQUENESS_SCAN: usize = 2_000;

/// True if the node's text is the same on both sides and occurs exactly once on each row, at the
/// node's own columns. Reads rows carrying more than one edit, which the prefix/suffix test cannot
/// (`void printVector(std::vector<int> vec)` gaining both `const ` and `&`).
///
/// Uniqueness is what makes this a reading rather than a guess, the standard
/// [`crate::diff::solve_orphaned_leaves`] also uses: if the text repeats on the row, "it stayed"
/// and "it swapped with its twin" both fit the bytes, so this declines.
fn node_uniquely_placed_on_its_row(
    source_row: &[char],
    destination_row: &[char],
    s: &TextRange,
    d: &TextRange,
) -> bool {
    if source_row.len() > MAX_ROW_FOR_UNIQUENESS_SCAN
        || destination_row.len() > MAX_ROW_FOR_UNIQUENESS_SCAN
    {
        return false;
    }
    let (Some(text), Some(other)) = (
        source_row.get(s.start_column..s.end_column),
        destination_row.get(d.start_column..d.end_column),
    ) else {
        return false;
    };
    if text.is_empty() || text != other {
        return false;
    }
    fn only_occurrence_of(row: &[char], text: &[char]) -> Option<usize> {
        let mut found = None;
        for (at, window) in row.windows(text.len()).enumerate() {
            if window == text {
                if found.is_some() {
                    return None;
                }
                found = Some(at);
            }
        }
        found
    }
    only_occurrence_of(source_row, text) == Some(s.start_column)
        && only_occurrence_of(destination_row, text) == Some(d.start_column)
}

/// True if a multi-row node's first-row tail, from its start column to the end of the line, is
/// identical on both sides, so the row's edit sits before it. The multi-row counterpart of
/// [`node_untouched_on_its_row`]'s suffix case.
fn node_first_row_tail_untouched(
    source: &str,
    destination: &str,
    s: &TextRange,
    d: &TextRange,
) -> bool {
    let tail = |text: &str, row: usize, column: usize| -> Option<Vec<char>> {
        let line: Vec<char> = text.split('\n').nth(row)?.chars().collect();
        (column <= line.len()).then(|| line[column..].to_vec())
    };
    match (
        tail(source, s.start_row, s.start_column),
        tail(destination, d.start_row, d.start_column),
    ) {
        (Some(source_tail), Some(destination_tail)) => source_tail == destination_tail,
        _ => false,
    }
}

/// The range for a node with no destination counterpart (not `Identical`, not `Move`): advances
/// `last_non_move_range` to its right limit and anchors the destination there.
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

/// [`advance_and_build_range`] for a span that is not a whole node.
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

/// Interleaves the other side's insertions into `source_ranges` as `Delete` placeholders. An
/// inserted node does not exist in the source tree, so this is what makes the two lists symmetric.
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

/// Rows a range covers, as `line_operations` counts them: an end column of 0 excludes the end row.
/// Never zero.
fn rows_covered(range: &TextRange) -> usize {
    let end_row = if range.end_column == 0 {
        range.end_row
    } else {
        range.end_row + 1
    };
    end_row.saturating_sub(range.start_row).max(1)
}

/// True if `inner` lies wholly inside `outer`.
fn range_contains(outer: &TextRange, inner: &TextRange) -> bool {
    (outer.start_row, outer.start_column) <= (inner.start_row, inner.start_column)
        && (inner.end_row, inner.end_column) <= (outer.end_row, outer.end_column)
}

/// Makes the two sides agree on which matched pair relocated, by believing whichever side blames
/// fewer rows.
///
/// `crossed_backwards` asks about each walk's own order, so on a reorder the two walks can name
/// different pairs: moving one import below five, the before walk flags the five and the after
/// walk flags the one. The intersection is empty, losing the reorder, and the union paints all
/// six. One line past five is one move, so the smaller account of the disagreeing pairs wins and
/// the other side is rewritten to match. Agreed pairs are never touched.
///
/// Under [`RenderOptions::paint_resized_moves`] a pair is agreed when the other side has any
/// `Move` nested either way with its destination: the walks decompose one subtree differently, and
/// treating extents a few columns apart as a conflict withdraws a correct claim with nothing to
/// promote in its place.
///
/// Limits: the verdict is per file, not per reorder, so of two opposite reorders the minority one
/// is decided wrongly; and a counterpart is found by exact extent only, so one that is not found
/// stays unpainted rather than being invented.
fn reconcile_moves(before: &mut [RangeMatch], after: &mut [RangeMatch], options: RenderOptions) {
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

    // `(index, counterpart's index on the other side)` for every `Move` whose counterpart is not
    // also a `Move`.
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
                    // `rust-next-font-imports-generator`: both walks call a de-indented chain
                    // `Move` over extents four columns apart. `None if`, not `_ if`: an
                    // `Identical` range at this exact extent is the counterpart to promote.
                    None if options.paint_resized_moves
                        && other.iter().any(|other| {
                            other.operation == TextOperation::Move
                                && !other.source.is_empty()
                                && (range_contains(&range_match.destination, &other.source)
                                    || range_contains(&other.source, &range_match.destination))
                        }) =>
                    {
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

    // Ties go to the before side: arbitrary, but fixed so the result is a function of the input.
    let (winner, loser) = if before_rows <= after_rows {
        (&before_disagreed, &mut *after)
    } else {
        (&after_disagreed, &mut *before)
    };

    // The loser's claims are withdrawn: their counterparts on the winning side are `Identical`.
    for &(index, _) in if before_rows <= after_rows {
        &after_disagreed
    } else {
        &before_disagreed
    } {
        loser[index].operation = TextOperation::Identical;
    }
    // ...and the winner's counterparts painted, so a `Move` leads to a highlighted node. None of
    // them was a `Move`, so none was just withdrawn.
    for &(_, counterpart) in winner {
        if let Some(counterpart) = counterpart {
            loser[counterpart].operation = TextOperation::Move;
        }
    }
}

impl TextDiff {
    /// [`Self::from_with_options`] under [`RenderOptions::FULL`], for callers with no opinion on
    /// the construction-time options.
    pub fn from(before: &Code, after: &Code, diff: &ASTDiff, node_cache: &NodeCache) -> Self {
        Self::from_with_options(before, after, diff, node_cache, RenderOptions::FULL)
    }

    /// Builds both sides' ranges under `options`. Only the construction-time options are read
    /// here; the rest filter a built list in [`ranges_for_options`], so each preset whose
    /// construction-time options differ needs its own `TextDiff`.
    pub fn from_with_options(
        before: &Code,
        after: &Code,
        diff: &ASTDiff,
        node_cache: &NodeCache,
        options: RenderOptions,
    ) -> Self {
        let mut before_ranges_plain = ranges(before, after, diff, node_cache, true, options);
        let mut after_ranges_plain = ranges(after, before, diff, node_cache, false, options);

        reconcile_moves(&mut before_ranges_plain, &mut after_ranges_plain, options);

        let before_ranges = merge_ranges(&before_ranges_plain, &after_ranges_plain);
        let after_ranges = merge_ranges(&after_ranges_plain, &before_ranges_plain);

        Self {
            before_ranges,
            after_ranges,
        }
    }

    /// All ranges of `side` (0 is before, anything else after).
    pub fn all(&self, side: usize) -> Vec<RangeMatch> {
        if side == 0 {
            return self.before_ranges.clone();
        }
        self.after_ranges.clone()
    }
}

#[cfg(test)]
mod tests;
