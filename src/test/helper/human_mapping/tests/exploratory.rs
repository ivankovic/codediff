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
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

// Split out of human_mapping.rs's inline `mod tests` block: this is the trailing cluster of
// diagnostic/exploratory tools (every fn here is #[test] #[ignore], printing or dumping analysis
// rather than asserting real pass/fail behavior) that had grown to dominate the file's visible
// size. Moved verbatim into this nested submodule to shrink human_mapping.rs - no behavior change.

use super::*;

/// EXPLORATORY, not a permanent check: for every painted fixture, how much do the two
/// human-authored ground truths (`entries`/tree mapping vs. `text_mappings`/painting)
/// disagree with *each other*, via `mapping_vs_painting_disagreements` - i.e. with codediff's
/// own (possibly wrong) node-matching algorithm taken out of the loop entirely, unlike
/// `painting_agreement`'s `compare_painting`, which renders `diff_code`'s real output.
///
/// Reports two figures per fixture: **structural** disagreement (what the two ground truths
/// actually disagree about) and **move-only** disagreement (bytes where the sole difference is
/// one side saying `Move`, the other "unchanged in place" - an artifact of `TextDiff::from`'s
/// column-shift heuristic, unavoidable in rendering but not a real disagreement between the
/// two humans - see `text_mapping_disagreements`'s doc comment). Structural is further split by
/// which side claims more: `tree_only` (the tree mapping says something changed, the painting
/// doesn't), `painting_only` (reverse), and `both_differ` (both say something changed, but
/// disagree on what).
#[test]
#[ignore]
fn exploratory_mapping_vs_painting_agreement_census() -> Result<()> {
    let names = [
        "cpp-add-const-correctness",
        "cpp-add-memory-management",
        "cpp-add-templates",
        "cpp-fix-segfault",
        "cpp-optimize-algorithm",
        "java-add-exception-handling",
        "java-add-interface",
        "java-add-logging",
        "java-fix-array-index",
        "java-refactor-constants",
        "javascript-add-array-method",
        "javascript-add-destructuring",
        "javascript-add-event-listener",
        "javascript-fix-promises",
        "javascript-refactor-arrow-func",
        "kotlin-add-data-class",
        "kotlin-add-null-check",
        "kotlin-add-validation",
        "kotlin-fix-loop-bug",
        "kotlin-refactor-function",
        "python-added-if-block",
        "python-added-if-block-small",
        "python-add-remove-block",
        "python-api-change",
        "python-bugfix-loop",
        "python-refactoring",
        "rust-add-comments-and-real-new-logic",
        "rust-add-if",
        "rust-add-to-existing-use",
        "rust-add-value-to-enum",
        "rust-algorithm-change",
        "rust-cost-optimization",
        "rust-data-structure",
        "rust-error-handling",
        "rust-firefox-webrenderer-borders",
        "rust-hash-optimization",
        "rust-hello-world-added-message",
        "rust-hello-world-removed-message",
        "rust-leetcode-1-bugfix",
        "rust-multi-map-duplicate-calls",
        "rust-no-change",
        "rust-small-addition-with-reuse-of-binary-expressions",
        "rust-sniffnet-protocol",
        "rust-tauri-api-build-1",
        "rust-tauri-api-build-2",
        "rust-tauri-cli-ios-dev",
        "rust-turbopack-persistence-tools-main",
        "typescript-add-error-handling",
        "typescript-add-generics",
        "typescript-add-type-annotations",
        "typescript-async-await",
    ];

    struct Row {
        name: String,
        solution: String,
        structural_bytes: usize,
        tree_only: usize,
        painting_only: usize,
        both_differ: usize,
        move_only_bytes: usize,
        total_bytes: usize,
    }

    let mut rows: Vec<Row> = Vec::new();
    for name in names {
        let (before, after) = &*crate::test::helper::handmade_test_code_pair(name)?;
        let mapping = load(name)?;
        let Some(check) = text_mapping_disagreements(&mapping, before, after)? else {
            eprintln!("{name}: no painting - skipped");
            continue;
        };

        let mut structural_bytes = 0;
        let mut tree_only = 0;
        let mut painting_only = 0;
        let mut both_differ = 0;
        let mut move_only_bytes = 0;
        for d in &check.disagreements {
            let width = d.end_byte - d.start_byte;
            if disagreement_is_move_only(d) {
                move_only_bytes += width;
            } else {
                structural_bytes += width;
                match (d.painted, d.from_tree) {
                    (None, Some(_)) => tree_only += width,
                    (Some(_), None) => painting_only += width,
                    _ => both_differ += width,
                }
            }
        }
        let total_bytes = before.contents.len() + after.contents.len();
        rows.push(Row {
            name: name.to_string(),
            solution: check.solution,
            structural_bytes,
            tree_only,
            painting_only,
            both_differ,
            move_only_bytes,
            total_bytes,
        });
    }

    rows.sort_by(|a, b| {
        let pct = |r: &Row| 100.0 * r.structural_bytes as f64 / r.total_bytes.max(1) as f64;
        pct(b).partial_cmp(&pct(a)).unwrap()
    });
    for r in &rows {
        let pct = 100.0 * r.structural_bytes as f64 / r.total_bytes.max(1) as f64;
        let move_pct = 100.0 * r.move_only_bytes as f64 / r.total_bytes.max(1) as f64;
        eprintln!(
            "{pct:>7.3}%  structural={:>6} (tree_only={:>5} painting_only={:>5} both_differ={:>5})  move_only={move_pct:>6.3}%  [{}]  {}",
            r.structural_bytes, r.tree_only, r.painting_only, r.both_differ, r.solution, r.name
        );
    }
    let zero = rows.iter().filter(|r| r.structural_bytes == 0).count();
    let mean: f64 = rows
        .iter()
        .map(|r| 100.0 * r.structural_bytes as f64 / r.total_bytes.max(1) as f64)
        .sum::<f64>()
        / rows.len() as f64;
    eprintln!(
        "\n{}/{} fixtures agree exactly on structure, mean structural disagreement {:.3}%",
        zero,
        rows.len(),
        mean
    );
    Ok(())
}

