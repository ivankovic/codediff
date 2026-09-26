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

//! Non-interactive counterpart to the TUI: prints a diff as text, for when stdout is not a
//! terminal (git's pager, pipes, CI) or `--headless` is given.

use std::path::Path;

use anyhow::Result;

use crate::code::Code;
use crate::code::language::language_for_path_and_content;
use crate::diff::nodes::is_semantically_structural;
use crate::diff::text::{
    RangeMatch, RenderOptions, TextOperation, ranges_for_options, summarize_diff_with_comment_check,
};
use crate::tui::actions::DiffSessionData;
use crate::tui::app::compute_diff_with_options;
use crate::tui::display_columns;

/// SGR grey for chrome (gutter, moved-chunk box). `90` rather than `37`: it is the one neutral
/// legible on both light and dark terminals.
const CHROME_COLOR: &str = "90";

/// ANSI SGR color for each `TextOperation`; must match the TUI palette (`tui::theme::OverlayTheme`,
/// moves grey per `every_themes_move_band_is_grey_rather_than_a_hue`).
fn ansi_color(operation: &TextOperation) -> Option<&'static str> {
    match operation {
        TextOperation::Insert => Some("32"),
        TextOperation::Delete => Some("31"),
        TextOperation::Move => Some(CHROME_COLOR),
        TextOperation::Update => Some("33"),
        TextOperation::Identical | TextOperation::NotYetSet => None,
    }
}

/// Which operation categories touch a line; several can be set at once.
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

/// Per-row flags and column-precise colored spans for one side's `ranges`. Uses
/// `TextRange::columns_on_row`, the same span math as the TUI's `code_viewer::overlay_row`, so
/// both highlight exactly the same characters.
///
/// `Move` sets only the flag: moves are shown by `render_side`'s box, and inline color on top of
/// it is too busy. Ranges are read via `rm.source` on both sides, since each side's list is in its
/// own coordinates.
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
            // Trimmed so a spanning range never colors trailing whitespace; in bytes, the unit
            // `TextRange` columns carry.
            let row_len = crate::diff::text_range::paint_row_len(lines[row]);
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

fn marker_char(present: bool, ch: char, op: TextOperation, use_color: bool) -> String {
    if !present {
        return " ".to_string();
    }
    match ansi_color(&op).filter(|_| use_color) {
        Some(code) => format!("\u{1b}[{code}m{ch}\u{1b}[0m"),
        None => ch.to_string(),
    }
}

/// The 2-column line-marker prefix plus a separator space: `|` for a moved line, then `~`, `-`
/// or `+`. `~` wins because a line with an update is never purely an insert or delete. One column
/// per category is too busy to read.
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

/// Fixed on purpose: the footer only marks where the box ends, it aligns with nothing.
const MOVED_CHUNK_FOOTER_WIDTH: usize = 20;

/// The other side's 1-indexed inclusive line range for the `Move` ranges whose source touches
/// rows `start_row..=end_row`. Destinations merge into one min/max span: two unrelated moves on
/// adjacent rows report a wider range, which is acceptable for a display hint.
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

/// Wraps each span of `line` in its operation's color, with every tab expanded to its tab stop
/// (see `tui::display_columns`), counted from the start of the line's text so indentation lines
/// up whatever the gutter's width. `spans` are **byte** columns (`text_range::SourceColumn`) into
/// `line` as it is, tabs included; an expanded tab is colored with the span it belongs to.
/// Malformed spans (off a char boundary, overlapping, past the end) are clamped; the text itself
/// is never altered beyond the tabs.
fn colorize_line(line: &str, spans: &[(usize, usize, TextOperation)], use_color: bool) -> String {
    if !use_color || spans.is_empty() {
        return display_columns::expand_tabs(line);
    }
    let boundary = |index: usize| crate::diff::text_range::floor_char_boundary(line, index);

    let mut out = String::new();
    let mut column = 0usize;
    let mut cut = 0usize;
    for (start, end, op) in spans {
        let start = boundary(*start);
        let end = boundary(*end);
        if start > cut {
            display_columns::push_expanded(&mut out, &line[cut..start], &mut column);
        }
        // `cut`, not `start`: an overlapping span must never duplicate the user's text.
        let segment_start = start.max(cut);
        if end > segment_start {
            let segment = &line[segment_start..end];
            match ansi_color(op) {
                Some(code) => {
                    out.push_str(&format!("\u{1b}[{code}m"));
                    display_columns::push_expanded(&mut out, segment, &mut column);
                    out.push_str("\u{1b}[0m");
                }
                None => display_columns::push_expanded(&mut out, segment, &mut column),
            }
        }
        cut = cut.max(end);
    }
    if cut < line.len() {
        display_columns::push_expanded(&mut out, &line[cut..], &mut column);
    }
    out
}

