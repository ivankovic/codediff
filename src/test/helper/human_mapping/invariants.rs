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
//! Nothing here runs `diff_code`. The mapping and painting tests grade codediff against
//! hand-authored data; these check that data against itself, so they fail only when a fixture's
//! mapping and paintings disagree with each other. The numbers are stable and referenced
//! elsewhere ("invariant 16"):
//!
//! 1. [`rows_end_on_visible_characters`] - no painted run ends in a row's *trailing* whitespace.
//! 2. [`full_painting_covers_minimal`] - every byte painted under `Minimal` is painted under `Full`.
//! 3. [`delimiter_pairs_agree`] - a bracket and its partner carry one verdict in the tree mapping.
//! 4. [`full_paints_a_wholly_changed_line_whole`] - a `Full` line whose every visible character is
//!    inserted, or every one deleted, has no unpainted byte before its last visible character.
//! 5. [`no_unpainted_whitespace_between_painted_regions`] - a `Full` painting never breaks one
//!    highlight in two over whitespace.
//! 6. [`minimal_never_paints_leading_whitespace`] - a `Minimal` painting never claims a line's
//!    indentation (the mirror of 5).
//! 7. [`painted_ranges_do_not_overlap`] - no two ranges of one painting claim the same byte.
//! 8. [`presets_agree_on_what_survives`] - a byte one preset paints `Move` is never painted
//!    `Insert` or `Delete` by the other.
//! 9. [`mapping_and_painting_agree_on_what_survives`] - a byte the painting paints `Move` is never
//!    one the tree mapping leaves unmatched.
//! 10. [`paired_leaves_are_not_deleted_and_inserted`] - a leaf the mapping pairs with a
//!     byte-identical leaf is never painted `Delete` while its partner is painted `Insert`.
//! 11. [`removed_leaves_are_painted`] - a named leaf the mapping deletes or inserts has at least
//!     one painted byte.
//! 12. [`edited_leaves_are_painted`] - a leaf the mapping pairs with a leaf that reads differently
//!     is painted on at least one side.
//! 13. [`painting_implies_mapping_edits`] - a painting that records an edit belongs to a mapping
//!     that records one too.
//! 14. [`identical_entries_are_token_identical`] - an `Identical` entry's two subtrees carry the
//!     same tokens.
//! 15. [`match_but_not_identical_entries_differ`] - a `MatchButNotIdentical` entry's two subtrees
//!     do not read byte-identically with every descendant paired inside.
//! 16. [`identifier_updates_are_painted_by_preset`] - a renamed identifier is painted at its own
//!     preset's granularity: `Minimal` marks the differing words or characters, `Full` marks it
//!     entire.
//! 17. [`boolean_flips_are_one_edit`] - a boolean the mapping pairs is not painted `Delete` on one
//!     side and `Insert` on the other.
//! 18. [`single_valued_fields_hold_a_pair`] - a matched pair's single-valued named field holds a
//!     matched pair, never a delete beside an insert.
//! 19. [`tokens_are_painted_whole`] - every byte of an operator such as `<=`, a boolean, or an
//!     access modifier such as `private` carries the same highlighting.
//!
//! 4 and 5 read only the paintings `FULL` answers to (see [`paintings_with_labels`]); `MINIMAL`
//! is free to leave whitespace alone. 9 is the only rule comparing the two ground truths'
//! rendered labels: they may chunk an edit differently, but cannot disagree about what survives.
//! 10 to 15 read the tree mapping through [`Caches`], so they can ask about pairs and do not
//! inherit the column-shift `Move` artifact that limits 9 to one direction. 14 compares tokens,
//! not text, because `Identical` subtrees may differ in whitespace. 19 is also a rule of the
//! renderer, which reads the same token list.
//!
//! Rejected as invariants: delimiter agreement *within a painting* (a `}` legitimately moves
//! while its `{` stays) and "a matched pair lands in one painting entry" (a rename is ordinarily
//! painted as a `Delete` plus an `Insert`).
//!
//! **All nineteen are intra-fixture**; `cross_fixture_convention_census`
//! (`tests/exploratory.rs`) covers the cross-fixture axis. They run as each fixture stub's
//! `invariants()` test, so a fixture records its own known violations beside its other clamps
//! rather than in a corpus-wide exemption list.

use anyhow::Result;
use tree_sitter::Node;

use super::{
    Caches, HumanTextSpan, MarkKind, NamedTextMapping, NodeStatus, TextLabel, label_bytes, load,
    paintings_for_mode, rebuild_caches_for_mapping, status_after, status_before,
};
use crate::code::Code;
use crate::diff::text::{RenderOptions, WHOLE_TOKENS};

/// One painting projected to per-byte labels, `[before, after]` - `None` where nothing paints that
/// byte.
type PaintedLabels = [Vec<Option<TextLabel>>; 2];

/// Where a violation is, on one side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViolationSite {
    /// 0 = before, 1 = after - the same side convention `TextDiff::all` and `PaintedLabels` use.
    pub side: usize,
    /// **0-based rows, byte columns** (the [`HumanTextSpan`] convention, for tools). Messages say
    /// 1-based rows, as a gutter shows them.
    pub span: HumanTextSpan,
}

/// One way a fixture's ground truth contradicts itself: which rule, what it says, and where to
/// look. The sites let a tool (`human_solver`'s `V` popup) jump to each location.
#[derive(Debug, Clone)]
pub struct GroundTruthViolation {
    /// Which of the nineteen rules, numbered as the module doc lists them.
    pub invariant: u8,
    /// The painting this is about, or `None` for the three rules that read only the tree mapping.
    pub painting: Option<String>,
    /// The human-readable line - what a test failure prints and what the popup lists.
    pub message: String,
    /// Every place to look, with runs of contiguous bytes already collapsed into one span each.
    pub sites: Vec<ViolationSite>,
}

impl std::fmt::Display for GroundTruthViolation {
    /// `[9] painting 'Full' before paints ...` - the rule's number, then its sentence. Not part of
    /// `message` because `human_solver`'s popup shows the number in its own column.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.invariant, self.message)
    }
}

impl GroundTruthViolation {
    fn new(
        invariant: u8,
        painting: Option<&str>,
        message: String,
        sites: Vec<ViolationSite>,
    ) -> Self {
        Self {
            invariant,
            painting: painting.map(str::to_string),
            message,
            sites,
        }
    }
}

/// At most this many sites are carried per violation. The message's count stays exact; this only
/// bounds how many can be jumped to.
const MAX_SITES: usize = 20;

/// The span covering `start..end` of `contents`, in 0-based rows and byte columns.
fn span_of_bytes(contents: &str, start: usize, end: usize) -> HumanTextSpan {
    let row_and_column = |offset: usize| {
        let row = contents[..offset].matches('\n').count();
        let column = offset - contents[..offset].rfind('\n').map_or(0, |index| index + 1);
        (row, column)
    };
    let (start_row, start_column) = row_and_column(start);
    let (end_row, end_column) = row_and_column(end);
    HumanTextSpan {
        start_row,
        start_column,
        end_row,
        end_column,
    }
}

/// The span a node occupies (`tree_sitter::Point::column` is already a byte offset).
fn span_of_node(node: Node) -> HumanTextSpan {
    HumanTextSpan {
        start_row: node.start_position().row,
        start_column: node.start_position().column,
        end_row: node.end_position().row,
        end_column: node.end_position().column,
    }
}

/// Byte offsets on one side as sites, with contiguous offsets collapsed into one span each (a
/// jump per byte is useless). `offsets` must be ascending.
fn sites_from_offsets(side: usize, contents: &str, offsets: &[usize]) -> Vec<ViolationSite> {
    let mut sites: Vec<ViolationSite> = Vec::new();
    let mut run: Option<(usize, usize)> = None;
    for &offset in offsets {
        match run {
            Some((start, end)) if end == offset => run = Some((start, offset + 1)),
            Some((start, end)) => {
                sites.push(ViolationSite {
                    side,
                    span: span_of_bytes(contents, start, end),
                });
                run = Some((offset, offset + 1));
            }
            None => run = Some((offset, offset + 1)),
        }
        if sites.len() >= MAX_SITES {
            return sites;
        }
    }
    if let Some((start, end)) = run {
        sites.push(ViolationSite {
            side,
            span: span_of_bytes(contents, start, end),
        });
    }
    sites
}

/// What invariants 2 and 8 carry while scoring one `Minimal` alternative against every `Full` one:
/// both report only the closest counterpart, since the alternatives are a disjunction (see
/// [`full_painting_covers_minimal`]).
struct ClosestFull<'a> {
    /// Bytes the two disagree about - the number being minimised.
    count: usize,
    /// The `Full` alternative's own name (invariant 2's message).
    name: &'a str,
    /// 1-based rows, per side, for the message's row list.
    rows: [Vec<usize>; 2],
    /// Byte offsets, per side, for the violation's sites.
    offsets: [Vec<usize>; 2],
    /// One example, already phrased (invariant 8's message); empty where there is none.
    detail: String,
}

/// One site covering a whole row's `start_column..end_column`, 0-based.
fn site_on_row(side: usize, row: usize, start_column: usize, end_column: usize) -> ViolationSite {
    ViolationSite {
        side,
        span: HumanTextSpan {
            start_row: row,
            start_column,
            end_row: row,
            end_column,
        },
    }
}

/// Every way `name`'s ground truth contradicts itself, in a stable order. Empty is a pass.
pub fn ground_truth_invariant_violations(name: &str) -> Result<Vec<GroundTruthViolation>> {
    let (before, after) = &*crate::test::helper::handmade_test_code_pair(name)?;
    ground_truth_invariant_violations_for(&load(name)?, before, after)
}

/// [`ground_truth_invariant_violations`] over an already-loaded mapping and code pair.
/// **`human_solver` must call this one**: its mapping is in memory and usually unsaved, so the
/// by-name form would report violations the reader has just repaired.
pub fn ground_truth_invariant_violations_for(
    mapping: &super::HumanMapping,
    before: &Code,
    after: &Code,
) -> Result<Vec<GroundTruthViolation>> {
    let mut violations = Vec::new();

    // Every check reads the painting through the scorer's own projection (`label_bytes`), so an
    // invariant can never fire on a byte no comparison looks at.
    let mut paintings: Vec<(&str, PaintedLabels)> = Vec::new();
    for named in &mapping.text_mappings {
        paintings.push((named.name.as_str(), painted_labels(named, before, after)?));
    }

    for (name, labels) in &paintings {
        violations.extend(rows_end_on_visible_characters(name, labels, before, after));
    }
    // Reads the spans, not the labels - see `painted_ranges_do_not_overlap`.
    for named in &mapping.text_mappings {
        violations.extend(painted_ranges_do_not_overlap(named, before, after));
    }
    violations.extend(full_painting_covers_minimal(mapping, before, after)?);
    violations.extend(presets_agree_on_what_survives(mapping, before, after)?);
    violations.extend(mapping_and_painting_agree_on_what_survives(
        mapping, before, after,
    )?);
    violations.extend(delimiter_pairs_agree(mapping, before, after));
    if let (Some(before_tree), Some(after_tree)) = (before.ast.as_ref(), after.ast.as_ref()) {
        let context = TreeContext::build(mapping, before_tree.root_node(), after_tree.root_node());
        for (name, labels) in &paintings {
            violations.extend(paired_leaves_are_not_deleted_and_inserted(
                name, labels, &context, before, after,
            ));
            violations.extend(removed_leaves_are_painted(
                name, labels, &context, before, after,
            ));
            violations.extend(edited_leaves_are_painted(
                name, labels, &context, before, after,
            ));
        }
        for named in &mapping.text_mappings {
            violations.extend(painting_implies_mapping_edits(
                mapping, named, before, after,
            ));
        }
        for (name, labels) in &paintings {
            violations.extend(identifier_updates_are_painted_by_preset(
                name, labels, &context, before, after,
            ));
            violations.extend(tokens_are_painted_whole(
                name, labels, &context, before, after,
            ));
        }
        violations.extend(boolean_flips_are_one_edit(
            &paintings, &context, before, after,
        ));
        violations.extend(identical_entries_are_token_identical(
            &context, before, after,
        ));
        violations.extend(match_but_not_identical_entries_differ(
            &context, before, after,
        ));
        violations.extend(single_valued_fields_hold_a_pair(&context, before, after));
    }
    // Invariants 4 and 5 read only the paintings `FULL` answers to.
    let (leading, interior, minimal_indentation) =
        full_painting_whitespace_violations(mapping, before, after)?;
    violations.extend(leading);
    violations.extend(interior);
    violations.extend(minimal_indentation);

    Ok(violations)
}

