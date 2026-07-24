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

//! Compares codediff, and other diff tools (`ExternalTool` - Unix `diff` is the first, more can
//! be added), against the human-authored ground truth in `src/test/data/diffs/*/human_mapping.json`
//! - the same corpus `benchmark_optimal_solutions` scores codediff against, but at line
//! granularity instead of AST-node granularity.
//!
//! Line granularity, not node granularity, because that's the only signal an external line-based
//! tool can produce at all: Unix `diff` has no notion of "this identifier was renamed," only
//! "these lines differ." To compare fairly, the human mapping and codediff's own `ASTDiff` are
//! both projected down to the same per-line "touched or not" signal an external tool can be
//! compared against - see `human_mapping::as_ast_diff` (human mapping -> synthetic `ASTDiff`) and
//! `diff::text::line_operations` (`ASTDiff` -> per-line `TextOperation`, shared with `tui::headless`).
//!
//! This necessarily throws away exactly the dimension codediff is built to get right (moves,
//! restructured-but-recognizable blocks), so a codediff fixture with zero AST-mismatches against
//! the human mapping can still show line mismatches here if the human moved a block far enough
//! that its line position changed - that's expected, not a bug in the scoring. The point of this
//! benchmark isn't "is codediff line-perfect," it's "how much of the corpus can a line-only tool
//! not even see," which is exactly what the codediff-vs-tool gap on each row shows.

use anyhow::{Context, Result, bail};
use clap::Parser;
use codediff::code::{Code, Language};
use codediff::diff::text::{TextDiff, TextOperation, line_operations};
use codediff::diff::{self, ASTDiff, NodeCache};
use codediff::test::helper;
use codediff::test::helper::human_mapping;
use csv::Writer;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::process::Command;

#[derive(Parser)]
struct Args {
    /// Print every before/after line where codediff or an external tool disagrees with the human
    /// mapping's touched/untouched call, for this one fixture, instead of the summary table.
    #[arg(long)]
    details: Option<String>,

    /// Output results as a CSV file. Default path: "./research/benchmark_other.csv"
    #[arg(long, value_name = "PATH", num_args = 0..=1)]
    csv: Option<Option<std::path::PathBuf>>,
}

/// An external, non-codediff diff tool being scored against the human-authored mapping. Adding a
/// tool means adding a variant here plus a `line_labels` match arm - `main`'s corpus loop, the
/// table, and the CSV all pick it up automatically via `ExternalTool::ALL`.
#[derive(Clone, Copy)]
enum ExternalTool {
    UnixDiff,
    GumTree,
}

impl ExternalTool {
    const ALL: &'static [ExternalTool] = &[ExternalTool::UnixDiff, ExternalTool::GumTree];

    fn name(&self) -> &'static str {
        match self {
            ExternalTool::UnixDiff => "unix_diff",
            ExternalTool::GumTree => "gumtree",
        }
    }

    /// Whether this tool has a generator it can be scored on for `language` - fixtures outside
    /// this set are skipped for this tool entirely (`None` in `Row`, not scored as a mismatch and
    /// not counted in its totals), the same way `benchmark_optimal_solutions` skips "unsolved"
    /// fixtures rather than silently failing or zero-filling them.
    ///
    /// GumTree's coverage is every corpus language `gumtree_generator` maps (i.e. every language
    /// with *any* registered generator, confirmed live via `gumtree list generators` against the
    /// actual v4.0.0-beta8 build - not just what its "Languages" wiki page claims, which is stale
    /// in places). This is deliberately wider than "only backends GumTree calls Stable": per that
    /// wiki page (checked 2026-07), only `java-jdt` (Java) and `css-phcss` (CSS) are "Stable" -
    /// every other generator here (`*-treesitter-ng`) is still "Testing" by GumTree's own
    /// classification, with none of this codebase's `nodes.rs`-style per-language tuning. Included
    /// anyway so the comparison covers the corpus GumTree can actually run on at all; the
    /// Stable/Testing split is exactly what `gumtree_generator`'s doc comment records per language,
    /// so a reader who wants only the "fair fight" subset can still filter by it.
    fn supports(&self, language: Language) -> bool {
        match self {
            ExternalTool::UnixDiff => true,
            ExternalTool::GumTree => gumtree_generator(language).is_some(),
        }
    }

    /// `(before_touched, after_touched)`: one bool per line of `before.contents`/`after.contents`,
    /// true where this tool considers that line part of the edit. Only meaningful (and only ever
    /// called by `main`'s corpus loop) when `supports` is true for the pair's language.
    fn line_labels(&self, before: &Code, after: &Code) -> Result<(Vec<bool>, Vec<bool>)> {
        match self {
            ExternalTool::UnixDiff => unix_diff_line_labels(before, after),
            ExternalTool::GumTree => gumtree_line_labels(before, after),
        }
    }
}