/// EXPLORATORY: prints every disagreement run for one named fixture, to read the *shape* of
/// what `exploratory_mapping_vs_painting_agreement_census` only counts.
#[test]
#[ignore]
fn exploratory_mapping_vs_painting_disagreement_detail() -> Result<()> {
    let name = "rust-add-if";
    let (before, after) = &*crate::test::helper::handmade_test_code_pair(name)?;
    let mapping = load(name)?;
    let check = text_mapping_disagreements(&mapping, before, after)?
        .with_context(|| format!("{name} has no painting"))?;
    eprintln!("best-matching painting: '{}'", check.solution);
    for d in &check.disagreements {
        let contents = if d.side == 0 {
            &before.contents
        } else {
            &after.contents
        };
        let text = &contents.as_bytes()[d.start_byte..d.end_byte];
        eprintln!(
            "side={} row={} bytes={}..{} painted={:?} tree={:?} move_only={} text={:?}",
            d.side,
            d.start_row,
            d.start_byte,
            d.end_byte,
            d.painted,
            d.from_tree,
            disagreement_is_move_only(d),
            String::from_utf8_lossy(text)
        );
    }
    Ok(())
}

/// EXPLORATORY: measures `compare_painting` (Minimal + Full) for every painted fixture in the
/// corpus and prints a report ranked by absolute mismatched bytes, plus three aggregate rates.
/// Run with `cargo test --lib --features test-fixtures painting_disagreement_report -- --ignored
/// --nocapture`.
///
/// Ranked by bytes rather than percent: the `/goal` this backs (fewer than 1% character
/// painting disagreement, in aggregate) is a bytes-summed rate, so a big fixture's
/// small percentage can outweigh a tiny fixture's big one - see this test's own printed rows
/// for a caller wanting the ranking, not the theory.
///
/// **Widened from `handmade` to the whole corpus on 2026-09-05**, when 84 `stratified` fixtures
/// became painted and measured at once. Until then this scanned `diffs/handmade` alone, which was
/// defensible while every painted fixture was handmade and became a silent under-report the moment
/// that stopped being true.
///
/// Three aggregates, because one number cannot carry what the corpus now holds:
///
///   * **whole corpus** - every painted fixture, the honest headline.
///   * **excluding parse errors** - the same, minus fixtures tree-sitter reports a parse error on
///     (`Node::has_error` on either side's root). A painting can only be as good as the tree under
///     it, so a fixture whose parse failed is not evidence about the renderer. Derived rather than
///     listed, so a fixture leaves this bucket the day its parse is fixed and nothing needs editing.
///
///     **Read this bucket for what it is, not for what it sounds like.** It flags 8 fixtures, and
///     they are mostly ordinary C headers whose macros tree-sitter-c stumbles on. It does *not*
///     catch the family that actually motivates the question - files named for one language that
///     hold another. Checked directly on 2026-09-05: of the four `.html` fixtures that are really
///     Go templates, three (`html-gohugoio-hugo-template-not-pure-html`, its `-2`, and
///     `html-prettier-prettier-not-pure-html-includes-yaml-as-well`) parse **clean**, because
///     tree-sitter-html is happy to read `{{ ... }}` as ordinary text. They carry four of the five
///     worst rates in the corpus and this bucket does not exclude them. Excluding parse errors in
///     fact *raises* the aggregate slightly, which is the tell: parse failure is not what drives
///     painting disagreement here.
///   * **handmade only** - the population every painting measurement before 2026-09-05 was made
///     against, kept so the historical series stays comparable rather than silently rebased.
#[test]
#[ignore]
fn painting_disagreement_report() -> Result<()> {
    use crate::diff::text::RenderOptions;

    let diffs_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("diffs");
    // Every dataset, not just `handmade` - see this function's doc comment. `DIFF_DATASETS` rather
    // than a `read_dir` of `diffs/` so this cannot drift from the split the rest of the suite
    // resolves names against.
    let mut names: Vec<(String, String)> = Vec::new();
    for dataset in crate::test::helper::DIFF_DATASETS {
        let dir = diffs_dir.join(dataset);
        if !dir.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&dir)?.filter_map(|entry| entry.ok()) {
            if entry.path().is_dir() {
                names.push((
                    (*dataset).to_string(),
                    entry.file_name().to_string_lossy().into_owned(),
                ));
            }
        }
    }
    names.sort();

    struct Row {
        name: String,
        dataset: String,
        minimal: PaintingComparison,
        full: PaintingComparison,
        parse_error: bool,
    }
    let mut rows = Vec::new();
    let mut errors = Vec::new();
    for (dataset, name) in &names {
        let minimal = compare_painting(name, RenderOptions::MINIMAL);
        let full = compare_painting(name, RenderOptions::FULL);
        match (minimal, full) {
            (Ok(minimal), Ok(full)) => {
                // Derived, not listed: a fixture counts as a parse error when tree-sitter reports
                // one on either side's root. A pair that fails to load at all is not a parse error
                // in this sense - it never reaches here, it lands in `errors` below.
                let parse_error = match crate::test::helper::handmade_test_code_pair(name) {
                    Ok(pair) => {
                        let (before, after) = &*pair;
                        [before, after].iter().any(|code| {
                            code.ast
                                .as_ref()
                                .is_some_and(|tree| tree.root_node().has_error())
                        })
                    }
                    Err(_) => false,
                };
                rows.push(Row {
                    name: name.clone(),
                    dataset: dataset.clone(),
                    minimal,
                    full,
                    parse_error,
                });
            }
            (minimal, full) => {
                // A fixture with no painting reports an error here; that is the overwhelmingly
                // common case now that this scans every dataset, so it is counted rather than
                // listed one line at a time.
                let msg = minimal.err().or(full.err()).unwrap();
                errors.push(format!("{name}: {msg:#}"));
            }
        }
    }

    rows.sort_by_key(|row| {
        std::cmp::Reverse(row.minimal.mismatched_bytes + row.full.mismatched_bytes)
    });

    eprintln!(
        "{:<70} {:>11} {:>10} {:>10} {:>10}",
        "fixture", "dataset", "minimal%", "full%", "sum_bytes"
    );
    for row in &rows {
        let sum_bytes = row.minimal.mismatched_bytes + row.full.mismatched_bytes;
        eprintln!(
            "{:<70} {:>11} {:>10.3} {:>10.3} {:>10}{}",
            row.name,
            row.dataset,
            row.minimal.percent(),
            row.full.percent(),
            sum_bytes,
            if row.parse_error {
                "  [parse error]"
            } else {
                ""
            },
        );
    }

    // One closure, three populations - so the three numbers cannot drift apart in how they are
    // computed, only in who they are computed over.
    let aggregate = |label: &str, keep: &dyn Fn(&Row) -> bool, note: &str| {
        let kept: Vec<&Row> = rows.iter().filter(|row| keep(row)).collect();
        let mismatched: usize = kept
            .iter()
            .map(|row| row.minimal.mismatched_bytes + row.full.mismatched_bytes)
            .sum();
        let total: usize = kept
            .iter()
            .map(|row| row.minimal.total_bytes + row.full.total_bytes)
            .sum();
        let percent = if total == 0 {
            0.0
        } else {
            100.0 * mismatched as f64 / total as f64
        };
        eprintln!(
            "  {label:<28} {:>3} fixtures  {mismatched:>7} / {total:<8} bytes = {percent:>7.4}%{note}",
            kept.len()
        );
    };

    // The goal marker rides on the whole-corpus line alone. The other two are context: one measures
    // something narrower than its name suggests (see the doc comment), the other exists only to keep
    // the pre-2026-09-05 series comparable. Neither is the bar.
    eprintln!("\naggregate:");
    aggregate("whole corpus", &|_| true, "  (goal: < 1%)");
    aggregate("excluding parse errors", &|row: &Row| !row.parse_error, "");
    aggregate(
        "handmade only (historical)",
        &|row: &Row| row.dataset == "handmade",
        "",
    );

    if !errors.is_empty() {
        eprintln!(
            "\n{} fixture(s) could not be measured (overwhelmingly: no painting)",
            errors.len()
        );
    }
    Ok(())
}

