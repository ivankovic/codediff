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
    NodeStatus, rebuild_caches_for_mapping, status_after, status_before,
};
use codediff::test::helper::{DIFF_DATASETS, code_pair_from_dir, human_mapping};

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
    println!(
        "{} fixtures written to {:?}\n  tree mapping complete: {} ({} with unmarked nodes left)\n  text painted: {} ({} not yet painted)",
        rows.len(),
        out,
        tree_done,
        rows.len() - tree_done,
        painted,
        rows.len() - painted,
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
    provenance: &HashMap<String, SampleRow>,
) -> Result<Option<Row>> {
    let Some((before, after)) = code_pair_from_dir(dir)? else {
        return Ok(None);
    };

    // `unwrap_or_default` rather than `?`: a fixture with no `human_mapping.json` yet is an
    // ordinary state this file exists to report, not an error. It reads as every node unmarked,
    // which is exactly true.
    let mapping = human_mapping::load(name).unwrap_or_default();

    let (before_nodes, after_nodes, unmatched_nodes) =
        match (before.ast.as_ref(), after.ast.as_ref()) {
            (Some(before_ast), Some(after_ast)) => {
                let before_root = before_ast.root_node();
                let after_root = after_ast.root_node();
                let caches = rebuild_caches_for_mapping(&mapping, before_root, after_root);
                (
                    count_nodes(before_root),
                    count_nodes(after_root),
                    count_unmarked(before_root, &caches, status_before)
                        + count_unmarked(after_root, &caches, status_after),
                )
            }
            // No grammar for this extension: the file pair is still real and still worth inventorying,
            // it just has no tree to count. Zeroes here mean "no AST", and `before_lines` still says
            // how big it is.
            _ => (0, 0, 0),
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
        comment: sample.map(|s| s.comment.clone()).unwrap_or_default(),
        before_lines: before.contents.split('\n').count(),
        before_nodes,
        after_lines: after.contents.split('\n').count(),
        after_nodes,
        unmatched_nodes,
        painting: PaintingState::of(&painting_names).name().to_string(),
        painting_names: painting_names.join("|"),
    }))
}

/// Nodes in `root`'s tree the mapping says nothing about.
fn count_unmarked(
    root: tree_sitter::Node,
    caches: &human_mapping::Caches,
    status_fn: fn(tree_sitter::Node, &human_mapping::Caches) -> NodeStatus,
) -> usize {
    let mut count = 0;
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if status_fn(node, caches) == NodeStatus::Unmarked {
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

/// The provenance columns of one `sample.csv` row.
struct SampleRow {
    repository: String,
    commit: String,
    path: String,
    comment: String,
}

/// `sample.csv` keyed by the fixture name it was promoted to.
///
/// Rows with an empty `promoted_to` are candidates that were never promoted (or were rejected), so
/// they name no fixture and are skipped.
fn sample_provenance() -> Result<HashMap<String, SampleRow>> {
    let path = data_root().join("sample.csv");
    let mut out = HashMap::new();
    if !path.exists() {
        return Ok(out);
    }
    let mut reader = csv::Reader::from_path(&path)
        .with_context(|| format!("reading sample provenance from {path:?}"))?;
    for record in reader.deserialize::<HashMap<String, String>>() {
        let record = record.context("parsing a sample.csv row")?;
        let promoted_to = record.get("promoted_to").cloned().unwrap_or_default();
        if promoted_to.is_empty() {
            continue;
        }
        let field = |key: &str| record.get(key).cloned().unwrap_or_default();
        out.insert(
            promoted_to,
            SampleRow {
                repository: field("repository"),
                commit: field("commit"),
                path: field("path"),
                comment: field("comment"),
            },
        );
    }
    Ok(out)
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
}
