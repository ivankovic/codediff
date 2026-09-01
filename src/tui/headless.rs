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
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Non-interactive counterpart to `tui::app`'s TUI: prints a diff as plain(ish) text instead of
//! drawing it. See `SPECS.md`'s "Git integration" entry for why this exists - a full-screen TUI
//! can't run when stdout isn't a real terminal (git's default pager for `GIT_EXTERNAL_DIFF`, a
//! redirected/piped invocation, CI, ...), so `main.rs` falls back to this whenever that's detected
//! or the caller explicitly asks for it (`--headless`/`--mode headless`).

use std::path::Path;

use anyhow::Result;

use crate::code::Code;
use crate::code::language::language_for_path_and_content;
use crate::diff::DiffMode;
use crate::diff::nodes::is_semantically_structural;
use crate::diff::text::{
    RangeMatch, RenderOptions, TextOperation, ranges_for_options, summarize_diff_with_comment_check,
};
use crate::tui::actions::DiffSessionData;
use crate::tui::app::compute_diff_with_update_style;

/// ANSI SGR color for each `TextOperation`, matching the TUI's own canonical palette
/// (`tui::theme::OverlayTheme`): insert green, delete red, move magenta, update yellow.
/// `Identical` (and the `NotYetSet` sentinel, which never survives into a real diff) are left
/// uncolored, same as the TUI's plain syntax-highlighted text.
fn ansi_color(operation: &TextOperation) -> Option<&'static str> {
    match operation {
        TextOperation::Insert => Some("32"),
        TextOperation::Delete => Some("31"),
        TextOperation::Move => Some("35"),
        TextOperation::Update => Some("33"),
        TextOperation::Identical | TextOperation::NotYetSet => None,
    }
}

/// Which of the four operation categories touch a given line, independently - more than one can
/// be true at once (e.g. a line that's both part of a moved block and has an updated token
/// within it).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct RowFlags {
    moved: bool,
    inserted: bool,
    deleted: bool,
    updated: bool,
}

impl RowFlags {
    fn any(&self) -> bool {
        self.moved || self.inserted || self.deleted || self.updated
    }
}

/// One row's colored column spans: `(start_col, end_col, operation)`, sorted by `start_col` and
/// non-overlapping (see `row_overlay`).
type RowSpans = Vec<(usize, usize, TextOperation)>;

/// Per-row diff overlay: which categories touch each line (`RowFlags`, for the marker columns),
/// and the exact colored column spans on each line (for inline highlighting) - both computed in
/// one pass over `ranges`, the same per-range-then-per-row walk `diff::text::line_operations`
/// uses, just keeping column precision instead of collapsing to one operation per row. Unlike
/// `line_operations`, this is column-precise: it reuses `TextRange::columns_on_row`, the same
/// span math the TUI's own `code_viewer::overlay_row` paints with, so headless and interactive
/// rendering agree on exactly which characters are highlighted, not just which lines.
///
/// `Move` only ever sets the flag, never a colored span: a moved line is conveyed by
/// `render_side`'s box (header/bar/footer, see `moved_chunk_destination`), not by inline color -
/// coloring the whole moved line's text on top of that box read as too busy in practice. Inline
/// coloring is reserved for `Insert`/`Delete`/`Update`, which don't have a box of their own.
///
/// Ranges are read via `rm.source` regardless of side, same convention as `line_operations`:
/// `before_ranges`' `.source` is already in before-side coordinates, `after_ranges`' `.source`
/// is already in after-side coordinates - each side's own `RangeMatch` list is self-referential.
fn row_overlay(ranges: &[RangeMatch], lines: &[&str]) -> (Vec<RowFlags>, Vec<RowSpans>) {
    let mut flags = vec![RowFlags::default(); lines.len()];
    let mut spans: Vec<RowSpans> = vec![Vec::new(); lines.len()];

    for rm in ranges {
        if rm.operation == TextOperation::Identical || rm.operation == TextOperation::NotYetSet {
            continue;
        }
        let r = &rm.source;
        if r.is_empty() || r.start_row >= lines.len() {
            continue;
        }
        let last_row = r.end_row.min(lines.len() - 1);
        for row in r.start_row..=last_row {
            // Trimmed, not the row's true length: a range that spans this row completely (it
            // isn't the range's own end row) should only color up to the last real character,
            // never a line's trailing whitespace or, past it, the newline - `colorize_line`
            // still prints that trailing text, just uncolored, via its own untrimmed tail append.
            let row_len = lines[row].trim_end().chars().count();
            let Some((start_col, end_col)) = r.columns_on_row(row, row_len) else {
                continue;
            };
            match rm.operation {
                TextOperation::Move => {
                    flags[row].moved = true;
                }
                TextOperation::Insert => {
                    flags[row].inserted = true;
                    spans[row].push((start_col, end_col, rm.operation.clone()));
                }
                TextOperation::Delete => {
                    flags[row].deleted = true;
                    spans[row].push((start_col, end_col, rm.operation.clone()));
                }
                TextOperation::Update => {
                    flags[row].updated = true;
                    spans[row].push((start_col, end_col, rm.operation.clone()));
                }
                TextOperation::Identical | TextOperation::NotYetSet => unreachable!(),
            }
        }
    }

    for row_spans in &mut spans {
        row_spans.sort_by_key(|&(start, _, _)| start);
    }

    (flags, spans)
}

