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
use codediff::test::helper;
use codediff::test::helper::human_mapping;
use std::collections::HashMap;
use std::fs::File;

use csv::Writer;

/// Every `ASTMappingReason` variant, in declaration order, paired with a short column label for
/// the terminal table. Kept as an explicit list (rather than deriving one) so the table's column
/// order stays stable even if variants are reordered in `diff.rs`.
const REASONS: &[(ASTMappingReason, &str)] = &[
    (ASTMappingReason::IdenticalHash, "IdHash"),
    (ASTMappingReason::IdenticalHashOfAncestor, "IdHashAnc"),
    (ASTMappingReason::FullymappingSubtrees, "FullMap"),
    (ASTMappingReason::StructurallyIdenticalSubtrees, "StructId"),
    (ASTMappingReason::StructurallyIdenticalAncestor, "StructAnc"),
    (ASTMappingReason::OptimalIDU, "OptIDU"),
    (ASTMappingReason::APTED, "APTED"),
    (ASTMappingReason::FlatSequenceDiff, "FlatSeq"),
    (ASTMappingReason::MovedSubtree, "Moved"),
    (ASTMappingReason::CommentSibling, "Comment"),
    (ASTMappingReason::BottomUpExpansion, "BottomUp"),
];

/// Runs codediff once and tallies every mapping entry (matched pairs *and* lone deletes/inserts)
/// by its `ASTMappingReason` - i.e. which algorithm pass is responsible for how much of the
/// diff. Independent of `human_mapping.json`, so this works for "unsolved" fixtures too.
fn reason_counts_for(before: &Code, after: &Code) -> HashMap<ASTMappingReason, usize> {
    let diff = codediff::diff::diff_code(before, after);
    let mut counts = HashMap::new();
    if let Some(diff_ast) = diff.ast {
        for mapping in diff_ast.mapping.values() {
            *counts.entry(mapping.reason).or_insert(0) += 1;
        }
    }
    counts
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

    /// Output results as a CSV file. Default path: "./research/optimal_solutions_benchmark.csv"
    #[arg(long, value_name = "PATH", num_args = 0..=1)]
    csv: Option<Option<std::path::PathBuf>>,
}

struct Row {
    name: String,
    /// `None` means there's no `human_mapping.json` for this fixture yet (unsolved), as opposed
    /// to `Some((0, _))`, which means codediff matches the human mapping exactly.
    ///
    /// The second element of the tuple is the total node count (before + after trees combined),
    /// the denominator for the mismatch percentage - see `human_mapping::total_node_count_for`.
    mismatches: Option<(usize, usize)>,
    /// How many mapping entries codediff produced for each `ASTMappingReason` - i.e. which pass
    /// (hash matching, semantic-structural anchoring, APTED, ...) did how much of the work.
    /// Computed unconditionally (doesn't need `human_mapping.json`), so this is populated even
    /// for "unsolved" fixtures.
    reason_counts: HashMap<ASTMappingReason, usize>,
}

