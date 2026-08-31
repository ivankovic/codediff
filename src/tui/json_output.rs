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

//! Machine-readable counterpart to `tui::headless`: prints a diff as a single JSON object on
//! stdout instead of ANSI text or an interactive TUI. Exists for editor/tool integrations (e.g. a
//! Neovim plugin) that want to place highlights/signs on their own already-open buffers rather
//! than parse ANSI escape codes back out of `headless`'s output.
//!
//! Schema (see `JsonDiff`/`JsonSide`/`JsonHunk`/`JsonRange` below for the authoritative field
//! list):
//!
//! ```json
//! {
//!   "before": {
//!     "path": "old.rs",
//!     "language": "Rust",
//!     "hunks": [
//!       { "operation": "delete", "range": { "start_row": 12, "start_column": 4, "end_row": 12, "end_column": 20 } }
//!     ]
//!   },
//!   "after": { "path": "new.rs", "language": "Rust", "hunks": [ ... ] },
//!   "fallback_used": false,
//!   "summary": "comment_only"
//! }
//! ```
//!
//! One pair never reaches the diff engine: if either side is binary (`code::is_binary_file`),
//! `main.rs` answers with `binary_diff_json` instead - the same object, with `"binary": true`,
//! empty `hunks`, and no `summary`. That field is omitted for every ordinary text diff.
//!
//! Each side's `hunks` list is that side's own complete account of its changes (`before`'s hunks
//! are ranges in the before file, `after`'s are ranges in the after file) - the same per-side
//! split `headless.rs` renders as two separate blocks, just serialized instead of printed. Rows
//! and columns are 0-indexed, matching `diff::text_range::TextRange`'s own convention.
//!
//! `summary` is the diff's overall shape (see `JsonDiffSummary`/`diff::text::DiffSummary`) -
//! `no_changes`, `new_file`, `deleted_file`, `whitespace_only`, `comment_only`, or
//! `refactor_moved_only` - omitted entirely for the ordinary case of a genuine mix of edits that
//! doesn't cleanly fit one of those, the same "print nothing extra" convention `headless::run`
//! follows for its own header line.
//!
//! Deliberately does not embed file contents: `main.rs`'s own doc comment on `GIT_EXTERNAL_DIFF`
//! notes both positional arguments are always real files (working-tree paths or git's temp blob
//! copies) - a caller like a Neovim plugin already has (or can open) that file itself, so
//! duplicating its content here would just be a second copy to keep in sync.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;

use crate::code::Code;
use crate::code::language::{language_for_path, language_for_path_and_content};
use crate::diff::DiffMode;
use crate::diff::text::{
    DiffSummary, RangeMatch, RenderOptions, TextOperation, ranges_for_options,
    summarize_diff_with_comment_check,
};
use crate::diff::text_range::TextRange;
use crate::tui::actions::DiffSessionData;
use crate::tui::app::compute_diff;
use crate::tui::headless::nearest_reference_line;

/// A `TextRange`, reshaped for JSON. Kept as a local type rather than `#[derive(Serialize)]` on
/// `TextRange` itself - that keeps `diff::text_range` free of a serde dependency on its public
/// type, the same boundary `headless.rs` already draws around ANSI concerns and `diff::text`.
#[derive(Debug, Serialize, PartialEq, Eq)]
struct JsonRange {
    start_row: usize,
    start_column: usize,
    end_row: usize,
    end_column: usize,
}

impl From<&TextRange> for JsonRange {
    fn from(r: &TextRange) -> Self {
        JsonRange {
            start_row: r.start_row,
            start_column: r.start_column,
            end_row: r.end_row,
            end_column: r.end_column,
        }
    }
}

/// Mirrors `TextOperation`, minus `Identical`/`NotYetSet`: unchanged text gets no hunk at all (see
/// this module's doc comment on why file contents aren't embedded), so there is nothing to report
/// for either sentinel.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum JsonOperation {
    Insert,
    Delete,
    Update,
    Move,
}

