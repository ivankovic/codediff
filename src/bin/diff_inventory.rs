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

//! One row per test fixture, written to `src/test/data/diffs.csv`: where it came from, how big it
//! is, and how far its two ground truths have been taken.
//!
//! An *inventory*, not an analysis - and the distinction is what justifies a separate binary next
//! to the two that already read this corpus. `analyze_human_mappings` answers questions about the
//! corpus's shape (operation mix, reparent rate, reorder rate) and `benchmark_optimal_solutions`
//! scores codediff against it. Neither answers "what have we got, and what still needs work",
//! which is the question you ask when deciding what to open next - and which now has two separate
//! answers per fixture, since the tree mapping and the text painting are independent ground truths
//! completed at different times (see `HumanTextMapping`).
//!
//! Deliberately written into `src/test/data/`, beside `sample.csv`, rather than under
//! `research/data/`: this describes the fixtures themselves, not a measurement over them, and it is
//! the same kind of file as the sample manifest it joins against. Everything in it is cheap and
//! derived, so it is safe to regenerate at any time and never needs hand-editing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Parser;

use codediff::test::helper::human_mapping::{
    rebuild_caches_for_mapping, status_after, status_before,
};
use codediff::test::helper::{
    DIFF_DATASETS, SampleProvenance, code_pair_from_dir, human_mapping, note_as_csv_cell,
    read_note, sample_provenance,
};

#[derive(Parser)]
#[command(
    about = "Inventory of src/test/data/diffs/ - provenance, size and ground-truth completeness, one row per fixture"
)]
struct Args {
    /// Where to write the CSV. Default: ./src/test/data/diffs.csv
    #[arg(long, value_name = "PATH")]
    out: Option<PathBuf>,
}

/// How far a fixture's text painting has been taken (see `HumanMapping::text_mappings`).
///
/// The three states the painter actually distinguishes, plus a catch-all. `MinimalAndFull` and
/// `Single` are not "more" and "less" work on the same thing: a single painting means the painter
/// judged this fixture's rendering unambiguous, and two means they judged it genuinely forked -
/// so the distinction is a finding about the fixture, not a progress bar. See
/// `research/data/quality/text_painting_findings.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaintingState {
    None,
    Single,
    MinimalAndFull,
    Other,
}

impl PaintingState {
    fn of(names: &[String]) -> Self {
        let has = |wanted: &str| names.iter().any(|name| name == wanted);
        match names.len() {
            0 => PaintingState::None,
            1 => PaintingState::Single,
            2 if has("Minimal") && has("Full") => PaintingState::MinimalAndFull,
            _ => PaintingState::Other,
        }
    }

    fn name(self) -> &'static str {
        match self {
            PaintingState::None => "none",
            PaintingState::Single => "single",
            PaintingState::MinimalAndFull => "minimal+full",
            PaintingState::Other => "other",
        }
    }
}

