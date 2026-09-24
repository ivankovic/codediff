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
use std::collections::HashMap;
use std::io::Write;
use std::process::Command;

use super::{char_offset_table, external_tool_bin, span_from_char_offsets, write_temp_pair};

/// `(generator id, file extension)` for every corpus language the installed GumTree build
/// (v4.0.0-beta8) registers a generator for. Re-verify by running each entry against a real
/// fixture whenever the build changes; `gumtree list GENERATORS` alone is not reliable.
///
/// Passed explicitly via `-g` because GumTree's auto-detect regex for C# (`\.[cs]$`) never matches
/// `.cs`. `*-treesitter-ng` is preferred over `*-srcml`: no extra binary, and the same parser family
/// codediff uses.
pub(crate) fn gumtree_generator(language: Language) -> Option<(&'static str, &'static str)> {
    match language {
        Language::Java => Some(("java-jdt", "java")), // Stable
        Language::CSS => Some(("css-phcss", "css")),  // Stable
        Language::Rust => Some(("rust-treesitter-ng", "rs")), // Testing
        Language::Kotlin => Some(("kotlin-treesitter-ng", "kt")), // Testing
        Language::C => Some(("c-treesitter-ng", "c")), // Testing
        Language::Go => Some(("go-treesitter-ng", "go")), // Testing
        Language::Python => Some(("python-treesitter-ng", "py")), // Testing
        Language::TypeScript => Some(("ts-treesitter-ng", "ts")), // Testing
        Language::JavaScript => Some(("js-treesitter-ng", "js")), // Testing
        Language::CSharp => Some(("cs-treesitter-ng", "cs")), // Testing
        Language::PHP => Some(("php-treesitter-ng", "php")),
        Language::Ruby => Some(("ruby-treesitter-ng", "rb")),
        Language::Swift => Some(("swift-treesitter-ng", "swift")),
        Language::R => Some(("r-treesitter-ng", "r")),
        Language::XML => Some(("xml-jsoup", "xml")),
        Language::YAML => Some(("yaml-snakeyaml", "yaml")),
        Language::CPP => Some(("cpp-treesitter-ng", "cpp")), // Testing
        Language::TSX => Some(("tsx-treesitter-ng", "tsx")), // Testing
        // No JSON: beta8 does not register `json-jackson`, whatever a generator listing says.
        _ => None,
    }
}

/// Path to GumTree's built `bin/gumtree` script, from `GUMTREE_BIN`. Errors rather than skipping:
/// a missing binary for a supported language is a configuration problem.
pub(crate) fn gumtree_bin() -> Result<std::path::PathBuf> {
    external_tool_bin(
        "GUMTREE_BIN",
        "point it at GumTree's built bin/gumtree script",
    )
}