/// Shells out to the real `diff`, not a reimplementation - the whole point of this benchmark is
/// comparing against the actual tool people run. Writes `before`/`after`'s contents to fresh temp
/// files rather than trusting a fixture's on-disk `before.<lang>.test`/`after.<lang>.test` naming,
/// so this works for any `Code` pair, not just ones that came from a fixture directory.
///
/// Uses GNU diffutils' `--old-line-format`/`--new-line-format`/`--unchanged-line-format` (`%dn`
/// prints a line's 1-indexed line number) instead of parsing unified-diff hunk headers by hand -
/// two invocations (one per side), each printing exactly the touched line numbers on that side and
/// nothing else. This offloads all the hunk/context accounting to `diff` itself rather than
/// re-deriving it from `-u` output, which would also have to special-case "\ No newline at end of
/// file" and multi-line hunks.
fn unix_diff_line_labels(before: &Code, after: &Code) -> Result<(Vec<bool>, Vec<bool>)> {
    let mut before_file = tempfile::NamedTempFile::new().context("creating before temp file")?;
    let mut after_file = tempfile::NamedTempFile::new().context("creating after temp file")?;
    before_file.write_all(before.contents.as_bytes()).context("writing before temp file")?;
    after_file.write_all(after.contents.as_bytes()).context("writing after temp file")?;

    let before_line_count = before.contents.split('\n').count();
    let after_line_count = after.contents.split('\n').count();

    let before_touched = touched_line_numbers(
        &["--old-line-format=%dn\n", "--new-line-format=", "--unchanged-line-format="],
        before_file.path(),
        after_file.path(),
        before_line_count,
    )?;
    let after_touched = touched_line_numbers(
        &["--old-line-format=", "--new-line-format=%dn\n", "--unchanged-line-format="],
        before_file.path(),
        after_file.path(),
        after_line_count,
    )?;

    Ok((before_touched, after_touched))
}

/// Runs `diff` with the given `--*-line-format` flags (see `unix_diff_line_labels`) and turns its
/// stdout - one 1-indexed line number per line - into a 0-indexed `line_count`-long touched mask.
fn touched_line_numbers(
    format_flags: &[&str],
    before_path: &std::path::Path,
    after_path: &std::path::Path,
    line_count: usize,
) -> Result<Vec<bool>> {
    let output = Command::new("diff")
        .args(format_flags)
        .arg(before_path)
        .arg(after_path)
        .output()
        .context("running `diff` - is diffutils installed?")?;
    // diff exits 0 for "no differences" and 1 for "differences found" - both are success for our
    // purposes. 2+ is a real error (bad flags, unreadable file, ...).
    if output.status.code().is_none_or(|c| c > 1) {
        bail!("diff exited with {:?}: {}", output.status.code(), String::from_utf8_lossy(&output.stderr));
    }

    let mut touched = vec![false; line_count];
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let line_number: usize = line.trim().parse().with_context(|| format!("parsing diff output line {:?}", line))?;
        if let Some(slot) = line_number.checked_sub(1).and_then(|idx| touched.get_mut(idx)) {
            *slot = true;
        }
    }
    Ok(touched)
}

