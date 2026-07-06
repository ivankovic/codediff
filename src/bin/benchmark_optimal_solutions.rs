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
use codediff::test::helper;
use codediff::test::helper::human_mapping;

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
}

struct Row {
    name: String,
    /// `None` means there's no `human_mapping.json` for this fixture yet (unsolved), as opposed
    /// to `Some(0)`, which means codediff matches the human mapping exactly.
    mismatches: Option<usize>,
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
        if !human_mapping::mapping_path(name).exists() {
            rows.push(Row {
                name: name.clone(),
                mismatches: None,
            });
            continue;
        }
        let mismatches = human_mapping::compute_mismatches(name)?;
        rows.push(Row {
            name: name.clone(),
            mismatches: Some(mismatches.len()),
        });
    }

    // Worst offenders first, so regressions/improvements are the first thing visible; unsolved
    // fixtures (nothing to compare against yet) sort after every solved one.
    rows.sort_by(|a, b| match (a.mismatches, b.mismatches) {
        (Some(x), Some(y)) => y.cmp(&x).then_with(|| a.name.cmp(&b.name)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.name.cmp(&b.name),
    });

    print_table(&rows);
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
        "{:<name_width$}  {:>10}  {:>13}",
        "Solution",
        "Mismatches",
        "Human Unsolved",
        name_width = name_width
    );
    println!("{}", "-".repeat(name_width + 2 + 10 + 2 + 13));

    let mut total_mismatches = 0usize;
    let mut total_unsolved = 0usize;
    for row in rows {
        match row.mismatches {
            Some(count) => {
                total_mismatches += count;
                println!(
                    "{:<name_width$}  {:>10}  {:>13}",
                    row.name,
                    count,
                    "",
                    name_width = name_width
                );
            }
            None => {
                total_unsolved += 1;
                println!(
                    "{:<name_width$}  {:>10}  {:>13}",
                    row.name,
                    "-",
                    "yes",
                    name_width = name_width
                );
            }
        }
    }

    println!("{}", "-".repeat(name_width + 2 + 10 + 2 + 13));
    println!(
        "{:<name_width$}  {:>10}  {:>13}",
        "TOTAL",
        total_mismatches,
        total_unsolved,
        name_width = name_width
    );
}
