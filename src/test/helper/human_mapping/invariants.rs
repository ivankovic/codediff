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
//! Properties every fixture's *ground truth* should hold, checked against the ground truth alone.
//!
//! Nothing here runs `diff_code` or reads codediff's output. `assert_matches_human_mapping` and
//! `assert_matches_human_painting_within_limit` both ask "is codediff right?", and both answer it
//! against data whose own internal consistency nothing checks - a painting that ends a highlight
//! in the middle of a run of spaces, or paints an opening brace and not its closing one, grades
//! codediff against a claim its author would not defend if it were pointed out. These three
//! invariants are that missing half: they can fail only because the hand-authored data disagrees
//! with itself.
//!
//! * [`rows_end_on_visible_characters`] - no painted run on a row may end on whitespace.
//! * [`full_painting_covers_minimal`] - every byte painted under `Minimal` is painted under `Full`.
//! * [`delimiter_pairs_agree`] - a painted or mapped bracket and its partner carry one verdict.
//!
//! **Per fixture, not corpus-wide.** These are wired in as a third `invariants()` test in each
//! `src/test/fixtures/**` file, next to that fixture's `mapping()` and `painting()`, so a fixture
//! that violates one records how badly in its own file - the same "clamp at the observed number,
//! with a dated note saying what it is" convention the other two already use, and the same reason:
//! a corpus-wide test would have to carry a list of exempt fixture names, which is the one place
//! nobody looks when they edit a fixture.

use anyhow::Result;
use tree_sitter::Node;

use super::{
    Caches, HumanTextSpan, MarkKind, NamedTextMapping, NodeStatus, TextLabel, label_bytes, load,
    paintings_for_mode, rebuild_caches_for_mapping, status_after, status_before,
};
use crate::code::Code;
use crate::diff::text::RenderOptions;

/// Every way `name`'s ground truth contradicts itself, one human-readable line each, in a stable
/// order. Empty is a pass.
pub fn ground_truth_invariant_violations(name: &str) -> Result<Vec<String>> {
    let (before, after) = &*crate::test::helper::handmade_test_code_pair(name)?;
    ground_truth_invariant_violations_for(&load(name)?, before, after)
}

/// [`ground_truth_invariant_violations`] over an already-loaded mapping and code pair.
pub fn ground_truth_invariant_violations_for(
    mapping: &super::HumanMapping,
    before: &Code,
    after: &Code,
) -> Result<Vec<String>> {
    let mut violations = Vec::new();

    // One byte-label vector per painting per side, built once: all three checks that read the
    // painting read it through exactly the projection the scorer does (`label_bytes`), so an
    // invariant can never fire on a byte no comparison would ever look at.
    let mut paintings: Vec<(&str, [Vec<Option<TextLabel>>; 2])> = Vec::new();
    for named in &mapping.text_mappings {
        paintings.push((named.name.as_str(), painted_labels(named, before, after)?));
    }

    for (name, labels) in &paintings {
        violations.extend(rows_end_on_visible_characters(name, labels, before, after));
    }
    violations.extend(full_painting_covers_minimal(mapping, before, after)?);
    violations.extend(delimiter_pairs_agree(mapping, before, after, &paintings));

    Ok(violations)
}