/// `(generator id, file extension)` for every corpus language GumTree v4.0.0-beta8 has a
/// registered generator for at all - confirmed live via `gumtree list generators` against the
/// actual build (2026-07), not just its wiki page (which is stale: it doesn't list JSON, which
/// still turns out to have no generator either way - `gen.json`'s only registered generator is
/// XML, despite the module name).
///
/// One backend picked per language, not the pick-by-priority-number default GumTree's own `-g`-
/// less auto-detection would use: every entry here is passed via `-g <id>` explicitly (see
/// `gumtree_line_labels`), both to sidestep a real bug in GumTree's own auto-detect regex for C#
/// (`cs-treesitter-ng` is registered as `\.[cs]$` - a character class matching a lone "c" or "s",
/// not the literal ".cs" - confirmed empirically: a `.cs` file never auto-selects it) and so this
/// table is the one place that documents exactly which generator every language actually runs
/// through, rather than leaving it to auto-detection's priority ordering.
///
/// `*-treesitter-ng` chosen over a `*-srcml` alternative (available for C/C++/C#/Java) wherever
/// both exist: no extra external binary to install, and it keeps every non-Java language on the
/// same parser family this codebase itself is built on, which is at least a more comparable
/// unknown than mixing parser families per language. Per GumTree's own "Languages" wiki page
/// (checked 2026-07), only `java-jdt` and `css-phcss` are marked "Stable" - every
/// `*-treesitter-ng` entry below is still "Testing" by GumTree's own classification (see
/// `ExternalTool::supports`'s doc comment for what that means for how to read results on them).
///
/// No entry for `Language::JSON` (1 fixture in the corpus) or any language outside this match:
/// confirmed no registered generator exists for it in this build.
fn gumtree_generator(language: Language) -> Option<(&'static str, &'static str)> {
    match language {
        Language::Java => Some(("java-jdt", "java")),               // Stable
        Language::CSS => Some(("css-phcss", "css")),                // Stable
        Language::Rust => Some(("rust-treesitter-ng", "rs")),       // Testing
        Language::CPP => Some(("cpp-treesitter-ng", "cpp")),        // Testing
        Language::Kotlin => Some(("kotlin-treesitter-ng", "kt")),   // Testing
        Language::C => Some(("c-treesitter-ng", "c")),              // Testing
        Language::Go => Some(("go-treesitter-ng", "go")),           // Testing
        Language::Python => Some(("python-treesitter-ng", "py")),   // Testing
        Language::TypeScript => Some(("ts-treesitter-ng", "ts")),   // Testing
        Language::JavaScript => Some(("js-treesitter-ng", "js")),   // Testing
        Language::CSharp => Some(("cs-treesitter-ng", "cs")),       // Testing
        _ => None,
    }
}

/// Path to the GumTree CLI script (`bin/gumtree` in its built distribution), from the `GUMTREE_BIN`
/// environment variable - not bundled or auto-installed, since it's a separate JVM project with
/// its own build (JDK 17 + Gradle; see the project's own install docs). Deliberately errors loudly
/// rather than silently skipping: unlike a language `ExternalTool::supports` excludes, a missing
/// binary for a language it claims to support is a real configuration problem.
fn gumtree_bin() -> Result<std::path::PathBuf> {
    let path = std::env::var("GUMTREE_BIN")
        .context("GUMTREE_BIN is not set - point it at GumTree's built bin/gumtree script")?;
    let path = std::path::PathBuf::from(path);
    if !path.is_file() {
        bail!("GUMTREE_BIN={:?} does not exist or is not a file", path);
    }
    Ok(path)
}

