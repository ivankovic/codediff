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

//! The line-level diff for files without a tree-sitter grammar.

use crate::diff::text_range::TextRange;

use super::render_options::{RangeMatch, TextOperation};
use super::{common_prefix_byte_len, common_suffix_byte_len};

/// The edit distance past which `myers_lcs` gives up and the whole file counts as replaced. Its
/// memory and work are quadratic in this only when the files really differ that much; an ordinary
/// edit stops at its actual distance. Far above `apted::common::FALLBACK_MAX_EDIT`, which bounds a
/// residual subtree forest rather than whole-file lines.
pub(crate) const PLAIN_TEXT_MAX_EDIT: usize = 10_000;

/// A line diff (Myers LCS over hashed lines) for files with no grammar, returning
/// `(before_ranges, after_ranges)` in the shape of [`super::TextDiff::all`].
///
/// Never produces `Move`: hashed lines carry no identity that survives relocation. Does produce
/// `Update` with sub-line columns: rows in a hunk that share enough affix (`shared_affix`) are
/// split by `intra_line_ranges` instead of rendering as a delete plus an insert.
pub fn plain_text_line_diff(before: &str, after: &str) -> (Vec<RangeMatch>, Vec<RangeMatch>) {
    plain_text_line_diff_with_max_edit(before, after, PLAIN_TEXT_MAX_EDIT)
}

/// [`plain_text_line_diff`] with the edit-distance cap as a parameter, so tests can reach the
/// give-up path cheaply.
pub(crate) fn plain_text_line_diff_with_max_edit(
    before: &str,
    after: &str,
    max_edit: usize,
) -> (Vec<RangeMatch>, Vec<RangeMatch>) {
    match line_diff_core(before, after, max_edit) {
        // Re-split rather than widen `LineDiffCore`, which the matching pipeline also uses and
        // which needs only counts and pairs.
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

/// The matched lines of a line diff, shared by [`plain_text_line_diff`] and the matching pipeline.
pub struct LineDiffCore {
    /// Matched `(before_row, after_row)` pairs, ascending in both.
    pub pairs: Vec<(usize, usize)>,
    pub before_line_count: usize,
    pub after_line_count: usize,
}

/// What kind of change a whole-file line diff is. Licenses a delete-free or insert-free resolver
/// downstream, so a class must never claim less than what changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WholeFileClass {
    /// No lines changed at all.
    Identical,
    /// Every before-line is matched (nothing deleted); at least one after-line is new.
    InsertOnly,
    /// Every after-line is matched (nothing inserted); at least one before-line is gone.
    DeleteOnly,
    /// Both insertions and deletions, or `myers_lcs` gave up: nothing safe can be licensed.
    Mixed,
}

impl LineDiffCore {
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

/// [`LineDiffCore::whole_file_class`] at `PLAIN_TEXT_MAX_EDIT`; `Mixed` when `myers_lcs` gives up.
pub fn whole_file_text_class(before: &str, after: &str) -> WholeFileClass {
    match line_diff_core(before, after, PLAIN_TEXT_MAX_EDIT) {
        Some(core) => core.whole_file_class(),
        None => WholeFileClass::Mixed,
    }
}

/// Runs `myers_lcs` over hashed lines. `None` when it gave up past `max_edit`, which callers must
/// not read as "no changes".
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

pub(crate) fn hash_lines(lines: &[&str]) -> Vec<u64> {
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

/// All of `row`, including its line break: `(row, 0)` to `(row + 1, 0)`.
pub(crate) fn whole_line_range(row: usize) -> TextRange {
    TextRange::new(row, 0, row + 1, 0)
}

/// The share of the longer of two unmatched lines their common prefix and suffix must cover for
/// `intra_line_ranges` to split them. A ratio, not a length: unrelated rows of a wide CSV share
/// long affixes like `,0,0,0,0` by coincidence, and splitting such a pair hides the real change
/// inside a span labelled `Identical`.
pub(crate) const MIN_SHARED_AFFIX_PERCENT: usize = 50;

/// Byte lengths of the common prefix and suffix of two unmatched lines, or `None` when they are
/// too dissimilar to be one line rewritten ([`MIN_SHARED_AFFIX_PERCENT`]) or are byte-identical.
/// The two never overlap.
pub(crate) fn shared_affix(before_line: &str, after_line: &str) -> Option<(usize, usize)> {
    let prefix = common_prefix_byte_len(before_line, after_line);
    let suffix = common_suffix_byte_len(&before_line[prefix..], &after_line[prefix..]);

    let longer = before_line.len().max(after_line.len());
    if longer == 0 || (prefix + suffix) * 100 < longer * MIN_SHARED_AFFIX_PERCENT {
        return None;
    }
    // `myers_lcs` can leave identical lines unmatched (reordered duplicates); pairing them here
    // would claim a match it deliberately did not make.
    if before_line.len() - suffix == prefix && after_line.len() - suffix == prefix {
        return None;
    }
    Some((prefix, suffix))
}

/// Sub-line ranges for one changed line pair: `Identical` prefix and suffix around an `Update`
/// middle. `None` under the same condition as `shared_affix`. Columns are bytes.
///
/// Both sides always get the same number of ranges, since the two lists are consumed
/// index-for-index downstream (see `merge_ranges`).
pub(crate) fn intra_line_ranges(
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

/// One `Identical` range per matched pair in `pairs`, and the gaps between them via [`emit_gap`].
///
/// An unmatched run's destination is anchored just past the other side's last match, as
/// `advance_and_build_range` does on the AST path: the two sides' row numbers diverge after any
/// earlier insert or delete.
pub(crate) fn build_line_ranges(
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

/// One unmatched run between two matches (or after the last one). A run with no rewritten row is
/// one merged `Delete` and one merged `Insert`, so a block reads and `n`/`p`-navigates as one
/// change. Otherwise it follows [`plan_gap`], still merging consecutive unpaired rows.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_gap(
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

    // `plan_gap` emits each side's rows in ascending order, so a run of them is contiguous.
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

/// One decision in a gap's plan: `Pair` is one line rewritten, the others whole-line changes.
pub(crate) enum GapOp {
    Pair(usize, usize),
    Delete(usize),
    Insert(usize),
}

/// How far [`plan_gap`] looks ahead on one side to resynchronise, i.e. the longest run of pure
/// insertions or deletions it can step over and still pair the rows after it. A gap's sides are
/// aligned only at their ends: rows inserted among rewritten rows (a sorted CSV) would otherwise end
/// the pairing for the rest of the gap. Bounded so the scan stays linear in the gap and an
/// unrelated block cannot find a spurious partner far away.
pub(crate) const GAP_RESYNC_WINDOW: usize = 16;

/// Decides which before-rows of one unmatched run are rewrites of which after-rows. Out of step,
/// it takes the nearest resynchronising position within [`GAP_RESYNC_WINDOW`] (insertions first on
/// a tie, for determinism) and emits the rows stepped over as plain inserts or deletes.
///
/// Pairs never cross: both range lists must stay in ascending row order for `merge_ranges` and
/// the cursor-follow logic.
pub(crate) fn plan_gap(
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
            // Replaced rather than rewritten.
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

/// The whole file as replaced, for when `myers_lcs` gives up. An empty side gets no range.
pub(crate) fn whole_file_replaced(
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
