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
//! codediff against a claim its author would not defend if it were pointed out. These five
//! invariants are that missing half: they can fail only because the hand-authored data disagrees
//! with itself.
//!
//! * [`rows_end_on_visible_characters`] - no painted run on a row may end on whitespace.
//! * [`full_painting_covers_minimal`] - every byte painted under `Minimal` is painted under `Full`.
//! * [`delimiter_pairs_agree`] - a bracket and its partner carry one verdict in the tree mapping.
//! * [`full_paints_a_wholly_changed_line_whole`] - a `Full` line whose every visible character is
//!   inserted, or every one deleted, has no unpainted byte before its last visible character.
//! * [`no_unpainted_whitespace_between_painted_regions`] - a `Full` painting never breaks one
//!   highlight in two over whitespace.
//!
//! The last two were added on 2026-09-08 and wired in the same day, at **zero violations across
//! all 249 painted fixtures** - so unlike the first three they arrive with no clamped fixtures
//! behind them. Both are scoped to the paintings `FULL` is answerable to, via
//! [`paintings_with_labels`]; `MINIMAL` is the tight reading and is free to leave whitespace
//! alone.
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

/// One painting projected to per-byte labels, `[before, after]` - `None` where nothing paints that
/// byte. Named because every check here passes it around and `clippy::type_complexity` is right
/// that the raw form reads badly in a signature.
type PaintedLabels = [Vec<Option<TextLabel>>; 2];

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

    // One byte-label vector per painting per side, built once: every check that reads the
    // painting reads it through exactly the projection the scorer does (`label_bytes`), so an
    // invariant can never fire on a byte no comparison would ever look at.
    let mut paintings: Vec<(&str, PaintedLabels)> = Vec::new();
    for named in &mapping.text_mappings {
        paintings.push((named.name.as_str(), painted_labels(named, before, after)?));
    }

    for (name, labels) in &paintings {
        violations.extend(rows_end_on_visible_characters(name, labels, before, after));
    }
    violations.extend(full_painting_covers_minimal(mapping, before, after)?);
    violations.extend(delimiter_pairs_agree(mapping, before, after));
    // Invariants 4 and 5 read only the paintings `FULL` answers to, so they take their own pass
    // over `paintings_with_labels` rather than the `paintings` list above - which holds every
    // painting, `Minimal` ones included, and those two rules have nothing to say about those.
    let (leading, interior, minimal_indentation) =
        full_painting_whitespace_violations(mapping, before, after)?;
    violations.extend(leading);
    violations.extend(interior);
    violations.extend(minimal_indentation);

    Ok(violations)
}

/// One painting reduced to per-byte labels, `[before, after]`.
fn painted_labels(named: &NamedTextMapping, before: &Code, after: &Code) -> Result<PaintedLabels> {
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
    labels: &PaintedLabels,
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
// Invariants 4, 5 and 6: what each preset may and may not do with whitespace
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// Every painting of `mapping` that `options`' own rules can be held to, paired with its per-byte
/// labels.
///
/// `paintings_for_mode` answers with the fixture's single painting when it has only one, so a
/// fixture painted once is measured here too: that one painting is what the preset is scored
/// against, so it is what the preset's rules have to hold for.
///
/// **Except when that sole painting is explicitly named for the *other* preset.** Six fixtures
/// were in that state on 2026-09-08 - five named `Minimal` (`cpp-ollama-ollama-update-commit-hash-3`
/// through `-6`, `javascript-typescript-interesting-small-edit-refactor`) and one named `Full`
/// (`c-openssl-openssl-whitespace-only`) - and `paintings_for_mode` hands a lone painting back for
/// both presets regardless of its name, because a lone painting has to answer for both. Holding a
/// reading the painter labelled *minimal* to the generous preset's closure rules, or one they
/// labelled *full* to the tight preset's prohibition, asserts something they never claimed. All
/// six have since been renamed to `Only one solution`; the filter stays as a guard against the
/// state recurring.
pub(crate) fn paintings_with_labels<'a>(
    mapping: &'a super::HumanMapping,
    before: &Code,
    after: &Code,
    options: RenderOptions,
) -> Result<Vec<(&'a str, PaintedLabels)>> {
    let Ok(paintings) = paintings_for_mode(mapping, options) else {
        return Ok(Vec::new());
    };
    // Only a *lone* painting can be misnamed in the way this guards against: with two or more,
    // `paintings_for_mode` has already filtered to the ones named for this preset.
    let wanted_minimal = options == RenderOptions::MINIMAL;
    paintings
        .iter()
        .filter(|named| {
            paintings.len() > 1
                || !(designates_minimal(&named.name) || designates_full(&named.name))
                || designates_minimal(&named.name) == wanted_minimal
        })
        .map(|named| Ok((named.name.as_str(), painted_labels(named, before, after)?)))
        .collect()
}