/// One painting reduced to per-byte labels, `[before, after]`.
fn painted_labels(
    named: &NamedTextMapping,
    before: &Code,
    after: &Code,
) -> Result<[Vec<Option<TextLabel>>; 2]> {
    let mut spans: [Vec<(HumanTextSpan, TextLabel)>; 2] = [Vec::new(), Vec::new()];
    for entry in &named.mapping.entries {
        let label = TextLabel::from_verdict(entry.verdict(&before.contents, &after.contents)?);
        for span in &entry.before {
            spans[0].push((*span, label));
        }
        for span in &entry.after {
            spans[1].push((*span, label));
        }
    }
    Ok([
        label_bytes(&before.contents, &spans[0]),
        label_bytes(&after.contents, &spans[1]),
    ])
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// Invariant 1: a painted row ends on a visible character
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// The last painted character on any row must be one a reader can see.
///
/// Highlighted trailing whitespace is a stripe of colour hanging off the end of a line, pointing
/// at nothing - it says "something happened here" about a region with no content to have happened
/// to. The rendering side of this was fixed in the product on 2026-08-31 (`columns_on_row` bounds
/// every painted row to that row's own content), so what remains is the data: a span whose
/// `end_column` sits a few columns past the last real character still *claims* that whitespace,
/// and every consumer that reads bytes rather than rendering them - the scorer included - honours
/// the claim.
///
/// **A row with nothing visible on it is skipped.** On a blank-but-indented line there is no
/// character that could legally end a painted run, so the rule has nothing to say rather than
/// condemning every possible painting of it. That is a real limit and not a loophole: interior
/// whitespace lives in the gaps between AST nodes where no painting can reach, which is why a
/// whitespace-only commit renders as nothing at all (measured 2026-09-05).
///
/// Checked against the *projected* labels, not the raw spans, deliberately. `label_bytes` already
/// drops the `\n` a span ending at column 0 of the next row swallows, so a span written that way -
/// 28 of them in the corpus, an artifact of how `from_treesitter_range` normalises a range ending
/// at end of row - is not reported: nothing downstream paints that newline. What is reported is
/// what a reader would actually see coloured.
fn rows_end_on_visible_characters(
    painting: &str,
    labels: &[Vec<Option<TextLabel>>; 2],
    before: &Code,
    after: &Code,
) -> Vec<String> {
    let mut violations = Vec::new();
    for (side, contents) in [(0usize, &before.contents), (1usize, &after.contents)] {
        let mut offset = 0usize;
        for (row, line) in contents.split('\n').enumerate() {
            let start = offset;
            offset += line.len() + 1;
            if !line.chars().any(|c| !c.is_whitespace()) {
                continue;
            }
            let Some(last) = (0..line.len())
                .rev()
                .find(|&i| labels[side][start + i].is_some())
            else {
                continue;
            };
            // `last` can land inside a multi-byte character; the character it belongs to is the
            // one a reader sees at the end of the painted run.
            let boundary = (0..=last)
                .rev()
                .find(|&i| line.is_char_boundary(i))
                .unwrap_or(0);
            let Some(character) = line[boundary..].chars().next() else {
                continue;
            };
            if character.is_whitespace() {
                violations.push(format!(
                    "painting '{painting}' {} row {} ends its last painted run on {character:?}, \
                     not on a visible character: {line:?}",
                    side_name(side),
                    row + 1,
                ));
            }
        }
    }
    violations
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// Invariant 2: Full paints everything Minimal paints
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// Whatever `Minimal` paints, `Full` paints too.
///
/// The two presets differ in *how much* of an edit to highlight, not in *what* the edit is:
/// `Minimal` is the tightest defensible reading and `Full` the most generous, so a byte the tight
/// reading calls changed cannot be one the generous reading calls untouched. The colour may
/// differ - `Full` routinely widens a `Move` into the `Update` that contains it - so only
/// painted-or-not is compared, which is exactly the part the two presets are not free to disagree
/// about.
///
/// A fixture painted once asserts its rendering is unambiguous and both presets answer to that one
/// painting, so there is nothing to compare and nothing is reported. Where a preset carries
/// several alternatives (`Minimal (left)`, `Minimal (right)` - two equally correct readings of the
/// same edit), each `Minimal` alternative need only be covered by *some* `Full` alternative: the
/// alternatives are a disjunction, and holding one arm of it to another arm's `Full` would be
/// asserting a pairing the painter never claimed.
fn full_painting_covers_minimal(
    mapping: &super::HumanMapping,
    before: &Code,
    after: &Code,
) -> Result<Vec<String>> {
    let (Ok(minimal), Ok(full)) = (
        paintings_for_mode(mapping, RenderOptions::MINIMAL),
        paintings_for_mode(mapping, RenderOptions::FULL),
    ) else {
        return Ok(Vec::new());
    };
    // A single painting answers for both presets - the same `NamedTextMapping` on both sides of
    // the comparison, which is trivially its own superset.
    if minimal
        .iter()
        .all(|m| full.iter().any(|f| std::ptr::eq(*m, *f)))
    {
        return Ok(Vec::new());
    }

    let mut violations = Vec::new();
    for minimal in &minimal {
        let minimal_labels = painted_labels(minimal, before, after)?;
        let mut closest: Option<(usize, &str)> = None;
        for full in &full {
            let full_labels = painted_labels(full, before, after)?;
            let missing: usize = (0..2)
                .map(|side| {
                    minimal_labels[side]
                        .iter()
                        .zip(full_labels[side].iter())
                        .filter(|(minimal, full)| minimal.is_some() && full.is_none())
                        .count()
                })
                .sum();
            if closest.is_none_or(|(best, _)| missing < best) {
                closest = Some((missing, full.name.as_str()));
            }
        }
        if let Some((missing, full)) = closest
            && missing > 0
        {
            violations.push(format!(
                "painting '{}' paints {missing} byte(s) that '{full}' leaves unpainted - a Full \
                 painting must cover everything its Minimal counterpart covers",
                minimal.name,
            ));
        }
    }
    Ok(violations)
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// Invariant 3: a delimiter and its partner carry one verdict
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// The delimiters this pairs, opener to the closers that may end it.
///
/// Taken from the corpus rather than from a grammar: every anonymous leaf whose kind *is* its own
/// text, over all 628 fixtures, is one of these twelve. The `kind == text` test is what keeps a
/// `)` inside a string literal or a comment out - those are `string_content`/`text`/`word` leaves
/// that merely happen to read `)`, and there are several hundred of them.
///
/// `<` takes `/>` as well as `>` because a self-closing tag ends with one; `<` with no closer at
/// all among its parent's children (`a < b`, and every other comparison) simply never forms a
/// pair, so nothing is asserted about it.
const DELIMITERS: &[(&str, &[&str])] = &[
    ("(", &[")"]),
    ("[", &["]"]),
    ("{", &["}"]),
    ("<", &[">", "/>"]),
    ("</", &[">"]),
    ("<?", &["?>"]),
];

fn closers_of(kind: &str) -> Option<&'static [&'static str]> {
    DELIMITERS
        .iter()
        .find(|(open, _)| *open == kind)
        .map(|(_, close)| *close)
}

fn is_closer(kind: &str) -> bool {
    DELIMITERS
        .iter()
        .any(|(_, closers)| closers.contains(&kind))
}

/// Paint or mark one delimiter and you have said something about the construct it opens, so its
/// partner must say the same thing.
///
/// Nobody deletes a `(` and keeps its `)`; a reader shown one highlighted brace and not the other
/// is being told the block half-changed, which is not a thing that can happen. This is the one
/// invariant that reads the tree mapping as well as the painting, because the tree mapping can
/// express the same contradiction: a matched `{` whose `}` is marked inserted.
///
/// **Pairing is structural, within one parent's direct children.** A stack over those children in
/// order pairs each closer with the nearest unclosed opener that admits it, so a parent holding
/// two pairs pairs them correctly and an unmatched delimiter is left out rather than paired with
/// something arbitrary. Nothing crosses a parent boundary, which is what makes this safe on the
/// 111 corpus sides that parse with errors somewhere.
///
/// **A pair with a parse error between its halves is skipped**, as the request that motivated this
/// asks: tree-sitter's recovery invents structure, and a `{` it paired with a `}` three functions
/// away is not a pair a human ever saw. 6819 of the corpus's ~42k pairs are skipped this way.
///
/// The mapping half compares **status only** - deleted, inserted, or matched - and not the derived
/// move flag or the recorded operation. `moved` is `before_path != after_path`, a consequence of
/// where a node landed rather than a judgement anyone entered, and a `{` whose path shifted while
/// its `}`'s did not is a numbering artifact, not a claim that half a block moved. A node the
/// mapping says nothing about (`Unmarked`) makes no claim to contradict, so a pair with an
/// unmarked half is not counted - which does mean a half-annotated fixture checks fewer pairs
/// than a finished one, exactly as `graded_nodes` already reports for the mapping itself.
fn delimiter_pairs_agree(
    mapping: &super::HumanMapping,
    before: &Code,
    after: &Code,
    paintings: &[(&str, [Vec<Option<TextLabel>>; 2])],
) -> Vec<String> {
    let (Some(before_tree), Some(after_tree)) = (before.ast.as_ref(), after.ast.as_ref()) else {
        return Vec::new();
    };
    let caches =
        rebuild_caches_for_mapping(mapping, before_tree.root_node(), after_tree.root_node());

    let mut violations = Vec::new();
    for (side, code, root) in [
        (0usize, before, before_tree.root_node()),
        (1usize, after, after_tree.root_node()),
    ] {
        let errors = error_ranges(root);
        for (open, close) in delimiter_pairs(root, &code.contents) {
            let (from, to) = (open.end_byte(), close.start_byte());
            if errors.iter().any(|&(start, end)| start < to && end > from) {
                continue;
            }

            for (painting, labels) in paintings {
                let (opened, closed) = (
                    labels[side][open.start_byte()],
                    labels[side][close.start_byte()],
                );
                if opened != closed {
                    violations.push(format!(
                        "painting '{painting}' {} paints {:?} on row {} as {} but its matching \
                         {:?} on row {} as {}",
                        side_name(side),
                        open.kind(),
                        open.start_position().row + 1,
                        verdict_name(opened),
                        close.kind(),
                        close.start_position().row + 1,
                        verdict_name(closed),
                    ));
                }
            }

            let (opened, closed) = (mark_of(open, side, &caches), mark_of(close, side, &caches));
            if let (Some(opened), Some(closed)) = (opened, closed)
                && opened != closed
            {
                violations.push(format!(
                    "mapping {} marks {:?} on row {} as {opened} but its matching {:?} on row {} \
                     as {closed}",
                    side_name(side),
                    open.kind(),
                    open.start_position().row + 1,
                    close.kind(),
                    close.start_position().row + 1,
                ));
            }
        }
    }
    violations
}

/// Byte ranges of every `ERROR`/`MISSING` node in `root`'s tree. A `MISSING` node is zero-width,
/// so its range is widened to one byte - otherwise it could never overlap anything and the pairs
/// tree-sitter invented around it would be checked as if they were real.
fn error_ranges(root: Node) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.is_error() || node.is_missing() {
            ranges.push((
                node.start_byte(),
                node.end_byte().max(node.start_byte() + 1),
            ));
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    ranges
}

/// Every (opener, closer) pair in `root`'s tree - see [`delimiter_pairs_agree`] for the rule.
fn delimiter_pairs<'tree>(root: Node<'tree>, contents: &str) -> Vec<(Node<'tree>, Node<'tree>)> {
    let mut pairs = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        let mut open: Vec<Node<'tree>> = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
            let kind = child.kind();
            if child.child_count() != 0 || contents.get(child.byte_range()) != Some(kind) {
                continue;
            }
            if closers_of(kind).is_some() {
                open.push(child);
            } else if is_closer(kind)
                && let Some(at) = open
                    .iter()
                    .rposition(|node| closers_of(node.kind()).is_some_and(|c| c.contains(&kind)))
            {
                pairs.push((open.remove(at), child));
            }
        }
    }
    pairs
}