/// Runs the real GumTree CLI (`textdiff ... -f JSON`) and reduces its output to the same per-line
/// touched signal every other `ExternalTool` produces.
///
/// GumTree's JSON has two top-level arrays:
/// - `matches`: `{src, dest}` pairs, each a `"KIND[: text] [start,end]"` string with a *character*
///   offset range into that side's source text (verified empirically against this project's own
///   fixtures, 2026-07 - `[start,end]` is half-open, matching this codebase's own `TextRange`
///   convention).
/// - `actions`: the edit script. `insert-tree`/`insert-node`'s `tree` is a dest-side reference
///   (no src counterpart exists yet); `delete-tree`/`delete-node`'s `tree` is src-side (no dest
///   counterpart). `update-node`/`move-tree`/`move-node` are the tricky ones: `tree` is *always*
///   src-side even though the node also has a dest-side position - getting the dest range means
///   looking `tree`'s exact string up in `matches` to find its `dest` counterpart (confirmed
///   empirically against `java-refactor-constants` and `java-fix-array-index`: a `move-tree`/
///   `update-node` action's `tree` string always appears verbatim as some `matches[].src` entry).
fn gumtree_line_labels(before: &Code, after: &Code) -> Result<(Vec<bool>, Vec<bool>)> {
    let language = before.metadata.language.unwrap_or_default();
    let (generator, ext) =
        gumtree_generator(language).with_context(|| format!("no GumTree generator for {language:?}"))?;
    let gumtree = gumtree_bin()?;

    let mut before_file = tempfile::Builder::new().suffix(&format!(".{ext}")).tempfile()?;
    let mut after_file = tempfile::Builder::new().suffix(&format!(".{ext}")).tempfile()?;
    before_file.write_all(before.contents.as_bytes()).context("writing before temp file")?;
    after_file.write_all(after.contents.as_bytes()).context("writing after temp file")?;

    let output = Command::new(&gumtree)
        .args(["textdiff", "-g", generator, "-f", "JSON"])
        .arg(before_file.path())
        .arg(after_file.path())
        .output()
        .with_context(|| format!("running {gumtree:?} textdiff -g {generator}"))?;
    if !output.status.success() {
        bail!("gumtree exited with {:?}: {}", output.status.code(), String::from_utf8_lossy(&output.stderr));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).context("parsing gumtree JSON output")?;
    let matches = json["matches"].as_array().context("gumtree JSON has no `matches` array")?;
    let actions = json["actions"].as_array().context("gumtree JSON has no `actions` array")?;

    let src_to_dest: HashMap<&str, &str> = matches
        .iter()
        .filter_map(|m| Some((m["src"].as_str()?, m["dest"].as_str()?)))
        .collect();

    let mut before_touched = vec![false; before.contents.split('\n').count()];
    let mut after_touched = vec![false; after.contents.split('\n').count()];

    let mark = |touched: &mut [bool], contents: &str, node_ref: &str| -> Result<()> {
        let (start, end) = gumtree_node_offsets(node_ref)?;
        for line in gumtree_line_range(contents, start, end) {
            if let Some(slot) = touched.get_mut(line) {
                *slot = true;
            }
        }
        Ok(())
    };

    for action in actions {
        let kind = action["action"].as_str().context("gumtree action missing `action`")?;
        let tree = action["tree"].as_str().context("gumtree action missing `tree`")?;
        match kind {
            "insert-tree" | "insert-node" => mark(&mut after_touched, &after.contents, tree)?,
            "delete-tree" | "delete-node" => mark(&mut before_touched, &before.contents, tree)?,
            "update-node" | "move-tree" | "move-node" => {
                mark(&mut before_touched, &before.contents, tree)?;
                match src_to_dest.get(tree) {
                    Some(dest) => mark(&mut after_touched, &after.contents, dest)?,
                    // Contradicts what every fixture checked during development showed (see this
                    // function's doc comment) - not fatal, but real enough to want visible in
                    // benchmark output rather than a silently under-counted after-side.
                    None => eprintln!("gumtree: no `matches` entry for {kind} tree {tree:?}, after-side line(s) not marked"),
                }
            }
            other => bail!("unrecognized gumtree action kind {other:?}"),
        }
    }

    Ok((before_touched, after_touched))
}

/// Parses the `[start,end]` character-offset suffix off a GumTree node-reference string like
/// `"SimpleName: foo [12,15]"` - the text before it (kind, optional `: text`) is irrelevant here,
/// only the position matters. Anchored to the end of the string since a node's own text can itself
/// contain brackets or commas (e.g. an array-literal leaf), which a naive first-bracket search
/// would misparse.
fn gumtree_node_offsets(node_ref: &str) -> Result<(usize, usize)> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"\[(\d+),(\d+)\]$").unwrap());
    let caps = re.captures(node_ref).with_context(|| format!("no [start,end] suffix in {node_ref:?}"))?;
    let start: usize = caps[1].parse()?;
    let end: usize = caps[2].parse()?;
    Ok((start, end))
}

