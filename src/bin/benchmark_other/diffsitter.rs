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

use anyhow::{Context, Result, bail};
use codediff::code::{Code, Language};
use codediff::diff::text_range::TextRange;
use std::process::Command;

use super::{external_tool_bin, merge_spans, write_temp_pair};

/// Path to the `diffsitter` binary, from `DIFFSITTER_BIN`. Install with
/// `cargo install --root /var/tmp/codediff-tools diffsitter`.
pub(crate) fn diffsitter_bin() -> Result<std::path::PathBuf> {
    external_tool_bin("DIFFSITTER_BIN", "point it at a built `diffsitter` binary")
}

/// `-t <FILE_TYPE>` value for `language`, matching `diffsitter list`. `None` means diffsitter has
/// no grammar for it; there is no generic fallback parser.
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

/// Per-line touched flags from `diffsitter -r json`. `-t` is explicit rather than inferred from the
/// temp file's extension, so it is certain which parser ran.
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

/// Reads `hunks`: each hunk has an `"Old"` or `"New"` key holding `{line_index, entries}` lines,
/// `line_index` 0-indexed. A listed line is touched.
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
                if let Some(line_index) = entry["line_index"].as_u64()
                    && let Some(slot) = touched.get_mut(line_index as usize)
                {
                    *slot = true;
                }
            }
        }
    }

    Ok((before_touched, after_touched))
}

/// diffsitter's changed spans from each entry's `start_position`/`end_position`. diffsitter emits
/// one entry per character, so spans are merged before scoring.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diffsitter_touched_from_json_reads_old_and_new_hunks_as_zero_indexed_lines() {
        let code = Code::from_string("a\nb\nc\n", &Language::Rust);
        let json = serde_json::json!({"hunks": [
            {"Old": [{"line_index": 0, "entries": []}]},
            {"New": [{"line_index": 2, "entries": []}]},
        ]});
        let (before, after) = diffsitter_touched_from_json(&code, &code, &json).unwrap();
        assert_eq!(before, vec![true, false, false, false]);
        assert_eq!(after, vec![false, false, true, false]);
    }
}