/// Prints every mapping codediff produces for one fixture, with human-readable paths, sorted by
/// the before path (inserts, having none, sort last).
fn dump_mapping(name: &str) -> Result<()> {
    use codediff::test::helper::path_for_node;

    let test_diffs = helper::handmade_test_code_pairs()?;
    let (before, after) = test_diffs
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("no before/after test code pair found for '{}'", name))?
        .clone();

    let diff = codediff::diff::diff_code(&before, &after);
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
            let bp = if b == 0 { "-" } else { paths.get(&b).map(String::as_str).unwrap_or("?") };
            let ap = if a == 0 { "-" } else { paths.get(&a).map(String::as_str).unwrap_or("?") };
            format!("{:?} ({:?})\n    B {}\n    A {}", m.operation, m.reason, bp, ap)
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

    if let Some(name) = args.details {
        if !human_mapping::mapping_path(&name).exists() {
            bail!("fixture '{}' has no human_mapping.json", name);
        }
        let mismatches = human_mapping::compute_mismatches(&name)?;
        println!("{}: {} mismatch(es)", name, mismatches.len());
        for m in &mismatches {
            println!("  {}", m);
        }
        return Ok(());
    }

    if let Some(name) = args.dump {
        return dump_mapping(&name);
    }

    let test_diffs = helper::handmade_test_code_pairs()?;
    let mut names: Vec<String> = test_diffs.keys().cloned().collect();
    names.sort();

    let mut rows = Vec::with_capacity(names.len());
    for name in &names {
        let (before, after) = test_diffs.get(name).expect("name came from test_diffs.keys()");
        let reason_counts = reason_counts_for(before, after);

        if !human_mapping::mapping_path(name).exists() {
            rows.push(Row {
                name: name.clone(),
                mismatches: None,
                reason_counts,
            });
            continue;
        }
        let mismatches = human_mapping::compute_mismatches_for(name, before, after)?;
        let total_nodes = human_mapping::total_node_count_for(before, after);
        rows.push(Row {
            name: name.clone(),
            mismatches: Some((mismatches.len(), total_nodes)),
            reason_counts,
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

    if let Some(csv_path) = args.csv {
        let path = csv_path.unwrap_or_else(|| std::path::PathBuf::from("./research/optimal_solutions_benchmark.csv"));
        write_csv(&rows, &path)?;
    }

    print_table(&rows);
    print_reason_table(&rows);
    Ok(())
}

fn print_table(rows: &[Row]) {
    let name_width = rows
        .iter()
        .map(|r| r.name.len())
        .chain(["Solution".len()])
        .max()
        .unwrap_or(0);

    println!(
        "{:<name_width$}  {:>10}  {:>7}  {:>13}",
        "Solution",
        "Mismatches",
        "Mism %",
        "Human Unsolved",
        name_width = name_width
    );
    println!("{}", "-".repeat(name_width + 2 + 10 + 2 + 7 + 2 + 13));

    let mut total_mismatches = 0usize;
    let mut total_nodes = 0usize;
    let mut total_unsolved = 0usize;
    for row in rows {
        match row.mismatches {
            Some((count, nodes)) => {
                total_mismatches += count;
                total_nodes += nodes;
                let pct = if nodes > 0 { 100.0 * count as f64 / nodes as f64 } else { 0.0 };
                println!(
                    "{:<name_width$}  {:>10}  {:>6.2}%  {:>13}",
                    row.name,
                    count,
                    pct,
                    "",
                    name_width = name_width
                );
            }
            None => {
                total_unsolved += 1;
                println!(
                    "{:<name_width$}  {:>10}  {:>7}  {:>13}",
                    row.name,
                    "-",
                    "-",
                    "yes",
                    name_width = name_width
                );
            }
        }
    }

    println!("{}", "-".repeat(name_width + 2 + 10 + 2 + 7 + 2 + 13));
    let total_pct = if total_nodes > 0 {
        100.0 * total_mismatches as f64 / total_nodes as f64
    } else {
        0.0
    };
    println!(
        "{:<name_width$}  {:>10}  {:>6.2}%  {:>13}",
        "TOTAL",
        total_mismatches,
        total_pct,
        total_unsolved,
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

    let active_reasons: Vec<(ASTMappingReason, &str)> = REASONS
        .iter()
        .copied()
        .filter(|(reason, _)| rows.iter().any(|r| r.reason_counts.get(reason).copied().unwrap_or(0) > 0))
        .collect();

    const COL_WIDTH: usize = 9;
    let rule_width = name_width + active_reasons.len() * (COL_WIDTH + 2);

    println!();
    println!("Mapping reasons per fixture (how much work each algorithm pass did):");
    print!("{:<name_width$}", "Solution", name_width = name_width);
    for (_, label) in &active_reasons {
        print!("  {:>COL_WIDTH$}", label);
    }
    println!();
    println!("{}", "-".repeat(rule_width));

    let mut totals: HashMap<ASTMappingReason, usize> = HashMap::new();
    for row in rows {
        print!("{:<name_width$}", row.name, name_width = name_width);
        for (reason, _) in &active_reasons {
            let count = row.reason_counts.get(reason).copied().unwrap_or(0);
            *totals.entry(*reason).or_insert(0) += count;
            print!("  {:>COL_WIDTH$}", count);
        }
        println!();
    }

    println!("{}", "-".repeat(rule_width));
    print!("{:<name_width$}", "TOTAL", name_width = name_width);
    for (reason, _) in &active_reasons {
        print!("  {:>COL_WIDTH$}", totals.get(reason).copied().unwrap_or(0));
    }
    println!();
}

fn write_csv(rows: &[Row], path: &std::path::Path) -> Result<()> {
    let file = File::create(path)?;
    let mut wtr = Writer::from_writer(file);

    let mut header = vec!["solution", "mismatches", "mismatch_pct", "total_nodes", "human_unsolved"];
    header.extend(REASONS.iter().map(|(_, label)| *label));
    wtr.write_record(&header)?;

    for row in rows {
        let reason_fields: Vec<String> = REASONS
            .iter()
            .map(|(reason, _)| row.reason_counts.get(reason).copied().unwrap_or(0).to_string())
            .collect();
        match row.mismatches {
            Some((count, nodes)) => {
                let pct = if nodes > 0 { 100.0 * count as f64 / nodes as f64 } else { 0.0 };
                let mut record = vec![
                    row.name.clone(),
                    count.to_string(),
                    format!("{:.2}", pct),
                    nodes.to_string(),
                    "false".to_string(),
                ];
                record.extend(reason_fields);
                wtr.write_record(&record)?;
            }
            None => {
                let mut record = vec![
                    row.name.clone(),
                    "-".to_string(),
                    "-".to_string(),
                    "-".to_string(),
                    "true".to_string(),
                ];
                record.extend(reason_fields);
                wtr.write_record(&record)?;
            }
        }
    }

    wtr.flush()?;
    Ok(())
}