/// EXPLORATORY: `exploratory_mapping_vs_painting_disagreement_detail`, parameterized by a
/// `FIXTURE` env var - checks the *tree mapping* against the painting (structural agreement),
/// unlike `painting_disagreement_detail` below which checks codediff's *rendering* against it.
/// `FIXTURE=name cargo test --lib --features test-fixtures
/// mapping_vs_painting_disagreement_detail_for_fixture -- --ignored --nocapture`.
#[test]
#[ignore]
fn mapping_vs_painting_disagreement_detail_for_fixture() -> Result<()> {
    let name = std::env::var("FIXTURE").unwrap_or_else(|_| "rust-add-if".to_string());
    let (before, after) = &*crate::test::helper::handmade_test_code_pair(&name)?;
    let mapping = load(&name)?;
    let check = text_mapping_disagreements(&mapping, before, after)?
        .with_context(|| format!("{name} has no painting"))?;
    eprintln!("best-matching painting: '{}'", check.solution);
    for d in &check.disagreements {
        let contents = if d.side == 0 {
            &before.contents
        } else {
            &after.contents
        };
        let text = &contents.as_bytes()[d.start_byte..d.end_byte];
        eprintln!(
            "side={} row={} bytes={}..{} painted={:?} tree={:?} move_only={} text={:?}",
            d.side,
            d.start_row,
            d.start_byte,
            d.end_byte,
            d.painted,
            d.from_tree,
            disagreement_is_move_only(d),
            String::from_utf8_lossy(text)
        );
    }
    Ok(())
}

