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

//! Whole-diff summaries of painted ranges: per-line operations, change counts, and the
//! [`DiffSummary`] label.

use tree_sitter::Node;

use crate::code::Code;
use crate::diff::{ASTDiff, NodeCache, nodes};

use super::render_options::{RangeMatch, TextOperation};
use super::{NodeChange, classify_node};

/// One `TextOperation` per line, from one side's ranges (`TextDiff::all`), for line-based consumers
/// such as the headless plain-text renderer and the comparison against Unix `diff`.
///
/// Row-granular on purpose: the ranges are whitespace-insensitive and leave small gaps (leading
/// indentation), so each row takes the most specific operation of any range touching it rather
/// than sub-line spans. Zero-width placeholder ranges touch no row.
pub fn line_operations(ranges: &[RangeMatch], line_count: usize) -> Vec<TextOperation> {
    let mut ops = vec![TextOperation::Identical; line_count];
    for rm in ranges {
        let r = &rm.source;
        if r.is_empty() {
            continue;
        }
        // An end column of 0 already excludes the end row.
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
            // A non-`Identical` range wins over an `Identical` one on the same row whatever the
            // order, or the whitespace around a changed token would hide it.
            if rm.operation != TextOperation::Identical || *row_op == TextOperation::Identical {
                *row_op = rm.operation.clone();
            }
        }
    }
    ops
}

/// Line-level counts for a status-bar summary like `+12 -4 ~2`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChangeCounts {
    pub insertions: usize,
    pub deletions: usize,
    pub updates: usize,
    pub moves: usize,
}

/// Counts lines from each side's [`line_operations`]: deletions from the before side, and
/// insertions, updates and moves from the after side, since an updated or moved line exists on
/// both sides and must count once.
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

/// A label for a diff's overall shape, when it has a common one. A label, not a style: callers map
/// each variant to their own presentation. `CommentOnly` needs the AST and comes only from
/// [`summarize_diff_with_comment_check`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffSummary {
    /// The two sides are byte-identical. Not "no operations": a pure reformat can produce none,
    /// and that is `WhitespaceOnly`.
    NoChanges,
    /// Every range is an `Insert`.
    NewFile,
    /// Every range is a `Delete`.
    DeletedFile,
    /// Equal once all whitespace is removed, but not byte-identical, whatever the operations. A
    /// reformat can produce many `Move`s or none at all; only the stripped content tells it apart
    /// from a real reorder, which changes token order.
    WhitespaceOnly,
    /// Every real change touches only comments ([`is_comment_only_diff`]). Loses to `NewFile` and
    /// `DeletedFile`, beats `RefactorMovedOnly`.
    CommentOnly,
    /// Every non-`Identical` range is a `Move`. Loses to `WhitespaceOnly`, the more specific claim.
    RefactorMovedOnly,
}

impl DiffSummary {
    /// A short human-readable label.
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

/// Whether `a` and `b` are equal once all whitespace is removed. Runs on whole files, so it
/// compares iterators rather than building strings.
pub(crate) fn whitespace_stripped_equal(a: &str, b: &str) -> bool {
    a.chars()
        .filter(|c| !c.is_whitespace())
        .eq(b.chars().filter(|c| !c.is_whitespace()))
}

/// The [`DiffSummary`] of a diff, or `None` for the ordinary mix of edits. The ranges are
/// `TextDiff::all(0)`/`all(1)`; the contents are each side's full source. The most specific
/// case wins (see each variant).
pub fn summarize_diff(
    before_contents: &str,
    after_contents: &str,
    before_ranges: &[RangeMatch],
    after_ranges: &[RangeMatch],
) -> Option<DiffSummary> {
    // Content first: a reformat matched as one subtree whose start did not move paints a single
    // `Identical` range and no `Move`, and must still be `WhitespaceOnly`, not `NoChanges`.
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

    // One line added to an untouched file is not a new file: a real new file has nothing to be
    // `Identical` to.
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

/// Whether every real change between `before` and `after` touches only comments. `false` when
/// either side has no AST, and when nothing changed at all (e.g. only `Move`s): "comment-only" is a
/// claim about what changed.
///
/// A real change is what [`classify_node`] says `ranges` paints; containers of one are descended.
/// A node counts as a comment when it or an ancestor is one, because some grammars (Rust's
/// `line_comment`) build a comment from child nodes such as its `//` marker.
pub fn is_comment_only_diff(
    before: &Code,
    after: &Code,
    diff: &ASTDiff,
    node_cache: &NodeCache,
) -> bool {
    let (Some(before_ast), Some(after_ast)) = (before.ast.as_ref(), after.ast.as_ref()) else {
        return false;
    };

    // Walks `node_to_parent` because `Node::parent()` descends from the root on every call.
    fn is_comment_or_inside_comment(node_id: usize, meta: &crate::code::ASTMetadata) -> bool {
        let mut current = Some(node_id);
        while let Some(id) = current {
            if meta
                .node_info
                .get(&id)
                .is_some_and(|info| nodes::is_comment(&info.kind))
            {
                return true;
            }
            current = meta.node_to_parent.get(&id).copied();
        }
        false
    }

    // `(found any real change, every one was a comment)`. `other_bytes` is the mapped side's
    // source.
    fn scan(
        root: Node,
        diff: &ASTDiff,
        node_cache: &NodeCache,
        own_meta: &crate::code::ASTMetadata,
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
                if !is_comment_or_inside_comment(node.id(), own_meta) {
                    all_comments = false;
                }
            };

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
    let before_meta = crate::code::metadata::metadata_of(before);
    let after_meta = crate::code::metadata::metadata_of(after);
    let (before_found, before_all_comments) = scan(
        before_ast.root_node(),
        diff,
        node_cache,
        &before_meta,
        before_bytes,
        after_bytes,
    );
    let (after_found, after_all_comments) = scan(
        after_ast.root_node(),
        diff,
        node_cache,
        &after_meta,
        after_bytes,
        before_bytes,
    );

    (before_found || after_found)
        && (!before_found || before_all_comments)
        && (!after_found || after_all_comments)
}

/// [`summarize_diff`] plus [`DiffSummary::CommentOnly`], which overrides only `None` and
/// `RefactorMovedOnly`. Separate because `is_comment_only` needs the AST, which the flattened
/// ranges do not carry.
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