impl JsonOperation {
    fn from_text_operation(op: &TextOperation) -> Option<Self> {
        match op {
            TextOperation::Insert => Some(JsonOperation::Insert),
            TextOperation::Delete => Some(JsonOperation::Delete),
            TextOperation::Update => Some(JsonOperation::Update),
            TextOperation::Move => Some(JsonOperation::Move),
            TextOperation::Identical | TextOperation::NotYetSet => None,
        }
    }
}

/// Mirrors `diff::text::DiffSummary` - the same overall-shape classification the interactive TUI
/// shows in its status bar and `headless::run` prints as a header line, serialized here as a tag a
/// script can match on instead of a prose label meant for a human. Kept as a local type rather than
/// `#[derive(Serialize)]` on `DiffSummary` itself, the same boundary `JsonRange`/`JsonOperation`
/// already draw around `diff::text`'s serde-free public types.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum JsonDiffSummary {
    NoChanges,
    NewFile,
    DeletedFile,
    WhitespaceOnly,
    CommentOnly,
    RefactorMovedOnly,
}

impl From<DiffSummary> for JsonDiffSummary {
    fn from(summary: DiffSummary) -> Self {
        match summary {
            DiffSummary::NoChanges => JsonDiffSummary::NoChanges,
            DiffSummary::NewFile => JsonDiffSummary::NewFile,
            DiffSummary::DeletedFile => JsonDiffSummary::DeletedFile,
            DiffSummary::WhitespaceOnly => JsonDiffSummary::WhitespaceOnly,
            DiffSummary::CommentOnly => JsonDiffSummary::CommentOnly,
            DiffSummary::RefactorMovedOnly => JsonDiffSummary::RefactorMovedOnly,
        }
    }
}

#[derive(Debug, Serialize)]
struct JsonHunk {
    operation: JsonOperation,
    range: JsonRange,
    /// Only set for `Move`: the real counterpart range in the *other* file, computed the same way
    /// as an `Identical` mapping's destination (see `diff::text::ranges`'s `Identical` match arm -
    /// a `Move` is exactly that same "found the matching node" case, just relocated). Every other
    /// operation has `RangeMatch::destination` filled with a synthetic bookkeeping anchor instead
    /// (`diff::text::advance_and_build_range`'s `last_non_move_range.right_limit()`), not a real
    /// position in the other file - surfacing that here would look like a jump target but isn't
    /// one, so it is deliberately omitted rather than included and mislabeled.
    #[serde(skip_serializing_if = "Option::is_none")]
    move_target: Option<JsonRange>,
    /// The row of the nearest enclosing named declaration (function, struct, ...) - same idea as
    /// `headless.rs`'s `@` breadcrumb, reusing its exact `nearest_reference_line` walk, just
    /// surfaced as a field instead of printed as a line.
    #[serde(skip_serializing_if = "Option::is_none")]
    reference_line: Option<usize>,
}

#[derive(Debug, Serialize)]
struct JsonSide {
    path: PathBuf,
    language: Option<String>,
    hunks: Vec<JsonHunk>,
}

#[derive(Debug, Serialize)]
struct JsonDiff {
    before: JsonSide,
    after: JsonSide,
    /// Whether `DiffMode::Fast`'s guard silently substituted a cheaper, less precise fallback for
    /// phase 6 (see `compute_diff`'s doc comment). `headless::run` reports this as a one-line
    /// stderr note; JSON mode is meant for machine consumption, so it is a field here instead - a
    /// script parsing stdout as JSON shouldn't also have to watch stderr for a caveat.
    fallback_used: bool,
    /// The diff's overall shape (see `JsonDiffSummary`) - `None` for the ordinary case (a genuine
    /// mix of edits that doesn't cleanly fit one of `DiffSummary`'s special cases), same as
    /// `headless::run` printing no header line at all in that case.
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<JsonDiffSummary>,
    /// Set when at least one side is a file codediff cannot read as text, so no diff was computed
    /// at all (see `binary_diff_json`). Both sides' `hunks` are empty in that case, which is
    /// otherwise indistinguishable from "the files are identical" - hence a flag rather than
    /// leaving the consumer to infer it. Omitted entirely for the ordinary text case, so an
    /// existing consumer sees the object shape it already knows.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    binary: bool,
}