/// EXPLORATORY: prints just the Minimal/Full percentages for fixtures named by the
/// `FIXTURES` env var (comma-separated): `FIXTURES=a,b,c cargo test --lib --features test-fixtures
/// measure_stub_fixtures -- --ignored --nocapture`.
///
/// Narrower than `painting_disagreement_report`, which since 2026-09-05 covers every dataset and no
/// longer leaves a gap for this to fill. Kept because naming a handful of fixtures is still the
/// fastest way to re-measure after a change, without paying for the whole corpus.
#[test]
#[ignore]
fn measure_stub_fixtures() -> Result<()> {
    use crate::diff::text::RenderOptions;

    let names = std::env::var("FIXTURES").unwrap_or_default();
    for name in names.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let minimal = compare_painting(name, RenderOptions::MINIMAL)?;
        let full = compare_painting(name, RenderOptions::FULL)?;
        eprintln!(
            "{name}: minimal {:.3}% ({}/{}), full {:.3}% ({}/{})",
            minimal.percent(),
            minimal.mismatched_bytes,
            minimal.total_bytes,
            full.percent(),
            full.mismatched_bytes,
            full.total_bytes
        );
    }
    Ok(())
}

/// TEMPORARY/EXPLORATORY (not for commit): same detail as `painting_disagreement_detail`
/// below, but for every fixture named in the comma-separated `FIXTURES` env var, both modes,
/// in one process - avoids a `cargo test` recompile+relaunch per fixture/mode when surveying
/// many fixtures at once. `FIXTURES=a,b,c cargo test --lib --features test-fixtures
/// painting_disagreement_detail_batch -- --ignored --nocapture`.
#[test]
#[ignore]
fn painting_disagreement_detail_batch() -> Result<()> {
    use crate::diff::text::RenderOptions;

    let names = std::env::var("FIXTURES").unwrap_or_default();
    for name in names.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        for (mode, options) in [
            ("minimal", RenderOptions::MINIMAL),
            ("full", RenderOptions::FULL),
        ] {
            let pair = match crate::test::helper::handmade_test_code_pair(name) {
                Ok(pair) => pair,
                Err(e) => {
                    eprintln!("fixture={name} mode={mode}: ERROR loading code pair: {e:#}");
                    continue;
                }
            };
            let (before, after) = &*pair;
            let mapping = match load(name) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("fixture={name} mode={mode}: ERROR loading mapping: {e:#}");
                    continue;
                }
            };
            // First candidate: these are diagnostics over one painting at a time, not the
            // grader, so the alternative readings a preset may now carry are reported one by one
            // rather than reduced to a best.
            let painting = match paintings_for_mode(&mapping, options).map(|all| all[0]) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("fixture={name} mode={mode}: ERROR: {e:#}");
                    continue;
                }
            };

            let mut painted: [Vec<(HumanTextSpan, TextLabel)>; 2] = [Vec::new(), Vec::new()];
            for entry in &painting.mapping.entries {
                let label =
                    TextLabel::from_verdict(entry.verdict(&before.contents, &after.contents)?);
                for span in &entry.before {
                    painted[0].push((*span, label));
                }
                for span in &entry.after {
                    painted[1].push((*span, label));
                }
            }

            let diff = crate::diff::diff_code(before, after);
            let ast = diff
                .ast
                .as_ref()
                .with_context(|| format!("codediff produced no AST diff for '{name}'"))?;
            let node_cache = crate::diff::NodeCache::build(before, after);
            let text_diff = crate::diff::text::TextDiff::from_with_options(
                before,
                after,
                ast,
                &node_cache,
                options,
            );

            let comparison = compare_painting(name, options)?;
            eprintln!(
                "=== fixture={name} mode={mode} percent={:.3}% (painting solution='{}') ===",
                comparison.percent(),
                painting.name
            );
            for (side, contents) in [(0usize, &before.contents), (1usize, &after.contents)] {
                let ours_ranges =
                    crate::diff::text::ranges_for_options(&text_diff.all(side), contents, options);
                let ours = label_bytes_from_ranges(contents, &ours_ranges);
                let theirs = label_bytes(contents, &painted[side]);

                let mut i = 0usize;
                while i < ours.len() {
                    if ours[i] == theirs[i] {
                        i += 1;
                        continue;
                    }
                    let start = i;
                    while i < ours.len() && ours[i] != theirs[i] {
                        i += 1;
                    }
                    let row = contents[..start].matches('\n').count();
                    let text = &contents.as_bytes()[start..i];
                    eprintln!(
                        "  side={side} row={row} bytes={start}..{i} ours={:?} theirs={:?} text={:?}",
                        ours[start],
                        theirs[start],
                        String::from_utf8_lossy(text)
                    );
                }
            }
        }
    }
    Ok(())
}

