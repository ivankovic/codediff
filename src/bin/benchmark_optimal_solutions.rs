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

use anyhow::{Result, bail};
use clap::Parser;
use codediff::code::Code;
use codediff::diff::ASTMappingReason;
use codediff::diff::cost::diff_cost;
use codediff::test::helper;
use codediff::test::helper::human_mapping;
use std::collections::HashMap;
use std::fs::File;

use csv::Writer;

/// Short column label for every `ASTMappingReason` variant *except* `APTED`, in declaration order.
/// Kept as an explicit list (rather than deriving one) so the table's column order stays stable
/// even if variants are reordered in `diff.rs`. `APTED` is deliberately excluded: unlike every
/// other variant, it doesn't map to one fixed column - see `reason_column_label`.
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

/// Every reason column that exists at all: the fixed `NON_APTED_REASON_LABELS` (all 16,
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
}

/// Prints every mapping codediff produces for one fixture, with human-readable paths, sorted by
/// the before path (inserts, having none, sort last).
fn dump_mapping(name: &str, config: &codediff::diff::HeuristicConfig) -> Result<()> {
    use codediff::test::helper::path_for_node;

    let test_diffs = helper::handmade_test_code_pairs()?;
    let (before, after) = test_diffs
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("no before/after test code pair found for '{}'", name))?
        .clone();

    let diff = codediff::diff::diff_code_with_config(&before, &after, config);
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

    let test_diffs = helper::handmade_test_code_pairs()?;
    let mut names: Vec<String> = test_diffs.keys().cloned().collect();
    names.sort();

    let started = std::time::Instant::now();
    let mut rows = Vec::with_capacity(names.len());
    for name in &names {
        let (before, after) = test_diffs
            .get(name)
            .expect("name came from test_diffs.keys()");
        // `handmade_test_code_pairs` hands out clones, and `Code`'s hand-written `Clone` drops
        // `ast_metadata` to `None` (see its doc comment) - without re-caching it here, every
        // `metadata_of` call across the pipeline silently recomputes whole-tree metadata from
        // scratch (measured 2026-08-17: ~26 full recomputes per `diff_code_with_config` call,
        // ~6s of pure recompute overhead on the largest fixture - inflating `elapsed_ms` ~6x
        // over what the production path, which precomputes metadata in `Code::from_string`/
        // `from_file`, actually costs). Mutate local clones so the timed diff below measures the
        // pipeline, not the harness artifact.
        let (mut before, mut after) = (before.clone(), after.clone());
        for code in [&mut before, &mut after] {
            if code.metadata.language.is_some() {
                code.ensure_parsed()?;
            }
        }
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
            });
            continue;
        }
        let visible = human_mapping::compute_visible_mismatches_for_with_config(
            name, before, after, &config,
        )?;
        let mismatch_count = visible.visible.len() + visible.invisible.len();
        let total_nodes = human_mapping::total_node_count_for(before, after);
        let human_cost = human_mapping::human_mapping_cost_for(name, before, after)?;
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

    let elapsed = started.elapsed();

    if let Some(csv_path) = args.csv {
        let path = csv_path.unwrap_or_else(|| {
            std::path::PathBuf::from("./research/data/quality/optimal_solutions_benchmark.csv")
        });
        write_csv(&rows, &path)?;
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
    Ok(())
}

/// The second accuracy goal's per-fixture ceiling: at most this fraction of a fixture's *visible*
/// nodes may disagree with the human mapping. 4%, not the 0.5% the all-nodes phrasing used - see
/// `src/diff/TODO.md`'s "Why 4% and not 0.5%": visible nodes are only ~3.4% of all nodes, so 0.5%
/// of them is under one node for a median fixture and the 99% tier came out stricter than the
/// 90%-at-zero tier it is supposed to relax.
const VISIBLE_RATE_GOAL: f64 = 0.04;

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
}

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
    let mut total_algorithm_cost = 0u64;
    // Only summed over fixtures that also have a human cost, so `total_cost_diff` below compares
    // like for like - an "unsolved" fixture's algorithm cost would otherwise inflate the TOTAL
    // algorithm side against nothing on the human side.
    let mut total_algorithm_cost_where_solved = 0u64;
    let mut total_human_cost = 0u64;
    let mut total_elapsed_ms = 0.0f64;
    for row in rows {
        total_algorithm_cost += row.algorithm_cost;
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
        total_algorithm_cost,
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
                ];
                record.extend(reason_fields);
                wtr.write_record(&record)?;
            }
        }
    }

    wtr.flush()?;
    Ok(())
}
