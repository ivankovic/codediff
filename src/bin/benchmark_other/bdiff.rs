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

// Split out of benchmark_other.rs (the `bdiff`-cluster functions) purely to shrink that
// file's visible size - no behavior change.

use anyhow::{Context, Result, bail};
use codediff::code::Code;
use codediff::diff::text_range::TextRange;
use std::collections::HashMap;
use std::io::Write;
use std::process::Command;

use super::git::git_env;
use super::{external_tool_bin, span_on_row_chars, whole_row_span, write_temp_pair};

/// The BDiff driver script (see its own doc comment), embedded rather than shipped as a loose
/// file so it cannot drift from the binary that runs it.
const BDIFF_DRIVER: &str = include_str!("../../../assets/bdiff_driver.py");

/// Python interpreter with BDiff importable, from `BDIFF_PYTHON` - by convention the `venv/bin/
/// python` of a virtualenv that has BDiff installed. Not auto-installed for the same reason
/// GumTree isn't: it is a separate project with its own dependencies.
///
/// Note BDiff's `pyproject.toml` under-declares: it lists numpy and scipy but its `bdiff.py`
/// also imports `rapidfuzz`, which must be installed separately or every invocation dies on
/// `ModuleNotFoundError`. See data/comparison/PROVENANCE.md.
pub(crate) fn bdiff_python() -> Result<std::path::PathBuf> {
    external_tool_bin(
        "BDIFF_PYTHON",
        "point it at a python interpreter with bdiff installed (see research/Makefile's \
         install-bdiff target)",
    )
}

/// `(before_touched, after_touched)` from BDiff's edit script.
///
/// BDiff reports eight edit modes. Every one carries `src_line` (before side) and `dest_line`
/// (after side), 1-indexed, and the block modes also carry `block_length`. Which side each mode
/// actually *touches* is the only judgement call here, and it is made to match what every other
/// tool in this comparison is scored on - `changed_spans` counts a line as changed when its
/// `TextOperation` is anything other than `Identical`, and `Move` is one of those - so a moved
/// line counts as touched for codediff and must count as touched here too:
///
/// * `insert` - after side only. Its `src_line` is the anchor the line was inserted at, not a
///   before-side line that changed.
/// * `delete` - before side only, for the mirror-image reason.
/// * `update`, `m_update`, `c_update` - both sides. The latter two are line-level updates inside
///   a move or copy block, and are ordinary updates for this metric.
/// * `move` - both sides, `block_length` lines from `src_line` and from `dest_line`.
/// * `split` - the one before-side line, and `block_length` after-side lines.
/// * `merge` - the mirror: `block_length` before-side lines, one after-side line.
/// * `copy` - **after side only.** A copy leaves its source block in place, unchanged, present
///   in both files; only the new duplicate at `dest_line` is a change. (Rare: 7 occurrences
///   across the first 60 fixtures.)
pub(crate) fn bdiff_line_labels(before: &Code, after: &Code) -> Result<(Vec<bool>, Vec<bool>)> {
    let script = bdiff_edit_script(before, after)?;
    bdiff_touched_from_script(before, after, &script)
}

/// Runs BDiff once and returns its raw edit script. Shared by [`bdiff_line_labels`] and
/// [`bdiff_node_spans`], which read different fields of the same entries - the same
/// one-invocation-per-metric shape `gumtree_line_labels`/`gumtree_node_spans` already have.
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

/// The pure half of [`bdiff_line_labels`], split out so the mode-to-side rules documented there
/// are unit-testable without an installed BDiff.
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

