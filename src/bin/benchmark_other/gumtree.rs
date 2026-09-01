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

// Split out of benchmark_other.rs (the `gumtree`-cluster functions) purely to shrink that
// file's visible size - no behavior change.

use anyhow::{Context, Result, bail};
use codediff::code::{Code, Language};
use codediff::diff::text_range::TextRange;
use std::collections::HashMap;
use std::io::Write;
use std::process::Command;

use super::{char_offset_table, external_tool_bin, span_from_char_offsets, write_temp_pair};

/// `(generator id, file extension)` for every corpus language the installed GumTree build has a
/// registered generator for at all - confirmed live via `gumtree list GENERATORS` *and* by
/// running each entry against a real fixture pair from this corpus (2026-08-20).
///
/// The installed build is **v4.0.0-beta8** (`/var/tmp/gumtree-installed/gumtree-4.0.0-beta8`),
/// re-verified entry by entry on 2026-08-20. This table has been wrong about the installed build
/// twice now, in both directions - it claimed beta8 while beta4 was installed, and mapped C++ to
/// a generator beta4 does not register - so re-verify the whole table by running it whenever the
/// build changes, rather than carrying any of these claims forward.
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
/// No entry for any language outside this match: confirmed no registered generator exists for it
/// in this build.
pub(crate) fn gumtree_generator(language: Language) -> Option<(&'static str, &'static str)> {
    match language {
        Language::Java => Some(("java-jdt", "java")), // Stable
        Language::CSS => Some(("css-phcss", "css")),  // Stable
        Language::Rust => Some(("rust-treesitter-ng", "rs")), // Testing
        Language::Kotlin => Some(("kotlin-treesitter-ng", "kt")), // Testing
        Language::C => Some(("c-treesitter-ng", "c")), // Testing
        Language::Go => Some(("go-treesitter-ng", "go")), // Testing
        Language::Python => Some(("python-treesitter-ng", "py")), // Testing
        Language::TypeScript => Some(("ts-treesitter-ng", "ts")), // Testing
        Language::JavaScript => Some(("js-treesitter-ng", "js")), // Testing
        Language::CSharp => Some(("cs-treesitter-ng", "cs")), // Testing
        // Added 2026-08-20, when this table listed only the ten entries above and GumTree was
        // therefore being scored on a non-random 48% of the corpus that excluded whole language
        // families. Each entry is verified by *running* it against a real fixture pair from this
        // corpus (a `textdiff -g <id> -f JSON` run producing a non-empty `matches` array), never
        // trusted from `gumtree list GENERATORS` alone - see the CPP note below for why that
        // distinction has already cost this table its accuracy once.
        Language::PHP => Some(("php-treesitter-ng", "php")),
        Language::Ruby => Some(("ruby-treesitter-ng", "rb")),
        Language::Swift => Some(("swift-treesitter-ng", "swift")),
        Language::R => Some(("r-treesitter-ng", "r")),
        Language::XML => Some(("xml-jsoup", "xml")),
        Language::YAML => Some(("yaml-snakeyaml", "yaml")),
        // Added 2026-08-20 in the same re-verification pass, when the installed build changed
        // from beta4 to beta8 (the beta4 tree under /var/tmp/tools/ no longer exists; beta8 is
        // what is installed, and is also what the paper's comparison section claims). beta8 ships
        // both of these and beta4 did not, so this is a build difference, not a corrected
        // oversight: 22 C++ and 19 TSX fixtures move from `unsupported` into GumTree's scored set.
        Language::CPP => Some(("cpp-treesitter-ng", "cpp")), // Testing
        Language::TSX => Some(("tsx-treesitter-ng", "tsx")), // Testing
        // `Language::JSON` deliberately absent, and this is the *reverse* direction of the same
        // build change: beta4 registered `json-jackson` and beta8 does not (verified by running
        // it - the client errors out on argument parsing, exactly as `cpp-treesitter-ng` did
        // under beta4). beta8's `gen.json` package registers only `xml-jsoup`. 18 JSON fixtures
        // therefore leave GumTree's scored set. Do not re-add this from a generator listing
        // alone; run it first.
        //
        // Still genuinely unsupported by beta8, and correctly absent: HTML, LUA, Vimscript,
        // ShellScript, Scala.
        _ => None,
    }
}

