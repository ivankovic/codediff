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

//! Scores codediff against every fixture in `src/test/data/diffs/` that has a
//! `human_mapping.json`, counting mismatched nodes; fixtures without one are reported as
//! "unsolved". A mismatch count shows partial progress that `cargo test optimal_solutions`'s
//! pass/fail cannot.

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

/// Column label for every `ASTMappingReason` except `APTED` (see `reason_column_label`). An
/// explicit list so CSV column positions stay stable: append new labels, never insert.
///
/// Must cover every label `bucket_label` can return, or that pass's entries silently vanish from
/// the CSV and the reason TOTAL; `non_apted_reason_labels_covers_every_bucket_label` enforces it.
/// `Comment` and `BottomUp` name retired passes but stay because `matching_reasons_report.py`
/// indexes them by name.
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
    "LeadSib",
    "BottomUpProp",
    "UniqueType",
    "Unresolved",
    "MutualAnc",
    "CondCollapse",
    "HeritageGrowth",
    "WrapGrowth",
];

/// Column label for one `ASTMappingReason`: `bucket_label`, except that `APTED` gets one column
/// per provenance (`"APTED:final_pass"`, ...), since this table is where that breakdown is read.
/// The APTED column set is therefore data-dependent; see `all_reason_columns`.
fn reason_column_label(reason: &ASTMappingReason) -> String {
    match reason {
        ASTMappingReason::APTED(source) => format!("APTED:{source}"),
        other => other.bucket_label().to_string(),
    }
}

/// Tallies every mapping entry, including lone deletes/inserts, by `reason_column_label`.
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

/// CSV columns: all of `NON_APTED_REASON_LABELS`, even when zero everywhere, so downstream tools
/// see a stable shape, then every observed `"APTED:<source>"` sorted by name. APTED columns go
/// last because their set depends on the data.
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

/// `all_reason_columns` without always-zero columns, for the terminal table only.
fn active_reason_columns(rows: &[Row]) -> Vec<String> {
    all_reason_columns(rows)
        .into_iter()
        .filter(|label| {
            rows.iter()
                .any(|r| r.reason_counts.get(label).copied().unwrap_or(0) > 0)
        })
        .collect()
}

/// Total unit-cost (`diff_cost`) of codediff's own mapping.
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

/// Wall-clock milliseconds for one diff, single-shot. Each of this file's measurements runs its
/// own diff; a benchmark run once by hand does not warrant sharing one.
fn elapsed_ms_for(before: &Code, after: &Code, config: &codediff::diff::HeuristicConfig) -> f64 {
    let started = std::time::Instant::now();
    let _diff = codediff::diff::diff_code_with_config(before, after, config);
    started.elapsed().as_secs_f64() * 1000.0
}

#[derive(Parser)]
struct Args {
    /// Print every mismatch for this fixture, with codediff's operation and reason, instead of the
    /// table.
    #[arg(long)]
    details: Option<String>,

    /// Print codediff's complete mapping for this fixture (paths, operation, reason) instead of the
    /// table.
    #[arg(long)]
    dump: Option<String>,

    /// Output results as a CSV file. Default path: "./research/data/quality/optimal_solutions_benchmark.csv"
    #[arg(long, value_name = "PATH", num_args = 0..=1)]
    csv: Option<Option<std::path::PathBuf>>,

    /// Compare against a per-fixture quality baseline and exit non-zero if any fixture regressed.
    /// This is the release gate.
    #[arg(long, value_name = "PATH")]
    compare: Option<std::path::PathBuf>,

    /// Write this run as the new quality baseline (`make update-quality-baseline`).
    #[arg(long, value_name = "PATH")]
    write_baseline: Option<std::path::PathBuf>,

    /// Enable `solve_moved_subtrees` (default).
    #[arg(long = "solver-moved-subtrees", action = clap::ArgAction::SetTrue, default_value_t = true, overrides_with = "no_solver_moved_subtrees")]
    solver_moved_subtrees: bool,
    /// Disable `solve_moved_subtrees` (deleted+inserted identical-subtree move pairing).
    #[arg(long = "no-solver-moved-subtrees", action = clap::ArgAction::SetTrue, default_value_t = false, overrides_with = "solver_moved_subtrees")]
    no_solver_moved_subtrees: bool,

