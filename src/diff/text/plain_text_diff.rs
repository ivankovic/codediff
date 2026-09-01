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

// Split out of text.rs (its plain-text/Myers line-diff fallback path, used for non-AST /
// non-code files) purely to shrink that file's visible size. No behavior change.

use crate::diff::text_range::TextRange;

use super::render_options::{RangeMatch, TextOperation};
use super::{common_prefix_byte_len, common_suffix_byte_len};

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
pub(crate) const PLAIN_TEXT_MAX_EDIT: usize = 10_000;

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
pub(crate) fn plain_text_line_diff_with_max_edit(
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

/// One whole line as a `TextRange`: `(row, 0)` to `(row + 1, 0)` - this module's convention for
/// "all of row, including its own line break" (see `text_range.rs`'s doc comment on referring to
/// a full row via `(row + 1, 0)`).
pub(crate) fn whole_line_range(row: usize) -> TextRange {
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
pub(crate) const MIN_SHARED_AFFIX_PERCENT: usize = 50;

/// Byte lengths of the common prefix and suffix of two unmatched lines, or `None` if they are too
/// dissimilar to be treated as one line rewritten (see [`MIN_SHARED_AFFIX_PERCENT`]). Split out
/// from [`intra_line_ranges`] because [`plan_gap`] needs the *decision* while it is still choosing
/// which lines pair with which, and only pays for the ranges once that is settled.
///
/// The suffix is measured on the already-prefix-trimmed remainders (exactly as
/// `intra_node_update_ranges` does), so prefix + suffix can never overlap on the shorter line.
pub(crate) fn shared_affix(before_line: &str, after_line: &str) -> Option<(usize, usize)> {
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
pub(crate) enum GapOp {
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
pub(crate) const GAP_RESYNC_WINDOW: usize = 16;

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
