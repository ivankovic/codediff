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

// Hand-run instruments, not checks. Every fn here is `#[test] #[ignore]`: it prints or dumps an
// analysis rather than asserting anything, so `cargo test` compiles them and runs none of them.
//
// Two write a live artifact and are wired into a Make target or a plan - `painting_failure_census`
// (`make update-painting-attribution`) and `mismatch_census`. The other four take a fixture name
// or a list through an env var and answer a question about that fixture, which is what makes them
// worth keeping: a corpus-wide census answers its question once and belongs in
// `research/data/quality/` as a write-up, while "why does *this* fixture disagree" comes up every
// time a fixture is clamped.

use super::*;

/// EXPLORATORY: every disagreement run between the *tree mapping* and the painting (structural
/// agreement) for the fixture named by the `FIXTURE` env var - unlike `painting_disagreement_detail`
/// below, which checks codediff's *rendering* against it.
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
/// `research/data/quality/painting_attribution.csv` already carries every fixture's rate; naming a
/// handful is the fastest way to re-measure after a change, without paying for the whole corpus.
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
    // already receives `reason`. Read against the fixtures a candidate blanket rule improves and
    // the ones it breaks: if the reasons separate the two sets, that is the predicate; if they do
    // not, no available predicate does.
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