/// 0-indexed line numbers `[start, end)` (a half-open character range into `contents`) touches,
/// inclusive of the line containing `end`'s last character - a range landing exactly on a line
/// boundary doesn't spuriously pull in the following, untouched line.
fn gumtree_line_range(contents: &str, start: usize, end: usize) -> std::ops::RangeInclusive<usize> {
    let line_of = |offset: usize| contents[..offset.min(contents.len())].matches('\n').count();
    line_of(start)..=line_of(end.saturating_sub(1).max(start))
}

/// Reduces one side's `TextOperation`s to "touched or not" - the only signal comparable against
/// an `ExternalTool`, which has no notion of codediff's finer-grained Update/Move/... distinction.
fn touched(ops: &[TextOperation]) -> Vec<bool> {
    ops.iter().map(|op| *op != TextOperation::Identical).collect()
}

/// Projects `ast_diff` down to per-line touched masks for both sides, via the same
/// `TextDiff`/`line_operations` path used for both codediff's own diff and the synthetic
/// human-mapping diff - so the two are reduced to line labels identically.
fn touched_lines(before: &Code, after: &Code, ast_diff: &ASTDiff, node_cache: &NodeCache) -> (Vec<bool>, Vec<bool>) {
    let text_diff = TextDiff::from(before, after, ast_diff, node_cache);
    let before_ops = line_operations(&text_diff.all(0), before.contents.split('\n').count());
    let after_ops = line_operations(&text_diff.all(1), after.contents.split('\n').count());
    (touched(&before_ops), touched(&after_ops))
}

/// Number of positions where `a` and `b` disagree. Panics on a length mismatch - `a`/`b` always
/// come from splitting the exact same `contents` string on `'\n'`, so their lengths can never
/// legitimately differ.
fn disagreement_count(a: &[bool], b: &[bool]) -> usize {
    assert_eq!(a.len(), b.len(), "line count mismatch between two labelings of the same file");
    a.iter().zip(b).filter(|(x, y)| x != y).count()
}

struct Row {
    name: String,
    /// (mismatched lines, total lines across before+after) for codediff against the
    /// human-projected line labels.
    codediff: (usize, usize),
    /// Same shape, one entry per `ExternalTool::ALL`, in that order. `None` means this tool
    /// doesn't claim to support the fixture's language (`ExternalTool::supports`) - excluded from
    /// its totals entirely, not scored as a mismatch and not counted toward "total fixtures," the
    /// same way `benchmark_optimal_solutions` treats a fixture with no `human_mapping.json` yet.
    tools: Vec<Option<(usize, usize)>>,
    /// Milliseconds to go from `before`/`after` to codediff's per-line touched labels
    /// (`diff::diff_code` plus the `touched_lines` projection) - timed the same way as each
    /// `ExternalTool`, so the two are comparable: "cost to produce a line-level touched/untouched
    /// verdict," not just "cost to run the underlying algorithm."
    codediff_ms: f64,
    /// Same shape as `tools`: milliseconds inside `ExternalTool::line_labels` (which already
    /// produces line-level labels directly, so no extra projection step to include). `None` in
    /// lockstep with `tools`.
    tool_ms: Vec<Option<f64>>,
}