/// Builds one side's `JsonSide` from its own complete `RangeMatch` list (`DiffSessionData::
/// before_ranges`/`after_ranges` - see `diff::text::TextDiff::all`'s doc comment), re-parsing
/// `contents` to walk it for `nearest_reference_line`, same trade-off `headless::render_side`
/// already makes (see its own doc comment: not on any hot path, so a redundant parse is fine).
fn build_side(contents: &str, path: &Path, ranges: &[RangeMatch]) -> JsonSide {
    let language = language_for_path_and_content(path, contents);
    let parsed = language.map(|lang| Code::from_string(contents, &lang));

    let mut hunks = Vec::new();
    for range_match in ranges {
        let Some(operation) = JsonOperation::from_text_operation(&range_match.operation) else {
            continue;
        };
        let move_target =
            (operation == JsonOperation::Move).then(|| JsonRange::from(&range_match.destination));
        let reference_line = match (&parsed, &language) {
            (Some(parsed), Some(lang)) => {
                nearest_reference_line(parsed, lang, range_match.source.start_row)
            }
            _ => None,
        };
        hunks.push(JsonHunk {
            operation,
            range: JsonRange::from(&range_match.source),
            move_target,
            reference_line,
        });
    }

    JsonSide {
        path: path.to_path_buf(),
        language: language.map(|lang| lang.to_string()),
        hunks,
    }
}

fn build_diff(data: &DiffSessionData, fallback_used: bool) -> JsonDiff {
    let summary = summarize_diff_with_comment_check(
        &data.before_contents,
        &data.after_contents,
        &data.before_ranges,
        &data.after_ranges,
        data.comment_only,
    )
    .map(JsonDiffSummary::from);

    JsonDiff {
        before: build_side(
            &data.before_contents,
            &data.before_path,
            &data.before_ranges,
        ),
        after: build_side(&data.after_contents, &data.after_path, &data.after_ranges),
        fallback_used,
        summary,
        binary: false,
    }
}

/// The `--mode json` answer for a pair `main.rs` refused to diff because a side is binary: the
/// same object shape as any other run, with `binary` set, no hunks on either side, and no
/// `summary` (none of `DiffSummary`'s shapes apply when no diff was computed). Returns the
/// serialized text rather than printing it, so the single `println!` for JSON output stays at
/// `main.rs`'s own call site alongside the text-mode notice it replaces.
///
/// `language` is still filled in per side from the path's extension, exactly as `build_side`
/// would: it describes the file, not the diff, and a consumer keying on it should not see it
/// vanish just because this pair was unreadable.
pub fn binary_diff_json(before: &Path, after: &Path) -> Result<String> {
    let side = |path: &Path| JsonSide {
        path: path.to_path_buf(),
        language: language_for_path(path).map(|lang| lang.to_string()),
        hunks: Vec::new(),
    };
    let diff = JsonDiff {
        before: side(before),
        after: side(after),
        fallback_used: false,
        summary: None,
        binary: true,
    };
    Ok(serde_json::to_string_pretty(&diff)?)
}

