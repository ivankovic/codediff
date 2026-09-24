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

//! Compares codediff against other diff tools (`ExternalTool`) at line granularity, against the
//! human mappings in `src/test/data/diffs/*/*/human_mapping.json`.
//!
//! Lines, because that is the only signal a line-based tool can produce: the human mapping and
//! codediff's `ASTDiff` are both projected down to per-line "touched or not" labels. That throws
//! away moves, so a fixture codediff gets node-perfect can still show line mismatches here.
//!
//! GumTree, difftastic and diffsitter are not bundled: point `GUMTREE_BIN`, `DIFFT_BIN` and
//! `DIFFSITTER_BIN` at built binaries. `treesitter_parse_ms` is timed as a reference lower bound,
//! and GumTree and BDiff each get a second, warm-process timing that excludes JVM or interpreter
//! startup (`gumtree_warm_batch`, `bdiff_warm_batch`).

use anyhow::{Context, Result, bail};
use clap::Parser;
use codediff::code::{Code, Language};
use codediff::diff;
use codediff::diff::text_range::TextRange;
use codediff::test::helper;
use codediff::test::helper::SampleProvenance;
use codediff::test::helper::human_mapping;
use csv::Writer;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;

#[path = "benchmark_other/bdiff.rs"]
mod bdiff;
#[path = "benchmark_other/diffsitter.rs"]
mod diffsitter;
#[path = "benchmark_other/difftastic.rs"]
mod difftastic;
#[path = "benchmark_other/git.rs"]
mod git;
#[path = "benchmark_other/gumtree.rs"]
mod gumtree;
#[path = "benchmark_other/nvim.rs"]
mod nvim;
use bdiff::*;
use diffsitter::*;
use difftastic::*;
use git::*;
use gumtree::*;
use nvim::*;

#[derive(Parser)]
struct Args {
    /// Print every before/after line where codediff or an external tool disagrees with the human
    /// mapping's touched/untouched call, for this one fixture, instead of the summary table.
    #[arg(long)]
    details: Option<String>,

    /// Output results as a CSV file. Default path: "./research/data/comparison/benchmark_other.csv"
    #[arg(long, value_name = "PATH", num_args = 0..=1)]
    csv: Option<Option<std::path::PathBuf>>,

    /// Accuracy-only run, no timing: score every tool's line and node agreement with the human
    /// mapping and write one row per fixture. Default path:
    /// "./research/data/comparison/benchmark_accuracy.csv".
    ///
    /// Node columns are a "did this node's text change" projection, not the node-to-node mapping
    /// fidelity `benchmark_optimal_solutions` reports: external tools parse their own trees, so
    /// codediff is scored through the same projection and is not comparable to its own
    /// optimal-solutions number. `*_visible_node_mismatches` restricts it to nodes that carry
    /// text of their own (`diff::nodes::is_structurally_visible`).
    // GumTree can emit a real node mapping, but its trees match tree-sitter's node for node only in
    // some languages (Java defaults to Eclipse JDT); a metric that answers for part of the corpus
    // is harder to read than one uniform projection. difftastic and diffsitter emit no node
    // correspondences at all.
    #[arg(long, value_name = "PATH", num_args = 0..=1)]
    accuracy_csv: Option<Option<std::path::PathBuf>>,

    /// How many times to repeat every timing measurement per fixture. Accuracy is deterministic
    /// and computed once; every repeat is written to the CSV so the spread stays visible.
    #[arg(long, default_value_t = 3)]
    repeats: usize,

    /// Only score these fixtures, by name, comma-separated. Default: every fixture with a human
    /// mapping. An unknown name is an error.
    #[arg(long, value_name = "NAMES", value_delimiter = ',')]
    fixtures: Vec<String>,

    /// Only score these tools, comma-separated: any `ExternalTool` name plus `codediff`. Default:
    /// all of them. Requires `--accuracy-csv`; an unknown name is an error.
    // Not offered for timing runs, whose tables and CSV header are built from `ExternalTool::ALL`.
    #[arg(long, value_name = "NAMES", value_delimiter = ',')]
    tools: Vec<String>,
}

/// Which tools an accuracy run scores. `None` means all of them. Names rather than
/// `ExternalTool`s because `codediff` is selectable too.
struct ToolSelection(Option<std::collections::HashSet<String>>);

impl ToolSelection {
    /// Errors on a name no tool answers to, listing the valid ones.
    fn parse(names: &[String]) -> Result<ToolSelection> {
        if names.is_empty() {
            return Ok(ToolSelection(None));
        }
        let known: Vec<&str> = std::iter::once("codediff")
            .chain(ExternalTool::ALL.iter().map(|tool| tool.name()))
            .collect();
        for name in names {
            if !known.contains(&name.as_str()) {
                bail!(
                    "unknown tool '{name}' - expected one of: {}",
                    known.join(", ")
                );
            }
        }
        Ok(ToolSelection(Some(names.iter().cloned().collect())))
    }

    fn includes(&self, name: &str) -> bool {
        self.0.as_ref().is_none_or(|wanted| wanted.contains(name))
    }
}

/// A diff tool scored against the human mapping. A new tool needs a variant here plus its arms in
/// `name`, `supports`, `line_labels` and `tool_node_spans`.
#[derive(Clone, Copy)]
enum ExternalTool {
    UnixDiff,
    GitMyers,
    GitMinimal,
    GitPatience,
    GitHistogram,
    BDiff,
    NvimDiff,
    GumTree,
    Difftastic,
    Diffsitter,
}

impl ExternalTool {
    const ALL: &'static [ExternalTool] = &[
        ExternalTool::UnixDiff,
        ExternalTool::GitMyers,
        ExternalTool::GitMinimal,
        ExternalTool::GitPatience,
        ExternalTool::GitHistogram,
        ExternalTool::BDiff,
        ExternalTool::NvimDiff,
        ExternalTool::GumTree,
        ExternalTool::Difftastic,
        ExternalTool::Diffsitter,
    ];

    fn name(&self) -> &'static str {
        match self {
            ExternalTool::UnixDiff => "unix_diff",
            ExternalTool::GitMyers => "git_myers",
            ExternalTool::GitMinimal => "git_minimal",
            ExternalTool::GitPatience => "git_patience",
            ExternalTool::GitHistogram => "git_histogram",
            ExternalTool::BDiff => "bdiff",
            ExternalTool::NvimDiff => "nvim_diff",
            ExternalTool::GumTree => "gumtree",
            ExternalTool::Difftastic => "difftastic",
            ExternalTool::Diffsitter => "diffsitter",
        }
    }

    /// The `--diff-algorithm` value for the four git variants, which share one labeller.
    fn git_algorithm(&self) -> Option<&'static str> {
        match self {
            ExternalTool::GitMyers => Some("myers"),
            ExternalTool::GitMinimal => Some("minimal"),
            ExternalTool::GitPatience => Some("patience"),
            ExternalTool::GitHistogram => Some("histogram"),
            _ => None,
        }
    }

    /// Whether this tool can be scored on `language`. Unsupported fixtures are skipped for the
    /// tool, not scored as a mismatch or counted in its totals.
    // GumTree covers every language with any generator, including those GumTree itself classifies
    // as Testing rather than Stable, so the comparison spans everything it can run on.
    fn supports(&self, language: Language) -> bool {
        match self {
            ExternalTool::UnixDiff
            | ExternalTool::GitMyers
            | ExternalTool::GitMinimal
            | ExternalTool::GitPatience
            | ExternalTool::GitHistogram
            | ExternalTool::BDiff
            | ExternalTool::NvimDiff => {
                let _ = language;
                true
            }
            ExternalTool::GumTree => gumtree_generator(language).is_some(),
            ExternalTool::Difftastic => difftastic_extension(language).is_some(),
            ExternalTool::Diffsitter => diffsitter_file_type(language).is_some(),
        }
    }

    /// `(before_touched, after_touched)`: one bool per line of each side, true where this tool
    /// considers the line part of the edit. Only meaningful when `supports` is true.
    fn line_labels(&self, before: &Code, after: &Code) -> Result<(Vec<bool>, Vec<bool>)> {
        match self {
            ExternalTool::UnixDiff => human_mapping::unix_diff_line_labels(before, after),
            ExternalTool::GitMyers
            | ExternalTool::GitMinimal
            | ExternalTool::GitPatience
            | ExternalTool::GitHistogram => git_line_labels(
                self.git_algorithm().expect("git variant has an algorithm"),
                before,
                after,
            ),
            ExternalTool::BDiff => bdiff_line_labels(before, after),
            ExternalTool::NvimDiff => nvim_line_labels(before, after),
            ExternalTool::GumTree => gumtree_line_labels(before, after),
            ExternalTool::Difftastic => difftastic_line_labels(before, after),
            ExternalTool::Diffsitter => diffsitter_line_labels(before, after),
        }
    }
}