/// EXPLORATORY: prints every run of bytes where codediff's rendering (under `options`)
/// disagrees with the human painting for one fixture - the `compare_painting` byte-projection
/// itself, not `text_mapping_disagreements`' separate node-vs-painting comparison. Fixture and
/// mode are read from env vars so this can be pointed at any `painting_disagreement_report`
/// offender without editing this function: `FIXTURE=<name> MODE=<minimal|full> cargo test --lib
/// --features test-fixtures painting_disagreement_detail -- --ignored --nocapture`.
#[test]
#[ignore]
fn painting_disagreement_detail() -> Result<()> {
    use crate::diff::text::RenderOptions;

    let name = std::env::var("FIXTURE").unwrap_or_else(|_| "rust-add-if".to_string());
    let mode = std::env::var("MODE").unwrap_or_else(|_| "minimal".to_string());
    let options = if mode.eq_ignore_ascii_case("full") {
        RenderOptions::FULL
    } else {
        RenderOptions::MINIMAL
    };

    let (before, after) = &*crate::test::helper::handmade_test_code_pair(&name)?;
    let mapping = load(&name)?;
    let painting = paintings_for_mode(&mapping, options)?[0];

    let mut painted: [Vec<(HumanTextSpan, TextLabel)>; 2] = [Vec::new(), Vec::new()];
    for entry in &painting.mapping.entries {
        let label = TextLabel::from_verdict(entry.verdict(&before.contents, &after.contents)?);
        for span in &entry.before {
            painted[0].push((*span, label));
        }
        for span in &entry.after {
            painted[1].push((*span, label));
        }
    }

    let diff = crate::diff::diff_code(before, after);
    let ast = diff
        .ast
        .as_ref()
        .with_context(|| format!("codediff produced no AST diff for '{name}'"))?;
    let node_cache = crate::diff::NodeCache::build(before, after);
    let text_diff =
        crate::diff::text::TextDiff::from_with_options(before, after, ast, &node_cache, options);

    eprintln!(
        "fixture={name} mode={mode} (painting solution='{}')",
        painting.name
    );
    for (side, contents) in [(0usize, &before.contents), (1usize, &after.contents)] {
        let ours_ranges =
            crate::diff::text::ranges_for_options(&text_diff.all(side), contents, options);
        let ours = label_bytes_from_ranges(contents, &ours_ranges);
        let theirs = label_bytes(contents, &painted[side]);

        let mut i = 0usize;
        while i < ours.len() {
            if ours[i] == theirs[i] {
                i += 1;
                continue;
            }
            let start = i;
            while i < ours.len() && ours[i] != theirs[i] {
                i += 1;
            }
            let row = contents[..start].matches('\n').count();
            let text = &contents.as_bytes()[start..i];
            eprintln!(
                "  side={side} row={row} bytes={start}..{i} ours={:?} theirs={:?} text={:?}",
                ours[start],
                theirs[start],
                String::from_utf8_lossy(text)
            );
        }
    }
    Ok(())
}

