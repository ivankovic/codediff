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
use codediff::code::Code;
use codediff::diff::text_range::TextRange;
use std::collections::HashMap;
use std::io::Write;
use std::process::Command;

use super::git::git_env;
use super::{external_tool_bin, span_on_row_chars, whole_row_span, write_temp_pair};

/// Embedded rather than shipped as a loose file so it cannot drift from the binary that runs it.
const BDIFF_DRIVER: &str = include_str!("../../../assets/bdiff_driver.py");

/// Python interpreter with BDiff importable, from `BDIFF_PYTHON`. BDiff's `pyproject.toml` omits
/// `rapidfuzz`, which must be installed separately (see data/comparison/PROVENANCE.md).
pub(crate) fn bdiff_python() -> Result<std::path::PathBuf> {
    external_tool_bin(
        "BDIFF_PYTHON",
        "point it at a python interpreter with bdiff installed (see research/Makefile's \
         install-bdiff target)",
    )
}

/// `(before_touched, after_touched)` from BDiff's edit script.
///
/// Every mode carries 1-indexed `src_line`/`dest_line`; block modes add `block_length`. Which side
/// a mode touches matches how codediff is scored (a moved line counts as changed):
///
/// * `insert` - after side only; its `src_line` is an anchor. `delete` is the mirror.
/// * `update`, `m_update`, `c_update` - both sides.
/// * `move` - both sides, `block_length` lines each.
/// * `split` - one before-side line, `block_length` after-side lines. `merge` is the mirror.
/// * `copy` - after side only: the source block is unchanged and present in both files.
pub(crate) fn bdiff_line_labels(before: &Code, after: &Code) -> Result<(Vec<bool>, Vec<bool>)> {
    let script = bdiff_edit_script(before, after)?;
    bdiff_touched_from_script(before, after, &script)
}

