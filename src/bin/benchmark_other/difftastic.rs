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

// Split out of benchmark_other.rs (the `difftastic`-cluster functions) purely to shrink
// that file's visible size - no behavior change.

use anyhow::{Context, Result, bail};
use codediff::code::{Code, Language};
use codediff::diff::text_range::TextRange;
use std::process::Command;

use super::{external_tool_bin, span_on_row, write_temp_pair};

/// Path to the `difft` binary, from the `DIFFT_BIN` environment variable - not bundled or
/// auto-installed, same reasoning as `gumtree_bin`. Install with
/// `cargo install --root /var/tmp/codediff-tools difftastic` (installs the `difft` binary to
/// `/var/tmp/codediff-tools/bin/difft`, outside this checkout and outside the system-wide cargo
/// bin directory) and point `DIFFT_BIN` at the result.
pub(crate) fn difftastic_bin() -> Result<std::path::PathBuf> {
    external_tool_bin("DIFFT_BIN", "point it at a built `difft` binary")
}

/// File extension difftastic's own language auto-detection maps back to `language`, confirmed
/// live against `difft --list-languages` (difftastic v0.69.0, 2026-07). `None` for every corpus
/// language difftastic has no grammar for at all: `Bazel` (also not tree-sitter-parseable by
/// codediff itself - see `Code`'s own doc comment), `MarkDown`, `ProtoBuf`, and `Vimscript`.
/// `Language::Lisp` maps to `.el` - codediff's own extension table (`code/language.rs`) treats
/// `Language::Lisp` as Emacs Lisp specifically, and difftastic lists Emacs Lisp and Common Lisp as
/// separate languages with disjoint extensions, so `.el` is the correct, unambiguous match.
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

/// Runs `difft --display json` and reduces its output to the same per-line touched signal every
/// other `ExternalTool` produces. `DFT_UNSTABLE=yes` is required by difftastic itself - JSON
/// output is explicitly marked an unstable feature that may change format in a future release
/// (confirmed live, difftastic v0.69.0 refuses `--display json` without it) - see
/// `difftastic_touched_from_json` for the schema this code depends on.
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

/// Difftastic's JSON has one relevant field, `chunks`: a list of hunks, each a list of line
/// entries. Each entry has an optional `lhs`/`rhs`, each `{line_number, changes}` - `line_number`
/// is 0-indexed (confirmed empirically against this project's own fixtures, 2026-07: a change on
/// a file's second line reports `line_number: 1`) and `changes` is empty for a context line shown
/// only for readability, non-empty for a line difftastic considers actually touched. Confirmed
/// live that `chunks` only ever contains touched lines, never surrounding context, in JSON mode
/// (unlike difftastic's own terminal display, which does show context) - so every `lhs`/`rhs`
/// entry present here is touched, `changes` non-empty or not. A file with `status: "unchanged"`
/// has no `chunks` key at all, not an empty array.
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
            if let Some(line_number) = entry["lhs"]["line_number"].as_u64() {
                if let Some(slot) = before_touched.get_mut(line_number as usize) {
                    *slot = true;
                }
            }
            if let Some(line_number) = entry["rhs"]["line_number"].as_u64() {
                if let Some(slot) = after_touched.get_mut(line_number as usize) {
                    *slot = true;
                }
            }
        }
    }

    Ok((before_touched, after_touched))
}

/// difftastic's changed spans. Its `--display json` chunks carry, per side, a `line_number` plus a
/// `changes` array of `{start, end}` column offsets into that line - the token-level highlighting
/// it draws - so each `changes` entry becomes one single-line span. An entry with an empty
/// `changes` array is a line difftastic reports as part of a chunk without marking any token on it
/// (context within a changed region), and contributes no span.
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