/// The binary path in the `env_var` environment variable; `hint` says what to point it at when
/// it is unset.
fn external_tool_bin(env_var: &str, hint: &str) -> Result<std::path::PathBuf> {
    let path = std::env::var(env_var).with_context(|| format!("{env_var} is not set - {hint}"))?;
    let path = std::path::PathBuf::from(path);
    if !path.is_file() {
        bail!("{env_var}={:?} does not exist or is not a file", path);
    }
    Ok(path)
}

/// Writes both sides to fresh temp files. GumTree and difftastic key behaviour off the file
/// extension even when told the language, so they pass a `suffix`.
fn write_temp_pair(
    before: &Code,
    after: &Code,
    suffix: Option<&str>,
) -> Result<(tempfile::NamedTempFile, tempfile::NamedTempFile)> {
    let mut before_builder = tempfile::Builder::new();
    let mut after_builder = tempfile::Builder::new();
    if let Some(suffix) = suffix {
        before_builder.suffix(suffix);
        after_builder.suffix(suffix);
    }
    let mut before_file = before_builder
        .tempfile()
        .context("creating before temp file")?;
    let mut after_file = after_builder
        .tempfile()
        .context("creating after temp file")?;
    before_file
        .write_all(before.contents.as_bytes())
        .context("writing before temp file")?;
    after_file
        .write_all(after.contents.as_bytes())
        .context("writing after temp file")?;
    Ok((before_file, after_file))
}

/// Milliseconds tree-sitter takes to reparse `source` from scratch: a lower bound every AST-aware
/// tool pays before diffing, timed for context and never scored.
fn treesitter_parse_ms(source: &Code, parser: &mut tree_sitter::Parser) -> f64 {
    let mut code = source.clone();
    code.ast = None;
    let started = std::time::Instant::now();
    code.parse(parser);
    started.elapsed().as_secs_f64() * 1000.0
}

struct Row {
    name: String,
    /// (mismatched lines, total lines across both sides) against the human line labels.
    codediff: (usize, usize),
    /// One entry per `ExternalTool::ALL`; `None` when the tool does not support the language or
    /// failed on this fixture.
    tools: Vec<Option<(usize, usize)>>,
    /// Milliseconds from parsed `Code` to codediff's line labels, one per repeat.
    ///
    /// Excludes parsing (`main` parses every fixture first), while a tool's timing is its whole
    /// subprocess, so it is not comparable to `tool_ms` alone: add `treesitter_ms` for end to end.
    /// The two stay separate so both "algorithm only" and "end to end" are answerable.
    codediff_ms: Vec<f64>,
    /// Milliseconds inside `ExternalTool::line_labels`, one per repeat; `None` exactly where
    /// `tools` is.
    tool_ms: Vec<Option<Vec<f64>>>,
    /// `treesitter_parse_ms` of both sides summed, one per repeat.
    treesitter_ms: Vec<f64>,
    /// GumTree's own milliseconds in the persistent batch driver, one per repeat. `None` for every
    /// row when the driver is unavailable, or for this row when the language is out of scope.
    /// Accuracy is identical to the CLI `gumtree` entry, so only timing is recorded.
    gumtree_warm_ms: Option<Vec<f64>>,
    /// BDiff's own milliseconds in one persistent Python interpreter; same contract as
    /// `gumtree_warm_ms`, but BDiff has no language scope.
    bdiff_warm_ms: Option<Vec<f64>>,
}

fn score_fixture(
    name: &str,
    before: &Code,
    after: &Code,
    repeats: usize,
    gumtree_warm_ms: Option<Vec<f64>>,
    bdiff_warm_ms: Option<Vec<f64>>,
) -> Result<Row> {
    let language = before.metadata.language.unwrap_or_default();
    let (human_before, human_after, node_cache) =
        human_mapping::human_touched_lines_for(name, before, after)?;
    let total_lines = human_before.len() + human_after.len();

    // Accuracy is deterministic, so it is scored on the first repeat only.
    let mut codediff_mismatches = 0usize;
    let mut codediff_ms = Vec::with_capacity(repeats);
    for i in 0..repeats {
        let started = std::time::Instant::now();
        let codediff_diff = diff::diff_code(before, after);
        let codediff_ast = codediff_diff
            .ast
            .context("codediff produced no AST mapping")?;
        let (codediff_before, codediff_after) =
            human_mapping::touched_lines(before, after, &codediff_ast, &node_cache);
        codediff_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        if i == 0 {
            codediff_mismatches =
                human_mapping::line_disagreement_count(&human_before, &codediff_before)
                    + human_mapping::line_disagreement_count(&human_after, &codediff_after);
        }
    }

    let mut tools = Vec::with_capacity(ExternalTool::ALL.len());
    let mut tool_ms = Vec::with_capacity(ExternalTool::ALL.len());
    for tool in ExternalTool::ALL {
        if !tool.supports(language) {
            tools.push(None);
            tool_ms.push(None);
            continue;
        }
        let mut mismatches = 0usize;
        let mut ms = Vec::with_capacity(repeats);
        let mut failure = None;
        for i in 0..repeats {
            let started = std::time::Instant::now();
            // A failing tool is a coverage gap for this fixture, recorded as unscored; it must not
            // abort the corpus run.
            match tool.line_labels(before, after) {
                Ok((tool_before, tool_after)) => {
                    ms.push(started.elapsed().as_secs_f64() * 1000.0);
                    if i == 0 {
                        mismatches =
                            human_mapping::line_disagreement_count(&human_before, &tool_before)
                                + human_mapping::line_disagreement_count(&human_after, &tool_after);
                    }
                }
                Err(err) => {
                    failure = Some(err);
                    break;
                }
            }
        }
        if let Some(err) = failure {
            eprintln!("  {name}: {} failed, unscored: {err:#}", tool.name());
            tools.push(None);
            tool_ms.push(None);
            continue;
        }
        tool_ms.push(Some(ms));
        tools.push(Some((mismatches, total_lines)));
    }

    let mut parser = tree_sitter::Parser::new();
    let treesitter_ms = (0..repeats)
        .map(|_| treesitter_parse_ms(before, &mut parser) + treesitter_parse_ms(after, &mut parser))
        .collect();

    Ok(Row {
        name: name.to_string(),
        codediff: (codediff_mismatches, total_lines),
        tools,
        codediff_ms,
        tool_ms,
        treesitter_ms,
        gumtree_warm_ms,
        bdiff_warm_ms,
    })
}

