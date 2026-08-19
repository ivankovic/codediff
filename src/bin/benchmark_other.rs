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
use std::process::Command;

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
}

/// An external, non-codediff diff tool being scored against the human-authored mapping. Adding a
/// tool means adding a variant here plus a `line_labels` match arm - `main`'s corpus loop, the
/// table, and the CSV all pick it up automatically via `ExternalTool::ALL`.
#[derive(Clone, Copy)]
enum ExternalTool {
    UnixDiff,
    GumTree,
    Difftastic,
    Diffsitter,
}

impl ExternalTool {
    const ALL: &'static [ExternalTool] = &[
        ExternalTool::UnixDiff,
        ExternalTool::GumTree,
        ExternalTool::Difftastic,
        ExternalTool::Diffsitter,
    ];

    fn name(&self) -> &'static str {
        match self {
            ExternalTool::UnixDiff => "unix_diff",
            ExternalTool::GumTree => "gumtree",
            ExternalTool::Difftastic => "difftastic",
            ExternalTool::Diffsitter => "diffsitter",
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
    ///
    /// Difftastic and diffsitter coverage is each tool's own compiled-in language set, confirmed
    /// live via `difft --list-languages` and `diffsitter list` against the actual installed
    /// builds - see `difftastic_extension`/`diffsitter_file_type`.
    fn supports(&self, language: Language) -> bool {
        match self {
            ExternalTool::UnixDiff => true,
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
            ExternalTool::GumTree => gumtree_line_labels(before, after),
            ExternalTool::Difftastic => difftastic_line_labels(before, after),
            ExternalTool::Diffsitter => diffsitter_line_labels(before, after),
        }
    }
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
        Language::Java => Some(("java-jdt", "java")), // Stable
        Language::CSS => Some(("css-phcss", "css")),  // Stable
        Language::Rust => Some(("rust-treesitter-ng", "rs")), // Testing
        Language::CPP => Some(("cpp-treesitter-ng", "cpp")), // Testing
        Language::Kotlin => Some(("kotlin-treesitter-ng", "kt")), // Testing
        Language::C => Some(("c-treesitter-ng", "c")), // Testing
        Language::Go => Some(("go-treesitter-ng", "go")), // Testing
        Language::Python => Some(("python-treesitter-ng", "py")), // Testing
        Language::TypeScript => Some(("ts-treesitter-ng", "ts")), // Testing
        Language::JavaScript => Some(("js-treesitter-ng", "js")), // Testing
        Language::CSharp => Some(("cs-treesitter-ng", "cs")), // Testing
        _ => None,
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

/// Path to the GumTree CLI script (`bin/gumtree` in its built distribution), from the `GUMTREE_BIN`
/// environment variable - not bundled or auto-installed, since it's a separate JVM project with
/// its own build (JDK 17 + Gradle; see the project's own install docs). Deliberately errors loudly
/// rather than silently skipping: unlike a language `ExternalTool::supports` excludes, a missing
/// binary for a language it claims to support is a real configuration problem.
fn gumtree_bin() -> Result<std::path::PathBuf> {
    external_tool_bin(
        "GUMTREE_BIN",
        "point it at GumTree's built bin/gumtree script",
    )
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
    let (generator, ext) = gumtree_generator(language)
        .with_context(|| format!("no GumTree generator for {language:?}"))?;
    let gumtree = gumtree_bin()?;

    let (before_file, after_file) = write_temp_pair(before, after, Some(&format!(".{ext}")))?;

    let output = Command::new(&gumtree)
        .args(["textdiff", "-g", generator, "-f", "JSON"])
        .arg(before_file.path())
        .arg(after_file.path())
        .output()
        .with_context(|| format!("running {gumtree:?} textdiff -g {generator}"))?;
    if !output.status.success() {
        bail!(
            "gumtree exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("parsing gumtree JSON output")?;
    gumtree_touched_from_json(before, after, &json)
}

/// Shared by both ways of running GumTree - the CLI subprocess (`gumtree_line_labels`) and the
/// batch driver (`gumtree_warm_batch`) - since both emit the exact same `{"matches": [...],
/// "actions": [...]}` schema (the driver reuses GumTree's own `ActionsIoUtils.toJson`, see
/// `research/drivers/gumtree-batch/BatchDriver.java`'s doc comment). See `gumtree_line_labels`'s
/// doc comment for what `matches`/`actions` mean.
fn gumtree_touched_from_json(
    before: &Code,
    after: &Code,
    json: &serde_json::Value,
) -> Result<(Vec<bool>, Vec<bool>)> {
    let matches = json["matches"]
        .as_array()
        .context("gumtree JSON has no `matches` array")?;
    let actions = json["actions"]
        .as_array()
        .context("gumtree JSON has no `actions` array")?;

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
        let kind = action["action"]
            .as_str()
            .context("gumtree action missing `action`")?;
        let tree = action["tree"]
            .as_str()
            .context("gumtree action missing `tree`")?;
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
                    None => eprintln!(
                        "gumtree: no `matches` entry for {kind} tree {tree:?}, after-side line(s) not marked"
                    ),
                }
            }
            other => bail!("unrecognized gumtree action kind {other:?}"),
        }
    }

    Ok((before_touched, after_touched))
}

