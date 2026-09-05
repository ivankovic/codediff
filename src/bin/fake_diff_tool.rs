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
//! `benchmark_other` scores external tools by spawning them, parsing their output and comparing
//! the result against the human mapping. Every one of those steps has been wrong at some point -
//! a `,0` hunk header shifting every before-side label by one, a char/byte column mix-up invisible
//! on ASCII input, BDiff silently returning an empty edit script under the user's own git config -
//! and none of it is testable against a real tool, whose correct answer nobody knows independently.
//!
//! This binary answers *predictably* instead of correctly, which makes the harness's arithmetic
//! checkable. It speaks difftastic's `--display json` protocol and is pointed at by `DIFFT_BIN`,
//! so it exercises a real adapter (`difftastic_line_labels` and `difftastic_node_spans`) rather
//! than a test-only path that could drift away from how tools are really read.
//!
//! `FAKE_DIFF_MODE` picks the answer:
//!
//! * `empty` - nothing changed. Mismatches against the human mapping then count exactly the lines
//!   the human says *did* change.
//! * `all` - every line on both sides is part of the edit. Mismatches then count exactly the lines
//!   the human says did *not* change.
//! * `random` - a per-line verdict from a hash of the line. Neither degenerate case, and identical
//!   on every run and in every process.
//! * `crash` - exit non-zero with a message on stderr, the way a real tool fails.
//!
//! The first two are complements, which is the property the end-to-end test is built on: their
//! mismatch counts must sum to the total number of lines, whatever the human mapping happens to
//! say. That holds without hardcoding a single expected count, so the test survives the corpus
//! being re-painted - which it constantly is.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde_json::json;

/// Which answer to give. Parsed from `FAKE_DIFF_MODE`; there is no default, because a fake tool
/// silently picking a mode is exactly the failure this whole binary exists to make impossible.
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

