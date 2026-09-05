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

// Split out of text.rs (its post-hoc diff summarization/analytics, consumed by the TUI
// header/footer - a distinct concern from rendering itself) purely to shrink that file's
// visible size. No behavior change.

use tree_sitter::Node;

use crate::code::Code;
use crate::diff::{ASTDiff, NodeCache, nodes};

use super::render_options::{RangeMatch, TextOperation};
use super::{NodeChange, classify_node};

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
pub(crate) fn whitespace_stripped_equal(a: &str, b: &str) -> bool {
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

            // The same classification `ranges` paints from, so "is this a visible change" and
            // "what gets painted" cannot drift apart.
            if let Some((mapped_id, mapping)) = diff.mapping_for_node(&node.id()) {
                match classify_node(
                    node,
                    mapped_id,
                    &mapping.operation,
                    node_cache,
                    own_bytes,
                    other_bytes,
                ) {
                    NodeChange::Identical(_) => descend = false,
                    NodeChange::PrunedSubtree(_) | NodeChange::OwnContentChanged(_) => {
                        descend = false;
                        mark_found();
                    }
                    NodeChange::Leaf(_) | NodeChange::Update(_) => mark_found(),
                    NodeChange::Descend => {}
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