/// Colors `ch` (or leaves it a plain space) for one marker column: blank when `present` is
/// false, otherwise `ch` in `op`'s ANSI color (when `use_color`) or plain.
fn marker_char(present: bool, ch: char, op: TextOperation, use_color: bool) -> String {
    if !present {
        return " ".to_string();
    }
    match ansi_color(&op).filter(|_| use_color) {
        Some(code) => format!("\u{1b}[{code}m{ch}\u{1b}[0m"),
        None => ch.to_string(),
    }
}

/// The 2-column line-marker prefix, followed by one separator space before the line text.
///
/// Column 1 is `|` when the line is part of a moved chunk - the same box `render_side` draws
/// around it (header/bar/footer) - blank otherwise. Column 2 reports the line's *other* changes,
/// in priority order: `~` if the line has any `Update` on it (whether alone or alongside an
/// `Insert`/`Delete` on the same line), else `-` for a pure delete, `+` for a pure insert, blank
/// if none of those apply. `Update` wins that priority because a line with an update in it is
/// never "purely" an insert or delete - flagging it `~` is strictly more informative than picking
/// one of the other two arbitrarily.
///
/// A four-column version of this (one column per category, always in the same position) was
/// tried first and rejected as too visually busy - two columns plus a moved-chunk box read far
/// calmer for the common case of a handful of scattered single-category changes.
///
/// `-` only ever appears on the before side and `+` only on the after side, by construction (see
/// `row_overlay`'s doc comment on why ranges are read from each side's own list) - not enforced
/// here, just a property of which flags can be set at all.
fn markers(flags: RowFlags, use_color: bool) -> String {
    let moved_col = marker_char(flags.moved, '|', TextOperation::Move, use_color);
    let op_col = if flags.updated {
        marker_char(true, '~', TextOperation::Update, use_color)
    } else if flags.deleted {
        marker_char(true, '-', TextOperation::Delete, use_color)
    } else if flags.inserted {
        marker_char(true, '+', TextOperation::Insert, use_color)
    } else {
        " ".to_string()
    };
    format!("{moved_col}{op_col} ")
}

/// How many dashes make up a moved chunk's closing footer line (see `render_side`) - a fixed
/// width by deliberate choice, not sized to the terminal or the chunk: it only needs to read as
/// "the box ends here," not to visually align with anything else.
const MOVED_CHUNK_FOOTER_WIDTH: usize = 20;

/// The destination-side line range (1-indexed, inclusive) covered by every `Move` range in
/// `ranges` whose *source* touches any row in `[start_row, end_row]` (inclusive) - the line
/// numbers `render_side`'s moved-chunk header reports. Merges every such range's destination into
/// one min/max span rather than tracking them individually: in the overwhelmingly common case a
/// contiguous moved run comes from one relocated subtree (one `Move` range, or several that all
/// point at essentially the same destination), so this is a simplification that only matters if
/// two *unrelated* moves happen to land on adjacent rows, in which case the reported range is
/// wider than either move alone - accepted rather than tracking sub-runs, since that's a rare
/// case and this is a display convenience, not a correctness-bearing computation.
///
/// `None` only if, somehow, no `Move` range actually touches this run - shouldn't happen, since
/// this is only ever called for a run of rows `row_overlay` already flagged `moved`.
fn moved_chunk_destination(
    ranges: &[RangeMatch],
    start_row: usize,
    end_row: usize,
) -> Option<(usize, usize)> {
    let last_touched_row = |r: &crate::diff::text_range::TextRange| -> usize {
        if r.end_column == 0 {
            r.end_row.saturating_sub(1)
        } else {
            r.end_row
        }
    };

    let mut span: Option<(usize, usize)> = None;
    for rm in ranges {
        if rm.operation != TextOperation::Move || rm.source.is_empty() {
            continue;
        }
        let source_last_row = last_touched_row(&rm.source);
        if source_last_row < start_row || rm.source.start_row > end_row {
            continue;
        }
        let dest_last_row = last_touched_row(&rm.destination);
        span = Some(match span {
            None => (rm.destination.start_row, dest_last_row),
            Some((lo, hi)) => (lo.min(rm.destination.start_row), hi.max(dest_last_row)),
        });
    }
    span.map(|(lo, hi)| (lo + 1, hi + 1))
}

/// Wraps each colored span of `line` (as computed by `row_overlay`) in its operation's ANSI
/// color, leaving the untouched characters between spans as plain text - genuine inline/per-hunk
/// highlighting rather than coloring the whole line by one dominant operation. `spans` are
/// non-overlapping and sorted by start column (ranges on one side never overlap by construction),
/// so a single left-to-right walk suffices.
fn colorize_line(line: &str, spans: &[(usize, usize, TextOperation)], use_color: bool) -> String {
    if !use_color || spans.is_empty() {
        return line.to_string();
    }
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut col = 0usize;
    for (start, end, op) in spans {
        let start = (*start).min(chars.len());
        let end = (*end).min(chars.len());
        if start > col {
            out.extend(&chars[col..start]);
        }
        if end > start {
            let segment: String = chars[start..end].iter().collect();
            match ansi_color(op) {
                Some(code) => out.push_str(&format!("\u{1b}[{code}m{segment}\u{1b}[0m")),
                None => out.push_str(&segment),
            }
        }
        col = col.max(end);
    }
    if col < chars.len() {
        out.extend(&chars[col..]);
    }
    out
}

