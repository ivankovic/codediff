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

//! `benchmark_other`'s harness end to end, scored against a tool whose answers are known.
//!
//! No one knows independently what a real tool should answer on a fixture, so `fake_diff_tool`
//! answers predictably in difftastic's JSON, via `DIFFT_BIN`, and the real difftastic adapter runs.
//! The load-bearing property is complementarity: "nothing changed" and "everything changed" must
//! give mismatch counts that sum to the total, which pins the comparison's direction, denominator
//! and alignment at once. No expected count is hardcoded, so re-painting a fixture cannot break these.
//! The other tools' output parsers are not covered here.

use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Output};

/// The JavaScript fixture contains multi-byte UTF-8: difftastic reports byte columns, and a
/// char/byte mix-up only shows past the first non-ASCII character.
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

/// Runs `benchmark_other --accuracy-csv` over the fixtures with the fake as difftastic.
///
/// `--tools difftastic` keeps expected values a function of the human mapping alone: scoring
/// codediff too would tie them to the diff algorithm's current output.
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
        .env("DIFFT_BIN", env!("CARGO_BIN_EXE_fake_diff_tool"))
        .env("FAKE_DIFF_MODE", mode)
        .output()
        .expect("spawning benchmark_other");
    (output, csv_path)
}

/// One accuracy CSV. The header is kept because which columns exist is what shows `--tools`
/// narrowed the run.
struct Csv {
    header: Vec<String>,
    rows: HashMap<String, HashMap<String, String>>,
}

impl std::ops::Index<&str> for Csv {
    type Output = HashMap<String, String>;
    fn index(&self, fixture: &str) -> &Self::Output {
        self.rows
            .get(fixture)
            .unwrap_or_else(|| panic!("no row for {fixture} in {:?}", self.rows.keys()))
    }
}

fn scored(mode: &str, out_dir: &Path) -> Csv {
    let (output, csv_path) = run(mode, out_dir);
    assert!(
        output.status.success(),
        "benchmark_other failed in {mode} mode: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    read_csv(&csv_path)
}

fn read_csv(path: &Path) -> Csv {
    let mut reader = csv::Reader::from_path(path).expect("reading the accuracy CSV");
    let header: Vec<String> = reader
        .headers()
        .expect("CSV header")
        .iter()
        .map(str::to_string)
        .collect();
    let rows = reader
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
        .collect();
    Csv { header, rows }
}

fn cell(row: &HashMap<String, String>, column: &str) -> usize {
    row[column]
        .parse()
        .unwrap_or_else(|_| panic!("column {column} was {:?}, expected a number", row[column]))
}

/// "Nothing changed" disagrees exactly where the human says something changed, "everything
/// changed" exactly everywhere else, so the two counts sum to the total at every granularity.
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
            // `0 + total` would pass even with inverted scoring.
            assert!(
                nothing > 0 && everything > 0,
                "{fixture}: {column} is degenerate ({nothing}/{everything} of {total}) - this \
                 fixture cannot constrain the partition and a different one should be used"
            );
        }
    }
}

/// Counting an unrun tool's silence as a score would make narrow-coverage tools look perfect or
/// hopeless.
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
    // Per fixture, not per run.
    for fixture in SUPPORTED {
        assert_eq!(rows[fixture]["difftastic_status"], "ok");
    }
}

/// Compares raw bytes, so column order and formatting count. `random` is the mode that can
/// expose order dependence; the other modes answer the same for every line.
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

/// Shows the harness reads the output line by line rather than a whole-file verdict, which both
/// degenerate modes are.
///
/// Rows are compared whole, not per cell: two different answers can have the same count (two
/// fixtures' leaf counts collide), so the property is that the four columns differ somewhere.
#[test]
fn a_random_answer_is_neither_degenerate_case() {
    let out_dir = tempfile::tempdir().expect("temp dir");
    let (empty, all, random) = (
        scored("empty", out_dir.path()),
        scored("all", out_dir.path()),
        scored("random", out_dir.path()),
    );
    let counts = |rows: &Csv, fixture: &str| -> Vec<usize> {
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

/// A failure must not abort the run, and must read `error`, not a number: a failure scored as
/// zero mismatches looks perfect, as an empty edit script with exit status 0 would.
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

/// The other tests still pass if both filters are ignored, so this checks the narrowing against
/// an unfiltered control run.
#[test]
fn the_scoping_flags_narrow_the_run() {
    let out_dir = tempfile::tempdir().expect("temp dir");
    let filtered = scored("all", out_dir.path());

    assert_eq!(
        filtered.rows.len(),
        SUPPORTED.len() + 1,
        "--fixtures did not narrow the corpus - scored {:?}",
        filtered.rows.keys().collect::<Vec<_>>()
    );
    let codediff_columns = |csv: &Csv| -> usize {
        csv.header
            .iter()
            .filter(|column| column.starts_with("codediff_"))
            .count()
    };
    assert_eq!(
        codediff_columns(&filtered),
        0,
        "--tools did not gate codediff: {:?}",
        filtered.header
    );

    // Without the control, "no codediff columns" could mean codediff is never scored at all.
    let control_path = out_dir.path().join("control.csv");
    let output = Command::new(env!("CARGO_BIN_EXE_benchmark_other"))
        .arg("--accuracy-csv")
        .arg(&control_path)
        .args(["--fixtures", SUPPORTED[0]])
        .env("DIFFT_BIN", env!("CARGO_BIN_EXE_fake_diff_tool"))
        .env("FAKE_DIFF_MODE", "all")
        .output()
        .expect("spawning benchmark_other");
    assert!(
        output.status.success(),
        "unfiltered run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let control = read_csv(&control_path);
    assert!(
        codediff_columns(&control) > 0,
        "codediff should be scored when --tools is absent: {:?}",
        control.header
    );
    assert!(
        control.header.len() > filtered.header.len(),
        "--tools should leave fewer columns than an unfiltered run ({} vs {})",
        filtered.header.len(),
        control.header.len()
    );
}

/// A typo would otherwise produce a clean CSV with the tool's columns simply absent.
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
    assert!(
        stderr.contains("gumtree"),
        "stderr should list valid names: {stderr}"
    );
}

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
