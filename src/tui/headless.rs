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

use crate::diff::DiffMode;
use crate::diff::text::{RangeMatch, TextOperation, line_operations};
use crate::tui::actions::DiffSessionData;
use crate::tui::app::compute_diff;

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

/// Renders one side (before or after) of a diff as colored, marker-prefixed lines.
fn render_side(contents: &str, ranges: &[RangeMatch], side_is_before: bool, use_color: bool) -> String {
    let lines: Vec<&str> = contents.split('\n').collect();
    let ops = line_operations(ranges, lines.len());

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
/// `diff::text::line_operations`'s doc comment for why this is row-granular rather than a true
/// interleaved unified-diff hunk format.
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
///
/// Headless mode never prompts (there's no interactive surface to ask on, unlike the TUI's
/// `SelectDiffMode` dialog) - `mode` is applied unconditionally. Under `DiffMode::Fast` (the
/// default), if the guard silently substituted the cheaper fallback for phase 6, a one-line note
/// goes to stderr (plain `eprintln!`, not `tracing` - headless mode never calls
/// `tui::initialize_logging`, so there's no subscriber installed to receive it) so a script
/// invoking this isn't left wondering why the diff looks less precise than expected.
pub fn run(before: &Path, after: &Path, use_color: bool, mode: DiffMode) -> Result<()> {
    let (data, fallback_used) = compute_diff(before, after, mode)?;
    if fallback_used {
        eprintln!(
            "codediff: the residual after the fast heuristic passes was too large for full \
             tree-edit-distance analysis; used a faster, less precise fallback instead \
             (pass --exact to force full analysis)."
        );
    }
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

    #[test]
    fn run_prints_a_readable_diff_for_two_real_files() -> Result<()> {
        let dir = tempfile::tempdir().expect("create temp dir");
        let before_path = dir.path().join("before_sample.rs");
        let after_path = dir.path().join("after_sample.rs");
        std::fs::write(&before_path, "fn main() {\n    old();\n}\n").unwrap();
        std::fs::write(&after_path, "fn main() {\n    new();\n}\n").unwrap();

        let (data, _fallback_used) = compute_diff(&before_path, &after_path, DiffMode::Fast)?;
        let text = render_text_diff(&data, false);

        assert!(text.contains("old();"), "before content missing: {text}");
        assert!(text.contains("new();"), "after content missing: {text}");
        Ok(())
    }
}