/// `--details`: prints every line where codediff or a tool disagrees with the human mapping.
fn print_details(name: &str, before: &Code, after: &Code) -> Result<()> {
    let language = before.metadata.language.unwrap_or_default();
    let (human_before, human_after, node_cache) =
        human_mapping::human_touched_lines_for(name, before, after)?;

    let codediff_diff = diff::diff_code(before, after);
    let codediff_ast = codediff_diff
        .ast
        .context("codediff produced no AST mapping")?;
    let (codediff_before, codediff_after) =
        human_mapping::touched_lines(before, after, &codediff_ast, &node_cache);

    let mut sources: Vec<(&str, Vec<bool>, Vec<bool>)> =
        vec![("codediff", codediff_before, codediff_after)];
    for tool in ExternalTool::ALL {
        if !tool.supports(language) {
            println!("{}: does not support {:?}, skipped", tool.name(), language);
            continue;
        }
        let (tool_before, tool_after) = tool.line_labels(before, after)?;
        sources.push((tool.name(), tool_before, tool_after));
    }

    for (source_name, source_before, source_after) in &sources {
        for (side_name, human_side, source_side) in [
            ("before", &human_before, source_before),
            ("after", &human_after, source_after),
        ] {
            for (i, (h, s)) in human_side.iter().zip(source_side).enumerate() {
                if h != s {
                    println!(
                        "{source_name} {side_name}:{}: human={h} {source_name}={s}",
                        i + 1
                    );
                }
            }
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let selection = ToolSelection::parse(&args.tools)?;
    if selection.0.is_some() && args.accuracy_csv.is_none() {
        bail!("--tools only applies to --accuracy-csv runs");
    }
    // Load only the named fixtures: loading the whole corpus parses every fixture, which dominates
    // a scoped run. Cloned out of the shared `Arc` because `ensure_parsed` needs `&mut Code`.
    let mut test_diffs: HashMap<String, (Code, Code)> = if args.fixtures.is_empty() {
        (*helper::handmade_test_code_pairs()?).clone()
    } else {
        args.fixtures
            .iter()
            .map(|name| {
                let pair = (*helper::handmade_test_code_pair(name)
                    .with_context(|| format!("no fixture named '{name}'"))?)
                .clone();
                Ok((name.clone(), pair))
            })
            .collect::<Result<_>>()?
    };

    // `Code`'s `Clone` drops `ast_metadata`, and `metadata_of` never writes it back through `&Code`,
    // so without this every `diff_code` call in every repeat would recompute it.
    for (before, after) in test_diffs.values_mut() {
        if before.metadata.language.is_some() {
            before.ensure_parsed()?;
        }
        if after.metadata.language.is_some() {
            after.ensure_parsed()?;
        }
    }

    if let Some(name) = args.details {
        let (before, after) = test_diffs
            .get(&name)
            .with_context(|| format!("no fixture named '{}'", name))?;
        return print_details(&name, before, after);
    }

    let mut names: Vec<String> = test_diffs
        .keys()
        .filter(|name| human_mapping::mapping_path(name).exists())
        .cloned()
        .collect();
    names.sort();

    // Before the warm batch passes, which only exist to produce timings.
    if let Some(path) = args.accuracy_csv {
        let path = path.unwrap_or_else(|| {
            std::path::PathBuf::from("./research/data/comparison/benchmark_accuracy.csv")
        });
        return run_accuracy(&names, &test_diffs, &path, &selection);
    }

    let warm_fixtures: Vec<(&str, &Code, &Code)> = names
        .iter()
        .map(|name| {
            let (before, after) = test_diffs
                .get(name)
                .expect("name came from test_diffs.keys()");
            (name.as_str(), before, after)
        })
        .collect();
    let mut gumtree_warm_runs: Vec<HashMap<String, f64>> = Vec::with_capacity(args.repeats);
    for repeat in 0..args.repeats {
        eprintln!(
            "gumtree_warm_batch: repeat {}/{}...",
            repeat + 1,
            args.repeats
        );
        match gumtree_warm_batch(&warm_fixtures)? {
            Some(results) => gumtree_warm_runs.push(results),
            // Driver unavailable: every repeat would fail the same way.
            None => break,
        }
    }
    let gumtree_warm_available = gumtree_warm_runs.len() == args.repeats && args.repeats > 0;

    let mut bdiff_warm_runs: Vec<HashMap<String, f64>> = Vec::new();
    for repeat in 0..args.repeats {
        eprintln!(
            "bdiff_warm_batch: repeat {}/{}...",
            repeat + 1,
            args.repeats
        );
        match bdiff_warm_batch(&warm_fixtures)? {
            Some(results) => bdiff_warm_runs.push(results),
            None => break,
        }
    }
    let bdiff_warm_available = bdiff_warm_runs.len() == args.repeats && args.repeats > 0;

    let started = std::time::Instant::now();
    let mut rows = Vec::with_capacity(names.len());
    for (i, name) in names.iter().enumerate() {
        eprintln!("[{}/{}] {name}", i + 1, names.len());
        let (before, after) = test_diffs
            .get(name)
            .expect("name came from test_diffs.keys()");
        let warm_ms = gumtree_warm_available.then(|| {
            gumtree_warm_runs
                .iter()
                .filter_map(|results| results.get(name).copied())
                .collect::<Vec<f64>>()
        });
        // Out of GumTree's language scope: no entry in any repeat, so `None` rather than empty.
        let warm_ms = warm_ms.filter(|v| !v.is_empty());
        let bdiff_warm = bdiff_warm_available
            .then(|| {
                bdiff_warm_runs
                    .iter()
                    .filter_map(|results| results.get(name).copied())
                    .collect::<Vec<f64>>()
            })
            .filter(|v| !v.is_empty());
        // A fixture no grammar parses (e.g. `handmade/bazel-not-actually-supported-by-treesitter`)
        // is skipped, not allowed to abort the whole run.
        match score_fixture(name, before, after, args.repeats, warm_ms, bdiff_warm) {
            Ok(row) => rows.push(row),
            Err(err) => eprintln!("  {name}: skipped ({err:#})"),
        }
    }
    let elapsed = started.elapsed();

    rows.sort_by(|a, b| {
        b.codediff
            .0
            .cmp(&a.codediff.0)
            .then_with(|| a.name.cmp(&b.name))
    });

    if let Some(csv_path) = args.csv {
        let path = csv_path.unwrap_or_else(|| {
            std::path::PathBuf::from("./research/data/comparison/benchmark_other.csv")
        });
        write_csv(&rows, &path)?;
    }

    print_table(&rows);
    print_runtime_table(&rows);
    println!(
        "\nHarness runtime: {:.3}s total, {:.1}ms/fixture ({} fixtures) - includes scoring/projection overhead on\ntop of the per-tool times in the table above, which time only each tool's own work.",
        elapsed.as_secs_f64(),
        elapsed.as_secs_f64() * 1000.0 / rows.len().max(1) as f64,
        rows.len()
    );
    Ok(())
}

fn print_table(rows: &[Row]) {
    let name_width = rows
        .iter()
        .map(|r| r.name.len())
        .chain(["Solution".len()])
        .max()
        .unwrap_or(0);
    let tool_names: Vec<&str> = ExternalTool::ALL.iter().map(|t| t.name()).collect();

    print!(
        "{:<name_width$}  {:>9}  {:>7}",
        "Solution",
        "codediff",
        "cd %",
        name_width = name_width
    );
    for tool_name in &tool_names {
        print!("  {:>9}  {:>7}", tool_name, format!("{tool_name} %"));
    }
    println!();
    let rule_width = name_width + (2 + 9 + 2 + 7) * (1 + tool_names.len());
    println!("{}", "-".repeat(rule_width));

    let mut codediff_total = (0usize, 0usize);
    // (mismatches, total lines, fixtures scored): the count tells "0 mismatches everywhere" apart
    // from "out of scope almost everywhere".
    let mut tool_totals = vec![(0usize, 0usize, 0usize); tool_names.len()];
    for row in rows {
        print!(
            "{:<name_width$}  {:>9}  {:>6.2}%",
            row.name,
            row.codediff.0,
            pct(row.codediff.0, row.codediff.1),
            name_width = name_width
        );
        codediff_total.0 += row.codediff.0;
        codediff_total.1 += row.codediff.1;
        for (i, cell) in row.tools.iter().enumerate() {
            match *cell {
                Some((mismatches, total)) => {
                    print!("  {:>9}  {:>6.2}%", mismatches, pct(mismatches, total));
                    tool_totals[i].0 += mismatches;
                    tool_totals[i].1 += total;
                    tool_totals[i].2 += 1;
                }
                None => print!("  {:>9}  {:>7}", "-", "-"),
            }
        }
        println!();
    }

    println!("{}", "-".repeat(rule_width));
    print!(
        "{:<name_width$}  {:>9}  {:>6.2}%",
        "TOTAL",
        codediff_total.0,
        pct(codediff_total.0, codediff_total.1),
        name_width = name_width
    );
    for &(mismatches, total, _) in &tool_totals {
        print!("  {:>9}  {:>6.2}%", mismatches, pct(mismatches, total));
    }
    println!();

    for (tool_name, &(_, _, scored)) in tool_names.iter().zip(&tool_totals) {
        if scored < rows.len() {
            println!(
                "  ({tool_name} scored on {scored}/{} fixtures - the rest are outside its language scope)",
                rows.len()
            );
        }
    }
}

fn pct(mismatches: usize, total: usize) -> f64 {
    if total > 0 {
        100.0 * mismatches as f64 / total as f64
    } else {
        0.0
    }
}

/// Mean over fixtures of each fixture's coefficient of variation (stddev / mean, in percent)
/// across its repeats. Fixtures with fewer than 2 repeats contribute nothing; `None` if none remain.
fn mean_coefficient_of_variation<'a>(samples: impl Iterator<Item = &'a [f64]>) -> Option<f64> {
    let cvs: Vec<f64> = samples
        .filter(|s| s.len() >= 2)
        .filter_map(|s| {
            let mean = s.iter().sum::<f64>() / s.len() as f64;
            if mean <= 0.0 {
                return None;
            }
            let variance = s.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / s.len() as f64;
            Some(100.0 * variance.sqrt() / mean)
        })
        .collect();
    if cvs.is_empty() {
        None
    } else {
        Some(cvs.iter().sum::<f64>() / cvs.len() as f64)
    }
}

fn print_runtime_table(rows: &[Row]) {
    let tool_names: Vec<&str> = ExternalTool::ALL.iter().map(|t| t.name()).collect();
    let label_width = ["codediff", "treesitter_parse", "gumtree_warm", "bdiff_warm"]
        .iter()
        .chain(&tool_names)
        .map(|s| s.len())
        .max()
        .unwrap_or(0);

    println!();
    println!(
        "Per-tool runtime (time to produce line-level touched/untouched labels, {} repeat(s)/fixture):",
        rows.first().map(|r| r.codediff_ms.len()).unwrap_or(0)
    );
    println!(
        "{:<label_width$}  {:>10}  {:>10}  {:>8}",
        "Tool",
        "Total ms",
        "Mean ms",
        "CoV %",
        label_width = label_width
    );
    println!("{}", "-".repeat(label_width + 2 + 10 + 2 + 10 + 2 + 8));

    // Total and mean flatten every repeat into one sample; CoV is per fixture, then averaged.
    let treesitter_flat: Vec<f64> = rows
        .iter()
        .flat_map(|r| r.treesitter_ms.iter().copied())
        .collect();
    let treesitter_total: f64 = treesitter_flat.iter().sum();
    println!(
        "{:<label_width$}  {:>10.1}  {:>10.3}  {:>7}  (n={})  <- tree-sitter parse only, reference lower bound",
        "treesitter_parse",
        treesitter_total,
        treesitter_total / treesitter_flat.len().max(1) as f64,
        mean_coefficient_of_variation(rows.iter().map(|r| r.treesitter_ms.as_slice()))
            .map(|cv| format!("{cv:.1}"))
            .unwrap_or_else(|| "-".to_string()),
        treesitter_flat.len(),
        label_width = label_width
    );

    let codediff_flat: Vec<f64> = rows
        .iter()
        .flat_map(|r| r.codediff_ms.iter().copied())
        .collect();
    let codediff_total: f64 = codediff_flat.iter().sum();
    println!(
        "{:<label_width$}  {:>10.1}  {:>10.3}  {:>7}  (n={})",
        "codediff",
        codediff_total,
        codediff_total / codediff_flat.len().max(1) as f64,
        mean_coefficient_of_variation(rows.iter().map(|r| r.codediff_ms.as_slice()))
            .map(|cv| format!("{cv:.1}"))
            .unwrap_or_else(|| "-".to_string()),
        codediff_flat.len(),
        label_width = label_width
    );
    for (i, tool_name) in tool_names.iter().enumerate() {
        // Mean over scored fixtures only, not every row, or out-of-scope rows would count as 0ms.
        let scored: Vec<&[f64]> = rows
            .iter()
            .filter_map(|r| r.tool_ms[i].as_deref())
            .collect();
        let flat: Vec<f64> = scored.iter().flat_map(|s| s.iter().copied()).collect();
        let total: f64 = flat.iter().sum();
        println!(
            "{:<label_width$}  {:>10.1}  {:>10.3}  {:>7}  (n={})",
            tool_name,
            total,
            total / flat.len().max(1) as f64,
            mean_coefficient_of_variation(scored.iter().copied())
                .map(|cv| format!("{cv:.1}"))
                .unwrap_or_else(|| "-".to_string()),
            flat.len(),
            label_width = label_width
        );
    }

    let warm: Vec<&[f64]> = rows
        .iter()
        .filter_map(|r| r.gumtree_warm_ms.as_deref())
        .collect();
    let warm_flat: Vec<f64> = warm.iter().flat_map(|s| s.iter().copied()).collect();
    if !warm_flat.is_empty() {
        let total: f64 = warm_flat.iter().sum();
        println!(
            "{:<label_width$}  {:>10.1}  {:>10.3}  {:>7}  (n={})  <- same algorithm as gumtree, warm JVM (see research/drivers/gumtree-batch)",
            "gumtree_warm",
            total,
            total / warm_flat.len() as f64,
            mean_coefficient_of_variation(warm.iter().copied())
                .map(|cv| format!("{cv:.1}"))
                .unwrap_or_else(|| "-".to_string()),
            warm_flat.len(),
            label_width = label_width
        );
    }

    let bdiff_warm: Vec<&[f64]> = rows
        .iter()
        .filter_map(|r| r.bdiff_warm_ms.as_deref())
        .collect();
    let bdiff_flat: Vec<f64> = bdiff_warm.iter().flat_map(|s| s.iter().copied()).collect();
    if !bdiff_flat.is_empty() {
        let total: f64 = bdiff_flat.iter().sum();
        println!(
            "{:<label_width$}  {:>10.1}  {:>10.3}  {:>7}  (n={})  <- same algorithm as bdiff, warm interpreter",
            "bdiff_warm",
            total,
            total / bdiff_flat.len() as f64,
            mean_coefficient_of_variation(bdiff_warm.iter().copied())
                .map(|cv| format!("{cv:.1}"))
                .unwrap_or_else(|| "-".to_string()),
            bdiff_flat.len(),
            label_width = label_width
        );
    }
}

/// Every repeat's value in run order, joined by `;`, so the CSV has one column per metric
/// whatever `--repeats` is. `benchmark_other_report.py` splits it back.
fn join_ms(values: &[f64]) -> String {
    values
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(";")
}

/// For every character offset (plus one past the end), its `(row, byte column)` - the space
/// `TextRange` uses. GumTree reports character offsets.
fn char_offset_table(contents: &str) -> Vec<(usize, usize)> {
    let mut table = Vec::with_capacity(contents.chars().count() + 1);
    let (mut row, mut col) = (0usize, 0usize);
    for ch in contents.chars() {
        table.push((row, col));
        if ch == '\n' {
            row += 1;
            col = 0;
        } else {
            col += ch.len_utf8();
        }
    }
    table.push((row, col));
    table
}

/// A `TextRange` for the half-open character range `[start, end)`, via `char_offset_table`.
fn span_from_char_offsets(table: &[(usize, usize)], start: usize, end: usize) -> TextRange {
    let at = |i: usize| -> (usize, usize) {
        *table
            .get(i)
            .or_else(|| table.last())
            .unwrap_or(&(0usize, 0usize))
    };
    let (start_row, start_column) = at(start);
    let (end_row, end_column) = at(end);
    TextRange {
        start_row,
        start_column,
        end_row,
        end_column,
    }
}

/// A single-line `TextRange` covering byte columns `[start, end)` on `row`.
fn span_on_row(row: usize, start: usize, end: usize) -> TextRange {
    TextRange {
        start_row: row,
        start_column: start,
        end_row: row,
        end_column: end.max(start),
    }
}

/// Each tool's changed regions as `(before_spans, after_spans)`, in `human_mapping::node_extents`'
/// row/byte-column space. `None` for the line-only tools: projecting whole lines onto nodes would
/// mark every node on a changed line, a different question rather than a worse score.
///
/// difftastic and diffsitter report byte columns and need no conversion; GumTree and BDiff report
/// character offsets and do. A mix-up is invisible on ASCII input.
fn tool_node_spans(
    tool: ExternalTool,
    before: &Code,
    after: &Code,
) -> Option<Result<(Vec<TextRange>, Vec<TextRange>)>> {
    match tool {
        ExternalTool::UnixDiff
        | ExternalTool::GitMyers
        | ExternalTool::GitMinimal
        | ExternalTool::GitPatience
        | ExternalTool::GitHistogram => None,
        // Not line-only: BDiff's `str_diff` has character offsets and Neovim paints `DiffText` per
        // column.
        ExternalTool::BDiff => Some(bdiff_node_spans(before, after)),
        ExternalTool::NvimDiff => Some(nvim_node_spans(before, after)),
        ExternalTool::GumTree => Some(gumtree_node_spans(before, after)),
        ExternalTool::Difftastic => Some(difftastic_node_spans(before, after)),
        ExternalTool::Diffsitter => Some(diffsitter_node_spans(before, after)),
    }
}

/// The `TextRange` covering characters `[start_char, end_char)` of `row`, converted to byte
/// columns; offsets past the line clamp to its end. For BDiff, whose offsets index a Python `str`.
fn span_on_row_chars(lines: &[&str], row: usize, start_char: usize, end_char: usize) -> TextRange {
    let line = lines.get(row).copied().unwrap_or("");
    let byte_at = |char_index: usize| -> usize {
        line.char_indices()
            .nth(char_index)
            .map(|(byte, _)| byte)
            .unwrap_or(line.len())
    };
    span_on_row(row, byte_at(start_char), byte_at(end_char))
}

/// The `TextRange` covering all of `row`.
fn whole_row_span(lines: &[&str], row: usize) -> TextRange {
    span_on_row(row, 0, lines.get(row).copied().unwrap_or("").len())
}

/// Sorts and coalesces overlapping or adjacent spans without changing what they cover. Only a
/// cost optimization: diffsitter reports one span per character.
fn merge_spans(mut spans: Vec<TextRange>) -> Vec<TextRange> {
    if spans.is_empty() {
        return spans;
    }
    spans.sort_by_key(|s| (s.start_row, s.start_column, s.end_row, s.end_column));
    let mut merged: Vec<TextRange> = Vec::with_capacity(spans.len());
    for span in spans {
        match merged.last_mut() {
            Some(last)
                if (span.start_row, span.start_column) <= (last.end_row, last.end_column) =>
            {
                if (span.end_row, span.end_column) > (last.end_row, last.end_column) {
                    last.end_row = span.end_row;
                    last.end_column = span.end_column;
                }
            }
            _ => merged.push(span),
        }
    }
    merged
}

/// One fixture's accuracy row; see `Args::accuracy_csv`.
struct AccuracyRow {
    solution: String,
    /// Provenance from `src/test/data/sample.csv`; blank for handmade fixtures never promoted from
    /// a sample.
    language: String,
    repository: String,
    commit: String,
    path: String,
    total_lines: usize,
    total_nodes: usize,
    total_leaf_nodes: usize,
    /// Denominator for the `*_visible_node_mismatches` columns.
    total_visible_nodes: usize,
    /// codediff first (if selected), then `ExternalTool::ALL` order.
    scores: Vec<ToolScore>,
}

/// One tool's agreement with the human mapping on one fixture.
struct ToolScore {
    name: &'static str,
    line_mismatches: Option<usize>,
    node_mismatches: Option<usize>,
    leaf_node_mismatches: Option<usize>,
    visible_node_mismatches: Option<usize>,
    /// `ok`, `unsupported` (no parser for this language; deliberately not scored 0, which reads
    /// as perfect), `error`, or `line_only` (no sub-line output, so no node columns).
    status: &'static str,
}

/// Scores every selected tool's line- and node-level agreement with the human mapping. Both
/// granularities derive from one synthetic `ASTDiff` (`human_mapping::as_ast_diff`).
fn score_accuracy(
    name: &str,
    before: &Code,
    after: &Code,
    provenance: &HashMap<String, SampleProvenance>,
    selection: &ToolSelection,
) -> Result<AccuracyRow> {
    let node_cache = codediff::diff::NodeCache::build(before, after);
    let truth_ast = human_mapping::as_ast_diff(name, before, after)?;

    let (truth_before_lines, truth_after_lines) =
        human_mapping::touched_lines(before, after, &truth_ast, &node_cache);
    let (truth_before_spans, truth_after_spans) =
        human_mapping::changed_spans(before, after, &truth_ast, &node_cache);

    let before_extents = human_mapping::node_extents(before);
    let after_extents = human_mapping::node_extents(after);
    let truth_before_nodes = human_mapping::nodes_touched_by(&before_extents, &truth_before_spans);
    let truth_after_nodes = human_mapping::nodes_touched_by(&after_extents, &truth_after_spans);

    // Every ancestor of a change counts as touched, so leaves separate "which tokens changed" from
    // "how deep is this grammar's tree".
    let leaf_filter = |labels: &[bool], extents: &[human_mapping::NodeExtent]| -> Vec<bool> {
        labels
            .iter()
            .zip(extents)
            .filter(|(_, extent)| extent.is_leaf)
            .map(|(touched, _)| *touched)
            .collect()
    };
    let truth_before_leaves = leaf_filter(&truth_before_nodes, &before_extents);
    let truth_after_leaves = leaf_filter(&truth_after_nodes, &after_extents);

    // Nodes whose classification reaches the screen. Visibility is structural and judged on the
    // source alone, so every tool is scored against one fixed set; judging each tool by its own
    // rendering gives each a different denominator, and judging by codediff's privileges codediff.
    // Neither a subset nor a superset of the leaf view.
    let before_visible_ids = codediff::diff::nodes::structurally_visible_node_ids(before);
    let after_visible_ids = codediff::diff::nodes::structurally_visible_node_ids(after);
    let visible_filter = |labels: &[bool],
                          extents: &[human_mapping::NodeExtent],
                          visible: &std::collections::HashSet<usize>|
     -> Vec<bool> {
        labels
            .iter()
            .zip(extents)
            .filter(|(_, extent)| visible.contains(&extent.node_id))
            .map(|(touched, _)| *touched)
            .collect()
    };
    let truth_before_visible =
        visible_filter(&truth_before_nodes, &before_extents, &before_visible_ids);
    let truth_after_visible =
        visible_filter(&truth_after_nodes, &after_extents, &after_visible_ids);

    let disagreement = human_mapping::line_disagreement_count;
    let mut scores = Vec::with_capacity(ExternalTool::ALL.len() + 1);

    if selection.includes("codediff") {
        let diff = codediff::diff::diff_code(before, after);
        let ast = diff.ast.as_ref().context("codediff produced no AST")?;
        let (cd_before_lines, cd_after_lines) =
            human_mapping::touched_lines(before, after, ast, &node_cache);
        let (cd_before_spans, cd_after_spans) =
            human_mapping::changed_spans(before, after, ast, &node_cache);
        let cd_before_nodes = human_mapping::nodes_touched_by(&before_extents, &cd_before_spans);
        let cd_after_nodes = human_mapping::nodes_touched_by(&after_extents, &cd_after_spans);
        scores.push(ToolScore {
            name: "codediff",
            line_mismatches: Some(
                disagreement(&truth_before_lines, &cd_before_lines)
                    + disagreement(&truth_after_lines, &cd_after_lines),
            ),
            node_mismatches: Some(
                disagreement(&truth_before_nodes, &cd_before_nodes)
                    + disagreement(&truth_after_nodes, &cd_after_nodes),
            ),
            leaf_node_mismatches: Some(
                disagreement(
                    &truth_before_leaves,
                    &leaf_filter(&cd_before_nodes, &before_extents),
                ) + disagreement(
                    &truth_after_leaves,
                    &leaf_filter(&cd_after_nodes, &after_extents),
                ),
            ),
            visible_node_mismatches: Some(
                disagreement(
                    &truth_before_visible,
                    &visible_filter(&cd_before_nodes, &before_extents, &before_visible_ids),
                ) + disagreement(
                    &truth_after_visible,
                    &visible_filter(&cd_after_nodes, &after_extents, &after_visible_ids),
                ),
            ),
            status: "ok",
        });
    }

    let language = before.metadata.language.unwrap_or_default();
    for &tool in ExternalTool::ALL {
        if !selection.includes(tool.name()) {
            continue;
        }
        if !tool.supports(language) {
            scores.push(ToolScore {
                name: tool.name(),
                line_mismatches: None,
                node_mismatches: None,
                leaf_node_mismatches: None,
                visible_node_mismatches: None,
                status: "unsupported",
            });
            continue;
        }
        let line_mismatches = match tool.line_labels(before, after) {
            Ok((tool_before, tool_after)) => Some(
                disagreement(&truth_before_lines, &tool_before)
                    + disagreement(&truth_after_lines, &tool_after),
            ),
            Err(err) => {
                eprintln!("  {name}: {} line scoring failed: {err:#}", tool.name());
                None
            }
        };
        let node_result = tool_node_spans(tool, before, after);
        let (node_mismatches, leaf_node_mismatches, visible_node_mismatches, status) =
            match node_result {
                None => (None, None, None, "line_only"),
                Some(Ok((tool_before_spans, tool_after_spans))) => {
                    let tb = human_mapping::nodes_touched_by(&before_extents, &tool_before_spans);
                    let ta = human_mapping::nodes_touched_by(&after_extents, &tool_after_spans);
                    (
                        Some(
                            disagreement(&truth_before_nodes, &tb)
                                + disagreement(&truth_after_nodes, &ta),
                        ),
                        Some(
                            disagreement(&truth_before_leaves, &leaf_filter(&tb, &before_extents))
                                + disagreement(
                                    &truth_after_leaves,
                                    &leaf_filter(&ta, &after_extents),
                                ),
                        ),
                        Some(
                            disagreement(
                                &truth_before_visible,
                                &visible_filter(&tb, &before_extents, &before_visible_ids),
                            ) + disagreement(
                                &truth_after_visible,
                                &visible_filter(&ta, &after_extents, &after_visible_ids),
                            ),
                        ),
                        if line_mismatches.is_some() {
                            "ok"
                        } else {
                            "error"
                        },
                    )
                }
                Some(Err(err)) => {
                    eprintln!("  {name}: {} node scoring failed: {err:#}", tool.name());
                    (None, None, None, "error")
                }
            };
        scores.push(ToolScore {
            name: tool.name(),
            line_mismatches,
            node_mismatches,
            leaf_node_mismatches,
            visible_node_mismatches,
            status,
        });
    }

    let SampleProvenance {
        repository,
        commit,
        path,
        ..
    } = provenance.get(name).cloned().unwrap_or_default();

    Ok(AccuracyRow {
        solution: name.to_string(),
        language: format!("{language:?}"),
        repository,
        commit,
        path,
        total_lines: before.contents.split('\n').count() + after.contents.split('\n').count(),
        total_nodes: before_extents.len() + after_extents.len(),
        total_leaf_nodes: before_extents.iter().filter(|e| e.is_leaf).count()
            + after_extents.iter().filter(|e| e.is_leaf).count(),
        total_visible_nodes: truth_before_visible.len() + truth_after_visible.len(),
        scores,
    })
}

fn write_accuracy_csv(rows: &[AccuracyRow], path: &std::path::Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {parent:?} for the accuracy CSV"))?;
    }
    let mut writer =
        Writer::from_path(path).with_context(|| format!("creating accuracy CSV at {path:?}"))?;

    let mut header = vec![
        "solution".to_string(),
        "language".to_string(),
        "repository".to_string(),
        "commit".to_string(),
        "path".to_string(),
        "total_lines".to_string(),
        "total_nodes".to_string(),
        "total_leaf_nodes".to_string(),
        "total_visible_nodes".to_string(),
    ];
    let tool_names: Vec<&str> = rows
        .first()
        .map(|row| row.scores.iter().map(|s| s.name).collect())
        .unwrap_or_default();
    for name in &tool_names {
        header.push(format!("{name}_line_mismatches"));
        header.push(format!("{name}_node_mismatches"));
        header.push(format!("{name}_leaf_node_mismatches"));
        header.push(format!("{name}_visible_node_mismatches"));
        header.push(format!("{name}_status"));
    }
    writer.write_record(&header)?;

    for row in rows {
        let mut record = vec![
            row.solution.clone(),
            row.language.clone(),
            row.repository.clone(),
            row.commit.clone(),
            row.path.clone(),
            row.total_lines.to_string(),
            row.total_nodes.to_string(),
            row.total_leaf_nodes.to_string(),
            row.total_visible_nodes.to_string(),
        ];
        let cell = |value: Option<usize>| value.map(|v| v.to_string()).unwrap_or_default();
        for score in &row.scores {
            record.push(cell(score.line_mismatches));
            record.push(cell(score.node_mismatches));
            record.push(cell(score.leaf_node_mismatches));
            record.push(cell(score.visible_node_mismatches));
            record.push(score.status.to_string());
        }
        writer.write_record(&record)?;
    }
    writer.flush()?;
    Ok(())
}

