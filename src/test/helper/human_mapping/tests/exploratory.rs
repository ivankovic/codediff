/*  This file is part of the OmniDiff code diffing tool.
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

// Hand-run instruments: every fn here is `#[test] #[ignore]` and prints or writes an analysis.
// `painting_failure_census`, `mismatch_census`, `cross_fixture_convention_census` and
// `nm_candidate_census` write artifacts, and `painting_failure_census` is also the painting gate
// (`make check-painting-attribution`, `PAINTING_ATTRIBUTION_CHECK=1`, run in CI). The rest answer
// "why does *this* fixture disagree" for the fixtures named in an env var.

use super::*;

/// EXPLORATORY: every disagreement run between the *tree mapping* and the painting for the fixture
/// in `FIXTURE` (`painting_disagreement_detail` checks omnidiff's *rendering* instead).
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

/// EXPLORATORY: just the Minimal/Full percentages for the comma-separated `FIXTURES`, to
/// re-measure a few fixtures after a change: `FIXTURES=a,b,c cargo test --lib --features
/// test-fixtures measure_stub_fixtures -- --ignored --nocapture`.
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

/// EXPLORATORY: every run of bytes where omnidiff's rendering disagrees with the closest human
/// painting for one fixture (`compare_painting`'s byte projection): `FIXTURE=<name>
/// MODE=<minimal|full> cargo test --lib --features test-fixtures painting_disagreement_detail --
/// --ignored --nocapture`.
///
/// `MAPPING=human` renders the human tree mapping instead, with omnidiff's reasons borrowed as
/// `painting_failure_census` does - the runs only a rendering change can fix.
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
    let human = std::env::var("MAPPING").is_ok_and(|m| m.eq_ignore_ascii_case("human"));

    let diff = crate::diff::diff_code(before, after);
    let real = diff
        .ast
        .as_ref()
        .with_context(|| format!("omnidiff produced no AST diff for '{name}'"))?;
    let ast = if human {
        let mut human_ast = as_ast_diff_for_mapping(&mapping, before, after)?;
        for (key, pair) in human_ast.mapping.iter_mut() {
            if let Some(ours) = real.mapping.get(key) {
                pair.reason = ours.reason;
            }
        }
        human_ast
    } else {
        real.clone()
    };
    let node_cache = crate::diff::NodeCache::build(before, after);
    let render = |ast: &ASTDiff| -> Vec<Vec<Option<TextLabel>>> {
        let text_diff = crate::diff::text::TextDiff::from_with_options(
            before,
            after,
            ast,
            &node_cache,
            options,
        );
        [(0usize, &before.contents), (1usize, &after.contents)]
            .into_iter()
            .map(|(side, contents)| {
                let ranges =
                    crate::diff::text::ranges_for_options(&text_diff.all(side), contents, options);
                label_bytes_from_ranges(contents, &ranges)
            })
            .collect()
    };
    let ours = render(&ast);
    // The candidate is chosen against omnidiff's own rendering even under `MAPPING=human`, as
    // the census chooses it, so the two report runs against the same painting.
    let chooser = if human { render(real) } else { ours.clone() };

    // Mismatched bytes, the painting, and its per-byte labels per side.
    type Candidate<'a> = (usize, &'a NamedTextMapping, [Vec<Option<TextLabel>>; 2]);
    let mut best: Option<Candidate> = None;
    for painting in paintings_for_mode(&mapping, options)? {
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
        let theirs = [
            label_bytes(&before.contents, &painted[0]),
            label_bytes(&after.contents, &painted[1]),
        ];
        let mismatched: usize = (0..2)
            .map(|side| {
                chooser[side]
                    .iter()
                    .zip(&theirs[side])
                    .filter(|(a, b)| a != b)
                    .count()
            })
            .sum();
        if best
            .as_ref()
            .is_none_or(|(least, _, _)| mismatched < *least)
        {
            best = Some((mismatched, painting, theirs));
        }
    }
    let (_, painting, theirs) = best.context("a preset with no candidate paintings")?;

    eprintln!(
        "fixture={name} mode={mode} mapping={} (painting solution='{}')",
        if human { "human" } else { "omnidiff" },
        painting.name
    );
    for (side, contents) in [(0usize, &before.contents), (1usize, &after.contents)] {
        let (ours, theirs) = (&ours[side], &theirs[side]);
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

/// DIAGNOSTIC: every ground-truth invariant violation, for the comma-separated `FIXTURES` or the
/// whole corpus: `FIXTURES=a,b cargo test --lib --features test-fixtures invariant_violations --
/// --ignored --nocapture`. The per-fixture test only counts them; this says what they are.
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

/// EXPLORATORY: the mismatches of one fixture spelled out, with each node's own bytes and
/// position (a mismatch on a `,` is unreadable without them). Invisible mismatches are printed
/// under their own heading: they often explain an arbitrary-looking visible one.
///
/// `FIXTURE=name cargo test --release --lib --features test-fixtures mismatch_detail_for_fixture
/// -- --ignored --nocapture`
#[test]
#[ignore]
fn mismatch_detail_for_fixture() -> Result<()> {
    let name = std::env::var("FIXTURE").unwrap_or_else(|_| "rust-add-if".to_string());
    let (before, after) = &*crate::test::helper::handmade_test_code_pair(&name)?;
    let config = crate::diff::HeuristicConfig::default();
    let found = compute_visible_mismatches_for_with_config(&name, before, after, &config)?;

    /// The node's own bytes and start position, or `-` when the id is not in that side's tree.
    fn locate(code: &crate::code::Code, node_id: usize) -> String {
        let Some(tree) = code.ast.as_ref() else {
            return "-".to_string();
        };
        let mut stack = vec![tree.root_node()];
        while let Some(node) = stack.pop() {
            if node.id() == node_id {
                let point = node.start_position();
                let text = code.contents.get(node.byte_range()).unwrap_or("");
                return format!("{}:{} {:?}", point.row + 1, point.column + 1, text);
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
        }
        "-".to_string()
    }

    // The message names the chosen partner by kind only; its position settles which one it was.
    let diff = crate::diff::diff_code_with_config(before, after, &config);
    let diff_ast = diff.ast.as_ref();

    for (heading, mismatches) in [("visible", &found.visible), ("invisible", &found.invisible)] {
        eprintln!("\n=== {} {heading} ===", mismatches.len());
        for mismatch in mismatches.iter() {
            let (side, code, partner_code) = match mismatch.side {
                Side::Before => ("before", before, after),
                Side::After => ("after", after, before),
            };
            let partner = diff_ast
                .and_then(|diff_ast| match mismatch.side {
                    Side::Before => diff_ast.before_node_map.get(&mismatch.node_id),
                    Side::After => diff_ast.after_node_map.get(&mismatch.node_id),
                })
                .map(|&partner_id| locate(partner_code, partner_id))
                .unwrap_or_else(|| "0 (unmapped)".to_string());
            eprintln!(
                "{side} {}  -> omnidiff chose {partner}",
                locate(code, mismatch.node_id)
            );
            eprintln!("    {}", mismatch.message);
        }
    }
    Ok(())
}

/// **Every mismatch in the corpus, classified** - the mapping-side counterpart of
/// [`painting_failure_census`]. One row per mismatch in
/// `research/data/quality/mismatch_census.csv`: `expected_op`, `actual_op` and `reason` parsed
/// back out of the generated mismatch message, `kind`/`parent_kind`/`named` from its node.
///
/// `cargo test --release --lib --features test-fixtures mismatch_census -- --ignored --nocapture`
#[test]
#[ignore]
// `csv` is only linked under `test-fixtures`.
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

    /// The `(op X, reason Y)` tail, or `-` when the message carries none.
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

    /// `APTED("qualified_name")` -> (`APTED`, `qualified_name`), so the reason groups by pass.
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
        // A `*WithChildren` message names no pass, so the node's own entry is read from a
        // recomputed diff, as `actual_mapping_info` does.
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

/// **Where two fixtures' paintings answer the same question differently** - the cross-fixture
/// axis the intra-fixture invariants cannot see.
///
/// The population is leaves the human's tree mapping pairs with a leaf that reads the same
/// ([`LeafStatus::Same`]): the text survived, so only its geometry and whether it is painted
/// remain. The geometry is facts about the *file*, not the renderer's predicates (which could only
/// rediscover the renderer): `row_moved`, `column_moved`, `row_edited`, and
/// `drift_matches_neighbours` (pushed by an insertion above, versus genuinely relocated).
///
/// The verdict is `clean`, `painted` (with its label) or `partial`; `partial` is the painting's
/// chunking and is counted as neither. Visible leaves and the two named presets only, since the
/// presets are specified to disagree. The CSV holds only non-`clean` leaves, each with its class's
/// `clean` count as denominator.
///
/// `cargo test --release --lib --features test-fixtures cross_fixture_convention_census --
/// --ignored --nocapture`
#[test]
#[ignore]
// `csv` is only linked under `test-fixtures`.
#[cfg(feature = "test-fixtures")]
fn cross_fixture_convention_census() -> Result<()> {
    use crate::test::helper::human_mapping::invariants::painted_labels;
    use crate::test::helper::human_mapping::invariants::{
        LeafStatus, TreeContext, is_visible_leaf,
    };
    use std::collections::BTreeMap;

    /// The row `offset` falls on, as text - `None` past the end.
    fn row_of(contents: &str, row: usize) -> Option<&str> {
        contents.split('\n').nth(row)
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

    /// One leaf whose painting verdict is not `clean`.
    struct Exception {
        fixture: String,
        preset: String,
        side: &'static str,
        class: String,
        verdict: String,
        kind: String,
        /// 1-based, so it reads like a file's own gutter.
        row: usize,
        drift: usize,
        drift_matches_neighbours: bool,
    }

    // Held until the end, when each class's `clean` denominator is known.
    let mut exceptions: Vec<Exception> = Vec::new();
    // (preset, class) -> fixture -> (clean, painted, partial)
    let mut tally: BTreeMap<(String, String), BTreeMap<String, [usize; 3]>> = BTreeMap::new();

    for name in &names {
        let Ok(pair) = crate::test::helper::handmade_test_code_pair(name) else {
            continue;
        };
        let (before, after) = &*pair;
        let Ok(mapping) = load(name) else { continue };
        let (Some(before_tree), Some(after_tree)) = (before.ast.as_ref(), after.ast.as_ref())
        else {
            continue;
        };
        let context = TreeContext::build(&mapping, before_tree.root_node(), after_tree.root_node());

        // Every `Same` leaf's row delta, per side, in document order.
        let drifts: [Vec<(usize, i64)>; 2] = std::array::from_fn(|side| {
            context.leaves[side]
                .iter()
                .filter_map(|&leaf| match context.status(leaf, side) {
                    LeafStatus::Same(partner) => Some((
                        leaf.id(),
                        partner.start_position().row as i64 - leaf.start_position().row as i64,
                    )),
                    _ => None,
                })
                .collect()
        });

        for named in &mapping.text_mappings {
            let preset = if super::invariants::designates_minimal(&named.name) {
                "Minimal"
            } else if super::invariants::designates_full(&named.name) {
                "Full"
            } else {
                continue;
            };
            let Ok(labels) = painted_labels(named, before, after) else {
                continue;
            };

            for side in 0..2 {
                let (contents, partner_contents) = if side == 0 {
                    (&before.contents, &after.contents)
                } else {
                    (&after.contents, &before.contents)
                };
                for &leaf in &context.leaves[side] {
                    if !is_visible_leaf(leaf, contents) {
                        continue;
                    }
                    let LeafStatus::Same(partner) = context.status(leaf, side) else {
                        continue;
                    };

                    let row_moved = leaf.start_position().row != partner.start_position().row;
                    let column_moved =
                        leaf.start_position().column != partner.start_position().column;
                    let row_edited = row_of(contents, leaf.start_position().row)
                        != row_of(partner_contents, partner.start_position().row);
                    let class = format!(
                        "{}{}{}",
                        if row_moved { "row" } else { "-" },
                        if column_moved { "+col" } else { "+-" },
                        if row_edited { "+edited" } else { "+same" },
                    );

                    // No `Same` neighbour reports `false`: the column means "verified to move
                    // with its surroundings".
                    let drift =
                        partner.start_position().row as i64 - leaf.start_position().row as i64;
                    let index = drifts[side].iter().position(|&(id, _)| id == leaf.id());
                    let drift_matches_neighbours = index.is_some_and(|index| {
                        let neighbours = index
                            .checked_sub(1)
                            .and_then(|before| drifts[side].get(before))
                            .into_iter()
                            .chain(drifts[side].get(index + 1));
                        let mut any = false;
                        for &(_, neighbour) in neighbours {
                            if neighbour != drift {
                                return false;
                            }
                            any = true;
                        }
                        any
                    });

                    let painted = &labels[side][leaf.byte_range()];
                    let verdict = if painted.iter().all(Option::is_none) {
                        "clean".to_string()
                    } else if let Some(label) = painted.first().copied().flatten()
                        && painted.iter().all(|slot| *slot == Some(label))
                    {
                        format!("painted:{label:?}")
                    } else {
                        "partial".to_string()
                    };
                    let bucket = tally
                        .entry((preset.to_string(), class.clone()))
                        .or_default()
                        .entry(name.clone())
                        .or_default();
                    bucket[match verdict.as_str() {
                        "clean" => 0,
                        "partial" => 2,
                        _ => 1,
                    }] += 1;

                    if verdict != "clean" {
                        exceptions.push(Exception {
                            fixture: name.clone(),
                            preset: preset.to_string(),
                            side: if side == 0 { "before" } else { "after" },
                            class,
                            verdict,
                            kind: leaf.kind().to_string(),
                            row: leaf.start_position().row + 1,
                            drift: drift.unsigned_abs() as usize,
                            drift_matches_neighbours,
                        });
                    }
                }
            }
        }
    }

    let csv_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("research")
        .join("data")
        .join("quality")
        .join("convention_census.csv");
    let mut writer = csv::Writer::from_path(&csv_path)?;
    writer.write_record([
        "fixture",
        "preset",
        "side",
        "class",
        "verdict",
        "kind",
        "row",
        "row_drift",
        "drift_matches_neighbours",
        "clean_in_class",
    ])?;
    for exception in &exceptions {
        let clean = tally
            .get(&(exception.preset.clone(), exception.class.clone()))
            .and_then(|per| per.get(&exception.fixture))
            .map(|counts| counts[0])
            .unwrap_or(0);
        writer.write_record([
            &exception.fixture,
            &exception.preset,
            &exception.side.to_string(),
            &exception.class,
            &exception.verdict,
            &exception.kind,
            &exception.row.to_string(),
            &exception.drift.to_string(),
            &exception.drift_matches_neighbours.to_string(),
            &clean.to_string(),
        ])?;
    }
    writer.flush()?;
    eprintln!(
        "{} exception(s) of {} leaves -> {}",
        exceptions.len(),
        tally
            .values()
            .flat_map(|per| per.values())
            .map(|counts| counts.iter().sum::<usize>())
            .sum::<usize>(),
        csv_path.display()
    );

    // A fixture both "clean" and "painted" for one class contradicts itself and is counted apart.
    eprintln!(
        "\n{:<9} {:<18} {:>6} {:>8} {:>6} {:>9}",
        "preset", "class", "clean", "painted", "both", "leaves"
    );
    for ((preset, class), per_fixture) in &tally {
        let (mut clean, mut painted, mut both) = (0usize, 0usize, 0usize);
        let mut leaves = 0usize;
        for counts in per_fixture.values() {
            leaves += counts[0] + counts[1] + counts[2];
            match (counts[0] > 0, counts[1] > 0) {
                (true, false) => clean += 1,
                (false, true) => painted += 1,
                (true, true) => both += 1,
                (false, false) => {}
            }
        }
        eprintln!("{preset:<9} {class:<18} {clean:>6} {painted:>8} {both:>6} {leaves:>9}");
    }
    Ok(())
}

/// EXPLORATORY: every run of bytes where omnidiff's rendering disagrees with the painting,
/// classified and **attributed** to the node matcher or the renderer, per preset:
///
/// * `real` - `diff_code`'s mapping, rendered. What a reader sees.
/// * `ideal` - the human tree mapping through the same `TextDiff` (via
///   [`as_ast_diff_for_mapping`]): what the renderer paints with perfect matching.
/// * `painted` - the painting the preset is answerable to.
///
/// `ideal` vs `painted` is what no matcher improvement can remove; `real` vs `ideal` is the
/// matcher's. Counted by runs and fixtures before bytes, since a byte ranking hides small
/// repeated mistakes.
///
/// `cargo test --release --lib --features test-fixtures painting_failure_census --
/// --ignored --nocapture`
#[test]
#[ignore]
// `csv` is only linked under `test-fixtures`.
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
        /// For a `Move` we painted: the geometry `identical_or_move` decided it on. Empty
        /// otherwise, or when no single range covers the run's first byte.
        geometry: String,
        sample: String,
    }

    fn label_name(label: Option<TextLabel>) -> &'static str {
        match label {
            None => "-",
            Some(label) => label.name(),
        }
    }

    /// `whitespace` (nothing visible), `punctuation` (every visible character is one
    /// [`crate::diff::text::is_structural_only`] drops), or `code` - the first two are what
    /// `MINIMAL`'s filters are *for*.
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

    /// The shape `identical_or_move` read to call the range covering `byte` a `Move`: start
    /// column changed, start row changed, spans a row boundary, text byte-identical. `col-same`
    /// with no row boundary can only be `crossed_backwards`' doing.
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
        // Three-valued: an unaddressable destination is *unknown*, not different.
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
    // One row per (fixture, preset), written at the end.
    let mut rows: Vec<[String; 8]> = Vec::new();
    // preset -> (real, ideal, matcher) mismatched bytes, and the corpus size they are out of.
    let mut totals: BTreeMap<&'static str, [usize; 4]> = BTreeMap::new();
    let mut measured: HashSet<String> = HashSet::new();
    let mut violating: HashSet<String> = HashSet::new();
    let mut single_painting: HashSet<String> = HashSet::new();
    let (mut unpainted, mut no_tree, mut errors) = (0usize, 0usize, Vec::new());

    // One fixture's share of every tally above, so the fixtures can be measured in parallel and
    // merged in name order: the artifact is the same as a serial pass's.
    #[derive(Default)]
    struct FixtureCensus {
        runs: Vec<(&'static str, Run)>,
        rows: Vec<[String; 8]>,
        totals: BTreeMap<&'static str, [usize; 4]>,
        measured: bool,
        violating: bool,
        single_painting: bool,
        unpainted: bool,
        no_tree: bool,
        error: Option<String>,
    }
    let census_of = |name: &String| -> Result<FixtureCensus> {
        let mut census = FixtureCensus::default();
        // Parsed here and dropped with the census, not kept in `handmade_test_code_pair`'s
        // process-wide cache: eight workers fill that with the whole corpus (19 GB).
        let Some(Ok(Some(pair))) = crate::test::helper::diffs_case_dir(name)
            .map(|dir| crate::test::helper::code_pair_from_dir(&dir))
        else {
            return Ok(census);
        };
        let (before, after) = (&pair.0, &pair.1);
        let Ok(mapping) = load_with(name, before, after) else {
            return Ok(census);
        };
        if mapping.text_mappings.is_empty() {
            census.unpainted = true;
            return Ok(census);
        }
        if before.ast.is_none() || after.ast.is_none() {
            // The plain-text fallback has no tree to classify against. Counted, not measured.
            census.no_tree = true;
            return Ok(census);
        }
        let real_diff = crate::diff::diff_code(before, after);
        let Some(real_ast) = real_diff.ast.as_ref() else {
            return Ok(census);
        };
        let mut human_ast = match as_ast_diff_for_mapping(&mapping, before, after) {
            Ok(diff) => diff,
            Err(e) => {
                census.error = Some(format!("{name}: human mapping -> ASTDiff: {e:#}"));
                return Ok(census);
            }
        };
        // The human format records no `ASTMappingReason`, but `identical_or_move` reads one to
        // keep a verified pure reindent or relocation unpainted. Pairs both mappings make borrow
        // omnidiff's reason, or `ideal` would blame the renderer for `Move`s it never paints.
        for (key, human) in human_ast.mapping.iter_mut() {
            if let Some(real) = real_ast.mapping.get(key) {
                human.reason = real.reason;
            }
        }
        let node_cache = crate::diff::NodeCache::build(before, after);
        let roots = [
            before.ast.as_ref().unwrap().root_node(),
            after.ast.as_ref().unwrap().root_node(),
        ];
        let contents = [&before.contents, &after.contents];

        if mapping.text_mappings.len() == 1 {
            census.single_painting = true;
        }
        if crate::test::helper::human_mapping::invariants::ground_truth_invariant_violations_for(
            &mapping, before, after,
        )
        .is_ok_and(|violations| !violations.is_empty())
        {
            census.violating = true;
        }

        for (preset, options) in [
            ("minimal", RenderOptions::MINIMAL),
            ("full", RenderOptions::FULL),
        ] {
            let Ok(candidates) = paintings_for_mode(&mapping, options) else {
                continue;
            };
            // The ranges come back with the labels: the range says *why* it was painted.
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

            // One painting per preset for all three comparisons, so they decompose: the one
            // closest to what a reader sees, as `compare_painting` chooses.
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

            census.measured = true;
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
            census.rows.push([
                name.clone(),
                preset.to_string(),
                (before.contents.len() + after.contents.len()).to_string(),
                per_fixture[0].to_string(),
                per_fixture[1].to_string(),
                per_fixture[2].to_string(),
                mapping.text_mappings.len().to_string(),
                usize::from(census.violating).to_string(),
            ]);
            let entry = census.totals.entry(preset).or_insert([0; 4]);
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
                    census.totals.get_mut(preset).unwrap()[index] += ours
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
                        // One run is one *verdict pair*: two adjacent different mistakes are two
                        // runs.
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
                        census.runs.push((
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
                                    // Cut at a character boundary: byte 40 can land inside one.
                                    &text[..(0..=40.min(text.len()))
                                        .rev()
                                        .find(|&at| text.is_char_boundary(at))
                                        .unwrap_or(0)]
                                ),
                            },
                        ));
                    }
                }
            }
        }
        Ok(census)
    };

    let next = std::sync::atomic::AtomicUsize::new(0);
    let results: Vec<std::sync::Mutex<Option<Result<FixtureCensus>>>> =
        names.iter().map(|_| std::sync::Mutex::new(None)).collect();
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    std::thread::scope(|scope| {
        for _ in 0..threads {
            std::thread::Builder::new()
                // As deep as the diff recurses; `tui::app::DIFF_COMPUTE_STACK_SIZE` is the same.
                .stack_size(256 * 1024 * 1024)
                .spawn_scoped(scope, || {
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(name) = names.get(i) else { break };
                        let census = census_of(name);
                        *results[i].lock().expect("no worker panics holding it") = Some(census);
                    }
                })
                .expect("spawn census worker");
        }
    });
    for (name, result) in names.iter().zip(results) {
        let census = result
            .into_inner()
            .expect("no worker panics holding it")
            .expect("every fixture measured")?;
        runs.extend(census.runs);
        rows.extend(census.rows);
        for (preset, counts) in census.totals {
            let entry = totals.entry(preset).or_insert([0; 4]);
            for (total, count) in entry.iter_mut().zip(counts) {
                *total += count;
            }
        }
        if census.measured {
            measured.insert(name.clone());
        }
        if census.violating {
            violating.insert(name.clone());
        }
        if census.single_painting {
            single_painting.insert(name.clone());
        }
        unpainted += usize::from(census.unpainted);
        no_tree += usize::from(census.no_tree);
        errors.extend(census.error);
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
        // Gate mode: a fixture in both runs may not get *worse*; one only in this run is new.
        // This baseline is a measurement, so improving the ground truth moves it legitimately,
        // and the failure message says so.
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
        "  real = omnidiff's mapping vs the painting; renderer = the human mapping rendered vs \
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
            "everything a reader sees (omnidiff's mapping rendered vs painting): ours -> theirs"
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

    // Split only by what the text *is*: the by-kind table below fragments a wide family.
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

    // Which *reason* mapped the node, per fixture: if the reasons separate the fixtures a
    // blanket punctuation rule improves from those it breaks, that is the predicate.
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

/// **Which identical leftovers the ground truth calls a copy.** Measures, before any engine code
/// exists, what a pass that attaches leftovers to N:M groups would gain and break.
///
/// A *candidate* is a node omnidiff deletes or inserts together with its whole subtree, whose
/// full hash equals a node on the same side that omnidiff pairs with an identical node (its
/// *twin*): the leftover a copy-attaching pass could add to its twin's pair as a group. Only
/// maximal candidates are rows; their descendants ride along and are counted in `size`.
///
/// Each row is labelled by the human's verdict on the candidate's root:
/// * `all_to_all` - a member of an all-to-all group: attaching it is right.
/// * `any_one_to_one` - a group leftover the human says is really gone or new: attaching it
///   turns a correct node into a mismatch.
/// * `removed` - a plain `Delete`/`Insert`, or inside a `*WithChildren` one: likewise.
/// * `matched` - the human pairs it one-to-one elsewhere: a mismatch either way.
/// * `ungraded`.
///
/// Then recall: every all-to-all member whose omnidiff partner is outside its group, and whether
/// a candidate covers it (is it, or an ancestor of it). Writes
/// `research/data/quality/nm_candidates.csv`.
///
/// `cargo test --release --lib --features test-fixtures nm_candidate_census -- --ignored
/// --nocapture`
#[test]
#[ignore]
#[cfg(feature = "test-fixtures")]
fn nm_candidate_census() -> Result<()> {
    use crate::diff::nodes::is_structurally_visible;
    use rustc_hash::{FxHashMap, FxHashSet};
    use std::collections::BTreeMap;

    const FUNCTION_WORDS: [&str; 6] = [
        "function",
        "method",
        "constructor",
        "lambda",
        "closure",
        "arrow",
    ];
    fn enclosing_function(node: Node) -> Option<usize> {
        let mut current = node.parent();
        while let Some(ancestor) = current {
            if FUNCTION_WORDS
                .iter()
                .any(|word| ancestor.kind().contains(word))
            {
                return Some(ancestor.id());
            }
            current = ancestor.parent();
        }
        None
    }
    fn all_nodes(root: Node) -> Vec<Node> {
        let mut nodes = Vec::new();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            nodes.push(node);
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
        }
        nodes
    }
    fn size_bucket(size: usize) -> &'static str {
        match size {
            1 => "1",
            2..=3 => "2-3",
            4..=7 => "4-7",
            8..=19 => "8-19",
            _ => "20+",
        }
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

    let header = [
        "fixture",
        "side",
        "label",
        "group_holds_twin",
        "kind",
        "size",
        "visible_nodes",
        "bytes",
        "same_side_count",
        "other_side_count",
        "twins",
        "sibling_of_twin",
        "parent_unmatched",
        "same_function_as_twin",
        "twin_distance_bytes",
        "row",
        "twin_row",
        "twin_displaced",
    ];
    let mut rows: Vec<Vec<String>> = Vec::new();
    // label -> size bucket -> (candidates, nodes)
    let mut by_label: BTreeMap<String, BTreeMap<&'static str, (usize, usize)>> = BTreeMap::new();
    let (mut missed_members, mut covered_members) = (0usize, 0usize);
    let (mut solved, mut skipped) = (0usize, 0usize);

    for name in &names {
        let Ok(mapping) = load(name) else {
            continue;
        };
        let Ok(pair) = crate::test::helper::handmade_test_code_pair(name) else {
            skipped += 1;
            continue;
        };
        let (before, after) = &*pair;
        let (Some(before_tree), Some(after_tree)) = (before.ast.as_ref(), after.ast.as_ref())
        else {
            skipped += 1;
            continue;
        };
        let config = crate::diff::HeuristicConfig::default();
        let diff = crate::diff::diff_code_with_config(before, after, &config);
        let Some(diff_ast) = diff.ast.as_ref() else {
            skipped += 1;
            continue;
        };
        solved += 1;
        let roots = [before_tree.root_node(), after_tree.root_node()];
        let metadata = [
            crate::code::metadata::metadata_of(before),
            crate::code::metadata::metadata_of(after),
        ];
        let sources = [before.contents.as_bytes(), after.contents.as_bytes()];
        let node_maps = [&diff_ast.before_node_map, &diff_ast.after_node_map];
        let caches = rebuild_caches_for_mapping(&mapping, roots[0], roots[1]);
        let groups = [&caches.before_group, &caches.after_group];
        let matches = [&caches.before_match, &caches.after_match];
        let removed = [&caches.before_removed, &caches.after_removed];
        let mut candidate_roots: [FxHashSet<usize>; 2] = Default::default();
        let node_cache = NodeCache::build(before, after);
        let node_caches = [&node_cache.before, &node_cache.after];

        for side in 0..2 {
            let other = 1 - side;
            let meta = &metadata[side];
            let node_map = node_maps[side];
            let nodes = all_nodes(roots[side]);
            let hash_of = |id: usize| meta.node_to_full_hash.get(&id).copied();

            // Post-order: children before parents, so a parent reads its children's verdicts.
            let mut fully_unmatched: FxHashMap<usize, bool> = FxHashMap::default();
            for node in nodes.iter().rev() {
                let own = node_map.get(&node.id()) == Some(&0);
                let mut cursor = node.walk();
                let children = node
                    .children(&mut cursor)
                    .all(|child| fully_unmatched.get(&child.id()) == Some(&true));
                fully_unmatched.insert(node.id(), own && children);
            }

            // Hash -> the nodes on this side omnidiff pairs with an identical node.
            let mut twins: FxHashMap<u64, Vec<Node>> = FxHashMap::default();
            for node in &nodes {
                let Some(&partner) = node_map.get(&node.id()) else {
                    continue;
                };
                if partner == 0 {
                    continue;
                }
                let (Some(own), Some(theirs)) = (
                    hash_of(node.id()),
                    metadata[other].node_to_full_hash.get(&partner).copied(),
                ) else {
                    continue;
                };
                if own == theirs {
                    twins.entry(own).or_default().push(*node);
                }
            }

            let is_candidate = |node: Node| {
                fully_unmatched.get(&node.id()) == Some(&true)
                    && hash_of(node.id()).is_some_and(|hash| twins.contains_key(&hash))
            };
            for node in &nodes {
                if !is_candidate(*node) || node.parent().is_some_and(is_candidate) {
                    continue;
                }
                candidate_roots[side].insert(node.id());
                let hash = hash_of(node.id()).expect("candidates have a hash");
                let node_twins = &twins[&hash];

                let in_removed_subtree = || {
                    if removed[side].contains_key(&node.id()) {
                        return true;
                    }
                    let mut current = node.parent();
                    while let Some(ancestor) = current {
                        if removed[side].get(&ancestor.id()) == Some(&true) {
                            return true;
                        }
                        current = ancestor.parent();
                    }
                    false
                };
                let group = groups[side]
                    .get(&node.id())
                    .map(|&idx| &mapping.groups[idx]);
                let label = match group {
                    Some(group) if group.pairing == GroupPairing::AllToAll => "all_to_all",
                    Some(_) => "any_one_to_one",
                    None if matches[side].contains_key(&node.id()) => "matched",
                    None if in_removed_subtree() => "removed",
                    None => "ungraded",
                };
                let group_holds_twin = groups[side].get(&node.id()).is_some_and(|idx| {
                    node_twins
                        .iter()
                        .any(|twin| groups[side].get(&twin.id()) == Some(idx))
                });

                let subtree = all_nodes(*node);
                let size = subtree.len();
                let visible_nodes = subtree
                    .iter()
                    .filter(|n| is_structurally_visible(**n, sources[side]))
                    .count();
                let same_side_count = meta.full_hash_to_node.get(&hash).map_or(0, Vec::len);
                let other_side_count = metadata[other]
                    .full_hash_to_node
                    .get(&hash)
                    .map_or(0, Vec::len);
                let sibling_of_twin = node_twins
                    .iter()
                    .any(|twin| twin.parent().map(|p| p.id()) == node.parent().map(|p| p.id()));
                let parent_unmatched = node
                    .parent()
                    .is_some_and(|parent| node_map.get(&parent.id()) == Some(&0));
                let function = enclosing_function(*node);
                let same_function_as_twin = function.is_some()
                    && node_twins
                        .iter()
                        .any(|twin| enclosing_function(*twin) == function);
                let nearest_twin = node_twins
                    .iter()
                    .min_by_key(|twin| twin.start_byte().abs_diff(node.start_byte()))
                    .expect("a candidate has a twin");
                let twin_distance_bytes = nearest_twin.start_byte().abs_diff(node.start_byte());
                // The edit moved or re-wrapped a twin: its parent does not pair with its partner's
                // parent. A twin left in place reads as unrelated code that happens to match.
                let twin_displaced = node_twins.iter().any(|twin| {
                    let partner = node_map[&twin.id()];
                    let partner_parent = node_caches[other]
                        .get(&partner)
                        .and_then(|partner| partner.parent())
                        .map(|parent| parent.id());
                    let twin_parent_partner = twin
                        .parent()
                        .and_then(|parent| node_map.get(&parent.id()).copied());
                    twin_parent_partner != partner_parent
                });

                let bucket = by_label
                    .entry(label.to_string())
                    .or_default()
                    .entry(size_bucket(size))
                    .or_default();
                bucket.0 += 1;
                bucket.1 += size;
                rows.push(vec![
                    name.clone(),
                    if side == 0 { "before" } else { "after" }.to_string(),
                    label.to_string(),
                    group_holds_twin.to_string(),
                    node.kind().to_string(),
                    size.to_string(),
                    visible_nodes.to_string(),
                    node.byte_range().len().to_string(),
                    same_side_count.to_string(),
                    other_side_count.to_string(),
                    node_twins.len().to_string(),
                    sibling_of_twin.to_string(),
                    parent_unmatched.to_string(),
                    same_function_as_twin.to_string(),
                    twin_distance_bytes.to_string(),
                    (node.start_position().row + 1).to_string(),
                    (nearest_twin.start_position().row + 1).to_string(),
                    twin_displaced.to_string(),
                ]);
            }
        }

        // Recall over the all-to-all members omnidiff leaves outside their group.
        let mut before_cache = PathCache::new();
        let mut after_cache = PathCache::new();
        for group in &mapping.groups {
            if group.pairing != GroupPairing::AllToAll {
                continue;
            }
            let before_members: FxHashSet<usize> = group
                .before_paths
                .iter()
                .filter_map(|path| before_cache.resolve(roots[0], &path_refs(path)).ok())
                .map(|node| node.id())
                .collect();
            let after_members: FxHashSet<usize> = group
                .after_paths
                .iter()
                .filter_map(|path| after_cache.resolve(roots[1], &path_refs(path)).ok())
                .map(|node| node.id())
                .collect();
            let members = [before_members, after_members];
            for side in 0..2 {
                for &member in &members[side] {
                    let partner = node_maps[side].get(&member).copied().unwrap_or(0);
                    if members[1 - side].contains(&partner) {
                        continue;
                    }
                    missed_members += 1;
                    let side_nodes = if side == 0 {
                        &node_cache.before
                    } else {
                        &node_cache.after
                    };
                    let node = side_nodes[&member];
                    let mut current = Some(node);
                    while let Some(n) = current {
                        if candidate_roots[side].contains(&n.id()) {
                            covered_members += 1;
                            break;
                        }
                        current = n.parent();
                    }
                }
            }
        }
    }

    let csv_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("research")
        .join("data")
        .join("quality")
        .join("nm_candidates.csv");
    let mut writer = csv::Writer::from_path(&csv_path)?;
    writer.write_record(header)?;
    for row in &rows {
        writer.write_record(row)?;
    }
    writer.flush()?;

    println!(
        "{} candidates over {solved} solved fixtures ({skipped} skipped) -> {}",
        rows.len(),
        csv_path.display()
    );
    println!("\ncandidates (nodes) by human label and subtree size:");
    for (label, buckets) in &by_label {
        let line: Vec<String> = ["1", "2-3", "4-7", "8-19", "20+"]
            .iter()
            .map(|bucket| {
                let (count, nodes) = buckets.get(bucket).copied().unwrap_or((0, 0));
                format!("{bucket}: {count} ({nodes})")
            })
            .collect();
        println!("  {label:<15} {}", line.join("  "));
    }
    println!(
        "\nall-to-all members omnidiff leaves outside their group: {missed_members}, \
         covered by a candidate: {covered_members}"
    );
    Ok(())
}

/// **Where omnidiff gives a node a second partner and silently drops the first**
/// ([`ASTDiff::node_map_disagreements`]), per fixture, with the passes that wrote each side of the
/// disagreement. Every fixture, solved or not.
///
/// `cargo test --release --lib --features test-fixtures node_map_disagreement_census -- --ignored
/// --nocapture`
#[test]
#[ignore]
fn node_map_disagreement_census() -> Result<()> {
    use std::collections::BTreeMap;

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

    let mut by_reasons: BTreeMap<String, usize> = BTreeMap::new();
    let (mut fixtures, mut total, mut diffed) = (0usize, 0usize, 0usize);
    for name in &names {
        let Ok(pair) = crate::test::helper::handmade_test_code_pair(name) else {
            continue;
        };
        let (before, after) = &*pair;
        let diff = crate::diff::diff_code(before, after);
        let Some(ast) = diff.ast.as_ref() else {
            continue;
        };
        diffed += 1;
        let found = ast.node_map_disagreements();
        if found.is_empty() {
            continue;
        }
        fixtures += 1;
        total += found.len();
        println!("{:>5}  {name}", found.len());
        for (b, a) in found {
            let reason_of = |key: (usize, usize)| {
                ast.mapping
                    .get(&key)
                    .map_or("-".to_string(), |m| format!("{:?}", m.reason))
            };
            let before_side = ast.before_node_map.get(&b).copied().unwrap_or(0);
            let after_side = ast.after_node_map.get(&a).copied().unwrap_or(0);
            *by_reasons
                .entry(format!(
                    "{} / {}",
                    reason_of((b, before_side)),
                    reason_of((after_side, a))
                ))
                .or_default() += 1;
        }
    }
    println!("\n{total} disagreements in {fixtures} of {diffed} fixtures");
    let mut reasons: Vec<_> = by_reasons.into_iter().collect();
    reasons.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    for (reasons, count) in reasons.iter().take(20) {
        println!("  {count:>5}  {reasons}");
    }
    Ok(())
}

/// EXPLORATORY: every position a matched pair pins whose occupant reads as one token and is
/// deleted beside an inserted one ([`invariants::pinned_removed_lexemes`]), classified by shape so
/// a rule over them can be scoped before it becomes an invariant. The shapes:
///
/// * `token-in-matched-wrapper` - the mapping pairs a node like C's `null` and deletes and
///   inserts its only token, `NULL` against `nullptr`.
/// * `wrapper/wrapper`, `leaf/wrapper`, `wrapper/leaf`, `leaf/leaf` - what occupies the position
///   on each side: a node around one token deleted or inserted whole, or a leaf (`!=` against
///   `==`, or a named leaf).
///
/// `inv18` marks what invariant 18 reports (named, one token each, different kinds);
/// `token-in-matched-wrapper` and `wrapper/wrapper` of one kind are what invariant 20 reports.
///
/// `cargo test --release --lib --features test-fixtures pinned_lexeme_census -- --ignored
/// --nocapture`
#[test]
#[ignore]
fn pinned_lexeme_census() -> Result<()> {
    use crate::test::helper::human_mapping::invariants::{
        TreeContext, field_arity, field_of, lexeme_token, pinned_removed_lexemes,
    };
    use std::collections::BTreeMap;

    let mut by_class: BTreeMap<String, (usize, std::collections::BTreeSet<String>)> =
        BTreeMap::new();
    let mut by_kinds: BTreeMap<String, usize> = BTreeMap::new();
    let (mut total, mut fixtures) = (0usize, 0usize);
    for (name, dir) in crate::test::helper::handmade_test_case_dirs()? {
        let Some((before, after)) = crate::test::helper::code_pair_from_dir(&dir)? else {
            continue;
        };
        let Ok(mapping) = load(&name) else { continue };
        let (Some(before_tree), Some(after_tree)) = (before.ast.as_ref(), after.ast.as_ref())
        else {
            continue;
        };
        let context = TreeContext::build(&mapping, before_tree.root_node(), after_tree.root_node());
        let found = pinned_removed_lexemes(&context, &before, &after);
        if found.is_empty() {
            continue;
        }
        fixtures += 1;
        for (b, a) in found {
            total += 1;
            let before_parent = b.parent().expect("a pinned occupant has a parent");
            let wraps =
                |node: Node| lexeme_token(node).is_some_and(|token| token.id() != node.id());
            let in_matched_wrapper = |node: Node| {
                node.parent()
                    .and_then(lexeme_token)
                    .is_some_and(|token| token.id() == node.id())
            };
            let shape = if in_matched_wrapper(b) && in_matched_wrapper(a) {
                "token-in-matched-wrapper".to_string()
            } else {
                let side = |node: Node| if wraps(node) { "wrapper" } else { "leaf" };
                format!("{}/{}", side(b), side(a))
            };
            let named = |node: Node| if node.is_named() { "named" } else { "unnamed" };
            let kinds = if b.kind() == a.kind() {
                "same-kind"
            } else {
                "cross-kind"
            };
            let inv18 =
                b.is_named() && a.is_named() && b.kind() != a.kind() && !(wraps(b) && wraps(a));
            let pinned = match field_of(before_parent, b) {
                Some(field) if field_arity(before_parent, &field) == 1 => format!("field:{field}"),
                _ => "elimination".to_string(),
            };
            let text = |node: Node, contents: &str| {
                let token = lexeme_token(node).unwrap_or(node);
                contents[token.byte_range()]
                    .chars()
                    .take(30)
                    .collect::<String>()
            };
            let class = format!(
                "{shape:<26} {:<7}/{:<7} {kinds:<10}{}",
                named(b),
                named(a),
                if inv18 { " inv18" } else { "" }
            );
            let entry = by_class.entry(class.clone()).or_default();
            entry.0 += 1;
            entry.1.insert(name.clone());
            *by_kinds
                .entry(format!("{} -> {}", b.kind(), a.kind()))
                .or_default() += 1;
            println!(
                "{name}\t{class}\t{}.{pinned}\tbefore {} {:?}\tafter {} {:?}",
                before_parent.kind(),
                b.start_position().row + 1,
                text(b, &before.contents),
                a.start_position().row + 1,
                text(a, &after.contents),
            );
        }
    }
    println!("\n{total} pinned deleted+inserted lexemes in {fixtures} fixtures\n");
    for (class, (count, names)) in &by_class {
        println!("{count:>6}  {:>4} fixtures  {class}", names.len());
    }
    println!();
    let mut kinds: Vec<_> = by_kinds.into_iter().collect();
    kinds.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    for (pair, count) in kinds.iter().take(60) {
        println!("{count:>6}  {pair}");
    }
    Ok(())
}

/// EXPLORATORY: candidate invariants between the tree mapping and the paintings, counted over the
/// corpus before any becomes a rule. Developed on `cpp-ladybird-refactor-variables-if-changes`,
/// painted after its mapping was written.
///
/// - `21` a leaf painted `Update` that the tree mapping deletes or inserts;
/// - `22` a leaf the tree mapping pairs, painted wholly `Delete` (before) or `Insert` (after);
/// - `23` a leaf inside a painted `Match` whose mapping partner lies outside that entry's spans on
///   the other side.
///
/// Each is split by `named` (text is not its own kind) against punctuation, and by partner `same`
/// (identical) against `paired` (edited). `FIXTURES=a,b` limits the run and prints every hit.
///
/// `cargo test --release --lib --features test-fixtures mapping_painting_census -- --ignored
/// --nocapture`
#[test]
#[ignore]
fn mapping_painting_census() -> Result<()> {
    use crate::test::helper::human_mapping::invariants::{LeafStatus, TreeContext, painted_labels};
    use std::collections::BTreeMap;

    let wanted = std::env::var("FIXTURES").unwrap_or_default();
    let wanted: Vec<&str> = wanted
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let verbose = !wanted.is_empty();
    let mut totals: BTreeMap<String, (usize, std::collections::BTreeSet<String>)> = BTreeMap::new();
    let mut samples: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (name, dir) in crate::test::helper::handmade_test_case_dirs()? {
        if !wanted.is_empty() && !wanted.contains(&name.as_str()) {
            continue;
        }
        let Some((before, after)) = crate::test::helper::code_pair_from_dir(&dir)? else {
            continue;
        };
        let Ok(mapping) = load(&name) else { continue };
        if mapping.text_mappings.is_empty() {
            continue;
        }
        let (Some(bt), Some(at)) = (before.ast.as_ref(), after.ast.as_ref()) else {
            continue;
        };
        let context = TreeContext::build(&mapping, bt.root_node(), at.root_node());
        let codes = [&before, &after];
        for named in &mapping.text_mappings {
            let labels = painted_labels(named, &before, &after)?;
            let whole = |side: usize, leaf: Node| -> Option<Option<TextLabel>> {
                let slice = labels[side].get(leaf.byte_range())?;
                let first = *slice.first()?;
                slice.iter().all(|l| *l == first).then_some(first)
            };
            let mut hit = |rule: &str, side: usize, leaf: Node, detail: String| {
                let named_leaf = codes[side].contents.get(leaf.byte_range()) != Some(leaf.kind());
                let key = format!("{rule} {}", if named_leaf { "named" } else { "punct" });
                let entry = totals.entry(key.clone()).or_default();
                entry.0 += 1;
                entry.1.insert(name.clone());
                let line = format!(
                    "{name} [{}] {} row {} `{}` {detail}",
                    named.name,
                    if side == 0 { "before" } else { "after" },
                    leaf.start_position().row + 1,
                    codes[side].contents[leaf.byte_range()]
                        .chars()
                        .take(30)
                        .collect::<String>()
                );
                let list = samples.entry(key).or_default();
                if verbose || list.len() < 12 {
                    list.push(line);
                }
            };
            let mut exact_update_spans: [std::collections::HashSet<(usize, usize)>; 2] =
                Default::default();
            for entry in &named.mapping.entries {
                if entry.operation != HumanTextOperation::Match {
                    continue;
                }
                for (side, spans) in [(0, &entry.before), (1, &entry.after)] {
                    for span in spans {
                        if let (Some(s), Some(e)) = (
                            byte_offset(&codes[side].contents, span.start_row, span.start_column),
                            byte_offset(&codes[side].contents, span.end_row, span.end_column),
                        ) {
                            exact_update_spans[side].insert((s, e));
                        }
                    }
                }
            }
            for side in 0..2 {
                for &leaf in &context.leaves[side] {
                    if codes[side].contents[leaf.byte_range()].trim().is_empty() {
                        continue;
                    }
                    let status = context.status(leaf, side);
                    let label = whole(side, leaf);
                    // 21: only where an update span is exactly this token, not a byte of a larger
                    // edited span (`width: Double` -> `val width: Double`).
                    if status == LeafStatus::Removed
                        && label == Some(Some(TextLabel::Update))
                        && exact_update_spans[side].contains(&(leaf.start_byte(), leaf.end_byte()))
                    {
                        hit("21 update-on-removed", side, leaf, String::new());
                    }
                    // 22
                    let removed_label = if side == 0 {
                        TextLabel::Delete
                    } else {
                        TextLabel::Insert
                    };
                    if label == Some(Some(removed_label)) {
                        match status {
                            LeafStatus::Same(p) => hit(
                                "22 paired-painted-removed same",
                                side,
                                leaf,
                                format!("partner row {}", p.start_position().row + 1),
                            ),
                            LeafStatus::Paired(p) => hit(
                                "22 paired-painted-removed paired",
                                side,
                                leaf,
                                format!(
                                    "partner row {} `{}`",
                                    p.start_position().row + 1,
                                    codes[1 - side].contents[p.byte_range()]
                                        .chars()
                                        .take(30)
                                        .collect::<String>()
                                ),
                            ),
                            _ => {}
                        }
                    }
                }
            }
            // 23
            let to_bytes = |side: usize, span: &HumanTextSpan| -> Option<(usize, usize)> {
                Some((
                    byte_offset(&codes[side].contents, span.start_row, span.start_column)?,
                    byte_offset(&codes[side].contents, span.end_row, span.end_column)?,
                ))
            };
            for entry in &named.mapping.entries {
                if entry.operation != HumanTextOperation::Match {
                    continue;
                }
                let sides: [Vec<(usize, usize)>; 2] = [
                    entry.before.iter().filter_map(|s| to_bytes(0, s)).collect(),
                    entry.after.iter().filter_map(|s| to_bytes(1, s)).collect(),
                ];
                for side in 0..2 {
                    for &leaf in &context.leaves[side] {
                        let r = leaf.byte_range();
                        if codes[side].contents[r.clone()].trim().is_empty()
                            || !sides[side].iter().any(|&(s, e)| s <= r.start && r.end <= e)
                        {
                            continue;
                        }
                        let partner = match context.status(leaf, side) {
                            LeafStatus::Same(p) | LeafStatus::Paired(p) => p,
                            _ => continue,
                        };
                        // Overlap, not containment: Minimal paints part of a token (`Linked` inserted,
                        // `HashMap` matched).
                        let pr = partner.byte_range();
                        if !sides[1 - side]
                            .iter()
                            .any(|&(s, e)| s < pr.end && pr.start < e)
                        {
                            hit(
                                "23 match-partner-outside",
                                side,
                                leaf,
                                format!("partner row {}", partner.start_position().row + 1),
                            );
                        }
                    }
                }
            }
        }
    }
    println!();
    for (key, (count, fixtures)) in &totals {
        println!("{key:45} {count:6} hits in {:4} fixtures", fixtures.len());
    }
    for (key, list) in &samples {
        println!("\n== {key}");
        for line in list {
            println!("  {line}");
        }
    }
    Ok(())
}

/// EXPLORATORY: every unnamed token in a position a matched pair pins ([`invariants::
/// pinned_counterpart`]) whose counterpart reads differently - `!=` against `==`, `&` against `*` -
/// and what the ground truth does with the two: pairs them, deletes one and inserts the other, or
/// something else. Measures whether "an unnamed token in a pinned slot is paired" could be an
/// invariant, and how consistent the ground truth already is about it.
///
/// `FIXTURES=a,b` prints every case; otherwise a summary and samples.
/// `cargo test --release --lib --features test-fixtures unnamed_slot_census -- --ignored
/// --nocapture`
#[test]
#[ignore]
fn unnamed_slot_census() -> Result<()> {
    use crate::test::helper::human_mapping::invariants::{
        LeafStatus, TreeContext, field_arity, field_of, pinned_counterpart,
    };
    use std::collections::BTreeMap;

    let wanted = std::env::var("FIXTURES").unwrap_or_default();
    let wanted: Vec<&str> = wanted
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let mut by_verdict: BTreeMap<String, (usize, std::collections::BTreeSet<String>)> =
        BTreeMap::new();
    let mut by_pair: BTreeMap<(String, String), BTreeMap<String, usize>> = BTreeMap::new();
    let mut samples: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (name, dir) in crate::test::helper::handmade_test_case_dirs()? {
        if !wanted.is_empty() && !wanted.contains(&name.as_str()) {
            continue;
        }
        let Some((before, after)) = crate::test::helper::code_pair_from_dir(&dir)? else {
            continue;
        };
        let Ok(mapping) = load(&name) else { continue };
        let (Some(bt), Some(at)) = (before.ast.as_ref(), after.ast.as_ref()) else {
            continue;
        };
        let context = TreeContext::build(&mapping, bt.root_node(), at.root_node());
        for &leaf in &context.leaves[0] {
            let text = &before.contents[leaf.byte_range()];
            if leaf.is_named() || text.trim().is_empty() {
                continue;
            }
            let Some(parent) = leaf.parent() else {
                continue;
            };
            let Some(after_parent) = context.partner(parent, 0) else {
                continue;
            };
            let Some(counterpart) = pinned_counterpart(&context, leaf, parent, after_parent) else {
                continue;
            };
            let other = &after.contents[counterpart.byte_range()];
            if counterpart.is_named() || counterpart.child_count() != 0 || other == text {
                continue;
            }
            let in_error = {
                let mut current = Some(parent);
                let mut found = false;
                while let Some(node) = current {
                    if node.is_error() {
                        found = true;
                        break;
                    }
                    current = node.parent();
                }
                found
            };
            let verdict = match (context.status(leaf, 0), context.status(counterpart, 1)) {
                (LeafStatus::Paired(p), _) if p.id() == counterpart.id() => "paired",
                (LeafStatus::Removed, LeafStatus::Removed) => "deleted+inserted",
                (LeafStatus::Undecided, _) | (_, LeafStatus::Undecided) => "undecided",
                _ => "other",
            };
            let pinned = match field_of(parent, leaf) {
                Some(field) if field_arity(parent, &field) == 1 => format!("field:{field}"),
                _ => "elimination".to_string(),
            };
            let parents = if parent.kind() == after_parent.kind() {
                "same-parent-kind"
            } else {
                "cross-parent-kind"
            };
            let key = format!(
                "{verdict:<17} {parents:<17}{}",
                if in_error { " in-ERROR" } else { "" }
            );
            let entry = by_verdict.entry(key.clone()).or_default();
            entry.0 += 1;
            entry.1.insert(name.clone());
            *by_pair
                .entry((text.to_string(), other.to_string()))
                .or_default()
                .entry(verdict.to_string())
                .or_default() += 1;
            let line = format!(
                "{name}  row {} `{text}` -> `{other}`  in {}->{} ({pinned})",
                leaf.start_position().row + 1,
                parent.kind(),
                after_parent.kind()
            );
            let list = samples.entry(key).or_default();
            if !wanted.is_empty() || list.len() < 25 {
                list.push(line);
            }
        }
    }
    println!();
    for (key, (count, fixtures)) in &by_verdict {
        println!("{count:6} in {:4} fixtures  {key}", fixtures.len());
    }
    println!("\nby token pair (verdict counts):");
    let mut pairs: Vec<_> = by_pair.into_iter().collect();
    pairs.sort_by_key(|(_, v)| std::cmp::Reverse(v.values().sum::<usize>()));
    for ((b, a), verdicts) in pairs.iter().take(40) {
        println!("  `{b}` -> `{a}`  {verdicts:?}");
    }
    for (key, list) in &samples {
        println!("\n== {key}");
        for line in list {
            println!("  {line}");
        }
    }
    Ok(())
}

/// EXPLORATORY: every all-to-all copy in the ground truth, one line per *copy* - the solver writes
/// one group per node of a copied subtree, so groups nested under another group's member are
/// folded into it. For each member: its row, whether the human pairs its parent with the parent of
/// a member on the other side ("in place"), and what omnidiff does with it today.
///
/// `cargo test --release --lib --features test-fixtures nm_copy_census -- --ignored --nocapture`
#[test]
#[ignore]
fn nm_copy_census() -> Result<()> {
    use crate::test::helper::node_for_path;

    let mut copies = 0usize;
    for (name, dir) in crate::test::helper::handmade_test_case_dirs()? {
        let Ok(mapping) = load(&name) else { continue };
        let groups: Vec<&MultiMapGroup> = mapping
            .groups
            .iter()
            .filter(|g| g.pairing == GroupPairing::AllToAll)
            .collect();
        if groups.is_empty() {
            continue;
        }
        let Some((before, after)) = crate::test::helper::code_pair_from_dir(&dir)? else {
            continue;
        };
        let (Some(bt), Some(at)) = (before.ast.as_ref(), after.ast.as_ref()) else {
            continue;
        };
        let (broot, aroot) = (bt.root_node(), at.root_node());
        let resolve = |root: Node<'_>, paths: &[Vec<String>]| -> Vec<Option<usize>> {
            paths
                .iter()
                .map(|p| node_for_path(root, &path_refs(p)).ok().map(|n| n.id()))
                .collect()
        };
        // Every member id per side, to fold nested groups.
        let mut all_before = std::collections::HashSet::new();
        let mut all_after = std::collections::HashSet::new();
        for g in &groups {
            all_before.extend(resolve(broot, &g.before_paths).into_iter().flatten());
            all_after.extend(resolve(aroot, &g.after_paths).into_iter().flatten());
        }
        let human: std::collections::HashMap<Vec<String>, Vec<String>> = mapping
            .entries
            .iter()
            .filter_map(|e| Some((e.before_path.clone()?, e.after_path.clone()?)))
            .collect();
        let diff = crate::diff::diff_code(&before, &after);
        let ast = diff.ast.as_ref();

        for g in &groups {
            let bnodes: Vec<Node> = g
                .before_paths
                .iter()
                .filter_map(|p| node_for_path(broot, &path_refs(p)).ok())
                .collect();
            let anodes: Vec<Node> = g
                .after_paths
                .iter()
                .filter_map(|p| node_for_path(aroot, &path_refs(p)).ok())
                .collect();
            let nested = |n: &Node, set: &std::collections::HashSet<usize>| {
                let mut cur = n.parent();
                while let Some(p) = cur {
                    if set.contains(&p.id()) {
                        return true;
                    }
                    cur = p.parent();
                }
                false
            };
            if bnodes.iter().any(|n| nested(n, &all_before))
                || anodes.iter().any(|n| nested(n, &all_after))
            {
                continue;
            }
            copies += 1;
            let text = |n: &Node, code: &crate::code::Code| {
                code.contents[n.byte_range()]
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .chars()
                    .take(50)
                    .collect::<String>()
            };
            let parent_path = |p: &Vec<String>| p[..p.len().saturating_sub(1)].to_vec();
            let in_place_before = |p: &Vec<String>| {
                human
                    .get(&parent_path(p))
                    .is_some_and(|ap| g.after_paths.iter().any(|q| &parent_path(q) == ap))
            };
            let in_place_after = |q: &Vec<String>| {
                g.before_paths
                    .iter()
                    .any(|p| human.get(&parent_path(p)) == Some(&parent_path(q)))
            };
            let ours = |id: usize, before_side: bool| -> String {
                let Some(ast) = ast else { return "-".into() };
                let partner = if before_side {
                    ast.before_node_map.get(&id)
                } else {
                    ast.after_node_map.get(&id)
                };
                match partner {
                    Some(0) | None => "gone".into(),
                    Some(p)
                        if (before_side && anodes.iter().any(|a| a.id() == *p))
                            || (!before_side && bnodes.iter().any(|b| b.id() == *p)) =>
                    {
                        "in-group".into()
                    }
                    Some(_) => "elsewhere".into(),
                }
            };
            let members = |nodes: &[Node], paths: &[Vec<String>], before_side: bool| {
                nodes
                    .iter()
                    .zip(paths)
                    .map(|(n, p)| {
                        let place = if before_side {
                            in_place_before(p)
                        } else {
                            in_place_after(p)
                        };
                        format!(
                            "r{}{}:{}",
                            n.start_position().row + 1,
                            if place { "=" } else { "*" },
                            ours(n.id(), before_side)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            };
            let (kind, sample) = match (bnodes.first(), anodes.first()) {
                (Some(n), _) => (n.kind(), text(n, &before)),
                (None, Some(n)) => (n.kind(), text(n, &after)),
                _ => ("?", String::new()),
            };
            println!(
                "{name}\t{}:{}\t{:?}\t{kind}\t{sample:?}\tbefore[{}]\tafter[{}]",
                bnodes.len(),
                anodes.len(),
                g.operation,
                members(&bnodes, &g.before_paths, true),
                members(&anodes, &g.after_paths, false),
            );
        }
    }
    println!("\n{copies} copies (all-to-all groups not nested in another)");
    Ok(())
}
