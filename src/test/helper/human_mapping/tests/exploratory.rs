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
// Three write a live artifact and are wired into a Make target or a plan -
// `painting_failure_census` (`make update-painting-attribution`), `mismatch_census` and
// `cross_fixture_convention_census`. The other five take a fixture name
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
///
/// `MAPPING=human` renders the fixture's human tree mapping instead of codediff's, with
/// codediff's reasons borrowed exactly as `painting_failure_census` does - the runs behind that
/// census's `renderer_bytes` column, i.e. what only a rendering change can fix. Either way the
/// comparison is against the closest of the preset's candidate paintings, as the census scores it.
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
        .with_context(|| format!("codediff produced no AST diff for '{name}'"))?;
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
    // The candidate is chosen against codediff's own rendering even under `MAPPING=human`, as
    // the census chooses it, so the two report runs against the same painting.
    let chooser = if human { render(real) } else { ours.clone() };

    // The closest candidate, as the census scores it.
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
        if human { "human" } else { "codediff" },
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

/// EXPLORATORY: the mismatches of one fixture, spelled out - the per-fixture counterpart of
/// [`mismatch_census`].
///
/// The census answers "what shapes are there, corpus-wide" and writes a CSV to group by. This
/// answers the question that follows it: the census says this fixture disagrees twice on `||`
/// under `binary_expression`, so *which* `||`, reading what, next to what. It prints the mismatch
/// message unmodified - `compute_mismatches_*` already writes the human's operation, both paths
/// and the partner codediff chose - and adds the one thing the message cannot carry, the node's
/// own bytes and where they are, since a mismatch on a `,` is unreadable without them.
///
/// Invisible mismatches are printed too, under their own heading: the goals are stated in visible
/// nodes, but when a visible one looks arbitrary its explanation is often an invisible sibling.
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

    // The message names the partner codediff chose by *kind* only ("but it mapped to ||"), which
    // is exactly the word that repeats in the cases worth reading. Its position is what settles
    // which `||` that was, so it is looked up here rather than inferred from the message.
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
                "{side} {}  -> codediff chose {partner}",
                locate(code, mismatch.node_id)
            );
            eprintln!("    {}", mismatch.message);
        }
    }
    Ok(())
}

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
///
/// `cargo test --release --lib --features test-fixtures mismatch_census -- --ignored --nocapture`
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

/// **Where two fixtures' paintings answer the same question differently.**
///
/// Every rule in [`crate::test::helper::human_mapping::invariants`] is *intra*-fixture: it asks
/// whether one fixture's mapping and paintings agree with each other. None of them compares two
/// fixtures, so a pair whose paintings imply opposite conventions for the same shape is invisible
/// to all seventeen - and a corpus that disagrees with itself across fixtures grades a renderer
/// against a coin flip, however self-consistent each fixture is on its own.
///
/// The population is the one place the two fixtures can be made comparable: a **leaf the human's
/// own tree mapping pairs with a leaf that reads the same** ([`LeafStatus::Same`]). No matcher is
/// involved and no judgement about what the change *is* - the human has already said this text
/// survived. All that is left is where it ended up, which is geometry, and whether the painting
/// colours it, which is the convention. Two fixtures that answer differently for the same geometry
/// are the finding.
///
/// Three booleans describe the geometry, and they are deliberately facts about the *file* rather
/// than about any rule in `diff::text` - a class defined by the renderer's own predicates could
/// only ever re-discover the renderer:
///
/// * `row_moved` - the leaf is on a different row index than its partner.
/// * `column_moved` - it starts at a different column.
/// * `row_edited` - its whole row reads differently on the two sides. False means the row is
///   byte-identical and whatever moved the leaf happened elsewhere in the file.
///
/// A fourth fact separates the two things `row_moved` alone conflates - a leaf pushed down by an
/// insertion somewhere above it, and a leaf that genuinely relocated. `drift_matches_neighbours`
/// asks whether the leaf's row delta is the one its immediate neighbours also carry: text that
/// insertions above it pushed down moves by the same amount as everything around it, and a block
/// that moved does not. The same neighbour evidence [`crate::diff::solve_orphaned_leaves`] and
/// `solve_leaf_neighbour_agreement` both key on, asked of rows.
///
/// The verdict is `clean` (no byte of the leaf is painted), `painted` (all of them are, and under
/// which label), or `partial`. `partial` is reported and never counted as either: a leaf half
/// inside a larger painted region is the painting's chunking talking, not its verdict on this
/// leaf.
///
/// Scoped to visible leaves, because a painting cannot colour whitespace it does not reach, and to
/// the two named presets, because `Minimal` and `Full` are *specified* to disagree - a split
/// between them is the design, and only a split *within* one of them is a contradiction.
///
/// **The CSV holds the exceptions, not the population.** One row per leaf whose verdict is not
/// `clean`, carrying its class's own `clean` count for that fixture and preset as the denominator
/// beside it. The population is 3.3 million leaves and 261MB of it; the exceptions are four
/// thousand, they are what anyone acts on, and each one arrives with the number it is an exception
/// to.
///
/// `cargo test --release --lib --features test-fixtures cross_fixture_convention_census --
/// --ignored --nocapture`
#[test]
#[ignore]
// The CSV half needs the `csv` crate, which is only linked under `test-fixtures` - the same gate
// `mismatch_census` above carries, for the same reason.
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

    // Held back until the end so each row can carry its own class's `clean` count for that fixture
    // and preset - the denominator is what makes an exception readable, and it is not known until
    // the class has been walked.
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

        // Every `Same` leaf's row delta, per side, in document order - the neighbourhood
        // `drift_matches_neighbours` reads. Built once per fixture rather than per painting.
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

                    // The drift its neighbours carry. A leaf with no `Same` neighbour on either
                    // side has nothing to agree with and reports `false`, which keeps the column
                    // meaning "verified to move with its surroundings" rather than "not known to
                    // disagree".
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

    // A fixture "says clean" for a class when every one of its leaves in that class is unpainted,
    // and "says painted" when every one is painted. A fixture that does both is itself the
    // contradiction and is counted apart - no cross-fixture comparison is needed to condemn it.
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
        let mut human_ast = match as_ast_diff_for_mapping(&mapping, before, after) {
            Ok(diff) => diff,
            Err(e) => {
                errors.push(format!("{name}: human mapping -> ASTDiff: {e:#}"));
                continue;
            }
        };
        // The human format records no `ASTMappingReason`, but the renderer reads one:
        // `identical_or_move` keeps a relocated node unpainted when the matcher verified the
        // relocation is a pure reindent or heritage-clause shift. Untagged, every human pair loses
        // those overrides, and the `ideal` column blames the renderer for `Move`s codediff's own
        // output never paints. Every pair both mappings make borrows codediff's reason, which is
        // a verified fact about that pair, not a choice of the matcher's.
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
                                    // Cut back to a character boundary, not to byte 40:
                                    // `xml-libreoffice-unicode`'s runs are CJK, where every
                                    // character is three bytes and byte 40 lands inside one.
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
