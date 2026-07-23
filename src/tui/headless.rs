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

use crate::diff::text::{RangeMatch, TextOperation};
use crate::tui::actions::DiffSessionData;
use crate::tui::app::compute_diff;
use crate::tui::widgets::code_viewer::is_empty_range;

/// ANSI SGR color for each `TextOperation`, matching the TUI's own convention (see "Diff overlay
/// and cursor model" in `SPECS.md`): insert green, delete red, move yellow, update magenta.
/// `Identical` (and the `NotYetSet` sentinel, which never survives into a real diff) are left
/// uncolored, same as the TUI's plain syntax-highlighted text.
fn ansi_color(operation: &TextOperation) -> Option<&'static str> {
    match operation {
        TextOperation::Insert => Some("32"),
        TextOperation::Delete => Some("31"),
        TextOperation::Update => Some("35"),
        TextOperation::Move => Some("33"),
        TextOperation::Identical | TextOperation::NotYetSet => None,
    }
}

/// The per-line marker for each operation. `-`/`+` deliberately reuse familiar unified-diff
/// markers for Delete/Insert, and for Update too (from the before side it reads as "old content
/// being replaced", from the after side as "new content replacing it"); `~` marks a Move, which -
/// unlike Delete/Insert/Update - has a real counterpart on both sides, just relocated.
fn marker(operation: &TextOperation, side_is_before: bool) -> &'static str {
    match operation {
        TextOperation::Delete => "- ",
        TextOperation::Insert => "+ ",
        TextOperation::Update if side_is_before => "- ",
        TextOperation::Update => "+ ",
        TextOperation::Move => "~ ",
        TextOperation::Identical | TextOperation::NotYetSet => "  ",
    }
}

/// Assigns one `TextOperation` to each of `line_count` lines, from one side's `RangeMatch` list
/// (`diff::text::TextDiff::all`).
///
/// Deliberately row-granular, not column-precise: a range's column bounds are only used to decide
/// whether it's a zero-width placeholder, never to split a single line between two operations.
/// `diff::text` ranges are whitespace-insensitive and can leave small gaps (e.g. leading
/// indentation - see `python_leetcode_1_added_if_block_all_ranges` in `diff/text.rs`), so lining
/// up exact sub-line spans for a plain-text renderer would be fragile; whole-line coloring instead
/// picks, for each row, the *most specific* operation among all the ranges that touch it (see the
/// precedence comment below). The TUI remains the column-precise view for anyone who needs that;
/// this is a fallback for contexts where the TUI can't run at all, not a replacement for it.
fn row_operations(ranges: &[RangeMatch], line_count: usize) -> Vec<TextOperation> {
    let mut ops = vec![TextOperation::Identical; line_count];
    for rm in ranges {
        let r = &rm.source;
        if is_empty_range(r) {
            // Zero-width placeholder: nothing on this side for this diff unit (see
            // `TextRange`'s doc comment on symmetric insert/delete placeholders).
            continue;
        }
        // `TextRange`'s convention: an end column of 0 already means "up to, not including, this
        // row", so only a genuinely mid-row end column needs the extra +1.
        let end_row = if r.end_column == 0 { r.end_row } else { r.end_row + 1 };
        for row in r.start_row..end_row.min(line_count) {
            // A row can legitimately be touched by more than one range (e.g. a changed token
            // shares its row with the identical whitespace/punctuation around it). Whichever
            // range for that row is *not* Identical wins, regardless of iteration order -
            // otherwise an Identical range for the same row ordered after the real change would
            // silently overwrite it back to plain, hiding the change entirely. Two non-Identical
            // ranges touching the same row is not expected to happen in practice (ranges are
            // built from a non-overlapping tree traversal, see `diff/text.rs`), so last-wins
            // between two of those is an arbitrary but harmless tiebreak.
            if rm.operation != TextOperation::Identical || ops[row] == TextOperation::Identical {
                ops[row] = rm.operation.clone();
            }
        }
    }
    ops
}