fn score_fixture(name: &str, before: &Code, after: &Code) -> Result<Row> {
    let language = before.metadata.language.unwrap_or_default();
    let human_diff = human_mapping::as_ast_diff(name, before, after)?;
    let node_cache = NodeCache::build(before, after);
    let (human_before, human_after) = touched_lines(before, after, &human_diff, &node_cache);
    let total_lines = human_before.len() + human_after.len();

    let started = std::time::Instant::now();
    let codediff_diff = diff::diff_code(before, after);
    let codediff_ast = codediff_diff.ast.context("codediff produced no AST mapping")?;
    let (codediff_before, codediff_after) = touched_lines(before, after, &codediff_ast, &node_cache);
    let codediff_ms = started.elapsed().as_secs_f64() * 1000.0;
    let codediff_mismatches =
        disagreement_count(&human_before, &codediff_before) + disagreement_count(&human_after, &codediff_after);

    let mut tools = Vec::with_capacity(ExternalTool::ALL.len());
    let mut tool_ms = Vec::with_capacity(ExternalTool::ALL.len());
    for tool in ExternalTool::ALL {
        if !tool.supports(language) {
            tools.push(None);
            tool_ms.push(None);
            continue;
        }
        let started = std::time::Instant::now();
        let (tool_before, tool_after) = tool.line_labels(before, after)?;
        tool_ms.push(Some(started.elapsed().as_secs_f64() * 1000.0));
        let mismatches =
            disagreement_count(&human_before, &tool_before) + disagreement_count(&human_after, &tool_after);
        tools.push(Some((mismatches, total_lines)));
    }

    Ok(Row {
        name: name.to_string(),
        codediff: (codediff_mismatches, total_lines),
        tools,
        codediff_ms,
        tool_ms,
    })
}