/// Runs every GumTree-supported fixture through one persistent JVM instead of one `gumtree
/// textdiff` subprocess per fixture (`gumtree_line_labels`) - see
/// `research/drivers/gumtree-batch/BatchDriver.java`'s doc comment for why: a fresh subprocess
/// pays JVM startup/JIT warmup on every single invocation, which dominates `gumtree_ms` for small
/// files (see `benchmark_other_runtime.png`'s gumtree violin sitting almost flat regardless of
/// file size). This isolates GumTree's own parse+match+edit-script cost from that overhead, as a
/// second, additional timing column - not a replacement for `gumtree_ms`, which is still an honest
/// number for "cost of invoking the CLI the way most users would."
///
/// Returns `Ok(None)` (not an error) when the batch driver isn't available - `GUMTREE_BIN` unset,
/// or `research/drivers/gumtree-batch/build.sh` hasn't been run yet - since this is an optional
/// second data point on top of the required `ExternalTool::GumTree` pass, unlike `gumtree_bin()`'s
/// own hard failure when a fixture claims GumTree support but its CLI binary is missing entirely.
///
/// Feeds the whole batch through the driver's stdin/stdout in one process (see the driver's doc
/// comment for the line-delimited-JSON protocol), writing the request body from a second thread so
/// a response stream larger than the OS pipe buffer can't deadlock against still-unwritten
/// request lines - `Command::wait_with_output` already reads stdout/stderr off background threads
/// for the same reason, this just extends that to stdin.
fn gumtree_warm_batch(fixtures: &[(&str, &Code, &Code)]) -> Result<Option<HashMap<String, f64>>> {
    let Ok(gumtree) = gumtree_bin() else {
        return Ok(None);
    };
    let Some(gumtree_dir) = gumtree.parent().and_then(|bin| bin.parent()) else {
        return Ok(None);
    };
    let jar = gumtree_dir.join("lib/gumtree.jar");
    let driver_out = std::path::Path::new("research/drivers/gumtree-batch/out");
    if !jar.is_file() || !driver_out.join("BatchDriver.class").is_file() {
        eprintln!(
            "gumtree_warm_ms: skipping (build the batch driver first: GUMTREE_BIN=... research/drivers/gumtree-batch/build.sh)"
        );
        return Ok(None);
    }

    // Kept alive until the JVM process below has read every one of them - the driver reads these
    // paths lazily off its stdin, well after this loop returns.
    let mut before_files = Vec::with_capacity(fixtures.len());
    let mut after_files = Vec::with_capacity(fixtures.len());
    let mut requests = String::new();
    for (name, before, after) in fixtures {
        let language = before.metadata.language.unwrap_or_default();
        let Some((generator, ext)) = gumtree_generator(language) else {
            continue;
        };
        let (before_file, after_file) = write_temp_pair(before, after, Some(&format!(".{ext}")))?;
        requests.push_str(
            &serde_json::json!({
                "id": name,
                "generator": generator,
                "before": before_file.path().display().to_string(),
                "after": after_file.path().display().to_string(),
            })
            .to_string(),
        );
        requests.push('\n');
        before_files.push(before_file);
        after_files.push(after_file);
    }

    let mut child = Command::new("java")
        .args([
            "-cp",
            &format!("{}:{}", jar.display(), driver_out.display()),
            "BatchDriver",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("spawning the GumTree batch driver (is `java` on PATH?)")?;
    let mut stdin = child
        .stdin
        .take()
        .context("batch driver child has no stdin")?;
    let writer = std::thread::spawn(move || stdin.write_all(requests.as_bytes()));
    let output = child
        .wait_with_output()
        .context("waiting for the GumTree batch driver")?;
    writer
        .join()
        .expect("batch driver stdin-writer thread panicked")
        .context("writing batch driver stdin")?;
    if !output.status.success() {
        bail!(
            "GumTree batch driver exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let mut results = HashMap::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let json: serde_json::Value = serde_json::from_str(line)
            .with_context(|| format!("parsing batch driver response line {line:?}"))?;
        let id = json["id"]
            .as_str()
            .context("batch driver response missing `id`")?
            .to_string();
        if let Some(error) = json["error"].as_str() {
            bail!("GumTree batch driver failed on {id:?}: {error}");
        }
        let ms = json["ms"]
            .as_f64()
            .context("batch driver response missing `ms`")?;
        results.insert(id, ms);
    }
    Ok(Some(results))
}

/// Parses the `[start,end]` character-offset suffix off a GumTree node-reference string like
/// `"SimpleName: foo [12,15]"` - the text before it (kind, optional `: text`) is irrelevant here,
/// only the position matters. Anchored to the end of the string since a node's own text can itself
/// contain brackets or commas (e.g. an array-literal leaf), which a naive first-bracket search
/// would misparse.
fn gumtree_node_offsets(node_ref: &str) -> Result<(usize, usize)> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"\[(\d+),(\d+)\]$").unwrap());
    let caps = re
        .captures(node_ref)
        .with_context(|| format!("no [start,end] suffix in {node_ref:?}"))?;
    let start: usize = caps[1].parse()?;
    let end: usize = caps[2].parse()?;
    Ok((start, end))
}

