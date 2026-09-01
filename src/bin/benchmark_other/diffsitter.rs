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

// Split out of benchmark_other.rs (the `diffsitter`-cluster functions) purely to shrink
// that file's visible size - no behavior change.

use anyhow::{Context, Result, bail};
use codediff::code::{Code, Language};
use codediff::diff::text_range::TextRange;
use std::process::Command;

use super::{external_tool_bin, merge_spans, write_temp_pair};

/// Path to the `diffsitter` binary, from the `DIFFSITTER_BIN` environment variable - see
/// `difftastic_bin`. Install with
/// `cargo install --root /var/tmp/codediff-tools diffsitter`.
pub(crate) fn diffsitter_bin() -> Result<std::path::PathBuf> {
    external_tool_bin("DIFFSITTER_BIN", "point it at a built `diffsitter` binary")
}

/// `-t <FILE_TYPE>` value diffsitter needs for `language`, confirmed live against `diffsitter
/// list` (diffsitter v0.9.0, 2026-07) - a much narrower compiled-in language set than difftastic
/// or GumTree: no `JavaScript`, `HTML`, `Kotlin`, `LUA`, `R`, `Scala`, `Swift`, `XML`, or `YAML`,
/// none of which this build was compiled with a grammar for at all (`diffsitter list`'s output is
/// the full, exhaustive set - there is no generic fallback parser).
pub(crate) fn diffsitter_file_type(language: Language) -> Option<&'static str> {
    match language {
        Language::ShellScript => Some("bash"),
        Language::C => Some("c"),
        Language::CSharp => Some("c_sharp"),
        Language::CPP => Some("cpp"),
        Language::CSS => Some("css"),
        Language::Go => Some("go"),
        Language::Java => Some("java"),
        Language::JSON => Some("json"),
        Language::MarkDown => Some("markdown"),
        Language::PHP => Some("php"),
        Language::Python => Some("python"),
        Language::Ruby => Some("ruby"),
        Language::Rust => Some("rust"),
        Language::TSX => Some("tsx"),
        Language::TypeScript => Some("typescript"),
        _ => None,
    }
}

/// Runs `diffsitter -r json` and reduces its output to the same per-line touched signal every
/// other `ExternalTool` produces. `-t <file-type>` is passed explicitly (see
/// `diffsitter_file_type`) rather than relying on the temp file's extension, the same reasoning as
/// GumTree's explicit `-g <generator>` in `gumtree_line_labels` - an explicit choice documents
/// exactly which parser ran, rather than leaving it to auto-detection.
pub(crate) fn diffsitter_line_labels(
    before: &Code,
    after: &Code,
) -> Result<(Vec<bool>, Vec<bool>)> {
    let language = before.metadata.language.unwrap_or_default();
    let file_type = diffsitter_file_type(language)
        .with_context(|| format!("no diffsitter file type mapping for {language:?}"))?;
    let diffsitter = diffsitter_bin()?;

    let (before_file, after_file) = write_temp_pair(before, after, None)?;

    let output = Command::new(&diffsitter)
        .args(["-n", "-r", "json", "-t", file_type])
        .arg(before_file.path())
        .arg(after_file.path())
        .output()
        .with_context(|| format!("running {diffsitter:?} -r json -t {file_type}"))?;
    if !output.status.success() {
        bail!(
            "diffsitter exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("parsing diffsitter JSON output")?;
    diffsitter_touched_from_json(before, after, &json)
}

/// Diffsitter's JSON has one relevant field, `hunks`: a list of hunk objects, each with exactly
/// one of two keys, `"Old"` or `"New"` (confirmed live, diffsitter v0.9.0: never both in the same
/// hunk object, even for a same-line before/after change - that shows up as two separate hunks,
/// one `Old` and one `New`). Each key holds a list of `{line_index, entries}`, `line_index`
/// 0-indexed the same way as `difftastic_touched_from_json`'s `line_number`. `entries` (the
/// character-level tokens changed on that line) is not needed here - `line_index`'s presence
/// alone means diffsitter considers that line touched.
pub(crate) fn diffsitter_touched_from_json(
    before: &Code,
    after: &Code,
    json: &serde_json::Value,
) -> Result<(Vec<bool>, Vec<bool>)> {
    let mut before_touched = vec![false; before.contents.split('\n').count()];
    let mut after_touched = vec![false; after.contents.split('\n').count()];

    let hunks = json["hunks"]
        .as_array()
        .context("diffsitter JSON has no `hunks` array")?;

    for hunk in hunks {
        let hunk = hunk
            .as_object()
            .context("diffsitter JSON hunk is not an object")?;
        for (side, touched) in [("Old", &mut before_touched), ("New", &mut after_touched)] {
            let Some(entries) = hunk.get(side).and_then(|v| v.as_array()) else {
                continue;
            };
            for entry in entries {
                if let Some(line_index) = entry["line_index"].as_u64() {
                    if let Some(slot) = touched.get_mut(line_index as usize) {
                        *slot = true;
                    }
                }
            }
        }
    }

    Ok((before_touched, after_touched))
}

/// diffsitter's changed spans, from the same `-r json` output `diffsitter_line_labels` parses -
/// but keeping each entry's `start_position`/`end_position` (row + column) instead of only its
/// `line_index`. diffsitter emits one entry per *character*, so the spans come out extremely
/// fine-grained; they're merged by `merge_spans` before scoring rather than tested one at a time.
pub(crate) fn diffsitter_node_spans(
    before: &Code,
    after: &Code,
) -> Result<(Vec<TextRange>, Vec<TextRange>)> {
    let language = before.metadata.language.unwrap_or_default();
    let file_type = diffsitter_file_type(language)
        .with_context(|| format!("no diffsitter file type mapping for {language:?}"))?;
    let diffsitter = diffsitter_bin()?;
    let (before_file, after_file) = write_temp_pair(before, after, None)?;

    let output = Command::new(&diffsitter)
        .args(["-n", "-r", "json", "-t", file_type])
        .arg(before_file.path())
        .arg(after_file.path())
        .output()
        .with_context(|| format!("running {diffsitter:?} -r json -t {file_type}"))?;
    if !output.status.success() {
        bail!(
            "diffsitter exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("parsing diffsitter JSON output")?;

    let mut before_spans = Vec::new();
    let mut after_spans = Vec::new();
    let hunks = json["hunks"]
        .as_array()
        .context("diffsitter JSON has no `hunks` array")?;
    for hunk in hunks {
        let hunk = hunk
            .as_object()
            .context("diffsitter JSON hunk is not an object")?;
        for (side, spans) in [("Old", &mut before_spans), ("New", &mut after_spans)] {
            let Some(lines) = hunk.get(side).and_then(|v| v.as_array()) else {
                continue;
            };
            for line in lines {
                let Some(entries) = line["entries"].as_array() else {
                    continue;
                };
                for entry in entries {
                    let (Some(start_row), Some(start_col), Some(end_row), Some(end_col)) = (
                        entry["start_position"]["row"].as_u64(),
                        entry["start_position"]["column"].as_u64(),
                        entry["end_position"]["row"].as_u64(),
                        entry["end_position"]["column"].as_u64(),
                    ) else {
                        continue;
                    };
                    spans.push(TextRange {
                        start_row: start_row as usize,
                        start_column: start_col as usize,
                        end_row: end_row as usize,
                        end_column: end_col as usize,
                    });
                }
            }
        }
    }
    Ok((merge_spans(before_spans), merge_spans(after_spans)))
}