/// Prints every before/after line where codediff or an external tool disagrees with the human
/// mapping's touched/untouched call for `name`, instead of the summary table - the raw material
/// for understanding why a fixture's mismatch count is what it is.
fn print_details(name: &str, before: &Code, after: &Code) -> Result<()> {
    let language = before.metadata.language.unwrap_or_default();
    let human_diff = human_mapping::as_ast_diff(name, before, after)?;
    let node_cache = NodeCache::build(before, after);
    let (human_before, human_after) = touched_lines(before, after, &human_diff, &node_cache);

    let codediff_diff = diff::diff_code(before, after);
    let codediff_ast = codediff_diff.ast.context("codediff produced no AST mapping")?;
    let (codediff_before, codediff_after) = touched_lines(before, after, &codediff_ast, &node_cache);

    let mut sources: Vec<(&str, Vec<bool>, Vec<bool>)> = vec![("codediff", codediff_before, codediff_after)];
    for tool in ExternalTool::ALL {
        if !tool.supports(language) {
            println!("{}: does not support {:?}, skipped", tool.name(), language);
            continue;
        }
        let (tool_before, tool_after) = tool.line_labels(before, after)?;
        sources.push((tool.name(), tool_before, tool_after));
    }

    for (source_name, source_before, source_after) in &sources {
        for (side_name, human_side, source_side) in
            [("before", &human_before, source_before), ("after", &human_after, source_after)]
        {
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
    let test_diffs = helper::handmade_test_code_pairs()?;

    if let Some(name) = args.details {
        let (before, after) =
            test_diffs.get(&name).with_context(|| format!("no fixture named '{}'", name))?;
        return print_details(&name, before, after);
    }

    let mut names: Vec<String> = test_diffs
        .keys()
        .filter(|name| human_mapping::mapping_path(name).exists())
        .cloned()
        .collect();
    names.sort();

    let started = std::time::Instant::now();
    let mut rows = Vec::with_capacity(names.len());
    for name in &names {
        let (before, after) = test_diffs.get(name).expect("name came from test_diffs.keys()");
        rows.push(score_fixture(name, before, after)?);
    }
    let elapsed = started.elapsed();

    // Worst codediff offenders first, so the fixtures where line-level scoring disagrees most
    // with codediff's own (node-level) view of its accuracy are the first thing visible.
    rows.sort_by(|a, b| b.codediff.0.cmp(&a.codediff.0).then_with(|| a.name.cmp(&b.name)));

    if let Some(csv_path) = args.csv {
        let path = csv_path.unwrap_or_else(|| std::path::PathBuf::from("./research/benchmark_other.csv"));
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
    let name_width = rows.iter().map(|r| r.name.len()).chain(["Solution".len()]).max().unwrap_or(0);
    let tool_names: Vec<&str> = ExternalTool::ALL.iter().map(|t| t.name()).collect();

    print!("{:<name_width$}  {:>9}  {:>7}", "Solution", "codediff", "cd %", name_width = name_width);
    for tool_name in &tool_names {
        print!("  {:>9}  {:>7}", tool_name, format!("{tool_name} %"));
    }
    println!();
    let rule_width = name_width + (2 + 9 + 2 + 7) * (1 + tool_names.len());
    println!("{}", "-".repeat(rule_width));

    let mut codediff_total = (0usize, 0usize);
    // (sum mismatches, sum total lines, fixtures actually scored) - the third field is what lets
    // the summary line below distinguish "scored 0 mismatches on every applicable fixture" from
    // "not applicable to most of this corpus," which the percentage alone can't (a tool scored on
    // only 12/93 fixtures and a tool scored on all 93 can land on the same aggregate % by chance).
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
            println!("  ({tool_name} scored on {scored}/{} fixtures - the rest are outside its language scope)", rows.len());
        }
    }
}

fn pct(mismatches: usize, total: usize) -> f64 {
    if total > 0 { 100.0 * mismatches as f64 / total as f64 } else { 0.0 }
}

/// Per-tool timing: total and mean milliseconds across every fixture, each tool timed identically
/// (see `Row::codediff_ms`/`tool_ms`'s doc comments) so the totals are directly comparable.
fn print_runtime_table(rows: &[Row]) {
    let tool_names: Vec<&str> = ExternalTool::ALL.iter().map(|t| t.name()).collect();
    let label_width = ["codediff"].iter().chain(&tool_names).map(|s| s.len()).max().unwrap_or(0);

    println!();
    println!("Per-tool runtime (time to produce line-level touched/untouched labels):");
    println!("{:<label_width$}  {:>10}  {:>10}", "Tool", "Total ms", "Mean ms", label_width = label_width);
    println!("{}", "-".repeat(label_width + 2 + 10 + 2 + 10));

    let codediff_total: f64 = rows.iter().map(|r| r.codediff_ms).sum();
    println!(
        "{:<label_width$}  {:>10.1}  {:>10.3}  (n={})",
        "codediff",
        codediff_total,
        codediff_total / rows.len().max(1) as f64,
        rows.len(),
        label_width = label_width
    );
    for (i, tool_name) in tool_names.iter().enumerate() {
        // Mean is over fixtures this tool was actually scored on, not every fixture in the
        // corpus - dividing by `rows.len()` would understate a language-scoped tool's real
        // per-fixture cost by mixing in zero-cost "not applicable" fixtures it never ran on.
        let scored: Vec<f64> = rows.iter().filter_map(|r| r.tool_ms[i]).collect();
        let total: f64 = scored.iter().sum();
        println!(
            "{:<label_width$}  {:>10.1}  {:>10.3}  (n={})",
            tool_name,
            total,
            total / scored.len().max(1) as f64,
            scored.len(),
            label_width = label_width
        );
    }
}

fn write_csv(rows: &[Row], path: &std::path::Path) -> Result<()> {
    let file = File::create(path)?;
    let mut wtr = Writer::from_writer(file);

    let mut header = vec![
        "solution".to_string(),
        "total_lines".to_string(),
        "codediff_mismatches".to_string(),
        "codediff_ms".to_string(),
    ];
    header.extend(ExternalTool::ALL.iter().flat_map(|t| [format!("{}_mismatches", t.name()), format!("{}_ms", t.name())]));
    wtr.write_record(&header)?;

    for row in rows {
        let mut record =
            vec![row.name.clone(), row.codediff.1.to_string(), row.codediff.0.to_string(), row.codediff_ms.to_string()];
        // Empty field, not "0" - a tool this fixture is out of scope for (`ExternalTool::supports`
        // was false) didn't score 0 mismatches, it wasn't scored at all. Downstream readers
        // (`benchmark_other_report.py`) must treat a blank the same way pandas/csv already do:
        // excluded from that tool's aggregate, not coerced to zero.
        record.extend(row.tools.iter().zip(&row.tool_ms).flat_map(|(cell, ms)| {
            [cell.map(|(mismatches, _)| mismatches.to_string()).unwrap_or_default(), ms.map(|v| v.to_string()).unwrap_or_default()]
        }));
        wtr.write_record(&record)?;
    }
    wtr.flush()?;
    Ok(())
}
