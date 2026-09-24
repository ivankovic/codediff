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
use std::process::Command;

use super::{external_tool_bin, span_on_row, whole_row_span, write_temp_pair};

/// Embedded so it cannot drift from this binary.
const NVIM_DRIVER: &str = include_str!("../../../assets/nvim_diff_driver.lua");

/// Neovim binary, from `NVIM_BIN`.
pub(crate) fn nvim_bin() -> Result<std::path::PathBuf> {
    external_tool_bin(
        "NVIM_BIN",
        "point it at a neovim binary (nvim-linux64/bin/nvim)",
    )
}

/// Runs `nvim -d` once and returns the driver's two side objects (before, after), each carrying
/// `lines` and `subline`.
///
/// `-u NONE` scores Neovim's shipped defaults rather than a user `diffopt`; `-n` disables swap
/// files, whose recovery prompt hangs a headless run.
pub(crate) fn nvim_diff_sides(before: &Code, after: &Code) -> Result<Vec<serde_json::Value>> {
    let nvim = nvim_bin()?;
    let (before_file, after_file) = write_temp_pair(before, after, None)?;

    let mut driver = tempfile::Builder::new()
        .suffix(".lua")
        .tempfile()
        .context("creating nvim driver temp file")?;
    std::io::Write::write_all(&mut driver, NVIM_DRIVER.as_bytes())
        .context("writing nvim driver temp file")?;

    let output = Command::new(&nvim)
        .args(["--headless", "-n", "-u", "NONE", "-d", "-S"])
        .arg(driver.path())
        .arg(before_file.path())
        .arg(after_file.path())
        .output()
        .with_context(|| format!("running {nvim:?} -d"))?;
    if !output.status.success() {
        bail!(
            "nvim -d exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // The last non-empty line, so startup chatter on stdout does not break the parse.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .rfind(|line| !line.trim().is_empty())
        .context("nvim driver produced no output")?;
    let sides: Vec<serde_json::Value> =
        serde_json::from_str(line).context("parsing nvim driver JSON output")?;
    if sides.len() != 2 {
        bail!("nvim driver returned {} sides, expected 2", sides.len());
    }

    Ok(sides)
}

/// `(before_touched, after_touched)` from `nvim -d`. Its line pass is libxdiff like the `git`
/// rows; it is kept because it is where many developers read diffs.
pub(crate) fn nvim_line_labels(before: &Code, after: &Code) -> Result<(Vec<bool>, Vec<bool>)> {
    let sides = nvim_diff_sides(before, after)?;
    let touched = |side: &serde_json::Value, line_count: usize| -> Vec<bool> {
        let mut flags = vec![false; line_count];
        if let Some(lines) = side.get("lines").and_then(|v| v.as_array()) {
            for value in lines {
                if let Some(index) = value.as_u64().and_then(|n| (n as usize).checked_sub(1))
                    && let Some(slot) = flags.get_mut(index)
                {
                    *slot = true;
                }
            }
        }
        flags
    };
    Ok((
        touched(&sides[0], before.contents.split('\n').count()),
        touched(&sides[1], after.contents.split('\n').count()),
    ))
}

/// Neovim's changed regions: the `DiffText` column runs, which are all it adds over the `git`
/// rows. A changed line with no run is wholly added or removed and contributes its whole row.
pub(crate) fn nvim_node_spans(
    before: &Code,
    after: &Code,
) -> Result<(Vec<TextRange>, Vec<TextRange>)> {
    let sides = nvim_diff_sides(before, after)?;
    let before_lines: Vec<&str> = before.contents.split('\n').collect();
    let after_lines: Vec<&str> = after.contents.split('\n').collect();

    let spans_for = |side: &serde_json::Value, lines: &[&str]| -> Vec<TextRange> {
        // `[lnum, start_col, end_col]`, 1-based byte columns with an exclusive end.
        let mut runs: HashMap<usize, Vec<(usize, usize)>> = HashMap::new();
        if let Some(entries) = side.get("subline").and_then(|v| v.as_array()) {
            for entry in entries {
                let Some(triple) = entry.as_array() else {
                    continue;
                };
                let (Some(lnum), Some(start), Some(end)) = (
                    triple.first().and_then(|v| v.as_u64()),
                    triple.get(1).and_then(|v| v.as_u64()),
                    triple.get(2).and_then(|v| v.as_u64()),
                ) else {
                    continue;
                };
                let Some(row) = (lnum as usize).checked_sub(1) else {
                    continue;
                };
                runs.entry(row)
                    .or_default()
                    .push((start as usize - 1, end as usize - 1));
            }
        }

        let mut spans = Vec::new();
        if let Some(entries) = side.get("lines").and_then(|v| v.as_array()) {
            for entry in entries {
                let Some(row) = entry
                    .as_u64()
                    .and_then(|lnum| (lnum as usize).checked_sub(1))
                else {
                    continue;
                };
                if row >= lines.len() {
                    continue;
                }
                match runs.get(&row) {
                    Some(line_runs) if !line_runs.is_empty() => spans.extend(
                        line_runs
                            .iter()
                            .map(|&(start, end)| span_on_row(row, start, end)),
                    ),
                    _ => spans.push(whole_row_span(lines, row)),
                }
            }
        }
        spans
    };

    Ok((
        spans_for(&sides[0], &before_lines),
        spans_for(&sides[1], &after_lines),
    ))
}