/// Runs BDiff once and returns its raw edit script.
pub(crate) fn bdiff_edit_script(before: &Code, after: &Code) -> Result<Vec<serde_json::Value>> {
    let python = bdiff_python()?;
    let (before_file, after_file) = write_temp_pair(before, after, None)?;

    let mut driver = tempfile::Builder::new()
        .suffix(".py")
        .tempfile()
        .context("creating bdiff driver temp file")?;
    std::io::Write::write_all(&mut driver, BDIFF_DRIVER.as_bytes())
        .context("writing bdiff driver temp file")?;

    let mut command = Command::new(&python);
    git_env(&mut command);
    let output = command
        .arg(driver.path())
        .arg(before_file.path())
        .arg(after_file.path())
        .output()
        .with_context(|| format!("running {python:?} bdiff_driver.py"))?;
    if !output.status.success() {
        bail!(
            "bdiff driver exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    serde_json::from_slice(&output.stdout).context("parsing bdiff driver JSON output")
}

/// The pure half of [`bdiff_line_labels`], testable without an installed BDiff.
pub(crate) fn bdiff_touched_from_script(
    before: &Code,
    after: &Code,
    script: &[serde_json::Value],
) -> Result<(Vec<bool>, Vec<bool>)> {
    let mut before_touched = vec![false; before.contents.split('\n').count()];
    let mut after_touched = vec![false; after.contents.split('\n').count()];

    let mark = |touched: &mut Vec<bool>, start: u64, count: u64| {
        for line_number in start..start + count.max(1) {
            if let Some(slot) = (line_number as usize)
                .checked_sub(1)
                .and_then(|i| touched.get_mut(i))
            {
                *slot = true;
            }
        }
    };

    for entry in script {
        let mode = entry
            .get("mode")
            .and_then(|m| m.as_str())
            .unwrap_or_default();
        let src = entry.get("src_line").and_then(|v| v.as_u64()).unwrap_or(0);
        let dest = entry.get("dest_line").and_then(|v| v.as_u64()).unwrap_or(0);
        let block = entry
            .get("block_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(1);
        match mode {
            "insert" => mark(&mut after_touched, dest, 1),
            "delete" => mark(&mut before_touched, src, 1),
            "update" | "m_update" | "c_update" => {
                mark(&mut before_touched, src, 1);
                mark(&mut after_touched, dest, 1);
            }
            "move" => {
                mark(&mut before_touched, src, block);
                mark(&mut after_touched, dest, block);
            }
            "split" => {
                mark(&mut before_touched, src, 1);
                mark(&mut after_touched, dest, block);
            }
            "merge" => {
                mark(&mut before_touched, src, block);
                mark(&mut after_touched, dest, 1);
            }
            "copy" => mark(&mut after_touched, dest, block),
            other => bail!("unknown BDiff edit mode {other:?} - see bdiff_line_labels"),
        }
    }
    Ok((before_touched, after_touched))
}

/// Per-fixture BDiff timings measured inside one Python interpreter (`bdiff_warm_ms`).
///
/// Importing numpy, scipy and rapidfuzz dominates a per-process run, so the per-process number
/// (`bdiff_ms`) says little about the algorithm; both are reported. `Ok(None)` when
/// `BDIFF_PYTHON` is unset.
pub(crate) fn bdiff_warm_batch(
    fixtures: &[(&str, &Code, &Code)],
) -> Result<Option<HashMap<String, f64>>> {
    let Ok(python) = bdiff_python() else {
        return Ok(None);
    };

    let mut driver = tempfile::Builder::new()
        .suffix(".py")
        .tempfile()
        .context("creating bdiff batch driver temp file")?;
    std::io::Write::write_all(&mut driver, BDIFF_DRIVER.as_bytes())
        .context("writing bdiff batch driver temp file")?;

    // Kept alive until the child has read every path off its stdin.
    let mut before_files = Vec::with_capacity(fixtures.len());
    let mut after_files = Vec::with_capacity(fixtures.len());
    let mut requests = String::new();
    for (name, before, after) in fixtures {
        let (before_file, after_file) = write_temp_pair(before, after, None)?;
        requests.push_str(
            &serde_json::json!({
                "id": name,
                "before": before_file.path().display().to_string(),
                "after": after_file.path().display().to_string(),
            })
            .to_string(),
        );
        requests.push('\n');
        before_files.push(before_file);
        after_files.push(after_file);
    }

    let mut command = Command::new(&python);
    git_env(&mut command);
    let mut child = command
        .arg(driver.path())
        .arg("--batch")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("spawning the BDiff batch driver")?;
    let mut stdin = child
        .stdin
        .take()
        .context("bdiff batch driver child has no stdin")?;
    let writer = std::thread::spawn(move || stdin.write_all(requests.as_bytes()));
    let output = child
        .wait_with_output()
        .context("waiting for the BDiff batch driver")?;
    writer
        .join()
        .expect("bdiff batch driver stdin-writer thread panicked")
        .context("writing bdiff batch driver stdin")?;
    if !output.status.success() {
        bail!(
            "BDiff batch driver exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let mut results = HashMap::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let json: serde_json::Value = serde_json::from_str(line)
            .with_context(|| format!("parsing bdiff batch response line {line:?}"))?;
        let id = json["id"]
            .as_str()
            .context("bdiff batch response missing `id`")?
            .to_string();
        // A pair BDiff fails on is a per-fixture gap, not a run-ending failure.
        if let Some(ms) = json.get("ms").and_then(|v| v.as_f64()) {
            results.insert(id, ms);
        }
    }
    Ok(Some(results))
}

/// BDiff's changed regions, keeping each `update` entry's character range from its `str_diff`
/// field: `[before_ranges, after_ranges]`, each a list of **inclusive** `[start, end]` character
/// offsets into the line (`[]` means nothing on that side, e.g. a pure insertion).
///
/// BDiff reports the hull of a line's changes, so lines with several separated edits over-report.
/// Other modes contribute whole lines on the sides [`bdiff_line_labels`] documents.
pub(crate) fn bdiff_node_spans(
    before: &Code,
    after: &Code,
) -> Result<(Vec<TextRange>, Vec<TextRange>)> {
    let script = bdiff_edit_script(before, after)?;
    bdiff_spans_from_script(before, after, &script)
}

/// The pure half of [`bdiff_node_spans`], testable without an installed BDiff.
pub(crate) fn bdiff_spans_from_script(
    before: &Code,
    after: &Code,
    script: &[serde_json::Value],
) -> Result<(Vec<TextRange>, Vec<TextRange>)> {
    let before_lines: Vec<&str> = before.contents.split('\n').collect();
    let after_lines: Vec<&str> = after.contents.split('\n').collect();
    let mut before_spans = Vec::new();
    let mut after_spans = Vec::new();

    let whole = |spans: &mut Vec<TextRange>, lines: &[&str], start: u64, count: u64| {
        for line_number in start..start + count.max(1) {
            if let Some(row) = (line_number as usize).checked_sub(1)
                && row < lines.len()
            {
                spans.push(whole_row_span(lines, row));
            }
        }
    };

    /// One side of a `str_diff` field, as `(start, end)` **inclusive** character offsets.
    fn side_ranges(str_diff: &serde_json::Value, side: usize) -> Vec<(usize, usize)> {
        str_diff
            .get(side)
            .and_then(|v| v.as_array())
            .map(|ranges| {
                ranges
                    .iter()
                    .filter_map(|range| {
                        let pair = range.as_array()?;
                        Some((
                            pair.first()?.as_u64()? as usize,
                            pair.get(1)?.as_u64()? as usize,
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    for entry in script {
        let mode = entry
            .get("mode")
            .and_then(|m| m.as_str())
            .unwrap_or_default();
        let src = entry.get("src_line").and_then(|v| v.as_u64()).unwrap_or(0);
        let dest = entry.get("dest_line").and_then(|v| v.as_u64()).unwrap_or(0);
        let block = entry
            .get("block_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(1);

        match mode {
            "insert" => whole(&mut after_spans, &after_lines, dest, 1),
            "delete" => whole(&mut before_spans, &before_lines, src, 1),
            "update" | "m_update" | "c_update" => {
                let str_diff = entry.get("str_diff");
                let sub = str_diff
                    .map(|d| (side_ranges(d, 0), side_ranges(d, 1)))
                    .unwrap_or_default();
                let (before_sub, after_sub) = sub;
                // No sub-line detail: report whole lines rather than no change.
                if before_sub.is_empty() && after_sub.is_empty() {
                    whole(&mut before_spans, &before_lines, src, 1);
                    whole(&mut after_spans, &after_lines, dest, 1);
                    continue;
                }
                for (row_1based, ranges, lines, spans) in [
                    (src, before_sub, &before_lines, &mut before_spans),
                    (dest, after_sub, &after_lines, &mut after_spans),
                ] {
                    let Some(row) = (row_1based as usize).checked_sub(1) else {
                        continue;
                    };
                    for (start, end) in ranges {
                        // Inclusive end -> half-open.
                        spans.push(span_on_row_chars(lines, row, start, end + 1));
                    }
                }
            }
            "move" => {
                whole(&mut before_spans, &before_lines, src, block);
                whole(&mut after_spans, &after_lines, dest, block);
            }
            "split" => {
                whole(&mut before_spans, &before_lines, src, 1);
                whole(&mut after_spans, &after_lines, dest, block);
            }
            "merge" => {
                whole(&mut before_spans, &before_lines, src, block);
                whole(&mut after_spans, &after_lines, dest, 1);
            }
            "copy" => whole(&mut after_spans, &after_lines, dest, block),
            other => bail!("unknown BDiff edit mode {other:?} - see bdiff_line_labels"),
        }
    }

    Ok((before_spans, after_spans))
}