    /// Enable `solve_bottom_up_propagation` (default).
    #[arg(long = "solver-bottom-up-propagation", action = clap::ArgAction::SetTrue, default_value_t = true, overrides_with = "no_solver_bottom_up_propagation")]
    solver_bottom_up_propagation: bool,
    /// Disable `solve_bottom_up_propagation`.
    #[arg(long = "no-solver-bottom-up-propagation", action = clap::ArgAction::SetTrue, default_value_t = false, overrides_with = "solver_bottom_up_propagation")]
    no_solver_bottom_up_propagation: bool,

    /// Enable `solve_unique_type_matching`, GumTree Simple's unique type matching (default).
    #[arg(long = "solver-unique-type-matching", action = clap::ArgAction::SetTrue, default_value_t = true, overrides_with = "no_solver_unique_type_matching")]
    solver_unique_type_matching: bool,
    /// Disable `solve_unique_type_matching`.
    #[arg(long = "no-solver-unique-type-matching", action = clap::ArgAction::SetTrue, default_value_t = false, overrides_with = "solver_unique_type_matching")]
    no_solver_unique_type_matching: bool,

    /// Enable `solve_mutual_ancestors`, mutual lowest-common-ancestor pairing (default).
    #[arg(long = "solver-mutual-ancestors", action = clap::ArgAction::SetTrue, default_value_t = true, overrides_with = "no_solver_mutual_ancestors")]
    solver_mutual_ancestors: bool,
    /// Disable `solve_mutual_ancestors`.
    #[arg(long = "no-solver-mutual-ancestors", action = clap::ArgAction::SetTrue, default_value_t = false, overrides_with = "solver_mutual_ancestors")]
    no_solver_mutual_ancestors: bool,
}

/// Resolves the `--solver-X`/`--no-solver-X` pairs; `overrides_with` makes the last flag win.
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
    /// `(mismatched, total nodes in both trees)`; `None` when the fixture is unsolved or
    /// text-only.
    mismatches: Option<(usize, usize)>,
    /// Mapping entries per `reason_column_label`.
    reason_counts: HashMap<String, usize>,
    algorithm_cost: u64,
    /// The human mapping's cost under the same model as `algorithm_cost`.
    human_cost: Option<u64>,
    elapsed_ms: f64,
    /// Like `mismatches`, restricted to nodes that carry text of their own
    /// (`is_structurally_visible`) rather than pure scaffolding such as a `block`.
    visible_mismatches: Option<(usize, usize)>,
    /// The pair has no AST (unsupported language, plain-text diff), so there is nothing to score.
    /// Kept apart from "unsolved", which must mean exactly "no `human_mapping.json` yet".
    text_only: bool,
    /// Node slots the human mapping grades, in `mismatches`' denominator's unit. The percentage
    /// divides by every node while only graded ones can mismatch, so a thinly annotated fixture
    /// looks better than it is without this.
    graded_nodes: Option<usize>,
}

/// Prints every mapping codediff produces for one fixture, with human-readable paths, sorted by
/// the before path (inserts, having none, sort last).
fn dump_mapping(name: &str, config: &codediff::diff::HeuristicConfig) -> Result<()> {
    use codediff::test::helper::path_for_node;

    // Borrowed, not cloned: `Code`'s `Clone` drops `ast_metadata`, which makes every
    // `metadata_of` recompute.
    let pair = helper::handmade_test_code_pair(name)?;
    let (before, after) = (&pair.0, &pair.1);

    let diff = codediff::diff::diff_code_with_config(before, after, config);
    let ast = diff.ast.expect("diff has AST");

    let before_ast = before.ast.as_ref().expect("before parsed");
    let after_ast = after.ast.as_ref().expect("after parsed");

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

    // Streamed per fixture rather than via the `handmade_test_code_pairs` cache, so peak memory is
    // the largest fixture, not the whole corpus (which does not fit a CI runner). Owned pairs,
    // never clones, because `Code`'s `Clone` drops `ast_metadata` and the timed diff would then
    // recompute it on every lookup.
    let cases = helper::handmade_test_case_dirs()?;

    let started = std::time::Instant::now();
    // Load+parse time is subtracted so the reported runtime covers scoring only.
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

        // Keyed off `Code::ast`, not `diff.ast`, which is `Some` even when neither side parsed.
        // The human-mapping calls below bail on an AST-less pair and would fail the whole gate.
        if before.ast.is_none() || after.ast.is_none() {
            rows.push(Row {
                name: name.clone(),
                mismatches: None,
                reason_counts,
                algorithm_cost,
                human_cost: None,
                elapsed_ms,
                visible_mismatches: None,
                graded_nodes: None,
                text_only: true,
            });
            continue;
        }

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
                text_only: false,
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
            text_only: false,
        });
    }

    // Worst first; unsolved last.
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

    // Last, so a failing gate still leaves the full table on screen.
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

