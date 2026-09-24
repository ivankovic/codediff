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

use super::{external_tool_bin, span_on_row, write_temp_pair};

/// Path to the `difft` binary, from `DIFFT_BIN`. Install with
/// `cargo install --root /var/tmp/codediff-tools difftastic`.
pub(crate) fn difftastic_bin() -> Result<std::path::PathBuf> {
    external_tool_bin("DIFFT_BIN", "point it at a built `difft` binary")
}

/// File extension that difftastic's auto-detection maps to `language` (`difft --list-languages`);
/// `None` where difftastic has no grammar. `Lisp` is `.el` because codediff's `Lisp` is Emacs Lisp,
/// which difftastic keeps separate from Common Lisp.
pub(crate) fn difftastic_extension(language: Language) -> Option<&'static str> {
    match language {
        Language::Rust => Some("rs"),
        Language::Python => Some("py"),
        Language::Go => Some("go"),
        Language::Kotlin => Some("kt"),
        Language::Java => Some("java"),
        Language::JavaScript => Some("js"),
        Language::TypeScript => Some("ts"),
        Language::TSX => Some("tsx"),
        Language::C => Some("c"),
        Language::CPP => Some("cpp"),
        Language::CSharp => Some("cs"),
        Language::Ruby => Some("rb"),
        Language::PHP => Some("php"),
        Language::Swift => Some("swift"),
        Language::Scala => Some("scala"),
        Language::LUA => Some("lua"),
        Language::CSS => Some("css"),
        Language::HTML => Some("html"),
        Language::JSON => Some("json"),
        Language::R => Some("R"),
        Language::ShellScript => Some("sh"),
        Language::XML => Some("xml"),
        Language::YAML => Some("yaml"),
        Language::SQL => Some("sql"),
        Language::Dart => Some("dart"),
        Language::Lisp => Some("el"),
        _ => None,
    }
}

/// Per-line touched flags from `difft --display json`, which difftastic refuses without
/// `DFT_UNSTABLE=yes`.
pub(crate) fn difftastic_line_labels(
    before: &Code,
    after: &Code,
) -> Result<(Vec<bool>, Vec<bool>)> {
    let language = before.metadata.language.unwrap_or_default();
    let ext = difftastic_extension(language)
        .with_context(|| format!("no difftastic extension mapping for {language:?}"))?;
    let difft = difftastic_bin()?;

    let (before_file, after_file) = write_temp_pair(before, after, Some(&format!(".{ext}")))?;

    let output = Command::new(&difft)
        .args(["--display", "json"])
        .env("DFT_UNSTABLE", "yes")
        .arg(before_file.path())
        .arg(after_file.path())
        .output()
        .with_context(|| format!("running {difft:?} --display json"))?;
    if !output.status.success() {
        bail!(
            "difft exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("parsing difft JSON output")?;
    difftastic_touched_from_json(before, after, &json)
}

/// Reads `chunks`: lists of entries with optional `lhs`/`rhs` `{line_number, changes}`,
/// `line_number` 0-indexed. JSON mode lists no context lines, so every present side is touched
/// even with empty `changes`. An unchanged file has no `chunks` key at all.
pub(crate) fn difftastic_touched_from_json(
    before: &Code,
    after: &Code,
    json: &serde_json::Value,
) -> Result<(Vec<bool>, Vec<bool>)> {
    let mut before_touched = vec![false; before.contents.split('\n').count()];
    let mut after_touched = vec![false; after.contents.split('\n').count()];

    let Some(chunks) = json["chunks"].as_array() else {
        return Ok((before_touched, after_touched));
    };

    for chunk in chunks {
        let entries = chunk
            .as_array()
            .context("difft JSON chunk is not an array")?;
        for entry in entries {
            if let Some(line_number) = entry["lhs"]["line_number"].as_u64()
                && let Some(slot) = before_touched.get_mut(line_number as usize)
            {
                *slot = true;
            }
            if let Some(line_number) = entry["rhs"]["line_number"].as_u64()
                && let Some(slot) = after_touched.get_mut(line_number as usize)
            {
                *slot = true;
            }
        }
    }

    Ok((before_touched, after_touched))
}

/// difftastic's changed spans: one single-line span per `{start, end}` in each side's `changes`.
pub(crate) fn difftastic_node_spans(
    before: &Code,
    after: &Code,
) -> Result<(Vec<TextRange>, Vec<TextRange>)> {
    let language = before.metadata.language.unwrap_or_default();
    let ext = difftastic_extension(language)
        .with_context(|| format!("no difftastic extension mapping for {language:?}"))?;
    let difft = difftastic_bin()?;
    let (before_file, after_file) = write_temp_pair(before, after, Some(&format!(".{ext}")))?;

    let output = Command::new(&difft)
        .args(["--display", "json"])
        .env("DFT_UNSTABLE", "yes")
        .arg(before_file.path())
        .arg(after_file.path())
        .output()
        .with_context(|| format!("running {difft:?} --display json"))?;
    if !output.status.success() {
        bail!(
            "difft exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("parsing difft JSON output")?;

    let mut before_spans = Vec::new();
    let mut after_spans = Vec::new();
    let Some(chunks) = json["chunks"].as_array() else {
        return Ok((before_spans, after_spans));
    };
    for chunk in chunks {
        let entries = chunk
            .as_array()
            .context("difft JSON chunk is not an array")?;
        for entry in entries {
            for (key, spans) in [("lhs", &mut before_spans), ("rhs", &mut after_spans)] {
                let Some(side) = entry.get(key) else { continue };
                let Some(line) = side["line_number"].as_u64() else {
                    continue;
                };
                let Some(changes) = side["changes"].as_array() else {
                    continue;
                };
                for change in changes {
                    let (Some(start), Some(end)) =
                        (change["start"].as_u64(), change["end"].as_u64())
                    else {
                        continue;
                    };
                    spans.push(span_on_row(line as usize, start as usize, end as usize));
                }
            }
        }
    }
    Ok((before_spans, after_spans))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn difftastic_touched_from_json_marks_a_side_with_empty_changes_as_touched() {
        let code = Code::from_string("a\nb\nc\n", &Language::Rust);
        let json = serde_json::json!({"chunks": [[
            {"lhs": {"line_number": 1, "changes": []}},
            {"rhs": {"line_number": 2, "changes": [{"start": 0, "end": 1}]}},
        ]]});
        let (before, after) = difftastic_touched_from_json(&code, &code, &json).unwrap();
        assert_eq!(before, vec![false, true, false, false]);
        assert_eq!(after, vec![false, false, true, false]);
    }

    #[test]
    fn difftastic_touched_from_json_without_chunks_touches_nothing() {
        let code = Code::from_string("a\nb\n", &Language::Rust);
        let json = serde_json::json!({"status": "unchanged"});
        let (before, after) = difftastic_touched_from_json(&code, &code, &json).unwrap();
        assert!(!before.contains(&true) && !after.contains(&true));
    }
}
