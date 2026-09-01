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

// Split out of benchmark_other.rs (the `nvim`-cluster functions) purely to shrink that
// file's visible size - no behavior change.

use anyhow::{Context, Result, bail};
use codediff::code::Code;
use codediff::diff::text_range::TextRange;
use std::collections::HashMap;
use std::process::Command;

use super::{external_tool_bin, span_on_row, whole_row_span, write_temp_pair};

/// The Neovim diff driver (see its own header), embedded so it cannot drift from this binary.
const NVIM_DRIVER: &str = include_str!("../../../assets/nvim_diff_driver.lua");

/// Neovim binary, from `NVIM_BIN`. Not auto-installed, same policy as every other external tool.
pub(crate) fn nvim_bin() -> Result<std::path::PathBuf> {
    external_tool_bin(
        "NVIM_BIN",
        "point it at a neovim binary (nvim-linux64/bin/nvim)",
    )
}

/// Runs `nvim -d` once and returns the driver's two side objects (before, after), each carrying
/// `lines` and `subline`. Shared by [`nvim_line_labels`] and [`nvim_node_spans`], which read
/// different fields of the same output.
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

    // The driver writes exactly one JSON line; take the last non-empty one so any startup chatter
    // Neovim emits on stdout ahead of it is ignored rather than breaking the parse.
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

/// `(before_touched, after_touched)` from `nvim -d`.
///
/// Included even though Neovim's *line* pass is libxdiff - the same engine as the four `git`
/// rows - because excluding it on that basis would be letting this metric's granularity decide
/// what gets measured. Neovim is the tool a large number of developers actually read diffs in,
/// and it is the only entry here that pairs a line-level match with character-level display, so
/// what it costs is one row and what it buys is a data point rather than an assumption. On a
/// 40-fixture sample its line set matched `git_myers` on 38; the corpus decides the rest.
///
/// Run with `-u NONE`: `diffopt` is user-configurable and can change both the algorithm
/// (`algorithm:histogram`) and the within-line alignment (`linematch:N`), so loading a user
/// config would silently turn this into a measurement of that config. What is scored is Neovim's
/// shipped default behaviour.
///
/// `-n` disables swap files - without it, concurrent or repeated runs over the same fixture path
/// prompt for swap recovery and hang a headless process forever.
pub(crate) fn nvim_line_labels(before: &Code, after: &Code) -> Result<(Vec<bool>, Vec<bool>)> {
    let sides = nvim_diff_sides(before, after)?;
    let touched = |side: &serde_json::Value, line_count: usize| -> Vec<bool> {
        let mut flags = vec![false; line_count];
        if let Some(lines) = side.get("lines").and_then(|v| v.as_array()) {
            for value in lines {
                if let Some(index) = value.as_u64().and_then(|n| (n as usize).checked_sub(1)) {
                    if let Some(slot) = flags.get_mut(index) {
                        *slot = true;
                    }
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

/// Neovim's changed regions, from the same driver output `nvim_line_labels` parses - but keeping
/// the `DiffText` column runs instead of collapsing each changed line to a boolean.
///
/// This is what moves `nvim -d` out of the `line_only` bucket, and it is the only thing in this
/// comparison that measures what Neovim actually adds over the four `git` rows: its *line* pass
/// is libxdiff, the same engine, so on a line-level metric it can only ever tie them. The
/// within-line highlight is computed by Neovim itself and is the whole difference.
///
/// A changed line with no `DiffText` run on it is a wholly added or removed line (`DiffAdd` /
/// `DiffDelete`), not a rewritten one, so it contributes its whole row - the same treatment
/// [`bdiff_spans_from_script`] gives an `insert` or a `delete`.
pub(crate) fn nvim_node_spans(
    before: &Code,
    after: &Code,
) -> Result<(Vec<TextRange>, Vec<TextRange>)> {
    let sides = nvim_diff_sides(before, after)?;
    let before_lines: Vec<&str> = before.contents.split('\n').collect();
    let after_lines: Vec<&str> = after.contents.split('\n').collect();

    let spans_for = |side: &serde_json::Value, lines: &[&str]| -> Vec<TextRange> {
        // `{lnum, start_col, end_col}`, 1-based byte columns with an exclusive end - see the
        // driver's own comment on why these are runs rather than a per-line flag.
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
