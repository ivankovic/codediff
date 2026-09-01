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

//! Compares codediff against other diff tools (`ExternalTool`: Unix `diff`, GumTree, difftastic,
//! diffsitter - more can be added), against the human-authored ground truth in
//! `src/test/data/diffs/*/*/human_mapping.json` - the same corpus `benchmark_optimal_solutions`
//! scores codediff against, but at line granularity instead of AST-node granularity.
//!
//! GumTree, difftastic, and diffsitter are all separate, non-Cargo binaries, not bundled or
//! auto-installed - each needs its own environment variable pointing at a built binary
//! (`GUMTREE_BIN`, `DIFFT_BIN`, `DIFFSITTER_BIN` - see `gumtree_bin`, `difftastic_bin`,
//! `diffsitter_bin`). difftastic and diffsitter are plain `cargo install`-able, so
//! `cargo install --root /var/tmp/codediff-tools difftastic diffsitter` keeps both out of the
//! system-wide cargo bin directory and this checkout alike.
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
//!
//! Alongside the tools, every fixture also gets a `treesitter_parse_ms` reference lower bound -
//! see `treesitter_parse_ms` - the cost of tree-sitter parsing alone, with no diffing or AST
//! mapping on top. It's not an `ExternalTool` (nothing to score for accuracy), just context for
//! reading `codediff_ms`/`tool_ms`: the minimum any AST-aware tool in this comparison must pay
//! before its own work can even start.
//!
//! GumTree also gets a second, optional timing column, `gumtree_warm_ms` - see
//! `gumtree_warm_batch`. `ExternalTool::GumTree`'s own `gumtree_ms` spawns a fresh `gumtree`
//! subprocess per fixture, so it includes JVM startup/JIT warmup on every single fixture; that
//! overhead dominates the number for small files (see `benchmark_other_runtime.png`'s gumtree
//! violin sitting almost flat regardless of file size). `gumtree_warm_ms` runs the same algorithm
//! against every fixture through one persistent JVM (`research/drivers/gumtree-batch/`), so it
//! isolates GumTree's own cost from process-spawn overhead. Both numbers are kept - one is "cost
//! of invoking the CLI the way most users would," the other is "cost of the algorithm alone."

use anyhow::{Context, Result, bail};
use clap::Parser;
use codediff::code::{Code, Language};
use codediff::diff;
use codediff::diff::text_range::TextRange;
use codediff::test::helper;
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

    /// Accuracy-only run: score every tool's *line* and *node* agreement with the human mapping
    /// and write one row per fixture to a CSV, skipping all timing. Default path:
    /// "./research/data/comparison/benchmark_accuracy.csv".
    ///
    /// The node columns are a "did you consider this node's text changed" projection, exactly
    /// like the line columns one granularity down - NOT the node-to-node mapping fidelity
    /// `benchmark_optimal_solutions` reports for codediff. External tools parse their own trees
    /// and have no node identities in common with this codebase's, so no mapping-level question
    /// can be asked of them. codediff is scored through the same projection here, which makes its
    /// column comparable to the other tools' and deliberately *not* comparable to its own
    /// optimal-solutions number. See `nodes_touched_by` in `test::helper::human_mapping`.
    ///
    /// Measured 2026-08-20, on whether that limit could be lifted for GumTree specifically:
    /// GumTree *can* emit a full node-to-node mapping (`textdiff -f JSON` carries a `matches`
    /// array covering intermediate nodes, not just the edit script's actions, with real byte
    /// offsets into the file). Whether its tree is comparable to codediff's turns out to be
    /// entirely per-language, not a single yes/no - node counts on one fixture's before side,
    /// GumTree vs codediff:
    ///
    /// ```text
    /// java-jdt            512 /   997  (1.95x)   python  6708 / 10314  (1.54x)
    /// java-treesitter-ng  585 /   997  (1.70x)   rust     810 /  1344  (1.66x)
    /// go                  174 /   233  (1.34x)   kotlin  1330 /  1792  (1.35x)
    /// js                   51 /    51  (1.00x)   ruby    1427 /  1427  (1.00x)
    /// c                 23971 / 23969  (1.00x)   php    34779 / 34771  (1.00x)
    /// cs                 2280 /  2280  (1.00x)
    /// ```
    ///
    /// So for js/ruby/c/php/cs the two trees agree node-for-node (GumTree's tree-sitter-ng
    /// bindings use the same grammar and keep the same tokens), while Java's *default* generator,
    /// the one this table selects, is Eclipse JDT: a different parser entirely, which also emits
    /// synthetic nodes (`METHOD_INVOCATION_ARGUMENTS`, `TYPE_DECLARATION_KIND`) with no
    /// tree-sitter counterpart. A real mapping-fidelity comparison is therefore plausible for the
    /// 1.00x languages and not for the rest; it is not attempted here because a metric that only
    /// answers for some of the corpus, with a per-language caveat driving the result, is harder to
    /// read than the uniform projection below. Recorded so the option is not re-litigated from
    /// scratch.
    ///
    /// difftastic (`--display json`, gated behind `DFT_UNSTABLE=yes`) emits changed line chunks
    /// plus line alignment, no node correspondences; diffsitter has no machine-readable output at
    /// all. Neither can be asked a mapping-level question at any granularity.
    ///
    /// The `*_visible_node_mismatches` columns restrict the same projection to nodes that
    /// carry text of their own (`diff::nodes::is_structurally_visible`) - a leaf, or an interior
    /// node with non-whitespace content its children don't cover. Structural, so every tool is
    /// scored against one identical fixed set. Note this shares the projection's parser-divergence
    /// caveat above: it narrows *which* of codediff's nodes are scored, it does not make the tools'
    /// own trees any more comparable to codediff's.
    #[arg(long, value_name = "PATH", num_args = 0..=1)]
    accuracy_csv: Option<Option<std::path::PathBuf>>,

    /// How many times to re-run every timing measurement (codediff, each `ExternalTool`,
    /// `treesitter_parse`, and - via `gumtree_warm_batch` - `gumtree_warm`) per fixture. Accuracy/
    /// mismatch counts are deterministic (same algorithm, same input, every run) and are only ever
    /// computed once regardless of this value - only wall-clock timing is repeated. Default 3:
    /// this benchmark's own runtime numbers were found (2026-07-26) to swing by roughly +-10%
    /// between back-to-back single-shot runs on a loaded machine, which is noise a single sample
    /// can't distinguish from a real regression - `write_csv` records every repeat, not just a
    /// mean, so downstream analysis can see the actual spread rather than a single point estimate.
    #[arg(long, default_value_t = 3)]
    repeats: usize,

    /// Only score these fixtures, by name, comma-separated. Default: every fixture with a human
    /// mapping.
    ///
    /// Applied to the corpus *before* it is parsed, which is the whole point: `main` calls
    /// `ensure_parsed` on every fixture it holds, and on the full corpus that dominates the cost
    /// of a run that only wanted three of them. An unknown name is an error rather than an empty
    /// run - a typo that silently scores nothing looks exactly like a tool that found nothing.
    #[arg(long, value_name = "NAMES", value_delimiter = ',')]
    fixtures: Vec<String>,

    /// Only score these tools, comma-separated - any `ExternalTool` name plus `codediff`. Default:
    /// all of them.
    ///
    /// **Accuracy runs only.** The timing path builds its tables and CSV header from
    /// `ExternalTool::ALL` in several places, and a run whose columns don't match its header is a
    /// worse outcome than not offering the flag; passing this without `--accuracy-csv` is an error
    /// rather than a silent no-op.
    ///
    /// Filters codediff too, not just the external tools. Without that this saves nothing worth
    /// having: `score_accuracy` runs `diff_code` and all ten tools unconditionally, so scoping to
    /// one tool would still spawn git four times and shell out to python and neovim per fixture.
    #[arg(long, value_name = "NAMES", value_delimiter = ',')]
    tools: Vec<String>,
}

