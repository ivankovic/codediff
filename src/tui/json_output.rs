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

//! Machine-readable counterpart to `tui::headless`: prints a diff as one JSON object on stdout,
//! for editor/tool integrations that place highlights on their own buffers.
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
//!   "large_residual": false,
//!   "summary": "comment_only"
//! }
//! ```
//!
//! If either side is binary, `main.rs` answers with `binary_diff_json` instead: the same object
//! with `"binary": true`, empty `hunks` and no `summary`. `binary` is omitted for text diffs.
//!
//! Each side's `hunks` are ranges in that side's own file. Rows and columns are 0-indexed.
//!
//! **Columns are byte offsets within their row**, as tree-sitter reports them. Neovim takes them
//! directly; VS Code / LSP need UTF-16 code units and character-offset consumers must decode the
//! row, both per line. No second coordinate space is offered: it could disagree with the first,
//! and every consumer already has the line's text.
//!
//! `summary` is the diff's overall shape (`no_changes`, `new_file`, `deleted_file`,
//! `whitespace_only`, `comment_only`, `refactor_moved_only`), omitted for an ordinary mix of edits.
//!
//! File contents are not embedded: both paths are always real files the caller can open, and a
//! second copy would be one more thing to keep in sync.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;

use crate::code::Code;
use crate::code::language::{language_for_path, language_for_path_and_content};
use crate::diff::text::{
    DiffSummary, RangeMatch, RenderOptions, TextOperation, ranges_for_options,
    summarize_diff_with_comment_check,
};
use crate::diff::text_range::TextRange;
use crate::tui::actions::DiffSessionData;
use crate::tui::app::compute_diff_with_options;
use crate::tui::headless::nearest_reference_line;

/// A `TextRange` for JSON. Local, like the other `Json*` types, so `diff` stays serde-free.
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

/// `TextOperation` minus `Identical`/`NotYetSet`: unchanged text gets no hunk.
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

/// `diff::text::DiffSummary` as a snake_case tag a script can match on.
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
    /// Only set for `Move`. Other operations carry a synthetic bookkeeping anchor in
    /// `destination`, not a real position, which would look like a jump target but is not one.
    #[serde(skip_serializing_if = "Option::is_none")]
    move_target: Option<JsonRange>,
    /// The row of the nearest enclosing named declaration, as in `headless`'s `@` breadcrumb.
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
    /// The diff left an unusually large unmatched residual. `headless` prints this to stderr; here
    /// it is a field so a JSON consumer need not watch stderr.
    large_residual: bool,
    /// `None` for an ordinary mix of edits.
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<JsonDiffSummary>,
    /// At least one side is binary and no diff was computed. A flag, because empty hunks alone
    /// look like identical files.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    binary: bool,
}

/// Re-parses `contents` for `nearest_reference_line`; not on a hot path, so the redundant parse
/// is fine.
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

fn build_diff(data: &DiffSessionData, large_residual: bool) -> JsonDiff {
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
        large_residual,
        summary,
        binary: false,
    }
}

/// The `--mode json` answer when a side is binary: the usual shape with `binary` set, no hunks
/// and no `summary`. `language` is still filled in, since it describes the file, not the diff.
/// Returns the text so `main.rs` keeps the single print site.
pub fn binary_diff_json(before: &Path, after: &Path) -> Result<String> {
    let side = |path: &Path| JsonSide {
        path: path.to_path_buf(),
        language: language_for_path(path).map(|lang| lang.to_string()),
        hunks: Vec::new(),
    };
    let diff = JsonDiff {
        before: side(before),
        after: side(after),
        large_residual: false,
        summary: None,
        binary: true,
    };
    Ok(serde_json::to_string_pretty(&diff)?)
}

/// Entry point for `--mode json`: computes the diff as `headless::run` does and prints it.
/// Returns whether the files differ byte-wise, for `main.rs`'s exit code.
pub fn run(before: &Path, after: &Path, render_options: RenderOptions) -> Result<bool> {
    let (mut data, large_residual) = compute_diff_with_options(before, after, render_options)?;
    // A presentation filter over the finished diff, not a different diff.
    data.before_ranges =
        ranges_for_options(&data.before_ranges, &data.before_contents, render_options);
    data.after_ranges =
        ranges_for_options(&data.after_ranges, &data.after_contents, render_options);
    let diff = build_diff(&data, large_residual);
    println!("{}", serde_json::to_string_pretty(&diff)?);
    Ok(std::fs::read(before)? != std::fs::read(after)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::compute_diff;

    /// One changed line: a Delete on the before side paired with an Insert on the after side.
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
    fn build_diff_carries_the_large_residual_flag_through_as_a_field() {
        assert!(build_diff(&sample_data(), true).large_residual);
        assert!(!build_diff(&sample_data(), false).large_residual);
    }

    #[test]
    fn build_diff_omits_summary_for_an_ordinary_mixed_edit() {
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

        let (data, large_residual) = compute_diff(&before_path, &after_path)?;
        let diff = build_diff(&data, large_residual);
        let json = serde_json::to_value(&diff)?;

        assert!(json.get("before").is_some());
        assert!(json.get("after").is_some());
        assert!(json.get("large_residual").is_some());
        // Only assert that hunks exist; which operation the rename becomes is the engine's call.
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

    #[test]
    fn binary_diff_json_keeps_language_but_has_no_hunks_or_summary() -> Result<()> {
        let json: serde_json::Value =
            serde_json::from_str(&binary_diff_json(Path::new("a.rs"), Path::new("b.png"))?)?;
        assert_eq!(json["binary"], true);
        assert_eq!(json["before"]["language"], "Rust");
        assert!(json["before"]["hunks"].as_array().unwrap().is_empty());
        assert!(json["after"]["hunks"].as_array().unwrap().is_empty());
        assert!(json.get("summary").is_none(), "{json}");
        Ok(())
    }

    #[test]
    fn text_diff_json_omits_the_binary_field() {
        let json = serde_json::to_value(build_diff(&sample_data(), false)).unwrap();
        assert!(json.get("binary").is_none(), "{json}");
    }
}