/// How many unchanged lines to keep on either side of a change, same convention as `diff -u`'s
/// default `-U3`. A run of unchanged lines longer than twice this (enough for both changes
/// bordering it to keep their own context) gets its middle collapsed into a single elision marker
/// instead of printed in full - see `lines_to_keep`. This is only the *default*: the `--context N`
/// CLI flag (`main.rs`) overrides it per invocation, threaded through `run`/`render_text_diff`.
pub const CONTEXT_LINES: usize = 3;

/// Which line indices `render_side` should actually print: any line touched by at least one
/// operation category (`RowFlags::any`), plus `CONTEXT_LINES` lines on either side of one.
/// Everything else is a candidate to collapse into an elision marker - this is what fixes
/// headless mode printing entire unchanged files twice (once per side) for a single-line change.
fn lines_to_keep(flags: &[RowFlags], context: usize) -> Vec<bool> {
    let mut keep = vec![false; flags.len()];
    for (i, f) in flags.iter().enumerate() {
        if f.any() {
            let start = i.saturating_sub(context);
            let end = (i + context + 1).min(flags.len());
            keep[start..end].fill(true);
        }
    }
    keep
}

/// Finds the row of the nearest enclosing (or self) named declaration for `row` - the same idea
/// as `git diff`'s `@@ ... @@ enclosing_function` hunk header, but using this project's own
/// language-aware AST classification instead of a regex heuristic. Walks up from the smallest
/// node covering `row` until [`is_semantically_structural`] matches.
///
/// Deliberately `is_semantically_structural`, not the broader `is_reference`: the latter also
/// includes nodes that are reference points for the *diff-matching* pipeline specifically (e.g.
/// Rust's `if_expression`), which would surface "you're inside this `if` block" as the landmark
/// instead of the enclosing function - not what a human orienting themselves in a hunk wants.
/// `is_semantically_structural` only matches nodes with an actual name (functions, structs,
/// classes, impls, ...), which is a closer match for "parts of code humans think about."
///
/// `pub(crate)`, not private: `tui::json_output` reuses this directly too, for the same
/// "which enclosing function is this hunk in" breadcrumb, just serialized as a field instead of
/// printed as an `@` line - both callers want the identical AST walk, not a re-derived copy of it.
pub(crate) fn nearest_reference_line(
    code: &Code,
    language: &crate::code::Language,
    row: usize,
) -> Option<usize> {
    let root = code.ast.as_ref()?.root_node();
    let point = tree_sitter::Point::new(row, 0);
    let mut node = root.descendant_for_point_range(point, point)?;
    loop {
        if is_semantically_structural(&node, language, code).is_some() {
            return Some(node.start_position().row);
        }
        node = node.parent()?;
    }
}