/// `--accuracy-csv`: scores every fixture, prints per-tool summaries, and writes the CSV.
fn run_accuracy(
    names: &[String],
    test_diffs: &HashMap<String, (Code, Code)>,
    path: &std::path::Path,
    selection: &ToolSelection,
) -> Result<()> {
    let provenance = helper::sample_provenance()?;
    let mut rows = Vec::with_capacity(names.len());
    for (index, name) in names.iter().enumerate() {
        if index % 25 == 0 {
            eprintln!("accuracy: {}/{}...", index, names.len());
        }
        let (before, after) = test_diffs
            .get(name)
            .expect("name came from test_diffs.keys()");
        match score_accuracy(name, before, after, &provenance, selection) {
            Ok(row) => rows.push(row),
            Err(err) => eprintln!("  {name}: skipped ({err:#})"),
        }
    }

    let tool_names: Vec<&str> = rows
        .first()
        .map(|row| row.scores.iter().map(|s| s.name).collect())
        .unwrap_or_default();

    // Coverage first: each tool scores only the fixtures it supports, so raw totals across tools
    // are not comparable.
    println!(
        "\n{:<14} {:>7} {:>7} {:>13} {:>10} {:>12} {:>12} {:>12}",
        "tool", "ok", "err", "unsupported", "line mm", "node mm", "leaf mm", "visible mm"
    );
    for (i, name) in tool_names.iter().enumerate() {
        let scores: Vec<&ToolScore> = rows.iter().filter_map(|row| row.scores.get(i)).collect();
        let count = |status: &str| scores.iter().filter(|s| s.status == status).count();
        let sum = |f: fn(&ToolScore) -> Option<usize>| -> usize {
            scores.iter().filter_map(|s| f(s)).sum()
        };
        let has_nodes = scores.iter().any(|s| s.node_mismatches.is_some());
        let cell = |v: usize| {
            if has_nodes {
                v.to_string()
            } else {
                "-".to_string()
            }
        };
        println!(
            "{name:<14} {:>7} {:>7} {:>13} {:>10} {:>12} {:>12} {:>12}",
            count("ok") + count("line_only"),
            count("error"),
            count("unsupported"),
            sum(|s| s.line_mismatches),
            cell(sum(|s| s.node_mismatches)),
            cell(sum(|s| s.leaf_node_mismatches)),
            cell(sum(|s| s.visible_node_mismatches)),
        );
    }

    // The only apples-to-apples table: fixtures every tool scored.
    let common: Vec<&AccuracyRow> = rows
        .iter()
        .filter(|row| {
            row.scores
                .iter()
                .all(|s| s.status == "ok" || s.status == "line_only")
        })
        .collect();
    let total_lines: usize = common.iter().map(|r| r.total_lines).sum();
    let total_nodes: usize = common.iter().map(|r| r.total_nodes).sum();
    let total_leaves: usize = common.iter().map(|r| r.total_leaf_nodes).sum();
    let total_visible: usize = common.iter().map(|r| r.total_visible_nodes).sum();
    println!(
        "\nCommon subset - the {} of {} fixtures every tool scored ({total_lines} lines, \
         {total_nodes} nodes, {total_leaves} leaf nodes, {total_visible} visible nodes):",
        common.len(),
        rows.len()
    );
    println!(
        "{:<14} {:>10} {:>8} {:>12} {:>8} {:>12} {:>8} {:>12} {:>8}",
        "tool", "line mm", "rate", "node mm", "rate", "leaf mm", "rate", "visible mm", "rate"
    );
    let rate = |value: usize, total: usize| {
        if total == 0 {
            0.0
        } else {
            100.0 * value as f64 / total as f64
        }
    };
    for (i, name) in tool_names.iter().enumerate() {
        let scores: Vec<&ToolScore> = common.iter().filter_map(|row| row.scores.get(i)).collect();
        let sum = |f: fn(&ToolScore) -> Option<usize>| -> usize {
            scores.iter().filter_map(|s| f(s)).sum()
        };
        let (lines, nodes, leaves, visible) = (
            sum(|s| s.line_mismatches),
            sum(|s| s.node_mismatches),
            sum(|s| s.leaf_node_mismatches),
            sum(|s| s.visible_node_mismatches),
        );
        let has_nodes = scores.iter().any(|s| s.node_mismatches.is_some());
        if has_nodes {
            println!(
                "{name:<14} {lines:>10} {:>7.2}% {nodes:>12} {:>7.2}% {leaves:>12} {:>7.2}% \
                 {visible:>12} {:>7.2}%",
                rate(lines, total_lines),
                rate(nodes, total_nodes),
                rate(leaves, total_leaves),
                rate(visible, total_visible),
            );
        } else {
            println!(
                "{name:<14} {lines:>10} {:>7.2}% {:>12} {:>8} {:>12} {:>8} {:>12} {:>8}",
                rate(lines, total_lines),
                "-",
                "-",
                "-",
                "-",
                "-",
                "-",
            );
        }
    }

    write_accuracy_csv(&rows, path)?;
    println!("\nWrote {}", path.display());
    Ok(())
}

