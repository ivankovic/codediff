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

//! `benchmark_other`'s harness, end to end, scored against a tool whose answers are known.
//!
//! Every other test of this binary is a unit test of one parser (`gumtree_line_range`,
//! `bdiff_spans_from_script`). The steps *between* them - locating the binary, spawning it,
//! pairing temp files, projecting its spans onto nodes, counting disagreements against the human
//! mapping, choosing a status, writing the CSV - have never been covered, and they are where this
//! code has actually been wrong: a `,0` hunk header shifting every before-side label by one, a
//! char/byte column mix-up invisible on ASCII input, BDiff returning an empty edit script under
//! the user's own git config and scoring as "nothing changed".
//!
//! None of that is testable against a real tool, because nobody knows independently what
//! difftastic *should* answer on a given fixture. So `fake_diff_tool` answers predictably instead,
//! speaking difftastic's JSON so a real adapter is what runs, and `DIFFT_BIN` points at it.
//!
//! **The load-bearing property is complementarity.** Told nothing changed, the harness must report
//! exactly the lines the human mapping says *did* change; told everything changed, exactly the
//! ones it says did *not*. Those two counts must sum to the file. That pins the direction of the
//! comparison, the denominator, and the alignment of the label vectors, all at once, and a
//! one-line shift anywhere breaks it.
//!
//! **Nothing here hardcodes an expected count.** Every expected value is read out of the CSV's own
//! total columns, so re-painting a fixture in `human_solver` - which happens constantly - cannot
//! turn these red. The totals and the mismatch counts check each other.
//!
//! What this does *not* cover: GumTree's, neovim's, BDiff's and the git variants' own output
//! parsers. One adapter is exercised (difftastic's, at both line and node granularity) plus the
//! harness around it.

use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Output};

/// Two fixtures difftastic covers, deliberately different in one respect that matters.
///
/// The JavaScript one contains multi-byte UTF-8. Difftastic's `changes[].start`/`.end` are *byte*
/// columns and the harness passes them to `TextRange` unconverted, while GumTree's are character
/// offsets and are converted - so a fake or an adapter that confused the two would agree on every
/// ASCII fixture and silently disagree past the first non-ASCII character. Both are small; the
/// whole suite spawns about a dozen processes and runs in well under a second.
const SUPPORTED: [&str; 2] = [
    "rust-hello-world-added-message",
    "javascript-jquery-ui-rails-jquery-ui-rails-update-text-in-string",
];

/// Vimscript is absent from `difftastic_extension`, so `ExternalTool::supports` is false for it.
const UNSUPPORTED: &str = "vimscript-chikamichi-mediawiki-add-one-autocmd";

/// The four columns every granularity of the comparison is reported in, paired with the total
/// each one's complement is measured against.
const GRANULARITIES: [(&str, &str); 4] = [
    ("difftastic_line_mismatches", "total_lines"),
    ("difftastic_node_mismatches", "total_nodes"),
    ("difftastic_leaf_node_mismatches", "total_leaf_nodes"),
    ("difftastic_visible_node_mismatches", "total_visible_nodes"),
];

/// Runs a real `benchmark_other --accuracy-csv` over the three fixtures with the fake standing in
/// for difftastic, and returns its `Output` plus the CSV path.
///
/// `--tools difftastic` is not just for speed. Without it `score_accuracy` also runs codediff and
/// the other nine tools on every fixture, which would spawn git four times and shell out to python
/// and neovim per fixture - and would make these assertions depend on codediff's own output, which
/// changes whenever the diff algorithm improves. Scoring one tool keeps the expected values a
/// function of the human mapping alone.
fn run(mode: &str, out_dir: &Path) -> (Output, std::path::PathBuf) {
    let csv_path = out_dir.join(format!("{mode}.csv"));
    let mut fixtures = SUPPORTED.join(",");
    fixtures.push(',');
    fixtures.push_str(UNSUPPORTED);

    let output = Command::new(env!("CARGO_BIN_EXE_benchmark_other"))
        .arg("--accuracy-csv")
        .arg(&csv_path)
        .args(["--fixtures", &fixtures])
        .args(["--tools", "difftastic"])
        // Overrides any real difftastic the developer running this happens to have configured.
        .env("DIFFT_BIN", env!("CARGO_BIN_EXE_fake_diff_tool"))
        .env("FAKE_DIFF_MODE", mode)
        .output()
        .expect("spawning benchmark_other");
    (output, csv_path)
}

