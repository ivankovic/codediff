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

//! Repeatable, non-`cargo test` benchmark over every fixture in `src/test/data/diffs/`: for each
//! one that already has a `human_mapping.json` (see `src/bin/human_solver.rs`), runs codediff and
//! counts how many nodes disagree with the human-authored mapping; for fixtures that don't have
//! one yet, counts them separately as "unsolved" rather than silently ignoring them.
//!
//! This exists so that algorithm changes can be measured by a single mismatch total instead of a
//! pass/fail count from `cargo test optimal_solutions` - a change that turns one fixture's 32
//! mismatches into 4 without yet reaching 0 shows up here as progress; under `cargo test` it's
//! just "still failing," indistinguishable from a change that made no difference at all.

use anyhow::{Context, Result, bail};
use clap::Parser;
use codediff::code::Code;
use codediff::diff::ASTMappingReason;
use codediff::diff::cost::diff_cost;
use codediff::test::helper;
use codediff::test::helper::human_mapping;
use std::collections::{BTreeMap, HashMap};
use std::fs::File;

use csv::Writer;

/// Short column label for every `ASTMappingReason` variant *except* `APTED`, in declaration order.
/// Kept as an explicit list (rather than deriving one) so the table's column order stays stable
/// even if variants are reordered in `diff.rs`. `APTED` is deliberately excluded: unlike every
/// other variant, it doesn't map to one fixed column - see `reason_column_label`.
///
/// **This list must cover every label `ASTMappingReason::bucket_label` can return**, and
/// `non_apted_reason_labels_covers_every_bucket_label` enforces it. An explicit list is only safe
/// with that test: `bucket_label`'s match is exhaustive, so adding a variant fails to compile
/// *there*, but nothing previously forced this list to follow. It had drifted by eight labels -
/// `LeadSib`, `BottomUpProp`, `UniqueType`, `Unresolved`, `MutualAnc`, `CondCollapse`,
/// `HeritageGrowth` and `WrapGrowth` had no column at all, so every mapping entry those passes
/// produced was silently dropped from the CSV and from the reason table's TOTAL (measured
/// 2026-09-02: `rust-add-if` produced 81 mapping entries and recorded 79). Since the attribution
/// table is what "which pass owns this fixture's mismatches?" is answered from, a pass that fired
/// reading as absent is the worst shape that failure could take.
///
/// The first sixteen are kept in their original order, and the eight are appended rather than
/// slotted in, so existing column positions are unchanged for anything reading the CSV.
///
/// Seven entries below are dead - `Comment`, `BottomUp` and the five `Norm*` variants name passes
/// deleted in 2026-08-14/16 - and are retained deliberately: `matching_reasons_report.py` indexes
/// `Comment` and `BottomUp` by name, so dropping them is a separate, consumer-breaking change.
const NON_APTED_REASON_LABELS: &[&str] = &[
    "IdHash",
    "IdHashAnc",
    "FullMap",
    "StructId",
    "StructAnc",
    "OptIDU",
    "FlatSeq",
    "Moved",
    "Comment",
    "BottomUp",
    "GreedyAnchor",
    "NormImport",
    "NormNoPunct",
    "NormNoLit",
    "NormNoId",
    "NormNoPunctLit",
    // Appended 2026-09-02 - live passes that had no column until then.
    "LeadSib",
    "BottomUpProp",
    "UniqueType",
    "Unresolved",
    "MutualAnc",
    "CondCollapse",
    "HeritageGrowth",
    "WrapGrowth",
];

/// Column label for one `ASTMappingReason`. For every variant except `APTED` this is
/// `ASTMappingReason::bucket_label` - same abbreviations `src/bin/human_solver.rs`'s
/// `reason_label` uses for its own compact per-node display, shared via that one method so the
/// two can't drift.
///
/// `APTED` is the deliberate exception: it does *not* bucket into one "APTED" column the way
/// `bucket_label`/`human_solver.rs`'s `reason_label` do for their compact glyph suffix. Each
/// distinct provenance string (see that variant's doc comment - which pass actually invoked
/// APTED) gets its own column instead (`"APTED:final_pass"`, `"APTED:bottom_up_expansion"`, ...):
/// the whole point of tracking provenance was to see this breakdown in the one place built to
/// show it, and collapsing it back down here would defeat that. The column *set* is therefore
/// data-dependent (it's whatever provenances actually fired in this run), unlike every other,
/// fixed column - see `all_reason_columns`, which is what discovers it.
fn reason_column_label(reason: &ASTMappingReason) -> String {
    match reason {
        ASTMappingReason::APTED(source) => format!("APTED:{source}"),
        other => other.bucket_label().to_string(),
    }
}

/// Runs codediff once and tallies every mapping entry (matched pairs *and* lone deletes/inserts)
/// by its `ASTMappingReason` column label (see `reason_column_label`) - i.e. which algorithm pass
/// (and, for APTED, which call site) is responsible for how much of the diff. Independent of
/// `human_mapping.json`, so this works for "unsolved" fixtures too.
fn reason_counts_for(
    before: &Code,
    after: &Code,
    config: &codediff::diff::HeuristicConfig,
) -> HashMap<String, usize> {
    let diff = codediff::diff::diff_code_with_config(before, after, config);
    let mut counts = HashMap::new();
    if let Some(diff_ast) = diff.ast {
        for mapping in diff_ast.mapping.values() {
            *counts
                .entry(reason_column_label(&mapping.reason))
                .or_insert(0) += 1;
        }
    }
    counts
}

/// Every reason column that exists at all: the fixed `NON_APTED_REASON_LABELS` (all of them,
/// unconditionally - a column being zero for every fixture in the corpus doesn't mean the
/// `ASTMappingReason` variant it names stopped existing), followed by every distinct
/// `"APTED:<source>"` column observed across all rows, sorted alphabetically by provenance for a
/// deterministic, readable order. The APTED-family columns are appended rather than interleaved
/// back into their old fixed position: the set size is data-dependent (it's whichever provenances
/// actually fired), so there's no fixed slot to put them in the way the other, always-present
/// columns have - and unlike the fixed labels, an APTED column is never included "just in case"
/// (there's no way to enumerate a provenance string that never appeared in the data), so this list
/// is inherently already restricted to provenances that fired at least once somewhere.
///
/// Used for the CSV, which is meant to be a complete, stable-shaped record for downstream tooling
/// (e.g. `research/analysis/matching_reasons_report.py`) - see `active_reason_columns` for the
/// display-only variant that additionally drops always-zero columns.
fn all_reason_columns(rows: &[Row]) -> Vec<String> {
    let mut columns: Vec<String> = NON_APTED_REASON_LABELS
        .iter()
        .map(|label| label.to_string())
        .collect();

    let apted_columns: std::collections::BTreeSet<&String> = rows
        .iter()
        .flat_map(|r| r.reason_counts.keys())
        .filter(|label| label.starts_with("APTED:"))
        .collect();
    columns.extend(apted_columns.into_iter().cloned());
    columns
}

/// `all_reason_columns`, filtered down to columns that actually have a nonzero count somewhere in
/// `rows` - keeps the interactive terminal table as narrow as the data warrants. Not used for the
/// CSV (see `all_reason_columns`'s doc comment on why that stays complete/unfiltered).
fn active_reason_columns(rows: &[Row]) -> Vec<String> {
    all_reason_columns(rows)
        .into_iter()
        .filter(|label| {
            rows.iter()
                .any(|r| r.reason_counts.get(label).copied().unwrap_or(0) > 0)
        })
        .collect()
}

/// Total unit-cost of codediff's own mapping (see `codediff::diff::cost::diff_cost`) - independent
/// of `human_mapping.json`, so this works for "unsolved" fixtures too, same as `reason_counts_for`.
/// Runs `diff_code` a second time rather than sharing a run with `reason_counts_for`/
/// `human_mapping::compute_mismatches_for`: this binary already re-diffs per computation (see
/// `total_node_count_for`'s own re-walk), and a shared-run refactor isn't worth the complexity for
/// a benchmark tool that's run interactively, not in a hot loop.
fn algorithm_cost_for(
    before: &Code,
    after: &Code,
    config: &codediff::diff::HeuristicConfig,
) -> u64 {
    let diff = codediff::diff::diff_code_with_config(before, after, config);
    let Some(diff_ast) = diff.ast else {
        return 0;
    };
    let before_metadata = codediff::code::metadata::metadata_of(before);
    let after_metadata = codediff::code::metadata::metadata_of(after);
    diff_cost(&diff_ast, &before_metadata, &after_metadata)
}