/// Renders one side (before or after) of a diff as colored, marker-prefixed lines, collapsing
/// runs of unchanged lines beyond `CONTEXT_LINES` into a single elision marker (dimmed, when
/// colored) rather than printing every line of an otherwise-untouched file. Also prefixes each
/// hunk (the first kept line after a gap, or line 0) with the nearest enclosing reference node's
/// own line (`nearest_reference_line`), marked with an `@` prefix, if that line wouldn't already
/// be visible in the hunk itself - e.g. jumping straight to line 340 inside a 20-line function
/// still shows you which function that is, even though its `fn foo(...) {` line is out of range
/// of `CONTEXT_LINES`.
///
/// Column-precise, same as the TUI: each line gets a 2-column marker (see `markers`) plus, for a
/// moved chunk, a surrounding box - a `Moved from`/`Moved to` header naming the other side's line
/// range, a `|` bar down every line of the chunk, and a dashed footer closing it. Only the exact
/// changed sub-spans of `Insert`/`Delete`/`Update` lines are colored inline (see `colorize_line`),
/// not the whole line by one dominant operation - all of it driven by `row_overlay`, which walks
/// `ranges` directly instead of the coarser, one-op-per-row `diff::text::line_operations`.
///
/// `side_is_before` only affects the moved-chunk header's wording (`Moved to` vs. `Moved from`) -
/// see `moved_chunk_destination`.
///
/// Re-parses `contents` from scratch to get a real tree-sitter tree to walk - `DiffSessionData`
/// only carries flattened text ranges, not the AST the original diff computation already parsed,
/// and duplicating that parse here (rather than threading the AST through the diff pipeline just
/// for this) keeps this purely a headless-rendering concern. Headless mode isn't on any hot path,
/// so the redundant parse is an acceptable trade for that isolation.
fn render_side(
    contents: &str,
    ranges: &[RangeMatch],
    side_is_before: bool,
    use_color: bool,
    path: &Path,
    context: usize,
) -> String {
    let lines: Vec<&str> = contents.split('\n').collect();
    let (flags, spans) = row_overlay(ranges, &lines);
    let keep = lines_to_keep(&flags, context);

    let language = language_for_path_and_content(path, contents);
    let parsed = language.map(|lang| Code::from_string(contents, &lang));

    // Every printed content line (and the `@` breadcrumb) is prefixed with its 1-indexed line
    // number, right-aligned to the file's widest number and dimmed when colored - without it, the
    // moved-chunk headers' "Moved to lines 40-60" cross-references were unresolvable by eye in an
    // output stream that showed no numbers at all.
    let number_width = lines.len().to_string().len();

    let mut out = String::new();
    let mut i = 0;
    let mut prev_line_shown = false;
    while i < lines.len() {
        if !keep[i] {
            let run_start = i;
            while i < lines.len() && !keep[i] {
                i += 1;
            }
            let skipped = i - run_start;
            let elision = format!(
                "      ... {skipped} unchanged line{} ...",
                if skipped == 1 { "" } else { "s" }
            );
            if use_color {
                out.push_str(&format!("\u{1b}[90m{elision}\u{1b}[0m\n"));
            } else {
                out.push_str(&elision);
                out.push('\n');
            }
            prev_line_shown = false;
            continue;
        }

        if !prev_line_shown {
            if let (Some(parsed), Some(lang)) = (&parsed, &language) {
                if let Some(ref_row) = nearest_reference_line(parsed, lang, i) {
                    // Only worth showing if it isn't already going to be visible in this hunk
                    // (or was already shown, or will be, as part of some other kept line).
                    if !keep[ref_row] {
                        let breadcrumb =
                            format!("{:>number_width$} @ {}", ref_row + 1, lines[ref_row]);
                        if use_color {
                            out.push_str(&format!("\u{1b}[90m{breadcrumb}\u{1b}[0m\n"));
                        } else {
                            out.push_str(&breadcrumb);
                            out.push('\n');
                        }
                    }
                }
            }
        }

        let starts_moved_chunk = flags[i].moved && (i == 0 || !flags[i - 1].moved);
        if starts_moved_chunk {
            let mut run_end = i;
            while run_end + 1 < flags.len() && flags[run_end + 1].moved {
                run_end += 1;
            }
            let verb = if side_is_before { "to" } else { "from" };
            let header = match moved_chunk_destination(ranges, i, run_end) {
                Some((start, end)) if start == end => format!("Moved {verb} line {start}"),
                Some((start, end)) => format!("Moved {verb} lines {start}-{end}"),
                None => format!("Moved {verb} elsewhere"),
            };
            if use_color {
                out.push_str(&format!("\u{1b}[35m{header}\u{1b}[0m\n"));
            } else {
                out.push_str(&header);
                out.push('\n');
            }
        }

        let number = format!("{:>number_width$} ", i + 1);
        if use_color {
            out.push_str(&format!("\u{1b}[90m{number}\u{1b}[0m"));
        } else {
            out.push_str(&number);
        }
        let prefix = markers(flags[i], use_color);
        let colored_line = colorize_line(lines[i], &spans[i], use_color);
        out.push_str(&prefix);
        out.push_str(&colored_line);
        out.push('\n');

        let ends_moved_chunk = flags[i].moved && (i + 1 == lines.len() || !flags[i + 1].moved);
        if ends_moved_chunk {
            let footer = "-".repeat(MOVED_CHUNK_FOOTER_WIDTH);
            if use_color {
                out.push_str(&format!("\u{1b}[35m{footer}\u{1b}[0m\n"));
            } else {
                out.push_str(&footer);
                out.push('\n');
            }
        }

        prev_line_shown = true;
        i += 1;
    }
    out
}

/// Prefixes `out` with the diff's overall shape (`diff::text::DiffSummary`) - the same
/// classification the interactive TUI shows in its status bar (`tui::app::status_bar_paragraph`),
/// reused here so a script running headless mode gets the same "no changes"/"comment changes
/// only"/... heads-up a person watching the TUI would, without having to re-derive it from the
/// rendered hunks. Bold, not colored: `use_color` off (piped output, `NO_COLOR`, ...) must still
/// carry *some* visual weight for a line meant to be noticed before the diff itself, and bold
/// degrades gracefully in a way an ANSI color code doesn't. Only emitted when
/// `summarize_diff_with_comment_check` actually classifies the diff (`Some`) - the ordinary case
/// (a genuine mix of edits) prints nothing extra, so this is purely additive over the previous
/// output shape.
fn summary_header(data: &DiffSessionData, use_color: bool) -> Option<String> {
    let summary = summarize_diff_with_comment_check(
        &data.before_contents,
        &data.after_contents,
        &data.before_ranges,
        &data.after_ranges,
        data.comment_only,
    )?;
    let label = summary.label();
    Some(if use_color {
        format!("\u{1b}[1m{label}\u{1b}[0m\n\n")
    } else {
        format!("{label}\n\n")
    })
}

/// Renders a full diff session as plain text: the "before" side (deletions/updates/moves
/// highlighted), then the "after" side (insertions/updates/moves highlighted). Each side's own
/// `RangeMatch` list already carries which rows/columns changed - see `render_side`'s doc comment
/// for why this is column-precise, not row-granular.
pub(crate) fn render_text_diff(data: &DiffSessionData, use_color: bool, context: usize) -> String {
    let mut out = String::new();
    if let Some(header) = summary_header(data, use_color) {
        out.push_str(&header);
    }
    out.push_str(&format!("=== before: {} ===\n", data.before_path.display()));
    out.push_str(&render_side(
        &data.before_contents,
        &data.before_ranges,
        true,
        use_color,
        &data.before_path,
        context,
    ));
    out.push_str(&format!("=== after: {} ===\n", data.after_path.display()));
    out.push_str(&render_side(
        &data.after_contents,
        &data.after_ranges,
        false,
        use_color,
        &data.after_path,
        context,
    ));
    out
}