/// 0-indexed line numbers `[start, end)` (a half-open character range into `contents`) touches,
/// inclusive of the line containing `end`'s last character - a range landing exactly on a line
/// boundary doesn't spuriously pull in the following, untouched line.
fn gumtree_line_range(contents: &str, start: usize, end: usize) -> std::ops::RangeInclusive<usize> {
    // `start`/`end` are *character* offsets (from GumTree's own `[start,end]` node reference, see
    // `gumtree_node_offsets`), not byte offsets - slicing `contents` directly at these values
    // panics ("not a char boundary") on any file containing multi-byte UTF-8 characters before the
    // offset (confirmed empirically: a Thai-locale string constant change crashed here). Translate
    // the character offset to its real byte offset first; a char offset past the end of `contents`
    // (GumTree's own `end` can point one past the last character) clamps to `contents.len()`.
    let byte_offset_of_char = |char_offset: usize| -> usize {
        contents
            .char_indices()
            .nth(char_offset)
            .map(|(byte_idx, _)| byte_idx)
            .unwrap_or(contents.len())
    };
    let line_of = |offset: usize| {
        contents[..byte_offset_of_char(offset)]
            .matches('\n')
            .count()
    };
    line_of(start)..=line_of(end.saturating_sub(1).max(start))
}

/// Path to the `difft` binary, from the `DIFFT_BIN` environment variable - not bundled or
/// auto-installed, same reasoning as `gumtree_bin`. Install with
/// `cargo install --root /var/tmp/codediff-tools difftastic` (installs the `difft` binary to
/// `/var/tmp/codediff-tools/bin/difft`, outside this checkout and outside the system-wide cargo
/// bin directory) and point `DIFFT_BIN` at the result.
fn difftastic_bin() -> Result<std::path::PathBuf> {
    external_tool_bin("DIFFT_BIN", "point it at a built `difft` binary")
}

/// Path to the `diffsitter` binary, from the `DIFFSITTER_BIN` environment variable - see
/// `difftastic_bin`. Install with
/// `cargo install --root /var/tmp/codediff-tools diffsitter`.
fn diffsitter_bin() -> Result<std::path::PathBuf> {
    external_tool_bin("DIFFSITTER_BIN", "point it at a built `diffsitter` binary")
}