/// Renders one side (before or after) of a diff as colored, marker-prefixed lines.
fn render_side(contents: &str, ranges: &[RangeMatch], side_is_before: bool, use_color: bool) -> String {
    let lines: Vec<&str> = contents.split('\n').collect();
    let ops = row_operations(ranges, lines.len());

    let mut out = String::new();
    for (line, operation) in lines.iter().zip(ops.iter()) {
        let prefix = marker(operation, side_is_before);
        match ansi_color(operation).filter(|_| use_color) {
            Some(code) => out.push_str(&format!("\u{1b}[{code}m{prefix}{line}\u{1b}[0m\n")),
            None => {
                out.push_str(prefix);
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    out
}

/// Renders a full diff session as plain text: the "before" side (deletions/updates/moves
/// highlighted), then the "after" side (insertions/updates/moves highlighted). See
/// `row_operations`'s doc comment for why this is row-granular rather than a true interleaved
/// unified-diff hunk format.
pub(crate) fn render_text_diff(data: &DiffSessionData, use_color: bool) -> String {
    let mut out = String::new();
    out.push_str(&format!("=== before: {} ===\n", data.before_path.display()));
    out.push_str(&render_side(&data.before_contents, &data.before_ranges, true, use_color));
    out.push_str(&format!("=== after: {} ===\n", data.after_path.display()));
    out.push_str(&render_side(&data.after_contents, &data.after_ranges, false, use_color));
    out
}

/// Entry point for headless/text-mode operation (`main.rs`): computes the diff exactly like the
/// TUI does (`app::compute_diff` - same parsing, same `ASTDiff`, same `TextDiff`), then prints it
/// as text on stdout instead of drawing an interactive terminal UI.
pub fn run(before: &Path, after: &Path, use_color: bool) -> Result<()> {
    let data = compute_diff(before, after)?;
    print!("{}", render_text_diff(&data, use_color));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
        }
    }

    /// Builds the expected plain-text rendering by concatenating each line's marker with its
    /// original text directly, rather than a hand-typed literal - counting the exact whitespace
    /// a "- "/"+ "/"  " prefix plus a 4-space-indented line produces by eye is error-prone.
    fn expected_plain_text() -> String {
        format!(
            "=== before: before.rs ===\n{}{}\n{}{}\n{}{}\n{}{}\n\
             === after: after.rs ===\n{}{}\n{}{}\n{}{}\n{}{}\n",
            "  ", "fn main() {",
            "- ", "    old_call();",
            "  ", "    same();",
            "  ", "}",
            "  ", "fn main() {",
            "+ ", "    new_call();",
            "  ", "    same();",
            "  ", "}",
        )
    }

    #[test]
    fn render_text_diff_without_color_shows_both_sides_with_markers() {
        assert_eq!(render_text_diff(&sample_data(), false), expected_plain_text());
    }

    #[test]
    fn render_text_diff_with_color_wraps_only_changed_lines_in_ansi_codes() {
        let text = render_text_diff(&sample_data(), true);
        let deleted = format!("\u{1b}[31m{}{}\u{1b}[0m", "- ", "    old_call();");
        let inserted = format!("\u{1b}[32m{}{}\u{1b}[0m", "+ ", "    new_call();");
        assert!(text.contains(&deleted), "deleted-side line should be red: {text}");
        assert!(text.contains(&inserted), "inserted-side line should be green: {text}");
        assert_eq!(
            text.matches('\u{1b}').count(),
            4,
            "only the two changed lines (one color-start plus one reset each) should carry ANSI codes: {text}"
        );
        assert!(
            text.contains(&format!("\n{}{}\n", "  ", "fn main() {")),
            "identical lines must stay uncolored: {text}"
        );
    }

    /// Regression guard: a real one-token change (e.g. renaming a call inside an otherwise
    /// unchanged statement) can leave the same row covered by both an Update range (the token)
    /// and an Identical range (the rest of the line/its surrounding punctuation). If the Identical
    /// range for that row happens to come *after* the Update range in `ranges`' order, a naive
    /// last-write-wins would silently overwrite the row back to `Identical`, hiding the change -
    /// this is exactly what a real end-to-end smoke test against the built binary caught.
    #[test]
    fn row_operations_does_not_let_a_same_row_identical_range_hide_a_real_change() {
        let ranges = vec![
            RangeMatch {
                source: crate::diff::text_range::TextRange::new(0, 4, 0, 12),
                destination: crate::diff::text_range::TextRange::new(0, 4, 0, 12),
                operation: TextOperation::Update,
            },
            // Ordered *after* the Update above on purpose - this is the ordering that triggered
            // the bug.
            RangeMatch {
                source: crate::diff::text_range::TextRange::new(0, 12, 1, 0),
                destination: crate::diff::text_range::TextRange::new(0, 12, 1, 0),
                operation: TextOperation::Identical,
            },
        ];
        assert_eq!(row_operations(&ranges, 1), vec![TextOperation::Update]);
    }

    #[test]
    fn row_operations_treats_a_zero_width_range_as_a_placeholder_not_a_real_row() {
        let ranges = vec![RangeMatch {
            source: crate::diff::text_range::TextRange::new(1, 0, 1, 0),
            destination: crate::diff::text_range::TextRange::new(1, 0, 2, 0),
            operation: TextOperation::Delete,
        }];
        let ops = row_operations(&ranges, 3);
        assert_eq!(ops, vec![TextOperation::Identical; 3]);
    }

    #[test]
    fn run_prints_a_readable_diff_for_two_real_files() -> Result<()> {
        let dir = tempfile::tempdir().expect("create temp dir");
        let before_path = dir.path().join("before_sample.rs");
        let after_path = dir.path().join("after_sample.rs");
        std::fs::write(&before_path, "fn main() {\n    old();\n}\n").unwrap();
        std::fs::write(&after_path, "fn main() {\n    new();\n}\n").unwrap();

        let data = compute_diff(&before_path, &after_path)?;
        let text = render_text_diff(&data, false);

        assert!(text.contains("old();"), "before content missing: {text}");
        assert!(text.contains("new();"), "after content missing: {text}");
        Ok(())
    }
}