/// `run`, asserting the process succeeded, and parsing the CSV into `fixture name -> column ->
/// cell`.
fn scored(mode: &str, out_dir: &Path) -> HashMap<String, HashMap<String, String>> {
    let (output, csv_path) = run(mode, out_dir);
    assert!(
        output.status.success(),
        "benchmark_other failed in {mode} mode: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    read_csv(&csv_path)
}

fn read_csv(path: &Path) -> HashMap<String, HashMap<String, String>> {
    let mut reader = csv::Reader::from_path(path).expect("reading the accuracy CSV");
    let header: Vec<String> = reader
        .headers()
        .expect("CSV header")
        .iter()
        .map(str::to_string)
        .collect();
    reader
        .records()
        .map(|record| {
            let record = record.expect("CSV row");
            let row: HashMap<String, String> = header
                .iter()
                .cloned()
                .zip(record.iter().map(str::to_string))
                .collect();
            (row["solution"].clone(), row)
        })
        .collect()
}

fn cell(row: &HashMap<String, String>, column: &str) -> usize {
    row[column]
        .parse()
        .unwrap_or_else(|_| panic!("column {column} was {:?}, expected a number", row[column]))
}

/// The property everything else rests on: the two degenerate answers partition every line, node,
/// leaf and visible node of both files between them.
///
/// A tool that reports nothing changed disagrees with the human mapping on exactly the things the
/// human says changed. A tool that reports everything changed disagrees on exactly the rest. Their
/// counts therefore sum to the total, at every granularity, for any fixture and any mapping -
/// which is why this needs no expected numbers of its own.
#[test]
fn nothing_changed_and_everything_changed_are_exact_complements() {
    let out_dir = tempfile::tempdir().expect("temp dir");
    let empty = scored("empty", out_dir.path());
    let all = scored("all", out_dir.path());

    for fixture in SUPPORTED {
        let (empty_row, all_row) = (&empty[fixture], &all[fixture]);
        for (column, total_column) in GRANULARITIES {
            let (nothing, everything) = (cell(empty_row, column), cell(all_row, column));
            let total = cell(empty_row, total_column);

            assert_eq!(
                nothing + everything,
                total,
                "{fixture}: {column} must partition {total_column} \
                 ({nothing} + {everything} != {total})"
            );
            // Otherwise the equality above could hold trivially as `0 + total`, which it would
            // even if the scoring were inverted, on a fixture whose mapping happened to touch
            // everything or nothing.
            assert!(
                nothing > 0 && everything > 0,
                "{fixture}: {column} is degenerate ({nothing}/{everything} of {total}) - this \
                 fixture cannot constrain the partition and a different one should be used"
            );
        }
    }
}

/// A language the tool has no parser for is skipped, not scored as though it had answered.
///
/// The distinction is the reason `supports` exists: counting an unrun tool's silence as
/// "everything matched" or "nothing matched" would make the tool with the narrowest language
/// coverage look either perfect or hopeless, and the totals uncomparable across tools.
#[test]
fn an_unsupported_language_is_skipped_rather_than_scored() {
    let out_dir = tempfile::tempdir().expect("temp dir");
    let rows = scored("all", out_dir.path());

    let unsupported = &rows[UNSUPPORTED];
    assert_eq!(unsupported["difftastic_status"], "unsupported");
    for (column, _) in GRANULARITIES {
        assert_eq!(
            unsupported[column], "",
            "{UNSUPPORTED}: {column} must be empty, not a number"
        );
    }
    // Per fixture, not per run: the same tool in the same invocation still scores the languages it
    // does support.
    for fixture in SUPPORTED {
        assert_eq!(rows[fixture]["difftastic_status"], "ok");
    }
}

/// The same input scores identically in a fresh process.
///
/// Compares raw bytes rather than parsed numbers, so column order and formatting are covered too.
/// `random` mode is the mode worth checking this on: `empty` and `all` would be reproducible even
/// if something in the pipeline were order-dependent, because their answers don't vary by line.
#[test]
fn a_random_answer_is_reproduced_exactly_by_a_second_run() {
    let first = tempfile::tempdir().expect("temp dir");
    let second = tempfile::tempdir().expect("temp dir");
    let (_, first_csv) = run("random", first.path());
    let (_, second_csv) = run("random", second.path());

    assert_eq!(
        std::fs::read_to_string(&first_csv).expect("first CSV"),
        std::fs::read_to_string(&second_csv).expect("second CSV"),
        "two runs over the same fixtures produced different CSVs"
    );
}

/// A per-line answer lands strictly between the two degenerate ones, and is a different answer
/// from either.
///
/// This is what shows the harness actually read the tool's output line by line, rather than
/// short-circuiting on a whole-file verdict - a bug both other modes would pass unchanged, since
/// each of them *is* a whole-file verdict.
///
/// **Compared as a whole row, not cell by cell.** A per-cell `!=` looks stronger and is in fact
/// wrong: a count is not an answer, and two different sets of touched nodes can be the same size.
/// The first version of this test asserted `random != empty` per column and failed on
/// `rust-hello-world-added-message`, whose 36 leaf nodes gave both 8 - a collision, not a defect.
/// Requiring the four columns to differ *somewhere* is the property actually meant, and needs all
/// four to collide at once before it can be fooled.
#[test]
fn a_random_answer_is_neither_degenerate_case() {
    let out_dir = tempfile::tempdir().expect("temp dir");
    let (empty, all, random) = (
        scored("empty", out_dir.path()),
        scored("all", out_dir.path()),
        scored("random", out_dir.path()),
    );
    let counts = |rows: &HashMap<String, HashMap<String, String>>, fixture: &str| -> Vec<usize> {
        GRANULARITIES
            .iter()
            .map(|(column, _)| cell(&rows[fixture], column))
            .collect()
    };

    for fixture in SUPPORTED {
        for (column, total_column) in GRANULARITIES {
            let scattered = cell(&random[fixture], column);
            let total = cell(&random[fixture], total_column);
            assert!(
                scattered > 0 && scattered < total,
                "{fixture}: {column} was {scattered}, outside (0, {total}) - a per-line answer                  that agrees or disagrees everywhere is one of the degenerate cases"
            );
        }
        let scattered = counts(&random, fixture);
        assert_ne!(scattered, counts(&empty, fixture), "{fixture} vs empty");
        assert_ne!(scattered, counts(&all, fixture), "{fixture} vs all");
    }
}

/// A tool that fails is recorded as an error and the run carries on.
///
/// Both halves matter. One tool dying must not abort a run that takes minutes and scores nine
/// others - and its column must say `error`, not hold a number, because a failure that scores as
/// zero mismatches reads as a perfect result. That is not hypothetical: it is exactly what BDiff
/// did under a `diff.external` git config, returning an empty edit script with exit status 0.
#[test]
fn a_failing_tool_is_recorded_as_an_error_without_stopping_the_run() {
    let out_dir = tempfile::tempdir().expect("temp dir");
    let (output, csv_path) = run("crash", out_dir.path());

    assert!(
        output.status.success(),
        "a failing tool must not fail the run: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows = read_csv(&csv_path);
    for fixture in SUPPORTED {
        assert_eq!(
            rows[fixture]["difftastic_status"], "error",
            "{fixture} should have recorded the failure"
        );
        for (column, _) in GRANULARITIES {
            assert_eq!(
                rows[fixture][column], "",
                "{fixture}: {column} must stay empty on a failure, never 0"
            );
        }
    }
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("deliberate failure"),
        "the tool's own error text should reach stderr"
    );
}

/// A mistyped tool name is rejected instead of scoring nothing.
///
/// `--tools gumtre` matching no tool would otherwise produce a clean, complete-looking CSV in
/// which every column is simply absent - the kind of empty result that reads as a finished run.
#[test]
fn an_unknown_tool_name_is_rejected() {
    let out_dir = tempfile::tempdir().expect("temp dir");
    let output = Command::new(env!("CARGO_BIN_EXE_benchmark_other"))
        .arg("--accuracy-csv")
        .arg(out_dir.path().join("unused.csv"))
        .args(["--fixtures", SUPPORTED[0]])
        .args(["--tools", "gumtre"])
        .output()
        .expect("spawning benchmark_other");

    assert!(!output.status.success(), "a bad --tools value must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown tool 'gumtre'"),
        "stderr was: {stderr}"
    );
    // The message has to name the alternatives, or the reader's next move is to read the source.
    assert!(
        stderr.contains("gumtree"),
        "stderr should list valid names: {stderr}"
    );
}

/// Same, for a fixture name nothing matches.
#[test]
fn an_unknown_fixture_name_is_rejected() {
    let out_dir = tempfile::tempdir().expect("temp dir");
    let output = Command::new(env!("CARGO_BIN_EXE_benchmark_other"))
        .arg("--accuracy-csv")
        .arg(out_dir.path().join("unused.csv"))
        .args(["--fixtures", "no-such-fixture-anywhere"])
        .output()
        .expect("spawning benchmark_other");

    assert!(!output.status.success(), "a bad --fixtures value must fail");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("no-such-fixture-anywhere"),
        "stderr should name the fixture it could not find"
    );
}