/// Path to the GumTree CLI script (`bin/gumtree` in its built distribution), from the `GUMTREE_BIN`
/// environment variable - not bundled or auto-installed, since it's a separate JVM project with
/// its own build (JDK 17 + Gradle; see the project's own install docs). Deliberately errors loudly
/// rather than silently skipping: unlike a language `ExternalTool::supports` excludes, a missing
/// binary for a language it claims to support is a real configuration problem.
pub(crate) fn gumtree_bin() -> Result<std::path::PathBuf> {
    external_tool_bin(
        "GUMTREE_BIN",
        "point it at GumTree's built bin/gumtree script",
    )
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
pub(crate) fn gumtree_line_labels(before: &Code, after: &Code) -> Result<(Vec<bool>, Vec<bool>)> {
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
pub(crate) fn gumtree_touched_from_json(
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
pub(crate) fn gumtree_warm_batch(
    fixtures: &[(&str, &Code, &Code)],
) -> Result<Option<HashMap<String, f64>>> {
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
    let mut failures = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let json: serde_json::Value = serde_json::from_str(line)
            .with_context(|| format!("parsing batch driver response line {line:?}"))?;
        let id = json["id"]
            .as_str()
            .context("batch driver response missing `id`")?
            .to_string();
        // One fixture GumTree cannot parse is a per-fixture gap, not a run-ending failure. This
        // used to `bail!`, which aborted the entire timing benchmark - and did, on 2026-08-20:
        // `css-fortawesome-font-awesome-upgrade-version-comment` makes beta8's `css-phcss`
        // generator throw a `SyntaxException`, and that single fixture killed a run covering all
        // 469. The per-invocation GumTree path already tolerates exactly this (it records the
        // fixture as an `error` and moves on), so the warm-JVM path treating it as fatal was an
        // inconsistency between two measurements of the same tool, not a deliberate policy.
        // Omitted ids simply have no `gumtree_warm_ms`, which every consumer already handles as
        // "not scored" rather than as a zero.
        if let Some(error) = json["error"].as_str() {
            failures.push(format!("  {id}: {error}"));
            continue;
        }
        let ms = json["ms"]
            .as_f64()
            .context("batch driver response missing `ms`")?;
        results.insert(id, ms);
    }
    if !failures.is_empty() {
        eprintln!(
            "gumtree_warm_ms: {} of {} fixtures failed in the batch driver and are unscored:\n{}",
            failures.len(),
            fixtures.len(),
            failures.join("\n")
        );
    }
    Ok(Some(results))
}

/// Parses the `[start,end]` character-offset suffix off a GumTree node-reference string like
/// `"SimpleName: foo [12,15]"` - the text before it (kind, optional `: text`) is irrelevant here,
/// only the position matters. Anchored to the end of the string since a node's own text can itself
/// contain brackets or commas (e.g. an array-literal leaf), which a naive first-bracket search
/// would misparse.
pub(crate) fn gumtree_node_offsets(node_ref: &str) -> Result<(usize, usize)> {
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
pub(crate) fn gumtree_line_range(
    contents: &str,
    start: usize,
    end: usize,
) -> std::ops::RangeInclusive<usize> {
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

/// GumTree's changed spans, from the same `textdiff -f JSON` output `gumtree_line_labels` parses
/// (see its doc comment for what `matches`/`actions` mean) - but keeping each action's real
/// character range instead of collapsing it to the lines it touches.
pub(crate) fn gumtree_node_spans(
    before: &Code,
    after: &Code,
) -> Result<(Vec<TextRange>, Vec<TextRange>)> {
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