/// One fixture's row.
#[derive(Debug, serde::Serialize)]
struct Row {
    name: String,
    /// Repository-relative directory, so a reader can open it without reconstructing the path from
    /// `dataset` and `name` themselves.
    path: String,
    dataset: String,
    language: String,
    // ── provenance, joined from sample.csv on `promoted_to` ──────────────────────────────────
    // Blank for a handmade fixture that was never promoted from a sample, which is not missing
    // data: it has no upstream commit to point at.
    repository: String,
    commit: String,
    source_path: String,
    comment: String,
    // ── size ────────────────────────────────────────────────────────────────────────────────
    before_lines: usize,
    before_nodes: usize,
    after_lines: usize,
    after_nodes: usize,
    /// Nodes across *both* trees that the tree mapping says nothing about (`NodeStatus::Unmarked`)
    /// - i.e. how much annotation work is left. `0` means the tree side is finished.
    unmatched_nodes: usize,
    /// tree-sitter `ERROR` nodes across both trees: places the grammar could not parse. 13.3% of
    /// the corpus has at least one (measured 2026-08-28), heavily concentrated by language - four
    /// in five C fixtures, against one in fifty for Rust.
    ///
    /// **Not a quality signal, despite looking like one.** Fixtures with parse errors are very
    /// slightly *less* likely to disagree with their human mapping than clean ones (26% vs 30%,
    /// at a 0.8x lower mismatch rate), and seven of the eight most shredded parses in the corpus
    /// score exactly zero mismatches. An `ERROR` wraps a run of text as a flat blob, and a flat
    /// blob matches another identical flat blob easily - the tree carries less information, so
    /// there is less to disagree about. This column is here to find grammar problems, not to
    /// predict accuracy ones.
    ///
    /// `MISSING` nodes are deliberately not counted here: they are zero-width nodes the parser
    /// *inserts* to recover from a small slip, not regions it failed to read, and 32 fixtures have
    /// one without having any `ERROR` at all.
    error_nodes: usize,
    /// `error_nodes` as a percentage of `before_nodes + after_nodes`, to 3 decimal places.
    ///
    /// Worth having next to the count because the two say different things: one `ERROR` wrapping
    /// an otherwise well-formed file (`c-microsoft-terminal-add-function`, 761 of 10,966 nodes) is
    /// nearly harmless, while one `ERROR` per 2.3 nodes (`css-shadcn-ui-ui-completely-broken-
    /// treesitter-parsing`, 13,933 of 32,682) means the grammar gave up. The count alone cannot
    /// tell those apart on files of different sizes.
    error_pct: String,
    /// `none`, `single`, `minimal+full` or `other` - see [`PaintingState`].
    painting: String,
    /// The painting names themselves, `|`-separated, since `painting` deliberately collapses them.
    painting_names: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let out = args.out.unwrap_or_else(default_out_path);

    let provenance = sample_provenance()?;
    let mut rows = Vec::new();
    let mut unreadable = Vec::new();

    for dataset in DIFF_DATASETS {
        let root = diffs_root().join(dataset);
        let Ok(entries) = std::fs::read_dir(&root) else {
            // A dataset directory that doesn't exist is normal - `DIFF_DATASETS` lists every split
            // this repository has ever used, and `stratified` is currently empty.
            continue;
        };
        for entry in entries {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            match row_for(&name, dataset, &entry.path(), &provenance) {
                Ok(Some(row)) => rows.push(row),
                // A directory with no readable before/after pair isn't a fixture at all.
                Ok(None) => {}
                Err(err) => unreadable.push((name, format!("{err:#}"))),
            }
        }
    }

    // Sorted by dataset then name, so a regenerated file diffs cleanly against the last one rather
    // than reshuffling with whatever order the filesystem handed back.
    rows.sort_by(|a, b| (&a.dataset, &a.name).cmp(&(&b.dataset, &b.name)));

    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut writer = csv::Writer::from_path(&out)
        .with_context(|| format!("writing the inventory to {out:?}"))?;
    for row in &rows {
        writer.serialize(row)?;
    }
    writer.flush()?;

    let painted = rows.iter().filter(|r| r.painting != "none").count();
    let tree_done = rows.iter().filter(|r| r.unmatched_nodes == 0).count();
    let with_errors = rows.iter().filter(|r| r.error_nodes > 0).count();
    let error_nodes: usize = rows.iter().map(|r| r.error_nodes).sum();
    let all_nodes: usize = rows.iter().map(|r| r.before_nodes + r.after_nodes).sum();
    println!(
        "{} fixtures written to {:?}\n  tree mapping complete: {} ({} with unmarked nodes left)\n  \
         text painted: {} ({} not yet painted)\n  parse errors: {} fixture(s), {} ERROR node(s) \
         ({:.2}% of the corpus)",
        rows.len(),
        out,
        tree_done,
        rows.len() - tree_done,
        painted,
        rows.len() - painted,
        with_errors,
        error_nodes,
        if all_nodes == 0 {
            0.0
        } else {
            100.0 * error_nodes as f64 / all_nodes as f64
        },
    );
    // Reported, never silently skipped: a fixture this couldn't read is exactly the kind of thing
    // an inventory exists to surface.
    if !unreadable.is_empty() {
        println!("  {} unreadable:", unreadable.len());
        for (name, err) in &unreadable {
            println!("    {name}: {err}");
        }
    }
    Ok(())
}