/// What the tree mapping says happened to `node`, or `None` if it says nothing.
fn mark_of(node: Node, side: usize, caches: &Caches) -> Option<&'static str> {
    let status = if side == 0 {
        status_before(node, caches)
    } else {
        status_after(node, caches)
    };
    match status {
        NodeStatus::Unmarked => None,
        NodeStatus::Matched => Some("matched"),
        NodeStatus::Marked {
            kind: MarkKind::Deleted,
            ..
        } => Some("deleted"),
        NodeStatus::Marked {
            kind: MarkKind::Inserted,
            ..
        } => Some("inserted"),
    }
}

fn verdict_name(label: Option<TextLabel>) -> &'static str {
    label.map_or("unpainted", TextLabel::name)
}

fn side_name(side: usize) -> &'static str {
    if side == 0 { "before" } else { "after" }
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// The assertions the per-fixture tests call
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// Asserts `name`'s ground truth contradicts itself in no way at all - what every fixture whose
/// data has been checked or repaired uses.
pub fn assert_ground_truth_invariants(name: &str) -> Result<()> {
    assert_ground_truth_invariants_with_known_violations(name, 0)
}

/// [`assert_ground_truth_invariants`] for a fixture whose ground truth has not been repaired yet:
/// `expected` is how many violations it has, and the test fails if the real number is anything
/// else.
///
/// **Exact, unlike the mapping and painting clamps next door, deliberately.** Those two record a
/// bound on a distance - a painting limit is `ceil(rate) + 0.01`, above the measurement by
/// construction - so only the upward direction can mean anything there. This records a *count of
/// specific contradictions*, each one written out in the comment above the call, and a count that
/// is allowed to be an over-estimate is a test that stops checking the moment somebody repairs one
/// of them. Half-repairing a fixture should say so, and here it does: the failure names the
/// number to write instead, which is a one-line edit made by the person who just changed the data.
///
/// The alternative - passing on fewer - is how a limit outlives its measurement, which this corpus
/// has been bitten by twice (the 86 painting stubs left at an unconditionally-passing `100.0`, and
/// the mapping limits recorded against a mapping that was only half annotated).
pub fn assert_ground_truth_invariants_with_known_violations(
    name: &str,
    expected: usize,
) -> Result<()> {
    let violations = ground_truth_invariant_violations(name)?;
    if violations.len() == expected {
        return Ok(());
    }
    if violations.len() < expected {
        anyhow::bail!(
            "'{name}' now breaks {} of its own ground-truth invariants, not the {expected} \
             recorded here - if that is a repair, record {} and update the note above this call \
             to describe what is left:\n  {}",
            violations.len(),
            violations.len(),
            violations.join("\n  ")
        );
    }
    anyhow::bail!(
        "'{name}' breaks {} of its own ground-truth invariants, more than the {expected} recorded \
         here:\n  {}",
        violations.len(),
        violations.join("\n  ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::{Code, Language};
    use crate::test::helper::human_mapping::{
        HumanMapping, HumanMappingEntry, HumanOperation, HumanTextEntry, HumanTextMapping,
        HumanTextOperation,
    };
    use crate::test::helper::path_for_node;

    fn rust(source: &str) -> Code {
        Code::from_string(source, &Language::Rust)
    }

    fn span(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
    ) -> HumanTextSpan {
        HumanTextSpan {
            start_row,
            start_column,
            end_row,
            end_column,
        }
    }

    /// A mapping carrying one named painting and nothing else - enough for the two painting
    /// invariants, which never look at `entries`.
    fn painted(paintings: Vec<(&str, Vec<HumanTextEntry>)>) -> HumanMapping {
        HumanMapping {
            text_mappings: paintings
                .into_iter()
                .map(|(name, entries)| NamedTextMapping {
                    name: name.to_string(),
                    mapping: HumanTextMapping { entries },
                })
                .collect(),
            ..Default::default()
        }
    }

    /// A `Delete` of `count` bytes from column `at` of the first row.
    fn deleted(at: usize, count: usize) -> HumanTextEntry {
        HumanTextEntry {
            operation: HumanTextOperation::Delete,
            before: vec![span(0, at, 0, at + count)],
            after: Vec::new(),
        }
    }

    fn violations(mapping: &HumanMapping, before: &Code, after: &Code) -> Vec<String> {
        ground_truth_invariant_violations_for(mapping, before, after).expect("checks run")
    }

    // ── Invariant 1 ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn a_painted_run_that_ends_on_a_trailing_space_is_reported() {
        let before = rust("let x = 1;  \n");
        let after = rust("\n");
        // `1;  ` - one column past the semicolon, into the trailing spaces.
        let mapping = painted(vec![("Only one solution", vec![deleted(8, 4)])]);

        let found = violations(&mapping, &before, &after);
        assert_eq!(found.len(), 1, "got {found:#?}");
        assert!(found[0].contains("not on a visible character"), "{found:?}");
    }

    #[test]
    fn a_painted_run_that_ends_on_the_last_visible_character_is_accepted() {
        let before = rust("let x = 1;  \n");
        let after = rust("\n");
        let mapping = painted(vec![("Only one solution", vec![deleted(8, 2)])]);

        assert!(violations(&mapping, &before, &after).is_empty());
    }

    #[test]
    fn a_row_with_nothing_visible_on_it_is_exempt() {
        // The painted row is `    `, all whitespace: there is no character on it that could
        // legally end a painted run, so the rule has nothing to say about it.
        let before = rust("fn f() {\n    \n}\n");
        let after = rust("fn f() {\n}\n");
        let mapping = painted(vec![(
            "Only one solution",
            vec![HumanTextEntry {
                operation: HumanTextOperation::Delete,
                before: vec![span(1, 0, 1, 4)],
                after: Vec::new(),
            }],
        )]);

        assert!(violations(&mapping, &before, &after).is_empty());
    }

    // ── Invariant 2 ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn a_minimal_painting_reaching_past_its_full_counterpart_is_reported() {
        let before = rust("let x = 1;\n");
        let after = rust("let x = 2;\n");
        let update = |from: usize, to: usize| HumanTextEntry {
            operation: HumanTextOperation::Match,
            before: vec![span(0, from, 0, to)],
            after: vec![span(0, from, 0, to)],
        };
        let mapping = painted(vec![
            ("Minimal", vec![update(4, 9)]),
            ("Full", vec![update(8, 9)]),
        ]);

        let found = violations(&mapping, &before, &after);
        assert_eq!(found.len(), 1, "got {found:#?}");
        assert!(found[0].contains("leaves unpainted"), "{found:?}");
    }

    #[test]
    fn a_full_painting_wider_than_its_minimal_counterpart_is_the_expected_shape() {
        let before = rust("let x = 1;\n");
        let after = rust("let x = 2;\n");
        let update = |from: usize, to: usize| HumanTextEntry {
            operation: HumanTextOperation::Match,
            before: vec![span(0, from, 0, to)],
            after: vec![span(0, from, 0, to)],
        };
        let mapping = painted(vec![
            ("Minimal", vec![update(8, 9)]),
            ("Full", vec![update(4, 9)]),
        ]);

        assert!(violations(&mapping, &before, &after).is_empty());
    }

    #[test]
    fn a_single_painting_answers_for_both_presets_so_there_is_nothing_to_compare() {
        let before = rust("let x = 1;\n");
        let after = rust("let x = 2;\n");
        let mapping = painted(vec![(
            "Only one solution",
            vec![HumanTextEntry {
                operation: HumanTextOperation::Match,
                before: vec![span(0, 8, 0, 9)],
                after: vec![span(0, 8, 0, 9)],
            }],
        )]);

        assert!(violations(&mapping, &before, &after).is_empty());
    }

    // ── Invariant 3, the painted half ───────────────────────────────────────────────────────

    #[test]
    fn a_painted_opening_brace_whose_closing_brace_is_unpainted_is_reported() {
        let before = rust("fn f() { g(); }\n");
        let after = rust("\n");
        let mapping = painted(vec![("Only one solution", vec![deleted(7, 1)])]);

        let found = violations(&mapping, &before, &after);
        assert!(
            found
                .iter()
                .any(|v| v.contains("as delete but its matching")),
            "got {found:#?}"
        );
    }

    #[test]
    fn painting_both_halves_of_a_pair_the_same_way_is_accepted() {
        let before = rust("fn f() { g(); }\n");
        let after = rust("\n");
        let mapping = painted(vec![(
            "Only one solution",
            vec![deleted(7, 1), deleted(14, 1)],
        )]);

        assert!(violations(&mapping, &before, &after).is_empty());
    }

    #[test]
    fn only_a_leaf_whose_kind_is_its_own_text_is_a_delimiter() {
        // Two `(` in this line: the call's, which is an anonymous `(` leaf, and the one inside the
        // string, which is a `string_content` leaf that merely reads `(`. Pairing the second would
        // be pairing text nobody wrote as a delimiter - the corpus holds several hundred such
        // leaves.
        let code = rust("fn f() { g(\"(\"); }\n");
        let pairs = delimiter_pairs(code.ast.as_ref().unwrap().root_node(), &code.contents);

        let mut kinds: Vec<(&str, usize)> = pairs
            .iter()
            .map(|(open, _)| (open.kind(), open.start_position().column))
            .collect();
        // `delimiter_pairs` walks the tree with a stack, so the order it reports parents in is
        // not the source order; only the set of pairs is the contract.
        kinds.sort_by_key(|(_, column)| *column);
        assert_eq!(
            kinds,
            vec![("(", 4), ("{", 7), ("(", 10)],
            "the `(` at column 12 is inside a string literal and must not be paired"
        );
    }

    // ── Invariant 3, the mapped half ────────────────────────────────────────────────────────

    /// Every before node paired with the identically-positioned after node, except the nodes whose
    /// text is `mark_deleted`, which are recorded as deletions instead.
    fn mapping_with_deleted_text(before: &Code, after: &Code, mark_deleted: &str) -> HumanMapping {
        let before_root = before.ast.as_ref().unwrap().root_node();
        let after_root = after.ast.as_ref().unwrap().root_node();
        let mut entries = Vec::new();
        let mut stack = vec![(before_root, after_root)];
        while let Some((b, a)) = stack.pop() {
            let (before_path, after_path) = (path_for_node(b), path_for_node(a));
            if &before.contents[b.byte_range()] == mark_deleted {
                entries.push(HumanMappingEntry {
                    operation: HumanOperation::Delete,
                    before_path: Some(before_path),
                    after_path: None,
                });
            } else {
                entries.push(HumanMappingEntry {
                    operation: HumanOperation::Identical,
                    before_path: Some(before_path),
                    after_path: Some(after_path),
                });
            }
            for index in 0..b.child_count().min(a.child_count()) {
                stack.push((b.child(index).unwrap(), a.child(index).unwrap()));
            }
        }
        HumanMapping {
            entries,
            ..Default::default()
        }
    }

    #[test]
    fn a_deleted_paren_whose_partner_is_matched_is_reported() {
        let before = rust("fn f() { g(); }\n");
        let after = before.clone();
        let mut after = rust(&after.contents);
        after.ensure_parsed().unwrap();
        let mapping = mapping_with_deleted_text(&before, &after, "(");

        let found = violations(&mapping, &before, &after);
        assert!(
            found
                .iter()
                .any(|v| v.contains("as deleted but its matching")),
            "got {found:#?}"
        );
    }

    #[test]
    fn a_mapping_that_says_nothing_about_a_delimiter_contradicts_nothing() {
        // No entries at all: every node is `Unmarked`, so no pair has two claims to compare.
        let before = rust("fn f() { g(); }\n");
        let after = rust("fn f() { g(); }\n");

        assert!(violations(&HumanMapping::default(), &before, &after).is_empty());
    }

    #[test]
    fn a_pair_with_a_parse_error_between_its_halves_is_skipped() {
        // tree-sitter recovers from the stray `@` by inventing structure; whatever braces it pairs
        // across that region are not pairs anybody wrote.
        let before = rust("fn f() { @ }\n");
        let after = rust("fn f() { @ }\n");
        assert!(
            !error_ranges(before.ast.as_ref().unwrap().root_node()).is_empty(),
            "this test is only meaningful if the source really does fail to parse"
        );
        let mapping = mapping_with_deleted_text(&before, &after, "{");

        assert!(
            violations(&mapping, &before, &after).is_empty(),
            "a pair straddling an ERROR must not be checked"
        );
    }

    // ── The assertion wrappers ──────────────────────────────────────────────────────────────

    #[test]
    fn the_delimiter_table_pairs_every_opener_with_at_least_one_closer() {
        for (open, closers) in DELIMITERS {
            assert!(!closers.is_empty(), "{open} has no closer");
            assert!(closers.iter().all(|closer| is_closer(closer)));
            assert!(closers_of(open).is_some());
        }
    }
}