/// Whether a painting's name declares it the `Minimal` reading - exactly the name, or the name
/// followed by a qualifier, matching `human_mapping::designates_preset`'s own rule.
pub(crate) fn designates_minimal(name: &str) -> bool {
    designates(name, "Minimal")
}

/// Whether a painting's name declares it the `Full` reading. See [`designates_minimal`].
pub(crate) fn designates_full(name: &str) -> bool {
    designates(name, "Full")
}

fn designates(name: &str, preset: &str) -> bool {
    name == preset
        || name
            .strip_prefix(preset)
            .is_some_and(|r| r.starts_with(' '))
}

/// Rows of `contents` as `(row index, byte offset of the row, the row itself)`.
fn rows_of(contents: &str) -> impl Iterator<Item = (usize, usize, &str)> {
    let mut offset = 0usize;
    contents.split('\n').enumerate().map(move |(row, line)| {
        let start = offset;
        offset += line.len() + 1;
        (row, start, line)
    })
}

/// **Invariant 4.** If a `Full` painting calls *every* visible character on a line
/// inserted, or every one of them deleted, the whole line - its whitespace included - carries that
/// verdict.
///
/// A line all of whose content is entering or leaving the file is entering or leaving *whole*. Its
/// indentation is not a survivor sitting on an inserted line and the spaces between its tokens are
/// not untouched ground; they are part of what was inserted. Painting the code but not the
/// whitespace draws a highlight broken into pieces, which reads as several edits to a surviving
/// line rather than one line arriving or departing. This is the data-side counterpart of the
/// closure `RenderOptions::leading_whitespace` already applies when rendering under `FULL` (see
/// that field's doc comment, and `extend_leading_whitespace`).
///
/// **The condition is "all of them", not "the first one".** An earlier draft asked only whether
/// the first visible character was `Insert`/`Delete` and required the indentation to match; that
/// is a different and wrong claim, because a *surviving* line can begin with an inserted token and
/// keep every space after it untouched, which no rule should forbid. Requiring the whole line to
/// be one verdict before saying anything about its whitespace is what makes the conclusion follow:
/// there is nothing on the line that survived, so there is nothing for the unpainted whitespace to
/// belong to.
///
/// Scoped to `Insert`/`Delete` deliberately. A line entirely `Move`d or `Update`d is still a
/// surviving line whose whitespace genuinely may not have changed - `Full` widening a `Move` over
/// a reindented block is exactly the case `paint_reindent_only_moves` exists for, and it has no
/// business claiming the old indentation as part of the move.
///
/// **What the whitespace has to be is *accounted for*, not identically labelled.** A deleted line
/// and the inserted line that replaces it can share their indentation, and a painter may say so by
/// pairing the two runs in one `Match` entry - which resolves to `Move`, a different label from
/// the `Insert`/`Delete` around it. That is a stronger claim than painting it `Insert`, not a
/// weaker one: it names the surviving bytes and their counterpart on the other side.
/// `go-lazygit-switch-to-strings` row 22 is the corpus's example, `\t\t\tindentation += "  "`
/// against `\t\t\tcount++`. So only an **unpainted** byte is reported; any verdict at all passes.
///
/// A line with no visible character at all is skipped: "every visible character is `Insert`" is
/// vacuously true there, and an all-whitespace line's own painting is what invariant 1 already
/// declines to judge.
fn full_paints_a_wholly_changed_line_whole(
    painting: &str,
    labels: &PaintedLabels,
    before: &Code,
    after: &Code,
) -> Vec<String> {
    let mut violations = Vec::new();
    for (side, contents) in [(0usize, &before.contents), (1usize, &after.contents)] {
        for (row, start, line) in rows_of(contents) {
            let visible: Vec<usize> = line
                .char_indices()
                .filter(|(_, c)| !c.is_whitespace())
                .map(|(i, _)| i)
                .collect();
            if visible.is_empty() {
                continue;
            }
            let Some(label) = labels[side][start + visible[0]] else {
                continue;
            };
            if !matches!(label, TextLabel::Insert | TextLabel::Delete) {
                continue;
            }
            if !visible
                .iter()
                .all(|&i| labels[side][start + i] == Some(label))
            {
                continue;
            }
            // Up to and including the last visible character, never past it. Invariant 1 forbids
            // a painted run *ending* on whitespace, so "the whole line" cannot mean the trailing
            // whitespace too without the two rules contradicting each other - and they would, on
            // real data: measured 2026-09-08, 16 of this check's 18 hits were a lone trailing
            // `\r` on a CRLF file or one trailing space, i.e. bytes invariant 1 exists to keep
            // unpainted. The line's *content* is what enters or leaves whole; what follows it is
            // the stripe of colour hanging off the end that invariant 1 already refuses.
            let end = visible.last().copied().unwrap_or(0)
                + line[visible.last().copied().unwrap_or(0)..]
                    .chars()
                    .next()
                    .map_or(1, char::len_utf8);
            let unpainted: Vec<usize> = (0..end)
                .filter(|&i| labels[side][start + i].is_none())
                .collect();
            // The exact columns, not just how many: this list is read by a human repairing the
            // painting by hand, and "12 of 16" does not say *which* twelve.
            if let (Some(&low), Some(&high)) = (unpainted.first(), unpainted.last()) {
                violations.push(format!(
                    "painting '{painting}' {} row {} paints every visible character {label:?} but \
                     leaves columns {low}..{} unpainted ({} byte(s) of whitespace inside the \
                     line's own content): {line:?}",
                    side_name(side),
                    row + 1,
                    high + 1,
                    unpainted.len(),
                ));
            }
        }
    }
    violations
}