fn row_for(
    name: &str,
    dataset: &str,
    dir: &Path,
    provenance: &HashMap<String, SampleProvenance>,
) -> Result<Option<Row>> {
    let Some((before, after)) = code_pair_from_dir(dir)? else {
        return Ok(None);
    };

    // `unwrap_or_default` rather than `?`: a fixture with no `human_mapping.json` yet is an
    // ordinary state this file exists to report, not an error. It reads as every node unmarked,
    // which is exactly true.
    let mapping = human_mapping::load(name).unwrap_or_default();

    let (before_nodes, after_nodes, unmatched_nodes, error_nodes) =
        match (before.ast.as_ref(), after.ast.as_ref()) {
            (Some(before_ast), Some(after_ast)) => {
                let before_root = before_ast.root_node();
                let after_root = after_ast.root_node();
                let caches = rebuild_caches_for_mapping(&mapping, before_root, after_root);
                (
                    count_nodes(before_root),
                    count_nodes(after_root),
                    human_mapping::unmarked_node_count(before_root, &caches, status_before)
                        + human_mapping::unmarked_node_count(after_root, &caches, status_after),
                    count_error_nodes(before_root) + count_error_nodes(after_root),
                )
            }
            // No grammar for this extension: the file pair is still real and still worth inventorying,
            // it just has no tree to count. Zeroes here mean "no AST", and `before_lines` still says
            // how big it is.
            _ => (0, 0, 0, 0),
        };

    let sample = provenance.get(name);
    let mut painting_names: Vec<String> = mapping
        .text_mappings
        .iter()
        .map(|named| named.name.clone())
        .collect();
    painting_names.sort();

    Ok(Some(Row {
        name: name.to_string(),
        path: format!("src/test/data/diffs/{dataset}/{name}"),
        dataset: dataset.to_string(),
        language: format!("{:?}", before.metadata.language.unwrap_or_default()),
        repository: sample.map(|s| s.repository.clone()).unwrap_or_default(),
        commit: sample.map(|s| s.commit.clone()).unwrap_or_default(),
        source_path: sample.map(|s| s.path.clone()).unwrap_or_default(),
        // `description.md` is the only place a promoted fixture's note lives. sample.csv used to
        // be a second home for it and this column used to fall back to that; promotion now moves
        // the note into the file and clears the cell, so there is nothing left to fall back to
        // and no way for two copies to disagree (see `action_promote`, and the
        // `no_promoted_row_carries_a_comment` test that pins it).
        comment: read_note(name)
            .map(|note| note_as_csv_cell(&note))
            .unwrap_or_default(),
        before_lines: before.contents.split('\n').count(),
        before_nodes,
        after_lines: after.contents.split('\n').count(),
        after_nodes,
        unmatched_nodes,
        error_nodes,
        // Blank rather than "0.000" when there is no tree to measure, so a fixture with no grammar
        // reads as "not applicable" instead of "parsed cleanly".
        error_pct: if before_nodes + after_nodes == 0 {
            String::new()
        } else {
            format!(
                "{:.3}",
                100.0 * error_nodes as f64 / (before_nodes + after_nodes) as f64
            )
        },
        painting: PaintingState::of(&painting_names).name().to_string(),
        painting_names: painting_names.join("|"),
    }))
}

/// tree-sitter `ERROR` nodes in a subtree, root inclusive.
///
/// `is_error()` only - not `is_missing()`. The two mean different things: an `ERROR` is a region
/// the grammar could not read, while a `MISSING` is a zero-width node the parser *inserted* to
/// recover from something small (a forgotten semicolon). Counting them together would put a file
/// with one missing brace in the same bucket as one the grammar gave up on. See `Row::error_nodes`.
fn count_error_nodes(root: tree_sitter::Node) -> usize {
    let mut count = 0;
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.is_error() {
            count += 1;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    count
}

/// Counts a tree-sitter subtree's size, root inclusive - a local copy of
/// `codediff::stats::count_nodes` for the same reason `analyze_human_mappings` keeps one: that
/// function is behind the `stats` feature (git2/rusqlite and the rest of its build cost), which
/// this binary has no other reason to pull in.
fn count_nodes(root: tree_sitter::Node) -> usize {
    let mut count = 0;
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        count += 1;
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    count
}

fn data_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
}

fn diffs_root() -> PathBuf {
    data_root().join("diffs")
}

