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
use crate::code::language::language_for_path;
use crate::diff::DiffMode;
use crate::diff::nodes::is_semantically_structural;
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

/// How many unchanged lines to keep on either side of a change, same convention as `diff -u`'s
/// default `-U3`. A run of unchanged lines longer than `2 * CONTEXT_LINES` (enough for both
/// changes bordering it to keep their own context) gets its middle collapsed into a single
/// elision marker instead of printed in full - see `lines_to_keep`.
const CONTEXT_LINES: usize = 3;

/// Which line indices `render_side` should actually print: any line that isn't `Identical`, plus
/// `CONTEXT_LINES` lines on either side of one. Everything else is a candidate to collapse into
/// an elision marker - this is what fixes headless mode printing entire unchanged files twice
/// (once per side) for a single-line change.
fn lines_to_keep(ops: &[TextOperation], context: usize) -> Vec<bool> {
    let mut keep = vec![false; ops.len()];
    for (i, op) in ops.iter().enumerate() {
        if *op != TextOperation::Identical && *op != TextOperation::NotYetSet {
            let start = i.saturating_sub(context);
            let end = (i + context + 1).min(ops.len());
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
) -> String {
    let lines: Vec<&str> = contents.split('\n').collect();
    let ops = line_operations(ranges, lines.len());
    let keep = lines_to_keep(&ops, CONTEXT_LINES);

    let language = language_for_path(path);
    let parsed = language.map(|lang| Code::from_string(contents, &lang));

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
                        let breadcrumb = format!("    @ {}", lines[ref_row]);
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

        let line = lines[i];
        let operation = &ops[i];
        let prefix = marker(operation, side_is_before);
        match ansi_color(operation).filter(|_| use_color) {
            Some(code) => out.push_str(&format!("\u{1b}[{code}m{prefix}{line}\u{1b}[0m\n")),
            None => {
                out.push_str(prefix);
                out.push_str(line);
                out.push('\n');
            }
        }
        prev_line_shown = true;
        i += 1;
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
    out.push_str(&render_side(
        &data.before_contents,
        &data.before_ranges,
        true,
        use_color,
        &data.before_path,
    ));
    out.push_str(&format!("=== after: {} ===\n", data.after_path.display()));
    out.push_str(&render_side(
        &data.after_contents,
        &data.after_ranges,
        false,
        use_color,
        &data.after_path,
    ));
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

    #[test]
    fn lines_to_keep_keeps_context_lines_around_a_change_and_nothing_else() {
        // 10 lines: only line 5 (index 5) changed. With context=2, indices 3..=7 should be kept.
        let mut ops = vec![TextOperation::Identical; 10];
        ops[5] = TextOperation::Insert;

        let keep = lines_to_keep(&ops, 2);
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
        let mut ops = vec![TextOperation::Identical; 10];
        ops[2] = TextOperation::Delete;
        ops[6] = TextOperation::Insert;

        let keep = lines_to_keep(&ops, 2);
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

        let rendered = render_side(&contents, &ranges, true, false, Path::new("plain.txt"));
        let line_count = rendered.lines().count();
        assert!(
            line_count < 15,
            "50 lines with 1 change should collapse to well under 15 output lines, got \
             {line_count}:\n{rendered}"
        );
        assert!(rendered.contains("- changed"));
        assert!(rendered.contains("unchanged lines"));
        // Context lines immediately around the change must still be shown in full.
        assert!(rendered.contains("  line22"));
        assert!(rendered.contains("  line28"));
        // But nothing further out should survive.
        assert!(!rendered.contains("line10\n"));
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
        let rendered = render_side(&contents, &ranges, true, false, Path::new("sample.rs"));

        assert!(
            rendered.contains("@ fn parse_args"),
            "should surface the enclosing function as a breadcrumb: {rendered}"
        );
        let breadcrumb_lines = rendered.lines().filter(|l| l.trim_start().starts_with('@'));
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
        }
    }

    /// Builds the expected plain-text rendering by concatenating each line's marker with its
    /// original text directly, rather than a hand-typed literal - counting the exact whitespace
    /// a "- "/"+ "/"  " prefix plus a 4-space-indented line produces by eye is error-prone.
    fn expected_plain_text() -> String {
        format!(
            "=== before: before.rs ===\n{}{}\n{}{}\n{}{}\n{}{}\n\
             === after: after.rs ===\n{}{}\n{}{}\n{}{}\n{}{}\n",
            "  ",
            "fn main() {",
            "- ",
            "    old_call();",
            "  ",
            "    same();",
            "  ",
            "}",
            "  ",
            "fn main() {",
            "+ ",
            "    new_call();",
            "  ",
            "    same();",
            "  ",
            "}",
        )
    }

    #[test]
    fn render_text_diff_without_color_shows_both_sides_with_markers() {
        assert_eq!(
            render_text_diff(&sample_data(), false),
            expected_plain_text()
        );
    }

    #[test]
    fn render_text_diff_with_color_wraps_only_changed_lines_in_ansi_codes() {
        let text = render_text_diff(&sample_data(), true);
        let deleted = format!("\u{1b}[31m{}{}\u{1b}[0m", "- ", "    old_call();");
        let inserted = format!("\u{1b}[32m{}{}\u{1b}[0m", "+ ", "    new_call();");
        assert!(
            text.contains(&deleted),
            "deleted-side line should be red: {text}"
        );
        assert!(
            text.contains(&inserted),
            "inserted-side line should be green: {text}"
        );
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