/// The two files to compare, picked out of an argument list written for the real difftastic.
///
/// Selected by "is this an existing file" rather than by position, so the flags the harness passes
/// (`--display json` today) are ignored without this needing to know them. A wrong count is an
/// error rather than a guess: silently comparing the wrong pair would produce plausible numbers,
/// which is the failure mode this binary exists to rule out.
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
///
/// Pure, so the shape of every mode is unit-testable without spawning anything.
fn render(mode: Mode, before: &str, after: &str) -> String {
    if mode == Mode::Empty {
        // Difftastic omits `chunks` entirely for an unchanged file rather than emitting an empty
        // array - see `difftastic_touched_from_json`'s doc comment, which reads it that way. The
        // fake reproduces the real shape so the adapter's "no chunks key" branch is what runs.
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
/// **Byte columns, not character offsets.** Difftastic's `changes[].start`/`.end` are byte columns
/// within the line and the harness passes them through to `TextRange` unconverted (GumTree's, by
/// contrast, are whole-file character offsets and *are* converted). A fake emitting character
/// offsets would agree with the harness on every ASCII fixture and quietly disagree on any line
/// with a multi-byte character in it, which is why the end-to-end test scores a fixture that has
/// one.
fn touched_span(mode: Mode, side: &str, index: usize, line: &str) -> Option<(usize, usize)> {
    match mode {
        Mode::Empty | Mode::Crash => None,
        // Including the empty string `split('\n')` yields after a trailing newline. That phantom
        // element is a real slot in the harness's label vector (`vec![false; split('\n').count()]`),
        // so "every line is part of the edit" has to fill it too or the two sides' counts are off
        // by one per file - the zero-width span below is how that slot is claimed.
        Mode::All => Some((0, line.len())),
        Mode::Random => {
            let hash = line_hash(side, index, line);
            if hash % 2 == 0 {
                return None;
            }
            // A sub-line range rather than the whole line: a real AST-aware tool reports spans
            // narrower than a line, and that is the path `difftastic_node_spans` takes.
            let start = snap_down(line, (hash >> 8) as usize % (line.len() + 1));
            let end = snap_up(
                line,
                start + (hash >> 24) as usize % (line.len() - start + 1),
            );
            Some((start, end))
        }
    }
}

/// A stable verdict for one line.
///
/// A hash of the content rather than a seeded RNG, deliberately: an RNG's answer depends on how
/// many numbers were drawn before it, so it would be stable only as long as nothing changed the
/// order lines are visited in or how many files a process handles. A hash gives the same answer
/// for the same line in any order, in any process, on any run - which is what "repeatable test"
/// has to mean. The side is folded in so the before and after sides of an unchanged line don't
/// receive the same verdict.
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

/// SplitMix64's finalizer, over the raw FNV-1a value.
///
/// **FNV-1a's low bits are not usable as a verdict.** Its multiplier is odd, so multiplying
/// preserves parity and the only step that can change it is the XOR - which makes bit 0 of the
/// result exactly "was an odd number of odd bytes eaten". Not a hash bit at all: a parity check.
/// Two lines differing by one even byte get the same answer, and `hash % 2` on this project's own
/// short fixture lines came out `1` for all eight lines of a four-line pair on the first try. The
/// shifted bits the span offsets are drawn from are correlated for the same reason.
///
/// This is the standard fix - an avalanche step that makes every output bit depend on every input
/// bit - and it keeps the property the verdict actually needs: a pure function of the line, with
/// no state carried between lines.
fn mix(mut hash: u64) -> u64 {
    hash ^= hash >> 30;
    hash = hash.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    hash ^= hash >> 27;
    hash = hash.wrapping_mul(0x94d0_49bb_1331_11eb);
    hash ^ (hash >> 31)
}

/// `index` moved back to the nearest character boundary, so a hash-derived offset never lands
/// inside a multi-byte character. Nothing downstream slices the line with these, so a split offset
/// would not panic - it would just describe a span no character starts at, which is a needlessly
/// strange thing for a test instrument to emit.
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

    /// The unchanged shape difftastic really emits - `difftastic_touched_from_json` branches on
    /// the *absence* of the key, so an empty array here would exercise a different path than a
    /// real unchanged file does.
    #[test]
    fn empty_mode_omits_the_chunks_key_entirely() {
        let json = parse(Mode::Empty, "a\n", "b\n");
        assert!(json.get("chunks").is_none(), "{json}");
    }

    /// Every slot in the harness's label vector is claimed, including the empty string
    /// `split('\n')` yields after a trailing newline. Miss it and both sides are short by one,
    /// which shows up as a complementarity failure of exactly 2.
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

    /// Byte columns, not character offsets - the convention difftastic uses and the harness reads
    /// unconverted. `é` is two bytes, so a two-character line is three columns wide.
    #[test]
    fn all_mode_reports_byte_columns_not_character_offsets() {
        let json = parse(Mode::All, "bé\n", "x\n");
        assert_eq!(json["chunks"][0][0]["lhs"]["changes"][0]["end"], 3);
    }

    /// The regression that made this file need `mix`, pinned by the property that actually
    /// separates a hash bit from a parity bit.
    ///
    /// FNV-1a's multiplier is odd, so multiplying preserves parity and bit 0 of the raw hash is
    /// exactly "was an odd number of odd bytes eaten" - a parity check, not a hash bit. Under it,
    /// changing an **even** byte to another even byte can never flip the verdict. So: one fixed
    /// index and side, lines differing only in an even-valued byte. A parity bit gives all sixteen
    /// the same answer; a hash splits them.
    ///
    /// A first version of this test varied `line {index}` instead, and passed with the finalizer
    /// removed - the odd digits in the varying text flipped the parity by themselves, so it never
    /// tested the thing it was named for. This one was verified to fail with `mix` taken out.
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

    /// A pure function of the line, so the same line scores the same way regardless of what was
    /// asked before it. This is what makes the end-to-end test's cross-process determinism hold.
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

    /// Offsets never land inside a multi-byte character.
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