fn default_out_path() -> PathBuf {
    data_root().join("diffs.csv")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn painting_state_distinguishes_the_three_states_the_painter_cares_about() {
        assert_eq!(PaintingState::of(&[]), PaintingState::None);
        assert_eq!(
            PaintingState::of(&["Only one solution".to_string()]),
            PaintingState::Single
        );
        assert_eq!(
            PaintingState::of(&["Full".to_string(), "Minimal".to_string()]),
            PaintingState::MinimalAndFull
        );
        // Two paintings that aren't that pair, or three of anything, are real but not one of the
        // states the painter converged on - reported as `other` rather than squeezed into one.
        assert_eq!(
            PaintingState::of(&["Full".to_string(), "Tight".to_string()]),
            PaintingState::Other
        );
        assert_eq!(
            PaintingState::of(&["A".to_string(), "B".to_string(), "C".to_string()]),
            PaintingState::Other
        );
    }

    /// The inventory has to describe the corpus it is run against, so this checks the real one
    /// rather than a constructed pair - a fixture that fails to read is exactly what it exists to
    /// surface, and a stub would never catch that.
    #[test]
    fn every_fixture_in_the_corpus_produces_a_row() {
        let provenance = sample_provenance().unwrap();
        let mut seen = 0usize;
        for dataset in DIFF_DATASETS {
            let root = diffs_root().join(dataset);
            let Ok(entries) = std::fs::read_dir(&root) else {
                continue;
            };
            for entry in entries.flatten() {
                if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                let row = row_for(&name, dataset, &entry.path(), &provenance)
                    .unwrap_or_else(|err| panic!("'{name}' should be readable: {err:#}"));
                if let Some(row) = row {
                    assert!(row.before_lines > 0, "'{name}' has no before content");
                    assert!(row.after_lines > 0, "'{name}' has no after content");
                    seen += 1;
                }
            }
        }
        assert!(seen > 100, "expected the whole corpus, saw {seen}");
    }

    #[test]
    fn a_handmade_fixture_has_blank_provenance_rather_than_a_missing_row() {
        let provenance = sample_provenance().unwrap();
        let dir = codediff::test::helper::diffs_case_dir("cpp-add-templates")
            .expect("a known handmade fixture");
        let row = row_for("cpp-add-templates", "handmade", &dir, &provenance)
            .unwrap()
            .expect("a row");

        assert_eq!(row.dataset, "handmade");
        assert_eq!(
            row.path, "src/test/data/diffs/handmade/cpp-add-templates",
            "the path should be openable as written"
        );
        // Never promoted from a sample, so it has no upstream commit to point at - blank, not
        // absent, since the fixture itself is perfectly real.
        assert!(row.repository.is_empty());
        assert!(row.commit.is_empty());
        assert!(row.before_nodes > 0 && row.after_nodes > 0);
    }

    /// Both ends of the error columns, against two real fixtures - one the grammar gave up on and
    /// one it read cleanly. A count without a known-zero case would pass just as happily if the
    /// walk counted every node.
    #[test]
    fn error_nodes_and_their_percentage_come_from_the_real_parse() {
        let provenance = sample_provenance().unwrap();
        let row_of = |name: &str, dataset: &str| {
            let dir = codediff::test::helper::diffs_case_dir(name).expect("a known fixture");
            row_for(name, dataset, &dir, &provenance)
                .unwrap()
                .expect("a row")
        };

        // The worst parse in the corpus: one ERROR per 2.3 nodes.
        let broken = row_of(
            "css-shadcn-ui-ui-completely-broken-treesitter-parsing",
            "full",
        );
        assert!(
            broken.error_nodes > 1000,
            "expected thousands of ERROR nodes, got {}",
            broken.error_nodes
        );
        let pct: f64 = broken.error_pct.parse().expect("a number");
        assert!((10.0..=100.0).contains(&pct), "got {pct}");
        // The percentage has to be of this fixture's own nodes, not of anything global.
        let expected =
            100.0 * broken.error_nodes as f64 / (broken.before_nodes + broken.after_nodes) as f64;
        assert!((pct - expected).abs() < 0.001, "{pct} vs {expected}");

        // And a fixture that parses cleanly reads as exactly zero, not as blank or absent.
        let clean = row_of("cpp-add-templates", "handmade");
        assert_eq!(clean.error_nodes, 0);
        assert_eq!(clean.error_pct, "0.000");
    }
}