/// TEMPORARY/EXPLORATORY (not for commit): dumps the top-level after-tree children's AST
/// mapping for a fixture named by `FIXTURE`, to find why a whole subtree renders unpainted.
/// `FIXTURE=name cargo test --lib --features test-fixtures dump_top_level_mapping --
/// --ignored --nocapture`.
#[test]
#[ignore]
fn dump_top_level_mapping() -> Result<()> {
    let name = std::env::var("FIXTURE").unwrap_or_else(|_| "python-api-change".to_string());
    let (before, after) = &*crate::test::helper::handmade_test_code_pair(&name)?;
    let diff = crate::diff::diff_code(before, after);
    let ast = diff.ast.as_ref().unwrap();

    if std::env::var("DUMP_RAW_RANGES").is_ok() {
        let node_cache = crate::diff::NodeCache::build(before, after);
        let text_diff = crate::diff::text::TextDiff::from(before, after, ast, &node_cache);
        eprintln!("--- raw after_ranges (unfiltered) ---");
        for r in text_diff.all(1) {
            eprintln!(
                "{:?} source={:?} dest={:?}",
                r.operation, r.source, r.destination
            );
        }
        eprintln!("--- after filtering (FULL) ---");
        let filtered = crate::diff::text::ranges_for_options(
            &text_diff.all(1),
            &after.contents,
            crate::diff::text::RenderOptions::FULL,
        );
        for r in &filtered {
            if r.source.start_row >= 20 {
                eprintln!("{:?} source={:?}", r.operation, r.source);
            }
        }
        eprintln!(
            "(total filtered ranges: {}, raw: {})",
            filtered.len(),
            text_diff.all(1).len()
        );
        return Ok(());
    }

    if let Ok(row) = std::env::var("SUBTREE_AT_ROW") {
        let target_row: usize = row.parse().unwrap();
        let after_root = after.ast.as_ref().unwrap().root_node();
        fn find_and_dump(
            node: tree_sitter::Node,
            target_row: usize,
            depth: usize,
            ast: &crate::diff::ASTDiff,
            contents: &[u8],
        ) -> bool {
            if node.start_position().row == target_row && depth < 20 {
                dump_subtree(node, 0, ast, contents);
                return true;
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if find_and_dump(child, target_row, depth + 1, ast, contents) {
                    return true;
                }
            }
            false
        }
        fn dump_subtree(
            node: tree_sitter::Node,
            depth: usize,
            ast: &crate::diff::ASTDiff,
            contents: &[u8],
        ) {
            let text = node
                .utf8_text(contents)
                .unwrap_or("")
                .lines()
                .next()
                .unwrap_or("");
            let indent = "  ".repeat(depth);
            match ast.mapping_for_node(&node.id()) {
                Some((other, mapping)) => eprintln!(
                    "{indent}#{} [{}] {:?} op={:?} reason={:?} -> #{other}",
                    node.id(),
                    node.kind(),
                    text,
                    mapping.operation,
                    mapping.reason
                ),
                None => eprintln!(
                    "{indent}#{} [{}] {:?} -> UNMAPPED",
                    node.id(),
                    node.kind(),
                    text
                ),
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                dump_subtree(child, depth + 1, ast, contents);
            }
        }
        find_and_dump(after_root, target_row, 0, ast, after.contents.as_bytes());
        return Ok(());
    }

    let after_root = after.ast.as_ref().unwrap().root_node();
    let mut cursor = after_root.walk();
    for child in after_root.children(&mut cursor) {
        let id = child.id();
        let text = child
            .utf8_text(after.contents.as_bytes())
            .unwrap_or("")
            .lines()
            .next()
            .unwrap_or("");
        match ast.mapping_for_node(&id) {
            Some((other_id, mapping)) => {
                eprintln!(
                    "after#{id} [{}] {:?} -> before#{other_id} op={:?} reason={:?}",
                    child.kind(),
                    text,
                    mapping.operation,
                    mapping.reason
                );
            }
            None => {
                eprintln!(
                    "after#{id} [{}] {:?} -> NOT MAPPED AT ALL",
                    child.kind(),
                    text
                );
            }
        }
    }

    eprintln!("--- before top-level ---");
    let before_root = before.ast.as_ref().unwrap().root_node();
    let mut cursor = before_root.walk();
    for child in before_root.children(&mut cursor) {
        let id = child.id();
        let text = child
            .utf8_text(before.contents.as_bytes())
            .unwrap_or("")
            .lines()
            .next()
            .unwrap_or("");
        match ast.mapping_for_node(&id) {
            Some((other_id, mapping)) => {
                eprintln!(
                    "before#{id} [{}] {:?} -> after#{other_id} op={:?} reason={:?}",
                    child.kind(),
                    text,
                    mapping.operation,
                    mapping.reason
                );
            }
            None => {
                eprintln!(
                    "before#{id} [{}] {:?} -> NOT MAPPED AT ALL",
                    child.kind(),
                    text
                );
            }
        }
    }
    Ok(())
}
