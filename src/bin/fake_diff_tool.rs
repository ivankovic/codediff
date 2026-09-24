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

//! A diff tool with no diff algorithm in it, for testing `benchmark_other`'s harness.
//!
//! A real tool's correct answer is unknown, so the harness's parsing and scoring cannot be checked
//! against one. This binary answers predictably instead. It speaks difftastic's `--display json`
//! and is pointed at by `DIFFT_BIN`, so it exercises the real difftastic adapter.
//!
//! `FAKE_DIFF_MODE` picks the answer:
//!
//! * `empty` - nothing changed. Mismatches against the human mapping then count exactly the lines
//!   the human says *did* change.
//! * `all` - every line on both sides is part of the edit. Mismatches then count exactly the lines
//!   the human says did *not* change.
//! * `random` - a per-line verdict from a hash of the line, identical in every process.
//! * `crash` - exit non-zero with a message on stderr, the way a real tool fails.
//!
//! `empty` and `all` are complements: their mismatch counts sum to the line count whatever the
//! human mapping says, so the end-to-end test needs no hardcoded counts.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde_json::json;

/// Parsed from `FAKE_DIFF_MODE`. No default: a silently picked mode gives plausible wrong numbers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Empty,
    All,
    Random,
    Crash,
}

impl Mode {
    fn parse(value: &str) -> Result<Mode> {
        Ok(match value {
            "empty" => Mode::Empty,
            "all" => Mode::All,
            "random" => Mode::Random,
            "crash" => Mode::Crash,
            other => bail!("unknown FAKE_DIFF_MODE '{other}' (empty|all|random|crash)"),
        })
    }
}

fn main() -> Result<()> {
    let mode = Mode::parse(&std::env::var("FAKE_DIFF_MODE").context("FAKE_DIFF_MODE is not set")?)?;

    if mode == Mode::Crash {
        eprintln!("fake_diff_tool: deliberate failure (FAKE_DIFF_MODE=crash)");
        std::process::exit(2);
    }

    let (before_path, after_path) = input_paths()?;
    let before = std::fs::read_to_string(&before_path)
        .with_context(|| format!("reading {before_path:?}"))?;
    let after =
        std::fs::read_to_string(&after_path).with_context(|| format!("reading {after_path:?}"))?;

    println!("{}", render(mode, &before, &after));
    Ok(())
}

/// The two arguments that are existing files, so difftastic's flags need no parsing. Any other
/// count is an error rather than a guess.
fn input_paths() -> Result<(PathBuf, PathBuf)> {
    let files: Vec<PathBuf> = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .filter(|arg| arg.is_file())
        .collect();
    match files.as_slice() {
        [before, after] => Ok((before.clone(), after.clone())),
        other => bail!(
            "expected exactly 2 existing file arguments, got {}: {:?}",
            other.len(),
            std::env::args_os().skip(1).collect::<Vec<_>>()
        ),
    }
}

/// The whole output as difftastic's `--display json` would carry it.
fn render(mode: Mode, before: &str, after: &str) -> String {
    if mode == Mode::Empty {
        // Difftastic omits `chunks` for an unchanged file rather than emitting an empty array.
        return json!({"status": "unchanged"}).to_string();
    }

    let mut entries = Vec::new();
    for (key, contents) in [("lhs", before), ("rhs", after)] {
        for (index, line) in contents.split('\n').enumerate() {
            let Some((start, end)) = touched_span(mode, key, index, line) else {
                continue;
            };
            entries.push(json!({
                key: {"line_number": index, "changes": [{"start": start, "end": end}]}
            }));
        }
    }
    json!({"chunks": [entries]}).to_string()
}

/// The byte range this mode reports as changed on one line, or `None` for an untouched line.
///
/// Byte columns, as difftastic reports and the harness reads unconverted.
fn touched_span(mode: Mode, side: &str, index: usize, line: &str) -> Option<(usize, usize)> {
    match mode {
        Mode::Empty | Mode::Crash => None,
        // Including the empty tail `split('\n')` yields after a final newline: the harness's label
        // vector has a slot for it.
        Mode::All => Some((0, line.len())),
        Mode::Random => {
            let hash = line_hash(side, index, line);
            if hash.is_multiple_of(2) {
                return None;
            }
            // Sub-line, like a real AST-aware tool.
            let start = snap_down(line, (hash >> 8) as usize % (line.len() + 1));
            let end = snap_up(
                line,
                start + (hash >> 24) as usize % (line.len() - start + 1),
            );
            Some((start, end))
        }
    }
}

