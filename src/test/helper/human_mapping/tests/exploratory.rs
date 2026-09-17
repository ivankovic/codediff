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

/// EXPLORATORY: which *kinds* of disagreement exist between the two ground truths, over the whole
/// corpus, as a census of `(painted, from_tree)` label pairs.
///
/// The question it answers is which pairs never occur. A pair that appears nowhere across the
/// corpus is a candidate invariant - the two humans never once said those two things about the
/// same byte - while a pair that appears everywhere is the granularity difference the paper
/// reports rather than a contradiction. Run with `cargo test --lib --features test-fixtures
/// mapping_vs_painting_label_census -- --ignored --nocapture`.
///
/// `move_only` is counted apart for the reason `text_mapping_disagreements`' own doc gives: the
/// tree side's `Move` comes from `TextDiff::from`'s column-shift heuristic and neither ground
/// truth expresses `Move` positionally, so a pair involving it measures the renderer rather than
/// either human.
#[test]
#[ignore]
fn mapping_vs_painting_label_census() -> Result<()> {
    use std::collections::BTreeMap;
    use std::fs;

    let diffs_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("diffs");
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

    let mut bytes: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut fixtures: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut move_only_bytes = 0usize;
    let mut scored = 0usize;
    let mut skipped = 0usize;
    for (_, name) in &names {
        let Ok(mapping) = load(name) else {
            continue;
        };
        if mapping.text_mappings.is_empty() {
            continue;
        }
        let Ok(pair) = crate::test::helper::handmade_test_code_pair(name) else {
            skipped += 1;
            continue;
        };
        let (before, after) = &*pair;
        let Ok(Some(check)) = text_mapping_disagreements(&mapping, before, after) else {
            skipped += 1;
            continue;
        };
        scored += 1;
        let mut seen: std::collections::BTreeSet<(String, String)> = Default::default();
        for d in &check.disagreements {
            if disagreement_is_move_only(d) {
                move_only_bytes += d.end_byte - d.start_byte;
                continue;
            }
            let key = (format!("{:?}", d.painted), format!("{:?}", d.from_tree));
            *bytes.entry(key.clone()).or_default() += d.end_byte - d.start_byte;
            seen.insert(key);
        }
        for key in seen {
            *fixtures.entry(key).or_default() += 1;
        }
    }

    eprintln!("{scored} painted fixture(s) scored, {skipped} skipped (no tree or unreadable)");
    eprintln!("move-only bytes (renderer artifact, counted apart): {move_only_bytes}");
    eprintln!(
        "{:<12} {:<12} {:>10} {:>10}",
        "painted", "from_tree", "bytes", "fixtures"
    );
    let mut rows: Vec<_> = bytes.iter().collect();
    rows.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
    for (key, count) in rows {
        eprintln!(
            "{:<12} {:<12} {:>10} {:>10}",
            key.0,
            key.1,
            count,
            fixtures.get(key).copied().unwrap_or(0)
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

/// DIAGNOSTIC: every ground-truth invariant `invariants()` would report, for the fixtures named in
/// the comma-separated `FIXTURES` env var, or for the whole corpus when it is unset.
/// `FIXTURES=a,b cargo test --lib --features test-fixtures invariant_violations -- --ignored
/// --nocapture`.
///
/// The per-fixture test only ever says how many there are (its recorded count is exact, so a repair
/// makes it fail with the new number), which is the right thing for a gate and useless while
/// actually repairing one. This prints what they are. The corpus-wide form is the worklist.
#[test]
#[ignore]
fn invariant_violations() -> Result<()> {
    let wanted = std::env::var("FIXTURES").unwrap_or_default();
    let wanted: Vec<&str> = wanted
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    let mut total = 0;
    for (name, dir) in crate::test::helper::handmade_test_case_dirs()? {
        if !wanted.is_empty() && !wanted.contains(&name.as_str()) {
            continue;
        }
        let Some((before, after)) = crate::test::helper::code_pair_from_dir(&dir)? else {
            continue;
        };
        let Ok(mapping) = load(&name) else { continue };
        let violations =
            crate::test::helper::human_mapping::invariants::ground_truth_invariant_violations_for(
                &mapping, &before, &after,
            )?;
        if violations.is_empty() {
            continue;
        }
        total += violations.len();
        eprintln!("{name} ({})", violations.len());
        for violation in &violations {
            eprintln!("    {violation}");
            for site in &violation.sites {
                let contents = if site.side == 0 {
                    &before.contents
                } else {
                    &after.contents
                };
                let text = crate::test::helper::human_mapping::span_text(contents, site.span);
                eprintln!(
                    "        side={} rows {}..{} cols {}..{} {:?}",
                    site.side,
                    site.span.start_row + 1,
                    site.span.end_row + 1,
                    site.span.start_column,
                    site.span.end_column,
                    text.unwrap_or_default()
                );
            }
        }
    }
    eprintln!("{total} violation(s)");
    Ok(())
}

/// EXPLORATORY: how much of the corpus's ground truth satisfies the two `Full` whitespace-closure
/// rules in `invariants.rs` - "a line whose first visible character is inserted or deleted paints
/// its own indentation too" and "no unpainted whitespace between two painted regions on one line".
///
/// Run before wiring either into `ground_truth_invariant_violations`, so the repair cost is known
/// first: `cargo test --release --lib --features test-fixtures
/// measure_full_painting_whitespace_invariants -- --ignored --nocapture`.
#[test]
#[ignore]
fn measure_full_painting_whitespace_invariants() -> Result<()> {
    let diffs_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("diffs");
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

    let (mut painted, mut leading_bad, mut interior_bad, mut minimal_bad) =
        (0usize, 0usize, 0usize, 0usize);
    let (mut leading_total, mut interior_total, mut minimal_total) = (0usize, 0usize, 0usize);
    for (dataset, name) in &names {
        let dir = diffs_dir.join(dataset).join(name);
        let Some((before, after)) = crate::test::helper::code_pair_from_dir(&dir)? else {
            continue;
        };
        let Ok(mapping) = load(name) else { continue };
        if mapping.text_mappings.is_empty() {
            continue;
        }
        painted += 1;
        let (leading, interior, minimal) =
            crate::test::helper::human_mapping::invariants::full_painting_whitespace_violations(
                &mapping, &before, &after,
            )?;
        if !leading.is_empty() {
            leading_bad += 1;
            leading_total += leading.len();
        }
        if !interior.is_empty() {
            interior_bad += 1;
            interior_total += interior.len();
        }
        if !minimal.is_empty() {
            minimal_bad += 1;
            minimal_total += minimal.len();
        }
        if leading.is_empty() && interior.is_empty() && minimal.is_empty() {
            continue;
        }
        eprintln!(
            "{name} [leading {} / interior {} / minimal-indent {}]",
            leading.len(),
            interior.len(),
            minimal.len()
        );
        for violation in leading.iter().chain(interior.iter()).chain(minimal.iter()) {
            eprintln!("    {violation}");
        }
    }
    eprintln!(
        "\nPAINTED {painted}\nLEADING  {leading_bad} fixture(s), {leading_total} violation(s)\n\
         INTERIOR {interior_bad} fixture(s), {interior_total} violation(s)\n\
         MINIMAL-INDENT {minimal_bad} fixture(s), {minimal_total} violation(s)"
    );
    Ok(())
}

/// EXPLORATORY: does every painted insert/delete run sit at its **leftmost** equivalent position?
///
/// The question behind it: when a change adds or removes characters *inside* a value a reader
/// reads character by character - an identifier, a string, a comment - the run is often free to
/// slide. `overscroll-none` -> `overscroll-y-none` can be painted as inserting `-y` after
/// `overscroll` or `y-` after `overscroll-`, and both produce the same text. Five fixtures record
/// that ambiguity explicitly, as a pair of paintings named `(left)`/`(right)`. The rule under test
/// is that `left` is always the one to keep.
///
/// **Scoped to runs strictly inside one node**, which is the whole subtlety. A comma that follows
/// a newly inserted argument can also slide - but it is a node of its own, the AST maps it, and
/// `RULES_AND_PREFERENCES.md`'s delimiter rule already says it should follow the thing it
/// delimits, which is a *right* preference. Applying "prefer left" there would contradict a rule
/// this corpus is already annotated under. A run qualifies as intra-value only when the smallest
/// node containing it contains it *strictly* and no child boundary falls inside it: that admits
/// identifier and string leaves, and excludes both a whole-node delimiter and anything spanning
/// two nodes.
///
/// `FIXTURES=a,b` narrows it; otherwise every painted fixture in the corpus.
#[test]
#[ignore]
fn painting_left_anchor_census() -> Result<()> {
    use crate::test::helper::human_mapping::invariants::painted_labels;

    let only: Vec<String> = std::env::var("FIXTURES")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let mut fixtures = 0usize;
    let (mut runs_total, mut runs_intra) = (0usize, 0usize);
    let (mut slidable_intra, mut slidable_structural) = (0usize, 0usize);
    let (mut declared, mut counterexamples) = (0usize, 0usize);
    let mut offenders: Vec<String> = Vec::new();

    let diffs_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("diffs");
    let mut names: Vec<String> = Vec::new();
    for dataset in crate::test::helper::DIFF_DATASETS {
        let dir = diffs_dir.join(dataset);
        if !dir.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&dir)?.filter_map(|entry| entry.ok()) {
            if entry.path().is_dir() {
                names.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
    }
    names.sort();

    for name in names {
        if !only.is_empty() && !only.contains(&name) {
            continue;
        }
        let Ok(pair) = crate::test::helper::handmade_test_code_pair(&name) else {
            continue;
        };
        let (before, after) = &*pair;
        let Ok(mapping) = load(&name) else { continue };
        if mapping.text_mappings.is_empty() {
            continue;
        }
        fixtures += 1;
        for named in &mapping.text_mappings {
            let Ok(labels) = painted_labels(named, before, after) else {
                continue;
            };
            for (side, code) in [(0usize, before), (1usize, after)] {
                let contents = code.contents.as_bytes();
                // The run a slide would move: Delete is only paintable on the before side and
                // Insert only on the after side, so one label per side is the whole question.
                let wanted = if side == 0 {
                    TextLabel::Delete
                } else {
                    TextLabel::Insert
                };
                let mut i = 0usize;
                while i < labels[side].len() {
                    if labels[side][i] != Some(wanted) {
                        i += 1;
                        continue;
                    }
                    let start = i;
                    while i < labels[side].len() && labels[side][i] == Some(wanted) {
                        i += 1;
                    }
                    let end = i;
                    runs_total += 1;
                    // Sliding one byte left leaves the same text iff the byte before the run
                    // equals the run's own last byte. A slide whose result reads *identically*
                    // (a run inside a stretch of one repeated character - four spaces of
                    // indentation, say) is excluded: there is no left and right to prefer
                    // between two spellings a reader cannot tell apart.
                    //
                    // The shared byte the run pivots on must also be a CONNECTOR. Any repeated
                    // byte makes a slide *possible*, but only a few make the two spellings
                    // equally readable, and the rest are token boundaries a slide would cut
                    // through: pivoting on `"` turns the inserted JSON pair `,"k":"v"` into the
                    // fragment `","k":"v`, and pivoting on `<` moves a vimscript ` iskeyword<`
                    // onto `< iskeyword`. Neither is a reading a human would choose, so neither
                    // is an ambiguity the left-anchor rule should be asked to settle. Limited to
                    // space, `_` and `-` for now - see RULES_AND_PREFERENCES.md.
                    let pivot = contents.get(end - 1).copied();
                    let slidable = start > 0
                        && contents.get(start - 1) == pivot.as_ref()
                        && matches!(pivot, Some(b' ' | b'_' | b'-'))
                        && contents[start..end] != contents[start - 1..end - 1];
                    let intra = intra_value_run(code, start, end);
                    if intra {
                        runs_intra += 1;
                    }
                    if !slidable {
                        continue;
                    }
                    if intra {
                        slidable_intra += 1;
                        let text = String::from_utf8_lossy(&contents[start..end]).into_owned();
                        let shifted =
                            String::from_utf8_lossy(&contents[start - 1..end - 1]).into_owned();
                        // A painting that *names itself* the right-hand reading is the ambiguity
                        // being recorded on purpose, not a painter's slip - the two are worth
                        // counting apart, because dropping the first is a data edit and
                        // disagreeing with the second is a claim about what a human prefers.
                        let declared_right = named.name.to_lowercase().contains("right");
                        if declared_right {
                            declared += 1;
                        } else {
                            counterexamples += 1;
                        }
                        offenders.push(format!(
                            "  [{}] {name} painting '{}' {} bytes {start}..{end} {text:?} -> \
                             left would be {shifted:?}",
                            if declared_right {
                                "declared-right"
                            } else {
                                "COUNTEREXAMPLE"
                            },
                            named.name,
                            if side == 0 { "before" } else { "after" },
                        ));
                    } else {
                        slidable_structural += 1;
                    }
                }
            }
        }
    }

    for line in &offenders {
        eprintln!("{line}");
    }
    eprintln!(
        "\nPAINTED FIXTURES            {fixtures}\n\
         INSERT/DELETE RUNS          {runs_total} ({runs_intra} intra-value)\n\
         NOT LEFTMOST, intra-value   {slidable_intra}\n\
         ..of which declared 'right'  {declared}   (the recorded ambiguity - dropping these is the rule)\n\
         ..of which counterexamples   {counterexamples}   (a human anchored right on purpose)\n\
         NOT LEFTMOST, structural    {slidable_structural}   (delimiters etc., out of scope)"
    );
    Ok(())
}

/// Whether `[start, end)` sits strictly inside a single node with no child boundary inside it -
/// see [`painting_left_anchor_census`] for why that is the line between "a value we read
/// character by character" and "a token the AST already maps".
///
/// Strictly inside on **both** sides, so a run that already starts at its node's first byte is
/// reported structural rather than intra-value. That is deliberate - such a run has no left to
/// slide to without leaving the node - but it also means the census cannot see runs that are
/// already at the leftmost legal position, and so undercounts how often the rule is satisfied.
fn intra_value_run(code: &crate::code::Code, start: usize, end: usize) -> bool {
    let Some(tree) = code.ast.as_ref() else {
        return false;
    };
    let node = tree
        .root_node()
        .descendant_for_byte_range(start, end.saturating_sub(1));
    let Some(node) = node else { return false };
    if node.start_byte() >= start || node.end_byte() <= end {
        return false;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .all(|c| c.end_byte() <= start || c.start_byte() >= end)
}

/// EXPLORATORY: candidate invariants beyond the nine in `invariants.rs`, measured over the whole
/// corpus so their repair cost is known before any is wired in. Every candidate here reads the
/// tree mapping through `Caches` (no renderer, so no column-shift `Move` artifact) and the
/// painting through `painted_labels` and its raw spans.
///
/// A before leaf's *effective* status walks up to the nearest node the mapping speaks about: its
/// own entry, an `Identical` ancestor (whose partner leaf is the one at the same offset in the
/// partner subtree - well defined because the subtrees read identically), or a `*WithChildren`
/// ancestor. A leaf under an `Update`/`MatchButNotIdentical` ancestor with no entry of its own is
/// undecided and skipped.
///
/// `cargo test --release --lib --features test-fixtures candidate_invariant_census -- --ignored
/// --nocapture`
#[test]
#[ignore]
fn candidate_invariant_census() -> Result<()> {
    use std::collections::{BTreeMap, BTreeSet, HashMap};

    use crate::test::helper::human_mapping::invariants::{
        delimiter_pairs, error_ranges, painted_labels,
    };

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Eff {
        Identical(usize),
        Edited(usize),
        Removed,
        Undecided,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum Cover {
        Unpainted,
        Whole(TextLabelKey),
        Mixed,
    }
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum TextLabelKey {
        Move,
        Update,
        Delete,
        Insert,
    }
    fn key_of(label: TextLabel) -> TextLabelKey {
        match label {
            TextLabel::Move => TextLabelKey::Move,
            TextLabel::Update => TextLabelKey::Update,
            TextLabel::Delete => TextLabelKey::Delete,
            TextLabel::Insert => TextLabelKey::Insert,
        }
    }
    fn cover(labels: &[Option<TextLabel>], start: usize, end: usize) -> Cover {
        let slice = &labels[start..end];
        let first = slice[0];
        if slice.iter().all(|l| *l == first) {
            match first {
                None => Cover::Unpainted,
                Some(l) => Cover::Whole(key_of(l)),
            }
        } else {
            Cover::Mixed
        }
    }
    fn offset(contents: &str, row: usize, col: usize) -> Option<usize> {
        let mut off = 0usize;
        for (i, line) in contents.split('\n').enumerate() {
            if i == row {
                return (col <= line.len()).then_some(off + col);
            }
            off += line.len() + 1;
        }
        None
    }
    fn row_of(contents: &str, byte: usize) -> usize {
        contents[..byte].matches('\n').count() + 1
    }
    fn index<'t>(root: Node<'t>) -> (HashMap<usize, Node<'t>>, Vec<Node<'t>>) {
        let mut ids = HashMap::new();
        let mut leaves = Vec::new();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            ids.insert(node.id(), node);
            if node.child_count() == 0 {
                leaves.push(node);
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
        }
        leaves.sort_by_key(|n| n.start_byte());
        (ids, leaves)
    }
    fn effective<'t>(
        leaf: Node<'t>,
        matches: &HashMap<usize, usize>,
        ops: &HashMap<usize, HumanOperation>,
        removed: &HashMap<usize, bool>,
        other_ids: &HashMap<usize, Node<'t>>,
    ) -> Eff {
        let mut cur = leaf;
        loop {
            if let Some(&partner) = matches.get(&cur.id()) {
                let op = ops.get(&cur.id()).copied();
                if cur.id() == leaf.id() {
                    return if op == Some(HumanOperation::Identical) {
                        Eff::Identical(partner)
                    } else {
                        Eff::Edited(partner)
                    };
                }
                if op != Some(HumanOperation::Identical) {
                    return Eff::Undecided;
                }
                let Some(pnode) = other_ids.get(&partner) else {
                    return Eff::Undecided;
                };
                let delta = leaf.start_byte() - cur.start_byte();
                let (ps, pe) = (
                    pnode.start_byte() + delta,
                    pnode.start_byte() + delta + (leaf.end_byte() - leaf.start_byte()),
                );
                return match pnode.descendant_for_byte_range(ps, pe) {
                    Some(d)
                        if d.start_byte() == ps
                            && d.end_byte() == pe
                            && d.kind() == leaf.kind() =>
                    {
                        Eff::Identical(d.id())
                    }
                    _ => Eff::Undecided,
                };
            }
            if let Some(&with_children) = removed.get(&cur.id()) {
                return if cur.id() == leaf.id() || with_children {
                    Eff::Removed
                } else {
                    Eff::Undecided
                };
            }
            match cur.parent() {
                Some(p) => cur = p,
                None => return Eff::Undecided,
            }
        }
    }

    #[derive(Default)]
    struct Tally {
        sites: BTreeMap<String, usize>,
        fixtures: BTreeMap<String, BTreeSet<String>>,
        examples: BTreeMap<String, Vec<String>>,
    }
    impl Tally {
        fn hit(&mut self, key: &str, fixture: &str, example: String) {
            *self.sites.entry(key.to_string()).or_default() += 1;
            self.fixtures
                .entry(key.to_string())
                .or_default()
                .insert(fixture.to_string());
            let ex = self.examples.entry(key.to_string()).or_default();
            if ex.len() < 3 {
                ex.push(format!("{fixture}: {example}"));
            }
        }
    }
    let mut tally = Tally::default();

    let mut scored = 0usize;
    let mut leaves_seen = [0usize; 2];
    let mut decided = [0usize; 2];
    for (name, dir) in crate::test::helper::handmade_test_case_dirs()? {
        let Ok(mapping) = load(&name) else { continue };
        if mapping.text_mappings.is_empty() {
            continue;
        }
        let Some((before, after)) = crate::test::helper::code_pair_from_dir(&dir)? else {
            continue;
        };
        let (Some(bt), Some(at)) = (before.ast.as_ref(), after.ast.as_ref()) else {
            continue;
        };
        scored += 1;
        let (broot, aroot) = (bt.root_node(), at.root_node());
        let caches = rebuild_caches_for_mapping(&mapping, broot, aroot);
        let (bids, bleaves) = index(broot);
        let (aids, aleaves) = index(aroot);
        let contents = [before.contents.as_str(), after.contents.as_str()];

        // ---- M1: the mapping against itself -------------------------------------------------
        for (&b, &a) in &caches.before_match {
            let (Some(bn), Some(an)) = (bids.get(&b), aids.get(&a)) else {
                continue;
            };
            let op = caches.before_operation.get(&b).copied();
            let same_text = contents[0][bn.byte_range()] == contents[1][an.byte_range()];
            let tokens = |n: Node, c: &str| -> Vec<(String, String)> {
                let mut out = Vec::new();
                let mut stack = vec![n];
                while let Some(x) = stack.pop() {
                    if x.child_count() == 0 {
                        let t = &c[x.byte_range()];
                        if !t.trim().is_empty() {
                            out.push((x.kind().to_string(), t.to_string()));
                        }
                    }
                    let mut cur = x.walk();
                    for ch in x.children(&mut cur) {
                        stack.push(ch);
                    }
                }
                out
            };
            let same_tokens = tokens(*bn, contents[0]) == tokens(*an, contents[1]);
            // Every descendant of `bn` with its own entry stays inside `an`, and none is removed.
            let descendants_stay = {
                let mut ok = true;
                let mut stack = vec![*bn];
                while let Some(x) = stack.pop() {
                    if x.id() != bn.id() {
                        if caches.before_removed.contains_key(&x.id()) {
                            ok = false;
                        }
                        if let Some(&q) = caches.before_match.get(&x.id())
                            && let Some(qn) = aids.get(&q)
                            && !(an.start_byte() <= qn.start_byte()
                                && qn.end_byte() <= an.end_byte())
                        {
                            ok = false;
                        }
                    }
                    let mut cur = x.walk();
                    for ch in x.children(&mut cur) {
                        stack.push(ch);
                    }
                }
                ok
            };
            let ex = format!(
                "{:?} row {} `{}`",
                bn.kind(),
                bn.start_position().row + 1,
                contents[0][bn.byte_range()]
                    .chars()
                    .take(40)
                    .collect::<String>()
            );
            match op {
                Some(HumanOperation::Identical) if !same_text => {
                    if same_tokens {
                        tally.hit(
                            "M1a Identical entry whose texts differ only in whitespace",
                            &name,
                            ex,
                        )
                    } else {
                        tally.hit(
                            "M1a' Identical entry whose token sequences differ",
                            &name,
                            ex,
                        )
                    }
                }
                Some(HumanOperation::Update) if same_text => {
                    tally.hit("M1b Update entry whose texts are identical", &name, ex)
                }
                Some(HumanOperation::Update) if bn.child_count() > 0 || an.child_count() > 0 => {
                    tally.hit("M1c Update entry on a node with children", &name, ex)
                }
                Some(HumanOperation::MatchButNotIdentical)
                    if same_text && bn.kind() == an.kind() =>
                {
                    if descendants_stay {
                        tally.hit("M1d' MatchButNotIdentical, identical text+kind, descendants all inside", &name, ex)
                    } else {
                        tally.hit(
                            "M1d MatchButNotIdentical, identical text+kind, a descendant leaves",
                            &name,
                            ex,
                        )
                    }
                }
                _ => {}
            }
        }

        let mapping_has_edits = mapping
            .entries
            .iter()
            .any(|e| e.operation != HumanOperation::Identical)
            || mapping
                .groups
                .iter()
                .any(|g| g.before_paths.len() != g.after_paths.len());

        for named in &mapping.text_mappings {
            let labels = painted_labels(named, &before, &after)?;
            let pname = named.name.as_str();

            // ---- C4: an empty painting against a mapping with edits, and vice versa ---------
            if named.mapping.entries.is_empty() && mapping_has_edits {
                let ops: Vec<String> = mapping
                    .entries
                    .iter()
                    .filter(|e| e.operation != HumanOperation::Identical)
                    .map(|e| {
                        format!(
                            "{:?} {:?}",
                            e.operation,
                            e.before_path
                                .as_ref()
                                .or(e.after_path.as_ref())
                                .and_then(|p| p.last())
                        )
                    })
                    .take(4)
                    .collect();
                tally.hit(
                    "C4a empty painting but the mapping has edits",
                    &name,
                    format!("'{pname}' {ops:?}"),
                );
            }
            if !named.mapping.entries.is_empty()
                && !mapping_has_edits
                && !mapping.entries.is_empty()
            {
                tally.hit(
                    "C4b painted, but every mapping entry is Identical",
                    &name,
                    format!("'{pname}'"),
                );
            }

            // Entry byte ranges, for the "same entry" test.
            let mut ranges: [Vec<(usize, usize, usize)>; 2] = [Vec::new(), Vec::new()];
            for (e, entry) in named.mapping.entries.iter().enumerate() {
                for (side, spans) in [(0usize, &entry.before), (1usize, &entry.after)] {
                    for span in spans {
                        if let (Some(s), Some(t)) = (
                            offset(contents[side], span.start_row, span.start_column),
                            offset(contents[side], span.end_row, span.end_column),
                        ) {
                            ranges[side].push((e, s, t));
                        }
                    }
                }
            }
            let entries_containing = |side: usize, s: usize, t: usize| -> BTreeSet<usize> {
                ranges[side]
                    .iter()
                    .filter(|&&(_, a, b)| a <= s && t <= b)
                    .map(|&(e, _, _)| e)
                    .collect()
            };

            // ---- C1/C2: correspondence agreement, and C3: edits left unpainted --------------
            for (side, leaves, matches, ops, removed, other_ids, other_side) in [
                (
                    0usize,
                    &bleaves,
                    &caches.before_match,
                    &caches.before_operation,
                    &caches.before_removed,
                    &aids,
                    1usize,
                ),
                (
                    1usize,
                    &aleaves,
                    &caches.after_match,
                    &caches.after_operation,
                    &caches.after_removed,
                    &bids,
                    0usize,
                ),
            ] {
                for leaf in leaves {
                    let (s, t) = (leaf.start_byte(), leaf.end_byte());
                    if contents[side][s..t].trim().is_empty() {
                        continue;
                    }
                    leaves_seen[side] += 1;
                    let eff = effective(*leaf, matches, ops, removed, other_ids);
                    if eff != Eff::Undecided {
                        decided[side] += 1;
                    }
                    let here = cover(&labels[side], s, t);
                    let punct = if contents[side][s..t] == *leaf.kind() {
                        "punct"
                    } else {
                        "named"
                    };
                    let ex = format!(
                        "'{pname}' {} row {} {:?} `{}`",
                        if side == 0 { "before" } else { "after" },
                        row_of(contents[side], s),
                        leaf.kind(),
                        contents[side][s..t].chars().take(30).collect::<String>()
                    );
                    match eff {
                        Eff::Removed => {
                            if here == Cover::Unpainted {
                                tally.hit(
                                    &format!("C3a removed leaf unpainted ({punct})"),
                                    &name,
                                    ex.clone(),
                                );
                            } else if here == Cover::Whole(TextLabelKey::Move) {
                                tally.hit(
                                    "C3c removed leaf painted Move (= invariant 9)",
                                    &name,
                                    ex.clone(),
                                );
                            }
                        }
                        Eff::Edited(p) => {
                            let pn = other_ids[&p];
                            let there = cover(&labels[other_side], pn.start_byte(), pn.end_byte());
                            if side == 0
                                && contents[side][s..t] != contents[other_side][pn.byte_range()]
                                && here == Cover::Unpainted
                                && there == Cover::Unpainted
                            {
                                tally.hit(&format!("C3b edited leaf (text differs) unpainted on both sides ({punct})"), &name, ex.clone());
                            }
                        }
                        _ => {}
                    }
                    // Correspondence: only from the before side, so each pair is counted once.
                    if side != 0 {
                        continue;
                    }
                    let (partner, kind) = match eff {
                        Eff::Identical(p) => (p, "identical"),
                        Eff::Edited(p) => (p, "edited"),
                        _ => continue,
                    };
                    let pn = other_ids[&partner];
                    let (ps, pt) = (pn.start_byte(), pn.end_byte());
                    let there = cover(&labels[1], ps, pt);
                    if here == Cover::Unpainted && there == Cover::Unpainted {
                        continue;
                    }
                    let eb = entries_containing(0, s, t);
                    let ea = entries_containing(1, ps, pt);
                    if eb.intersection(&ea).next().is_some() {
                        continue;
                    }
                    let ex = format!(
                        "{ex} -> after row {} `{}`",
                        row_of(contents[1], ps),
                        contents[1][ps..pt].chars().take(30).collect::<String>()
                    );
                    let key = if kind == "identical" {
                        format!(
                            "C1 {kind} pair in different entries: {here:?} -> {there:?} ({punct})"
                        )
                    } else {
                        format!("C1 {kind} pair in different entries: {here:?} -> {there:?}")
                    };
                    tally.hit(&key, &name, ex);
                }
            }

            // ---- C5: delimiter pairs in the painting ----------------------------------------
            for (side, root) in [(0usize, broot), (1usize, aroot)] {
                let errors = error_ranges(root);
                for (open, close) in delimiter_pairs(root, contents[side]) {
                    let (from, to) = (open.end_byte(), close.start_byte());
                    if errors.iter().any(|&(a, b)| a < to && b > from) {
                        continue;
                    }
                    let o = cover(&labels[side], open.start_byte(), open.end_byte());
                    let c = cover(&labels[side], close.start_byte(), close.end_byte());
                    let ex = format!(
                        "'{pname}' {} row {} {:?}={o:?} row {} {:?}={c:?}",
                        if side == 0 { "before" } else { "after" },
                        open.start_position().row + 1,
                        open.kind(),
                        close.start_position().row + 1,
                        close.kind()
                    );
                    match (o, c) {
                        (Cover::Whole(x), Cover::Whole(y)) if x != y => {
                            tally.hit("C5a delimiter pair painted with two verdicts", &name, ex)
                        }
                        (Cover::Whole(_), Cover::Unpainted)
                        | (Cover::Unpainted, Cover::Whole(_)) => {
                            tally.hit("C5b delimiter painted, partner unpainted", &name, ex)
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    eprintln!("{scored} painted fixture(s) with an AST scored");
    eprintln!(
        "leaves: before {} ({} decided), after {} ({} decided)",
        leaves_seen[0], decided[0], leaves_seen[1], decided[1]
    );
    eprintln!("{:<70} {:>8} {:>9}", "candidate", "sites", "fixtures");
    for (key, sites) in &tally.sites {
        eprintln!("{:<70} {:>8} {:>9}", key, sites, tally.fixtures[key].len());
    }
    eprintln!();
    for (key, examples) in &tally.examples {
        let fx = &tally.fixtures[key];
        if fx.len() <= 14 {
            eprintln!(
                "{key}  [{}]",
                fx.iter().cloned().collect::<Vec<_>>().join(", ")
            );
        } else {
            eprintln!("{key}");
        }
        for ex in examples {
            eprintln!("    {ex}");
        }
    }
    Ok(())
}

/// EXPLORATORY: for every byte invariant 9 reports, is it inside a node the tree mapping actually
/// leaves unmatched, or inside one it matched?
///
/// Invariant 9's doc asserts the second case cannot happen - "`Delete` and `Insert` on the tree
/// side carry no such caveat: they are nodes the human left unmatched". That is the claim this
/// checks, because the tree side is read through `TextDiff::from` rather than through `Caches`,
/// and that renderer also emits `Delete`/`Insert` for the characters that *changed inside* a
/// matched-but-edited leaf. Run with `cargo test --release --lib --features test-fixtures
/// invariant_nine_provenance -- --ignored --nocapture`.
#[test]
#[ignore]
fn invariant_nine_provenance() -> Result<()> {
    use crate::test::helper::human_mapping::invariants::painted_labels;

    /// The smallest node of `root` covering `offset`.
    fn leaf_at(root: Node, offset: usize) -> Option<Node> {
        if root.child_count() == 0 {
            return (root.start_byte() <= offset && offset < root.end_byte()).then_some(root);
        }
        let mut cursor = root.walk();
        root.children(&mut cursor)
            .filter(|child| child.start_byte() <= offset && offset < child.end_byte())
            .find_map(|child| leaf_at(child, offset))
    }

    println!(
        "{:<52} {:<22} {:>6} {:>9} {:>9} {:>9}",
        "fixture", "painting/side", "bytes", "unmatched", "matched", "no node"
    );
    for (name, dir) in crate::test::helper::handmade_test_case_dirs()? {
        let Ok(mapping) = load(&name) else { continue };
        if mapping.text_mappings.is_empty() || mapping.entries.is_empty() {
            continue;
        }
        let Some((before, after)) = crate::test::helper::code_pair_from_dir(&dir)? else {
            continue;
        };
        let (Some(bt), Some(at)) = (before.ast.as_ref(), after.ast.as_ref()) else {
            continue;
        };
        let Ok(ast_diff) = as_ast_diff_for_mapping(&mapping, &before, &after) else {
            continue;
        };
        let node_cache = crate::diff::NodeCache::build(&before, &after);
        let text_diff = crate::diff::text::TextDiff::from(&before, &after, &ast_diff, &node_cache);
        let tree = [
            label_bytes_from_ranges(&before.contents, &text_diff.all(0)),
            label_bytes_from_ranges(&after.contents, &text_diff.all(1)),
        ];
        let caches = rebuild_caches_for_mapping(&mapping, bt.root_node(), at.root_node());

        for named in &mapping.text_mappings {
            let painted = painted_labels(named, &before, &after)?;
            for (side, root) in [(0usize, bt.root_node()), (1usize, at.root_node())] {
                let (mut unmatched, mut matched, mut absent) = (0usize, 0usize, 0usize);
                for (offset, (paint, from_tree)) in
                    painted[side].iter().zip(tree[side].iter()).enumerate()
                {
                    if *paint != Some(TextLabel::Move)
                        || !matches!(from_tree, Some(TextLabel::Delete | TextLabel::Insert))
                    {
                        continue;
                    }
                    match leaf_at(root, offset).map(|leaf| {
                        if side == 0 {
                            status_before(leaf, &caches)
                        } else {
                            status_after(leaf, &caches)
                        }
                    }) {
                        Some(NodeStatus::Marked { .. }) => unmatched += 1,
                        Some(_) => matched += 1,
                        None => absent += 1,
                    }
                }
                if unmatched + matched + absent == 0 {
                    continue;
                }
                println!(
                    "{:<52} {:<22} {:>6} {:>9} {:>9} {:>9}",
                    name,
                    format!(
                        "{}/{}",
                        named.name,
                        if side == 0 { "before" } else { "after" }
                    ),
                    unmatched + matched + absent,
                    unmatched,
                    matched,
                    absent,
                );
            }
        }
    }
    Ok(())
}

/// EXPLORATORY: the painting round's worklist - every run of bytes where codediff's rendering
/// disagrees with the hand-painted ground truth, classified by what it is, and **attributed** to
/// the node matcher or to the renderer.
///
/// Three label vectors per side per preset:
///
/// * `real` - `diff_code`'s own mapping, rendered. What a reader actually sees.
/// * `ideal` - the fixture's *human tree mapping* pushed through the same `TextDiff` (via
///   [`as_ast_diff_for_mapping`]). What the renderer would paint if node matching were perfect.
/// * `painted` - the human painting the preset is answerable to.
///
/// `ideal` vs `painted` is the residue **no matcher improvement can remove**: a rendering rule
/// disagreeing with a human about a mapping the two agree on. That is what a painting-only round
/// can fix. `real` vs `ideal` is matcher-attributable, and `real` vs `painted` is the number
/// `painting_disagreement_report` prints.
///
/// One caveat on the `ideal` column: `as_ast_diff_for_mapping` leaves `ASTMappingReason::default()`
/// on every entry, so the two reason-tagged escapes from `Move` (`known_pure_reindent` via
/// `NestedConditionCollapse`/`WrapGrowth`, `known_pure_relocation` via `HeritageClauseGrowth`)
/// cannot fire there. It over-attributes `Move` to the renderer, never under-attributes.
///
/// Counted by **runs and fixtures first, bytes last**: a stray `}` is one byte and the corpus's
/// largest fixture is twelve thousand, so the byte-weighted ranking
/// `painting_disagreement_report` sorts by hides exactly the small repeated mistake this round is
/// about.
///
/// `cargo test --release --lib --features test-fixtures painting_failure_census --
/// **Every mismatch in the corpus, classified - the matching-side counterpart of
/// [`painting_failure_census`].**
///
/// That census answers "who owns the painting disagreement" and its answer is the renderer. This
/// one asks the same question of the *mapping*: of the mismatches `assert_matches_human_mapping`
/// counts, what shapes are they, and which pass produced the mapping that disagrees. One row per
/// mismatch in `research/data/quality/mismatch_census.csv`, so the taxonomy is a file to group by
/// rather than a table someone read once.
///
/// The three fields that discriminate are `expected_op` (what the human said), `actual_op` (what
/// codediff produced) and `reason` (the [`crate::diff::ASTMappingReason`] of the mapping it
/// produced instead), all parsed back out of the mismatch message - the message is generated text,
/// not free-form, so the parse is against a format this file's own `compute_mismatches_*` writes.
/// `kind`/`parent_kind`/`named` come from the node id the mismatch already carries.
#[test]
#[ignore]
// The CSV half needs the `csv` crate, which is only linked under `test-fixtures` - the same gate
// `painting_failure_census` below carries, for the same reason.
#[cfg(feature = "test-fixtures")]
fn mismatch_census() -> Result<()> {
    use std::collections::BTreeMap;

    /// `Delete (with children)` / `Identical` / ... - the leading operation word of a message.
    fn expected_op_of(message: &str) -> String {
        let head = message.split(&[' ', '['][..]).next().unwrap_or("");
        if message.starts_with(&format!("{head} (with children)")) {
            format!("{head}WithChildren")
        } else {
            head.to_string()
        }
    }

    /// The `(op X, reason Y)` tail, when the message carries one. `Delete (with children)` style
    /// messages name neither, and report `-`.
    fn op_and_reason_of(message: &str) -> (String, String) {
        let Some(start) = message.rfind("(op ") else {
            return ("-".to_string(), "-".to_string());
        };
        let tail = &message[start + 4..];
        let tail = tail.strip_suffix(')').unwrap_or(tail);
        match tail.split_once(", reason ") {
            Some((op, reason)) => (op.trim().to_string(), reason.trim().to_string()),
            None => (tail.trim().to_string(), "-".to_string()),
        }
    }

    /// `APTED("qualified_name")` -> `APTED`, so the reason groups by pass rather than by pass and
    /// argument. The argument is kept as its own column.
    fn split_reason(reason: &str) -> (String, String) {
        match reason.split_once('(') {
            Some((pass, rest)) => (
                pass.to_string(),
                rest.trim_end_matches(')').trim_matches('"').to_string(),
            ),
            None => (reason.to_string(), String::new()),
        }
    }

    fn node_for_id(root: Node, id: usize) -> Option<Node> {
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if node.id() == id {
                return Some(node);
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
        }
        None
    }

    let diffs_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("diffs");
    let mut names: Vec<String> = Vec::new();
    for dataset in crate::test::helper::DIFF_DATASETS {
        let dir = diffs_dir.join(dataset);
        if !dir.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&dir)?.filter_map(|entry| entry.ok()) {
            if entry.path().is_dir() {
                names.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
    }
    names.sort();

    let mut rows: Vec<[String; 10]> = Vec::new();
    let mut by_shape: BTreeMap<(String, String, String), usize> = BTreeMap::new();
    let mut by_fixture: BTreeMap<String, usize> = BTreeMap::new();
    let (mut solved, mut skipped) = (0usize, 0usize);

    for name in &names {
        let Ok(pair) = crate::test::helper::handmade_test_code_pair(name) else {
            skipped += 1;
            continue;
        };
        let (before, after) = &*pair;
        if load(name).is_err() {
            skipped += 1;
            continue;
        }
        let config = crate::diff::HeuristicConfig::default();
        let Ok(found) = compute_visible_mismatches_for_with_config(name, before, after, &config)
        else {
            skipped += 1;
            continue;
        };
        // A `*WithChildren` message names no pass - `check_subtree_maps_to_zero` reports the node
        // it found a mapping for, not the mapping. That is 22% of the corpus's mismatches, so the
        // diff is recomputed here and the node's own entry read out of it directly, the way
        // `actual_mapping_info` does for the messages that do carry one.
        let diff = crate::diff::diff_code_with_config(before, after, &config);
        let diff_ast = diff.ast.as_ref();
        solved += 1;
        let roots = [
            before.ast.as_ref().map(|tree| tree.root_node()),
            after.ast.as_ref().map(|tree| tree.root_node()),
        ];

        for (visible, mismatch) in found
            .visible
            .iter()
            .map(|mismatch| (true, mismatch))
            .chain(found.invisible.iter().map(|mismatch| (false, mismatch)))
        {
            let side = match mismatch.side {
                Side::Before => 0usize,
                Side::After => 1usize,
            };
            let node = roots[side].and_then(|root| node_for_id(root, mismatch.node_id));
            let (kind, parent_kind, named) = match node {
                Some(node) => {
                    let contents = if side == 0 {
                        &before.contents
                    } else {
                        &after.contents
                    };
                    (
                        node.kind().to_string(),
                        node.parent()
                            .map(|parent| parent.kind().to_string())
                            .unwrap_or_default(),
                        (node.child_count() == 0
                            && contents.get(node.byte_range()) != Some(node.kind()))
                        .to_string(),
                    )
                }
                None => ("-".to_string(), String::new(), "-".to_string()),
            };
            let expected = expected_op_of(&mismatch.message);
            let (mut actual, mut reason) = op_and_reason_of(&mismatch.message);
            if reason == "-"
                && let Some(diff_ast) = diff_ast
            {
                let key = if side == 0 {
                    diff_ast
                        .before_node_map
                        .get(&mismatch.node_id)
                        .map(|partner| (mismatch.node_id, *partner))
                } else {
                    diff_ast
                        .after_node_map
                        .get(&mismatch.node_id)
                        .map(|partner| (*partner, mismatch.node_id))
                };
                if let Some(entry) = key.and_then(|key| diff_ast.mapping.get(&key)) {
                    actual = format!("{:?}", entry.operation);
                    reason = format!("{:?}", entry.reason);
                }
            }
            let (pass, argument) = split_reason(&reason);
            *by_shape
                .entry((expected.clone(), actual.clone(), pass.clone()))
                .or_default() += 1;
            *by_fixture.entry(name.clone()).or_default() += 1;
            rows.push([
                name.clone(),
                if side == 0 { "before" } else { "after" }.to_string(),
                visible.to_string(),
                expected,
                actual,
                pass,
                argument,
                kind,
                parent_kind,
                named,
            ]);
        }
    }

    let csv_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("research")
        .join("data")
        .join("quality")
        .join("mismatch_census.csv");
    let mut writer = csv::Writer::from_path(&csv_path)?;
    writer.write_record([
        "fixture",
        "side",
        "visible",
        "expected_op",
        "actual_op",
        "reason",
        "reason_argument",
        "kind",
        "parent_kind",
        "named_leaf",
    ])?;
    for row in &rows {
        writer.write_record(row)?;
    }
    writer.flush()?;

    println!(
        "{} mismatches over {solved} solved fixtures ({skipped} skipped) -> {}",
        rows.len(),
        csv_path.display()
    );
    println!(
        "\n{:<28} {:<24} {:<18} count",
        "expected", "actual", "reason"
    );
    let mut shapes: Vec<_> = by_shape.into_iter().collect();
    shapes.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    for ((expected, actual, reason), count) in shapes.iter().take(30) {
        println!("{expected:<28} {actual:<24} {reason:<18} {count}");
    }
    println!("\nTop fixtures");
    let mut fixtures: Vec<_> = by_fixture.into_iter().collect();
    fixtures.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    for (fixture, count) in fixtures.iter().take(25) {
        println!("  {count:>5}  {fixture}");
    }
    Ok(())
}

/// --ignored --nocapture`
#[test]
#[ignore]
// The CSV half needs the `csv` crate, which is only linked under `test-fixtures` (see
// `helper::sample_provenance`'s own note) - and this measurement has nothing to read without the
// fixtures anyway. Same gate `the_quality_baseline_accuracy_columns_are_a_projection_of_the_stub_limits`
// carries for the same reason.
#[cfg(feature = "test-fixtures")]
fn painting_failure_census() -> Result<()> {
    use crate::diff::text::{RangeMatch, RenderOptions};
    use std::collections::{BTreeMap, HashSet};

    /// One classified disagreement run.
    struct Run {
        fixture: String,
        preset: &'static str,
        ours: Option<TextLabel>,
        theirs: Option<TextLabel>,
        bytes: usize,
        /// `whitespace` / `punctuation` / `code` - see `text_class`.
        class: &'static str,
        /// Kind of the smallest node containing the run.
        kind: String,
        /// Whether the run covers that node exactly, rather than part of it or a span crossing it.
        exact: bool,
        /// What the *human* tree mapping says about the nearest mapped ancestor of that node.
        human_op: String,
        /// What the mapping that produced `ours` says about it, and why.
        ours_op: String,
        ours_reason: String,
        /// For a `Move` we painted: the geometry `identical_or_move` decided it on - whether the
        /// span's start column and start row changed, whether it spans a row boundary, and
        /// whether the two sides' text is byte-identical. Empty when the run is not a `Move` of
        /// ours, or when no single range covers its first byte.
        geometry: String,
        sample: String,
    }

    fn label_name(label: Option<TextLabel>) -> &'static str {
        match label {
            None => "-",
            Some(label) => label.name(),
        }
    }

    /// `whitespace` when there is no visible character at all, `punctuation` when every visible
    /// character is one [`crate::diff::text::is_structural_only`] would drop, `code` otherwise.
    /// The split matters because the first two are what `MINIMAL`'s filters are *for*.
    fn text_class(text: &str) -> &'static str {
        if text.chars().all(char::is_whitespace) {
            "whitespace"
        } else if crate::diff::text::is_structural_only(text) {
            "punctuation"
        } else {
            "code"
        }
    }

    /// Byte offsets of a [`TextRange`] in the file it addresses, `None` for a position the file
    /// does not have (a row past the end, a column inside a multi-byte character).
    fn range_bytes(
        contents: &str,
        range: &crate::diff::text_range::TextRange,
    ) -> Option<(usize, usize)> {
        Some((
            byte_offset(contents, range.start_row, range.start_column)?,
            byte_offset(contents, range.end_row, range.end_column)?,
        ))
    }

    /// The shape `identical_or_move` read to call the range covering `byte` a `Move`: did its
    /// start column change (the `column_shift_is_meaningful` branch), did its start row, does it
    /// span a row boundary (the precondition of both `crossed_backwards` and
    /// `shifted_within_its_own_line`), and is the text on the two sides byte-identical.
    ///
    /// `col-same` with no row boundary cannot have come from a column shift at all, so a `Move`
    /// there is `crossed_backwards`' doing - the two are the only ways out of `Identical` and
    /// they need completely different fixes.
    fn move_geometry(
        ranges: &[RangeMatch],
        contents: [&String; 2],
        side: usize,
        byte: usize,
    ) -> String {
        let covering = ranges.iter().find(|range| {
            range.operation == crate::diff::text::TextOperation::Move
                && range_bytes(contents[side], &range.source)
                    .is_some_and(|(start, end)| start <= byte && byte < end)
        });
        let Some(range) = covering else {
            return "no-single-range".to_string();
        };
        let (source, destination) = (&range.source, &range.destination);
        // Three-valued on purpose. A range whose destination this function cannot address - a
        // column inside a multi-byte character, a row past the end - is *unknown*, not different,
        // and folding the two together would invent a family out of the UTF-8-heavy fixtures.
        let same_text = match (
            range_bytes(contents[side], source),
            range_bytes(contents[1 - side], destination),
        ) {
            (Some((s, e)), Some((ds, de))) => {
                match (contents[side].get(s..e), contents[1 - side].get(ds..de)) {
                    (Some(ours), Some(theirs)) if ours == theirs => "same-text",
                    (Some(_), Some(_)) => "text-differs",
                    _ => "text-unknown",
                }
            }
            _ => "text-unknown",
        };
        format!(
            "{}/{}/{}/{}",
            if source.start_column == destination.start_column {
                "col-same"
            } else {
                "col-shift"
            },
            if source.start_row == destination.start_row {
                "row-same"
            } else {
                "row-shift"
            },
            if source.end_row > source.start_row {
                "multi-row"
            } else {
                "single-row"
            },
            same_text,
        )
    }

    /// The nearest ancestor of `node` (itself included) that the mapping says anything about.
    fn nearest_mapping<'tree>(
        node: Node<'tree>,
        diff: &ASTDiff,
    ) -> Option<(Node<'tree>, crate::diff::ASTMapping)> {
        let mut current = Some(node);
        while let Some(node) = current {
            if let Some((_, mapping)) = diff.mapping_for_node(&node.id()) {
                return Some((node, mapping));
            }
            current = node.parent();
        }
        None
    }

    let diffs_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("diffs");
    let mut names: Vec<String> = Vec::new();
    for dataset in crate::test::helper::DIFF_DATASETS {
        let dir = diffs_dir.join(dataset);
        if !dir.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&dir)?.filter_map(|entry| entry.ok()) {
            if entry.path().is_dir() {
                names.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
    }
    names.sort();

    let mut runs: Vec<(&'static str, Run)> = Vec::new();
    // One row per (fixture, preset), written out at the end: this is the artifact that makes the
    // renderer's own residue a number the project can watch rather than a table someone read once.
    let mut rows: Vec<[String; 8]> = Vec::new();
    // preset -> (real, ideal, matcher) mismatched bytes, and the corpus size they are out of.
    let mut totals: BTreeMap<&'static str, [usize; 4]> = BTreeMap::new();
    let mut measured: HashSet<String> = HashSet::new();
    let mut violating: HashSet<String> = HashSet::new();
    let mut single_painting: HashSet<String> = HashSet::new();
    let (mut unpainted, mut no_tree, mut errors) = (0usize, 0usize, Vec::new());

    for name in &names {
        let Ok(pair) = crate::test::helper::handmade_test_code_pair(name) else {
            continue;
        };
        let (before, after) = &*pair;
        let Ok(mapping) = load(name) else {
            continue;
        };
        if mapping.text_mappings.is_empty() {
            unpainted += 1;
            continue;
        }
        if before.ast.is_none() || after.ast.is_none() {
            // The plain-text fallback has no tree to classify a run against, and no human tree
            // mapping to build an `ideal` column from. Counted, not measured.
            no_tree += 1;
            continue;
        }
        let real_diff = crate::diff::diff_code(before, after);
        let Some(real_ast) = real_diff.ast.as_ref() else {
            continue;
        };
        let human_ast = match as_ast_diff_for_mapping(&mapping, before, after) {
            Ok(diff) => diff,
            Err(e) => {
                errors.push(format!("{name}: human mapping -> ASTDiff: {e:#}"));
                continue;
            }
        };
        let node_cache = crate::diff::NodeCache::build(before, after);
        let roots = [
            before.ast.as_ref().unwrap().root_node(),
            after.ast.as_ref().unwrap().root_node(),
        ];
        let contents = [&before.contents, &after.contents];

        if mapping.text_mappings.len() == 1 {
            single_painting.insert(name.clone());
        }
        if crate::test::helper::human_mapping::invariants::ground_truth_invariant_violations_for(
            &mapping, before, after,
        )
        .is_ok_and(|violations| !violations.is_empty())
        {
            violating.insert(name.clone());
        }

        for (preset, options) in [
            ("minimal", RenderOptions::MINIMAL),
            ("full", RenderOptions::FULL),
        ] {
            let Ok(candidates) = paintings_for_mode(&mapping, options) else {
                continue;
            };
            // The filtered ranges come back alongside the labels: a label says *what* we
            // painted, and the range it came from says *why* - which is the half a rule has to
            // key on.
            let sides = |ast: &ASTDiff| -> ([Vec<Option<TextLabel>>; 2], [Vec<RangeMatch>; 2]) {
                let text_diff = crate::diff::text::TextDiff::from_with_options(
                    before,
                    after,
                    ast,
                    &node_cache,
                    options,
                );
                let ranges = [0usize, 1usize].map(|side| {
                    crate::diff::text::ranges_for_options(
                        &text_diff.all(side),
                        contents[side],
                        options,
                    )
                });
                let labels = [0usize, 1usize]
                    .map(|side| label_bytes_from_ranges(contents[side], &ranges[side]));
                (labels, ranges)
            };
            let (real_labels, real_ranges) = sides(real_ast);
            let (ideal_labels, ideal_ranges) = sides(&human_ast);
            let ours = [real_labels, ideal_labels];
            let ours_ranges = [real_ranges, ideal_ranges];

            // One painting per preset for all three comparisons, so the three numbers decompose.
            // Chosen the way `compare_painting`'s grader chooses: the candidate closest to what a
            // reader actually sees.
            let mut best: Option<([Vec<Option<TextLabel>>; 2], usize)> = None;
            for painting in candidates {
                let mut spans: [Vec<(HumanTextSpan, TextLabel)>; 2] = [Vec::new(), Vec::new()];
                for entry in &painting.mapping.entries {
                    let label =
                        TextLabel::from_verdict(entry.verdict(&before.contents, &after.contents)?);
                    for span in &entry.before {
                        spans[0].push((*span, label));
                    }
                    for span in &entry.after {
                        spans[1].push((*span, label));
                    }
                }
                let theirs = [0usize, 1usize].map(|side| label_bytes(contents[side], &spans[side]));
                let distance: usize = (0..2)
                    .map(|side| {
                        ours[0][side]
                            .iter()
                            .zip(&theirs[side])
                            .filter(|(ours, theirs)| ours != theirs)
                            .count()
                    })
                    .sum();
                if best.as_ref().is_none_or(|(_, best)| distance < *best) {
                    best = Some((theirs, distance));
                }
            }
            let Some((theirs, _)) = best else { continue };

            measured.insert(name.clone());
            let per_fixture = {
                let mut counts = [0usize; 3];
                for (index, side) in (0..2).flat_map(|side| [(0usize, side), (1usize, side)]) {
                    counts[index] += ours[index][side]
                        .iter()
                        .zip(&theirs[side])
                        .filter(|(ours, theirs)| ours != theirs)
                        .count();
                }
                for (real, ideal) in ours[0].iter().zip(&ours[1]) {
                    counts[2] += real
                        .iter()
                        .zip(ideal)
                        .filter(|(real, ideal)| real != ideal)
                        .count();
                }
                counts
            };
            rows.push([
                name.clone(),
                preset.to_string(),
                (before.contents.len() + after.contents.len()).to_string(),
                per_fixture[0].to_string(),
                per_fixture[1].to_string(),
                per_fixture[2].to_string(),
                mapping.text_mappings.len().to_string(),
                usize::from(violating.contains(name)).to_string(),
            ]);
            let entry = totals.entry(preset).or_insert([0; 4]);
            entry[3] += before.contents.len() + after.contents.len();
            for (real, ideal) in ours[0].iter().zip(&ours[1]) {
                entry[2] += real
                    .iter()
                    .zip(ideal)
                    .filter(|(real, ideal)| real != ideal)
                    .count();
            }

            for (stream, index, ast) in [("real", 0usize, real_ast), ("ideal", 1usize, &human_ast)]
            {
                for side in 0..2 {
                    let (ours, theirs) = (&ours[index][side], &theirs[side]);
                    totals.get_mut(preset).unwrap()[index] += ours
                        .iter()
                        .zip(theirs)
                        .filter(|(ours, theirs)| ours != theirs)
                        .count();
                    let mut offset = 0usize;
                    while offset < ours.len() {
                        if ours[offset] == theirs[offset] {
                            offset += 1;
                            continue;
                        }
                        let start = offset;
                        // One run is one *verdict pair*, not merely one stretch of disagreement:
                        // a `Delete`-where-`Move`-was-wanted next to an unpainted-where-`Insert`
                        // are two different mistakes and must not be counted as one.
                        while offset < ours.len()
                            && ours[offset] != theirs[offset]
                            && ours[offset] == ours[start]
                            && theirs[offset] == theirs[start]
                        {
                            offset += 1;
                        }
                        let text = &contents[side][start..offset];
                        let geometry = if ours[start] == Some(TextLabel::Move) {
                            move_geometry(&ours_ranges[index][side], contents, side, start)
                        } else {
                            String::new()
                        };
                        let node = roots[side]
                            .descendant_for_byte_range(start, offset.max(start + 1))
                            .unwrap_or(roots[side]);
                        let (human_op, _) = nearest_mapping(node, &human_ast)
                            .map(|(node, mapping)| {
                                (format!("{:?}", mapping.operation), node.kind().to_string())
                            })
                            .unwrap_or_else(|| ("unmapped".to_string(), String::new()));
                        let (ours_op, ours_reason) = nearest_mapping(node, ast)
                            .map(|(_, mapping)| {
                                (
                                    format!("{:?}", mapping.operation),
                                    format!("{:?}", mapping.reason),
                                )
                            })
                            .unwrap_or_else(|| ("unmapped".to_string(), String::new()));
                        runs.push((
                            stream,
                            Run {
                                fixture: name.clone(),
                                preset,
                                ours: ours[start],
                                theirs: theirs[start],
                                bytes: offset - start,
                                class: text_class(text),
                                kind: node.kind().to_string(),
                                exact: node.start_byte() == start && node.end_byte() == offset,
                                human_op,
                                ours_op,
                                ours_reason,
                                geometry,
                                sample: format!(
                                    "{name} {preset} side={side} row={} {:?}",
                                    contents[side][..start].matches('\n').count() + 1,
                                    if text.len() > 40 { &text[..40] } else { text }
                                ),
                            },
                        ));
                    }
                }
            }
        }
    }

    // ---- the artifact ----------------------------------------------------------------------
    let csv_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("research")
        .join("data")
        .join("quality")
        .join("painting_attribution.csv");
    rows.sort();
    let header = [
        "fixture",
        "preset",
        "total_bytes",
        "real_bytes",
        "renderer_bytes",
        "matcher_bytes",
        "paintings",
        "breaks_invariants",
    ];
    if std::env::var("PAINTING_ATTRIBUTION_CHECK").is_ok() {
        // Gate mode: compare against the committed baseline instead of overwriting it. A fixture
        // present in both may not get *worse*; one only in this run is new data, not a regression.
        //
        // The baseline is a measurement, not a hand-authored limit, so editing a painting or a
        // mapping moves it legitimately - unlike `quality_baseline.csv`, whose accuracy columns are
        // a projection of the stubs precisely so that no run can re-baseline a regression away.
        // The failure message says so, because otherwise this gate reads as broken every time the
        // ground truth is improved.
        let mut baseline: std::collections::HashMap<(String, String), [usize; 3]> =
            std::collections::HashMap::new();
        let mut reader = csv::Reader::from_path(&csv_path).with_context(|| {
            format!(
                "reading the painting-attribution baseline from {} - write one with \
                 `make update-painting-attribution`",
                csv_path.display()
            )
        })?;
        for record in reader.records() {
            let record = record?;
            baseline.insert(
                (record[0].to_string(), record[1].to_string()),
                [record[3].parse()?, record[4].parse()?, record[5].parse()?],
            );
        }
        let mut worse = Vec::new();
        let mut fresh = 0usize;
        for row in &rows {
            let Some(was) = baseline.get(&(row[0].clone(), row[1].clone())) else {
                fresh += 1;
                continue;
            };
            let now: [usize; 3] = [row[3].parse()?, row[4].parse()?, row[5].parse()?];
            for (index, label) in [(0, "a reader sees"), (1, "the renderer owns")] {
                if now[index] > was[index] {
                    worse.push(format!(
                        "  {} {}: {label} {} bytes, was {}",
                        row[0], row[1], now[index], was[index]
                    ));
                }
            }
        }
        eprintln!(
            "painting attribution: {} rows checked against {}, {fresh} new",
            rows.len(),
            csv_path.display()
        );
        if !worse.is_empty() {
            bail!(
                "painting attribution regressed on {} fixture/preset pair(s):\n{}\n\nIf this \
                 follows a deliberate change to a painting or a mapping, the baseline is a \
                 measurement and has to move with it: re-run `make update-painting-attribution` \
                 and say in the commit which ground truth changed. If it follows a change to \
                 `diff::text` or to the matcher, it is a regression.",
                worse.len(),
                worse.join("\n")
            );
        }
        return Ok(());
    }
    if let Some(parent) = csv_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut writer = csv::Writer::from_path(&csv_path)?;
    writer.write_record(header)?;
    for row in &rows {
        writer.write_record(row)?;
    }
    writer.flush()?;
    eprintln!("{} rows written to {}", rows.len(), csv_path.display());

    // ---- what the numbers are over --------------------------------------------------------
    eprintln!(
        "{} fixtures measured; {unpainted} unpainted, {no_tree} with no tree-sitter grammar, \
         {} could not be measured",
        measured.len(),
        errors.len()
    );
    eprintln!(
        "{} of the measured fixtures break a ground-truth invariant, {} carry a single painting \
         held to both presets",
        measured.intersection(&violating).count(),
        measured.intersection(&single_painting).count(),
    );

    // ---- attribution ----------------------------------------------------------------------
    eprintln!("\n=== attribution: who owns the disagreement ===");
    eprintln!(
        "{:<9} {:>12} {:>12} {:>12} {:>14}",
        "preset", "real", "renderer", "matcher", "corpus bytes"
    );
    for (preset, [real, ideal, matcher, total]) in &totals {
        let percent = |part: usize| 100.0 * part as f64 / *total as f64;
        eprintln!(
            "{preset:<9} {real:>7} {:>4.2}% {ideal:>7} {:>4.2}% {matcher:>7} {:>4.2}% {total:>14}",
            percent(*real),
            percent(*ideal),
            percent(*matcher),
        );
    }
    eprintln!(
        "  real = codediff's mapping vs the painting; renderer = the human mapping rendered vs \
         the painting\n  (what no matcher fix can remove); matcher = the two renderings of the two \
         mappings against each other."
    );

    // ---- tables ---------------------------------------------------------------------------
    struct Bucket {
        runs: usize,
        bytes: usize,
        fixtures: HashSet<String>,
        sample: String,
    }
    let tally = |keep: &dyn Fn(&(&'static str, Run)) -> bool,
                 key: &dyn Fn(&Run) -> String|
     -> Vec<(String, Bucket)> {
        let mut buckets: BTreeMap<String, Bucket> = BTreeMap::new();
        for run in runs.iter().filter(|run| keep(run)) {
            let bucket = buckets.entry(key(&run.1)).or_insert_with(|| Bucket {
                runs: 0,
                bytes: 0,
                fixtures: HashSet::new(),
                sample: run.1.sample.clone(),
            });
            bucket.runs += 1;
            bucket.bytes += run.1.bytes;
            bucket.fixtures.insert(run.1.fixture.clone());
        }
        let mut rows: Vec<(String, Bucket)> = buckets.into_iter().collect();
        rows.sort_by_key(|(_, bucket)| std::cmp::Reverse((bucket.runs, bucket.bytes)));
        rows
    };
    let print = |title: &str, rows: &[(String, Bucket)], limit: usize, width: usize| {
        eprintln!("\n=== {title} ===");
        eprintln!(
            "{:<width$} {:>7} {:>9} {:>9}   example",
            "bucket",
            "runs",
            "fixtures",
            "bytes",
            width = width
        );
        for (key, bucket) in rows.iter().take(limit) {
            eprintln!(
                "{key:<width$} {:>7} {:>9} {:>9}   {}",
                bucket.runs,
                bucket.fixtures.len(),
                bucket.bytes,
                bucket.sample,
                width = width
            );
        }
        if rows.len() > limit {
            eprintln!("  ... and {} more buckets", rows.len() - limit);
        }
    };

    for stream in ["ideal", "real"] {
        let title = if stream == "ideal" {
            "renderer's own errors (human mapping rendered vs painting): ours -> theirs"
        } else {
            "everything a reader sees (codediff's mapping rendered vs painting): ours -> theirs"
        };
        print(
            title,
            &tally(&|(run_stream, _)| *run_stream == stream, &|run| {
                format!(
                    "{:<8} {:<7} -> {:<7}",
                    run.preset,
                    label_name(run.ours),
                    label_name(run.theirs)
                )
            }),
            24,
            30,
        );
    }

    // The confusion cells above split only by what the text *is*. Coarse on purpose: the
    // by-node-kind table below fragments a wide family across dozens of kinds and then shows only
    // its top rows, so a family that is 128 runs over 81 fixtures can fail to appear there at all.
    print(
        "renderer's own errors by what the disagreeing text is",
        &tally(&|(stream, _)| *stream == "ideal", &|run| {
            format!(
                "{:<8} {:<7}->{:<7} {:<11}",
                run.preset,
                label_name(run.ours),
                label_name(run.theirs),
                run.class,
            )
        }),
        40,
        40,
    );

    // The confusion cells above, opened up. The kind of the smallest node containing the run and
    // what the *human* mapping says about it are the two facts a rule has to key on.
    print(
        "renderer's own errors, by node kind and what the human mapping calls it",
        &tally(&|(stream, _)| *stream == "ideal", &|run| {
            format!(
                "{:<8} {:<7}->{:<7} {:<11} {:<24} {:<20}",
                run.preset,
                label_name(run.ours),
                label_name(run.theirs),
                run.class,
                run.kind,
                run.human_op,
            )
        }),
        50,
        86,
    );

    // The shape the user reports: a matched node painted grey where the painting wants nothing.
    print(
        "grey-where-nothing-was-wanted (ours=move, theirs=unpainted), renderer's own",
        &tally(
            &|(stream, run)| {
                *stream == "ideal" && run.ours == Some(TextLabel::Move) && run.theirs.is_none()
            },
            &|run| {
                format!(
                    "{:<8} {:<11} {:<26} {:<22} exact={:<5}",
                    run.preset, run.class, run.kind, run.human_op, run.exact
                )
            },
        ),
        40,
        80,
    );
    print(
        "grey-where-nothing-was-wanted, as a reader sees it, by the reason that mapped the node",
        &tally(
            &|(stream, run)| {
                *stream == "real" && run.ours == Some(TextLabel::Move) && run.theirs.is_none()
            },
            &|run| {
                format!(
                    "{:<8} {:<11} {:<26} {:<22} {:<28}",
                    run.preset, run.class, run.kind, run.ours_op, run.ours_reason
                )
            },
        ),
        40,
        100,
    );

    print(
        "grey-where-nothing-was-wanted, by the geometry that produced the Move",
        &tally(
            &|(stream, run)| {
                *stream == "ideal" && run.ours == Some(TextLabel::Move) && run.theirs.is_none()
            },
            &|run| format!("{:<8} {:<11} {:<46}", run.preset, run.class, run.geometry),
        ),
        30,
        68,
    );

    print(
        "renderer's own errors by language (the fixture name's own prefix)",
        &tally(&|(stream, _)| *stream == "ideal", &|run| {
            format!(
                "{:<12} {}",
                run.fixture.split('-').next().unwrap_or("?"),
                run.preset
            )
        }),
        60,
        24,
    );

    // The worklist for the shape the user reports, by fixture: a single punctuation token the
    // human mapping calls `Identical`, painted `Move` where the painting wants nothing.
    print(
        "the punctuation-painted-grey worklist, by fixture",
        &tally(
            &|(stream, run)| {
                *stream == "ideal"
                    && run.ours == Some(TextLabel::Move)
                    && run.theirs.is_none()
                    && run.class == "punctuation"
            },
            &|run| format!("{:<8} {}", run.preset, run.fixture),
        ),
        60,
        72,
    );

    // The cross-tab the punctuation rule needs: which *reason* mapped the node, per fixture. The
    // corpus's two reason-tagged carve-outs from `Move` (`known_pure_reindent`,
    // `known_pure_relocation`) are the house pattern for a narrow rule, and `identical_or_move`
    // already receives `reason`. Read against `painting_rule_experiments`' better/worse lists:
    // if the reasons separate the fixtures a blanket rule improves from the ones it breaks, that
    // is the predicate; if they do not, no available predicate does.
    print(
        "the punctuation-painted-grey worklist, by fixture and mapping reason (as a reader sees it)",
        &tally(
            &|(stream, run)| {
                *stream == "real"
                    && run.ours == Some(TextLabel::Move)
                    && run.theirs.is_none()
                    && run.class == "punctuation"
            },
            &|run| {
                format!(
                    "{:<8} {:<52} {:<30}",
                    run.preset, run.fixture, run.ours_reason
                )
            },
        ),
        80,
        92,
    );

    print(
        "fixtures ranked by the renderer's own errors",
        &tally(&|(stream, _)| *stream == "ideal", &|run| {
            run.fixture.clone()
        }),
        40,
        62,
    );

    if !errors.is_empty() {
        eprintln!("\nerrors:");
        for error in &errors {
            eprintln!("  {error}");
        }
    }
    Ok(())
}

/// EXPLORATORY: measures a candidate painting rule against the whole painted corpus **without
/// changing the product**, by post-processing the range list `ranges_for_options` produces (or,
/// where a rule is construction-time, by building the diff under a different `RenderOptions`) and
/// re-scoring the result against the same hand-painted ground truth.
///
/// The point is to keep a proposal falsifiable before it is a code change: every rule in
/// `painting_failure_census_2026_09_15.md` that could be simulated was, and the numbers here are
/// what it is worth. A rule that improves the corpus here still has to be implemented in
/// `diff::text` properly - a post-filter on a range list is not where `MINIMAL`'s punctuation
/// policy belongs - but a rule that does *not* improve it here never needs to be written at all.
///
/// Reported per experiment: bytes better/worse against each preset's own painting, and how many
/// fixtures moved in each direction. Fixture counts are the honest measure - a single large
/// fixture can carry a byte total on its own.
///
/// `cargo test --release --lib --features test-fixtures painting_rule_experiments --
/// --ignored --nocapture`
#[test]
#[ignore]
fn painting_rule_experiments() -> Result<()> {
    use crate::diff::text::{RangeMatch, RenderOptions, TextOperation};
    use std::collections::BTreeMap;

    /// The candidate rules. Each takes the raw (unfiltered) ranges for one side and returns the
    /// list to paint, standing in for what `ranges_for_options` would produce if the rule were
    /// part of it.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Rule {
        /// Baseline: exactly what ships today.
        AsShipped,
        /// R1 - a single-row range that is nothing but structural punctuation does not become a
        /// `Move` on a column shift alone. Simulated by dropping such a range after the fact,
        /// which is what calling it `Identical` in `identical_or_move` would amount to here.
        NoPunctuationMove,
        /// R2 - `MINIMAL` drops punctuation that merely moved or stayed, and keeps punctuation
        /// that was *inserted or deleted*. Simulated by filtering with `structural_punctuation`
        /// on and then dropping the structural-only ranges that are not `Insert`/`Delete`.
        MinimalKeepsEditedPunctuation,
        /// R1b - R1, narrowed by the corpus's own bracket rule: brackets share fate (see
        /// `text_painting_findings.md`, 426 pairs, zero exceptions). A lone punctuation `Move`
        /// is dropped only when it is *isolated* - when no bracket in it has a partner that some
        /// other painted range covers. A `{`/`}` pair both painted `Move` because the construct
        /// really did relocate keeps both; a `;` shifted sideways by an edit earlier on its line
        /// has no partner and goes.
        NoIsolatedPunctuationMove,
        /// R1c - R1b with the partner test narrowed to a partner that is itself painted `Move`.
        /// `restore_paired_brackets`' own `covered` asks only whether *some* range covers the
        /// partner, which keeps a `)` whose `(` sits inside an `Update` because the line was
        /// rewritten around it - a different fate, not a shared one.
        NoPunctuationMoveWithoutMovedPartner,
        /// R1d - a lone punctuation `Move` survives only when something else on its own row also
        /// painted `Move`. The reading is positional rather than syntactic: a token that shifted
        /// because its line grew is alone in moving, while a construct that genuinely relocated
        /// drags its whole row along.
        NoPunctuationMoveAloneOnItsRow,
    }

    fn range_bytes(
        contents: &str,
        range: &crate::diff::text_range::TextRange,
    ) -> Option<(usize, usize)> {
        Some((
            byte_offset(contents, range.start_row, range.start_column)?,
            byte_offset(contents, range.end_row, range.end_column)?,
        ))
    }

    /// Byte position -> matching partner byte position, for every balanced `(`/`)`, `[`/`]`,
    /// `{`/`}` pair. A local copy of `render_options::bracket_pair_partners`, which is
    /// `pub(crate)` inside a private module: re-exporting it to reach one exploratory test would
    /// leave an import the product itself never uses.
    fn bracket_pair_partners(source: &str) -> std::collections::HashMap<usize, usize> {
        let mut partners = std::collections::HashMap::new();
        let mut stack: Vec<(char, usize)> = Vec::new();
        for (byte, character) in source.char_indices() {
            match character {
                '(' | '[' | '{' => stack.push((character, byte)),
                ')' | ']' | '}' => {
                    let opener = match character {
                        ')' => '(',
                        ']' => '[',
                        _ => '{',
                    };
                    if stack.last().is_some_and(|&(open, _)| open == opener)
                        && let Some((_, open_byte)) = stack.pop()
                    {
                        partners.insert(open_byte, byte);
                        partners.insert(byte, open_byte);
                    }
                }
                _ => {}
            }
        }
        partners
    }

    fn structural_only(contents: &str, range: &RangeMatch) -> bool {
        range_bytes(contents, &range.source)
            .and_then(|(start, end)| contents.get(start..end))
            .is_some_and(crate::diff::text::is_structural_only)
    }

    let apply = |rule: Rule,
                 raw: &[RangeMatch],
                 contents: &str,
                 options: RenderOptions|
     -> Vec<RangeMatch> {
        match rule {
            Rule::AsShipped => crate::diff::text::ranges_for_options(raw, contents, options),
            Rule::NoPunctuationMove => {
                let mut ranges = crate::diff::text::ranges_for_options(raw, contents, options);
                ranges.retain(|range| {
                    range.operation != TextOperation::Move
                        || range.source.end_row != range.source.start_row
                        || !structural_only(contents, range)
                });
                ranges
            }
            Rule::NoIsolatedPunctuationMove => {
                let ranges = crate::diff::text::ranges_for_options(raw, contents, options);
                let droppable = |range: &RangeMatch| {
                    range.operation == TextOperation::Move
                        && range.source.end_row == range.source.start_row
                        && structural_only(contents, range)
                };
                // "Covered by another painted range" in the same sense
                // `restore_paired_brackets` means it: any surviving range, the punctuation ones
                // included, so a `{`/`}` pair painted `Move` on both ends keeps itself.
                let covered = |byte: usize| {
                    ranges.iter().any(|range| {
                        range_bytes(contents, &range.source)
                            .is_some_and(|(start, end)| start <= byte && byte < end)
                    })
                };
                let partners = bracket_pair_partners(contents);
                let has_painted_partner = |range: &RangeMatch| {
                    let Some((start, end)) = range_bytes(contents, &range.source) else {
                        return false;
                    };
                    let Some(text) = contents.get(start..end) else {
                        return false;
                    };
                    text.char_indices()
                        .filter(|&(_, c)| matches!(c, '(' | ')' | '[' | ']' | '{' | '}'))
                        .any(|(offset, _)| {
                            partners
                                .get(&(start + offset))
                                .is_some_and(|&partner| covered(partner))
                        })
                };
                let keep: Vec<bool> = ranges
                    .iter()
                    .map(|range| !droppable(range) || has_painted_partner(range))
                    .collect();
                ranges
                    .into_iter()
                    .zip(keep)
                    .filter_map(|(range, keep)| keep.then_some(range))
                    .collect()
            }
            Rule::NoPunctuationMoveWithoutMovedPartner => {
                let ranges = crate::diff::text::ranges_for_options(raw, contents, options);
                let droppable = |range: &RangeMatch| {
                    range.operation == TextOperation::Move
                        && range.source.end_row == range.source.start_row
                        && structural_only(contents, range)
                };
                let moved_covers = |byte: usize, self_start: usize| {
                    ranges.iter().any(|range| {
                        range.operation == TextOperation::Move
                            && range_bytes(contents, &range.source).is_some_and(|(start, end)| {
                                start != self_start && start <= byte && byte < end
                            })
                    })
                };
                let partners = bracket_pair_partners(contents);
                ranges
                    .iter()
                    .filter(|range| {
                        if !droppable(range) {
                            return true;
                        }
                        let Some((start, end)) = range_bytes(contents, &range.source) else {
                            return true;
                        };
                        let Some(text) = contents.get(start..end) else {
                            return true;
                        };
                        text.char_indices()
                            .filter(|&(_, c)| matches!(c, '(' | ')' | '[' | ']' | '{' | '}'))
                            .any(|(offset, _)| {
                                partners
                                    .get(&(start + offset))
                                    .is_some_and(|&partner| moved_covers(partner, start))
                            })
                    })
                    .cloned()
                    .collect()
            }
            Rule::NoPunctuationMoveAloneOnItsRow => {
                let ranges = crate::diff::text::ranges_for_options(raw, contents, options);
                let droppable = |range: &RangeMatch| {
                    range.operation == TextOperation::Move
                        && range.source.end_row == range.source.start_row
                        && structural_only(contents, range)
                };
                ranges
                    .iter()
                    .filter(|range| {
                        !droppable(range)
                            || ranges.iter().any(|other| {
                                other.operation == TextOperation::Move
                                    && other.source != range.source
                                    && other.source.start_row <= range.source.start_row
                                    && other.source.end_row >= range.source.start_row
                            })
                    })
                    .cloned()
                    .collect()
            }
            Rule::MinimalKeepsEditedPunctuation => {
                let kept = crate::diff::text::ranges_for_options(
                    raw,
                    contents,
                    RenderOptions {
                        structural_punctuation: true,
                        ..options
                    },
                );
                kept.into_iter()
                    .filter(|range| {
                        matches!(
                            range.operation,
                            TextOperation::Insert | TextOperation::Delete
                        ) || !structural_only(contents, range)
                    })
                    .collect()
            }
        }
    };

    let diffs_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("diffs");
    let mut names: Vec<String> = Vec::new();
    for dataset in crate::test::helper::DIFF_DATASETS {
        let dir = diffs_dir.join(dataset);
        if !dir.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&dir)?.filter_map(|entry| entry.ok()) {
            if entry.path().is_dir() {
                names.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
    }
    names.sort();

    /// `(baseline bytes, candidate bytes, fixtures better, fixtures worse)` per experiment.
    #[derive(Default)]
    struct Score {
        baseline: usize,
        candidate: usize,
        better: usize,
        worse: usize,
        better_names: Vec<String>,
        worse_names: Vec<String>,
    }
    let mut scores: BTreeMap<String, Score> = BTreeMap::new();

    for name in &names {
        let Ok(pair) = crate::test::helper::handmade_test_code_pair(name) else {
            continue;
        };
        let (before, after) = &*pair;
        let Ok(mapping) = load(name) else {
            continue;
        };
        if mapping.text_mappings.is_empty() || before.ast.is_none() || after.ast.is_none() {
            continue;
        }
        let diff = crate::diff::diff_code(before, after);
        let Some(ast) = diff.ast.as_ref() else {
            continue;
        };
        let node_cache = crate::diff::NodeCache::build(before, after);
        let contents = [&before.contents, &after.contents];

        for (preset, options) in [
            ("minimal", RenderOptions::MINIMAL),
            ("full", RenderOptions::FULL),
        ] {
            let Ok(candidates) = paintings_for_mode(&mapping, options) else {
                continue;
            };
            // Every candidate painting for the preset, reduced to labels once - the experiments
            // are each scored against the closest of them, exactly as the grader does.
            let mut painted: Vec<[Vec<Option<TextLabel>>; 2]> = Vec::new();
            for painting in candidates {
                let mut spans: [Vec<(HumanTextSpan, TextLabel)>; 2] = [Vec::new(), Vec::new()];
                for entry in &painting.mapping.entries {
                    let label =
                        TextLabel::from_verdict(entry.verdict(&before.contents, &after.contents)?);
                    for span in &entry.before {
                        spans[0].push((*span, label));
                    }
                    for span in &entry.after {
                        spans[1].push((*span, label));
                    }
                }
                painted
                    .push([0usize, 1usize].map(|side| label_bytes(contents[side], &spans[side])));
            }

            let raw = {
                let text_diff = crate::diff::text::TextDiff::from_with_options(
                    before,
                    after,
                    ast,
                    &node_cache,
                    options,
                );
                [text_diff.all(0), text_diff.all(1)]
            };
            // Scored against the closest of the preset's candidate paintings, exactly as
            // `compare_painting`'s grader scores: alternatives under one preset are alternative
            // readings, not a conjunction.
            let score_labels = |ours: &[Vec<Option<TextLabel>>; 2]| -> usize {
                painted
                    .iter()
                    .map(|theirs| -> usize {
                        (0..2)
                            .map(|side| {
                                ours[side]
                                    .iter()
                                    .zip(&theirs[side])
                                    .filter(|(ours, theirs)| ours != theirs)
                                    .count()
                            })
                            .sum()
                    })
                    .min()
                    .unwrap_or(0)
            };
            let distance = |rule: Rule| -> usize {
                let ours = [0usize, 1usize].map(|side| {
                    let ranges = apply(rule, &raw[side], contents[side], options);
                    label_bytes_from_ranges(contents[side], &ranges)
                });
                score_labels(&ours)
            };

            let baseline = distance(Rule::AsShipped);
            let mut rules = vec![
                ("R1  no punctuation Move", Rule::NoPunctuationMove),
                (
                    "R1b no isolated punctuation Move",
                    Rule::NoIsolatedPunctuationMove,
                ),
                (
                    "R1c punctuation Move needs a moved partner",
                    Rule::NoPunctuationMoveWithoutMovedPartner,
                ),
                (
                    "R1d punctuation Move needs company on its row",
                    Rule::NoPunctuationMoveAloneOnItsRow,
                ),
            ];
            if preset == "minimal" {
                rules.push((
                    "R2 minimal keeps edited punctuation",
                    Rule::MinimalKeepsEditedPunctuation,
                ));
            }
            for (label, rule) in rules {
                let candidate = distance(rule);
                let score = scores.entry(format!("{preset:<8} {label}")).or_default();
                score.baseline += baseline;
                score.candidate += candidate;
                if candidate < baseline {
                    score.better += 1;
                    score.better_names.push(name.clone());
                } else if candidate > baseline {
                    score.worse += 1;
                    score.worse_names.push(name.clone());
                }
            }

            // R3 is construction-time, not a post-filter: `whole_pair_updates` decides which
            // ranges `TextDiff` builds at all (see its own doc comment), so it needs its own
            // build rather than a different filtering of `raw`.
            if preset == "full" {
                let options = RenderOptions {
                    whole_pair_updates: true,
                    ..RenderOptions::FULL
                };
                let text_diff = crate::diff::text::TextDiff::from_with_options(
                    before,
                    after,
                    ast,
                    &node_cache,
                    options,
                );
                let ours = [0usize, 1usize].map(|side| {
                    let ranges = crate::diff::text::ranges_for_options(
                        &text_diff.all(side),
                        contents[side],
                        options,
                    );
                    label_bytes_from_ranges(contents[side], &ranges)
                });
                let candidate = score_labels(&ours);
                let score = scores
                    .entry("full     R3 whole-pair updates".to_string())
                    .or_default();
                score.baseline += baseline;
                score.candidate += candidate;
                if candidate < baseline {
                    score.better += 1;
                    score.better_names.push(name.clone());
                } else if candidate > baseline {
                    score.worse += 1;
                    score.worse_names.push(name.clone());
                }
            }
        }
    }

    eprintln!(
        "{:<48} {:>10} {:>10} {:>9} {:>7} {:>7}",
        "experiment", "baseline", "candidate", "delta", "better", "worse"
    );
    for (label, score) in &scores {
        eprintln!(
            "{label:<48} {:>10} {:>10} {:>9} {:>7} {:>7}",
            score.baseline,
            score.candidate,
            score.candidate as i64 - score.baseline as i64,
            score.better,
            score.worse,
        );
    }
    for (label, score) in &scores {
        eprintln!("\n{label}");
        eprintln!("  better: {}", score.better_names.join(", "));
        eprintln!("  worse:  {}", score.worse_names.join(", "));
    }
    Ok(())
}

/// EXPLORATORY: every `Move` range codediff paints over text that is nothing but structural
/// punctuation, for one fixture, raw and after filtering, with both sides' text.
///
/// The census reports this family as 89 runs over 35 fixtures - single tokens (`;`, `{`, `)`,
/// `);`) the human mapping calls `Identical`, painted `Move` under `FULL` where the painting wants
/// nothing. It also reports their two sides' text as *differing*, which a `;` matched to a `;`
/// should not do; `RangeMatch::extends` merges same-operation ranges across whitespace, so the
/// question this answers is whether the range being painted is the token itself or a merged span
/// that merely starts at one. The rule that fixes the family has to be written against whichever
/// one it is.
///
/// `FIXTURE=<name> SIDE=<0|1> cargo test --release --lib --features test-fixtures
/// punctuation_move_provenance -- --ignored --nocapture`
#[test]
#[ignore]
fn punctuation_move_provenance() -> Result<()> {
    use crate::diff::text::{RenderOptions, TextOperation};

    let name = std::env::var("FIXTURE")
        .unwrap_or_else(|_| "java-defects4j-mockito-17-mocksettingsimpl".to_string());
    let side: usize = std::env::var("SIDE")
        .ok()
        .and_then(|side| side.parse().ok())
        .unwrap_or(0);
    let (before, after) = &*crate::test::helper::handmade_test_code_pair(&name)?;
    let diff = crate::diff::diff_code(before, after);
    let ast = diff.ast.as_ref().context("no AST diff")?;
    let node_cache = crate::diff::NodeCache::build(before, after);
    let contents = [&before.contents, &after.contents];

    let text_at = |side: usize, range: &crate::diff::text_range::TextRange| -> String {
        match (
            byte_offset(contents[side], range.start_row, range.start_column),
            byte_offset(contents[side], range.end_row, range.end_column),
        ) {
            (Some(start), Some(end)) => contents[side]
                .get(start..end)
                .map(|text| format!("{text:?}"))
                .unwrap_or_else(|| "<unaddressable>".to_string()),
            _ => "<unaddressable>".to_string(),
        }
    };

    let text_diff = crate::diff::text::TextDiff::from_with_options(
        before,
        after,
        ast,
        &node_cache,
        RenderOptions::FULL,
    );
    let raw = text_diff.all(side);
    let filtered = crate::diff::text::ranges_for_options(&raw, contents[side], RenderOptions::FULL);

    for (label, ranges) in [("raw", &raw), ("filtered FULL", &filtered)] {
        eprintln!("--- {label}: Move ranges whose source text is structural-only ---");
        for range in ranges.iter() {
            if range.operation != TextOperation::Move {
                continue;
            }
            let source = text_at(side, &range.source);
            let structural = source
                .trim_matches('"')
                .chars()
                .all(|c| c.is_whitespace() || "()[]{},;:".contains(c));
            if !structural {
                continue;
            }
            eprintln!(
                "  source={:?} {} <-> destination={:?} {}",
                range.source,
                source,
                range.destination,
                text_at(1 - side, &range.destination),
            );
        }
    }
    Ok(())
}
