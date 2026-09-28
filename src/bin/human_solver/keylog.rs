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

//! The keystroke log: one line per key the solver handles, so the next round of speed-ups is
//! measured rather than guessed. `--key-log-summary` reads it back.
//!
//! The file is `<session memory dir>/human_solver_keys.tsv`, outside the repository, appended
//! across sessions. A line is `<unix ms>\t<case>\t<mode>\t<key>\t<edited>`: the mode is which
//! view took the key (`tree`, `text`, `picker`, ...), `edited` is `1` when the key changed the
//! mapping. In a mode that takes typed text (a name filter, a search, a line number) a character
//! is logged as `typed`, not as itself: the log is about where the keys go, not what was written.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{Modal, session_memory_path};

/// `<session memory dir>/human_solver_keys.tsv`, or `None` with no home to put it under.
pub(crate) fn key_log_path() -> Option<PathBuf> {
    Some(
        session_memory_path()?
            .parent()?
            .join("human_solver_keys.tsv"),
    )
}

/// The open log file. Every `record` is one line and one write: a crash loses nothing.
pub(crate) struct KeyLog {
    file: fs::File,
}

impl KeyLog {
    pub(crate) fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("opening {}", path.display()))?;
        Ok(KeyLog { file })
    }

    /// Appends one key. A write failure is ignored: the log must never cost a keystroke.
    pub(crate) fn record(&mut self, case: &str, mode: &str, key: &str, edited: bool) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let _ = writeln!(
            self.file,
            "{now}\t{case}\t{mode}\t{key}\t{}",
            u8::from(edited)
        );
    }
}

/// Which view a key went to, as the log names it. A mode ending in `-typing` takes text, and
/// `key_name` hides the characters there.
pub(crate) fn mode_name(modal: Option<&Modal>) -> &'static str {
    match modal {
        None => "tree",
        Some(Modal::TextView { state }) => {
            if state.line_prompt.is_some() {
                "text-typing"
            } else {
                "text"
            }
        }
        Some(Modal::OpenDiffPicker { name_input, .. }) => {
            if name_input.is_some() {
                "picker-typing"
            } else {
                "picker"
            }
        }
        Some(Modal::OpenSamplePicker { name_input, .. }) => {
            if name_input.is_some() {
                "sample-picker-typing"
            } else {
                "sample-picker"
            }
        }
        Some(Modal::SolutionPicker { new_name, .. }) => {
            if new_name.is_some() {
                "painting-picker-typing"
            } else {
                "painting-picker"
            }
        }
        Some(Modal::PromptPromoteName { .. })
        | Some(Modal::PromptRejectReason { .. })
        | Some(Modal::PromptComment { .. })
        | Some(Modal::PromptSearch { .. }) => "prompt-typing",
        Some(Modal::ConfirmKindMismatch { .. }) => "kind-mismatch",
        Some(Modal::ConfirmMultiMapGroup { .. }) => "group-confirm",
        Some(Modal::ConfirmResetCase { .. }) => "reset-confirm",
        Some(Modal::ConfirmDiscardUnsaved { .. }) => "discard-confirm",
        Some(Modal::InvariantList { .. }) => "invariants",
        Some(Modal::UnixDiffView { .. }) => "unix-diff",
        Some(Modal::Help { .. }) => "help",
        Some(Modal::OpenCommitPicker { .. }) | Some(Modal::OpenCommitFilePicker { .. }) => {
            "commit-picker"
        }
    }
}

/// The key as the log writes it: the character itself, `Ctrl-r`, `Enter`, `Up`...; in a typing
/// mode a plain character is `typed`.
pub(crate) fn key_name(key: KeyEvent, mode: &str) -> String {
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char(c) if control => format!("Ctrl-{c}"),
        KeyCode::Char(_) if mode.ends_with("-typing") => "typed".to_string(),
        KeyCode::Char(' ') => "Space".to_string(),
        KeyCode::Char('\t') => "Tab".to_string(),
        KeyCode::Char(c) => c.to_string(),
        other => format!("{other:?}"),
    }
}

/// One parsed log line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KeyRecord {
    pub(crate) at_ms: u64,
    pub(crate) case: String,
    pub(crate) mode: String,
    pub(crate) key: String,
    pub(crate) edited: bool,
}

/// Parses the log, skipping lines that do not parse (a partial last line after a crash).
pub(crate) fn parse_key_log(contents: &str) -> Vec<KeyRecord> {
    contents
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let at_ms = fields.next()?.parse().ok()?;
            let case = fields.next()?.to_string();
            let mode = fields.next()?.to_string();
            let key = fields.next()?.to_string();
            let edited = fields.next()? == "1";
            Some(KeyRecord {
                at_ms,
                case,
                mode,
                key,
                edited,
            })
        })
        .collect()
}