/// Default unchanged lines kept around a change, as `diff -u`'s `-U3`; `--context N` overrides it.
pub const CONTEXT_LINES: usize = 3;

/// Which lines to print: changed lines plus `context` lines either side; the rest elide.
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

/// Row of the nearest enclosing named declaration of `row` (like `git diff`'s hunk-header
/// function), or `None` without an AST or enclosing declaration.
///
/// Uses [`is_semantically_structural`], not the broader `is_reference`, which also matches
/// matching anchors such as Rust's `if_expression` that are no use as a landmark.
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

/// Renders one side of a diff: numbered, marker-prefixed lines with elided unchanged runs, an
/// `@` breadcrumb per hunk when its enclosing declaration is out of view, and a box around each
/// moved chunk. `side_is_before` only picks the box header's wording (`Moved to`/`Moved from`).
///
/// Re-parses `contents` for the breadcrumb rather than threading the diff's AST through
/// `DiffSessionData`; headless is not a hot path.
fn render_side(
    contents: &str,
    ranges: &[RangeMatch],
    side_is_before: bool,
    use_color: bool,
    path: &Path,
    context: usize,
) -> String {
    // `display_safe` keeps a CRLF `\r` as part of the terminator; it comes off here.
    let lines: Vec<&str> = contents
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect();
    let (flags, spans) = row_overlay(ranges, &lines);
    let keep = lines_to_keep(&flags, context);

    let language = language_for_path_and_content(path, contents);
    let parsed = language.map(|lang| Code::from_string(contents, &lang));

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

        if !prev_line_shown
            && let (Some(parsed), Some(lang)) = (&parsed, &language)
            && let Some(ref_row) = nearest_reference_line(parsed, lang, i)
            && !keep[ref_row]
        {
            let breadcrumb = format!(
                "{:>number_width$} @ {}",
                ref_row + 1,
                display_columns::expand_tabs(lines[ref_row])
            );
            if use_color {
                out.push_str(&format!("\u{1b}[90m{breadcrumb}\u{1b}[0m\n"));
            } else {
                out.push_str(&breadcrumb);
                out.push('\n');
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
                out.push_str(&format!("\u{1b}[{CHROME_COLOR}m{header}\u{1b}[0m\n"));
            } else {
                out.push_str(&header);
                out.push('\n');
            }
        }

        let number = format!("{:>number_width$} ", i + 1);
        if use_color {
            out.push_str(&format!("\u{1b}[{CHROME_COLOR}m{number}\u{1b}[0m"));
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
                out.push_str(&format!("\u{1b}[{CHROME_COLOR}m{footer}\u{1b}[0m\n"));
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

/// The diff's `DiffSummary` label, as the TUI's status bar shows it, or `None` for an ordinary
/// mixed edit. Bold rather than colored, so it does not read as an operation color.
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

/// Renders a diff session as text: optional summary header, then the before and after sides.
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

/// Computes the diff exactly as the TUI does and prints it to stdout. Returns whether the files'
/// bytes differ, for a `diff`-style exit code.
///
/// A large unmatched residual is reported with `eprintln!`, not `tracing`: headless mode installs
/// no subscriber.
pub fn run(
    before: &Path,
    after: &Path,
    use_color: bool,
    context: usize,
    render_options: RenderOptions,
) -> Result<bool> {
    let (mut data, large_residual) = compute_diff_with_options(before, after, render_options)?;
    // A presentation filter over a finished diff, so not part of `compute_diff`.
    data.before_ranges =
        ranges_for_options(&data.before_ranges, &data.before_contents, render_options);
    data.after_ranges =
        ranges_for_options(&data.after_ranges, &data.after_contents, render_options);
    if large_residual {
        eprintln!(
            "codediff: this diff left an unusually large unmatched residual after the \
             heuristic passes; the structural matching for that portion may be coarser than \
             usual."
        );
    }
    write_stdout(&render_text_diff(&data, use_color, context))?;
    // Raw bytes: `data`'s contents went through `display_safe`, which replaces control
    // characters.
    Ok(std::fs::read(before)? != std::fs::read(after)?)
}

/// Writes non-interactive output to stdout as an `io::Result`, where `print!` would panic. The
/// reader closing the pipe early (`codediff a b | head`, quitting the pager) is the ordinary end of
/// a run, not a crash: it surfaces as `ErrorKind::BrokenPipe`, which `main` exits quietly on.
pub fn write_stdout(text: &str) -> std::io::Result<()> {
    use std::io::Write;

    let mut stdout = std::io::stdout().lock();
    stdout.write_all(text.as_bytes())?;
    stdout.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::compute_diff;
    use std::path::PathBuf;

    #[test]
    fn lines_to_keep_keeps_context_lines_around_a_change_and_nothing_else() {
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
        let mut flags = vec![RowFlags::default(); 10];
        flags[2].deleted = true;
        flags[6].inserted = true;

        let keep = lines_to_keep(&flags, 2);
        assert_eq!(
            keep,
            vec![true, true, true, true, true, true, true, true, true, false]
        );
    }

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
        // Line index 22 prints as line number 23.
        assert!(rendered.contains("23    line22"));
        assert!(rendered.contains("29    line28"));
        assert!(!rendered.contains("line10\n"));
    }

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
        assert!(
            colored.ends_with("   "),
            "trailing whitespace must survive uncolored, not be dropped: {colored:?}"
        );
        assert_eq!(colored, "\u{1b}[33mfoo\u{1b}[0m   ");
    }

    /// The text a highlight actually covers, unwrapped from its ANSI escapes.
    fn highlighted_segments(colored: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = colored;
        while let Some(open) = rest.find('\u{1b}') {
            let Some(open_end) = rest[open..].find('m').map(|i| open + i + 1) else {
                break;
            };
            let Some(close) = rest[open_end..].find('\u{1b}').map(|i| open_end + i) else {
                break;
            };
            if !rest[open_end..close].is_empty() {
                out.push(rest[open_end..close].to_string());
            }
            rest = &rest[close..];
            if let Some(reset_end) = rest.find('m') {
                rest = &rest[reset_end + 1..];
            }
        }
        out
    }

    /// `TextRange` columns are bytes, not characters. Driven through the real pipeline because
    /// hand-written columns would encode whichever unit the author assumed.
    #[test]
    fn a_highlight_covers_the_same_text_on_ascii_and_non_ascii_rows() {
        for (label, before_src, after_src) in [
            ("ascii", "let a = \"xx\";\n", "let a = \"yy\";\n"),
            ("two-byte", "let é = \"xx\";\n", "let é = \"yy\";\n"),
            ("three-byte", "let 漢 = \"xx\";\n", "let 漢 = \"yy\";\n"),
            ("astral", "let 𝛼 = \"xx\";\n", "let 𝛼 = \"yy\";\n"),
        ] {
            let before = crate::code::Code::from_string(before_src, &crate::code::Language::Rust);
            let after = crate::code::Code::from_string(after_src, &crate::code::Language::Rust);
            let diff = crate::diff::diff_code(&before, &after);
            let ast = diff.ast.as_ref().expect("the pair should parse");
            let cache = crate::diff::NodeCache::build(&before, &after);
            let text_diff = crate::diff::text::TextDiff::from(&before, &after, ast, &cache);

            let lines: Vec<&str> = after_src.split('\n').collect();
            let (_flags, spans) = row_overlay(&text_diff.all(1), &lines);
            let colored = colorize_line(lines[0], &spans[0], true);

            assert_eq!(
                highlighted_segments(&colored),
                vec!["yy".to_string()],
                "{label}: the highlight should cover exactly the changed text, got {colored:?}"
            );
        }
    }

    #[test]
    fn colorizing_never_alters_the_text_of_the_row() {
        let line = "let é = 漢字;";
        for spans in [
            vec![(0usize, 3usize, TextOperation::Insert)],
            // Deliberately malformed: overlapping, and a boundary inside the two-byte 'é'.
            vec![
                (0usize, 6usize, TextOperation::Insert),
                (5usize, 12usize, TextOperation::Delete),
            ],
            // Past the end of the row.
            vec![(4usize, 999usize, TextOperation::Update)],
        ] {
            let colored = colorize_line(line, &spans, true);
            let stripped: String = {
                let mut out = String::new();
                let mut in_escape = false;
                for ch in colored.chars() {
                    if ch == '\u{1b}' {
                        in_escape = true;
                    } else if in_escape {
                        in_escape = ch != 'm';
                    } else {
                        out.push(ch);
                    }
                }
                out
            };
            assert_eq!(stripped, line, "spans {spans:?} altered the row");
        }
    }

    /// Hand-built `Move`: small synthetic files do not trigger move detection.
    #[test]
    fn render_side_wraps_a_moved_chunk_in_a_box_with_header_bar_and_footer() {
        let contents = "fn main() {\n    moved_call();\n    same();\n}";
        let ranges = vec![RangeMatch {
            source: crate::diff::text_range::TextRange::new(1, 4, 2, 0),
            destination: crate::diff::text_range::TextRange::new(9, 0, 10, 0),
            operation: TextOperation::Move,
        }];
        let footer = "-".repeat(MOVED_CHUNK_FOOTER_WIDTH);

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
            colored.contains("\u{1b}[90mMoved from line 10\u{1b}[0m"),
            "the header should be grey: {colored}"
        );
        assert!(
            colored.contains(&format!("\u{1b}[90m{footer}\u{1b}[0m")),
            "the footer should be grey: {colored}"
        );
        assert!(
            colored.contains("\u{1b}[90m|\u{1b}[0m"),
            "the box bar should be grey: {colored}"
        );
        assert!(
            !colored.contains("\u{1b}[90mmoved_call();"),
            "a purely-moved line's text should stay plain, not inline-colored: {colored}"
        );
    }

    /// A change whose enclosing `fn` is out of context, with an `is_reference` `if` between them.
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

    /// One changed line: a Delete on the before side and an Insert on the after side.
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
            plain_text_fallback: false,
        }
    }

    /// Expected number-plus-marker prefix, built independently of `markers()`. `n` is one digit.
    fn plain_prefix(n: usize, moved: bool, op: Option<char>) -> String {
        format!(
            "{n} {}{} ",
            if moved { "|" } else { " " },
            op.map(String::from).unwrap_or_else(|| " ".to_string()),
        )
    }

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

        assert!(
            text.contains("\u{1b}[31m-\u{1b}[0m"),
            "the deleted marker column should be red: {text}"
        );
        assert!(
            text.contains("    \u{1b}[31mold_call();\u{1b}[0m"),
            "only the changed substring, not the leading indent, should be wrapped in red: {text}"
        );

        assert!(
            text.contains("\u{1b}[32m+\u{1b}[0m"),
            "the inserted marker column should be green: {text}"
        );
        assert!(
            text.contains("    \u{1b}[32mnew_call();\u{1b}[0m"),
            "only the changed substring, not the leading indent, should be wrapped in green: {text}"
        );

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

        let (data, _large_residual) = compute_diff(&before_path, &after_path)?;
        let text = render_text_diff(&data, false, CONTEXT_LINES);

        assert!(text.contains("old();"), "before content missing: {text}");
        assert!(text.contains("new();"), "after content missing: {text}");
        Ok(())
    }

    #[test]
    fn render_text_diff_omits_the_summary_header_for_an_ordinary_mixed_edit() {
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

        let (data, _large_residual) = compute_diff(&before_path, &after_path)?;
        let text = render_text_diff(&data, false, CONTEXT_LINES);

        assert!(
            text.starts_with(crate::diff::text::DiffSummary::NoChanges.label()),
            "identical files should get a 'no changes' header: {text}"
        );
        Ok(())
    }

    #[test]
    fn render_text_diff_shows_a_comment_only_header_when_only_a_comment_changed() -> Result<()> {
        // Real pipeline, so `comment_only` is shown to actually reach the header.
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

        let (data, _large_residual) = compute_diff(before.path(), after.path())?;
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

    #[test]
    fn update_marker_wins_over_insert_and_delete_on_the_same_line() {
        let flags = RowFlags {
            moved: true,
            inserted: true,
            deleted: true,
            updated: true,
        };
        assert_eq!(markers(flags, false), "|~ ");
        let flags = RowFlags {
            deleted: true,
            inserted: true,
            ..RowFlags::default()
        };
        assert_eq!(markers(flags, false), " - ");
        assert_eq!(markers(RowFlags::default(), false), "   ");
    }

    #[test]
    fn render_side_strips_the_carriage_return_of_a_crlf_row() {
        let rendered = render_side(
            "a\r\nb\r\n",
            &[],
            true,
            false,
            Path::new("plain.txt"),
            CONTEXT_LINES,
        );
        assert!(
            !rendered.contains('\r'),
            "a CRLF `\\r` was printed: {rendered:?}"
        );
    }

    #[test]
    fn render_side_omits_the_breadcrumb_when_the_enclosing_line_is_already_shown() {
        let (contents, _) = rust_file_with_a_change_buried_in_a_function();
        let ranges = vec![RangeMatch {
            source: crate::diff::text_range::TextRange::new(6, 0, 7, 0),
            destination: crate::diff::text_range::TextRange::new(6, 0, 7, 0),
            operation: TextOperation::Delete,
        }];
        let rendered = render_side(
            &contents,
            &ranges,
            true,
            false,
            Path::new("sample.rs"),
            CONTEXT_LINES,
        );
        assert!(
            !rendered.contains(" @ "),
            "`fn parse_args` is within context, so no breadcrumb: {rendered}"
        );
    }

    #[test]
    fn colorize_line_expands_tabs_and_colors_exactly_the_spanned_characters() {
        let line = "\tx\t= é\tnew;";
        // The last tab starts in column 11 and so takes one column.
        let new_start = line.find("new").unwrap();
        let spans = vec![(new_start, new_start + 3, TextOperation::Update)];

        assert_eq!(colorize_line(line, &spans, false), "    x   = é new;");
        assert_eq!(
            colorize_line(line, &spans, true),
            "    x   = é \u{1b}[33mnew\u{1b}[0m;"
        );
    }

    #[test]
    fn colorize_line_colors_a_changed_tab_as_the_spaces_it_expands_to() {
        let spans = vec![(0, 1, TextOperation::Insert)];
        assert_eq!(
            colorize_line("\tx", &spans, true),
            "\u{1b}[32m    \u{1b}[0mx"
        );
        let spans = vec![(1, 2, TextOperation::Insert)];
        assert_eq!(
            colorize_line("a\tx", &spans, true),
            "a\u{1b}[32m   \u{1b}[0mx",
            "a tab after `a` reaches column 4, three columns on"
        );
    }

    /// Tab stops count from the start of the text, not the terminal's left edge, so a gutter
    /// of any width leaves the indentation of every row in line.
    #[test]
    fn render_side_expands_tab_indentation_from_the_start_of_each_rows_text() {
        let contents = "a\n\tb\n\t\tc\n";
        let ranges = vec![RangeMatch {
            source: crate::diff::text_range::TextRange::new(2, 2, 2, 3),
            destination: crate::diff::text_range::TextRange::new(2, 2, 2, 3),
            operation: TextOperation::Update,
        }];

        let rendered = render_side(
            contents,
            &ranges,
            true,
            false,
            Path::new("sample.txt"),
            CONTEXT_LINES,
        );

        assert!(rendered.contains("1    a\n"), "{rendered}");
        assert!(rendered.contains("2        b\n"), "{rendered}");
        assert!(rendered.contains("3  ~         c\n"), "{rendered}");
        assert!(!rendered.contains('\t'), "{rendered}");
    }

    #[test]
    fn run_reports_a_tab_versus_spaces_difference() -> Result<()> {
        let dir = tempfile::tempdir().expect("create temp dir");
        let before_path = dir.path().join("before.txt");
        let after_path = dir.path().join("after.txt");
        std::fs::write(&before_path, "\tx\n").unwrap();
        std::fs::write(&after_path, "    x\n").unwrap();
        assert!(run(
            &before_path,
            &after_path,
            false,
            CONTEXT_LINES,
            RenderOptions::FULL
        )?);
        Ok(())
    }
}