/// A stable verdict for one line. A hash rather than a seeded RNG, whose answer would depend on
/// visiting order. The side is folded in so both sides of an unchanged line differ.
fn line_hash(side: &str, index: usize, line: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for &byte in bytes {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x1000_0000_01b3);
        }
    };
    eat(side.as_bytes());
    eat(&index.to_le_bytes());
    eat(line.as_bytes());
    mix(hash)
}

/// SplitMix64's finalizer, over the raw FNV-1a value. FNV-1a's multiplier is odd, so its bit 0 is
/// only the parity of the odd bytes eaten; the avalanche step makes every bit usable.
fn mix(mut hash: u64) -> u64 {
    hash ^= hash >> 30;
    hash = hash.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    hash ^= hash >> 27;
    hash = hash.wrapping_mul(0x94d0_49bb_1331_11eb);
    hash ^ (hash >> 31)
}

/// `index` moved back to the nearest character boundary.
fn snap_down(line: &str, index: usize) -> usize {
    codediff::diff::text_range::floor_char_boundary(line, index)
}

fn snap_up(line: &str, mut index: usize) -> usize {
    while index < line.len() && !line.is_char_boundary(index) {
        index += 1;
    }
    index.min(line.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(mode: Mode, before: &str, after: &str) -> serde_json::Value {
        serde_json::from_str(&render(mode, before, after)).expect("valid JSON")
    }

    /// The adapter branches on the key's absence, so an empty array would take a different path.
    #[test]
    fn empty_mode_omits_the_chunks_key_entirely() {
        let json = parse(Mode::Empty, "a\n", "b\n");
        assert!(json.get("chunks").is_none(), "{json}");
    }

    /// Missing the empty tail shows up as a complementarity failure of exactly 2.
    #[test]
    fn all_mode_covers_every_line_including_the_one_after_the_final_newline() {
        let json = parse(Mode::All, "a\nbb\n", "c\n");
        let entries = json["chunks"][0].as_array().expect("one chunk");
        let lines = |key: &str| -> Vec<u64> {
            entries
                .iter()
                .filter_map(|entry| entry.get(key))
                .map(|side| side["line_number"].as_u64().unwrap())
                .collect()
        };
        assert_eq!(
            lines("lhs"),
            vec![0, 1, 2],
            "\"a\", \"bb\", and the empty tail"
        );
        assert_eq!(lines("rhs"), vec![0, 1]);
    }

    /// `é` is two bytes, so a two-character line is three columns wide.
    #[test]
    fn all_mode_reports_byte_columns_not_character_offsets() {
        let json = parse(Mode::All, "bé\n", "x\n");
        assert_eq!(json["chunks"][0][0]["lhs"]["changes"][0]["end"], 3);
    }

    /// Lines differing only in an even-valued byte: a parity bit gives all sixteen the same
    /// verdict, a hash splits them. Varying odd bytes would pass even without `mix`.
    #[test]
    fn random_mode_does_not_collapse_onto_the_parity_of_its_input() {
        let touched = (0..16u8)
            .filter(|step| {
                let line = format!("x{}", char::from(b'@' + step * 2));
                touched_span(Mode::Random, "lhs", 0, &line).is_some()
            })
            .count();
        assert!(
            (1..16).contains(&touched),
            "{touched}/16 lines differing only in an even byte got the same verdict - the answer \
             has collapsed onto the parity of the input instead of a hash of it"
        );
    }

    /// What makes the end-to-end test's cross-process determinism hold.
    #[test]
    fn random_mode_is_a_pure_function_of_side_index_and_line() {
        let once = touched_span(Mode::Random, "lhs", 3, "some line");
        for _ in 0..5 {
            let _ = touched_span(Mode::Random, "rhs", 9, "other line");
        }
        assert_eq!(once, touched_span(Mode::Random, "lhs", 3, "some line"));
        assert_ne!(
            once,
            touched_span(Mode::Random, "rhs", 3, "some line"),
            "the side has to matter, or an unchanged line gets the same verdict on both sides"
        );
    }

    #[test]
    fn random_mode_spans_start_and_end_on_character_boundaries() {
        for index in 0..50 {
            let line = format!("aé{index}béc");
            if let Some((start, end)) = touched_span(Mode::Random, "lhs", index, &line) {
                assert!(line.is_char_boundary(start), "{line:?} start {start}");
                assert!(line.is_char_boundary(end), "{line:?} end {end}");
                assert!(start <= end && end <= line.len());
            }
        }
    }

    #[test]
    fn an_unknown_mode_is_rejected() {
        assert!(Mode::parse("sometimes").is_err());
    }
}