/// Which tools an accuracy run scores. `None` means all of them.
///
/// A set rather than a list of `ExternalTool`s because `codediff` is selectable here too and is
/// not an `ExternalTool` - it is scored by `score_accuracy`'s own block, through the same
/// projection, so that it lands in the same columns as everything else.
struct ToolSelection(Option<std::collections::HashSet<String>>);

impl ToolSelection {
    /// Errors on a name no tool answers to, listing what does. A mistyped `--tools gumtre` would
    /// otherwise produce a clean, complete-looking CSV with no rows scored by anything.
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

/// An external, non-codediff diff tool being scored against the human-authored mapping. Adding a
/// tool means adding a variant here plus a `line_labels` match arm - `main`'s corpus loop, the
/// table, and the CSV all pick it up automatically via `ExternalTool::ALL`.
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

    /// The `--diff-algorithm` value for the four git variants, `None` for every other tool.
    /// Git's four algorithms are the same engine (libxdiff) reached through one flag, so they
    /// share a single labeller rather than getting four near-identical ones.
    fn git_algorithm(&self) -> Option<&'static str> {
        match self {
            ExternalTool::GitMyers => Some("myers"),
            ExternalTool::GitMinimal => Some("minimal"),
            ExternalTool::GitPatience => Some("patience"),
            ExternalTool::GitHistogram => Some("histogram"),
            _ => None,
        }
    }

    /// Whether this tool has a generator it can be scored on for `language` - fixtures outside
    /// this set are skipped for this tool entirely (`None` in `Row`, not scored as a mismatch and
    /// not counted in its totals), the same way `benchmark_optimal_solutions` skips "unsolved"
    /// fixtures rather than silently failing or zero-filling them.
    ///
    /// GumTree's coverage is every corpus language `gumtree_generator` maps (i.e. every language
    /// with *any* registered generator, confirmed live via `gumtree list GENERATORS` *and* by
    /// running each one against a real fixture pair from this corpus - the installed build is
    /// **v4.0.0-beta8**, which is also what the paper's comparison section claims. A generator
    /// listing alone is not trustworthy, and the set genuinely moves between builds in both
    /// directions: beta8 adds C++ and TSX over beta4 and drops JSON. See `gumtree_generator` for
    /// the per-language detail and `research/data/comparison/PROVENANCE.md` for which build each
    /// committed measurement was taken against.)
    /// This is deliberately wider than "only backends GumTree calls Stable": per that
    /// wiki page (checked 2026-07), only `java-jdt` (Java) and `css-phcss` (CSS) are "Stable" -
    /// every other generator here (`*-treesitter-ng`) is still "Testing" by GumTree's own
    /// classification, with none of this codebase's `nodes.rs`-style per-language tuning. Included
    /// anyway so the comparison covers the corpus GumTree can actually run on at all; the
    /// Stable/Testing split is exactly what `gumtree_generator`'s doc comment records per language,
    /// so a reader who wants only the "fair fight" subset can still filter by it.
    ///
    /// Difftastic and diffsitter coverage is each tool's own compiled-in language set, confirmed
    /// live via `difft --list-languages` and `diffsitter list` against the actual installed
    /// builds - see `difftastic_extension`/`diffsitter_file_type`.
    fn supports(&self, language: Language) -> bool {
        match self {
            // Every text-based tool is language-agnostic by construction: it compares lines, and
            // has no parser or generator that could fail to exist for a language. That makes
            // their coverage the full corpus, exactly like Unix diff's.
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

    /// `(before_touched, after_touched)`: one bool per line of `before.contents`/`after.contents`,
    /// true where this tool considers that line part of the edit. Only meaningful (and only ever
    /// called by `main`'s corpus loop) when `supports` is true for the pair's language.
    fn line_labels(&self, before: &Code, after: &Code) -> Result<(Vec<bool>, Vec<bool>)> {
        match self {
            // Shared with `generate_mapping_site`'s index-page columns - see
            // `human_mapping::unix_diff_line_labels`'s own doc comment.
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

/// Path to an external diff tool's binary, read from the `env_var` environment variable - not
/// bundled or auto-installed, since each is a separate project this codebase merely shells out to.
/// `hint` is folded into the "not set" error to say specifically what to point the variable at
/// (each call site's own doc comment gives the fuller install story). Shared by
/// `gumtree_bin`/`difftastic_bin`/`diffsitter_bin`, which used to each hand-roll this identical
/// env-var-lookup -> `PathBuf` -> `is_file` check -> `bail!` sequence independently.
fn external_tool_bin(env_var: &str, hint: &str) -> Result<std::path::PathBuf> {
    let path = std::env::var(env_var).with_context(|| format!("{env_var} is not set - {hint}"))?;
    let path = std::path::PathBuf::from(path);
    if !path.is_file() {
        bail!("{env_var}={:?} does not exist or is not a file", path);
    }
    Ok(path)
}

/// Writes `before`/`after`'s contents into two fresh temp files - shared by every `ExternalTool`
/// that needs real files on disk to shell out to (GumTree, difftastic, diffsitter all take file
/// paths, not stdin), which previously each hand-rolled this identical "build a
/// `NamedTempFile`, write the contents, wrap both errors with which side failed" sequence.
/// `suffix`, when `Some`, gives both files the same extension (e.g. `".rs"`) - GumTree and
/// difftastic each key some of their own behavior off the file extension even when also told
/// explicitly which language/generator to use, so those two need a real one; diffsitter is told
/// its language purely via `-t` and doesn't care, so its own call site passes `None`.
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

/// Milliseconds tree-sitter alone takes to parse `source`'s contents into an AST - a reference
/// lower bound, not an `ExternalTool`: it produces no line labels and is never scored for
/// accuracy, only timed. Every AST-aware tool in this benchmark (codediff included) must pay at
/// least this cost before any diffing work can start, so it puts `codediff_ms`/`tool_ms` in
/// context rather than competing with them.
///
/// `source` already carries a parsed `ast` (every fixture in the corpus was parsed once by
/// `helper::handmade_test_code_pairs()`), so this clones it, clears `ast` back to `None`, and
/// times a fresh `Code::parse` call - reparsing from scratch, not reading the cached tree.
fn treesitter_parse_ms(source: &Code, parser: &mut tree_sitter::Parser) -> f64 {
    let mut code = source.clone();
    code.ast = None;
    let started = std::time::Instant::now();
    code.parse(parser);
    started.elapsed().as_secs_f64() * 1000.0
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
    /// verdict," not just "cost to run the underlying algorithm." One entry per `--repeats`
    /// iteration (default 3, see `Args::repeats`) - kept as the full sample, not collapsed to a
    /// mean, so a noisy fixture is visible as noise rather than silently averaged away.
    codediff_ms: Vec<f64>,
    /// Same shape as `tools`: milliseconds inside `ExternalTool::line_labels` (which already
    /// produces line-level labels directly, so no extra projection step to include), `Some(one
    /// entry per repeat)` in lockstep with `tools`' `Some`, `None` when `tools`' entry is `None`.
    tool_ms: Vec<Option<Vec<f64>>>,
    /// Milliseconds tree-sitter alone spends parsing `before` and `after` into ASTs (summed across
    /// both sides, like `codediff_ms`/`tool_ms` are) - see `treesitter_parse_ms`. A reference lower
    /// bound, not a competing tool: always present (every corpus language parses), never scored for
    /// accuracy. One entry per repeat, same as `codediff_ms`.
    treesitter_ms: Vec<f64>,
    /// Milliseconds GumTree itself (parse+match+edit-script, no process-spawn/JVM-startup
    /// overhead) spent on this fixture inside the persistent batch driver - see
    /// `gumtree_warm_batch`. `None` when the batch driver wasn't available for this run (in
    /// lockstep across every row, not per-fixture like `tool_ms`'s `None`) or - same as
    /// `tools`/`tool_ms` - when this fixture's language is outside GumTree's scope. Accuracy is
    /// identical to the CLI-based `gumtree` entry in `tools` (same algorithm, same generator, same
    /// JSON schema - see `gumtree_touched_from_json`), so there's no separate mismatch count here,
    /// only a second timing. One entry per repeat when present - `main` re-runs the whole-corpus
    /// batch driver `--repeats` times (see `gumtree_warm_batch`'s call site), same all-or-nothing
    /// availability as before, just repeated.
    gumtree_warm_ms: Option<Vec<f64>>,
    /// Milliseconds BDiff itself spent on this fixture inside one persistent Python interpreter -
    /// see `bdiff_warm_batch`. Exactly the same contract as `gumtree_warm_ms` above: `None` when
    /// the batch wasn't available for this run, in lockstep across every row. There is no
    /// per-fixture language scope to collapse here, because BDiff is text-based and applies to
    /// every fixture. Accuracy is identical to the per-process `bdiff` entry in `tools` (same
    /// library call), so this is a second timing only.
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

    // Mismatch/accuracy counts are computed once, on the first repeat only - the algorithms under
    // test are all deterministic (same input, same output, every call), so re-deriving them on
    // every repeat would be pure wasted work, not a second independent measurement the way timing
    // is. Only wall-clock time is re-measured `repeats` times below.
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
            // A tool that fails on one fixture is a gap in that tool's coverage, not a reason to
            // abandon a benchmark run over the whole corpus. This used to be a `?`, and on
            // 2026-08-20 that ended a full run 59 fixtures in, when GumTree's `css-phcss`
            // generator produced empty output for `css-fortawesome-font-awesome-upgrade-version-
            // comment` (exit status 0, so the `status.success()` check above did not catch it).
            // `score_accuracy` already treats a per-tool failure this way, recording an `error`
            // status and continuing; the timing path diverging from it was an oversight rather
            // than a decision. A failed tool contributes no timing and no mismatch count for this
            // fixture, which every consumer already reads as "not scored" rather than as a zero.
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

/// Prints every before/after line where codediff or an external tool disagrees with the human
/// mapping's touched/untouched call for `name`, instead of the summary table - the raw material
/// for understanding why a fixture's mismatch count is what it is.
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
        bail!("--tools only applies to --accuracy-csv runs (see its own help text for why)");
    }
    // `handmade_test_code_pair` per name rather than filtering `handmade_test_code_pairs`, which
    // reads and tree-sitter-parses all ~500 corpus fixtures before any filter could apply: that
    // sweep, not the scoring, is what a scoped run spends its time on. Measured on one fixture,
    // release build: 18.8s filtering the full load, 0.6s loading only what was asked for.
    let mut test_diffs: HashMap<String, (Code, Code)> = if args.fixtures.is_empty() {
        helper::handmade_test_code_pairs()?
    } else {
        args.fixtures
            .iter()
            .map(|name| {
                let pair = helper::handmade_test_code_pair(name)
                    .with_context(|| format!("no fixture named '{name}'"))?;
                Ok((name.clone(), pair))
            })
            .collect::<Result<_>>()?
    };

    // `handmade_test_code_pairs` always hands back a fresh `.clone()` of its internal cache (see
    // its own doc comment), and `Code`'s hand-written `Clone` deliberately drops `ast_metadata`
    // back to `None` on every clone (see `Code`'s doc comment for why - a cloned `tree_sitter::
    // Tree` doesn't preserve its root node's id, so carrying stale metadata across a clone would
    // silently corrupt lookups). That means every `Code` here arrives metadata-less regardless of
    // what the on-disk fixture loader itself does, and - since `metadata_of` only ever *borrows*,
    // never writes back through a `&Code` - nothing downstream can ever populate it once `main`
    // only holds shared references. Call `ensure_parsed` once per fixture, here, before any
    // scoring/timing begins: every one of `score_fixture`'s (possibly `--repeats`-many) `diff_code`
    // calls below reads the *same* `Code` value (no further cloning happens in this file), so this
    // one-time cost is paid once per fixture for the whole run, not once per `metadata_of` call
    // per phase per repeat (confirmed 2026-07-26: that was 20 recomputes for a single diff before
    // this fix - see TODO.md's "Found the real bottleneck" entry).
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

    // Checked before any timing machinery below: an accuracy run does no timing at all, so it
    // must not pay for the warm-JVM batch passes (`gumtree_warm_batch`, repeated `--repeats`
    // times) that only exist to produce runtime numbers.
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
    // Repeated `args.repeats` times, same as every other timing below (see `Args::repeats`) -
    // `gumtree_warm_batch` re-parses/re-diffs the whole corpus inside one persistent JVM per call,
    // so each repeat here is a genuine independent wall-clock sample, not a cached replay.
    let mut gumtree_warm_runs: Vec<HashMap<String, f64>> = Vec::with_capacity(args.repeats);
    for repeat in 0..args.repeats {
        // This whole tool has no other progress output during a run that can easily take several
        // minutes (a fresh GumTree JVM per fixture below, plus this whole-corpus warm-batch pass
        // repeated `args.repeats` times) - stderr so it stays visible even when stdout/the final
        // table is redirected to a file.
        eprintln!(
            "gumtree_warm_batch: repeat {}/{}...",
            repeat + 1,
            args.repeats
        );
        match gumtree_warm_batch(&warm_fixtures)? {
            Some(results) => gumtree_warm_runs.push(results),
            // All-or-nothing per `Row::gumtree_warm_ms`'s doc comment: if the batch driver isn't
            // available at all, every repeat will report the same `None`, so bail out of the loop
            // on the first miss rather than repeating a doomed call `args.repeats` times.
            None => break,
        }
    }
    let gumtree_warm_available = gumtree_warm_runs.len() == args.repeats && args.repeats > 0;

    // Same shape as the GumTree warm pass above, one persistent interpreter instead of one
    // persistent JVM - see `bdiff_warm_batch` for why BDiff needs the same cold/warm split.
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
        // A fixture outside GumTree's language scope has no entry in any repeat's batch results
        // (see `gumtree_warm_batch`) - `Some(vec![])` there would misrepresent "not applicable" as
        // "applicable but somehow always empty," so collapse it back to `None` per-fixture, same
        // as `tool_ms`'s per-fixture `None` for an out-of-scope tool.
        let warm_ms = warm_ms.filter(|v| !v.is_empty());
        let bdiff_warm = bdiff_warm_available
            .then(|| {
                bdiff_warm_runs
                    .iter()
                    .filter_map(|results| results.get(name).copied())
                    .collect::<Vec<f64>>()
            })
            .filter(|v| !v.is_empty());
        rows.push(score_fixture(
            name,
            before,
            after,
            args.repeats,
            warm_ms,
            bdiff_warm,
        )?);
    }
    let elapsed = started.elapsed();

    // Worst codediff offenders first, so the fixtures where line-level scoring disagrees most
    // with codediff's own (node-level) view of its accuracy are the first thing visible.
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

/// Per-tool timing: total and mean milliseconds across every fixture, each tool timed identically
/// (see `Row::codediff_ms`/`tool_ms`'s doc comments) so the totals are directly comparable.
/// Mean per-fixture coefficient of variation (stddev / mean, expressed as a percentage) across
/// `samples`' repeats - one value per fixture that had at least 2 repeats, then averaged. Answers
/// "how noisy is a typical single measurement for this tool," as a single summary number
/// alongside the full per-repeat spread `write_csv` records - a fixture with only 1 repeat
/// contributes nothing (there's no spread to measure from a single sample).
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

    // Printed first, above codediff, since it's a lower bound every other row's cost sits on top
    // of, not another tool competing with them. "Total"/"Mean"/n now flatten every repeat into one
    // sample (n = fixtures * repeats), same convention for every row below - "CoV %" is the
    // separate per-fixture-then-averaged spread measure (see `mean_coefficient_of_variation`),
    // not derivable from the flattened total/mean alone.
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
        // Mean is over fixtures this tool was actually scored on, not every fixture in the
        // corpus - dividing by `rows.len()` would understate a language-scoped tool's real
        // per-fixture cost by mixing in zero-cost "not applicable" fixtures it never ran on.
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

    // Only printed when the batch driver actually ran - see `gumtree_warm_batch`'s doc comment for
    // when that's `None` across every row.
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

    // The BDiff equivalent, same reason: without it BDiff's per-process row is ~97% Python import
    // overhead (see `bdiff_warm_batch`).
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

/// Serializes a repeat-timing sample as a single CSV field: every repeat's value, in run order,
/// joined by `;` - e.g. `"12.3;13.1;12.8"` for 3 repeats. Keeps `benchmark_other.csv`'s column
/// count and shape stable regardless of `--repeats` (1 column per metric, same as before repeats
/// existed) while still recording every individual measurement, not a mean - `benchmark_other_
/// report.py` splits this back into a `list[float]` per fixture. A single-repeat run (`--repeats
/// 1`) produces a one-element field, so the format is a strict superset of the old single-float
/// column, not a breaking change to what "no repeats" output looks like.
fn join_ms(values: &[f64]) -> String {
    values
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(";")
}

/// Maps a whole-file *character* offset onto the `(row, byte column)` space `TextRange` uses
/// everywhere in this codebase (tree-sitter's own convention, passed through unchanged by
/// `TextRange::from_treesitter_range`). GumTree reports node positions as character offsets into
/// the file, so its spans need this before they can be compared against node extents; building
/// the table once per file keeps the conversion linear rather than rescanning per span.
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

/// A single-line `TextRange` covering byte columns `[start, end)` on `row` - the shape both
/// difftastic (`changes[].start`/`.end`) and diffsitter (`start_position`/`end_position`) report
/// their sub-line spans in.
fn span_on_row(row: usize, start: usize, end: usize) -> TextRange {
    TextRange {
        start_row: row,
        start_column: start,
        end_row: row,
        end_column: end.max(start),
    }
}

/// The changed regions each AST-aware external tool reports, as `(before_spans, after_spans)` in
/// the same space `human_mapping::node_extents` produces node extents in.
///
/// `None` for the purely line-based tools (Unix diff and the four git algorithms) by
/// construction, not by omission: they report whole lines with no sub-line structure at all, so
/// projecting one onto nodes would mark every node on a changed line as changed. That isn't a
/// worse node score, it's a different (and meaningless) question - which is why the CSV leaves
/// their node columns empty rather than filling in a number that would read as comparable.
///
/// Coordinate conventions, all three verified empirically (2026-08-19) against a fixture with a
/// multi-byte character before the change, since a char/byte mix-up would silently shift every
/// span past any non-ASCII text and is invisible on ASCII-only input:
/// - difftastic's `changes[].start`/`.end` and diffsitter's `*_position.column` are **byte**
///   columns within the line - the same convention `tree_sitter::Node`'s own positions (and
///   therefore `node_extents`) use, so those two need no conversion at all.
/// - GumTree's `[start,end]` are whole-file **character** offsets and do need converting; see
///   `char_offset_table`.
fn tool_node_spans(
    tool: ExternalTool,
    before: &Code,
    after: &Code,
) -> Option<Result<(Vec<TextRange>, Vec<TextRange>)>> {
    match tool {
        // These five report changed *lines* and nothing finer - there is no sub-line structure in
        // the output to project onto this codebase's AST at all. `None` here is what makes
        // `score_accuracy` record them as `line_only` rather than as an error or a perfect zero.
        ExternalTool::UnixDiff
        | ExternalTool::GitMyers
        | ExternalTool::GitMinimal
        | ExternalTool::GitPatience
        | ExternalTool::GitHistogram => None,
        // BDiff and Neovim are *not* line-only, despite both matching lines with libxdiff-class
        // machinery: BDiff's edit script carries `str_diff` character offsets and Neovim paints
        // `DiffText` per column. Both were scored `line_only` here until 2026-08-24, which meant
        // the two tools in this comparison whose distinctive output is sub-line were the two whose
        // sub-line output was never read. See their span builders below.
        ExternalTool::BDiff => Some(bdiff_node_spans(before, after)),
        ExternalTool::NvimDiff => Some(nvim_node_spans(before, after)),
        ExternalTool::GumTree => Some(gumtree_node_spans(before, after)),
        ExternalTool::Difftastic => Some(difftastic_node_spans(before, after)),
        ExternalTool::Diffsitter => Some(diffsitter_node_spans(before, after)),
    }
}

/// The `TextRange` covering characters `[start_char, end_char)` of `row` (0-based), converted to
/// the **byte** columns `TextRange` uses everywhere else in this codebase.
///
/// BDiff is a Python program, so every offset in its edit script indexes a Python `str` - i.e.
/// characters, not bytes. `span_on_row` takes byte columns. Getting this wrong is invisible on
/// ASCII and silently shifts every span past the first non-ASCII character on the line, which is
/// the same trap `tool_node_spans`' doc comment already records for the other three tools.
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

/// Sorts and coalesces overlapping/adjacent spans. Purely a cost optimization - `nodes_touched_by`
/// tests every node against every span, and diffsitter reports one span per character, so a
/// 2,000-character edit would otherwise mean 2,000 comparisons per node. Merging preserves exactly
/// which positions are covered, so it cannot change any score.
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

/// One fixture's accuracy row - see `Args::accuracy_csv` for what the node columns do and do not
/// mean.
struct AccuracyRow {
    solution: String,
    /// Provenance from `src/test/data/sample.csv`, keyed by its `promoted_to` column. Empty for
    /// the handmade fixtures that were never promoted from a sample (60 of 432 as of 2026-08-19) -
    /// they're real corpus members, so they get a row with blank provenance rather than being
    /// dropped.
    language: String,
    repository: String,
    commit: String,
    path: String,
    total_lines: usize,
    total_nodes: usize,
    total_leaf_nodes: usize,
    /// How many of `total_nodes` are visible in a ground-truth rendering - the denominator the
    /// `*_visible_node_mismatches` columns need to be read as a rate rather than an absolute.
    total_visible_nodes: usize,
    /// Per tool (plus codediff itself), in `ExternalTool::ALL` order with codediff first.
    scores: Vec<ToolScore>,
}

/// One tool's agreement with the human mapping on one fixture. `line_mismatches` and the node
/// counts are `None` for different reasons - see `status`.
struct ToolScore {
    name: &'static str,
    line_mismatches: Option<usize>,
    node_mismatches: Option<usize>,
    leaf_node_mismatches: Option<usize>,
    /// Same projection as `node_mismatches`, restricted to the nodes that actually reach the
    /// screen in a *ground-truth* rendering of this fixture - see `visible_filter` in
    /// `accuracy_row_for` for why visibility is judged against the human mapping rather than
    /// against either codediff's or the tool's own output.
    visible_node_mismatches: Option<usize>,
    /// `ok`, `unsupported` (the tool has no parser/generator for this language - not a failure,
    /// and deliberately not scored as 0, which would read as a perfect result), `error` (the tool
    /// was supposed to handle this language and didn't), or `line_only` (Unix diff and the four
    /// git algorithms, which have no sub-line output at all by construction).
    status: &'static str,
}

/// Reads `src/test/data/sample.csv` into `promoted_to -> (language, repository, commit, path)`,
/// the provenance join `AccuracyRow` carries so a row can be traced back to the real-world commit
/// it was sampled from.
fn sample_provenance() -> Result<HashMap<String, (String, String, String, String)>> {
    let path = std::path::Path::new("src/test/data/sample.csv");
    let mut out = HashMap::new();
    if !path.exists() {
        return Ok(out);
    }
    let mut reader = csv::Reader::from_path(path)
        .with_context(|| format!("reading sample provenance from {path:?}"))?;
    for record in reader.deserialize::<HashMap<String, String>>() {
        let record = record.context("parsing a sample.csv row")?;
        let Some(promoted) = record.get("promoted_to") else {
            continue;
        };
        if promoted.is_empty() {
            continue;
        }
        let field = |key: &str| record.get(key).cloned().unwrap_or_default();
        out.insert(
            promoted.clone(),
            (
                field("language"),
                field("repository"),
                field("commit"),
                field("path"),
            ),
        );
    }
    Ok(out)
}

/// Scores every tool's line- and node-level agreement with the human mapping for one fixture.
///
/// Ground truth for both granularities comes from the same synthetic `ASTDiff`
/// (`human_mapping::as_ast_diff`) the line-level benchmark already uses, so the two granularities
/// are two views of one ground truth rather than two independently-derived ones.
fn score_accuracy(
    name: &str,
    before: &Code,
    after: &Code,
    provenance: &HashMap<String, (String, String, String, String)>,
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

    // Leaf-only views of the same labelings: the all-nodes count includes every ancestor of a
    // change (see `nodes_touched_by`), so a leaves-only count is reported alongside it to
    // separate "which tokens changed" from "how deep is this grammar's tree".
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

    // Visible-only views of the same labelings - the nodes whose classification actually reaches
    // the screen when the diff is rendered, per `diff::nodes::structurally_visible_node_ids`. A mismatch on a
    // pure container (a `block`, an `argument_list`) has no independent effect on what a reader
    // sees, so this separates "how much of the disagreement is user-visible" from the raw count.
    //
    // Visibility is judged against `truth_ast` - the *human* mapping - not against each tool's
    // own output. This is a deliberate divergence from `diff::text::visible_ids_for_side`'s own
    // doc'd choice (which judges codediff's real diff by codediff's own rendering, answering
    // "what does the user see right now"): here the question is comparative, so every tool must
    // be scored against one fixed, tool-independent set of visible nodes. Judging each tool by
    // its own rendering would give every tool a different denominator and make the columns
    // incomparable - and judging all of them by *codediff's* rendering would quietly privilege
    // codediff. The ground-truth mapping is the only basis that is neutral between them.
    //
    // Deliberately *not* the same thing as the leaf view above, in either direction: an
    // `Identical` leaf inside a terminal subtree is never reached by the renderer (leaf, not
    // visible), and a `MatchButNotIdentical` container whose own content diverges emits its own
    // span (visible, not a leaf). If the two columns come out close, that's a coincidence worth
    // noting, not a cross-check.
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

    // codediff itself, scored through the identical projection so its columns sit on the same
    // scale as the external tools' (and, deliberately, not on the same scale as its own
    // `benchmark_optimal_solutions` node-mismatch number).
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
                // Unix diff: no sub-line output exists to project onto nodes at all.
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

    let (language_name, repository, commit, path) = provenance
        .get(name)
        .cloned()
        .unwrap_or_else(|| (String::new(), String::new(), String::new(), String::new()));

    Ok(AccuracyRow {
        solution: name.to_string(),
        language: if language_name.is_empty() {
            format!("{language:?}")
        } else {
            language_name
        },
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

/// Writes the accuracy CSV: one row per fixture, one line/node/leaf-node mismatch column per tool
/// plus a status column, with `sample.csv` provenance for cross-referencing.
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

/// `--accuracy-csv`'s whole run: score every fixture with a human mapping, print a compact
/// per-tool summary, and write the CSV. No timing at all - accuracy is deterministic, so there is
/// nothing to repeat.
fn run_accuracy(
    names: &[String],
    test_diffs: &HashMap<String, (Code, Code)>,
    path: &std::path::Path,
    selection: &ToolSelection,
) -> Result<()> {
    let provenance = sample_provenance()?;
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

    // Coverage first, because it is what makes the raw totals below un-comparable: each tool
    // scores only the fixtures its own parser supports (GumTree covers well under half the
    // corpus), so a tool that skipped the hard half would otherwise look best simply for having
    // attempted less.
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

    // The only apples-to-apples table: the fixtures every tool actually scored. Rates, not raw
    // counts, since the subset's own denominators are what make them readable.
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
        // Empty field, not "0" - a tool this fixture is out of scope for (`ExternalTool::supports`
        // was false) didn't score 0 mismatches, it wasn't scored at all. Downstream readers
        // (`benchmark_other_report.py`) must treat a blank the same way pandas/csv already do:
        // excluded from that tool's aggregate, not coerced to zero.
        record.extend(row.tools.iter().zip(&row.tool_ms).flat_map(|(cell, ms)| {
            [
                cell.map(|(mismatches, _)| mismatches.to_string())
                    .unwrap_or_default(),
                ms.as_deref().map(join_ms).unwrap_or_default(),
            ]
        }));
        // Blank whenever the batch driver wasn't available for this run at all (see
        // `gumtree_warm_batch`'s doc comment), same "blank means not scored" convention as above -
        // not blank per-fixture the way `tool_ms` can be, since GumTree's language scope already
        // determines that before the batch driver even runs.
        record.push(
            row.gumtree_warm_ms
                .as_deref()
                .map(join_ms)
                .unwrap_or_default(),
        );
        // Same contract as the column above - see `Row::bdiff_warm_ms`.
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
        // end == 6 is the newline right after "line0" - the range should stop at line 0, not
        // spill into line 1 just because `end` technically points past it.
        assert_eq!(gumtree_line_range(contents, 0, 6), 0..=0);
    }

    /// Regression test for a real crash: GumTree's `[start,end]` are *character* offsets, not
    /// byte offsets. A file containing multi-byte UTF-8 text (e.g. Thai, 3 bytes/char) before the
    /// touched range used to panic with "byte index N is not a char boundary" when
    /// `gumtree_line_range` sliced `contents` directly at the character-offset value as if it were
    /// a byte index - confirmed on a real fixture (a RustDesk Thai-locale string constant change).
    #[test]
    fn gumtree_line_range_handles_multi_byte_utf8_text_before_the_touched_range() {
        // "สวัสดี" (Thai, 6 characters, 18 bytes) sits entirely on line 0; the touched range is on
        // line 1. A byte-index slice at character offset 7 (mid-line-1) would land inside one of
        // line 0's multi-byte characters and panic.
        let contents = "สวัสดี\nhello\nworld\n";
        assert_eq!(gumtree_line_range(contents, 7, 9), 1..=1);
    }

    #[test]
    fn gumtree_line_range_clamps_an_end_offset_past_the_end_of_the_file() {
        let contents = "line0\nline1\n";
        // `end` one character past the last character in `contents` (GumTree's own half-open
        // convention) must not panic or index out of bounds.
        let total_chars = contents.chars().count();
        assert_eq!(gumtree_line_range(contents, 6, total_chars), 1..=1);
    }

    /// GumTree reports character offsets; node extents live in tree-sitter's row/*byte*-column
    /// space. The table has to bridge exactly that, including past a multi-byte character.
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

    /// `merge_spans` is a pure cost optimization (diffsitter emits one span per character), so it
    /// must coalesce overlapping and adjacent spans without ever changing which positions are
    /// covered - and must leave a genuine gap alone.
    #[test]
    fn merge_spans_coalesces_adjacent_and_overlapping_spans_but_keeps_gaps() {
        let merged = merge_spans(vec![
            span_on_row(0, 4, 5),
            span_on_row(0, 0, 2),
            span_on_row(0, 2, 4), // adjacent to the first, overlaps nothing
            span_on_row(1, 0, 3),
            span_on_row(1, 1, 9), // overlaps the previous
        ]);
        let as_tuples: Vec<_> = merged
            .iter()
            .map(|s| (s.start_row, s.start_column, s.end_row, s.end_column))
            .collect();
        assert_eq!(as_tuples, vec![(0, 0, 0, 5), (1, 0, 1, 9)]);

        // A real gap must survive: columns 0-1 and 5-6 are not one span.
        let merged = merge_spans(vec![span_on_row(0, 0, 1), span_on_row(0, 5, 6)]);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn merge_spans_handles_the_empty_case() {
        assert!(merge_spans(Vec::new()).is_empty());
    }

    /// The `,0` trap in `git_line_labels`'s doc comment, as a fixed input: a pure insertion whose
    /// before-side header is `@@ -3,0 +4,2 @@`. Line 3 of the before side is the *anchor*, not a
    /// touched line, so the before side must come back all-false. This is the one parser bug that
    /// produces plausible-looking rates rather than an obvious failure, so it is pinned here
    /// rather than left to the corpus-wide differential check below.
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

    /// GNU diffutils and git's libxdiff are independent implementations of the same Myers family,
    /// so on the same input they must mark the same lines. Any divergence here is a bug in
    /// `git_line_labels`'s header parsing, **not** a finding about the algorithms - which is
    /// exactly why this compares against the long-standing `unix_diff` path rather than against
    /// a hand-written expectation.
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

    /// BDiff's `str_diff` offsets are **inclusive** on both ends and index a Python `str`, i.e.
    /// characters. Both conventions are transcribed from live output (see
    /// `bdiff_spans_from_script`'s doc comment); either one wrong is invisible on ASCII-only
    /// input and silently shifts every span, so they get a test with a multi-byte character in
    /// front of the change.
    #[test]
    fn bdiff_spans_from_script_reads_str_diff_as_inclusive_character_offsets() {
        // 'é' is two bytes, so byte columns run ahead of character offsets from column 4 on.
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

        // "let é = abc" is 12 bytes (the 'é' costs two), so the change starts at byte column 12.
        assert_eq!(before_spans, vec![span_on_row(0, 12, 14)]);
        assert_eq!(after_spans, vec![span_on_row(0, 12, 15)]);
        // And the columns really do name the changed text on each side.
        assert_eq!(&before.contents[12..14], "de");
        assert_eq!(&after.contents[12..15], "XYZ");
    }

    /// A pure insertion into a line gives the before side an empty `[]` range - "nothing here" -
    /// which must contribute no span rather than a zero-width one at offset 0.
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

    /// Every mode without a `str_diff` still has to contribute its whole lines, on the same sides
    /// `bdiff_line_labels` documents - otherwise BDiff would be scored only on the lines it calls
    /// updates, which is not a comparison of the same thing.
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

        // Characters 1..3 of "aéb" are "éb", which starts at byte 1 and ends at byte 4.
        assert_eq!(span_on_row_chars(&lines, 0, 1, 3), span_on_row(0, 1, 4));
        // Past the end of the line clamps rather than panicking.
        assert_eq!(span_on_row_chars(&lines, 0, 0, 99), span_on_row(0, 0, 4));
        assert_eq!(span_on_row_chars(&lines, 9, 0, 1), span_on_row(9, 0, 0));
    }

    /// The mode-to-side rules from `bdiff_line_labels`'s doc comment, against a hand-built script
    /// so they are pinned without needing BDiff installed. The two that are judgement calls, and
    /// so the two most likely to be changed by accident, are `insert`/`delete` (whose *other*
    /// side carries an anchor line number that must not be marked) and `copy` (whose source block
    /// stays in place unchanged and must not be marked).
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
        // Before: only the delete's line 5. The insert's src_line 2 is an anchor, and the copy's
        // source block (lines 1-2) is unchanged text still present in both files.
        assert_eq!(touched(&before_touched), vec![5]);
        // After: the insert's line 3, and the copy's destination block (lines 4-5). The delete's
        // dest_line 1 is an anchor.
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