/// Wall-clock time for one `diff_code_with_config` call, in milliseconds - a single-shot
/// measurement (no repeats/averaging, unlike `benchmark_other.rs`'s `--repeats`), kept as its own
/// isolated call rather than reusing `reason_counts_for`/`algorithm_cost_for`'s own separate
/// `diff_code_with_config` runs, matching this file's existing "each computation gets its own
/// independent diff_code call" convention (see `algorithm_cost_for`'s doc comment) - a shared-run
/// refactor would also need to reach into `compute_mismatches_for_with_config` (a third independent
/// call, in `human_mapping.rs`), more complexity than this benchmark tool's interactive,
/// not-hot-loop use case warrants. Independent of `human_mapping.json`, so this works for
/// "unsolved" fixtures too, same as `reason_counts_for`/`algorithm_cost_for`.
fn elapsed_ms_for(before: &Code, after: &Code, config: &codediff::diff::HeuristicConfig) -> f64 {
    let started = std::time::Instant::now();
    let _diff = codediff::diff::diff_code_with_config(before, after, config);
    started.elapsed().as_secs_f64() * 1000.0
}

#[derive(Parser)]
struct Args {
    /// Print every individual mismatch for this one fixture (including the operation and
    /// `ASTMappingReason` of the mapping codediff actually produced) instead of the table.
    #[arg(long)]
    details: Option<String>,

    /// Print codediff's complete mapping for this one fixture (every pair with paths, operation
    /// and reason) instead of the table - the raw material for debugging a mismatch.
    #[arg(long)]
    dump: Option<String>,

    /// Output results as a CSV file. Default path: "./research/data/quality/optimal_solutions_benchmark.csv"
    #[arg(long, value_name = "PATH", num_args = 0..=1)]
    csv: Option<Option<std::path::PathBuf>>,

    /// Compare this run against a per-fixture quality baseline and exit non-zero if any fixture
    /// regressed. This is the release gate - see the quality-gate section below for why it is
    /// per-fixture rather than one aggregate number.
    #[arg(long, value_name = "PATH")]
    compare: Option<std::path::PathBuf>,

    /// Write this run out as a new quality baseline. A deliberate step (`make
    /// update-quality-baseline`), never a side effect of a normal run.
    #[arg(long, value_name = "PATH")]
    write_baseline: Option<std::path::PathBuf>,

    /// Enable MoveDetectionRecovery / phase 7 (default; see the `--no-solver-...` form).
    #[arg(long = "solver-moved-subtrees", action = clap::ArgAction::SetTrue, default_value_t = true, overrides_with = "no_solver_moved_subtrees")]
    solver_moved_subtrees: bool,
    /// Disable `solve_moved_subtrees` (deleted+inserted identical-subtree move pairing).
    #[arg(long = "no-solver-moved-subtrees", action = clap::ArgAction::SetTrue, default_value_t = false, overrides_with = "solver_moved_subtrees")]
    no_solver_moved_subtrees: bool,

    /// Enable `solve_bottom_up_propagation` (phases-4-7 rearchitecture, `TODO.md`; default, since
    /// its own isolated corpus measurement came back clean - see `HeuristicConfig`'s doc comment).
    #[arg(long = "solver-bottom-up-propagation", action = clap::ArgAction::SetTrue, default_value_t = true, overrides_with = "no_solver_bottom_up_propagation")]
    solver_bottom_up_propagation: bool,
    /// Disable `solve_bottom_up_propagation`.
    #[arg(long = "no-solver-bottom-up-propagation", action = clap::ArgAction::SetTrue, default_value_t = false, overrides_with = "solver_bottom_up_propagation")]
    no_solver_bottom_up_propagation: bool,

    /// Enable `solve_unique_type_matching` (GumTree Simple's "unique type matching" recovery
    /// sub-phase - see `TODO.md`'s 2026-08-17 literature survey). Newly added, default `true`
    /// pending full-corpus measurement.
    #[arg(long = "solver-unique-type-matching", action = clap::ArgAction::SetTrue, default_value_t = true, overrides_with = "no_solver_unique_type_matching")]
    solver_unique_type_matching: bool,
    /// Disable `solve_unique_type_matching`.
    #[arg(long = "no-solver-unique-type-matching", action = clap::ArgAction::SetTrue, default_value_t = false, overrides_with = "solver_unique_type_matching")]
    no_solver_unique_type_matching: bool,

    /// Enable `solve_mutual_ancestors` (mutual lowest-common-ancestor container pairing).
    #[arg(long = "solver-mutual-ancestors", action = clap::ArgAction::SetTrue, default_value_t = true, overrides_with = "no_solver_mutual_ancestors")]
    solver_mutual_ancestors: bool,
    /// Disable `solve_mutual_ancestors`.
    #[arg(long = "no-solver-mutual-ancestors", action = clap::ArgAction::SetTrue, default_value_t = false, overrides_with = "solver_mutual_ancestors")]
    no_solver_mutual_ancestors: bool,
}

/// Resolves `Args`' `--solver-X`/`--no-solver-X` pairs into a `HeuristicConfig` - `--no-solver-X`
/// wins whenever the two disagree at the end of parsing (see the flag pair's `overrides_with`,
/// verified against clap's actual last-flag-wins behavior).
fn config_from_args(args: &Args) -> codediff::diff::HeuristicConfig {
    codediff::diff::HeuristicConfig {
        solver_moved_subtrees: args.solver_moved_subtrees && !args.no_solver_moved_subtrees,
        solver_bottom_up_propagation: args.solver_bottom_up_propagation
            && !args.no_solver_bottom_up_propagation,
        solver_unique_type_matching: args.solver_unique_type_matching
            && !args.no_solver_unique_type_matching,
        solver_mutual_ancestors: args.solver_mutual_ancestors && !args.no_solver_mutual_ancestors,
    }
}

struct Row {
    name: String,
    /// `None` means there's no `human_mapping.json` for this fixture yet (unsolved), as opposed
    /// to `Some((0, _))`, which means codediff matches the human mapping exactly.
    ///
    /// The second element of the tuple is the total node count (before + after trees combined),
    /// the denominator for the mismatch percentage - see `human_mapping::total_node_count_for`.
    mismatches: Option<(usize, usize)>,
    /// How many mapping entries codediff produced for each `ASTMappingReason` column label (see
    /// `reason_column_label`) - i.e. which pass (hash matching, semantic-structural anchoring,
    /// APTED, ...) did how much of the work. Computed unconditionally (doesn't need
    /// `human_mapping.json`), so this is populated even for "unsolved" fixtures.
    reason_counts: HashMap<String, usize>,
    /// Total unit-cost of codediff's own mapping (`codediff::diff::cost::diff_cost`). Computed
    /// unconditionally, same as `reason_counts` - this is "how expensive codediff's mapping is",
    /// independent of whether there's a human mapping to compare it against.
    algorithm_cost: u64,
    /// Total unit-cost of the human-authored mapping (`human_mapping::human_mapping_cost_for`),
    /// under the exact same cost model as `algorithm_cost` so the two are directly comparable.
    /// `None` for "unsolved" fixtures (no `human_mapping.json` yet), same convention as
    /// `mismatches`.
    human_cost: Option<u64>,
    /// Wall-clock time for one `diff_code_with_config` call (see `elapsed_ms_for`), in
    /// milliseconds. Computed unconditionally, same as `reason_counts`/`algorithm_cost` - this is
    /// "how long codediff took on this fixture", independent of whether there's a human mapping to
    /// compare it against.
    elapsed_ms: f64,
    /// How many of `mismatches`' nodes are *visible* - carry text of their own, per
    /// `codediff::diff::nodes::is_structurally_visible` - vs. sitting on pure structural
    /// scaffolding (a `block`, a `declaration_list`, ...) whose every readable byte belongs to
    /// some descendant. The second element is the total visible-node count (before + after combined),
    /// the denominator for the visible-mismatch percentage - same `(count, denominator)` shape as
    /// `mismatches`, and `None` under the same "unsolved" convention.
    visible_mismatches: Option<(usize, usize)>,
    /// How many node slots this fixture's human mapping actually grades, in the same unit as
    /// `mismatches`' denominator - see `human_mapping::graded_node_count`. `None` for an unsolved
    /// fixture, same convention as the fields above.
    ///
    /// Reported because the mismatch percentage divides by *every* node in both trees while a
    /// mismatch can only arise on a graded one, so a thinly-annotated fixture reports a rate far
    /// below what it actually demonstrates. Without this column a 0.01% is unreadable: it may mean
    /// "nearly perfect" or "0.3% of the tree was ever checked."
    graded_nodes: Option<usize>,
}