/// File extension difftastic's own language auto-detection maps back to `language`, confirmed
/// live against `difft --list-languages` (difftastic v0.69.0, 2026-07). `None` for every corpus
/// language difftastic has no grammar for at all: `Bazel` (also not tree-sitter-parseable by
/// codediff itself - see `Code`'s own doc comment), `MarkDown`, `ProtoBuf`, and `Vimscript`.
/// `Language::Lisp` maps to `.el` - codediff's own extension table (`code/language.rs`) treats
/// `Language::Lisp` as Emacs Lisp specifically, and difftastic lists Emacs Lisp and Common Lisp as
/// separate languages with disjoint extensions, so `.el` is the correct, unambiguous match.
fn difftastic_extension(language: Language) -> Option<&'static str> {
    match language {
        Language::Rust => Some("rs"),
        Language::Python => Some("py"),
        Language::Go => Some("go"),
        Language::Kotlin => Some("kt"),
        Language::Java => Some("java"),
        Language::JavaScript => Some("js"),
        Language::TypeScript => Some("ts"),
        Language::TSX => Some("tsx"),
        Language::C => Some("c"),
        Language::CPP => Some("cpp"),
        Language::CSharp => Some("cs"),
        Language::Ruby => Some("rb"),
        Language::PHP => Some("php"),
        Language::Swift => Some("swift"),
        Language::Scala => Some("scala"),
        Language::LUA => Some("lua"),
        Language::CSS => Some("css"),
        Language::HTML => Some("html"),
        Language::JSON => Some("json"),
        Language::R => Some("R"),
        Language::ShellScript => Some("sh"),
        Language::XML => Some("xml"),
        Language::YAML => Some("yaml"),
        Language::SQL => Some("sql"),
        Language::Dart => Some("dart"),
        Language::Lisp => Some("el"),
        _ => None,
    }
}

/// `-t <FILE_TYPE>` value diffsitter needs for `language`, confirmed live against `diffsitter
/// list` (diffsitter v0.9.0, 2026-07) - a much narrower compiled-in language set than difftastic
/// or GumTree: no `JavaScript`, `HTML`, `Kotlin`, `LUA`, `R`, `Scala`, `Swift`, `XML`, or `YAML`,
/// none of which this build was compiled with a grammar for at all (`diffsitter list`'s output is
/// the full, exhaustive set - there is no generic fallback parser).
fn diffsitter_file_type(language: Language) -> Option<&'static str> {
    match language {
        Language::ShellScript => Some("bash"),
        Language::C => Some("c"),
        Language::CSharp => Some("c_sharp"),
        Language::CPP => Some("cpp"),
        Language::CSS => Some("css"),
        Language::Go => Some("go"),
        Language::Java => Some("java"),
        Language::JSON => Some("json"),
        Language::MarkDown => Some("markdown"),
        Language::PHP => Some("php"),
        Language::Python => Some("python"),
        Language::Ruby => Some("ruby"),
        Language::Rust => Some("rust"),
        Language::TSX => Some("tsx"),
        Language::TypeScript => Some("typescript"),
        _ => None,
    }
}

