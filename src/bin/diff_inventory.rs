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
//! is, and how far its two independent ground truths (tree mapping, text painting) have been taken.
//!
//! It lives in `src/test/data/`, not `research/data/`, because it describes the fixtures rather
//! than measuring them. Everything in it is derived; regenerate it rather than editing it.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Parser;

use codediff::test::helper::human_mapping::{
    rebuild_caches_for_mapping, status_after, status_before,
};
use codediff::test::helper::{
    DIFF_DATASETS, code_pair_from_dir_without_metadata, human_mapping, note_as_csv_cell, read_note,
    readme_provenance,
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
/// `Single` vs `MinimalAndFull` is a finding about the fixture, not progress: one painting means
/// the painter judged its rendering unambiguous, two that it genuinely forks. See
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
    /// Repository-relative directory.
    path: String,
    dataset: String,
    language: String,
    // Provenance, from the fixture's own README.md (blank for a handmade fixture), not from
    // sample.csv: a fixture must be self-describing.
    repository: String,
    commit: String,
    source_path: String,
    comment: String,
    before_lines: usize,
    before_nodes: usize,
    after_lines: usize,
    after_nodes: usize,
    /// `NodeStatus::Unmarked` nodes across both trees. `0` means the tree mapping is finished.
    unmatched_nodes: usize,
    /// tree-sitter `ERROR` nodes across both trees (see [`count_error_nodes`]).
    ///
    /// For finding grammar problems, **not** a quality signal: an `ERROR` is a flat blob that
    /// matches an identical blob easily, so badly parsed fixtures disagree with their human
    /// mapping no more often than clean ones.
    error_nodes: usize,
    /// `error_nodes` as a percentage of `before_nodes + after_nodes`, to 3 decimal places; blank
    /// when there is no tree. Separates "one bad region" from "the grammar gave up" across sizes.
    error_pct: String,
    /// `none`, `single`, `minimal+full` or `other` - see [`PaintingState`].
    painting: String,
    /// The painting names, `|`-separated.
    painting_names: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let out = args.out.unwrap_or_else(default_out_path);

    let mut rows = Vec::new();
    let mut unreadable = Vec::new();

    for dataset in DIFF_DATASETS {
        let root = diffs_root().join(dataset);
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            match row_for(&name, dataset, &entry.path()) {
                Ok(Some(row)) => rows.push(row),
                Ok(None) => {}
                Err(err) => unreadable.push((name, format!("{err:#}"))),
            }
        }
    }

    // A stable order, so a regenerated file diffs cleanly against the last one.
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
    if !unreadable.is_empty() {
        println!("  {} unreadable:", unreadable.len());
        for (name, err) in &unreadable {
            println!("    {name}: {err}");
        }
    }
    Ok(())
}

fn row_for(name: &str, dataset: &str, dir: &Path) -> Result<Option<Row>> {
    // Nothing here diffs, so the metadata `code_pair_from_dir` computes would be wasted.
    let Some((before, after)) = code_pair_from_dir_without_metadata(dir)? else {
        return Ok(None);
    };

    // No `human_mapping.json` yet is a state to report, not an error: every node reads unmarked.
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
            // No grammar for this extension: still inventoried, with zero nodes.
            _ => (0, 0, 0, 0),
        };

    let sample = readme_provenance(name);
    let sample = sample.as_ref();
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
        // `description.md` is the only home of a promoted fixture's note; promotion clears the
        // sample.csv cell (pinned by `no_promoted_row_carries_a_comment`).
        comment: read_note(name)
            .map(|note| note_as_csv_cell(&note))
            .unwrap_or_default(),
        before_lines: before.contents.split('\n').count(),
        before_nodes,
        after_lines: after.contents.split('\n').count(),
        after_nodes,
        unmatched_nodes,
        error_nodes,
        // Blank, not "0.000", so no grammar does not read as "parsed cleanly".
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
/// `MISSING` nodes are not counted: they are zero-width recoveries from a small slip (a
/// forgotten semicolon), not regions the grammar could not read.
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

/// A subtree's node count, root inclusive. A local copy of `codediff::stats::count_nodes`, which
/// sits behind the `stats` feature this binary otherwise does not need.
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
        assert_eq!(
            PaintingState::of(&["Full".to_string(), "Tight".to_string()]),
            PaintingState::Other
        );
        assert_eq!(
            PaintingState::of(&["A".to_string(), "B".to_string(), "C".to_string()]),
            PaintingState::Other
        );
    }

    #[test]
    fn every_fixture_in_the_corpus_produces_a_row() {
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
                let row = row_for(&name, dataset, &entry.path())
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
        let dir = codediff::test::helper::diffs_case_dir("cpp-add-templates")
            .expect("a known handmade fixture");
        let row = row_for("cpp-add-templates", "handmade", &dir)
            .unwrap()
            .expect("a row");

        assert_eq!(row.dataset, "handmade");
        assert_eq!(
            row.path, "src/test/data/diffs/handmade/cpp-add-templates",
            "the path should be openable as written"
        );
        assert!(row.repository.is_empty());
        assert!(row.commit.is_empty());
        assert!(row.before_nodes > 0 && row.after_nodes > 0);
    }

    /// The known-zero case guards against a walk that counts every node.
    #[test]
    fn error_nodes_and_their_percentage_come_from_the_real_parse() {
        let row_of = |name: &str, dataset: &str| {
            let dir = codediff::test::helper::diffs_case_dir(name).expect("a known fixture");
            row_for(name, dataset, &dir).unwrap().expect("a row")
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
        let expected =
            100.0 * broken.error_nodes as f64 / (broken.before_nodes + broken.after_nodes) as f64;
        assert!((pct - expected).abs() < 0.001, "{pct} vs {expected}");

        let clean = row_of("cpp-add-templates", "handmade");
        assert_eq!(clean.error_nodes, 0);
        assert_eq!(clean.error_pct, "0.000");
    }

    #[test]
    fn missing_nodes_are_not_counted_as_errors() {
        let code = codediff::code::Code::from_string(
            "fn f() { let x = 1 }\n",
            &codediff::code::Language::Rust,
        );
        let root = code.ast.as_ref().expect("a tree").root_node();
        let mut has_missing = false;
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            has_missing |= node.is_missing();
            let mut cursor = node.walk();
            stack.extend(node.children(&mut cursor));
        }
        assert!(has_missing, "the fixture should parse with a MISSING node");
        assert_eq!(count_error_nodes(root), 0);
    }
}