/// Entry point for `--mode json` (`main.rs`): computes the diff exactly like `headless::run` and
/// the interactive TUI do (same `compute_diff` - same parsing, same `ASTDiff`, same `TextDiff`),
/// then prints it as one pretty-printed JSON object on stdout. See this module's doc comment for
/// the schema.
/// Returns whether the two files differ at all (raw byte comparison, same convention as
/// `headless::run`) so `main.rs` can turn it into a `diff`-style exit code.
pub fn run(
    before: &Path,
    after: &Path,
    mode: DiffMode,
    render_options: RenderOptions,
) -> Result<bool> {
    let (mut data, fallback_used) = compute_diff(before, after, mode)?;
    // See `headless::run`'s note: a presentation filter over a finished diff, not a different one.
    data.before_ranges =
        ranges_for_options(&data.before_ranges, &data.before_contents, render_options);
    data.after_ranges =
        ranges_for_options(&data.after_ranges, &data.after_contents, render_options);
    let diff = build_diff(&data, fallback_used);
    println!("{}", serde_json::to_string_pretty(&diff)?);
    Ok(std::fs::read(before)? != std::fs::read(after)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Same synthetic 4-line change `headless.rs`'s own tests use (one line changed, modeled as a
    /// Delete on the before side paired with an Insert on the after side).
    fn sample_data() -> DiffSessionData {
        DiffSessionData {
            before_path: PathBuf::from("before.rs"),
            after_path: PathBuf::from("after.rs"),
            before_contents: "fn main() {\n    old_call();\n    same();\n}".to_string(),
            after_contents: "fn main() {\n    new_call();\n    same();\n}".to_string(),
            before_ranges: vec![
                RangeMatch {
                    source: TextRange::new(0, 0, 1, 0),
                    destination: TextRange::new(0, 0, 1, 0),
                    operation: TextOperation::Identical,
                },
                RangeMatch {
                    source: TextRange::new(1, 4, 2, 0),
                    destination: TextRange::new(1, 4, 2, 0),
                    operation: TextOperation::Delete,
                },
                RangeMatch {
                    source: TextRange::new(2, 0, 4, 0),
                    destination: TextRange::new(2, 0, 4, 0),
                    operation: TextOperation::Identical,
                },
            ],
            after_ranges: vec![
                RangeMatch {
                    source: TextRange::new(0, 0, 1, 0),
                    destination: TextRange::new(0, 0, 1, 0),
                    operation: TextOperation::Identical,
                },
                RangeMatch {
                    source: TextRange::new(1, 4, 2, 0),
                    destination: TextRange::new(1, 4, 2, 0),
                    operation: TextOperation::Insert,
                },
                RangeMatch {
                    source: TextRange::new(2, 0, 4, 0),
                    destination: TextRange::new(2, 0, 4, 0),
                    operation: TextOperation::Identical,
                },
            ],
            comment_only: false,
            mode: DiffMode::Fast,
            plain_text_fallback: false,
        }
    }

    #[test]
    fn build_diff_omits_identical_ranges_and_keeps_only_the_real_change() {
        let diff = build_diff(&sample_data(), false);

        assert_eq!(diff.before.hunks.len(), 1, "{:?}", diff.before.hunks);
        assert_eq!(diff.before.hunks[0].operation, JsonOperation::Delete);
        assert_eq!(
            diff.before.hunks[0].range,
            JsonRange {
                start_row: 1,
                start_column: 4,
                end_row: 2,
                end_column: 0
            }
        );

        assert_eq!(diff.after.hunks.len(), 1, "{:?}", diff.after.hunks);
        assert_eq!(diff.after.hunks[0].operation, JsonOperation::Insert);
    }

    #[test]
    fn build_diff_never_sets_move_target_for_non_move_operations() {
        let diff = build_diff(&sample_data(), false);
        assert!(diff.before.hunks[0].move_target.is_none());
        assert!(diff.after.hunks[0].move_target.is_none());
    }

    #[test]
    fn build_diff_sets_move_target_only_for_a_move_operation() {
        let mut data = sample_data();
        data.before_ranges[1].operation = TextOperation::Move;
        data.before_ranges[1].destination = TextRange::new(5, 0, 6, 0);

        let diff = build_diff(&data, false);
        assert_eq!(diff.before.hunks[0].operation, JsonOperation::Move);
        assert_eq!(
            diff.before.hunks[0].move_target,
            Some(JsonRange {
                start_row: 5,
                start_column: 0,
                end_row: 6,
                end_column: 0
            })
        );
    }

    #[test]
    fn build_diff_carries_the_fallback_used_flag_through_as_a_field() {
        assert!(build_diff(&sample_data(), true).fallback_used);
        assert!(!build_diff(&sample_data(), false).fallback_used);
    }

    #[test]
    fn build_diff_omits_summary_for_an_ordinary_mixed_edit() {
        // Same synthetic data used throughout this module's tests - a real Delete+Insert pair
        // alongside Identical ranges, which shouldn't classify as any `DiffSummary` special case.
        assert!(build_diff(&sample_data(), false).summary.is_none());
    }

    #[test]
    fn build_diff_sets_summary_to_no_changes_for_identical_content() {
        let mut data = sample_data();
        data.before_contents = "same\n".to_string();
        data.after_contents = "same\n".to_string();
        data.before_ranges = vec![RangeMatch {
            source: TextRange::new(0, 0, 1, 0),
            destination: TextRange::new(0, 0, 1, 0),
            operation: TextOperation::Identical,
        }];
        data.after_ranges = data.before_ranges.clone();

        let diff = build_diff(&data, false);
        assert_eq!(diff.summary, Some(JsonDiffSummary::NoChanges));
        let json = serde_json::to_value(&diff).unwrap();
        assert_eq!(json["summary"], "no_changes");
    }

    #[test]
    fn build_diff_omits_the_summary_field_entirely_from_serialized_json_when_none() {
        let json = serde_json::to_value(build_diff(&sample_data(), false)).unwrap();
        assert!(
            json.get("summary").is_none(),
            "the summary field should be omitted, not null, for the ordinary case: {json}"
        );
    }

    #[test]
    fn run_prints_valid_json_with_the_expected_top_level_shape() -> Result<()> {
        let dir = tempfile::tempdir().expect("create temp dir");
        let before_path = dir.path().join("before_sample.rs");
        let after_path = dir.path().join("after_sample.rs");
        std::fs::write(&before_path, "fn main() {\n    old();\n}\n").unwrap();
        std::fs::write(&after_path, "fn main() {\n    new();\n}\n").unwrap();

        let (data, fallback_used) = compute_diff(&before_path, &after_path, DiffMode::Fast)?;
        let diff = build_diff(&data, fallback_used);
        let json = serde_json::to_value(&diff)?;

        assert!(json.get("before").is_some());
        assert!(json.get("after").is_some());
        assert!(json.get("fallback_used").is_some());
        // `old()` -> `new()` is a single-identifier rename, which the diff pipeline reports as an
        // Update (not a Delete+Insert pair) - same real-file behavior `headless.rs`'s own
        // equivalent test observes by checking rendered content rather than assuming an operation.
        assert!(
            !json["before"]["hunks"]
                .as_array()
                .expect("hunks should be an array")
                .is_empty(),
            "expected at least one hunk on the before side: {json}"
        );
        assert!(
            !json["after"]["hunks"]
                .as_array()
                .expect("hunks should be an array")
                .is_empty(),
            "expected at least one hunk on the after side: {json}"
        );
        Ok(())
    }

    #[test]
    fn build_side_sets_reference_line_to_the_enclosing_function() {
        let contents = "fn unrelated() {}\n\nfn parse_args() {\n    let x = 1;\n}\n";
        let ranges = vec![RangeMatch {
            source: TextRange::new(3, 4, 4, 0),
            destination: TextRange::new(3, 4, 4, 0),
            operation: TextOperation::Update,
        }];

        let side = build_side(contents, Path::new("sample.rs"), &ranges);
        assert_eq!(side.hunks.len(), 1);
        assert_eq!(
            side.hunks[0].reference_line,
            Some(2),
            "row 3 (`let x = 1;`) is inside `fn parse_args` at row 2"
        );
    }
}