/// Per-fixture BDiff timings measured inside **one** Python interpreter, mirroring
/// `gumtree_warm_batch` and existing for exactly the same reason.
///
/// Importing BDiff pulls in numpy, scipy and rapidfuzz: ~394 ms, against a ~12 ms bare
/// interpreter (measured 2026-08-23). A per-invocation wall-clock number for BDiff is therefore
/// ~97% import overhead, which would put it last in any speed table while saying nothing at all
/// about its algorithm. `bdiff_ms` keeps that per-process number, because it is what a developer
/// running the tool once actually waits for; this function supplies `bdiff_warm_ms`, the cost
/// once startup is amortized. Reporting only one of the two would be misleading in one direction
/// or the other.
///
/// `Ok(None)` when `BDIFF_PYTHON` is unset - the same opt-in-per-run contract `gumtree_warm_batch`
/// has, not a per-fixture language scope.
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

    // Kept alive until the child has read every path off its stdin, exactly as in
    // `gumtree_warm_batch`.
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
        // A pair BDiff itself fails on is a per-fixture gap, not a run-ending failure - same
        // policy as the GumTree batch above, and for the same reason.
        if let Some(ms) = json.get("ms").and_then(|v| v.as_f64()) {
            results.insert(id, ms);
        }
    }
    Ok(Some(results))
}

/// BDiff's changed regions, from the same edit script `bdiff_line_labels` parses - but keeping
/// each `update` entry's real character range instead of collapsing it to the line it sits on.
///
/// This is what moves BDiff out of the `line_only` bucket. Its edit script carries a `str_diff`
/// field on every `update`-family entry: `[before_ranges, after_ranges]`, where each side is a
/// list of **inclusive** `[start, end]` character offsets into that line (an empty `[]` means the
/// side has nothing there, e.g. a pure insertion into a line). Verified live, 2026-08-24:
///
/// * `abcdefghij` -> `abcXYZfghij` gives `[[[3, 4]], [[3, 5]]]` - `de` on one side, `XYZ` on the
///   other, so both ends are inclusive and the two sides' lengths differ independently.
/// * `hello world` -> `hello there world` gives `[[[]], [[6, 11]]]` - the before side's empty
///   list is the insertion's zero-width position.
///
/// One limitation worth knowing before reading the resulting numbers: BDiff reports the **hull**
/// of a line's changes, not each one. `one two three four` -> `onX two threX four` gives a single
/// `[2, 12]`, spanning the untouched `e two thre` between the two edited characters, rather than
/// two ranges. So its sub-line output is finer than a line but coarser than the true edit, and it
/// will over-report on lines with several separated changes.
///
/// Modes with no `str_diff` (`insert`, `delete`, `move`, `split`, `merge`, `copy`) contribute
/// whole-line spans, following exactly the same mode-to-side rules `bdiff_line_labels` documents -
/// dropping them would leave BDiff scored only on the lines it happens to call updates.
pub(crate) fn bdiff_node_spans(
    before: &Code,
    after: &Code,
) -> Result<(Vec<TextRange>, Vec<TextRange>)> {
    let script = bdiff_edit_script(before, after)?;
    bdiff_spans_from_script(before, after, &script)
}

/// The pure half of [`bdiff_node_spans`], split out for the same reason
/// [`bdiff_touched_from_script`] is: the range conventions above are unit-testable without an
/// installed BDiff, and they are exactly the part that is easy to get wrong.
pub(crate) fn bdiff_spans_from_script(
    before: &Code,
    after: &Code,
    script: &[serde_json::Value],
) -> Result<(Vec<TextRange>, Vec<TextRange>)> {
    let before_lines: Vec<&str> = before.contents.split('\n').collect();
    let after_lines: Vec<&str> = after.contents.split('\n').collect();
    let mut before_spans = Vec::new();
    let mut after_spans = Vec::new();

    // BDiff line numbers are 1-based; `TextRange` rows are 0-based.
    let whole = |spans: &mut Vec<TextRange>, lines: &[&str], start: u64, count: u64| {
        for line_number in start..start + count.max(1) {
            if let Some(row) = (line_number as usize).checked_sub(1) {
                if row < lines.len() {
                    spans.push(whole_row_span(lines, row));
                }
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
                        // `[]` is the empty range BDiff emits for the side that has no text here.
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
                // No `str_diff` at all (or one that named nothing on either side) means BDiff
                // gave no sub-line detail for this update, so fall back to the whole lines rather
                // than silently reporting no change there.
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
                        // Inclusive end -> half-open, which is what every other span here is.
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