/// A pause longer than this between two keys is a break, not thinking time, and is not counted.
pub(crate) const BREAK_MS: u64 = 60_000;

/// The summary `--key-log-summary` prints: where the keys and the time between them went. The
/// time before a key is charged to that key (and its mode and case): it is the thinking that
/// led to it. Gaps of `BREAK_MS` or more are dropped.
pub(crate) fn summarize_key_log(records: &[KeyRecord]) -> String {
    if records.is_empty() {
        return "No keys logged yet.\n".to_string();
    }
    let mut by_mode: HashMap<&str, (usize, u64)> = HashMap::new();
    let mut by_key: HashMap<(&str, &str), (usize, u64)> = HashMap::new();
    let mut by_case: HashMap<&str, (usize, u64, usize)> = HashMap::new();
    let mut active_ms = 0u64;
    let mut edits = 0usize;
    let mut previous: Option<u64> = None;
    for record in records {
        let gap = previous
            .map(|at| record.at_ms.saturating_sub(at))
            .filter(|gap| *gap < BREAK_MS)
            .unwrap_or(0);
        previous = Some(record.at_ms);
        active_ms += gap;
        edits += usize::from(record.edited);
        let mode = by_mode.entry(record.mode.as_str()).or_default();
        mode.0 += 1;
        mode.1 += gap;
        let key = by_key
            .entry((record.mode.as_str(), record.key.as_str()))
            .or_default();
        key.0 += 1;
        key.1 += gap;
        let case = by_case.entry(record.case.as_str()).or_default();
        case.0 += 1;
        case.1 += gap;
        case.2 += usize::from(record.edited);
    }

    let mut out = String::new();
    out.push_str(&format!(
        "{} keys, {} of them edits, {} active (pauses under {}s between keys)\n\n",
        records.len(),
        edits,
        duration(active_ms),
        BREAK_MS / 1000
    ));

    out.push_str("By mode:\n");
    let mut modes: Vec<_> = by_mode.into_iter().collect();
    modes.sort_by(|a, b| b.1.1.cmp(&a.1.1).then(a.0.cmp(b.0)));
    for (mode, (count, ms)) in modes {
        out.push_str(&format!(
            "  {mode:<24} {count:>7} keys  {:>9}  {:>6.1}s/key\n",
            duration(ms),
            seconds_per_key(ms, count)
        ));
    }

    out.push_str("\nTop keys by time (the pause before each):\n");
    let mut keys: Vec<_> = by_key.into_iter().collect();
    keys.sort_by(|a, b| {
        b.1.1
            .cmp(&a.1.1)
            .then(b.1.0.cmp(&a.1.0))
            .then(a.0.cmp(&b.0))
    });
    for ((mode, key), (count, ms)) in keys.iter().take(20) {
        out.push_str(&format!(
            "  {:<24} {count:>7} keys  {:>9}  {:>6.1}s/key\n",
            format!("{mode} {key}"),
            duration(*ms),
            seconds_per_key(*ms, *count)
        ));
    }

    out.push_str("\nTop keys by count:\n");
    keys.sort_by(|a, b| b.1.0.cmp(&a.1.0).then(a.0.cmp(&b.0)));
    for ((mode, key), (count, ms)) in keys.iter().take(20) {
        out.push_str(&format!(
            "  {:<24} {count:>7} keys  {:>9}\n",
            format!("{mode} {key}"),
            duration(*ms)
        ));
    }

    out.push_str("\nBy case (time, keys, edits):\n");
    let mut cases: Vec<_> = by_case.into_iter().collect();
    cases.sort_by(|a, b| b.1.1.cmp(&a.1.1).then(a.0.cmp(b.0)));
    for (case, (count, ms, edits)) in cases.iter().take(30) {
        out.push_str(&format!(
            "  {:>9}  {count:>6} keys  {edits:>6} edits  {case}\n",
            duration(*ms)
        ));
    }
    out
}

fn seconds_per_key(ms: u64, count: usize) -> f64 {
    if count == 0 {
        0.0
    } else {
        ms as f64 / 1000.0 / count as f64
    }
}

/// `1h02m`, `4m07s` or `12.3s`.
fn duration(ms: u64) -> String {
    let seconds = ms / 1000;
    if seconds >= 3600 {
        format!("{}h{:02}m", seconds / 3600, (seconds % 3600) / 60)
    } else if seconds >= 60 {
        format!("{}m{:02}s", seconds / 60, seconds % 60)
    } else {
        format!("{:.1}s", ms as f64 / 1000.0)
    }
}