/// One painting reduced to per-byte labels, `[before, after]`.
pub(crate) fn painted_labels(
    named: &NamedTextMapping,
    before: &Code,
    after: &Code,
) -> Result<PaintedLabels> {
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

/// No painted run may end in a row's **trailing** whitespace: a stripe of colour hanging off the
/// end of a line, pointing at nothing.
///
/// A run ending on a space mid-row is the ordinary shape of an edit (deleting `foo ` from
/// `foo bar`), and a run that is *entirely* whitespace has no visible character to end on; both
/// pass. A row with nothing visible is skipped: no painting of it could pass.
///
/// Checked against the projected labels, not raw spans, so the `\n` a span ending at column 0 of
/// the next row swallows is not reported: nothing paints it.
fn rows_end_on_visible_characters(
    painting: &str,
    labels: &PaintedLabels,
    before: &Code,
    after: &Code,
) -> Vec<GroundTruthViolation> {
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
            if !character.is_whitespace() {
                continue;
            }
            // Only *trailing* whitespace counts.
            if line[boundary..].chars().any(|c| !c.is_whitespace()) {
                continue;
            }
            // An all-whitespace run is exempt. Both ends snap to character boundaries: `last` is a
            // byte index.
            let run_start = (0..=last)
                .rev()
                .take_while(|&i| labels[side][start + i].is_some())
                .last()
                .unwrap_or(last);
            let run_start = (0..=run_start)
                .rev()
                .find(|&i| line.is_char_boundary(i))
                .unwrap_or(0);
            let run_end = boundary + character.len_utf8();
            if line[run_start..run_end].chars().all(char::is_whitespace) {
                continue;
            }
            violations.push(GroundTruthViolation::new(
                1,
                Some(painting),
                format!(
                    "painting '{painting}' {} row {} ends its last painted run on {character:?}, \
                     not on a visible character: {line:?}",
                    side_name(side),
                    row + 1,
                ),
                vec![site_on_row(side, row, run_start, run_end)],
            ));
        }
    }
    violations
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// Invariant 2: Full paints everything Minimal paints
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// Whatever `Minimal` paints, `Full` paints too.
///
/// The presets differ in *how much* of an edit to highlight, not in *what* the edit is. Only
/// painted-or-not is compared, since `Full` routinely widens a `Move` into the enclosing `Update`.
/// A lone painting answers for both presets, so nothing is compared. With several alternatives
/// per preset, each `Minimal` need only be covered by *some* `Full`: holding one arm of a
/// disjunction to another's `Full` asserts a pairing the painter never claimed.
fn full_painting_covers_minimal(
    mapping: &super::HumanMapping,
    before: &Code,
    after: &Code,
) -> Result<Vec<GroundTruthViolation>> {
    let (Ok(minimal), Ok(full)) = (
        paintings_for_mode(mapping, RenderOptions::MINIMAL),
        paintings_for_mode(mapping, RenderOptions::FULL),
    ) else {
        return Ok(Vec::new());
    };
    // A single painting is trivially its own superset.
    if minimal
        .iter()
        .all(|m| full.iter().any(|f| std::ptr::eq(*m, *f)))
    {
        return Ok(Vec::new());
    }

    let mut violations = Vec::new();
    for minimal in &minimal {
        let minimal_labels = painted_labels(minimal, before, after)?;
        let mut closest: Option<ClosestFull> = None;
        for full in &full {
            let full_labels = painted_labels(full, before, after)?;
            let mut missing = 0usize;
            let mut rows: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
            let mut offsets: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
            for (side, contents) in [(0usize, &before.contents), (1usize, &after.contents)] {
                for (offset, (minimal, full)) in minimal_labels[side]
                    .iter()
                    .zip(full_labels[side].iter())
                    .enumerate()
                {
                    if minimal.is_some() && full.is_none() {
                        missing += 1;
                        rows[side].push(row_of(contents, offset));
                        offsets[side].push(offset);
                    }
                }
            }
            if closest.as_ref().is_none_or(|best| missing < best.count) {
                closest = Some(ClosestFull {
                    count: missing,
                    name: full.name.as_str(),
                    rows,
                    offsets,
                    detail: String::new(),
                });
            }
        }
        if let Some(ClosestFull {
            count: missing,
            name: full,
            rows,
            offsets,
            ..
        }) = closest
            && missing > 0
        {
            violations.push(GroundTruthViolation::new(
                2,
                Some(&minimal.name),
                format!(
                    "painting '{}' paints {missing} byte(s) that '{full}' leaves unpainted, on {} \
                     - a Full painting must cover everything its Minimal counterpart covers",
                    minimal.name,
                    site_rows(&rows),
                ),
                [
                    sites_from_offsets(0, &before.contents, &offsets[0]),
                    sites_from_offsets(1, &after.contents, &offsets[1]),
                ]
                .concat(),
            ));
        }
    }
    Ok(violations)
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// Invariants 4, 5 and 6: what each preset may and may not do with whitespace
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// Every painting of `mapping` that `options`' own rules can be held to, paired with its per-byte
/// labels. A lone painting counts for both presets - **except** when it is explicitly named for
/// the *other* preset: holding a reading labelled *minimal* to the generous preset's rules
/// asserts something the painter never claimed.
pub(crate) fn paintings_with_labels<'a>(
    mapping: &'a super::HumanMapping,
    before: &Code,
    after: &Code,
    options: RenderOptions,
) -> Result<Vec<(&'a str, PaintedLabels)>> {
    let Ok(paintings) = paintings_for_mode(mapping, options) else {
        return Ok(Vec::new());
    };
    // Only a *lone* painting can be misnamed this way: with several, `paintings_for_mode` has
    // already filtered by name.
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

/// Whether a painting's name declares it the `Minimal` reading (the name, or the name and a
/// qualifier - `human_mapping::designates_preset`'s rule). `pub` for `human_solver`, which keeps
/// invariant 6 for the painter as they paint.
pub fn designates_minimal(name: &str) -> bool {
    super::designates_preset(name, "Minimal")
}

/// Whether a painting's name declares it the `Full` reading. `pub` for `human_solver`, which keeps
/// invariant 4 for the painter when branching a painting.
pub fn designates_full(name: &str) -> bool {
    super::designates_preset(name, "Full")
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

/// **Invariant 4.** If a `Full` painting calls *every* visible character on a line inserted, or
/// every one deleted, the whole line up to its last visible character - whitespace included - is
/// accounted for. A line entering or leaving the file does so whole; unpainted whitespace in it
/// reads as several edits to a surviving line. The data-side counterpart of
/// `RenderOptions::leading_whitespace`.
///
/// "All of them", not "the first one": a *surviving* line may begin with an inserted token.
/// Only `Insert`/`Delete`: a wholly `Move`d or `Update`d line survives, and its whitespace may
/// genuinely be unchanged. Any verdict on the whitespace passes, only **unpainted** bytes are
/// reported: pairing shared indentation in a `Match` is a stronger claim, not a weaker one
/// (`go-lazygit-switch-to-strings` row 22). A line with nothing visible is skipped.
fn full_paints_a_wholly_changed_line_whole(
    painting: &str,
    labels: &PaintedLabels,
    before: &Code,
    after: &Code,
) -> Vec<GroundTruthViolation> {
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
            // Never past the last visible character: trailing whitespace is what invariant 1
            // forbids painting, so including it would make the two rules contradict each other.
            let end = visible.last().copied().unwrap_or(0)
                + line[visible.last().copied().unwrap_or(0)..]
                    .chars()
                    .next()
                    .map_or(1, char::len_utf8);
            let unpainted: Vec<usize> = (0..end)
                .filter(|&i| labels[side][start + i].is_none())
                .collect();
            // The exact columns: a human repairing the painting needs *which* bytes.
            if let (Some(&low), Some(&high)) = (unpainted.first(), unpainted.last()) {
                violations.push(GroundTruthViolation::new(
                    4,
                    Some(painting),
                    format!(
                        "painting '{painting}' {} row {} paints every visible character {label:?} \
                         but leaves columns {low}..{} unpainted ({} byte(s) of whitespace inside \
                         the line's own content): {line:?}",
                        side_name(side),
                        row + 1,
                        high + 1,
                        unpainted.len(),
                    ),
                    vec![site_on_row(side, row, low, high + 1)],
                ));
            }
        }
    }
    violations
}

/// **Invariant 5.** A `Full` painting never leaves an all-whitespace run unpainted between two
/// painted regions on the same row: that breaks one highlight into two edits. A gap holding any
/// visible character is a real gap between two edits and passes.
fn no_unpainted_whitespace_between_painted_regions(
    painting: &str,
    labels: &PaintedLabels,
    before: &Code,
    after: &Code,
) -> Vec<GroundTruthViolation> {
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
                        violations.push(GroundTruthViolation::new(
                            5,
                            Some(painting),
                            format!(
                                "painting '{painting}' {} row {} leaves columns {}..{} unpainted \
                                 between two painted regions, and they are only whitespace: \
                                 {line:?}",
                                side_name(side),
                                row + 1,
                                run_start,
                                i,
                            ),
                            vec![site_on_row(side, row, run_start, i)],
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
/// The mirror of invariant 4, and the data side of `RenderOptions::leading_whitespace` being off
/// under `MINIMAL`: a `Minimal` painting that claims indentation grades codediff against a
/// reading `MINIMAL` never produces. Unconditional on the verdict, `Insert` on a new line
/// included - that is exactly where the presets part company. A line with nothing visible is
/// skipped, as in invariants 1 and 4.
fn minimal_never_paints_leading_whitespace(
    painting: &str,
    labels: &PaintedLabels,
    before: &Code,
    after: &Code,
) -> Vec<GroundTruthViolation> {
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
                violations.push(GroundTruthViolation::new(
                    6,
                    Some(painting),
                    format!(
                        "painting '{painting}' {} row {} paints columns {low}..{} of its own \
                         leading whitespace ({} byte(s)) - Minimal never claims a line's \
                         indentation: {line:?}",
                        side_name(side),
                        row + 1,
                        high + 1,
                        painted.len(),
                    ),
                    vec![site_on_row(side, row, low, high + 1)],
                ));
            }
        }
    }
    violations
}