/// **Invariant 5.** A `Full` painting never leaves a run of whitespace unpainted
/// between two painted regions on the same line.
///
/// `Full` is the generous reading: once both sides of a gap are highlighted, the space between
/// them is not a third, untouched thing - leaving it unpainted breaks one highlight into two and
/// reads as two separate edits. Only runs that are *entirely* whitespace are reported; an
/// unpainted run holding any visible character is a real gap between two real edits and this rule
/// has nothing to say about it.
///
/// Both painted neighbours have to exist on the same row, so a painted run reaching the end of a
/// line closes nothing across the newline.
fn no_unpainted_whitespace_between_painted_regions(
    painting: &str,
    labels: &PaintedLabels,
    before: &Code,
    after: &Code,
) -> Vec<String> {
    let mut violations = Vec::new();
    for (side, contents) in [(0usize, &before.contents), (1usize, &after.contents)] {
        for (row, start, line) in rows_of(contents) {
            let painted: Vec<usize> = (0..line.len())
                .filter(|&i| labels[side][start + i].is_some())
                .collect();
            let (Some(&first), Some(&last)) = (painted.first(), painted.last()) else {
                continue;
            };
            let mut run: Option<usize> = None;
            for i in first..=last {
                if labels[side][start + i].is_some() {
                    if let Some(run_start) = run.take()
                        && line[run_start..i].chars().all(char::is_whitespace)
                    {
                        violations.push(format!(
                            "painting '{painting}' {} row {} leaves columns {}..{} unpainted \
                             between two painted regions, and they are only whitespace: {line:?}",
                            side_name(side),
                            row + 1,
                            run_start,
                            i,
                        ));
                    }
                } else if run.is_none() {
                    run = Some(i);
                }
            }
        }
    }
    violations
}