/// Prints every mapping codediff produces for one fixture, with human-readable paths, sorted by
/// the before path (inserts, having none, sort last).
fn dump_mapping(name: &str, config: &codediff::diff::HeuristicConfig) -> Result<()> {
    use codediff::test::helper::path_for_node;

    // Per-name, not the full-corpus map: this dumps one fixture, so parsing all 500+ to reach one
    // of them is ~5.5GB and ~18s of pure waste (see the streaming comment in `main`). Used by
    // reference rather than cloned, which also preserves the `ast_metadata` that
    // `code_pair_from_dir` already computed - `Code`'s hand-written `Clone` drops it to `None`
    // (see its doc comment), which would silently make every `metadata_of` call below recompute.
    let pair = helper::handmade_test_code_pair(name)?;
    let (before, after) = (&pair.0, &pair.1);

    let diff = codediff::diff::diff_code_with_config(before, after, config);
    let ast = diff.ast.expect("diff has AST");

    let before_ast = before.ast.as_ref().expect("before parsed");
    let after_ast = after.ast.as_ref().expect("after parsed");

    // node id -> path string, for both trees.
    let mut paths: std::collections::HashMap<usize, String> = std::collections::HashMap::new();
    for root in [before_ast.root_node(), after_ast.root_node()] {
        let mut stack = vec![root];
        while let Some(n) = stack.pop() {
            paths.insert(n.id(), path_for_node(n).join("/"));
            let mut cursor = n.walk();
            for child in n.children(&mut cursor) {
                stack.push(child);
            }
        }
    }

    let mut lines: Vec<String> = ast
        .mapping
        .iter()
        .map(|(&(b, a), m)| {
            let bp = if b == 0 {
                "-"
            } else {
                paths.get(&b).map(String::as_str).unwrap_or("?")
            };
            let ap = if a == 0 {
                "-"
            } else {
                paths.get(&a).map(String::as_str).unwrap_or("?")
            };
            format!(
                "{:?} ({:?})\n    B {}\n    A {}",
                m.operation, m.reason, bp, ap
            )
        })
        .collect();
    lines.sort();
    for line in lines {
        println!("{}", line);
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let config = config_from_args(&args);

    if let Some(name) = args.details {
        if !human_mapping::mapping_path(&name).exists() {
            bail!("fixture '{}' has no human_mapping.json", name);
        }
        let visible = human_mapping::compute_visible_mismatches_with_config(&name, &config)?;
        let total = visible.visible.len() + visible.invisible.len();
        println!(
            "{}: {} mismatch(es) ({} visible / {} invisible, out of {} visible nodes)",
            name,
            total,
            visible.visible.len(),
            visible.invisible.len(),
            visible.before_visible_node_count + visible.after_visible_node_count,
        );
        for m in &visible.visible {
            println!("  [visible]   {}", m.message);
        }
        for m in &visible.invisible {
            println!("  [invisible] {}", m.message);
        }
        return Ok(());
    }

    if let Some(name) = args.dump {
        return dump_mapping(&name, &config);
    }

    // Streamed one fixture at a time, deliberately *not* via `handmade_test_code_pairs`: this
    // binary visits every fixture exactly once, in order, so the full-corpus cache buys it
    // nothing and costs it the whole corpus resident at once - all 500+ fixtures with a parsed
    // `tree_sitter::Tree` and its `ast_metadata` per side. Measured 2026-09-02: RSS climbed to
    // 5564MB over the first 18 seconds and then sat flat there for the remaining ~130s of
    // measurement, i.e. that memory was retention, not working set, against a 7GB limit on a
    // standard CI runner. Loading per fixture and dropping it at the end of each iteration makes
    // the peak the largest single fixture instead of the sum of all of them.
    //
    // `code_pair_from_dir` calls `ensure_parsed` itself, so what it hands back already carries
    // the `ast_metadata` the timed diff below needs. That is also why this no longer clones:
    // cloning was only ever needed because the cache handed out shared `Code`s whose
    // hand-written `Clone` drops `ast_metadata` to `None` (see its doc comment), which without a
    // re-`ensure_parsed` here silently turned every `metadata_of` call into a full whole-tree
    // recompute (measured 2026-08-17: ~26 recomputes per `diff_code_with_config` call, ~6s on
    // the largest fixture, inflating `elapsed_ms` ~6x over the production path). Owning a
    // freshly-loaded pair sidesteps that entirely and matches what `Code::from_file` does in
    // production.
    let cases = helper::handmade_test_case_dirs()?;

    let started = std::time::Instant::now();
    // Fixture load+parse is timed separately and subtracted from the reported figure below. Before
    // this binary streamed the corpus, the whole corpus was loaded *before* `started`, so the
    // reported runtime covered scoring only; loading per fixture moved that cost inside the timed
    // span and silently changed what the number meant (measured 2026-09-02: 294.5 -> 302.7
    // ms/fixture across the change, while total wall clock actually fell). Subtracting it keeps
    // the figure comparable to every value recorded before the switch.
    let mut load_time = std::time::Duration::ZERO;
    let mut rows = Vec::with_capacity(cases.len());
    for (name, dir) in &cases {
        let load_started = std::time::Instant::now();
        let pair = helper::code_pair_from_dir(dir)?;
        load_time += load_started.elapsed();
        let Some((before, after)) = pair else {
            continue;
        };
        let (before, after) = (&before, &after);
        let reason_counts = reason_counts_for(before, after, &config);
        let algorithm_cost = algorithm_cost_for(before, after, &config);
        let elapsed_ms = elapsed_ms_for(before, after, &config);

        if !human_mapping::mapping_path(name).exists() {
            rows.push(Row {
                name: name.clone(),
                mismatches: None,
                reason_counts,
                algorithm_cost,
                human_cost: None,
                elapsed_ms,
                visible_mismatches: None,
                graded_nodes: None,
            });
            continue;
        }
        let visible = human_mapping::compute_visible_mismatches_for_with_config(
            name, before, after, &config,
        )?;
        let mismatch_count = visible.visible.len() + visible.invisible.len();
        let total_nodes = human_mapping::total_node_count_for(before, after);
        let human_cost = human_mapping::human_mapping_cost_for(name, before, after)?;
        let graded_nodes = human_mapping::graded_node_count_for(name, before, after)?;
        rows.push(Row {
            name: name.clone(),
            mismatches: Some((mismatch_count, total_nodes)),
            reason_counts,
            algorithm_cost,
            human_cost: Some(human_cost),
            elapsed_ms,
            visible_mismatches: Some((
                visible.visible.len(),
                visible.before_visible_node_count + visible.after_visible_node_count,
            )),
            graded_nodes: Some(graded_nodes),
        });
    }

    // Worst offenders first, so regressions/improvements are the first thing visible; unsolved
    // fixtures (nothing to compare against yet) sort after every solved one.
    rows.sort_by(|a, b| match (a.mismatches, b.mismatches) {
        (Some((x, _)), Some((y, _))) => y.cmp(&x).then_with(|| a.name.cmp(&b.name)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.name.cmp(&b.name),
    });

    let elapsed = started.elapsed().saturating_sub(load_time);

    if let Some(csv_path) = args.csv {
        let path = csv_path.unwrap_or_else(|| {
            std::path::PathBuf::from("./research/data/quality/optimal_solutions_benchmark.csv")
        });
        write_csv(&rows, &path)?;
    }

    if let Some(path) = &args.write_baseline {
        write_baseline(&rows, path)?;
        println!("Wrote quality baseline to {path:?}");
    }

    print_table(&rows);
    print_reason_table(&rows);
    print_goal_progress(&rows);
    println!(
        "\nRuntime: {:.3}s total, {:.1}ms/fixture ({} fixtures)",
        elapsed.as_secs_f64(),
        elapsed.as_secs_f64() * 1000.0 / rows.len() as f64,
        rows.len()
    );

    // Last, and after everything else has printed: a failing gate should still leave the full
    // table on screen, since the first thing anyone does with a red gate is look at the table.
    if let Some(path) = &args.compare {
        let baseline = read_baseline(path)?;
        let report = compare_to_baseline(&rows, &baseline);
        print_gate_report(&report, path);
        if report.failed() {
            std::process::exit(1);
        }
    }
    Ok(())
}

/// The second accuracy goal's per-fixture ceiling: at most this fraction of a fixture's *visible*
/// nodes may disagree with the human mapping.
///
/// 1% against the structural visible set (`nodes::is_structurally_visible`), which is ~68% of all
/// nodes - so this allows roughly 19 visible mismatches on a median fixture's 1,875 visible nodes,
/// a genuine relaxation of the 90%-at-exactly-zero tier rather than a restatement of it. The
/// threshold has moved twice and the reason is worth keeping: it was briefly 4%, chosen while
/// visibility was renderer-derived and the denominator was ~3.4% of nodes, where anything tighter
/// collapsed into "exactly zero" for most of the corpus. Once visibility became structural that
/// denominator grew ~20x and 4% stopped discriminating - it passed 98.3% of fixtures and only
/// caught the extreme tail. See `src/diff/TODO.md` item 0 for the full table.
const VISIBLE_RATE_GOAL: f64 = 0.01;

/// Progress against the project's two accuracy goals, both stated in *visible* nodes - see the
/// README's "Accurate" principle and `src/diff/TODO.md`. Printed after the tables so the number a
/// change is actually trying to move is the last thing on screen, rather than something a reader
/// has to recompute from the CSV.
///
/// Only "solved" fixtures count, the same scoping the tables use: a fixture with no
/// `human_mapping.json` has nothing to be right or wrong about.
fn print_goal_progress(rows: &[Row]) {
    let scored: Vec<(usize, usize)> = rows.iter().filter_map(|r| r.visible_mismatches).collect();
    if scored.is_empty() {
        return;
    }

    let total = scored.len();
    let zero = scored.iter().filter(|(count, _)| *count == 0).count();
    // A fixture with no visible nodes at all has nothing to get wrong, so it clears the rate bar.
    let within = scored
        .iter()
        .filter(|(count, nodes)| {
            *nodes == 0 || (*count as f64) / (*nodes as f64) <= VISIBLE_RATE_GOAL
        })
        .count();

    let goal = |have: usize, target_pct: usize| {
        let need = (total * target_pct).div_ceil(100);
        (
            100.0 * have as f64 / total as f64,
            need,
            need.saturating_sub(have),
        )
    };
    let (zero_pct, zero_need, zero_gap) = goal(zero, 90);
    let (within_pct, within_need, within_gap) = goal(within, 99);

    println!("\nAccuracy goals (visible nodes, {total} solved fixtures):");
    println!(
        "  zero visible mismatches   {zero:>4}/{total}  {zero_pct:>5.1}%  (goal 90% = {zero_need}, {} to go)",
        zero_gap
    );
    println!(
        "  within {:.0}% visible         {within:>4}/{total}  {within_pct:>5.1}%  (goal 99% = {within_need}, {} to go)",
        VISIBLE_RATE_GOAL * 100.0,
        within_gap
    );

    // The rate goal above is only as meaningful as the annotation it divides into: a mismatch can
    // only arise on a node the human mapping grades, but the denominator is every node in both
    // trees, so a thinly-annotated fixture clears the bar for free (see
    // `human_mapping::graded_node_count`). Reported as a caveat on the goal rather than a
    // correction to it, because the fix is to finish annotating those fixtures, not to rescale the
    // metric - and a silent free pass is the failure mode worth surfacing.
    let thin = rows
        .iter()
        .filter_map(|r| match (r.mismatches, r.graded_nodes) {
            (Some((_, total_nodes)), Some(graded)) if total_nodes > 0 => {
                Some((graded as f64) / (total_nodes as f64))
            }
            _ => None,
        })
        .filter(|coverage| *coverage < THIN_ANNOTATION_COVERAGE)
        .count();
    if thin > 0 {
        println!(
            "  note: {thin} fixture(s) grade under {:.0}% of their nodes - they clear the rate bar \
             largely by not being annotated, not by being diffed well (see the graded_nodes column)",
            THIN_ANNOTATION_COVERAGE * 100.0
        );
    }
}

/// Below this share of its nodes graded by the human mapping, a fixture's mismatch *rate* says
/// more about how much of it was annotated than about how well it was diffed, so
/// `print_goal_progress` calls it out rather than letting it silently pad the goal. Set at 90%
/// because that is where the corpus actually separates: on 2026-09-02, 42 of 512 fixtures fell
/// below it and the rest sat far above.
const THIN_ANNOTATION_COVERAGE: f64 = 0.9;

fn print_table(rows: &[Row]) {
    let name_width = rows
        .iter()
        .map(|r| r.name.len())
        .chain(["Solution".len()])
        .max()
        .unwrap_or(0);

    println!(
        "{:<name_width$}  {:>10}  {:>7}  {:>9}  {:>7}  {:>13}  {:>9}  {:>9}  {:>9}  {:>12}",
        "Solution",
        "Mismatches",
        "Mism %",
        "Vis Mism",
        "Vis %",
        "Human Unsolved",
        "Alg Cost",
        "Hum Cost",
        "Cost Diff",
        "Elapsed(ms)",
        name_width = name_width
    );
    println!(
        "{}",
        "-".repeat(
            name_width + 2 + 10 + 2 + 7 + 2 + 9 + 2 + 7 + 2 + 13 + 2 + 9 + 2 + 9 + 2 + 9 + 2 + 12
        )
    );

    let mut total_mismatches = 0usize;
    let mut total_nodes = 0usize;
    let mut total_visible_mismatches = 0usize;
    let mut total_visible_nodes = 0usize;
    let mut total_unsolved = 0usize;
    // Only summed over fixtures that also have a human cost, so `total_cost_diff` below compares
    // like for like - an "unsolved" fixture's algorithm cost would otherwise inflate the TOTAL
    // algorithm side against nothing on the human side. This is also the figure the TOTAL row
    // *prints*: it used to print an all-rows sum beside a solved-only human cost and a solved-only
    // difference, so `Alg Cost - Hum Cost` did not equal the `Cost Diff` column next to it (off by
    // 9819 on the 2026-09-02 corpus, exactly the one unsolved fixture's algorithm cost, and
    // growing with every unsolved fixture added).
    let mut total_algorithm_cost_where_solved = 0u64;
    let mut total_human_cost = 0u64;
    let mut total_elapsed_ms = 0.0f64;
    for row in rows {
        total_elapsed_ms += row.elapsed_ms;
        match (row.mismatches, row.human_cost) {
            (Some((count, nodes)), Some(human_cost)) => {
                total_mismatches += count;
                total_nodes += nodes;
                total_algorithm_cost_where_solved += row.algorithm_cost;
                total_human_cost += human_cost;
                let pct = if nodes > 0 {
                    100.0 * count as f64 / nodes as f64
                } else {
                    0.0
                };
                let (visible_count, visible_nodes) = row.visible_mismatches.unwrap_or((0, 0));
                total_visible_mismatches += visible_count;
                total_visible_nodes += visible_nodes;
                let visible_pct = if visible_nodes > 0 {
                    100.0 * visible_count as f64 / visible_nodes as f64
                } else {
                    0.0
                };
                let cost_diff = row.algorithm_cost as i64 - human_cost as i64;
                println!(
                    "{:<name_width$}  {:>10}  {:>6.2}%  {:>9}  {:>6.2}%  {:>13}  {:>9}  {:>9}  {:>+9}  {:>12.1}",
                    row.name,
                    count,
                    pct,
                    visible_count,
                    visible_pct,
                    "",
                    row.algorithm_cost,
                    human_cost,
                    cost_diff,
                    row.elapsed_ms,
                    name_width = name_width
                );
            }
            _ => {
                total_unsolved += 1;
                println!(
                    "{:<name_width$}  {:>10}  {:>7}  {:>9}  {:>7}  {:>13}  {:>9}  {:>9}  {:>9}  {:>12.1}",
                    row.name,
                    "-",
                    "-",
                    "-",
                    "-",
                    "yes",
                    row.algorithm_cost,
                    "-",
                    "-",
                    row.elapsed_ms,
                    name_width = name_width
                );
            }
        }
    }

    println!(
        "{}",
        "-".repeat(
            name_width + 2 + 10 + 2 + 7 + 2 + 9 + 2 + 7 + 2 + 13 + 2 + 9 + 2 + 9 + 2 + 9 + 2 + 12
        )
    );
    let total_pct = if total_nodes > 0 {
        100.0 * total_mismatches as f64 / total_nodes as f64
    } else {
        0.0
    };
    let total_visible_pct = if total_visible_nodes > 0 {
        100.0 * total_visible_mismatches as f64 / total_visible_nodes as f64
    } else {
        0.0
    };
    let total_cost_diff = total_algorithm_cost_where_solved as i64 - total_human_cost as i64;
    println!(
        "{:<name_width$}  {:>10}  {:>6.2}%  {:>9}  {:>6.2}%  {:>13}  {:>9}  {:>9}  {:>+9}  {:>12.1}",
        "TOTAL",
        total_mismatches,
        total_pct,
        total_visible_mismatches,
        total_visible_pct,
        total_unsolved,
        total_algorithm_cost_where_solved,
        total_human_cost,
        total_cost_diff,
        total_elapsed_ms,
        name_width = name_width
    );
}

/// Prints a fixture x reason table: how many mapping entries each algorithm pass (hash matching,
/// semantic-structural anchoring, APTED, ...) produced for each fixture, plus a TOTAL row.
/// Reasons that are zero for every fixture are dropped from the table to keep it narrower - the
/// active set varies run to run depending on which passes actually fire.
fn print_reason_table(rows: &[Row]) {
    let name_width = rows
        .iter()
        .map(|r| r.name.len())
        .chain(["Solution".len()])
        .max()
        .unwrap_or(0);

    let active_reasons = active_reason_columns(rows);
    // Column widths vary now: an `APTED:<source>` label (e.g. "APTED:bottom_up_expansion") can be
    // much longer than the old fixed 9-char budget, and a fixed width would misalign the table
    // the moment one appears.
    const MIN_COL_WIDTH: usize = 9;
    let col_widths: Vec<usize> = active_reasons
        .iter()
        .map(|label| label.len().max(MIN_COL_WIDTH))
        .collect();
    let rule_width = name_width + col_widths.iter().map(|w| w + 2).sum::<usize>();

    println!();
    println!("Mapping reasons per fixture (how much work each algorithm pass did):");
    print!("{:<name_width$}", "Solution", name_width = name_width);
    for (label, width) in active_reasons.iter().zip(&col_widths) {
        print!("  {:>width$}", label, width = width);
    }
    println!();
    println!("{}", "-".repeat(rule_width));

    let mut totals: HashMap<&str, usize> = HashMap::new();
    for row in rows {
        print!("{:<name_width$}", row.name, name_width = name_width);
        for (label, width) in active_reasons.iter().zip(&col_widths) {
            let count = row.reason_counts.get(label).copied().unwrap_or(0);
            *totals.entry(label.as_str()).or_insert(0) += count;
            print!("  {:>width$}", count, width = width);
        }
        println!();
    }

    println!("{}", "-".repeat(rule_width));
    print!("{:<name_width$}", "TOTAL", name_width = name_width);
    for (label, width) in active_reasons.iter().zip(&col_widths) {
        print!(
            "  {:>width$}",
            totals.get(label.as_str()).copied().unwrap_or(0),
            width = width
        );
    }
    println!();
}

// ─── The quality gate ────────────────────────────────────────────────────────────────────────
//
// `make check-quality` runs this, and `make deploy` runs that, so what follows decides whether a
// release may go out.
//
// **Per fixture, not in aggregate, and that is the whole point.** The gate used to compare one
// number - the corpus's total mismatch count - against a checked-in baseline. That cannot
// distinguish the two things it needs to tell apart. This corpus grows deliberately toward *hard*
// cases: the 35 fixtures added over three days in August 2026 carried mismatches at 0.44% of their
// nodes against the corpus's own 0.07%, taking the total from 3235 to 6473. The gate read that as
// a 100% quality regression when the algorithm had not changed at all. Switching the aggregate to
// a rate does not fix it either - measured on the same corpus change, the rate went 0.0657% ->
// 0.1142%, a 74% rise. No aggregate over a growing corpus can separate "the algorithm got worse"
// from "we added hard fixtures", so the gate asks the only question that survives new data: did
// any fixture *that already had a baseline* get worse?
//
// **Not a second copy of the `optimal_solutions` tests.** Those clamp each fixture at a recorded
// value, and 151 of the corpus's 509 are clamped above zero - by construction they cannot see a
// fixture drift from 100 mismatches to 214 under its own 214-mismatch clamp. This is what covers
// that gap.

/// One fixture's row in the gate baseline.
#[derive(Debug, Clone, Copy, PartialEq)]
struct BaselineEntry {
    mismatches: usize,
    visible_mismatches: usize,
    /// Wall-clock milliseconds for this fixture's diff. Carried alongside the accuracy numbers but
    /// deliberately **not** gated on - see [`print_gate_report`]'s latency section for the measured
    /// reason: run to run on one idle machine, a fixture's own time moves by up to 4.9x while the
    /// run's aggregate shape moves by 3%. There is no per-fixture threshold that catches a real
    /// slowdown without also firing on noise.
    elapsed_ms: f64,
}

/// What one run says about one fixture, relative to the baseline.
#[derive(Debug, Clone)]
struct GateChange {
    name: String,
    before: BaselineEntry,
    after: BaselineEntry,
}

/// The result of comparing a run against a baseline.
#[derive(Debug, Default)]
struct GateReport {
    regressed: Vec<GateChange>,
    improved: Vec<GateChange>,
    /// Fixtures in the run with no baseline row. Never a failure: a fixture cannot regress before
    /// it has a baseline, and failing on new data is exactly the behaviour this gate replaced.
    added: Vec<String>,
    /// Baseline rows with no fixture in the run. Also never a failure - a fixture can legitimately
    /// be renamed or dropped - but always printed, because deleting an inconvenient fixture would
    /// otherwise be a silent way to pass.
    removed: Vec<String>,
    /// Fixtures with no `human_mapping.json`, in neither the run's comparison nor the baseline.
    unsolved: usize,
    /// Every fixture present in both, for the latency section. Separate from `regressed`/`improved`
    /// because latency is reported and never gated - see [`print_gate_report`].
    latency: Vec<GateChange>,
    /// How many of the baseline's fixtures this run actually scored. The denominator for
    /// [`GateReport::lost_too_much_of_the_corpus`], which is the only thing it exists for.
    covered: usize,
}

impl GateReport {
    fn failed(&self) -> bool {
        !self.regressed.is_empty() || self.lost_too_much_of_the_corpus()
    }

    /// Did this run cover so little of the baseline that its verdict is meaningless?
    ///
    /// `removed` on its own is deliberately not a failure - a fixture can legitimately be renamed
    /// or dropped - but "not a failure per fixture" silently became "no lower bound at all": every
    /// path that loses fixtures is quiet. A dataset directory that fails to check out is skipped
    /// without a word (`helper::handmade_test_case_dirs`), and so is any fixture missing its
    /// `before.*.test` (`code_pair_from_dir` returns `Ok(None)`). Lose `diffs/full/` and 232 of 512
    /// baseline rows move to `removed`, `regressed` is empty, and the gate passes green over a
    /// corpus less than half its size. At zero rows it is worse still: the runtime figure divides
    /// by zero, the Makefile's `grep` finds no `ms/fixture` to warn about, and an empty run reports
    /// success in every channel it has.
    ///
    /// Proportional rather than any-missing, so that deleting one fixture stays the routine,
    /// non-gating operation it is meant to be, while losing a whole dataset cannot pass.
    fn lost_too_much_of_the_corpus(&self) -> bool {
        let baseline_size = self.covered + self.removed.len();
        // A share is not a meaningful quantity over a handful of rows: on a two-fixture baseline,
        // dropping the one fixture that is legitimately allowed to go missing is a 50% loss. The
        // floor is aimed at "a dataset directory failed to check out" on a corpus of hundreds, so
        // it stays out of the way below a size where a proportion means anything - see
        // `a_fixture_missing_from_the_run_is_named_rather_than_silently_ignored`, which encodes
        // the behaviour this must not break.
        //
        // Guarding on the *baseline's* size rather than the run's keeps the case that matters
        // most: a run that scores nothing at all against a real baseline is still 0% covered, and
        // still fails.
        if baseline_size < MIN_BASELINE_FOR_COVERAGE_CHECK {
            return false;
        }
        (self.covered as f64) / (baseline_size as f64) < MIN_BASELINE_COVERAGE
    }
}

/// A run must still score at least this share of the fixtures its baseline names, or
/// [`GateReport::failed`] fails it regardless of whether anything regressed - see
/// [`GateReport::lost_too_much_of_the_corpus`]. Set well below any plausible intentional churn
/// (the corpus grows by a handful of fixtures at a time and rarely shrinks at all) and well above
/// the "lost an entire dataset directory" case that motivates it: the smallest of the four
/// datasets is 61 of 513 fixtures, i.e. a 12% loss.
const MIN_BASELINE_COVERAGE: f64 = 0.95;

/// Baselines smaller than this are exempt from [`MIN_BASELINE_COVERAGE`] entirely: a percentage of
/// a handful of fixtures says nothing, and every such baseline in practice is a unit test's. The
/// real corpus is two orders of magnitude above this.
const MIN_BASELINE_FOR_COVERAGE_CHECK: usize = 20;

/// The scored fixtures of a run, in baseline form. Fixtures with no human mapping are skipped:
/// there is nothing to be right or wrong about yet.
fn baseline_from_rows(rows: &[Row]) -> BTreeMap<String, BaselineEntry> {
    rows.iter()
        .filter_map(|row| {
            let (mismatches, _) = row.mismatches?;
            let (visible_mismatches, _) = row.visible_mismatches?;
            Some((
                row.name.clone(),
                BaselineEntry {
                    mismatches,
                    visible_mismatches,
                    elapsed_ms: row.elapsed_ms,
                },
            ))
        })
        .collect()
}

fn read_baseline(path: &std::path::Path) -> Result<BTreeMap<String, BaselineEntry>> {
    let mut reader = csv::Reader::from_path(path)
        .with_context(|| format!("reading the quality baseline from {path:?}"))?;
    let mut out = BTreeMap::new();
    for record in reader.deserialize::<HashMap<String, String>>() {
        let record = record.context("parsing a quality-baseline row")?;
        let field = |key: &str| -> Result<usize> {
            record
                .get(key)
                .with_context(|| format!("the baseline has no '{key}' column"))?
                .trim()
                .parse()
                .with_context(|| format!("the baseline's '{key}' column is not a number"))
        };
        // `.trim()`, like every numeric field above. Without it a hand-edited row carrying a
        // stray space (`rust-add-if , 4, 2`) keys as `"rust-add-if "`, so the live fixture finds
        // no baseline and is exempted as "new" while the stale key is exempted as "removed" -
        // both non-gating by design, so that fixture could regress without limit at exit 0, and
        // nothing in either list would show the two are the same fixture.
        let name = record
            .get("solution")
            .context("the baseline has no 'solution' column")?
            .trim()
            .to_string();
        out.insert(
            name,
            BaselineEntry {
                mismatches: field("mismatches")?,
                visible_mismatches: field("visible_mismatches")?,
                // Absent in a baseline written before latency was recorded: 0.0 reads as "no
                // previous timing", and the latency section below skips a fixture without one
                // rather than reporting an infinite speedup.
                elapsed_ms: record
                    .get("elapsed_ms")
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(0.0),
            },
        );
    }
    Ok(out)
}

fn write_baseline(rows: &[Row], path: &std::path::Path) -> Result<()> {
    let mut writer = Writer::from_writer(
        File::create(path).with_context(|| format!("writing the quality baseline to {path:?}"))?,
    );
    writer.write_record(["solution", "mismatches", "visible_mismatches", "elapsed_ms"])?;
    for (name, entry) in baseline_from_rows(rows) {
        writer.write_record([
            name,
            entry.mismatches.to_string(),
            entry.visible_mismatches.to_string(),
            format!("{:.2}", entry.elapsed_ms),
        ])?;
    }
    writer.flush()?;
    Ok(())
}

/// Compares a run against a baseline.
///
/// **Both columns gate.** They move independently - a change can leave a fixture's total flat
/// while turning invisible scaffolding mismatches into ones a reader can see, which is a
/// regression in the thing the project's accuracy goals are actually stated in. Either going up is
/// a failure.
fn compare_to_baseline(rows: &[Row], baseline: &BTreeMap<String, BaselineEntry>) -> GateReport {
    let current = baseline_from_rows(rows);
    let mut report = GateReport {
        unsolved: rows.len() - current.len(),
        ..Default::default()
    };

    for (name, after) in &current {
        let Some(before) = baseline.get(name) else {
            report.added.push(name.clone());
            continue;
        };
        report.covered += 1;
        let change = GateChange {
            name: name.clone(),
            before: *before,
            after: *after,
        };
        if after.mismatches > before.mismatches
            || after.visible_mismatches > before.visible_mismatches
        {
            report.regressed.push(change);
        } else if (after.mismatches, after.visible_mismatches)
            != (before.mismatches, before.visible_mismatches)
        {
            report.improved.push(change);
        }
        // Latency moved or not, it is never accuracy - it is reported separately below.
        report.latency.push(GateChange {
            name: name.clone(),
            before: *before,
            after: *after,
        });
    }
    for name in baseline.keys() {
        if !current.contains_key(name) {
            report.removed.push(name.clone());
        }
    }
    report
}

fn print_gate_report(report: &GateReport, baseline_path: &std::path::Path) {
    println!("\nQuality gate (per fixture, against {baseline_path:?})");
    println!(
        "  {:4} regressed   {:4} improved   {:4} new   {:4} removed   {:4} unsolved",
        report.regressed.len(),
        report.improved.len(),
        report.added.len(),
        report.removed.len(),
        report.unsolved,
    );

    let describe = |change: &GateChange| {
        format!(
            "    {:<80} {:>6} -> {:<6}  visible {:>6} -> {}",
            change.name,
            change.before.mismatches,
            change.after.mismatches,
            change.before.visible_mismatches,
            change.after.visible_mismatches,
        )
    };
    if !report.regressed.is_empty() {
        println!("\n  REGRESSED - these fixtures got worse than their baseline:");
        for change in &report.regressed {
            println!("{}", describe(change));
        }
    }
    if !report.improved.is_empty() {
        println!("\n  Improved:");
        for change in &report.improved {
            println!("{}", describe(change));
        }
    }
    // New and removed are listed rather than counted, so neither can pass unnoticed: a new fixture
    // is exempt from the gate by design, and a removed one is how somebody could lower the bar
    // without any number moving.
    if !report.added.is_empty() {
        println!("\n  New since the baseline (not gated):");
        for name in &report.added {
            println!("    {name}");
        }
    }
    if !report.removed.is_empty() {
        println!("\n  In the baseline but not in this run (not gated):");
        for name in &report.removed {
            println!("    {name}");
        }
    }

    print_latency_report(&report.latency);

    if report.lost_too_much_of_the_corpus() {
        // Reported before, and separately from, any regression list: a run this incomplete has no
        // verdict to give, and reading it as "N fixtures regressed" would send someone hunting an
        // algorithm change when the corpus itself is what went missing.
        println!(
            "\nerror: this run scored only {} of the baseline's {} fixtures ({:.1}%, floor is \
             {:.0}%). A missing dataset directory or an unreadable fixture is skipped silently, so \
             an incomplete corpus would otherwise pass green - the gate cannot certify fixtures it \
             never ran. Check that src/test/data/diffs/ is fully checked out.",
            report.covered,
            report.covered + report.removed.len(),
            100.0 * report.covered as f64 / (report.covered + report.removed.len()).max(1) as f64,
            MIN_BASELINE_COVERAGE * 100.0,
        );
    }
    if !report.regressed.is_empty() {
        println!(
            "\nerror: {} fixture(s) regressed against the baseline. Fix the regression, or - if \
             this is a reviewed, deliberate trade - re-baseline with `make \
             update-quality-baseline`.",
            report.regressed.len()
        );
    }
    if !report.failed() {
        println!("\nQuality gate passed: no fixture is worse than its baseline.");
    }
}

/// A fixture has to be at least this slow before a change in its own time means anything. Measured
/// 2026-08-28 by running the whole corpus twice on one idle machine: with no floor, the worst
/// run-to-run swing on a single fixture is 4.9x; above 20ms it is 1.66x, and above 50ms 1.53x. The
/// noise lives entirely in the fast fixtures, where a millisecond of scheduling is the whole
/// measurement.
const LATENCY_FLOOR_MS: f64 = 20.0;

/// And it has to move by at least this much. p99 of the run-to-run swing above the floor is 1.55x,
/// so 2x is the first threshold that is not mostly noise.
const LATENCY_FACTOR: f64 = 2.0;

/// Latency against the baseline - **reported, never gated**, and the two thresholds above are why.
///
/// Accuracy is algorithm-only and reproduces exactly, which is what makes the gate above safe to
/// fail on. Wall-clock does not: on one idle machine, a single fixture's own time moves run to run
/// by up to 4.9x, while the run's *aggregate* shape moves by about 3%. So the aggregate is the
/// trustworthy signal and is always printed; individual fixtures are only named when they clear
/// both a floor and a factor, and even then as something to look at rather than something failed.
fn print_latency_report(changes: &[GateChange]) {
    let timed: Vec<&GateChange> = changes
        .iter()
        .filter(|c| c.before.elapsed_ms > 0.0 && c.after.elapsed_ms > 0.0)
        .collect();
    if timed.is_empty() {
        return;
    }

    let percentile = |mut v: Vec<f64>, p: f64| -> f64 {
        v.sort_by(|a, b| a.partial_cmp(b).expect("no NaN timings"));
        v[((v.len() - 1) as f64 * p / 100.0).round() as usize]
    };
    let before: Vec<f64> = timed.iter().map(|c| c.before.elapsed_ms).collect();
    let after: Vec<f64> = timed.iter().map(|c| c.after.elapsed_ms).collect();

    println!(
        "\nLatency vs the baseline ({} fixtures timed, reported not gated)",
        timed.len()
    );
    println!(
        "             {:>12} {:>12} {:>9}",
        "baseline", "this run", "change"
    );
    for (label, p) in [("p50", 50.0), ("p90", 90.0), ("p99", 99.0), ("max", 100.0)] {
        let (b, a) = (percentile(before.clone(), p), percentile(after.clone(), p));
        println!(
            "  {label:<10} {b:>10.1}ms {a:>10.1}ms {:>8.0}%",
            if b > 0.0 { 100.0 * (a - b) / b } else { 0.0 }
        );
    }
    let (bt, at): (f64, f64) = (before.iter().sum(), after.iter().sum());
    println!(
        "  {:<10} {:>10.1}s  {:>10.1}s  {:>8.0}%",
        "total",
        bt / 1000.0,
        at / 1000.0,
        if bt > 0.0 {
            100.0 * (at - bt) / bt
        } else {
            0.0
        }
    );

    let mut moved: Vec<(f64, &GateChange)> = timed
        .iter()
        .filter(|c| c.before.elapsed_ms.max(c.after.elapsed_ms) >= LATENCY_FLOOR_MS)
        .filter_map(|c| {
            let factor = c.after.elapsed_ms / c.before.elapsed_ms;
            (factor >= LATENCY_FACTOR || factor <= 1.0 / LATENCY_FACTOR).then_some((factor, *c))
        })
        .collect();
    moved.sort_by(|a, b| b.0.partial_cmp(&a.0).expect("no NaN factors"));
    if moved.is_empty() {
        println!("  no fixture over {LATENCY_FLOOR_MS:.0}ms moved by {LATENCY_FACTOR:.0}x or more");
        return;
    }
    println!(
        "  {} fixture(s) over {LATENCY_FLOOR_MS:.0}ms moved by at least {LATENCY_FACTOR:.0}x:",
        moved.len()
    );
    for (factor, c) in moved {
        println!(
            "    {:<70} {:>9.1} -> {:<9.1} {factor:>6.1}x",
            c.name, c.before.elapsed_ms, c.after.elapsed_ms
        );
    }
}

fn write_csv(rows: &[Row], path: &std::path::Path) -> Result<()> {
    let file = File::create(path)?;
    let mut wtr = Writer::from_writer(file);

    let columns = all_reason_columns(rows);

    let mut header = vec![
        "solution",
        "mismatches",
        "mismatch_pct",
        "total_nodes",
        "visible_mismatches",
        "visible_mismatch_pct",
        "visible_nodes",
        "human_unsolved",
        "algorithm_cost",
        "human_cost",
        "cost_diff",
        "elapsed_ms",
        "graded_nodes",
    ];
    header.extend(columns.iter().map(String::as_str));
    wtr.write_record(&header)?;

    for row in rows {
        let reason_fields: Vec<String> = columns
            .iter()
            .map(|label| {
                row.reason_counts
                    .get(label)
                    .copied()
                    .unwrap_or(0)
                    .to_string()
            })
            .collect();
        match (row.mismatches, row.human_cost) {
            (Some((count, nodes)), Some(human_cost)) => {
                let pct = if nodes > 0 {
                    100.0 * count as f64 / nodes as f64
                } else {
                    0.0
                };
                let (visible_count, visible_nodes) = row.visible_mismatches.unwrap_or((0, 0));
                let visible_pct = if visible_nodes > 0 {
                    100.0 * visible_count as f64 / visible_nodes as f64
                } else {
                    0.0
                };
                let cost_diff = row.algorithm_cost as i64 - human_cost as i64;
                let mut record = vec![
                    row.name.clone(),
                    count.to_string(),
                    format!("{:.2}", pct),
                    nodes.to_string(),
                    visible_count.to_string(),
                    format!("{:.2}", visible_pct),
                    visible_nodes.to_string(),
                    "false".to_string(),
                    row.algorithm_cost.to_string(),
                    human_cost.to_string(),
                    cost_diff.to_string(),
                    format!("{:.3}", row.elapsed_ms),
                    row.graded_nodes
                        .map_or_else(|| "-".to_string(), |g| g.to_string()),
                ];
                record.extend(reason_fields);
                wtr.write_record(&record)?;
            }
            _ => {
                let mut record = vec![
                    row.name.clone(),
                    "-".to_string(),
                    "-".to_string(),
                    "-".to_string(),
                    "-".to_string(),
                    "-".to_string(),
                    "-".to_string(),
                    "true".to_string(),
                    row.algorithm_cost.to_string(),
                    "-".to_string(),
                    "-".to_string(),
                    format!("{:.3}", row.elapsed_ms),
                    row.graded_nodes
                        .map_or_else(|| "-".to_string(), |g| g.to_string()),
                ];
                record.extend(reason_fields);
                wtr.write_record(&record)?;
            }
        }
    }

    wtr.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every label `ASTMappingReason::bucket_label` can return must have a column in
    /// `NON_APTED_REASON_LABELS`, or the mapping entries that pass produces are silently dropped
    /// from the CSV and from the reason table - which is exactly what happened to eight passes
    /// until 2026-09-02 (see `NON_APTED_REASON_LABELS`' own doc comment).
    ///
    /// The variant list below is written out rather than derived because Rust cannot enumerate a
    /// plain enum without a helper crate. It does not need to be exhaustive to be useful: adding a
    /// variant already fails to compile in `bucket_label`'s match, and this test is what turns
    /// "forgot the column too" from silent data loss into a failure that names the missing label.
    #[test]
    fn non_apted_reason_labels_covers_every_bucket_label() {
        let every_variant = [
            ASTMappingReason::IdenticalHash,
            ASTMappingReason::IdenticalHashOfAncestor,
            ASTMappingReason::FullyMappingSubtrees,
            ASTMappingReason::StructurallyIdenticalSubtrees,
            ASTMappingReason::StructurallyIdenticalAncestor,
            ASTMappingReason::OptimalIDU,
            ASTMappingReason::FlatSequenceDiff,
            ASTMappingReason::MovedSubtree,
            ASTMappingReason::LeadingSibling,
            ASTMappingReason::GreedyAnchorBlock,
            ASTMappingReason::BottomUpPropagation,
            ASTMappingReason::UniqueTypeMatching,
            ASTMappingReason::UnresolvedNode,
            ASTMappingReason::MutualAncestor,
            ASTMappingReason::NestedConditionCollapse,
            ASTMappingReason::HeritageClauseGrowth,
            ASTMappingReason::WrapGrowth,
        ];

        for reason in every_variant {
            let label = reason.bucket_label();
            assert!(
                NON_APTED_REASON_LABELS.contains(&label),
                "{reason:?} buckets to the label {label:?}, which has no column in \
                 NON_APTED_REASON_LABELS - every mapping entry that pass produces would be \
                 silently dropped from the CSV and the reason table. Append {label:?} to that list."
            );
        }
    }

    /// `APTED` is the one variant deliberately absent from `NON_APTED_REASON_LABELS`: it fans out
    /// into one `APTED:<source>` column per call site rather than bucketing into a single column,
    /// so the test above must not be "read" as requiring it.
    #[test]
    fn apted_is_deliberately_not_a_fixed_column() {
        assert!(!NON_APTED_REASON_LABELS.contains(&ASTMappingReason::APTED("any").bucket_label()));
    }

    /// A scored fixture. `elapsed_ms`/`algorithm_cost` and friends play no part in the gate, so
    /// they are whatever compiles.
    fn row(name: &str, mismatches: usize, visible: usize) -> Row {
        Row {
            name: name.to_string(),
            mismatches: Some((mismatches, 1000)),
            visible_mismatches: Some((visible, 700)),
            graded_nodes: Some(700),
            reason_counts: HashMap::new(),
            algorithm_cost: 0,
            human_cost: Some(0),
            elapsed_ms: 0.0,
        }
    }

    /// A fixture with no `human_mapping.json` yet: nothing to be right or wrong about, so it must
    /// stay out of the baseline entirely rather than enter it as a zero.
    fn unsolved(name: &str) -> Row {
        Row {
            name: name.to_string(),
            mismatches: None,
            visible_mismatches: None,
            graded_nodes: None,
            reason_counts: HashMap::new(),
            algorithm_cost: 0,
            human_cost: None,
            elapsed_ms: 0.0,
        }
    }

    fn baseline(entries: &[(&str, usize, usize)]) -> BTreeMap<String, BaselineEntry> {
        entries
            .iter()
            .map(|(name, mismatches, visible_mismatches)| {
                (
                    name.to_string(),
                    BaselineEntry {
                        mismatches: *mismatches,
                        visible_mismatches: *visible_mismatches,
                        elapsed_ms: 0.0,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn a_fixture_that_got_worse_fails_the_gate() {
        let report = compare_to_baseline(
            &[row("a", 12, 8), row("b", 3, 1)],
            &baseline(&[("a", 10, 8), ("b", 3, 1)]),
        );

        assert!(report.failed());
        assert_eq!(report.regressed.len(), 1);
        assert_eq!(report.regressed[0].name, "a");
        assert_eq!(report.regressed[0].before.mismatches, 10);
        assert_eq!(report.regressed[0].after.mismatches, 12);
    }

    /// The two columns move independently: a change can leave a fixture's total flat while turning
    /// invisible scaffolding mismatches into ones a reader actually sees. The project's accuracy
    /// goals are stated in visible nodes, so that is a regression even though the total didn't
    /// move.
    #[test]
    fn a_fixture_whose_mismatches_became_visible_fails_the_gate() {
        let report = compare_to_baseline(&[row("a", 10, 9)], &baseline(&[("a", 10, 4)]));

        assert!(report.failed());
        assert_eq!(report.regressed.len(), 1);
    }

    #[test]
    fn improvements_pass_and_are_reported_separately() {
        let report = compare_to_baseline(
            &[row("a", 4, 2), row("b", 3, 1)],
            &baseline(&[("a", 10, 8), ("b", 3, 1)]),
        );

        assert!(!report.failed());
        assert_eq!(report.improved.len(), 1, "only 'a' moved");
        assert_eq!(report.improved[0].name, "a");
    }

    /// The defect this gate replaced. The corpus grows deliberately toward hard cases, so an
    /// aggregate baseline reads every such addition as a quality regression. A fixture with no
    /// baseline row cannot have got worse, so it is reported and not gated.
    #[test]
    fn a_new_hard_fixture_is_reported_but_does_not_fail_the_gate() {
        let report = compare_to_baseline(
            &[row("old", 3, 1), row("brand-new-and-hard", 334, 227)],
            &baseline(&[("old", 3, 1)]),
        );

        assert!(!report.failed(), "a new fixture must never fail the gate");
        assert_eq!(report.added, vec!["brand-new-and-hard".to_string()]);
        assert!(report.regressed.is_empty());
    }

    /// Losing a whole dataset directory must not pass green. `removed` is non-gating per fixture,
    /// which had silently become "no lower bound at all" - see
    /// `GateReport::lost_too_much_of_the_corpus`.
    #[test]
    fn a_run_that_lost_most_of_the_corpus_fails_even_with_no_regression() {
        let baseline_rows: Vec<(&str, usize, usize)> =
            (0..100).map(|_| ("placeholder", 3, 1)).collect();
        let named: Vec<(String, usize, usize)> = baseline_rows
            .iter()
            .enumerate()
            .map(|(i, (_, m, v))| (format!("fixture-{i:03}"), *m, *v))
            .collect();
        let full: Vec<(&str, usize, usize)> =
            named.iter().map(|(n, m, v)| (n.as_str(), *m, *v)).collect();

        // Only the first 50 of 100 fixtures were scored, and every one of them matches its
        // baseline exactly: nothing regressed, and half the corpus is simply absent.
        let scored: Vec<Row> = named
            .iter()
            .take(50)
            .map(|(n, m, v)| row(n, *m, *v))
            .collect();
        let report = compare_to_baseline(&scored, &baseline(&full));

        assert!(report.regressed.is_empty(), "nothing actually got worse");
        assert_eq!(report.removed.len(), 50);
        assert!(
            report.failed(),
            "a run covering half its baseline must not report success"
        );
    }

    /// The other half of the floor: routine churn stays non-gating. Dropping or renaming one
    /// fixture out of 100 is an ordinary corpus operation and must not turn the gate red.
    #[test]
    fn dropping_a_single_fixture_still_passes() {
        let named: Vec<(String, usize, usize)> = (0..100)
            .map(|i| (format!("fixture-{i:03}"), 3, 1))
            .collect();
        let full: Vec<(&str, usize, usize)> =
            named.iter().map(|(n, m, v)| (n.as_str(), *m, *v)).collect();
        let scored: Vec<Row> = named
            .iter()
            .take(99)
            .map(|(n, m, v)| row(n, *m, *v))
            .collect();

        let report = compare_to_baseline(&scored, &baseline(&full));

        assert_eq!(report.removed.len(), 1);
        assert!(
            !report.failed(),
            "one dropped fixture is routine and must stay non-gating"
        );
    }

    /// Deleting an inconvenient fixture is the one way to make every number improve without
    /// improving anything. It can't fail the gate - fixtures are legitimately renamed and dropped -
    /// so it has to be named in the report instead.
    #[test]
    fn a_fixture_missing_from_the_run_is_named_rather_than_silently_ignored() {
        let report = compare_to_baseline(
            &[row("a", 3, 1)],
            &baseline(&[("a", 3, 1), ("gone", 900, 700)]),
        );

        assert!(!report.failed());
        assert_eq!(report.removed, vec!["gone".to_string()]);
    }

    #[test]
    fn an_unsolved_fixture_is_counted_but_never_enters_the_baseline() {
        let rows = [row("a", 3, 1), unsolved("not-yet-mapped")];

        assert_eq!(
            baseline_from_rows(&rows).keys().collect::<Vec<_>>(),
            vec!["a"],
            "a fixture with no human mapping has nothing to be right or wrong about"
        );
        let report = compare_to_baseline(&rows, &baseline(&[("a", 3, 1)]));
        assert_eq!(report.unsolved, 1);
        assert!(report.added.is_empty() && !report.failed());
    }

    /// The contract that keeps the gate trustworthy: wall-clock never fails it. A fixture can get
    /// arbitrarily slower and the gate still passes, because on one idle machine a single fixture's
    /// own time moves run to run by up to 4.9x - a latency gate at any threshold tight enough to
    /// catch a real slowdown would fire on noise instead, and a gate that cries wolf gets ignored
    /// for the accuracy regressions it *can* prove.
    #[test]
    fn latency_is_reported_but_never_fails_the_gate() {
        let mut slow = row("a", 3, 1);
        slow.elapsed_ms = 5_000.0;
        let baseline = BTreeMap::from([(
            "a".to_string(),
            BaselineEntry {
                mismatches: 3,
                visible_mismatches: 1,
                elapsed_ms: 5.0,
            },
        )]);

        let report = compare_to_baseline(&[slow], &baseline);

        assert!(
            !report.failed(),
            "a 1000x slowdown is not an accuracy regression"
        );
        assert!(report.regressed.is_empty() && report.improved.is_empty());
        assert_eq!(
            report.latency.len(),
            1,
            "but it is still carried for reporting"
        );
        assert_eq!(report.latency[0].after.elapsed_ms, 5_000.0);
    }

    /// A baseline written from a run must compare equal to that same run - otherwise
    /// `make update-quality-baseline` would leave the gate red.
    #[test]
    fn a_freshly_written_baseline_passes_against_its_own_run() {
        let rows = [row("a", 12, 8), row("b", 0, 0), unsolved("c")];
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("quality_baseline.csv");

        write_baseline(&rows, &path).expect("write");
        let report = compare_to_baseline(&rows, &read_baseline(&path).expect("read"));

        assert!(!report.failed());
        assert!(report.improved.is_empty() && report.added.is_empty() && report.removed.is_empty());
    }
}