fn write_csv(rows: &[Row], path: &std::path::Path) -> Result<()> {
    let file = File::create(path)?;
    let mut wtr = Writer::from_writer(file);

    let mut header = vec![
        "solution".to_string(),
        "total_lines".to_string(),
        "codediff_mismatches".to_string(),
        "codediff_ms".to_string(),
        "treesitter_parse_ms".to_string(),
    ];
    header.extend(ExternalTool::ALL.iter().flat_map(|t| {
        [
            format!("{}_mismatches", t.name()),
            format!("{}_ms", t.name()),
        ]
    }));
    header.push("gumtree_warm_ms".to_string());
    header.push("bdiff_warm_ms".to_string());
    wtr.write_record(&header)?;

    for row in rows {
        let mut record = vec![
            row.name.clone(),
            row.codediff.1.to_string(),
            row.codediff.0.to_string(),
            join_ms(&row.codediff_ms),
            join_ms(&row.treesitter_ms),
        ];
        // Blank, not 0, for an unscored tool: `benchmark_other_report.py` excludes blanks.
        record.extend(row.tools.iter().zip(&row.tool_ms).flat_map(|(cell, ms)| {
            [
                cell.map(|(mismatches, _)| mismatches.to_string())
                    .unwrap_or_default(),
                ms.as_deref().map(join_ms).unwrap_or_default(),
            ]
        }));
        record.push(
            row.gumtree_warm_ms
                .as_deref()
                .map(join_ms)
                .unwrap_or_default(),
        );
        record.push(
            row.bdiff_warm_ms
                .as_deref()
                .map(join_ms)
                .unwrap_or_default(),
        );
        wtr.write_record(&record)?;
    }
    wtr.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gumtree_node_offsets_parses_the_trailing_bracketed_range() {
        assert_eq!(
            gumtree_node_offsets("SimpleName: foo [12,15]").unwrap(),
            (12, 15)
        );
    }

    #[test]
    fn gumtree_node_offsets_errors_without_a_bracketed_suffix() {
        assert!(gumtree_node_offsets("SimpleName: foo").is_err());
    }

    #[test]
    fn gumtree_line_range_finds_the_single_line_a_small_range_sits_on() {
        let contents = "line0\nline1\nline2\n";
        // "line1" starts at character offset 6, ends at offset 11.
        assert_eq!(gumtree_line_range(contents, 6, 11), 1..=1);
    }

    #[test]
    fn gumtree_line_range_spans_every_line_a_range_crosses() {
        let contents = "line0\nline1\nline2\n";
        // From partway through "line0" to partway through "line2".
        assert_eq!(gumtree_line_range(contents, 2, 15), 0..=2);
    }

    #[test]
    fn gumtree_line_range_does_not_pull_in_the_next_line_when_end_lands_on_a_boundary() {
        let contents = "line0\nline1\nline2\n";
        // end == 6 is the newline after "line0"; `end` is exclusive.
        assert_eq!(gumtree_line_range(contents, 0, 6), 0..=0);
    }

    /// GumTree's offsets are characters; slicing `contents` at them as bytes panics past
    /// multi-byte text.
    #[test]
    fn gumtree_line_range_handles_multi_byte_utf8_text_before_the_touched_range() {
        // 6 characters, 18 bytes: byte index 7 is inside a character.
        let contents = "สวัสดี\nhello\nworld\n";
        assert_eq!(gumtree_line_range(contents, 7, 9), 1..=1);
    }

    #[test]
    fn gumtree_line_range_clamps_an_end_offset_past_the_end_of_the_file() {
        let contents = "line0\nline1\n";
        let total_chars = contents.chars().count();
        assert_eq!(gumtree_line_range(contents, 6, total_chars), 1..=1);
    }

    #[test]
    fn char_offset_table_maps_char_offsets_to_rows_and_byte_columns() {
        let table = char_offset_table("ab\ncd");
        assert_eq!(table[0], (0, 0));
        assert_eq!(table[1], (0, 1));
        assert_eq!(table[2], (0, 2)); // the '\n' itself
        assert_eq!(table[3], (1, 0));
        assert_eq!(table[4], (1, 1));
        assert_eq!(table[5], (1, 2), "one-past-the-end entry");

        // 'é' is one character but two bytes, so the column after it advances by 2.
        let table = char_offset_table("é x");
        assert_eq!(table[0], (0, 0));
        assert_eq!(
            table[1],
            (0, 2),
            "column is a byte offset, not a char index"
        );
        assert_eq!(table[2], (0, 3));
    }

    #[test]
    fn span_from_char_offsets_clamps_past_the_end_instead_of_panicking() {
        let table = char_offset_table("ab");
        let span = span_from_char_offsets(&table, 0, 99);
        assert_eq!((span.start_row, span.start_column), (0, 0));
        assert_eq!((span.end_row, span.end_column), (0, 2));
    }

    #[test]
    fn merge_spans_coalesces_adjacent_and_overlapping_spans_but_keeps_gaps() {
        let merged = merge_spans(vec![
            span_on_row(0, 4, 5),
            span_on_row(0, 0, 2),
            span_on_row(0, 2, 4), // adjacent, not overlapping
            span_on_row(1, 0, 3),
            span_on_row(1, 1, 9), // overlaps the previous
        ]);
        let as_tuples: Vec<_> = merged
            .iter()
            .map(|s| (s.start_row, s.start_column, s.end_row, s.end_column))
            .collect();
        assert_eq!(as_tuples, vec![(0, 0, 0, 5), (1, 0, 1, 9)]);

        let merged = merge_spans(vec![span_on_row(0, 0, 1), span_on_row(0, 5, 6)]);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn mean_coefficient_of_variation_ignores_single_sample_fixtures() {
        let samples: [&[f64]; 3] = [&[10.0], &[9.0, 11.0], &[]];
        let cv = mean_coefficient_of_variation(samples.into_iter()).unwrap();
        assert!((cv - 10.0).abs() < 1e-9, "got {cv}");
        assert_eq!(
            mean_coefficient_of_variation([&[5.0][..]].into_iter()),
            None
        );
    }

    #[test]
    fn merge_spans_handles_the_empty_case() {
        assert!(merge_spans(Vec::new()).is_empty());
    }

    /// A pure insertion's before header is `@@ -3,0 +4,2 @@`: line 3 is an anchor, not touched.
    /// Misreading it yields plausible rates rather than an obvious failure, so it is pinned here.
    #[test]
    fn git_line_labels_treats_a_zero_count_side_as_untouched() {
        let before = Code::from_string("a\nb\nc\n", &Language::Rust);
        let after = Code::from_string("a\nb\nc\nd\ne\n", &Language::Rust);
        let (before_touched, after_touched) =
            git_line_labels("myers", &before, &after).expect("git diff runs");
        assert!(
            !before_touched.iter().any(|touched| *touched),
            "a pure insertion touches no before-side line, got {before_touched:?}"
        );
        assert_eq!(
            after_touched
                .iter()
                .enumerate()
                .filter(|(_, touched)| **touched)
                .map(|(index, _)| index + 1)
                .collect::<Vec<_>>(),
            vec![4, 5],
            "the two inserted lines, 1-indexed"
        );
    }

    /// GNU diff and libxdiff are independent Myers implementations, so any disagreement is a bug in
    /// `git_line_labels`'s header parsing, not a finding about the algorithms.
    // Apple's diff has no `--*-line-format`, so the GNU-only runner cannot work there.
    #[cfg_attr(target_os = "macos", ignore = "needs GNU diff")]
    #[test]
    fn git_myers_agrees_with_unix_diff() {
        let cases = [
            ("fn a() {\n  one();\n}\n", "fn a() {\n  two();\n}\n"),
            ("a\nb\nc\n", "a\nb\nc\nd\n"),
            ("a\nb\nc\nd\n", "a\nd\n"),
            ("x\n", "x\n"),
            (
                "one\ntwo\nthree\nfour\nfive\n",
                "one\nthree\ntwo\nfour\nsix\n",
            ),
        ];
        for (before_text, after_text) in cases {
            let before = Code::from_string(before_text, &Language::Rust);
            let after = Code::from_string(after_text, &Language::Rust);
            let git = git_line_labels("myers", &before, &after).expect("git diff runs");
            let unix = human_mapping::unix_diff_line_labels(&before, &after).expect("diff runs");
            assert_eq!(
                git, unix,
                "git myers and unix diff disagree on {before_text:?} -> {after_text:?}"
            );
        }
    }

    /// BDiff's `str_diff` offsets are inclusive on both ends and count characters; either mistake is
    /// invisible on ASCII input, hence the multi-byte character before the change.
    #[test]
    fn bdiff_spans_from_script_reads_str_diff_as_inclusive_character_offsets() {
        let before = Code::from_string("let é = abcdefghij;\n", &Language::Rust);
        let after = Code::from_string("let é = abcXYZfghij;\n", &Language::Rust);
        // Characters 11..=12 of the before line are "de"; 11..=13 of the after line are "XYZ".
        let script: Vec<serde_json::Value> = serde_json::from_str(
            r#"[{"mode": "update", "src_line": 1, "dest_line": 1,
                 "str_diff": [[[11, 12]], [[11, 13]]]}]"#,
        )
        .unwrap();

        let (before_spans, after_spans) =
            bdiff_spans_from_script(&before, &after, &script).unwrap();

        // "let é = abc" is 12 bytes.
        assert_eq!(before_spans, vec![span_on_row(0, 12, 14)]);
        assert_eq!(after_spans, vec![span_on_row(0, 12, 15)]);
        assert_eq!(&before.contents[12..14], "de");
        assert_eq!(&after.contents[12..15], "XYZ");
    }

    /// A pure insertion gives the before side an empty `[]` range: no span, not a zero-width one.
    #[test]
    fn bdiff_spans_from_script_skips_an_empty_side_range() {
        let before = Code::from_string("hello world\n", &Language::Rust);
        let after = Code::from_string("hello there world\n", &Language::Rust);
        let script: Vec<serde_json::Value> = serde_json::from_str(
            r#"[{"mode": "update", "src_line": 1, "dest_line": 1,
                 "str_diff": [[[]], [[6, 11]]]}]"#,
        )
        .unwrap();

        let (before_spans, after_spans) =
            bdiff_spans_from_script(&before, &after, &script).unwrap();

        assert!(before_spans.is_empty(), "got {before_spans:?}");
        assert_eq!(after_spans, vec![span_on_row(0, 6, 12)]);
        assert_eq!(&after.contents[6..12], "there ");
    }

    /// Modes without a `str_diff` contribute whole lines on the same sides as `bdiff_line_labels`,
    /// or BDiff would be scored only on its updates.
    #[test]
    fn bdiff_spans_from_script_falls_back_to_whole_lines_without_str_diff() {
        let before = Code::from_string("aa\nbb\ncc\n", &Language::Rust);
        let after = Code::from_string("aa\nbb\ncc\n", &Language::Rust);
        let script: Vec<serde_json::Value> = serde_json::from_str(
            r#"[
                {"mode": "delete", "src_line": 1, "dest_line": 1},
                {"mode": "insert", "src_line": 1, "dest_line": 2},
                {"mode": "update", "src_line": 3, "dest_line": 3},
                {"mode": "copy",   "src_line": 1, "dest_line": 1, "block_length": 2}
            ]"#,
        )
        .unwrap();

        let (before_spans, after_spans) =
            bdiff_spans_from_script(&before, &after, &script).unwrap();

        // delete -> before row 0; update with no str_diff -> both sides' row 2.
        assert_eq!(
            before_spans,
            vec![span_on_row(0, 0, 2), span_on_row(2, 0, 2)]
        );
        // insert -> after row 1; update -> after row 2; copy -> after rows 0 and 1.
        assert_eq!(
            after_spans,
            vec![
                span_on_row(1, 0, 2),
                span_on_row(2, 0, 2),
                span_on_row(0, 0, 2),
                span_on_row(1, 0, 2),
            ]
        );
    }

    #[test]
    fn span_on_row_chars_converts_character_offsets_to_byte_columns() {
        let lines = vec!["aéb", "plain"];

        // "éb" is bytes 1..4.
        assert_eq!(span_on_row_chars(&lines, 0, 1, 3), span_on_row(0, 1, 4));
        assert_eq!(span_on_row_chars(&lines, 0, 0, 99), span_on_row(0, 0, 4));
        assert_eq!(span_on_row_chars(&lines, 9, 0, 1), span_on_row(9, 0, 0));
    }

    /// `bdiff_line_labels`' mode-to-side rules without BDiff installed. The judgement calls:
    /// insert/delete's other-side line is an anchor, and copy's source block is unchanged.
    #[test]
    fn bdiff_touched_from_script_follows_the_documented_mode_rules() {
        let before = Code::from_string("1\n2\n3\n4\n5\n6\n", &Language::Rust);
        let after = Code::from_string("1\n2\n3\n4\n5\n6\n", &Language::Rust);
        let script: Vec<serde_json::Value> = serde_json::from_str(
            r#"[
                {"mode": "insert", "src_line": 2, "dest_line": 3},
                {"mode": "delete", "src_line": 5, "dest_line": 1},
                {"mode": "copy",   "src_line": 1, "dest_line": 4, "block_length": 2}
            ]"#,
        )
        .expect("valid test JSON");
        let (before_touched, after_touched) =
            bdiff_touched_from_script(&before, &after, &script).expect("known modes");
        let touched = |v: &Vec<bool>| {
            v.iter()
                .enumerate()
                .filter(|(_, t)| **t)
                .map(|(i, _)| i + 1)
                .collect::<Vec<_>>()
        };
        assert_eq!(touched(&before_touched), vec![5]);
        assert_eq!(touched(&after_touched), vec![3, 4, 5]);
    }

    #[test]
    fn bdiff_touched_from_script_rejects_an_unknown_mode() {
        let code = Code::from_string("1\n", &Language::Rust);
        let script: Vec<serde_json::Value> =
            serde_json::from_str(r#"[{"mode": "teleport", "src_line": 1, "dest_line": 1}]"#)
                .expect("valid test JSON");
        assert!(bdiff_touched_from_script(&code, &code, &script).is_err());
    }
}