/// Entry point for headless/text-mode operation (`main.rs`): computes the diff exactly like the
/// TUI does (`app::compute_diff` - same parsing, same `ASTDiff`, same `TextDiff`), then prints it
/// as text on stdout instead of drawing an interactive terminal UI.
///
/// Under `DiffMode::Fast` (the default), if the diff left an unusually large unmatched residual
/// (`PendingDiff::looks_expensive`), a one-line note goes to stderr (plain `eprintln!`, not
/// `tracing` - headless mode never calls `tui::initialize_logging`, so there's no subscriber
/// installed to receive it) so a script invoking this isn't left wondering why the structural
/// matching looks coarser than usual for that portion.
/// Returns whether the two files differ at all (byte comparison) so `main.rs` can turn it into a
/// `diff`-style exit code - the rendered output itself is unaffected by the return value.
pub fn run(
    before: &Path,
    after: &Path,
    use_color: bool,
    mode: DiffMode,
    context: usize,
    render_options: RenderOptions,
) -> Result<bool> {
    let (mut data, fallback_used) =
        compute_diff_with_update_style(
            before,
            after,
            mode,
            render_options.whole_pair_updates,
            render_options.paint_reindent_only_moves,
        )?;
    // Applied here rather than inside `compute_diff`: the mapping is identical under every set of
    // options, so this is a presentation filter over a finished diff, not a different diff.
    data.before_ranges =
        ranges_for_options(&data.before_ranges, &data.before_contents, render_options);
    data.after_ranges =
        ranges_for_options(&data.after_ranges, &data.after_contents, render_options);
    if fallback_used {
        // Deliberately no "--exact fixes this" suggestion: since the phases-4-7
        // rearchitecture, `PendingDiff::finish` runs the same pipeline regardless of mode, so
        // that flag would not change the result - this is a heads-up about input shape, not a
        // pointer to a more precise alternative.
        eprintln!(
            "codediff: this diff left an unusually large unmatched residual after the \
             heuristic passes; the structural matching for that portion may be coarser than \
             usual."
        );
    }
    print!("{}", render_text_diff(&data, use_color, context));
    // Raw on-disk bytes, not `data`'s contents: those have been through `display_safe` (tabs
    // replaced by spaces for terminal rendering), which would make a tab-vs-space-only difference
    // compare equal here.
    Ok(std::fs::read(before)? != std::fs::read(after)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::compute_diff;
    use std::path::PathBuf;

    #[test]
    fn lines_to_keep_keeps_context_lines_around_a_change_and_nothing_else() {
        // 10 lines: only line 5 (index 5) changed. With context=2, indices 3..=7 should be kept.
        let mut flags = vec![RowFlags::default(); 10];
        flags[5].inserted = true;

        let keep = lines_to_keep(&flags, 2);
        assert_eq!(
            keep,
            vec![
                false, false, false, true, true, true, true, true, false, false
            ]
        );
    }

    #[test]
    fn lines_to_keep_merges_context_windows_of_nearby_changes() {
        // Changes at indices 2 and 6, context=2: windows [0,5) and [4,9) overlap at 4, so
        // everything from 0 to 8 merges into one kept run with nothing collapsed between.
        let mut flags = vec![RowFlags::default(); 10];
        flags[2].deleted = true;
        flags[6].inserted = true;

        let keep = lines_to_keep(&flags, 2);
        assert_eq!(
            keep,
            vec![true, true, true, true, true, true, true, true, true, false]
        );
    }

    /// This is the actual regression case: a large file with one small change used to print
    /// every unchanged line on both sides in full - see this module's own history for a real
    /// fixture (`c-microsoft-terminal-add-function`) where a 1-line change produced 981 lines of
    /// output. Reproduced synthetically here with a controlled line count.
    #[test]
    fn render_side_collapses_a_long_run_of_unchanged_lines() {
        let mut lines: Vec<String> = (0..50).map(|i| format!("line{i}")).collect();
        lines[25] = "changed".to_string();
        let contents = lines.join("\n");

        let ranges = vec![RangeMatch {
            source: crate::diff::text_range::TextRange::new(25, 0, 26, 0),
            destination: crate::diff::text_range::TextRange::new(25, 0, 26, 0),
            operation: TextOperation::Delete,
        }];

        let rendered = render_side(
            &contents,
            &ranges,
            true,
            false,
            Path::new("plain.txt"),
            CONTEXT_LINES,
        );
        let line_count = rendered.lines().count();
        assert!(
            line_count < 15,
            "50 lines with 1 change should collapse to well under 15 output lines, got \
             {line_count}:\n{rendered}"
        );
        assert!(rendered.contains(" - changed"));
        assert!(rendered.contains("unchanged lines"));
        // Context lines immediately around the change must still be shown in full, prefixed with
        // their 1-indexed line numbers (line index 22 is line number 23).
        assert!(rendered.contains("23    line22"));
        assert!(rendered.contains("29    line28"));
        // But nothing further out should survive.
        assert!(!rendered.contains("line10\n"));
    }

    /// A range spanning a row completely (it isn't the range's own end row) is colored only up to
    /// the row's last real character - never its trailing whitespace, and least of all the
    /// newline past it. `colorize_line` still prints that trailing text in full, just uncolored.
    #[test]
    fn row_overlay_does_not_color_a_middle_rows_trailing_whitespace() {
        let lines = ["foo   ", "bar"];
        let ranges = vec![RangeMatch {
            source: crate::diff::text_range::TextRange::new(0, 0, 1, 3),
            destination: crate::diff::text_range::TextRange::zero(),
            operation: TextOperation::Update,
        }];

        let (_, spans) = row_overlay(&ranges, &lines);

        assert_eq!(
            spans[0],
            vec![(0, 3, TextOperation::Update)],
            "row 0's span must stop at 'foo', not run through its trailing spaces"
        );

        let colored = colorize_line(lines[0], &spans[0], true);
        // The trailing spaces are still present in full, just outside any ANSI escape.
        assert!(
            colored.ends_with("   "),
            "trailing whitespace must survive uncolored, not be dropped: {colored:?}"
        );
        assert_eq!(colored, "\u{1b}[33mfoo\u{1b}[0m   ");
    }

    /// No other test exercises `Move` directly - real move-detection heuristics didn't fire in
    /// manual smoke testing on small synthetic files, so this builds the `RangeMatch` by hand
    /// (with a `destination` in some other line range) instead of depending on the diff engine's
    /// move classification.
    #[test]
    fn render_side_wraps_a_moved_chunk_in_a_box_with_header_bar_and_footer() {
        let contents = "fn main() {\n    moved_call();\n    same();\n}";
        let ranges = vec![RangeMatch {
            source: crate::diff::text_range::TextRange::new(1, 4, 2, 0),
            destination: crate::diff::text_range::TextRange::new(9, 0, 10, 0),
            operation: TextOperation::Move,
        }];
        let footer = "-".repeat(MOVED_CHUNK_FOOTER_WIDTH);

        // Rendered as the after side: this line came FROM before-line 10 (1-indexed).
        let plain = render_side(
            contents,
            &ranges,
            false,
            false,
            Path::new("plain.txt"),
            CONTEXT_LINES,
        );
        assert!(
            plain.contains("Moved from line 10\n"),
            "should announce where the moved chunk came from: {plain}"
        );
        assert!(
            plain.contains(&format!("\n{footer}\n")),
            "should close the box with a dashed footer: {plain}"
        );
        assert!(
            plain.contains("|      moved_call();"),
            "the moved line should start with the box bar: {plain}"
        );

        // Rendered as the before side, the same range reads as a destination, not an origin.
        let before_plain = render_side(
            contents,
            &ranges,
            true,
            false,
            Path::new("plain.txt"),
            CONTEXT_LINES,
        );
        assert!(
            before_plain.contains("Moved to line 10\n"),
            "before-side wording should say where it went, not where it came from: \
             {before_plain}"
        );

        let colored = render_side(
            contents,
            &ranges,
            false,
            true,
            Path::new("plain.txt"),
            CONTEXT_LINES,
        );
        assert!(
            colored.contains("\u{1b}[35mMoved from line 10\u{1b}[0m"),
            "the header should be magenta: {colored}"
        );
        assert!(
            colored.contains(&format!("\u{1b}[35m{footer}\u{1b}[0m")),
            "the footer should be magenta: {colored}"
        );
        assert!(
            colored.contains("\u{1b}[35m|\u{1b}[0m"),
            "the box bar should be magenta: {colored}"
        );
        // A purely-moved line's own text must NOT be inline-colored - only Insert/Delete/Update
        // get that treatment; Move is conveyed purely by the box.
        assert!(
            !colored.contains("\u{1b}[35mmoved_call();"),
            "a purely-moved line's text should stay plain, not inline-colored: {colored}"
        );
    }

    /// Builds a Rust file with a change deep inside a function whose own `fn` line is well
    /// outside `CONTEXT_LINES`, plus a nested `if` between the function line and the change -
    /// `if_expression` is one of Rust's `is_reference` kinds (used for diff-matching anchors),
    /// but must NOT be what `nearest_reference_line` reports here (see that function's doc
    /// comment on why `is_semantically_structural`, not `is_reference`, is used).
    fn rust_file_with_a_change_buried_in_a_function() -> (String, Vec<RangeMatch>) {
        let lines: Vec<&str> = vec![
            "fn unrelated_helper() {",
            "    println!(\"noise\");",
            "}",
            "",
            "fn parse_args(args: &[String]) -> Vec<String> {",
            "    let mut result = Vec::new();",
            "    for arg in args {",
            "        if arg.starts_with(\"--\") {",
            "            result.push(arg.clone());",
            "        }",
            "        if arg.starts_with(\"-x\") {",
            "            result.push(format!(\"expanded-{arg}\"));",
            "        }",
            "    }",
            "    result",
            "}",
        ];
        let changed_row = 11;
        let ranges = vec![RangeMatch {
            source: crate::diff::text_range::TextRange::new(changed_row, 0, changed_row + 1, 0),
            destination: crate::diff::text_range::TextRange::new(
                changed_row,
                0,
                changed_row + 1,
                0,
            ),
            operation: TextOperation::Delete,
        }];
        (lines.join("\n"), ranges)
    }

    #[test]
    fn nearest_reference_line_finds_the_enclosing_function_not_the_nearest_if() {
        let (contents, _) = rust_file_with_a_change_buried_in_a_function();
        let code = Code::from_string(&contents, &crate::code::Language::Rust);

        // Row 11 is the changed line itself, several rows below both the enclosing `if` (row 7)
        // and the enclosing `fn` (row 4).
        let ref_row = nearest_reference_line(&code, &crate::code::Language::Rust, 11)
            .expect("a Rust function should be found enclosing this row");
        assert_eq!(
            ref_row, 4,
            "should find `fn parse_args`, not the nearer `if`"
        );
    }

    #[test]
    fn render_side_shows_the_enclosing_function_as_a_breadcrumb_when_out_of_context() {
        let (contents, ranges) = rust_file_with_a_change_buried_in_a_function();
        let rendered = render_side(
            &contents,
            &ranges,
            true,
            false,
            Path::new("sample.rs"),
            CONTEXT_LINES,
        );

        assert!(
            rendered.contains("@ fn parse_args"),
            "should surface the enclosing function as a breadcrumb: {rendered}"
        );
        // The breadcrumb carries its own line number (`fn parse_args` is row 4, so line 5).
        assert!(
            rendered.contains("5 @ fn parse_args"),
            "the breadcrumb should be prefixed with its 1-indexed line number: {rendered}"
        );
        let breadcrumb_lines = rendered.lines().filter(|l| l.contains(" @ "));
        for line in breadcrumb_lines {
            assert!(
                !line.contains("if arg.starts_with"),
                "must not surface the nearer `if_expression` as the breadcrumb: {line}"
            );
        }
    }

    /// A synthetic 4-line diff (one line changed, modeled as a Delete on the before side paired
    /// with an Insert on the after side - each side's renderer only ever looks at its own list, so
    /// this doesn't need to be a real `TextDiff::from` output, just internally consistent per side).
    fn sample_data() -> DiffSessionData {
        DiffSessionData {
            before_path: PathBuf::from("before.rs"),
            after_path: PathBuf::from("after.rs"),
            before_contents: "fn main() {\n    old_call();\n    same();\n}".to_string(),
            after_contents: "fn main() {\n    new_call();\n    same();\n}".to_string(),
            before_ranges: vec![
                RangeMatch {
                    source: crate::diff::text_range::TextRange::new(0, 0, 1, 0),
                    destination: crate::diff::text_range::TextRange::new(0, 0, 1, 0),
                    operation: TextOperation::Identical,
                },
                RangeMatch {
                    source: crate::diff::text_range::TextRange::new(1, 4, 2, 0),
                    destination: crate::diff::text_range::TextRange::new(1, 4, 2, 0),
                    operation: TextOperation::Delete,
                },
                RangeMatch {
                    source: crate::diff::text_range::TextRange::new(2, 0, 4, 0),
                    destination: crate::diff::text_range::TextRange::new(2, 0, 4, 0),
                    operation: TextOperation::Identical,
                },
            ],
            after_ranges: vec![
                RangeMatch {
                    source: crate::diff::text_range::TextRange::new(0, 0, 1, 0),
                    destination: crate::diff::text_range::TextRange::new(0, 0, 1, 0),
                    operation: TextOperation::Identical,
                },
                RangeMatch {
                    source: crate::diff::text_range::TextRange::new(1, 4, 2, 0),
                    destination: crate::diff::text_range::TextRange::new(1, 4, 2, 0),
                    operation: TextOperation::Insert,
                },
                RangeMatch {
                    source: crate::diff::text_range::TextRange::new(2, 0, 4, 0),
                    destination: crate::diff::text_range::TextRange::new(2, 0, 4, 0),
                    operation: TextOperation::Identical,
                },
            ],
            comment_only: false,
            mode: DiffMode::Fast,
            plain_text_fallback: false,
        }
    }

    /// The line-number-plus-2-column-marker prefix a plain (uncolored) line with these flags
    /// should get - built independently of `markers()` itself, from the documented column order,
    /// rather than calling the function under test. `n` is the 1-indexed line number (the sample
    /// files are under 10 lines, so the number column is 1 character wide); `op` is the marker's
    /// column-2 character (`None` for a plain space).
    fn plain_prefix(n: usize, moved: bool, op: Option<char>) -> String {
        format!(
            "{n} {}{} ",
            if moved { "|" } else { " " },
            op.map(String::from).unwrap_or_else(|| " ".to_string()),
        )
    }

    /// Builds the expected plain-text rendering by concatenating each line's number+marker with
    /// its original text directly, rather than a hand-typed literal - counting the exact
    /// whitespace a marker prefix plus a 4-space-indented line produces by eye is error-prone.
    fn expected_plain_text() -> String {
        let none = |n: usize| plain_prefix(n, false, None);
        let deleted = plain_prefix(2, false, Some('-'));
        let inserted = plain_prefix(2, false, Some('+'));
        format!(
            "=== before: before.rs ===\n{}fn main() {{\n{deleted}    old_call();\n\
             {}    same();\n{}}}\n\
             === after: after.rs ===\n{}fn main() {{\n{inserted}    new_call();\n\
             {}    same();\n{}}}\n",
            none(1),
            none(3),
            none(4),
            none(1),
            none(3),
            none(4),
        )
    }

    #[test]
    fn render_text_diff_without_color_shows_both_sides_with_markers() {
        assert_eq!(
            render_text_diff(&sample_data(), false, CONTEXT_LINES),
            expected_plain_text()
        );
    }

    #[test]
    fn render_text_diff_with_color_highlights_only_the_changed_substring_inline() {
        let text = render_text_diff(&sample_data(), true, CONTEXT_LINES);

        // The deleted marker column ('-') and the changed substring are both red - but the
        // leading 4-space indent, part of the same line, is NOT wrapped: this is genuine
        // inline/per-hunk coloring, not whole-line coloring.
        assert!(
            text.contains("\u{1b}[31m-\u{1b}[0m"),
            "the deleted marker column should be red: {text}"
        );
        assert!(
            text.contains("    \u{1b}[31mold_call();\u{1b}[0m"),
            "only the changed substring, not the leading indent, should be wrapped in red: {text}"
        );

        // Same shape on the after side, in green for Insert.
        assert!(
            text.contains("\u{1b}[32m+\u{1b}[0m"),
            "the inserted marker column should be green: {text}"
        );
        assert!(
            text.contains("    \u{1b}[32mnew_call();\u{1b}[0m"),
            "only the changed substring, not the leading indent, should be wrapped in green: {text}"
        );

        // Identical lines must stay uncolored, markers included - the only escape sequence on
        // them is the dimmed line-number gutter itself.
        assert!(
            text.contains("\u{1b}[90m1 \u{1b}[0m   fn main() {\n"),
            "identical lines must stay uncolored apart from the dimmed number gutter: {text}"
        );
    }

    #[test]
    fn run_prints_a_readable_diff_for_two_real_files() -> Result<()> {
        let dir = tempfile::tempdir().expect("create temp dir");
        let before_path = dir.path().join("before_sample.rs");
        let after_path = dir.path().join("after_sample.rs");
        std::fs::write(&before_path, "fn main() {\n    old();\n}\n").unwrap();
        std::fs::write(&after_path, "fn main() {\n    new();\n}\n").unwrap();

        let (data, _fallback_used) = compute_diff(&before_path, &after_path, DiffMode::Fast)?;
        let text = render_text_diff(&data, false, CONTEXT_LINES);

        assert!(text.contains("old();"), "before content missing: {text}");
        assert!(text.contains("new();"), "after content missing: {text}");
        Ok(())
    }

    #[test]
    fn render_text_diff_omits_the_summary_header_for_an_ordinary_mixed_edit() {
        // Same synthetic data headless.rs's other rendering tests use - a real Delete+Insert pair
        // alongside Identical ranges, which shouldn't classify as any of `DiffSummary`'s special
        // cases (see this file's `summary_header`).
        let text = render_text_diff(&sample_data(), false, CONTEXT_LINES);
        assert!(
            text.starts_with("=== before:"),
            "an ordinary mixed edit should get no summary header at all: {text}"
        );
    }

    #[test]
    fn render_text_diff_shows_a_no_changes_header_for_identical_files() -> Result<()> {
        let dir = tempfile::tempdir().expect("create temp dir");
        let before_path = dir.path().join("before_sample.rs");
        let after_path = dir.path().join("after_sample.rs");
        std::fs::write(&before_path, "fn main() {\n    same();\n}\n").unwrap();
        std::fs::write(&after_path, "fn main() {\n    same();\n}\n").unwrap();

        let (data, _fallback_used) = compute_diff(&before_path, &after_path, DiffMode::Fast)?;
        let text = render_text_diff(&data, false, CONTEXT_LINES);

        assert!(
            text.starts_with(crate::diff::text::DiffSummary::NoChanges.label()),
            "identical files should get a 'no changes' header: {text}"
        );
        Ok(())
    }

    #[test]
    fn render_text_diff_shows_a_comment_only_header_when_only_a_comment_changed() -> Result<()> {
        // Same fixture `tui::app`'s own `compute_diff_reports_comment_only_for_a_real_inserted_
        // comment` test uses, for the same real-pipeline (not hand-built `DiffSessionData`)
        // guarantee that `comment_only` actually reaches this header.
        let before = tempfile::Builder::new()
            .suffix(".rs")
            .tempfile()
            .expect("create temp file");
        std::fs::write(before.path(), "fn main() {}\n").expect("write temp file");
        let after = tempfile::Builder::new()
            .suffix(".rs")
            .tempfile()
            .expect("create temp file");
        std::fs::write(after.path(), "// a comment\nfn main() {}\n").expect("write temp file");

        let (data, _fallback_used) = compute_diff(before.path(), after.path(), DiffMode::Exact)?;
        let text = render_text_diff(&data, false, CONTEXT_LINES);

        assert!(
            text.starts_with(crate::diff::text::DiffSummary::CommentOnly.label()),
            "a comment-only edit should get a 'comment changes only' header: {text}"
        );
        Ok(())
    }

    #[test]
    fn render_text_diff_bolds_the_summary_header_when_colored() {
        let mut data = sample_data();
        data.before_contents = "same\n".to_string();
        data.after_contents = "same\n".to_string();
        data.before_ranges = vec![RangeMatch {
            source: crate::diff::text_range::TextRange::new(0, 0, 1, 0),
            destination: crate::diff::text_range::TextRange::new(0, 0, 1, 0),
            operation: TextOperation::Identical,
        }];
        data.after_ranges = data.before_ranges.clone();

        let text = render_text_diff(&data, true, CONTEXT_LINES);
        assert!(
            text.starts_with("\u{1b}[1mNo changes - files are identical\u{1b}[0m\n\n"),
            "the header should be bold, not colored, when use_color is on: {text}"
        );
    }
}