/// The three preset-scoped whitespace rules, as `(invariant 4, invariant 5, invariant 6)`, kept
/// apart so a corpus sweep can say which one a fixture fails.
pub fn full_painting_whitespace_violations(
    mapping: &super::HumanMapping,
    before: &Code,
    after: &Code,
) -> Result<(
    Vec<GroundTruthViolation>,
    Vec<GroundTruthViolation>,
    Vec<GroundTruthViolation>,
)> {
    let mut leading = Vec::new();
    let mut interior = Vec::new();
    let mut minimal_indentation = Vec::new();
    // A lone painting asserts its rendering is unambiguous, not that it follows `Full`'s
    // conventions, so invariant 5 applies only to a painting actually named for `Full`.
    let named_for_full = mapping.text_mappings.len() > 1;
    for (painting, labels) in paintings_with_labels(mapping, before, after, RenderOptions::FULL)? {
        leading.extend(full_paints_a_wholly_changed_line_whole(
            painting, &labels, before, after,
        ));
        if named_for_full {
            interior.extend(no_unpainted_whitespace_between_painted_regions(
                painting, &labels, before, after,
            ));
        }
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
// Invariant 8: a byte one preset calls Move is not removed or added by the other
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// **Invariant 8.** A byte one preset paints `Move` is never painted `Insert` or `Delete` by the
/// other. `Move` says the bytes *survive*; two renderings of one edit cannot disagree about that.
///
/// `Move` against `Update` is the ordinary widening between presets (see
/// [`full_painting_covers_minimal`]) and is not reported. Alternatives are a disjunction, as in
/// invariant 2: each `Minimal` is scored against its closest `Full`. A lone painting is skipped.
fn presets_agree_on_what_survives(
    mapping: &super::HumanMapping,
    before: &Code,
    after: &Code,
) -> Result<Vec<GroundTruthViolation>> {
    let (Ok(minimal), Ok(full)) = (
        paintings_for_mode(mapping, RenderOptions::MINIMAL),
        paintings_for_mode(mapping, RenderOptions::FULL),
    ) else {
        return Ok(Vec::new());
    };
    if minimal
        .iter()
        .all(|m| full.iter().any(|f| std::ptr::eq(*m, *f)))
    {
        return Ok(Vec::new());
    }

    /// Whether one byte's two readings disagree about the code being there at all.
    fn contradicts(one: TextLabel, other: TextLabel) -> bool {
        matches!(
            (one, other),
            (TextLabel::Move, TextLabel::Insert | TextLabel::Delete)
                | (TextLabel::Insert | TextLabel::Delete, TextLabel::Move)
        )
    }

    let mut violations = Vec::new();
    for minimal in &minimal {
        let minimal_labels = painted_labels(minimal, before, after)?;
        let mut closest: Option<ClosestFull> = None;
        for full in &full {
            let full_labels = painted_labels(full, before, after)?;
            let mut count = 0usize;
            let mut first = String::new();
            let mut rows: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
            let mut offsets: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
            for (side, contents) in [(0usize, &before.contents), (1usize, &after.contents)] {
                for (offset, (left, right)) in minimal_labels[side]
                    .iter()
                    .zip(full_labels[side].iter())
                    .enumerate()
                {
                    let (Some(left), Some(right)) = (left, right) else {
                        continue;
                    };
                    if !contradicts(*left, *right) {
                        continue;
                    }
                    if count == 0 {
                        first = format!(
                            "the first reads {left:?} under '{}' and {right:?} under '{}'",
                            minimal.name, full.name,
                        );
                    }
                    rows[side].push(row_of(contents, offset));
                    offsets[side].push(offset);
                    count += 1;
                }
            }
            if closest.as_ref().is_none_or(|best| count < best.count) {
                closest = Some(ClosestFull {
                    count,
                    name: full.name.as_str(),
                    rows,
                    offsets,
                    detail: first,
                });
            }
        }
        if let Some(ClosestFull {
            count,
            rows,
            offsets,
            detail: first,
            ..
        }) = closest
            && count > 0
        {
            violations.push(GroundTruthViolation::new(
                8,
                Some(&minimal.name),
                format!(
                    "{count} byte(s) survive under one preset and do not under the other, on {} - \
                     {first}. A Move says the code is still there, so the other preset cannot \
                     call it removed or added",
                    site_rows(&rows),
                ),
                [
                    sites_from_offsets(0, &before.contents, &offsets[0]),
                    sites_from_offsets(1, &after.contents, &offsets[1]),
                ]
                .concat(),
            ));
        }
    }
    Ok(violations)
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// Invariant 7: two painted ranges never claim the same byte
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// **Invariant 7.** No two ranges of one painting claim the same byte.
///
/// An overlap renders one way (`render_paint_side` takes the highest-ranked verdict) and scores
/// another (`label_bytes` takes the last entry). `human_solver` refuses new ones at the keystroke
/// (`overlapping_painted_range`); this checks what is on disk.
///
/// **Checked against the raw spans**, unlike every other rule: the projection is exactly what
/// hides a double claim. Line terminators are never painted, so ranges meeting at a line break do
/// not overlap. One violation per range that lands on an earlier range's ground, per side.
fn painted_ranges_do_not_overlap(
    named: &NamedTextMapping,
    before: &Code,
    after: &Code,
) -> Vec<GroundTruthViolation> {
    let mut violations = Vec::new();
    for (side, contents) in [(0usize, &before.contents), (1usize, &after.contents)] {
        let bytes = contents.as_bytes();
        // Which entry claimed each byte, in list order - the order `label_bytes` resolves by.
        let mut claimed: Vec<Option<usize>> = vec![None; contents.len()];
        for (index, entry) in named.mapping.entries.iter().enumerate() {
            let spans = if side == 0 {
                &entry.before
            } else {
                &entry.after
            };
            for span in spans {
                let (Some(start), Some(end)) = (
                    super::byte_offset(contents, span.start_row, span.start_column),
                    super::byte_offset(contents, span.end_row, span.end_column),
                ) else {
                    continue;
                };
                let mut clash: Option<(usize, usize)> = None;
                for offset in start..end.min(contents.len()) {
                    if bytes[offset] == b'\n'
                        || (bytes[offset] == b'\r' && bytes.get(offset + 1) == Some(&b'\n'))
                    {
                        continue;
                    }
                    match claimed[offset] {
                        Some(owner) if clash.is_none() => clash = Some((owner, offset)),
                        _ => {}
                    }
                    claimed[offset] = Some(index);
                }
                if let Some((owner, offset)) = clash {
                    let row = contents[..offset].matches('\n').count();
                    violations.push(GroundTruthViolation::new(
                        7,
                        Some(&named.name),
                        format!(
                            "painting '{}' {} range {} claims byte {offset} (row {}) already \
                             claimed by range {owner} - an overlap renders by highest verdict and \
                             scores by list order, so it reads as one painting and grades as \
                             another",
                            named.name,
                            side_name(side),
                            index,
                            row + 1,
                        ),
                        // The whole offending range: shortening a range is the repair.
                        vec![ViolationSite {
                            side,
                            span: span_of_bytes(contents, start, end.min(contents.len())),
                        }],
                    ));
                }
            }
        }
    }
    violations
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// Invariant 9: the mapping and the painting agree on what survives
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// **Invariant 9.** A byte the painting calls `Move` is not one the tree mapping leaves unmatched.
///
/// The two ground truths chunk one edit differently by design (see
/// [`super::text_mapping_disagreements`]), but a painted `Move` says the code survives and an
/// unmatched node says it has no counterpart; only one can hold.
///
/// **Only that direction**: the tree side's own `Move` labels come from `TextDiff::from`'s
/// column-shift heuristic, so a tree `Move` against a painted `Delete` measures the renderer, not
/// the humans. **And only where the mapping really leaves the byte unmatched**, per
/// [`unmatched_bytes`]: `TextDiff::from` also emits `Delete`/`Insert` for characters edited inside
/// a matched leaf (`rust-rust-lang-rust-update-comment`).
///
/// Every painting is checked, not the best one: each claims to be a correct rendering.
fn mapping_and_painting_agree_on_what_survives(
    mapping: &super::HumanMapping,
    before: &Code,
    after: &Code,
) -> Result<Vec<GroundTruthViolation>> {
    if mapping.text_mappings.is_empty() || mapping.entries.is_empty() {
        return Ok(Vec::new());
    }
    let Ok(ast_diff) = super::as_ast_diff_for_mapping(mapping, before, after) else {
        return Ok(Vec::new());
    };
    let (Some(before_tree), Some(after_tree)) = (before.ast.as_ref(), after.ast.as_ref()) else {
        return Ok(Vec::new());
    };
    let node_cache = crate::diff::NodeCache::build(before, after);
    let text_diff = crate::diff::text::TextDiff::from(before, after, &ast_diff, &node_cache);
    let tree = [
        super::label_bytes_from_ranges(&before.contents, &text_diff.all(0)),
        super::label_bytes_from_ranges(&after.contents, &text_diff.all(1)),
    ];
    let caches =
        rebuild_caches_for_mapping(mapping, before_tree.root_node(), after_tree.root_node());
    let unmatched = [
        unmatched_bytes(
            before_tree.root_node(),
            before.contents.len(),
            0,
            &mapping.groups,
            &caches,
        ),
        unmatched_bytes(
            after_tree.root_node(),
            after.contents.len(),
            1,
            &mapping.groups,
            &caches,
        ),
    ];

    let mut violations = Vec::new();
    for named in &mapping.text_mappings {
        let painted = painted_labels(named, before, after)?;
        violations.extend(move_against_unmatched(
            &named.name,
            &painted,
            &tree,
            &unmatched,
            before,
            after,
        ));
    }
    Ok(violations)
}

/// Per byte of one side, whether the **smallest node containing it** is one the tree mapping
/// leaves unmatched.
///
/// A node its multi-map group leaves free does not count: which member is left over is
/// [`representative_entries`](super::representative_entries)' choice, not the human's
/// (`java-defects4j-chart-9-timeseries`). Painted in preorder so a child overwrites its parent,
/// and whitespace between children keeps the parent's answer - what `descendant_for_byte_range`
/// would say per byte, in one walk.
fn unmatched_bytes(
    root: Node,
    len: usize,
    side: usize,
    groups: &[super::MultiMapGroup],
    caches: &Caches,
) -> Vec<bool> {
    let mut mask = vec![false; len];
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        let status = if side == 0 {
            status_before(node, caches)
        } else {
            status_after(node, caches)
        };
        let unmatched = matches!(status, NodeStatus::Marked { .. })
            && !group_leaves_status_open(node, side, groups, caches);
        let (start, end) = (node.start_byte().min(len), node.end_byte().min(len));
        mask[start..end].fill(unmatched);
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    mask
}

/// [`mapping_and_painting_agree_on_what_survives`]' comparison over the built projections, split
/// out to test against label vectors. `unmatched` is [`unmatched_bytes`]' output per side.
fn move_against_unmatched(
    painting: &str,
    painted: &PaintedLabels,
    tree: &PaintedLabels,
    unmatched: &[Vec<bool>; 2],
    before: &Code,
    after: &Code,
) -> Vec<GroundTruthViolation> {
    let mut violations = Vec::new();
    for (side, contents) in [(0usize, &before.contents), (1usize, &after.contents)] {
        let mut count = 0usize;
        let mut rows = Vec::new();
        let mut offsets = Vec::new();
        let mut labels: Vec<&'static str> = Vec::new();
        for (offset, (paint, from_tree)) in painted[side].iter().zip(tree[side].iter()).enumerate()
        {
            if *paint != Some(TextLabel::Move)
                || !matches!(from_tree, Some(TextLabel::Delete | TextLabel::Insert))
                // The renderer says these bytes went away; only the mapping can say whether that
                // is a node with no counterpart or a character edited out of one that has one.
                || !unmatched[side].get(offset).copied().unwrap_or(false)
                // Whitespace lives between tokens, where the tree has no node, so neither ground
                // truth really describes it: `unmatched_bytes` gives it the enclosing verdict while
                // a `Full` painting takes indentation with its construct.
                || contents.as_bytes()[offset].is_ascii_whitespace()
            {
                continue;
            }
            let label = if *from_tree == Some(TextLabel::Delete) {
                "Delete"
            } else {
                "Insert"
            };
            if !labels.contains(&label) {
                labels.push(label);
            }
            rows.push(row_of(contents, offset));
            offsets.push(offset);
            count += 1;
        }
        if count > 0 {
            violations.push(GroundTruthViolation::new(
                9,
                Some(painting),
                format!(
                    "painting '{painting}' {} paints {count} byte(s) Move on {} that the tree \
                     mapping reads {} - a Move says the code survives and an unmatched node says \
                     it does not",
                    side_name(side),
                    row_list(&rows),
                    labels.join("/"),
                ),
                sites_from_offsets(side, contents, &offsets),
            ));
        }
    }
    violations
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// Invariant 3: a delimiter and its partner carry one verdict
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// The delimiters this pairs, opener to the closers that may end it: every anonymous leaf in the
/// corpus whose kind *is* its own text. `kind == text` keeps a `)` inside a string or comment
/// out. `<` also takes `/>`; a `<` with no closer among its siblings (`a < b`) never pairs.
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

/// **Invariant 3.** A delimiter and its partner carry one status in the tree mapping: nobody
/// deletes a `(` and keeps its `)`.
///
/// **The mapping only.** A painting may legitimately replace one delimiter with another (`<tag>`
/// becoming `<tag/>`) or move a `}` without its `{`; a tree node is matched or not, so there the
/// two halves really are two claims about one construct.
///
/// Pairing is structural, within one parent's direct children: a stack pairs each closer with the
/// nearest unclosed opener that admits it, so nothing crosses a parent. A pair with a parse error
/// between its halves is skipped, since error recovery invents pairs no human saw.
///
/// A half its multi-map group leaves free is not a claim ([`group_leaves_status_open`]): the
/// question is whether *some* admissible pairing agrees, as `check_group_entry` asks of codediff.
/// Checked per pair, so two pairs jointly infeasible through one group's count go unreported -
/// a false negative, never a false positive.
///
/// Compares **status only** (deleted, inserted, matched), not the derived move flag, which is a
/// numbering consequence. An `Unmarked` half makes no claim, so the pair is not counted.
fn delimiter_pairs_agree(
    mapping: &super::HumanMapping,
    before: &Code,
    after: &Code,
) -> Vec<GroundTruthViolation> {
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

            let (opened, closed) = (
                mark_of(open, side, &mapping.groups, &caches),
                mark_of(close, side, &mapping.groups, &caches),
            );
            if let (Some(opened), Some(closed)) = (opened, closed)
                && opened != closed
            {
                violations.push(GroundTruthViolation::new(
                    3,
                    None,
                    format!(
                        "mapping {} marks {:?} on row {} as {opened} but its matching {:?} on row \
                         {} as {closed}",
                        side_name(side),
                        open.kind(),
                        open.start_position().row + 1,
                        close.kind(),
                        close.start_position().row + 1,
                    ),
                    vec![
                        ViolationSite {
                            side,
                            span: span_of_node(open),
                        },
                        ViolationSite {
                            side,
                            span: span_of_node(close),
                        },
                    ],
                ));
            }
        }
    }
    violations
}

/// Byte ranges of every `ERROR`/`MISSING` node in `root`'s tree. A zero-width `MISSING` node is
/// widened to one byte, or it could never overlap the pairs invented around it.
pub(crate) fn error_ranges(root: Node) -> Vec<(usize, usize)> {
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
pub(crate) fn delimiter_pairs<'tree>(
    root: Node<'tree>,
    contents: &str,
) -> Vec<(Node<'tree>, Node<'tree>)> {
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

/// True when the pairings `node`'s [`MultiMapGroup`](super::MultiMapGroup) admits disagree about
/// whether *this* member is matched, so the mapping makes no claim here.
///
/// With N before and M after members, `min(N, M)` pairs match: every member of the shorter side is
/// always matched, and if the other side is empty none is. Strictly between, which member is left
/// over is not something the human wrote down. An
/// [`AllToAll`](super::GroupPairing::AllToAll) group leaves nothing open. Nothing propagates to
/// descendants: both halves of a delimiter pair share a parent, so one member holds both.
fn group_leaves_status_open(
    node: Node,
    side: usize,
    groups: &[super::MultiMapGroup],
    caches: &Caches,
) -> bool {
    let index = if side == 0 {
        caches.before_group.get(&node.id())
    } else {
        caches.after_group.get(&node.id())
    };
    let Some(group) = index.and_then(|index| groups.get(*index)) else {
        return false;
    };
    if group.pairing == super::GroupPairing::AllToAll {
        return false;
    }
    let (mine, theirs) = if side == 0 {
        (group.before_paths.len(), group.after_paths.len())
    } else {
        (group.after_paths.len(), group.before_paths.len())
    };
    let matched = mine.min(theirs);
    matched > 0 && matched < mine
}

/// What the tree mapping says happened to `node`, or `None` if it says nothing.
fn mark_of(
    node: Node,
    side: usize,
    groups: &[super::MultiMapGroup],
    caches: &Caches,
) -> Option<&'static str> {
    if group_leaves_status_open(node, side, groups, caches) {
        return None;
    }
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

/// One side's sites as a sorted, deduplicated row list, capped at `MAX_LISTED_ROWS` with the
/// remainder counted.
fn row_list(rows: &[usize]) -> String {
    const MAX_LISTED_ROWS: usize = 10;
    let mut rows = rows.to_vec();
    rows.sort_unstable();
    rows.dedup();
    let shown = rows
        .iter()
        .take(MAX_LISTED_ROWS)
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    match rows.len().checked_sub(MAX_LISTED_ROWS) {
        None | Some(0) if rows.len() == 1 => format!("row {shown}"),
        None | Some(0) => format!("rows {shown}"),
        Some(rest) => format!("rows {shown} and {rest} more"),
    }
}

/// Both sides' sites as one row list, naming each side - `before rows 3, 4 and after row 7`. A
/// side with no sites is left out.
fn site_rows(rows: &[Vec<usize>; 2]) -> String {
    [0usize, 1]
        .into_iter()
        .filter(|&side| !rows[side].is_empty())
        .map(|side| format!("{} {}", side_name(side), row_list(&rows[side])))
        .collect::<Vec<_>>()
        .join(" and ")
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// Invariants 10-15: the tree mapping read at the leaf, against the painting and against itself
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// What the tree mapping says about one leaf: the nearest entry on the path from it to the root,
/// its own or an ancestor's, since a mapping speaks about subtrees, not leaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LeafStatus<'tree> {
    /// Paired with a leaf that reads the same: the leaf's own `Identical` entry, or the leaf at
    /// the same offset under an `Identical` ancestor - well defined because such subtrees read
    /// token-for-token the same (invariant 14).
    Same(Node<'tree>),
    /// Paired by the leaf's own `Update` or `MatchButNotIdentical` entry.
    Paired(Node<'tree>),
    /// Deleted or inserted, by its own entry or under a `*WithChildren` ancestor.
    Removed,
    /// Under an `Update`/`MatchButNotIdentical` ancestor, or a childless `Delete`/`Insert`, with no
    /// entry of its own, or in a group member the group could leave over: the mapping has not said.
    Undecided,
}

/// The two trees indexed for the leaf-level invariants, built once per fixture.
pub(crate) struct TreeContext<'tree> {
    caches: Caches,
    /// The mapping's multi-map groups: a leaf whose group could leave it over is `Undecided`.
    groups: Vec<super::MultiMapGroup>,
    /// Node id to node, per side - how a partner id from [`Caches`] becomes a node again.
    ids: [std::collections::HashMap<usize, Node<'tree>>; 2],
    /// Every leaf per side, in source order.
    pub(crate) leaves: [Vec<Node<'tree>>; 2],
}

impl<'tree> TreeContext<'tree> {
    pub(crate) fn build(
        mapping: &super::HumanMapping,
        before_root: Node<'tree>,
        after_root: Node<'tree>,
    ) -> Self {
        let caches = rebuild_caches_for_mapping(mapping, before_root, after_root);
        let (before_ids, before_leaves) = index_tree(before_root);
        let (after_ids, after_leaves) = index_tree(after_root);
        Self {
            caches,
            groups: mapping.groups.clone(),
            ids: [before_ids, after_ids],
            leaves: [before_leaves, after_leaves],
        }
    }

    pub(crate) fn status(&self, leaf: Node<'tree>, side: usize) -> LeafStatus<'tree> {
        if group_leaves_status_open(leaf, side, &self.groups, &self.caches) {
            return LeafStatus::Undecided;
        }
        let (matches, operations, removed) = if side == 0 {
            (
                &self.caches.before_match,
                &self.caches.before_operation,
                &self.caches.before_removed,
            )
        } else {
            (
                &self.caches.after_match,
                &self.caches.after_operation,
                &self.caches.after_removed,
            )
        };
        let mut current = leaf;
        loop {
            if let Some(&partner) = matches.get(&current.id()) {
                let identical = operations.get(&current.id()).copied()
                    == Some(super::HumanOperation::Identical);
                let Some(partner) = self.ids[1 - side].get(&partner).copied() else {
                    return LeafStatus::Undecided;
                };
                if current.id() == leaf.id() {
                    return if identical {
                        LeafStatus::Same(partner)
                    } else {
                        LeafStatus::Paired(partner)
                    };
                }
                if !identical {
                    return LeafStatus::Undecided;
                }
                // The leaf at the same offset in the partner subtree.
                let start = partner.start_byte() + (leaf.start_byte() - current.start_byte());
                let end = start + (leaf.end_byte() - leaf.start_byte());
                return match partner.descendant_for_byte_range(start, end) {
                    Some(twin)
                        if twin.start_byte() == start
                            && twin.end_byte() == end
                            && twin.kind() == leaf.kind() =>
                    {
                        LeafStatus::Same(twin)
                    }
                    _ => LeafStatus::Undecided,
                };
            }
            if let Some(&with_children) = removed.get(&current.id()) {
                return if group_leaves_status_open(current, side, &self.groups, &self.caches) {
                    // The group could equally have left another member over.
                    LeafStatus::Undecided
                } else if current.id() == leaf.id() || with_children {
                    LeafStatus::Removed
                } else {
                    LeafStatus::Undecided
                };
            }
            match current.parent() {
                Some(parent) => current = parent,
                None => return LeafStatus::Undecided,
            }
        }
    }
}

fn index_tree<'tree>(
    root: Node<'tree>,
) -> (
    std::collections::HashMap<usize, Node<'tree>>,
    Vec<Node<'tree>>,
) {
    let mut ids = std::collections::HashMap::new();
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
    leaves.sort_by_key(|node| node.start_byte());
    (ids, leaves)
}

/// A leaf with something visible in it. Zero-width and whitespace-only leaves (Python's `indent`
/// and `newline`, and the like) carry nothing a painting could colour.
pub(crate) fn is_visible_leaf(leaf: Node, contents: &str) -> bool {
    contents
        .get(leaf.byte_range())
        .is_some_and(|text| !text.trim().is_empty())
}

/// A leaf whose text is not its own kind name: identifiers, literals, comments, string contents.
/// Punctuation is excluded where a rule says so, since which of two `}` survives is each ground
/// truth's own choice.
fn is_named_leaf(leaf: Node, contents: &str) -> bool {
    contents.get(leaf.byte_range()) != Some(leaf.kind())
}

/// The painting's verdict over a whole leaf: `Some(None)` unpainted, `Some(Some(label))` one label
/// over every byte, `None` a mixture.
fn whole_leaf_label(labels: &[Option<TextLabel>], leaf: Node) -> Option<Option<TextLabel>> {
    let slice = labels.get(leaf.byte_range())?;
    let first = *slice.first()?;
    slice.iter().all(|label| *label == first).then_some(first)
}

fn leaf_text(leaf: Node, contents: &str) -> String {
    contents[leaf.byte_range()].chars().take(40).collect()
}

fn row_of(contents: &str, byte: usize) -> usize {
    contents[..byte].matches('\n').count() + 1
}

/// The text with all whitespace removed: a reformatting is not an edit to a token.
fn without_whitespace(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Invariant 10: a leaf the mapping pairs with a byte-identical leaf is never painted `Delete`
/// while that partner is painted `Insert`.
///
/// The mirror of invariant 9, asked of the tree's pairs. `Identical` says one text; `Delete` and
/// `Insert` say it left and something new arrived. A line painted whole leaves the mapper's
/// tokens under `Update` ancestors, which [`LeafStatus::Undecided`] skips.
fn paired_leaves_are_not_deleted_and_inserted(
    painting: &str,
    painted: &PaintedLabels,
    context: &TreeContext,
    before: &Code,
    after: &Code,
) -> Vec<GroundTruthViolation> {
    let mut count = 0usize;
    let mut first = String::new();
    let mut rows: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
    let mut sites = Vec::new();
    for leaf in &context.leaves[0] {
        if !is_visible_leaf(*leaf, &before.contents) {
            continue;
        }
        let LeafStatus::Same(partner) = context.status(*leaf, 0) else {
            continue;
        };
        if whole_leaf_label(&painted[0], *leaf) != Some(Some(TextLabel::Delete))
            || whole_leaf_label(&painted[1], partner) != Some(Some(TextLabel::Insert))
        {
            continue;
        }
        if count == 0 {
            first = leaf_text(*leaf, &before.contents);
        }
        rows[0].push(row_of(&before.contents, leaf.start_byte()));
        rows[1].push(row_of(&after.contents, partner.start_byte()));
        if sites.len() < MAX_SITES {
            sites.push(ViolationSite {
                side: 0,
                span: span_of_node(*leaf),
            });
            sites.push(ViolationSite {
                side: 1,
                span: span_of_node(partner),
            });
        }
        count += 1;
    }
    if count == 0 {
        return Vec::new();
    }
    vec![GroundTruthViolation::new(
        10,
        Some(painting),
        format!(
            "painting '{painting}' paints {count} leaf pair(s) gone on one side and new on the \
             other that the tree mapping calls the same text, on {} - the first is `{first}`",
            site_rows(&rows),
        ),
        sites,
    )]
}

/// Invariant 11: a named leaf the mapping deletes or inserts has at least one painted byte.
///
/// Unpainted text claims "unchanged, and in place". Named leaves only ([`is_named_leaf`]): a
/// deleted `}` whose partner the painter kept instead is the ordinary brace-identity difference.
fn removed_leaves_are_painted(
    painting: &str,
    painted: &PaintedLabels,
    context: &TreeContext,
    before: &Code,
    after: &Code,
) -> Vec<GroundTruthViolation> {
    let mut violations = Vec::new();
    for (side, code) in [(0usize, before), (1usize, after)] {
        let mut count = 0usize;
        let mut first = String::new();
        let mut rows = Vec::new();
        let mut sites = Vec::new();
        for leaf in &context.leaves[side] {
            if !is_visible_leaf(*leaf, &code.contents) || !is_named_leaf(*leaf, &code.contents) {
                continue;
            }
            if context.status(*leaf, side) != LeafStatus::Removed
                || whole_leaf_label(&painted[side], *leaf) != Some(None)
            {
                continue;
            }
            if count == 0 {
                first = format!("{:?} `{}`", leaf.kind(), leaf_text(*leaf, &code.contents));
            }
            rows.push(row_of(&code.contents, leaf.start_byte()));
            if sites.len() < MAX_SITES {
                sites.push(ViolationSite {
                    side,
                    span: span_of_node(*leaf),
                });
            }
            count += 1;
        }
        if count > 0 {
            violations.push(GroundTruthViolation::new(
                11,
                Some(painting),
                format!(
                    "painting '{painting}' {} leaves {count} removed leaf/leaves unpainted on {}, \
                     the first {first} - unpainted text is unchanged and in place, and the tree \
                     mapping says this is gone",
                    side_name(side),
                    row_list(&rows),
                ),
                sites,
            ));
        }
    }
    violations
}

/// Invariant 12: a leaf the mapping pairs with a leaf that reads differently is painted on at
/// least one side.
///
/// *At least one side*: `Minimal` paints a rename's deleted prefix and nothing on the after side,
/// which is exact. Whitespace-only differences are exempt, as in invariant 14.
fn edited_leaves_are_painted(
    painting: &str,
    painted: &PaintedLabels,
    context: &TreeContext,
    before: &Code,
    after: &Code,
) -> Vec<GroundTruthViolation> {
    let mut count = 0usize;
    let mut first = String::new();
    let mut rows: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
    let mut sites = Vec::new();
    for leaf in &context.leaves[0] {
        if !is_visible_leaf(*leaf, &before.contents) {
            continue;
        }
        let LeafStatus::Paired(partner) = context.status(*leaf, 0) else {
            continue;
        };
        if without_whitespace(&before.contents[leaf.byte_range()])
            == without_whitespace(&after.contents[partner.byte_range()])
        {
            continue;
        }
        if whole_leaf_label(&painted[0], *leaf) != Some(None)
            || whole_leaf_label(&painted[1], partner) != Some(None)
        {
            continue;
        }
        if count == 0 {
            first = format!(
                "`{}` becoming `{}`",
                leaf_text(*leaf, &before.contents),
                leaf_text(partner, &after.contents),
            );
        }
        rows[0].push(row_of(&before.contents, leaf.start_byte()));
        rows[1].push(row_of(&after.contents, partner.start_byte()));
        if sites.len() < MAX_SITES {
            sites.push(ViolationSite {
                side: 0,
                span: span_of_node(*leaf),
            });
            sites.push(ViolationSite {
                side: 1,
                span: span_of_node(partner),
            });
        }
        count += 1;
    }
    if count == 0 {
        return Vec::new();
    }
    vec![GroundTruthViolation::new(
        12,
        Some(painting),
        format!(
            "painting '{painting}' paints nothing on either side of {count} edited leaf/leaves, \
             on {} - the first is {first}; the tree mapping says the text changed",
            site_rows(&rows),
        ),
        sites,
    )]
}

/// Invariant 13: a painting that records an edit belongs to a mapping that records one too.
///
/// An all-`Identical` mapping with balanced groups says nothing changed, and grades codediff
/// against nothing. A painting with a visible `Delete`/`Insert`, or a `Match` differing beyond
/// whitespace, says otherwise. Whitespace is exempt: it lives between nodes, where the tree cannot
/// record it.
fn painting_implies_mapping_edits(
    mapping: &super::HumanMapping,
    named: &NamedTextMapping,
    before: &Code,
    after: &Code,
) -> Vec<GroundTruthViolation> {
    if mapping.entries.is_empty()
        || mapping
            .entries
            .iter()
            .any(|entry| entry.operation != super::HumanOperation::Identical)
        || mapping
            .groups
            .iter()
            .any(|group| group.before_paths.len() != group.after_paths.len())
    {
        return Vec::new();
    }
    let side_text = |contents: &str, spans: &[HumanTextSpan]| -> String {
        spans
            .iter()
            .filter_map(|span| super::span_text(contents, *span))
            .map(without_whitespace)
            .collect()
    };
    let mut edits = 0usize;
    let mut rows: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
    let mut sites = Vec::new();
    for entry in &named.mapping.entries {
        let before_text = side_text(&before.contents, &entry.before);
        let after_text = side_text(&after.contents, &entry.after);
        let is_edit = match entry.operation {
            super::HumanTextOperation::Match => before_text != after_text,
            super::HumanTextOperation::Delete => !before_text.is_empty(),
            super::HumanTextOperation::Insert => !after_text.is_empty(),
        };
        if !is_edit {
            continue;
        }
        edits += 1;
        for (side, spans) in [(0usize, &entry.before), (1usize, &entry.after)] {
            rows[side].extend(spans.iter().map(|span| span.start_row + 1));
            sites.extend(
                spans
                    .iter()
                    .take(MAX_SITES.saturating_sub(sites.len()))
                    .map(|span| ViolationSite { side, span: *span }),
            );
        }
    }
    if edits == 0 {
        return Vec::new();
    }
    vec![GroundTruthViolation::new(
        13,
        Some(&named.name),
        format!(
            "painting '{}' records {edits} edit(s) to visible text on {} but every entry of the \
             tree mapping is Identical - one of the two records has not been finished",
            named.name,
            site_rows(&rows),
        ),
        sites,
    )]
}

/// The visible tokens under `node`, in source order, as (kind, text).
fn tokens_of(node: Node, contents: &str) -> Vec<(&'static str, String)> {
    let mut tokens = Vec::new();
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        if current.child_count() == 0 {
            if is_visible_leaf(current, contents) {
                tokens.push((current.kind(), contents[current.byte_range()].to_string()));
            }
        } else {
            let mut cursor = current.walk();
            let children: Vec<Node> = current.children(&mut cursor).collect();
            stack.extend(children.into_iter().rev());
        }
    }
    tokens
}

/// Invariant 14: an `Identical` entry's two subtrees carry the same tokens. Tokens, not text,
/// since a reformatting is not an edit. The leaf-level rules rely on it to find a leaf's twin.
fn identical_entries_are_token_identical(
    context: &TreeContext,
    before: &Code,
    after: &Code,
) -> Vec<GroundTruthViolation> {
    let mut violations = Vec::new();
    let mut pairs: Vec<(usize, usize)> = context
        .caches
        .before_match
        .iter()
        .filter(|(b, _)| {
            context.caches.before_operation.get(*b).copied()
                == Some(super::HumanOperation::Identical)
        })
        .map(|(b, a)| (*b, *a))
        .collect();
    pairs.sort_by_key(|(b, _)| context.ids[0].get(b).map(|n| n.start_byte()));
    for (b, a) in pairs {
        let (Some(before_node), Some(after_node)) =
            (context.ids[0].get(&b), context.ids[1].get(&a))
        else {
            continue;
        };
        let before_tokens = tokens_of(*before_node, &before.contents);
        let after_tokens = tokens_of(*after_node, &after.contents);
        if before_tokens == after_tokens {
            continue;
        }
        let difference = before_tokens
            .iter()
            .zip(after_tokens.iter())
            .find(|(x, y)| x != y)
            .map(|(x, y)| format!("`{}` against `{}`", x.1, y.1))
            .unwrap_or_else(|| {
                format!(
                    "{} token(s) against {}",
                    before_tokens.len(),
                    after_tokens.len()
                )
            });
        violations.push(GroundTruthViolation::new(
            14,
            None,
            format!(
                "mapping calls {:?} on before row {} Identical to {:?} on after row {}, but their \
                 tokens differ: {difference}",
                before_node.kind(),
                row_of(&before.contents, before_node.start_byte()),
                after_node.kind(),
                row_of(&after.contents, after_node.start_byte()),
            ),
            vec![
                ViolationSite {
                    side: 0,
                    span: span_of_node(*before_node),
                },
                ViolationSite {
                    side: 1,
                    span: span_of_node(*after_node),
                },
            ],
        ));
    }
    violations
}

/// Which field of `parent` holds `node`, or `None` when the grammar gives it no field.
fn field_of<'tree>(parent: Node<'tree>, node: Node<'tree>) -> Option<String> {
    let mut cursor = parent.walk();
    let children: Vec<Node<'tree>> = parent.children(&mut cursor).collect();
    children
        .iter()
        .position(|child| child.id() == node.id())
        .and_then(|i| parent.field_name_for_child(i as u32))
        .map(str::to_string)
}

/// How many of `parent`'s children carry `field`. More than one makes it a list, and a position
/// in a list is not an identity.
fn field_arity(parent: Node, field: &str) -> usize {
    let mut cursor = parent.walk();
    let count = parent.children(&mut cursor).count();
    (0..count)
        .filter(|i| parent.field_name_for_child(*i as u32) == Some(field))
        .count()
}

/// The after-side node that occupies the same unambiguous position as `before_leaf`, or `None`.
///
/// * **By name** - a *named* field holding exactly one child on both sides.
/// * **By elimination** - equal child counts with every *other* position paired. Needed because
///   some grammars name no fields where position is obvious (tree-sitter-java's `argument_list`).
fn pinned_counterpart<'tree>(
    context: &TreeContext<'tree>,
    before_leaf: Node<'tree>,
    before_parent: Node<'tree>,
    after_parent: Node<'tree>,
) -> Option<Node<'tree>> {
    if let Some(field) = field_of(before_parent, before_leaf)
        && field_arity(before_parent, &field) == 1
        && field_arity(after_parent, &field) == 1
        && let Some(after_leaf) = after_parent.child_by_field_name(field.as_str())
    {
        return Some(after_leaf);
    }

    let mut before_cursor = before_parent.walk();
    let before_children: Vec<Node<'tree>> = before_parent.children(&mut before_cursor).collect();
    let mut after_cursor = after_parent.walk();
    let after_children: Vec<Node<'tree>> = after_parent.children(&mut after_cursor).collect();
    if before_children.len() != after_children.len() {
        return None;
    }
    let index = before_children
        .iter()
        .position(|child| child.id() == before_leaf.id())?;
    // Every other position paired, in order. One unpaired position is pinned; two are a guess.
    for (i, (b, a)) in before_children
        .iter()
        .zip(after_children.iter())
        .enumerate()
    {
        if i == index {
            continue;
        }
        if context.caches.before_match.get(&b.id()) != Some(&a.id()) {
            return None;
        }
    }
    Some(after_children[index])
}

/// Invariant 18: an unambiguous position of a matched pair holds a matched pair, never a
/// delete beside an insert: the parent match already says the role persists.
///
/// Each condition prevents a real false positive: *childless* nodes (an empty container has no
/// named children either), a *named* field (two adjacent comments share no role), and arity *one*
/// (a removed flag beside an added subcommand in one argument list is not a pair).
/// `LeafStatus::Undecided` is not a violation.
fn single_valued_fields_hold_a_pair(
    context: &TreeContext,
    before: &Code,
    after: &Code,
) -> Vec<GroundTruthViolation> {
    let mut violations = Vec::new();
    for before_leaf in &context.leaves[0] {
        if !before_leaf.is_named() || context.status(*before_leaf, 0) != LeafStatus::Removed {
            continue;
        }
        let Some(before_parent) = before_leaf.parent() else {
            continue;
        };
        let Some(after_parent) = context
            .caches
            .before_match
            .get(&before_parent.id())
            .and_then(|id| context.ids[1].get(id))
        else {
            continue;
        };
        let Some(after_leaf) =
            pinned_counterpart(context, *before_leaf, before_parent, *after_parent)
        else {
            continue;
        };
        if !after_leaf.is_named()
            || after_leaf.child_count() != 0
            || context.status(after_leaf, 1) != LeafStatus::Removed
        {
            continue;
        }
        // Cross-kind pairs only: `Update` requires equal kinds, so a cross-kind pair has no
        // schema representation, while a same-kind delete+insert is a decision the author was
        // entitled to make.
        if before_leaf.kind() == after_leaf.kind() {
            continue;
        }
        violations.push(GroundTruthViolation::new(
            18,
            None,
            format!(
                "{} holds {} `{}` on before row {} and {} `{}` on after row {}, but the mapping \
                 deletes one and inserts the other - the parents are matched and nothing else can \
                 occupy that position, so the two are the same element",
                field_of(before_parent, *before_leaf).map_or_else(
                    || before_parent.kind().to_string(),
                    |field| format!("{}.{field}", before_parent.kind()),
                ),
                before_leaf.kind(),
                before_leaf
                    .utf8_text(before.contents.as_bytes())
                    .unwrap_or("<unreadable>"),
                row_of(&before.contents, before_leaf.start_byte()),
                after_leaf.kind(),
                after_leaf
                    .utf8_text(after.contents.as_bytes())
                    .unwrap_or("<unreadable>"),
                row_of(&after.contents, after_leaf.start_byte()),
            ),
            vec![
                ViolationSite {
                    side: 0,
                    span: span_of_node(*before_leaf),
                },
                ViolationSite {
                    side: 1,
                    span: span_of_node(after_leaf),
                },
            ],
        ));
    }
    violations
}

/// Invariant 15: a `MatchButNotIdentical` entry's two subtrees do not read byte-identically with
/// every descendant paired inside. Such an entry could only be satisfied by codediff calling an
/// identical subtree not identical, since `check_entry` is strict about the operation. Group
/// members are skipped: the operation describes the whole group.
fn match_but_not_identical_entries_differ(
    context: &TreeContext,
    before: &Code,
    after: &Code,
) -> Vec<GroundTruthViolation> {
    let mut violations = Vec::new();
    let mut pairs: Vec<(usize, usize)> = context
        .caches
        .before_match
        .iter()
        .filter(|(b, a)| {
            context.caches.before_operation.get(*b).copied()
                == Some(super::HumanOperation::MatchButNotIdentical)
                && !context.caches.before_group.contains_key(*b)
                && !context.caches.after_group.contains_key(*a)
        })
        .map(|(b, a)| (*b, *a))
        .collect();
    pairs.sort_by_key(|(b, _)| context.ids[0].get(b).map(|n| n.start_byte()));
    for (b, a) in pairs {
        let (Some(before_node), Some(after_node)) =
            (context.ids[0].get(&b), context.ids[1].get(&a))
        else {
            continue;
        };
        if before_node.kind() != after_node.kind()
            || before.contents[before_node.byte_range()] != after.contents[after_node.byte_range()]
        {
            continue;
        }
        let mut descendants_stay = true;
        let mut stack = vec![*before_node];
        while let Some(node) = stack.pop() {
            if node.id() != before_node.id() {
                if context.caches.before_removed.contains_key(&node.id()) {
                    descendants_stay = false;
                }
                if let Some(partner) = context
                    .caches
                    .before_match
                    .get(&node.id())
                    .and_then(|id| context.ids[1].get(id))
                    && !(after_node.start_byte() <= partner.start_byte()
                        && partner.end_byte() <= after_node.end_byte())
                {
                    descendants_stay = false;
                }
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
        }
        if !descendants_stay {
            continue;
        }
        violations.push(GroundTruthViolation::new(
            15,
            None,
            format!(
                "mapping calls {:?} on before row {} MatchButNotIdentical to after row {}, but \
                 the two read byte-identically and every descendant with an entry pairs inside it",
                before_node.kind(),
                row_of(&before.contents, before_node.start_byte()),
                row_of(&after.contents, after_node.start_byte()),
            ),
            vec![
                ViolationSite {
                    side: 0,
                    span: span_of_node(*before_node),
                },
                ViolationSite {
                    side: 1,
                    span: span_of_node(*after_node),
                },
            ],
        ));
    }
    violations
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// Invariant 16: an identifier the mapping calls edited is painted narrow by Minimal, whole by Full
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// Byte offsets inside an identifier at which a word may be cut: 0, both ends of any run of `_`,
/// and any uppercase letter that begins a word (`fooBar` -> 3; `HTTPServer` -> 0, 4, where an
/// uppercase followed by lowercase ends an acronym). Always ends with the identifier's length.
///
/// Both ends of a `_` run: `open_read_only` -> `open_read_only_with_config` appends text starting
/// *at* the underscore.
pub(crate) fn identifier_word_starts(text: &str) -> Vec<usize> {
    let characters: Vec<(usize, char)> = text.char_indices().collect();
    let mut starts = vec![0usize];
    for (index, &(offset, character)) in characters.iter().enumerate() {
        if index == 0 {
            continue;
        }
        let previous = characters[index - 1].1;
        let next = characters.get(index + 1).map(|&(_, character)| character);
        let underscore_edge = (character == '_') != (previous == '_');
        let starts_a_word = character.is_uppercase()
            && (!previous.is_uppercase() || next.is_some_and(char::is_lowercase));
        if underscore_edge || starts_a_word {
            starts.push(offset);
        }
    }
    starts.push(text.len());
    starts.dedup();
    starts
}

/// The part of `text` that differs from `other`, widened outward to whole identifier words: a
/// highlight split inside a word is not something a reader would draw (`calculateArea` -> `area`
/// widens to the whole of both, `EVENT_NEW_FRAME` -> `SC_EVENT_NEW_FRAME` gives `SC_`).
///
/// An empty span (`start == end`) means every byte of this side survives into the other.
pub(crate) fn differing_affix(text: &str, other: &str) -> (usize, usize) {
    let (prefix, end) = raw_affix(text, other);
    let starts = identifier_word_starts(text);
    let start = starts
        .iter()
        .rev()
        .find(|&&start| start <= prefix)
        .copied()
        .unwrap_or(0);
    let end = starts
        .iter()
        .find(|&&start| start >= end)
        .copied()
        .unwrap_or(text.len());
    (start, end.max(start))
}

/// The bare common-prefix/common-suffix span, before any widening. The common suffix is taken
/// first, placing an ambiguous run as far left as it goes: `last_packet_timestamp` ->
/// `last_filtered_packet_timestamp` yields `_filtered`, which is what the corpus paints.
fn raw_affix(text: &str, other: &str) -> (usize, usize) {
    let suffix: usize = text
        .chars()
        .rev()
        .zip(other.chars().rev())
        .take_while(|(ours, theirs)| ours == theirs)
        .map(|(ours, _)| ours.len_utf8())
        .sum();
    let end = text.len() - suffix;
    let prefix = text[..end]
        .char_indices()
        .zip(other[..other.len() - suffix].char_indices())
        .take_while(|((_, ours), (_, theirs))| ours == theirs)
        .last()
        .map_or(0, |((offset, ours), _)| offset + ours.len_utf8());
    (prefix, end.max(prefix))
}

/// Every span a `Minimal` painting may legitimately mark on this side of a rename: the differing
/// words (the widened span) or the differing characters (the bare affix). The corpus holds both
/// readings, and words alone misfire on case fixes (`PRIU64` -> `PRIu64`). What is rejected is
/// marking the unchanged rest of the identifier.
fn minimal_affix_candidates(text: &str, other: &str) -> Vec<(usize, usize)> {
    let widened = differing_affix(text, other);
    let (start, end) = raw_affix(text, other);
    let mut candidates = vec![widened];
    if (start, end) != widened {
        candidates.push((start, end.max(start)));
    }
    candidates
}

/// A leaf that is an identifier: a named leaf whose text reads as one, and not a keyword. Judged
/// by text shape, not a per-grammar list of kinds.
fn is_identifier_leaf(leaf: Node, contents: &str) -> bool {
    if leaf.child_count() != 0 || !is_named_leaf(leaf, contents) {
        return false;
    }
    let Some(text) = contents.get(leaf.byte_range()) else {
        return false;
    };
    let mut characters = text.chars();
    characters
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_' || first == '$')
        && characters
            .all(|character| character.is_alphanumeric() || character == '_' || character == '$')
}

/// **Invariant 16.** When the tree mapping pairs two identifiers that differ, `Minimal` paints
/// only the differing words or characters and `Full` paints the whole identifier on both sides.
///
/// The presets are two conventions (`text_painting_findings.md`, rule 1), and a rename is the edit
/// where they must differ. Only paintings named for a preset are checked: a lone painting is held
/// to both presets, whose halves of this rule contradict each other.
fn identifier_updates_are_painted_by_preset(
    painting: &str,
    painted: &PaintedLabels,
    context: &TreeContext,
    before: &Code,
    after: &Code,
) -> Vec<GroundTruthViolation> {
    let minimal = super::designates_preset(painting, "Minimal");
    let full = super::designates_preset(painting, "Full");
    if !minimal && !full {
        return Vec::new();
    }
    let mut violations = Vec::new();
    for leaf in &context.leaves[0] {
        let LeafStatus::Paired(partner) = context.status(*leaf, 0) else {
            continue;
        };
        if !is_identifier_leaf(*leaf, &before.contents)
            || !is_identifier_leaf(partner, &after.contents)
        {
            continue;
        }
        let (Some(ours), Some(theirs)) = (
            before.contents.get(leaf.byte_range()),
            after.contents.get(partner.byte_range()),
        ) else {
            continue;
        };
        if ours == theirs {
            continue;
        }
        for (side, node, text, other) in [
            (0usize, *leaf, ours, theirs),
            (1usize, partner, theirs, ours),
        ] {
            let contents = if side == 0 {
                &before.contents
            } else {
                &after.contents
            };
            let labels = &painted[side];
            if full {
                if whole_leaf_label(labels, node).is_some_and(|label| label.is_some()) {
                    continue;
                }
                violations.push(GroundTruthViolation::new(
                    16,
                    Some(painting),
                    format!(
                        "painting {painting:?} {} row {} does not paint the whole of {text:?}, \
                         which the mapping pairs with {other:?} - a Full painting marks a renamed \
                         identifier entire on both sides",
                        side_name(side),
                        row_of(contents, node.start_byte()),
                    ),
                    vec![ViolationSite {
                        side,
                        span: span_of_node(node),
                    }],
                ));
                continue;
            }
            let candidates = minimal_affix_candidates(text, other);
            let mut nearest: Option<(usize, usize, Vec<usize>, Vec<usize>)> = None;
            for (start, end) in candidates.iter().copied() {
                let painted_outside: Vec<usize> = (0..text.len())
                    .filter(|offset| !(start..end).contains(offset))
                    .filter(|offset| labels[node.start_byte() + offset].is_some())
                    .map(|offset| node.start_byte() + offset)
                    .collect();
                let unpainted_inside: Vec<usize> = (start..end)
                    .filter(|offset| labels[node.start_byte() + offset].is_none())
                    .map(|offset| node.start_byte() + offset)
                    .collect();
                if painted_outside.is_empty() && unpainted_inside.is_empty() {
                    nearest = None;
                    break;
                }
                let wrong = painted_outside.len() + unpainted_inside.len();
                if nearest
                    .as_ref()
                    .is_none_or(|(_, _, outside, inside)| wrong < outside.len() + inside.len())
                {
                    nearest = Some((start, end, painted_outside, unpainted_inside));
                }
            }
            // Any candidate matching exactly is agreement; the message names the closest one.
            let Some((start, end, painted_outside, unpainted_inside)) = nearest else {
                continue;
            };
            let wanted = if start == end {
                "nothing".to_string()
            } else {
                format!("{:?}", &text[start..end])
            };
            violations.push(GroundTruthViolation::new(
                16,
                Some(painting),
                format!(
                    "painting {painting:?} {} row {} paints {text:?} against {other:?} wrongly for \
                     a Minimal reading - the differing words are {wanted}, and everything else in \
                     the identifier should be left alone",
                    side_name(side),
                    row_of(contents, node.start_byte()),
                ),
                sites_from_offsets(
                    side,
                    contents,
                    &painted_outside
                        .into_iter()
                        .chain(unpainted_inside)
                        .collect::<Vec<usize>>(),
                ),
            ));
        }
    }
    violations
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// Invariant 17: a boolean that flipped is an edit to one token, not a deletion and an insertion
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// `true` against `false`, either way round.
fn opposite_boolean(text: &str) -> Option<&'static str> {
    match text {
        "true" => Some("false"),
        "false" => Some("true"),
        _ => None,
    }
}

/// **Invariant 17.** A `true` that became a `false` in the same place is one token edited, not a
/// token deleted and another inserted - in the tree mapping and in every painting.
///
/// * **mapping** - a boolean leaf the mapping removes, whose partner subtree (under ancestors the
///   mapping pairs) contains the opposite literal also removed. Correspondence, not
///   co-occurrence: a `true` deleted in one method and a `false` added in another is fine.
/// * **painting** - a boolean pair the mapping pairs, painted `Delete` on one side and `Insert`
///   on the other. Invariant 10 cannot reach these, since the two sides differ.
fn boolean_flips_are_one_edit(
    paintings: &[(&str, PaintedLabels)],
    context: &TreeContext,
    before: &Code,
    after: &Code,
) -> Vec<GroundTruthViolation> {
    let mut violations = Vec::new();
    for leaf in &context.leaves[0] {
        let Some(text) = before.contents.get(leaf.byte_range()) else {
            continue;
        };
        let Some(opposite) = opposite_boolean(text) else {
            continue;
        };

        if let LeafStatus::Paired(partner) = context.status(*leaf, 0)
            && after.contents.get(partner.byte_range()) == Some(opposite)
        {
            for (name, painted) in paintings {
                let ours = whole_leaf_label(&painted[0], *leaf);
                let theirs = whole_leaf_label(&painted[1], partner);
                if ours == Some(Some(TextLabel::Delete)) && theirs == Some(Some(TextLabel::Insert))
                {
                    violations.push(GroundTruthViolation::new(
                        17,
                        Some(name),
                        format!(
                            "painting {name:?} deletes {text:?} on before row {} and inserts \
                             {opposite:?} on after row {}, which the mapping pairs as one edited \
                             token",
                            row_of(&before.contents, leaf.start_byte()),
                            row_of(&after.contents, partner.start_byte()),
                        ),
                        vec![
                            ViolationSite {
                                side: 0,
                                span: span_of_node(*leaf),
                            },
                            ViolationSite {
                                side: 1,
                                span: span_of_node(partner),
                            },
                        ],
                    ));
                }
            }
            continue;
        }

        if !matches!(context.status(*leaf, 0), LeafStatus::Removed) {
            continue;
        }
        // The nearest ancestor the mapping pairs: the flip has to have happened *here*.
        let mut ancestor = Some(*leaf);
        let partner_subtree = loop {
            let Some(node) = ancestor else { break None };
            if let Some(partner) = context
                .caches
                .before_match
                .get(&node.id())
                .and_then(|id| context.ids[1].get(id))
            {
                break Some(*partner);
            }
            ancestor = node.parent();
        };
        let Some(partner_subtree) = partner_subtree else {
            continue;
        };
        let Some(twin) = context.leaves[1]
            .iter()
            .filter(|candidate| {
                partner_subtree.start_byte() <= candidate.start_byte()
                    && candidate.end_byte() <= partner_subtree.end_byte()
            })
            .find(|candidate| {
                after.contents.get(candidate.byte_range()) == Some(opposite)
                    && matches!(context.status(**candidate, 1), LeafStatus::Removed)
            })
        else {
            continue;
        };
        violations.push(GroundTruthViolation::new(
            17,
            None,
            format!(
                "mapping removes {text:?} on before row {} and adds {opposite:?} on after row {} \
                 inside the subtree it pairs with - one flipped literal, which an Update entry on \
                 the two says and a delete plus an insert does not",
                row_of(&before.contents, leaf.start_byte()),
                row_of(&after.contents, twin.start_byte()),
            ),
            vec![
                ViolationSite {
                    side: 0,
                    span: span_of_node(*leaf),
                },
                ViolationSite {
                    side: 1,
                    span: span_of_node(*twin),
                },
            ],
        ));
    }
    violations
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// Invariant 19: an operator, a boolean or an access modifier is painted whole
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// **Invariant 19.** Every byte of a token in [`WHOLE_TOKENS`] carries the same highlighting, in
/// every painting, on both sides. Each is one symbol to a reader: `<=` becoming `<` is a different
/// comparison, not a surviving `<`; `private` becoming `protected` keeps no `pr`. The presets may
/// disagree about *whether* such a token is painted, never about painting part of one. The list
/// is the renderer's own, which follows the same rule.
fn tokens_are_painted_whole(
    painting: &str,
    painted: &PaintedLabels,
    context: &TreeContext,
    before: &Code,
    after: &Code,
) -> Vec<GroundTruthViolation> {
    let mut violations = Vec::new();
    for (side, code) in [(0usize, before), (1usize, after)] {
        for leaf in &context.leaves[side] {
            let Some(text) = code.contents.get(leaf.byte_range()) else {
                continue;
            };
            if !WHOLE_TOKENS.contains(&text) || whole_leaf_label(&painted[side], *leaf).is_some() {
                continue;
            }
            violations.push(GroundTruthViolation::new(
                19,
                Some(painting),
                format!(
                    "painting '{painting}' paints only part of `{text}` on {} row {}",
                    side_name(side),
                    row_of(&code.contents, leaf.start_byte()),
                ),
                vec![ViolationSite {
                    side,
                    span: span_of_node(*leaf),
                }],
            ));
        }
    }
    violations
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// The assertions the per-fixture tests call
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// Asserts `name`'s ground truth contradicts itself in no way at all.
pub fn assert_ground_truth_invariants(name: &str) -> Result<()> {
    assert_ground_truth_invariants_with_known_violations(name, 0)
}

/// [`assert_ground_truth_invariants`] for a fixture with `expected` known violations.
///
/// **Exact, unlike the mapping and painting clamps**: those bound a distance, while this counts
/// specific contradictions, each described above the call. An over-estimate would stop checking
/// the moment one is repaired; the failure names the number to write instead.
pub fn assert_ground_truth_invariants_with_known_violations(
    name: &str,
    expected: usize,
) -> Result<()> {
    let violations = ground_truth_invariant_violations(name)?;
    let listed = violation_messages(&violations).join("\n  ");
    if violations.len() == expected {
        return Ok(());
    }
    if violations.len() < expected {
        anyhow::bail!(
            "'{name}' now breaks {} of its own ground-truth invariants, not the {expected} \
             recorded here - if that is a repair, record {} and update the note above this call \
             to describe what is left:\n  {listed}",
            violations.len(),
            violations.len(),
        );
    }
    anyhow::bail!(
        "'{name}' breaks {} of its own ground-truth invariants, more than the {expected} recorded \
         here:\n  {listed}",
        violations.len(),
    )
}

/// Each violation's message, numbered by the rule it comes from - what a failure prints.
pub fn violation_messages(violations: &[GroundTruthViolation]) -> Vec<String> {
    violations.iter().map(ToString::to_string).collect()
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

    #[test]
    fn identifier_words_split_on_case_and_underscores() {
        assert_eq!(identifier_word_starts("fooBar"), vec![0, 3, 6]);
        assert_eq!(
            identifier_word_starts("SC_EVENT_NEW_FRAME"),
            vec![0, 2, 3, 8, 9, 12, 13, 18]
        );
        // An acronym is one word, and hands off to the next at the capital before a lowercase.
        assert_eq!(identifier_word_starts("XMLHttpRequest"), vec![0, 3, 7, 14]);
        assert_eq!(identifier_word_starts("lowercase"), vec![0, 9]);
        assert_eq!(identifier_word_starts(""), vec![0]);
    }

    #[test]
    fn a_pure_prefix_insertion_paints_only_the_new_words() {
        // The shape `c-genymobile-scrcpy-rename-defines` renames five times over.
        assert_eq!(
            differing_affix("SC_EVENT_NEW_FRAME", "EVENT_NEW_FRAME"),
            (0, 3)
        );
        // An ambiguous run goes as far left as it will go: `_filtered` after `last`, not
        // `filtered_` after `last_`, as rust-gyulyvgc-sniffnet-rename-one-identifier paints it.
        assert_eq!(
            differing_affix("last_filtered_packet_timestamp", "last_packet_timestamp"),
            (4, 13)
        );
        let (start, end) =
            differing_affix("last_packet_timestamp", "last_filtered_packet_timestamp");
        assert_eq!(
            start, end,
            "the side that only lost it has nothing to paint"
        );
        // The appended text starts *at* the underscore, and the highlight stays that tight
        // rather than widening back to the previous word.
        assert_eq!(
            differing_affix("open_read_only_with_config", "open_read_only"),
            (14, 26)
        );
        // ... and nothing at all on the side that only lost it.
        let (start, end) = differing_affix("EVENT_NEW_FRAME", "SC_EVENT_NEW_FRAME");
        assert_eq!(start, end, "the unchanged side has no differing words");
    }

    #[test]
    fn a_single_word_identifier_accepts_the_bare_affix_or_the_whole_token() {
        // `align` -> `halign` and `v` -> `value` have nothing to orient on, so both readings pass.
        let candidates = minimal_affix_candidates("halign", "align");
        assert!(candidates.contains(&(0, 1)), "the added `h` alone");
        assert!(candidates.contains(&(0, 6)), "or the whole token");
        let candidates = minimal_affix_candidates("value", "v");
        assert!(candidates.contains(&(1, 5)));
        assert!(candidates.contains(&(0, 5)));
        // With a boundary to orient on, the word and the characters both pass.
        assert_eq!(
            minimal_affix_candidates("getDeclaredConstructors", "getDeclaredConstructor"),
            vec![(11, 23), (22, 23)]
        );
    }

    #[test]
    fn a_shared_run_inside_a_word_does_not_split_it() {
        // The bare shared suffix "rea" falls inside a word, so widening takes both whole.
        assert_eq!(differing_affix("calculateArea", "area"), (0, 13));
        assert_eq!(differing_affix("area", "calculateArea"), (0, 4));
        // A suffix added at a word boundary stays narrow.
        assert_eq!(differing_affix("ReplaceAll", "Replace"), (7, 10));
    }

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
        ground_truth_invariant_violations_for(mapping, before, after)
            .expect("checks run")
            .into_iter()
            .map(|violation| violation.message)
            .collect()
    }

    // ── Invariant 8 ─────────────────────────────────────────────────────────────────────────

    /// A `Match` over the same columns of both sides - byte-identical, so it resolves to `Move`.
    fn moved(at: usize, count: usize) -> HumanTextEntry {
        HumanTextEntry {
            operation: HumanTextOperation::Match,
            before: vec![span(0, at, 0, at + count)],
            after: vec![span(0, at, 0, at + count)],
        }
    }

    fn survival_violations(mapping: &HumanMapping, before: &Code, after: &Code) -> Vec<String> {
        violations(mapping, before, after)
            .into_iter()
            .filter(|v| v.contains("survive under one preset"))
            .collect()
    }

    #[test]
    fn a_byte_moved_under_one_preset_and_deleted_under_the_other_is_reported() {
        let source = rust("let value = 1;\n");
        let mapping = painted(vec![
            ("Minimal", vec![moved(4, 5)]),
            ("Full", vec![deleted(4, 5)]),
        ]);

        let reported = survival_violations(&mapping, &source, &source);
        assert_eq!(reported.len(), 1, "got {reported:?}");
        assert!(reported[0].contains("5 byte(s)"), "got {reported:?}");
    }

    /// The pair the presets are *expected* to differ on: `Full` widening a `Move` into the `Update`
    /// that contains it. Both readings agree the code survived, so there is nothing to report.
    #[test]
    fn a_move_widened_into_an_update_is_not_a_contradiction() {
        let before = rust("let value = 1;\n");
        let after = rust("let value = 2;\n");
        let updated = HumanTextEntry {
            operation: HumanTextOperation::Match,
            before: vec![span(0, 4, 0, 13)],
            after: vec![span(0, 4, 0, 13)],
        };
        let mapping = painted(vec![
            ("Minimal", vec![moved(4, 5)]),
            ("Full", vec![updated]),
        ]);

        assert!(
            survival_violations(&mapping, &before, &after).is_empty(),
            "a widening changes the colour, not whether the code is there"
        );
    }

    /// Alternatives are a disjunction: one `Full` reading contradicting a `Minimal` says nothing
    /// while another agrees with it.
    #[test]
    fn a_minimal_consistent_with_some_full_alternative_is_not_reported() {
        let source = rust("let value = 1;\n");
        let mapping = painted(vec![
            ("Minimal", vec![moved(4, 5)]),
            ("Full (left)", vec![deleted(4, 5)]),
            ("Full (right)", vec![moved(4, 5)]),
        ]);

        assert!(
            survival_violations(&mapping, &source, &source).is_empty(),
            "'Full (right)' agrees, which is all a disjunction needs"
        );
    }

    // ── Invariant 7 ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn two_ranges_claiming_the_same_byte_are_reported() {
        let before = rust("let value = 1;\n");
        let mapping = painted(vec![(
            "Only one solution",
            vec![deleted(4, 5), deleted(8, 3)],
        )]);

        let all = violations(&mapping, &before, &before);
        let reported: Vec<&String> = all
            .iter()
            .filter(|v| v.contains("already claimed"))
            .collect();
        assert_eq!(reported.len(), 1, "got {reported:?}");
        assert!(reported[0].contains("range 1"), "got {reported:?}");
    }

    /// A range ending at column 0 of the next row swallows the break, and nothing paints a line
    /// terminator.
    #[test]
    fn two_ranges_meeting_at_a_line_break_do_not_overlap() {
        let before = rust("let x = 1;\nlet y = 2;\n");
        let first = HumanTextEntry {
            operation: HumanTextOperation::Delete,
            before: vec![span(0, 0, 1, 0)],
            after: Vec::new(),
        };
        let second = HumanTextEntry {
            operation: HumanTextOperation::Delete,
            before: vec![span(1, 0, 1, 10)],
            after: Vec::new(),
        };
        let mapping = painted(vec![("Only one solution", vec![first, second])]);

        assert!(
            !violations(&mapping, &before, &before)
                .iter()
                .any(|v| v.contains("already claimed")),
            "sharing only the newline is not an overlap"
        );
    }

    /// A Windows break is two bytes, and neither is painted.
    #[test]
    fn a_crlf_break_between_two_ranges_is_not_an_overlap() {
        let before = rust("let x = 1;\r\nlet y = 2;\r\n");
        let first = HumanTextEntry {
            operation: HumanTextOperation::Delete,
            before: vec![span(0, 0, 1, 0)],
            after: Vec::new(),
        };
        let second = HumanTextEntry {
            operation: HumanTextOperation::Delete,
            before: vec![span(1, 0, 1, 10)],
            after: Vec::new(),
        };
        let mapping = painted(vec![("Only one solution", vec![first, second])]);

        assert!(
            !violations(&mapping, &before, &before)
                .iter()
                .any(|v| v.contains("already claimed")),
            "the CR belongs to the terminator, not to either range"
        );
    }

    /// Two paintings are alternatives, not conjuncts, so one may claim a byte the other claims.
    #[test]
    fn two_paintings_may_claim_the_same_byte_as_each_other() {
        let before = rust("let value = 1;\n");
        let mapping = painted(vec![
            ("Minimal", vec![deleted(4, 5)]),
            ("Full", vec![deleted(0, 9)]),
        ]);

        assert!(
            !violations(&mapping, &before, &before)
                .iter()
                .any(|v| v.contains("already claimed")),
            "the rule is within one painting, not across them"
        );
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

    /// A run stopping on a space with visible text still to come on the row is an ordinary edit
    /// (go-gin-gonic-gin-whitespace-in-comment).
    #[test]
    fn a_painted_run_that_ends_on_a_mid_row_space_is_accepted() {
        let before = rust("let x = 1;  // a  b\n");
        let after = rust("\n");
        // `a ` at columns 15..17 ends on the space at 16, with `b` still to come. It holds a
        // visible character, so the all-whitespace exemption does not mask the mid-row check.
        let mapping = painted(vec![("Only one solution", vec![deleted(15, 2)])]);

        assert!(
            violations(&mapping, &before, &after).is_empty(),
            "a mid-row space is not trailing whitespace"
        );
    }

    /// A run with no visible character is exempt, as a blank row is (a commit stripping trailing
    /// spaces).
    #[test]
    fn a_painted_run_that_is_entirely_whitespace_is_exempt() {
        let before = rust("let x = 1;  \n");
        let after = rust("let x = 1;\n");
        // Just the two trailing spaces: trailing, but with nothing visible inside the run.
        let mapping = painted(vec![("Only one solution", vec![deleted(10, 2)])]);

        assert!(
            violations(&mapping, &before, &after).is_empty(),
            "a run that is all whitespace has no visible character it could end on"
        );
    }

    #[test]
    fn a_row_with_nothing_visible_on_it_is_exempt() {
        // The painted row is all whitespace, so nothing on it could legally end a run.
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
        // Asked of the tree mapping alone: a painting may say the `{` went and the `}` stayed.
        let before = rust("fn f() { g(); }\n");
        let after = rust("\n");
        let mapping = painted(vec![("Only one solution", vec![deleted(7, 1)])]);

        assert!(violations(&mapping, &before, &after).is_empty());
    }

    #[test]
    fn only_a_leaf_whose_kind_is_its_own_text_is_a_delimiter() {
        // The second `(` is a `string_content` leaf that merely reads `(`, not a delimiter.
        let code = rust("fn f() { g(\"(\"); }\n");
        let pairs = delimiter_pairs(code.ast.as_ref().unwrap().root_node(), &code.contents);

        let mut kinds: Vec<(&str, usize)> = pairs
            .iter()
            .map(|(open, _)| (open.kind(), open.start_position().column))
            .collect();
        // Reported in stack order; only the set of pairs is the contract.
        kinds.sort_by_key(|(_, column)| *column);
        assert_eq!(
            kinds,
            vec![("(", 4), ("{", 7), ("(", 10)],
            "the `(` at column 12 is inside a string literal and must not be paired"
        );
    }

    // ── Invariant 9 ─────────────────────────────────────────────────────────────────────────

    fn survival_across_records(mapping: &HumanMapping, before: &Code, after: &Code) -> Vec<String> {
        violations(mapping, before, after)
            .into_iter()
            .filter(|v| v.contains("the tree mapping leaves unmatched"))
            .collect()
    }

    /// Label vectors rather than a built mapping: a synthetic pair contrived to make `TextDiff`
    /// emit a `Delete` would test the renderer instead of the rule.
    fn labels(before_len: usize, after_len: usize) -> PaintedLabels {
        [vec![None; before_len], vec![None; after_len]]
    }

    /// An `unmatched_bytes` mask saying every byte is unmatched - the gate held open.
    fn all_unmatched(before_len: usize, after_len: usize) -> [Vec<bool>; 2] {
        [vec![true; before_len], vec![true; after_len]]
    }

    #[test]
    fn a_painted_move_over_a_node_the_mapping_deletes_is_reported() {
        let source = rust("let value = 1;\n");
        let mut painted = labels(source.contents.len(), source.contents.len());
        let mut tree = labels(source.contents.len(), source.contents.len());
        for offset in 4..9 {
            painted[0][offset] = Some(TextLabel::Move);
            tree[0][offset] = Some(TextLabel::Delete);
        }

        let unmatched = all_unmatched(source.contents.len(), source.contents.len());
        let reported =
            move_against_unmatched("Minimal", &painted, &tree, &unmatched, &source, &source);
        assert_eq!(reported.len(), 1, "got {reported:#?}");
        assert!(
            reported[0].message.contains("5 byte(s) Move"),
            "got {reported:#?}"
        );
        assert!(reported[0].message.contains("Delete"), "got {reported:#?}");
        assert_eq!(reported[0].invariant, 9);
        // The five bytes are contiguous, so they collapse to one site rather than five.
        assert_eq!(
            reported[0].sites,
            vec![site_on_row(0, 0, 4, 9)],
            "got {reported:#?}"
        );
    }

    #[test]
    fn a_painted_move_over_a_node_the_mapping_inserts_is_reported() {
        let source = rust("let value = 1;\n");
        let mut painted = labels(source.contents.len(), source.contents.len());
        let mut tree = labels(source.contents.len(), source.contents.len());
        painted[1][4] = Some(TextLabel::Move);
        tree[1][4] = Some(TextLabel::Insert);

        let unmatched = all_unmatched(source.contents.len(), source.contents.len());
        let reported =
            move_against_unmatched("Full", &painted, &tree, &unmatched, &source, &source);
        assert_eq!(reported.len(), 1, "got {reported:#?}");
        assert!(reported[0].message.contains("after"), "got {reported:#?}");
        assert_eq!(reported[0].sites, vec![site_on_row(1, 0, 4, 5)]);
    }

    /// The renderer says these bytes were deleted, but they sit in a node the mapping *matched*: a
    /// character edited out of a surviving leaf (`rust-rust-lang-rust-update-comment`).
    #[test]
    fn a_painted_move_the_renderer_deletes_from_inside_a_matched_node_is_not_reported() {
        let source = rust("let value = 1;\n");
        let mut painted = labels(source.contents.len(), source.contents.len());
        let mut tree = labels(source.contents.len(), source.contents.len());
        for offset in 4..9 {
            painted[0][offset] = Some(TextLabel::Move);
            tree[0][offset] = Some(TextLabel::Delete);
        }
        // Everything else is as in `a_painted_move_over_a_node_the_mapping_deletes_is_reported`,
        // which reports it; the mask is the only difference.
        let unmatched = [
            vec![false; source.contents.len()],
            vec![false; source.contents.len()],
        ];
        assert!(
            move_against_unmatched("Minimal", &painted, &tree, &unmatched, &source, &source)
                .is_empty(),
            "a tree-side Delete inside a matched node is the renderer, not a missing counterpart"
        );
    }

    /// The smallest node containing a byte decides: a matched node inside a deleted subtree reads
    /// as matched, while whitespace between that subtree's children keeps the deleted answer.
    #[test]
    fn unmatched_bytes_lets_a_matched_node_override_the_subtree_deleted_around_it() {
        let before = rust("fn f() { g(); }\n");
        let root = before.ast.as_ref().unwrap().root_node();
        let find = |text: &str| {
            let mut stack = vec![root];
            while let Some(node) = stack.pop() {
                if node.child_count() == 0 && &before.contents[node.byte_range()] == text {
                    return node;
                }
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    stack.push(child);
                }
            }
            panic!("no leaf reads {text:?}");
        };
        let block = find("{").parent().expect("the brace's block");
        assert_eq!(block.kind(), "block");
        let call = find("g");

        let mut caches = Caches::default();
        // The whole block deleted, children included - so everything under it inherits the mark -
        // and one leaf inside it matched anyway, which is the case being drawn.
        caches.before_removed.insert(block.id(), true);
        caches.before_match.insert(call.id(), call.id());

        let mask = unmatched_bytes(root, before.contents.len(), 0, &[], &caches);
        assert!(
            mask[block.start_byte()],
            "the deleted block's own brace inherits the mark"
        );
        assert!(
            mask[block.start_byte() + 1],
            "and so does the space after it, which no node of its own covers"
        );
        assert!(
            !mask[call.start_byte()],
            "but the leaf the mapping matched does not"
        );
        assert!(
            !mask[0],
            "and nothing outside the block is touched - `fn` is unmarked, which is not unmatched"
        );
    }

    /// Not reported: a tree-side `Move` comes from `TextDiff::from`'s column-shift heuristic, so
    /// against a painted `Delete` it measures the renderer.
    #[test]
    fn a_painted_delete_over_a_node_the_renderer_calls_moved_is_not_reported() {
        let source = rust("let value = 1;\n");
        let mut painted = labels(source.contents.len(), source.contents.len());
        let mut tree = labels(source.contents.len(), source.contents.len());
        painted[0][4] = Some(TextLabel::Delete);
        tree[0][4] = Some(TextLabel::Move);

        assert!(
            move_against_unmatched(
                "Minimal",
                &painted,
                &tree,
                &all_unmatched(source.contents.len(), source.contents.len()),
                &source,
                &source
            )
            .is_empty(),
            "only a painted Move against an unmatched node is a contradiction"
        );
    }

    /// The two records differing about *granularity* is the ordinary case, not a contradiction.
    #[test]
    fn a_painted_move_the_mapping_calls_updated_is_not_reported() {
        let source = rust("let value = 1;\n");
        let mut painted = labels(source.contents.len(), source.contents.len());
        let mut tree = labels(source.contents.len(), source.contents.len());
        painted[0][4] = Some(TextLabel::Move);
        tree[0][4] = Some(TextLabel::Update);

        assert!(
            move_against_unmatched(
                "Minimal",
                &painted,
                &tree,
                &all_unmatched(source.contents.len(), source.contents.len()),
                &source,
                &source
            )
            .is_empty(),
            "an Update says the node survived too"
        );
    }

    /// A fixture with one ground truth and not the other has nothing to compare.
    #[test]
    fn a_fixture_with_no_tree_mapping_is_skipped() {
        let source = rust("fn f() { g(); }\n");
        let mapping = painted(vec![(
            "Only one solution",
            vec![HumanTextEntry {
                operation: HumanTextOperation::Match,
                before: vec![span(0, 9, 0, 10)],
                after: vec![span(0, 9, 0, 10)],
            }],
        )]);

        assert!(survival_across_records(&mapping, &source, &source).is_empty());
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

    /// Every leaf in `code` whose text is exactly `text` and whose parent is `parent`, in document
    /// order. The parent is what keeps `fn f()`'s own parentheses out of a group about a call's.
    fn leaves_reading<'a>(code: &'a Code, text: &str, parent: &str) -> Vec<Node<'a>> {
        let root = code.ast.as_ref().unwrap().root_node();
        let mut found = Vec::new();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if node.child_count() == 0
                && &code.contents[node.byte_range()] == text
                && node
                    .parent()
                    .is_some_and(|node_parent| node_parent.kind() == parent)
            {
                found.push(node);
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
        }
        found.sort_by_key(Node::start_byte);
        found
    }

    #[test]
    fn a_delimiter_whose_group_leaves_it_free_contradicts_nothing() {
        // One call becomes two nested calls: 1:2 groups for `(` and for `)`. Pairing outer with
        // outer is consistent, but `representative_entries` sorts by start byte and takes the
        // outer opener with the *inner* closer, splitting both pairs.
        let before = rust("fn f() { g(x); }\n");
        let after = rust("fn f() { g(h(x)); }\n");

        let mut groups = Vec::new();
        for delimiter in ["(", ")"] {
            groups.push(super::super::MultiMapGroup {
                before_paths: leaves_reading(&before, delimiter, "arguments")
                    .iter()
                    .map(|node| path_for_node(*node))
                    .collect(),
                after_paths: leaves_reading(&after, delimiter, "arguments")
                    .iter()
                    .map(|node| path_for_node(*node))
                    .collect(),
                operation: HumanOperation::Identical,
                with_children: false,
                pairing: super::super::GroupPairing::AnyOneToOne,
            });
        }
        assert_eq!(groups[0].before_paths.len(), 1);
        assert_eq!(
            groups[0].after_paths.len(),
            2,
            "the after side nests two calls"
        );

        let mapping = HumanMapping {
            groups,
            ..Default::default()
        };

        assert!(
            violations(&mapping, &before, &after).is_empty(),
            "a member the group could leave over states nothing to contradict"
        );
    }

    #[test]
    fn an_all_to_all_group_member_is_a_claim_under_every_reading() {
        // One statement duplicated under an all-to-all group: a leaf inside either copy has a
        // twin, so nothing is undecided and nothing is objected to.
        let before = rust("fn f() { g(); }\n");
        let after = rust("fn f() { g(); g(); }\n");
        let before_root = before.ast.as_ref().unwrap().root_node();
        let after_root = after.ast.as_ref().unwrap().root_node();
        fn statements<'t>(root: Node<'t>) -> Vec<Node<'t>> {
            let body = root
                .child(0)
                .and_then(|f| f.child_by_field_name("body"))
                .expect("fn f has a body");
            let mut cursor = body.walk();
            body.named_children(&mut cursor).collect()
        }
        let before_statements = statements(before_root);
        let after_statements = statements(after_root);
        assert_eq!((before_statements.len(), after_statements.len()), (1, 2));

        let group = |pairing| super::super::MultiMapGroup {
            before_paths: before_statements
                .iter()
                .map(|n| path_for_node(*n))
                .collect(),
            after_paths: after_statements.iter().map(|n| path_for_node(*n)).collect(),
            operation: HumanOperation::Identical,
            with_children: true,
            pairing,
        };
        fn leaf_of<'t>(statement: Node<'t>) -> Node<'t> {
            let mut node = statement;
            while let Some(first) = node.child(0) {
                node = first;
            }
            node
        }

        let open = HumanMapping {
            groups: vec![group(super::super::GroupPairing::AnyOneToOne)],
            ..Default::default()
        };
        let context = TreeContext::build(&open, before_root, after_root);
        let undecided = (0..2)
            .filter(|i| {
                matches!(
                    context.status(leaf_of(after_statements[*i]), 1),
                    LeafStatus::Undecided
                )
            })
            .count();
        assert_eq!(
            undecided, 1,
            "one copy is the leftover the group leaves open"
        );

        let all = HumanMapping {
            groups: vec![group(super::super::GroupPairing::AllToAll)],
            ..Default::default()
        };
        let context = TreeContext::build(&all, before_root, after_root);
        for statement in &after_statements {
            assert!(
                matches!(context.status(leaf_of(*statement), 1), LeafStatus::Same(_)),
                "every copy's leaf has a twin in the original"
            );
        }
        assert!(
            violations(&all, &before, &after).is_empty(),
            "{:#?}",
            violations(&all, &before, &after)
        );
    }

    #[test]
    fn a_delimiter_its_group_pins_is_still_checked() {
        // A 1:1 group matches its only member under every pairing it admits, so "matched" here is
        // a claim the group really makes - and an entry deleting the partner contradicts it.
        let before = rust("fn f() { g(); }\n");
        let after = rust("fn f() { g(); }\n");

        let open = leaves_reading(&before, "(", "arguments");
        let after_open = leaves_reading(&after, "(", "arguments");
        let close = leaves_reading(&before, ")", "arguments");
        let mapping = HumanMapping {
            entries: vec![HumanMappingEntry {
                operation: HumanOperation::Delete,
                before_path: Some(path_for_node(close[0])),
                after_path: None,
            }],
            groups: vec![super::super::MultiMapGroup {
                before_paths: vec![path_for_node(open[0])],
                after_paths: vec![path_for_node(after_open[0])],
                operation: HumanOperation::Identical,
                with_children: false,
                pairing: super::super::GroupPairing::AnyOneToOne,
            }],
            ..Default::default()
        };

        let found = violations(&mapping, &before, &after);
        assert!(
            found
                .iter()
                .any(|v| v.contains("as matched but its matching")),
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

    // ── Invariants 10-15 ────────────────────────────────────────────────────────────────────

    /// A mapping built from the two texts alone: equal subtrees are `Identical`, differing leaves
    /// of one kind are `Update`, differing interior nodes are `MatchButNotIdentical` with their
    /// children paired by index, and leftover children are `Delete`/`Insert`.
    fn mapping_by_text(before: &Code, after: &Code) -> HumanMapping {
        fn walk(b: Node, a: Node, before: &str, after: &str, entries: &mut Vec<HumanMappingEntry>) {
            let entry = |operation, b: Option<Node>, a: Option<Node>| HumanMappingEntry {
                operation,
                before_path: b.map(path_for_node),
                after_path: a.map(path_for_node),
            };
            if before[b.byte_range()] == after[a.byte_range()] {
                entries.push(entry(HumanOperation::Identical, Some(b), Some(a)));
                return;
            }
            if b.child_count() == 0 && a.child_count() == 0 {
                entries.push(entry(HumanOperation::Update, Some(b), Some(a)));
                return;
            }
            entries.push(entry(
                HumanOperation::MatchButNotIdentical,
                Some(b),
                Some(a),
            ));
            let shared = b.child_count().min(a.child_count());
            for index in 0..shared {
                walk(
                    b.child(index).unwrap(),
                    a.child(index).unwrap(),
                    before,
                    after,
                    entries,
                );
            }
            for index in shared..b.child_count() {
                entries.push(entry(
                    HumanOperation::DeleteWithChildren,
                    Some(b.child(index).unwrap()),
                    None,
                ));
            }
            for index in shared..a.child_count() {
                entries.push(entry(
                    HumanOperation::InsertWithChildren,
                    None,
                    Some(a.child(index).unwrap()),
                ));
            }
        }
        let mut entries = Vec::new();
        walk(
            before.ast.as_ref().unwrap().root_node(),
            after.ast.as_ref().unwrap().root_node(),
            &before.contents,
            &after.contents,
            &mut entries,
        );
        HumanMapping {
            entries,
            ..Default::default()
        }
    }

    fn with_painting(
        mut mapping: HumanMapping,
        name: &str,
        entries: Vec<HumanTextEntry>,
    ) -> HumanMapping {
        mapping.text_mappings.push(NamedTextMapping {
            name: name.to_string(),
            mapping: HumanTextMapping { entries },
        });
        mapping
    }

    fn inserted(at: usize, count: usize) -> HumanTextEntry {
        HumanTextEntry {
            operation: HumanTextOperation::Insert,
            before: Vec::new(),
            after: vec![span(0, at, 0, at + count)],
        }
    }

    fn matched(before: (usize, usize), after: (usize, usize)) -> HumanTextEntry {
        HumanTextEntry {
            operation: HumanTextOperation::Match,
            before: vec![span(0, before.0, 0, before.1)],
            after: vec![span(0, after.0, 0, after.1)],
        }
    }

    fn violations_mentioning(
        mapping: &HumanMapping,
        before: &Code,
        after: &Code,
        phrase: &str,
    ) -> Vec<String> {
        violations(mapping, before, after)
            .into_iter()
            .filter(|v| v.contains(phrase))
            .collect()
    }

    #[test]
    fn a_paired_leaf_painted_gone_and_new_is_reported() {
        let (before, after) = (rust("let x = 1;\n"), rust("let x = 1;\n"));
        let mapping = with_painting(
            mapping_by_text(&before, &after),
            "Full",
            vec![deleted(4, 1), inserted(4, 1)],
        );
        let found = violations_mentioning(
            &mapping,
            &before,
            &after,
            "gone on one side and new on the other",
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].contains("on before row 1 and after row 1 - the first is `x`"),
            "{found:?}"
        );
    }

    #[test]
    fn a_paired_leaf_painted_gone_on_one_side_only_is_not_reported() {
        let (before, after) = (rust("let x = 1;\n"), rust("let x = 1;\n"));
        let mapping = with_painting(
            mapping_by_text(&before, &after),
            "Full",
            vec![deleted(4, 1)],
        );
        assert!(violations_mentioning(&mapping, &before, &after, "gone on one side").is_empty());
    }

    #[test]
    fn a_removed_named_leaf_nobody_painted_is_reported_and_punctuation_is_not() {
        let (before, after) = (rust("f(x, y);\n"), rust("f(x);\n"));
        let mapping = with_painting(mapping_by_text(&before, &after), "Full", Vec::new());
        let found =
            violations_mentioning(&mapping, &before, &after, "removed leaf/leaves unpainted");
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].contains(
                "before leaves 1 removed leaf/leaves unpainted on row 1, the first \"identifier\" `y`"
            ),
            "{found:?}"
        );
    }

    #[test]
    fn a_removed_leaf_with_one_painted_byte_is_not_reported() {
        let (before, after) = (rust("f(x, y);\n"), rust("f(x);\n"));
        let mapping = with_painting(
            mapping_by_text(&before, &after),
            "Full",
            vec![deleted(3, 3)],
        );
        assert!(
            violations_mentioning(&mapping, &before, &after, "removed leaf/leaves unpainted")
                .is_empty()
        );
    }

    #[test]
    fn an_edited_leaf_painted_on_neither_side_is_reported() {
        let (before, after) = (rust("let x = 1;\n"), rust("let x = 2;\n"));
        let mapping = with_painting(mapping_by_text(&before, &after), "Minimal", Vec::new());
        let found = violations_mentioning(&mapping, &before, &after, "edited leaf/leaves");
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].contains("on before row 1 and after row 1 - the first is `1` becoming `2`"),
            "{found:?}"
        );
    }

    #[test]
    fn an_edited_leaf_painted_on_one_side_is_enough() {
        let (before, after) = (rust("let x = 1;\n"), rust("let x = 2;\n"));
        let mapping = with_painting(
            mapping_by_text(&before, &after),
            "Minimal",
            vec![inserted(8, 1)],
        );
        assert!(violations_mentioning(&mapping, &before, &after, "edited leaf/leaves").is_empty());
    }

    #[test]
    fn a_painted_edit_over_an_all_identical_mapping_is_reported() {
        let (before, after) = (rust("let x = 1;\n"), rust("let x = 2;\n"));
        let mapping = with_painting(
            mapping_by_text(&before, &before),
            "Minimal",
            vec![matched((8, 9), (8, 9))],
        );
        let found = violations_mentioning(
            &mapping,
            &before,
            &after,
            "every entry of the tree mapping is Identical",
        );
        assert_eq!(found.len(), 1, "{found:?}");
    }

    #[test]
    fn a_whitespace_only_painting_over_an_all_identical_mapping_is_not_reported() {
        let (before, after) = (rust("let x = 1;\n"), rust("let x =  1;\n"));
        let mapping = with_painting(
            mapping_by_text(&before, &after),
            "Minimal",
            vec![matched((6, 9), (6, 10))],
        );
        assert!(
            violations_mentioning(&mapping, &before, &after, "every entry of the tree mapping")
                .is_empty()
        );
    }

    #[test]
    fn an_identical_entry_over_differing_tokens_is_reported() {
        let (before, after) = (rust("let x = 1;\n"), rust("let y = 1;\n"));
        // The mapping built from `before` alone pairs every node with itself by path, so it is
        // all `Identical` - and wrong about the root once the after side reads `y`.
        let mapping = mapping_by_text(&before, &before);
        let found = violations_mentioning(&mapping, &before, &after, "tokens differ");
        assert!(!found.is_empty(), "{found:?}");
        assert!(found[0].contains("`x` against `y`"), "{found:?}");
    }

    #[test]
    fn an_identical_entry_over_a_whitespace_difference_is_not_reported() {
        let (before, after) = (rust("let x = 1;\n"), rust("let x =  1;\n"));
        let mapping = mapping_by_text(&before, &before);
        assert!(violations_mentioning(&mapping, &before, &after, "tokens differ").is_empty());
    }

    #[test]
    fn a_match_but_not_identical_over_byte_identical_subtrees_is_reported() {
        let (before, after) = (rust("let x = 1;\n"), rust("let x = 1;\n"));
        let mut mapping = mapping_by_text(&before, &after);
        mapping.entries[0].operation = HumanOperation::MatchButNotIdentical;
        let found = violations_mentioning(&mapping, &before, &after, "read byte-identically");
        assert_eq!(found.len(), 1, "{found:?}");
    }

    #[test]
    fn a_match_but_not_identical_whose_subtrees_differ_is_the_expected_shape() {
        let (before, after) = (rust("let x = 1;\n"), rust("let x = 2;\n"));
        let mapping = mapping_by_text(&before, &after);
        assert!(
            violations_mentioning(&mapping, &before, &after, "read byte-identically").is_empty()
        );
    }
}