/// The second accuracy goal's per-fixture ceiling on the share of *visible* nodes that may
/// disagree with the human mapping. It is tied to the structural visible set, which is most of
/// the tree; a looser rate stops discriminating.
const VISIBLE_RATE_GOAL: f64 = 0.01;

/// Progress against the README's two accuracy goals, in visible nodes, over solved fixtures only.
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

    // A caveat, not a correction: thinly annotated fixtures clear the rate bar for free, and the
    // fix is to finish annotating them, not to rescale the metric.
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

/// Below this graded share, a fixture's mismatch rate reflects annotation coverage more than diff
/// quality. The corpus separates cleanly around this value.
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
    // Solved fixtures only, so the TOTAL row's `Alg Cost - Hum Cost` equals its `Cost Diff`.
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
                if !row.text_only {
                    total_unsolved += 1;
                }
                let unsolved_cell = if row.text_only { "text-only" } else { "yes" };
                println!(
                    "{:<name_width$}  {:>10}  {:>7}  {:>9}  {:>7}  {:>13}  {:>9}  {:>9}  {:>9}  {:>12.1}",
                    row.name,
                    "-",
                    "-",
                    "-",
                    "-",
                    unsolved_cell,
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

/// Prints mapping entries per fixture and reason, plus a TOTAL row.
fn print_reason_table(rows: &[Row]) {
    let name_width = rows
        .iter()
        .map(|r| r.name.len())
        .chain(["Solution".len()])
        .max()
        .unwrap_or(0);

    let active_reasons = active_reason_columns(rows);
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
// The release gate (`make check-quality`). Per fixture, never aggregate: the corpus grows toward
// hard cases, so any aggregate total or rate rises when hard fixtures are added and cannot tell
// that apart from a regression. It also catches drift below the `optimal_solutions` tests'
// per-fixture clamps, which only fire above the recorded value.

/// One fixture's row in the gate baseline.
#[derive(Debug, Clone, Copy, PartialEq)]
struct BaselineEntry {
    mismatches: usize,
    visible_mismatches: usize,
    /// Reported but never gated; see [`print_latency_report`].
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
    /// Fixtures with no baseline row. Never a failure: nothing to regress from.
    added: Vec<String>,
    /// Baseline rows missing from the run. Not a failure per fixture (renames happen), but always
    /// printed so deleting a fixture cannot pass silently.
    removed: Vec<String>,
    /// Fixtures with no `human_mapping.json`, in neither the run's comparison nor the baseline.
    unsolved: usize,
    /// Every fixture present in both, for the ungated latency section.
    latency: Vec<GateChange>,
    /// Baseline fixtures this run scored; see [`GateReport::lost_too_much_of_the_corpus`].
    covered: usize,
}

impl GateReport {
    fn failed(&self) -> bool {
        !self.regressed.is_empty() || self.lost_too_much_of_the_corpus()
    }

    /// Did this run cover so little of the baseline that its verdict is meaningless? Needed because
    /// every path that loses fixtures is silent (an unchecked-out dataset directory, a fixture
    /// without `before.*.test`). Proportional, so dropping one fixture stays routine while losing
    /// a dataset fails.
    fn lost_too_much_of_the_corpus(&self) -> bool {
        let baseline_size = self.covered + self.removed.len();
        // Guarded on the baseline's size, not the run's, so a run that scored nothing still fails.
        if baseline_size < MIN_BASELINE_FOR_COVERAGE_CHECK {
            return false;
        }
        (self.covered as f64) / (baseline_size as f64) < MIN_BASELINE_COVERAGE
    }
}

/// Minimum share of the baseline a run must score. Below any intentional churn and above the loss
/// of the smallest dataset directory.
const MIN_BASELINE_COVERAGE: f64 = 0.95;

/// Baselines smaller than this skip the coverage check: a share of a handful of fixtures means
/// nothing.
const MIN_BASELINE_FOR_COVERAGE_CHECK: usize = 20;

/// The scored fixtures of a run, in baseline form; unsolved and text-only fixtures are left out.
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
        // Trimmed: an untrimmed stray space makes the fixture "new" and its baseline "removed",
        // both ungated, so it could regress freely.
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
                // A missing column reads as 0.0, "untimed", which the latency report skips.
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

/// Compares a run against a baseline. Both columns gate: a flat total can hide invisible
/// mismatches becoming visible ones, which is what the accuracy goals are stated in.
fn compare_to_baseline(rows: &[Row], baseline: &BTreeMap<String, BaselineEntry>) -> GateReport {
    let current = baseline_from_rows(rows);
    let mut report = GateReport {
        // Not `rows.len() - current.len()`: text-only fixtures are absent from `current` too.
        unsolved: rows
            .iter()
            .filter(|row| row.mismatches.is_none() && !row.text_only)
            .count(),
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
    // Listed by name, since both are ungated and a removal lowers the bar without moving a number.
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

/// Below this, a fixture's own time is mostly scheduling noise.
const LATENCY_FLOOR_MS: f64 = 20.0;

/// The smallest per-fixture change above the floor that is not mostly run-to-run noise.
const LATENCY_FACTOR: f64 = 2.0;

/// Latency against the baseline, reported and never gated. Accuracy reproduces exactly; one
/// fixture's wall-clock swings far more run to run than any threshold that would catch a real
/// slowdown, while the aggregate percentiles are stable. So the percentiles are always printed and
/// single fixtures are named only past both thresholds.
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
                    // `true` means "no human_mapping.json"; a text-only fixture has one.
                    if row.text_only {
                        "text-only".to_string()
                    } else {
                        "true".to_string()
                    },
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

    /// Written out because a plain enum cannot be enumerated; a new variant already breaks
    /// `bucket_label`'s match, and this makes "forgot the column" fail by name.
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

    #[test]
    fn apted_is_deliberately_not_a_fixed_column() {
        assert!(!NON_APTED_REASON_LABELS.contains(&ASTMappingReason::APTED("any").bucket_label()));
    }

    /// A scored fixture; the fields the gate ignores are placeholders.
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
            text_only: false,
        }
    }

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
            text_only: false,
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

    #[test]
    fn a_fixture_missing_from_the_run_is_named_rather_than_silently_ignored() {
        let report = compare_to_baseline(
            &[row("a", 3, 1)],
            &baseline(&[("a", 3, 1), ("gone", 900, 700)]),
        );

        assert!(!report.failed());
        assert_eq!(report.removed, vec!["gone".to_string()]);
    }

    /// No grammar, so no tree to score, but solved: it has a `human_mapping.json`.
    fn text_only(name: &str) -> Row {
        Row {
            text_only: true,
            ..unsolved(name)
        }
    }

    #[test]
    fn a_text_only_fixture_stays_out_of_the_baseline_without_counting_as_unsolved() {
        let rows = [row("a", 3, 1), text_only("bazel-no-grammar")];

        assert_eq!(
            baseline_from_rows(&rows).keys().collect::<Vec<_>>(),
            vec!["a"],
            "a fixture with no AST has no mapping score to record"
        );
        let report = compare_to_baseline(&rows, &baseline(&[("a", 3, 1)]));
        assert_eq!(
            report.unsolved, 0,
            "text-only is solved; only a missing human_mapping.json is unsolved"
        );
        assert!(report.added.is_empty() && !report.failed());
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

    fn read_baseline_text(text: &str) -> BTreeMap<String, BaselineEntry> {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("quality_baseline.csv");
        std::fs::write(&path, text).expect("write");
        read_baseline(&path).expect("read")
    }

    #[test]
    fn a_baseline_name_with_stray_whitespace_still_gates_its_fixture() {
        let baseline =
            read_baseline_text("solution,mismatches,visible_mismatches\nrust-add-if , 4, 2\n");

        let report = compare_to_baseline(&[row("rust-add-if", 9, 2)], &baseline);

        assert!(report.added.is_empty() && report.removed.is_empty());
        assert!(
            report.failed(),
            "the regression must not escape as new+removed"
        );
    }

    #[test]
    fn a_baseline_without_elapsed_ms_reads_as_untimed() {
        let baseline = read_baseline_text("solution,mismatches,visible_mismatches\na,3,1\n");

        assert_eq!(baseline["a"].elapsed_ms, 0.0);
    }
}