/// Per-line touched flags from `gumtree textdiff -f JSON`.
///
/// - `matches`: `{src, dest}` pairs of `"KIND[: text] [start,end]"` node references, `[start,end]`
///   a half-open *character* range.
/// - `actions`: `insert-*` name a dest-side node, `delete-*` a src-side one. `update-node` and
///   `move-*` name the src-side node; its dest position is found by looking the same string up in
///   `matches`.
pub(crate) fn gumtree_line_labels(before: &Code, after: &Code) -> Result<(Vec<bool>, Vec<bool>)> {
    let language = before.metadata.language.unwrap_or_default();
    let (generator, ext) = gumtree_generator(language)
        .with_context(|| format!("no GumTree generator for {language:?}"))?;
    let gumtree = gumtree_bin()?;

    let (before_file, after_file) = write_temp_pair(before, after, Some(&format!(".{ext}")))?;

    let output = Command::new(&gumtree)
        .args(["textdiff", "-g", generator, "-f", "JSON"])
        .arg(before_file.path())
        .arg(after_file.path())
        .output()
        .with_context(|| format!("running {gumtree:?} textdiff -g {generator}"))?;
    if !output.status.success() {
        bail!(
            "gumtree exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("parsing gumtree JSON output")?;
    gumtree_touched_from_json(before, after, &json)
}

/// The pure half of [`gumtree_line_labels`]; see its doc comment for the schema.
pub(crate) fn gumtree_touched_from_json(
    before: &Code,
    after: &Code,
    json: &serde_json::Value,
) -> Result<(Vec<bool>, Vec<bool>)> {
    let matches = json["matches"]
        .as_array()
        .context("gumtree JSON has no `matches` array")?;
    let actions = json["actions"]
        .as_array()
        .context("gumtree JSON has no `actions` array")?;

    let src_to_dest: HashMap<&str, &str> = matches
        .iter()
        .filter_map(|m| Some((m["src"].as_str()?, m["dest"].as_str()?)))
        .collect();

    let mut before_touched = vec![false; before.contents.split('\n').count()];
    let mut after_touched = vec![false; after.contents.split('\n').count()];

    let mark = |touched: &mut [bool], contents: &str, node_ref: &str| -> Result<()> {
        let (start, end) = gumtree_node_offsets(node_ref)?;
        for line in gumtree_line_range(contents, start, end) {
            if let Some(slot) = touched.get_mut(line) {
                *slot = true;
            }
        }
        Ok(())
    };

    for action in actions {
        let kind = action["action"]
            .as_str()
            .context("gumtree action missing `action`")?;
        let tree = action["tree"]
            .as_str()
            .context("gumtree action missing `tree`")?;
        match kind {
            "insert-tree" | "insert-node" => mark(&mut after_touched, &after.contents, tree)?,
            "delete-tree" | "delete-node" => mark(&mut before_touched, &before.contents, tree)?,
            "update-node" | "move-tree" | "move-node" => {
                mark(&mut before_touched, &before.contents, tree)?;
                match src_to_dest.get(tree) {
                    Some(dest) => mark(&mut after_touched, &after.contents, dest)?,
                    // Not expected; reported rather than silently under-counting the after side.
                    None => eprintln!(
                        "gumtree: no `matches` entry for {kind} tree {tree:?}, after-side line(s) not marked"
                    ),
                }
            }
            other => bail!("unrecognized gumtree action kind {other:?}"),
        }
    }

    Ok((before_touched, after_touched))
}

/// Per-fixture GumTree timings from one persistent JVM (`gumtree_warm_ms`), so JVM startup and
/// JIT warmup do not dominate. An additional column; `gumtree_ms` stays the per-process cost.
///
/// `Ok(None)` when the batch driver is unavailable (`GUMTREE_BIN` unset or
/// `research/drivers/gumtree-batch/build.sh` not run). Stdin is written from a second thread so
/// a response larger than the pipe buffer cannot deadlock against unwritten requests.
pub(crate) fn gumtree_warm_batch(
    fixtures: &[(&str, &Code, &Code)],
) -> Result<Option<HashMap<String, f64>>> {
    let Ok(gumtree) = gumtree_bin() else {
        return Ok(None);
    };
    let Some(gumtree_dir) = gumtree.parent().and_then(|bin| bin.parent()) else {
        return Ok(None);
    };
    let jar = gumtree_dir.join("lib/gumtree.jar");
    let driver_out = std::path::Path::new("research/drivers/gumtree-batch/out");
    if !jar.is_file() || !driver_out.join("BatchDriver.class").is_file() {
        eprintln!(
            "gumtree_warm_ms: skipping (build the batch driver first: GUMTREE_BIN=... research/drivers/gumtree-batch/build.sh)"
        );
        return Ok(None);
    }

    // Kept alive until the driver, which reads these paths lazily, is done.
    let mut before_files = Vec::with_capacity(fixtures.len());
    let mut after_files = Vec::with_capacity(fixtures.len());
    let mut requests = String::new();
    for (name, before, after) in fixtures {
        let language = before.metadata.language.unwrap_or_default();
        let Some((generator, ext)) = gumtree_generator(language) else {
            continue;
        };
        let (before_file, after_file) = write_temp_pair(before, after, Some(&format!(".{ext}")))?;
        requests.push_str(
            &serde_json::json!({
                "id": name,
                "generator": generator,
                "before": before_file.path().display().to_string(),
                "after": after_file.path().display().to_string(),
            })
            .to_string(),
        );
        requests.push('\n');
        before_files.push(before_file);
        after_files.push(after_file);
    }

    let mut child = Command::new("java")
        .args([
            "-cp",
            &format!("{}:{}", jar.display(), driver_out.display()),
            "BatchDriver",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("spawning the GumTree batch driver (is `java` on PATH?)")?;
    let mut stdin = child
        .stdin
        .take()
        .context("batch driver child has no stdin")?;
    let writer = std::thread::spawn(move || stdin.write_all(requests.as_bytes()));
    let output = child
        .wait_with_output()
        .context("waiting for the GumTree batch driver")?;
    writer
        .join()
        .expect("batch driver stdin-writer thread panicked")
        .context("writing batch driver stdin")?;
    if !output.status.success() {
        bail!(
            "GumTree batch driver exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let mut results = HashMap::new();
    let mut failures = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let json: serde_json::Value = serde_json::from_str(line)
            .with_context(|| format!("parsing batch driver response line {line:?}"))?;
        let id = json["id"]
            .as_str()
            .context("batch driver response missing `id`")?
            .to_string();
        // A fixture GumTree cannot parse is a per-fixture gap, as on the per-process path;
        // omitted ids read as "not scored", not zero.
        if let Some(error) = json["error"].as_str() {
            failures.push(format!("  {id}: {error}"));
            continue;
        }
        let ms = json["ms"]
            .as_f64()
            .context("batch driver response missing `ms`")?;
        results.insert(id, ms);
    }
    if !failures.is_empty() {
        eprintln!(
            "gumtree_warm_ms: {} of {} fixtures failed in the batch driver and are unscored:\n{}",
            failures.len(),
            fixtures.len(),
            failures.join("\n")
        );
    }
    Ok(Some(results))
}

/// The `[start,end]` suffix of a node reference like `"SimpleName: foo [12,15]"`. Anchored to the
/// end because a node's own text can contain brackets.
pub(crate) fn gumtree_node_offsets(node_ref: &str) -> Result<(usize, usize)> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"\[(\d+),(\d+)\]$").unwrap());
    let caps = re
        .captures(node_ref)
        .with_context(|| format!("no [start,end] suffix in {node_ref:?}"))?;
    let start: usize = caps[1].parse()?;
    let end: usize = caps[2].parse()?;
    Ok((start, end))
}

/// 0-indexed lines the half-open character range `[start, end)` touches. An `end` on a line
/// boundary does not pull in the next line; an `end` past the text clamps.
pub(crate) fn gumtree_line_range(
    contents: &str,
    start: usize,
    end: usize,
) -> std::ops::RangeInclusive<usize> {
    // Character offsets, not byte offsets: slicing at them directly panics on multi-byte text.
    let byte_offset_of_char = |char_offset: usize| -> usize {
        contents
            .char_indices()
            .nth(char_offset)
            .map(|(byte_idx, _)| byte_idx)
            .unwrap_or(contents.len())
    };
    let line_of = |offset: usize| {
        contents[..byte_offset_of_char(offset)]
            .matches('\n')
            .count()
    };
    line_of(start)..=line_of(end.saturating_sub(1).max(start))
}

/// GumTree's changed spans: each action's character range, on the sides [`gumtree_line_labels`]
/// documents.
pub(crate) fn gumtree_node_spans(
    before: &Code,
    after: &Code,
) -> Result<(Vec<TextRange>, Vec<TextRange>)> {
    let language = before.metadata.language.unwrap_or_default();
    let (generator, ext) = gumtree_generator(language)
        .with_context(|| format!("no GumTree generator for {language:?}"))?;
    let gumtree = gumtree_bin()?;
    let (before_file, after_file) = write_temp_pair(before, after, Some(&format!(".{ext}")))?;

    let output = Command::new(&gumtree)
        .args(["textdiff", "-g", generator, "-f", "JSON"])
        .arg(before_file.path())
        .arg(after_file.path())
        .output()
        .with_context(|| format!("running {gumtree:?} textdiff -g {generator}"))?;
    if !output.status.success() {
        bail!(
            "gumtree exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("parsing gumtree JSON output")?;

    let matches = json["matches"]
        .as_array()
        .context("gumtree JSON has no `matches` array")?;
    let actions = json["actions"]
        .as_array()
        .context("gumtree JSON has no `actions` array")?;
    let src_to_dest: HashMap<&str, &str> = matches
        .iter()
        .filter_map(|m| Some((m["src"].as_str()?, m["dest"].as_str()?)))
        .collect();

    let before_table = char_offset_table(&before.contents);
    let after_table = char_offset_table(&after.contents);
    let mut before_spans = Vec::new();
    let mut after_spans = Vec::new();

    for action in actions {
        let Some(action_type) = action["action"].as_str() else {
            continue;
        };
        let Some(tree) = action["tree"].as_str() else {
            continue;
        };
        let (start, end) = gumtree_node_offsets(tree)?;
        match action_type {
            "insert-tree" | "insert-node" => {
                after_spans.push(span_from_char_offsets(&after_table, start, end));
            }
            "delete-tree" | "delete-node" => {
                before_spans.push(span_from_char_offsets(&before_table, start, end));
            }
            _ => {
                before_spans.push(span_from_char_offsets(&before_table, start, end));
                if let Some(dest) = src_to_dest.get(tree) {
                    let (d_start, d_end) = gumtree_node_offsets(dest)?;
                    after_spans.push(span_from_char_offsets(&after_table, d_start, d_end));
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
    fn gumtree_node_offsets_reads_the_final_range_when_the_text_contains_brackets() {
        assert_eq!(
            gumtree_node_offsets("ArrayInit: [1,2] [30,35]").unwrap(),
            (30, 35)
        );
    }

    /// Positions are half-open character ranges: "b" is `[2,3]` on line 1 of both sides.
    #[test]
    fn gumtree_touched_from_json_marks_moves_on_the_dest_side_through_matches() {
        let before = Code::from_string("a\nb\nc\n", &Language::Java);
        let after = Code::from_string("c\nx\nb\n", &Language::Java);
        let json = serde_json::json!({
            "matches": [{"src": "Name: b [2,3]", "dest": "Name: b [4,5]"}],
            "actions": [
                {"action": "move-tree", "tree": "Name: b [2,3]"},
                {"action": "insert-node", "tree": "Name: x [2,3]"},
                {"action": "delete-node", "tree": "Name: a [0,1]"},
            ],
        });
        let (before_touched, after_touched) =
            gumtree_touched_from_json(&before, &after, &json).unwrap();
        assert_eq!(before_touched, vec![true, true, false, false]);
        assert_eq!(after_touched, vec![false, true, true, false]);
    }
}