/// Runs `difft --display json` and reduces its output to the same per-line touched signal every
/// other `ExternalTool` produces. `DFT_UNSTABLE=yes` is required by difftastic itself - JSON
/// output is explicitly marked an unstable feature that may change format in a future release
/// (confirmed live, difftastic v0.69.0 refuses `--display json` without it) - see
/// `difftastic_touched_from_json` for the schema this code depends on.
fn difftastic_line_labels(before: &Code, after: &Code) -> Result<(Vec<bool>, Vec<bool>)> {
    let language = before.metadata.language.unwrap_or_default();
    let ext = difftastic_extension(language)
        .with_context(|| format!("no difftastic extension mapping for {language:?}"))?;
    let difft = difftastic_bin()?;

    let (before_file, after_file) = write_temp_pair(before, after, Some(&format!(".{ext}")))?;

    let output = Command::new(&difft)
        .args(["--display", "json"])
        .env("DFT_UNSTABLE", "yes")
        .arg(before_file.path())
        .arg(after_file.path())
        .output()
        .with_context(|| format!("running {difft:?} --display json"))?;
    if !output.status.success() {
        bail!(
            "difft exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("parsing difft JSON output")?;
    difftastic_touched_from_json(before, after, &json)
}

/// Difftastic's JSON has one relevant field, `chunks`: a list of hunks, each a list of line
/// entries. Each entry has an optional `lhs`/`rhs`, each `{line_number, changes}` - `line_number`
/// is 0-indexed (confirmed empirically against this project's own fixtures, 2026-07: a change on
/// a file's second line reports `line_number: 1`) and `changes` is empty for a context line shown
/// only for readability, non-empty for a line difftastic considers actually touched. Confirmed
/// live that `chunks` only ever contains touched lines, never surrounding context, in JSON mode
/// (unlike difftastic's own terminal display, which does show context) - so every `lhs`/`rhs`
/// entry present here is touched, `changes` non-empty or not. A file with `status: "unchanged"`
/// has no `chunks` key at all, not an empty array.
fn difftastic_touched_from_json(
    before: &Code,
    after: &Code,
    json: &serde_json::Value,
) -> Result<(Vec<bool>, Vec<bool>)> {
    let mut before_touched = vec![false; before.contents.split('\n').count()];
    let mut after_touched = vec![false; after.contents.split('\n').count()];

    let Some(chunks) = json["chunks"].as_array() else {
        return Ok((before_touched, after_touched));
    };

    for chunk in chunks {
        let entries = chunk
            .as_array()
            .context("difft JSON chunk is not an array")?;
        for entry in entries {
            if let Some(line_number) = entry["lhs"]["line_number"].as_u64() {
                if let Some(slot) = before_touched.get_mut(line_number as usize) {
                    *slot = true;
                }
            }
            if let Some(line_number) = entry["rhs"]["line_number"].as_u64() {
                if let Some(slot) = after_touched.get_mut(line_number as usize) {
                    *slot = true;
                }
            }
        }
    }

    Ok((before_touched, after_touched))
}

/// Runs `diffsitter -r json` and reduces its output to the same per-line touched signal every
/// other `ExternalTool` produces. `-t <file-type>` is passed explicitly (see
/// `diffsitter_file_type`) rather than relying on the temp file's extension, the same reasoning as
/// GumTree's explicit `-g <generator>` in `gumtree_line_labels` - an explicit choice documents
/// exactly which parser ran, rather than leaving it to auto-detection.
fn diffsitter_line_labels(before: &Code, after: &Code) -> Result<(Vec<bool>, Vec<bool>)> {
    let language = before.metadata.language.unwrap_or_default();
    let file_type = diffsitter_file_type(language)
        .with_context(|| format!("no diffsitter file type mapping for {language:?}"))?;
    let diffsitter = diffsitter_bin()?;

    let (before_file, after_file) = write_temp_pair(before, after, None)?;

    let output = Command::new(&diffsitter)
        .args(["-n", "-r", "json", "-t", file_type])
        .arg(before_file.path())
        .arg(after_file.path())
        .output()
        .with_context(|| format!("running {diffsitter:?} -r json -t {file_type}"))?;
    if !output.status.success() {
        bail!(
            "diffsitter exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("parsing diffsitter JSON output")?;
    diffsitter_touched_from_json(before, after, &json)
}

/// Diffsitter's JSON has one relevant field, `hunks`: a list of hunk objects, each with exactly
/// one of two keys, `"Old"` or `"New"` (confirmed live, diffsitter v0.9.0: never both in the same
/// hunk object, even for a same-line before/after change - that shows up as two separate hunks,
/// one `Old` and one `New`). Each key holds a list of `{line_index, entries}`, `line_index`
/// 0-indexed the same way as `difftastic_touched_from_json`'s `line_number`. `entries` (the
/// character-level tokens changed on that line) is not needed here - `line_index`'s presence
/// alone means diffsitter considers that line touched.
fn diffsitter_touched_from_json(
    before: &Code,
    after: &Code,
    json: &serde_json::Value,
) -> Result<(Vec<bool>, Vec<bool>)> {
    let mut before_touched = vec![false; before.contents.split('\n').count()];
    let mut after_touched = vec![false; after.contents.split('\n').count()];

    let hunks = json["hunks"]
        .as_array()
        .context("diffsitter JSON has no `hunks` array")?;

    for hunk in hunks {
        let hunk = hunk
            .as_object()
            .context("diffsitter JSON hunk is not an object")?;
        for (side, touched) in [("Old", &mut before_touched), ("New", &mut after_touched)] {
            let Some(entries) = hunk.get(side).and_then(|v| v.as_array()) else {
                continue;
            };
            for entry in entries {
                if let Some(line_index) = entry["line_index"].as_u64() {
                    if let Some(slot) = touched.get_mut(line_index as usize) {
                        *slot = true;
                    }
                }
            }
        }
    }

    Ok((before_touched, after_touched))
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
}

fn score_fixture(
    name: &str,
    before: &Code,
    after: &Code,
    repeats: usize,
    gumtree_warm_ms: Option<Vec<f64>>,
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
        for i in 0..repeats {
            let started = std::time::Instant::now();
            let (tool_before, tool_after) = tool.line_labels(before, after)?;
            ms.push(started.elapsed().as_secs_f64() * 1000.0);
            if i == 0 {
                mismatches = human_mapping::line_disagreement_count(&human_before, &tool_before)
                    + human_mapping::line_disagreement_count(&human_after, &tool_after);
            }
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
    let mut test_diffs = helper::handmade_test_code_pairs()?;

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
        return run_accuracy(&names, &test_diffs, &path);
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
        rows.push(score_fixture(name, before, after, args.repeats, warm_ms)?);
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
    let label_width = ["codediff", "treesitter_parse", "gumtree_warm"]
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
/// `None` for `UnixDiff` by construction, not by omission: a line-based tool reports whole lines
/// with no sub-line structure at all, so projecting it onto nodes would mark every node on a
/// changed line as changed. That isn't a worse node score, it's a different (and meaningless)
/// question - which is why the CSV leaves Unix diff's node columns empty rather than filling in a
/// number that would read as comparable.
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
        ExternalTool::UnixDiff => None,
        ExternalTool::GumTree => Some(gumtree_node_spans(before, after)),
        ExternalTool::Difftastic => Some(difftastic_node_spans(before, after)),
        ExternalTool::Diffsitter => Some(diffsitter_node_spans(before, after)),
    }
}

/// GumTree's changed spans, from the same `textdiff -f JSON` output `gumtree_line_labels` parses
/// (see its doc comment for what `matches`/`actions` mean) - but keeping each action's real
/// character range instead of collapsing it to the lines it touches.
fn gumtree_node_spans(before: &Code, after: &Code) -> Result<(Vec<TextRange>, Vec<TextRange>)> {
    let language = before.metadata.language.unwrap_or_default();
    let (generator, ext) = gumtree_generator(language)
        .with_context(|| format!("no GumTree generator for {language:?}"))?;
    let gumtree = gumtree_bin()?;
    let (before_file, after_file) = write_temp_pair(before, after, Some(&format!(".{ext}")))?;

    let output = Command::new(&gumtree)
        .args(["textdiff", "-g", generator, "-f", "JSON"])
        .arg(before_file.path())
        .arg(after_file.path())
        .output()
        .with_context(|| format!("running {gumtree:?} textdiff -g {generator}"))?;
    if !output.status.success() {
        bail!(
            "gumtree exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("parsing gumtree JSON output")?;

    let matches = json["matches"]
        .as_array()
        .context("gumtree JSON has no `matches` array")?;
    let actions = json["actions"]
        .as_array()
        .context("gumtree JSON has no `actions` array")?;
    let src_to_dest: HashMap<&str, &str> = matches
        .iter()
        .filter_map(|m| Some((m["src"].as_str()?, m["dest"].as_str()?)))
        .collect();

    let before_table = char_offset_table(&before.contents);
    let after_table = char_offset_table(&after.contents);
    let mut before_spans = Vec::new();
    let mut after_spans = Vec::new();

    for action in actions {
        let Some(action_type) = action["action"].as_str() else {
            continue;
        };
        let Some(tree) = action["tree"].as_str() else {
            continue;
        };
        let (start, end) = gumtree_node_offsets(tree)?;
        // Same src/dest side rules as `gumtree_line_labels`: insert-* is dest-side, delete-* is
        // src-side, and update/move name a src-side node whose dest counterpart has to be looked
        // up in `matches`.
        match action_type {
            "insert-tree" | "insert-node" => {
                after_spans.push(span_from_char_offsets(&after_table, start, end));
            }
            "delete-tree" | "delete-node" => {
                before_spans.push(span_from_char_offsets(&before_table, start, end));
            }
            _ => {
                before_spans.push(span_from_char_offsets(&before_table, start, end));
                if let Some(dest) = src_to_dest.get(tree) {
                    let (d_start, d_end) = gumtree_node_offsets(dest)?;
                    after_spans.push(span_from_char_offsets(&after_table, d_start, d_end));
                }
            }
        }
    }
    Ok((before_spans, after_spans))
}

/// difftastic's changed spans. Its `--display json` chunks carry, per side, a `line_number` plus a
/// `changes` array of `{start, end}` column offsets into that line - the token-level highlighting
/// it draws - so each `changes` entry becomes one single-line span. An entry with an empty
/// `changes` array is a line difftastic reports as part of a chunk without marking any token on it
/// (context within a changed region), and contributes no span.
fn difftastic_node_spans(before: &Code, after: &Code) -> Result<(Vec<TextRange>, Vec<TextRange>)> {
    let language = before.metadata.language.unwrap_or_default();
    let ext = difftastic_extension(language)
        .with_context(|| format!("no difftastic extension mapping for {language:?}"))?;
    let difft = difftastic_bin()?;
    let (before_file, after_file) = write_temp_pair(before, after, Some(&format!(".{ext}")))?;

    let output = Command::new(&difft)
        .args(["--display", "json"])
        .env("DFT_UNSTABLE", "yes")
        .arg(before_file.path())
        .arg(after_file.path())
        .output()
        .with_context(|| format!("running {difft:?} --display json"))?;
    if !output.status.success() {
        bail!(
            "difft exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("parsing difft JSON output")?;

    let mut before_spans = Vec::new();
    let mut after_spans = Vec::new();
    let Some(chunks) = json["chunks"].as_array() else {
        return Ok((before_spans, after_spans));
    };
    for chunk in chunks {
        let entries = chunk
            .as_array()
            .context("difft JSON chunk is not an array")?;
        for entry in entries {
            for (key, spans) in [("lhs", &mut before_spans), ("rhs", &mut after_spans)] {
                let Some(side) = entry.get(key) else { continue };
                let Some(line) = side["line_number"].as_u64() else {
                    continue;
                };
                let Some(changes) = side["changes"].as_array() else {
                    continue;
                };
                for change in changes {
                    let (Some(start), Some(end)) =
                        (change["start"].as_u64(), change["end"].as_u64())
                    else {
                        continue;
                    };
                    spans.push(span_on_row(line as usize, start as usize, end as usize));
                }
            }
        }
    }
    Ok((before_spans, after_spans))
}

/// diffsitter's changed spans, from the same `-r json` output `diffsitter_line_labels` parses -
/// but keeping each entry's `start_position`/`end_position` (row + column) instead of only its
/// `line_index`. diffsitter emits one entry per *character*, so the spans come out extremely
/// fine-grained; they're merged by `merge_spans` before scoring rather than tested one at a time.
fn diffsitter_node_spans(before: &Code, after: &Code) -> Result<(Vec<TextRange>, Vec<TextRange>)> {
    let language = before.metadata.language.unwrap_or_default();
    let file_type = diffsitter_file_type(language)
        .with_context(|| format!("no diffsitter file type mapping for {language:?}"))?;
    let diffsitter = diffsitter_bin()?;
    let (before_file, after_file) = write_temp_pair(before, after, None)?;

    let output = Command::new(&diffsitter)
        .args(["-n", "-r", "json", "-t", file_type])
        .arg(before_file.path())
        .arg(after_file.path())
        .output()
        .with_context(|| format!("running {diffsitter:?} -r json -t {file_type}"))?;
    if !output.status.success() {
        bail!(
            "diffsitter exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("parsing diffsitter JSON output")?;

    let mut before_spans = Vec::new();
    let mut after_spans = Vec::new();
    let hunks = json["hunks"]
        .as_array()
        .context("diffsitter JSON has no `hunks` array")?;
    for hunk in hunks {
        let hunk = hunk
            .as_object()
            .context("diffsitter JSON hunk is not an object")?;
        for (side, spans) in [("Old", &mut before_spans), ("New", &mut after_spans)] {
            let Some(lines) = hunk.get(side).and_then(|v| v.as_array()) else {
                continue;
            };
            for line in lines {
                let Some(entries) = line["entries"].as_array() else {
                    continue;
                };
                for entry in entries {
                    let (Some(start_row), Some(start_col), Some(end_row), Some(end_col)) = (
                        entry["start_position"]["row"].as_u64(),
                        entry["start_position"]["column"].as_u64(),
                        entry["end_position"]["row"].as_u64(),
                        entry["end_position"]["column"].as_u64(),
                    ) else {
                        continue;
                    };
                    spans.push(TextRange {
                        start_row: start_row as usize,
                        start_column: start_col as usize,
                        end_row: end_row as usize,
                        end_column: end_col as usize,
                    });
                }
            }
        }
    }
    Ok((merge_spans(before_spans), merge_spans(after_spans)))
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
    /// `ok`, `unsupported` (the tool has no parser/generator for this language - not a failure,
    /// and deliberately not scored as 0, which would read as a perfect result), `error` (the tool
    /// was supposed to handle this language and didn't), or `line_only` (Unix diff, which has no
    /// node-level output at all by construction).
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

    let disagreement = human_mapping::line_disagreement_count;
    let mut scores = Vec::with_capacity(ExternalTool::ALL.len() + 1);

    // codediff itself, scored through the identical projection so its columns sit on the same
    // scale as the external tools' (and, deliberately, not on the same scale as its own
    // `benchmark_optimal_solutions` node-mismatch number).
    {
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
            status: "ok",
        });
    }

    let language = before.metadata.language.unwrap_or_default();
    for &tool in ExternalTool::ALL {
        if !tool.supports(language) {
            scores.push(ToolScore {
                name: tool.name(),
                line_mismatches: None,
                node_mismatches: None,
                leaf_node_mismatches: None,
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
        let (node_mismatches, leaf_node_mismatches, status) = match node_result {
            // Unix diff: no sub-line output exists to project onto nodes at all.
            None => (None, None, "line_only"),
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
                            + disagreement(&truth_after_leaves, &leaf_filter(&ta, &after_extents)),
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
                (None, None, "error")
            }
        };
        scores.push(ToolScore {
            name: tool.name(),
            line_mismatches,
            node_mismatches,
            leaf_node_mismatches,
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
    ];
    let tool_names: Vec<&str> = rows
        .first()
        .map(|row| row.scores.iter().map(|s| s.name).collect())
        .unwrap_or_default();
    for name in &tool_names {
        header.push(format!("{name}_line_mismatches"));
        header.push(format!("{name}_node_mismatches"));
        header.push(format!("{name}_leaf_node_mismatches"));
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
        ];
        let cell = |value: Option<usize>| value.map(|v| v.to_string()).unwrap_or_default();
        for score in &row.scores {
            record.push(cell(score.line_mismatches));
            record.push(cell(score.node_mismatches));
            record.push(cell(score.leaf_node_mismatches));
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
        match score_accuracy(name, before, after, &provenance) {
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
        "\n{:<14} {:>7} {:>7} {:>13} {:>10} {:>12} {:>12}",
        "tool", "ok", "err", "unsupported", "line mm", "node mm", "leaf mm"
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
            "{name:<14} {:>7} {:>7} {:>13} {:>10} {:>12} {:>12}",
            count("ok") + count("line_only"),
            count("error"),
            count("unsupported"),
            sum(|s| s.line_mismatches),
            cell(sum(|s| s.node_mismatches)),
            cell(sum(|s| s.leaf_node_mismatches)),
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
    println!(
        "\nCommon subset - the {} of {} fixtures every tool scored ({total_lines} lines, \
         {total_nodes} nodes, {total_leaves} leaf nodes):",
        common.len(),
        rows.len()
    );
    println!(
        "{:<14} {:>10} {:>8} {:>12} {:>8} {:>12} {:>8}",
        "tool", "line mm", "rate", "node mm", "rate", "leaf mm", "rate"
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
        let (lines, nodes, leaves) = (
            sum(|s| s.line_mismatches),
            sum(|s| s.node_mismatches),
            sum(|s| s.leaf_node_mismatches),
        );
        let has_nodes = scores.iter().any(|s| s.node_mismatches.is_some());
        if has_nodes {
            println!(
                "{name:<14} {lines:>10} {:>7.2}% {nodes:>12} {:>7.2}% {leaves:>12} {:>7.2}%",
                rate(lines, total_lines),
                rate(nodes, total_nodes),
                rate(leaves, total_leaves),
            );
        } else {
            println!(
                "{name:<14} {lines:>10} {:>7.2}% {:>12} {:>8} {:>12} {:>8}",
                rate(lines, total_lines),
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
}