/// **Invariant 6.** A `Minimal` painting never paints a line's leading whitespace.
///
/// The mirror of invariant 4, and the reason the two presets need separate rules rather than one
/// shared one. `Minimal` is the tightest defensible reading of an edit: nobody marking up a diff
/// by hand draws the highlight through the indentation in front of the code they are pointing at,
/// on any line, whether that line is new or edited in place. `RenderOptions::leading_whitespace`
/// is off under `MINIMAL` for exactly this reason and its own doc comment carries the corpus
/// measurement behind it (flipping the then-separate interior-indentation half to `false` alone
/// moved the handmade aggregate 1.2590% -> 1.1811% across ~40 fixtures). This is the data side of
/// that setting: a `Minimal` painting that claims the indentation is grading codediff against a
/// reading `MINIMAL` will never produce.
///
/// **Unconditional on the verdict, unlike invariant 4.** Invariant 4 has to ask what the rest of
/// the line says before it can conclude anything about the whitespace, because on a *surviving*
/// line the indentation genuinely may be untouched. Here there is nothing to ask: `Minimal` paints
/// as few bytes as it can, so leading whitespace is out under every verdict - `Insert` on a new
/// line included, which is precisely where `Full` and `Minimal` part company.
///
/// A line with no visible character is skipped, as in invariants 1 and 4. Its whole content is
/// whitespace, so "leading whitespace" is not a distinguishable part of it, and condemning every
/// possible painting of such a line is not a claim this rule is making.
fn minimal_never_paints_leading_whitespace(
    painting: &str,
    labels: &PaintedLabels,
    before: &Code,
    after: &Code,
) -> Vec<String> {
    let mut violations = Vec::new();
    for (side, contents) in [(0usize, &before.contents), (1usize, &after.contents)] {
        for (row, start, line) in rows_of(contents) {
            let Some(first) = line.find(|c: char| !c.is_whitespace()) else {
                continue;
            };
            let painted: Vec<usize> = (0..first)
                .filter(|&i| labels[side][start + i].is_some())
                .collect();
            if let (Some(&low), Some(&high)) = (painted.first(), painted.last()) {
                violations.push(format!(
                    "painting '{painting}' {} row {} paints columns {low}..{} of its own leading \
                     whitespace ({} byte(s)) - Minimal never claims a line's indentation: {line:?}",
                    side_name(side),
                    row + 1,
                    high + 1,
                    painted.len(),
                ));
            }
        }
    }
    violations
}

/// The three preset-scoped whitespace rules, as `(invariant 4, invariant 5, invariant 6)`.
///
/// [`ground_truth_invariant_violations_for`] calls this and flattens all three into its own list;
/// they stay separate here for `measure_full_painting_whitespace_invariants`, which reports the
/// counts apart so a corpus-wide sweep says which rule a fixture is failing. The first two read
/// the paintings `FULL` answers to and the third those `MINIMAL` answers to, which on a
/// two-painting fixture are different objects entirely.
pub fn full_painting_whitespace_violations(
    mapping: &super::HumanMapping,
    before: &Code,
    after: &Code,
) -> Result<(Vec<String>, Vec<String>, Vec<String>)> {
    let mut leading = Vec::new();
    let mut interior = Vec::new();
    let mut minimal_indentation = Vec::new();
    for (painting, labels) in paintings_with_labels(mapping, before, after, RenderOptions::FULL)? {
        leading.extend(full_paints_a_wholly_changed_line_whole(
            painting, &labels, before, after,
        ));
        interior.extend(no_unpainted_whitespace_between_painted_regions(
            painting, &labels, before, after,
        ));
    }
    for (painting, labels) in paintings_with_labels(mapping, before, after, RenderOptions::MINIMAL)?
    {
        minimal_indentation.extend(minimal_never_paints_leading_whitespace(
            painting, &labels, before, after,
        ));
    }
    Ok((leading, interior, minimal_indentation))
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

/// Mark one delimiter in the tree mapping and you have said something about the construct it
/// opens, so its partner must say the same thing: nobody deletes a `(` and keeps its `)`.
///
/// **The tree mapping only - the painting is deliberately not checked this way.** It was, and the
/// corpus answered: a delimiter can be *replaced by a different delimiter*, and a painting that
/// says so correctly looks like a contradiction here. `<tag>` becoming `<tag/>` makes the `/>`
/// genuinely new while the `<` it closes is not, and a CSS rule collapsing onto one line moves its
/// `}` without moving its `{`. Both were painted right and both were reported. The tree mapping
/// cannot express that shape - a node is one node, matched or not - so a `{` marked inserted whose
/// `}` is matched really is two claims about one construct. The rule holds where identity is the
/// subject and fails where motion and content are, so it is asked only of the mapping.
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
/// Compares **status only** - deleted, inserted, or matched - and not the derived
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
    fn a_painting_that_marks_one_half_of_a_pair_is_not_a_violation() {
        // The rule is asked of the tree mapping alone - see `delimiter_pairs_agree`. A painting is
        // free to say the `{` went and the `}` stayed, because that is a claim about text moving
        // and changing rather than about which node is which.
        let before = rust("fn f() { g(); }\n");
        let after = rust("\n");
        let mapping = painted(vec![("Only one solution", vec![deleted(7, 1)])]);

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
