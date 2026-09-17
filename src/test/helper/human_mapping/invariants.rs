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
//! codediff against a claim its author would not defend if it were pointed out. These fifteen
//! invariants are that missing half: they can fail only because the hand-authored data disagrees
//! with itself.
//!
//! * [`rows_end_on_visible_characters`] - no painted run may end in a row's *trailing*
//!   whitespace.
//! * [`full_painting_covers_minimal`] - every byte painted under `Minimal` is painted under `Full`.
//! * [`delimiter_pairs_agree`] - a bracket and its partner carry one verdict in the tree mapping.
//! * [`full_paints_a_wholly_changed_line_whole`] - a `Full` line whose every visible character is
//!   inserted, or every one deleted, has no unpainted byte before its last visible character.
//! * [`no_unpainted_whitespace_between_painted_regions`] - a `Full` painting never breaks one
//!   highlight in two over whitespace.
//! * [`minimal_never_paints_leading_whitespace`] - a `Minimal` painting never claims a line's
//!   indentation, which is the mirror of the rule above it.
//! * [`painted_ranges_do_not_overlap`] - no two ranges of one painting claim the same byte.
//! * [`presets_agree_on_what_survives`] - a byte one preset paints `Move` is never painted
//!   `Insert` or `Delete` by the other.
//! * [`mapping_and_painting_agree_on_what_survives`] - a byte the painting paints `Move` is never
//!   one the tree mapping leaves unmatched.
//! * [`paired_leaves_are_not_deleted_and_inserted`] - a leaf the mapping pairs with a
//!   byte-identical leaf is never painted `Delete` while its partner is painted `Insert`.
//! * [`removed_leaves_are_painted`] - a named leaf the mapping deletes or inserts has at least one
//!   painted byte.
//! * [`edited_leaves_are_painted`] - a leaf the mapping pairs with a leaf that reads differently is
//!   painted on at least one side.
//! * [`painting_implies_mapping_edits`] - a painting that records an edit belongs to a mapping
//!   that records one too.
//! * [`identical_entries_are_token_identical`] - an `Identical` entry's two subtrees carry the same
//!   tokens.
//! * [`match_but_not_identical_entries_differ`] - a `MatchButNotIdentical` entry's two subtrees do
//!   not read byte-identically with every descendant paired inside.
//!
//! Invariants 4 and 5 were added on 2026-09-08 and wired in the same day, at **zero violations
//! across all 249 painted fixtures** - so unlike the first three they arrived with no clamped
//! fixtures behind them. Both are scoped to the paintings `FULL` is answerable to, via
//! [`paintings_with_labels`]; `MINIMAL` is the tight reading and is free to leave whitespace
//! alone, which is what invariant 6 states in its own right.
//!
//! Invariant 7 arrived on 2026-09-13 with seven `handmade` fixtures behind it, the only candidate
//! of eleven measured that day that fired anywhere at all. Invariant 8 followed it the same day at
//! **zero violations**, and is not vacuous for it: 285 fixtures carry both a `Minimal` and a `Full`
//! painting, 2,653 bytes are painted `Move` by both of them, and not one of those is called
//! `Insert` or `Delete` by the other preset. Invariant 9 is the same question asked across the two
//! *ground truths* rather than across the two presets, and is the only rule here that compares them
//! at all: they are expected to differ about how one edit is chunked, which is what the paper
//! reports, and this is the one thing they cannot differ about. Four fixtures break it - five
//! until 2026-09-14, when the fifth turned out to be the rule's own false positive rather than a
//! contradiction in the data; see `unmatched_bytes`.
//!
//! Invariants 10 to 15 read the tree mapping through [`Caches`] rather than through the renderer -
//! so, unlike invariant 9, they can ask about the tree side's *pairs* and not only its rendered
//! labels, and none of them inherits the column-shift `Move` artifact that limits 9 to one
//! direction. Three cross the two ground truths at the leaf (10, 11, 12), one at the whole
//! fixture (13), and two hold the mapping to itself (14, 15). Invariant 14 arrived at zero
//! violations and is not vacuous: 159 `Identical` subtrees across 12 fixtures differ in
//! whitespace, which is why it compares tokens and not text. The census also measured, and
//! rejected, delimiter agreement *within a painting* (a `}` legitimately moves while its `{` stays
//! put) and "a matched pair lands in one painting entry" (the ordinary `Delete`+`Insert`
//! chunking of a rename, 99 fixtures).
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

/// Where a violation is, on one side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViolationSite {
    /// 0 = before, 1 = after - the same side convention `TextDiff::all` and `PaintedLabels` use.
    pub side: usize,
    /// **0-based rows, byte columns** - the [`HumanTextSpan`] convention, which is what a painting
    /// stores on disk and what `human_solver`'s text cursor addresses. The *messages* alongside
    /// say 1-based rows instead, because that is what a file's gutter shows a reader. Anything
    /// that consumes a site programmatically wants the former and anything that reads a message
    /// wants the latter, so both exist rather than one being converted at every call.
    pub span: HumanTextSpan,
}

/// One way a fixture's ground truth contradicts itself: which rule, what it says, and where to
/// look.
///
/// The locations exist so a *tool* can act on the violation - `human_solver`'s `V` popup puts both
/// trees and both text panels on a site - which a sentence with a row number in it cannot support.
/// Every rule already holds this data while it builds its message (nodes for 3, 14 and 15, byte
/// offsets for 2, 8 and 9, rows and columns for 1, 4, 5 and 6, raw spans for 7 and 13, leaves for
/// 10, 11 and 12), so carrying it out is bookkeeping rather than a second analysis.
#[derive(Debug, Clone)]
pub struct GroundTruthViolation {
    /// Which of the fifteen rules, numbered as the module doc lists them.
    pub invariant: u8,
    /// The painting this is about, or `None` for the three rules that read only the tree mapping.
    pub painting: Option<String>,
    /// The human-readable line - what a test failure prints and what the popup lists.
    pub message: String,
    /// Every place to look, with runs of contiguous bytes already collapsed into one span each.
    pub sites: Vec<ViolationSite>,
}

impl std::fmt::Display for GroundTruthViolation {
    /// `[9] painting 'Full' before paints ...` - the rule's number, then its sentence.
    ///
    /// The number is not part of `message` because `human_solver`'s popup shows it in a column of
    /// its own, where a repeated `[9]` in the text beside it would be noise. Everything that
    /// prints a violation as one line wants it, so it is added here rather than at each of them.
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

/// At most this many sites are carried per violation.
///
/// Capped for the same reason the row list in a message is: a violation can cover hundreds of
/// runs, and neither a popup nor a struct is improved by holding every one. The message's own
/// count stays exact, so nothing is hidden - this bounds only how many of them can be jumped to.
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

/// The span a node occupies. `tree_sitter::Point::column` is already a byte offset into its row,
/// which is the one thing that makes this a rename rather than a conversion.
fn span_of_node(node: Node) -> HumanTextSpan {
    HumanTextSpan {
        start_row: node.start_position().row,
        start_column: node.start_position().column,
        end_row: node.end_position().row,
        end_column: node.end_position().column,
    }
}

/// Byte offsets on one side as sites, with contiguous offsets collapsed into one span each.
///
/// A per-byte site list is the wrong shape for every consumer: invariant 9's 60 bytes on
/// `rust-next-font-imports-generator` are four runs of indentation, and a popup offering sixty
/// jumps to four places is a popup nobody can use. `offsets` is expected in ascending order,
/// which every caller produces by scanning a label vector forwards.
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

/// What invariants 2 and 8 carry while scoring one `Minimal` alternative against every `Full` one.
///
/// Both rules report only the closest counterpart, because the alternatives are a disjunction -
/// see [`full_painting_covers_minimal`] - so both keep a best-so-far and everything needed to
/// describe it. They differ only in what counts as a disagreement, which is why the bookkeeping is
/// one type: invariant 2 names the `Full` painting in its message and has no example to give,
/// invariant 8 gives an example that already names both paintings and so never reads the name.
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
///
/// **This is the one `human_solver` must call**, never the by-name form: that one reads
/// `human_mapping.json` off disk, and the solver's mapping is in memory and usually unsaved, so a
/// disk read would report violations the reader has just repaired.
pub fn ground_truth_invariant_violations_for(
    mapping: &super::HumanMapping,
    before: &Code,
    after: &Code,
) -> Result<Vec<GroundTruthViolation>> {
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
    // Reads the spans rather than the labels, so it takes the paintings themselves - see
    // `painted_ranges_do_not_overlap` on why the projection cannot answer this one.
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
    }
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

/// No painted run may end in a row's **trailing** whitespace.
///
/// Highlighted trailing whitespace is a stripe of colour hanging off the end of a line, pointing
/// at nothing - it says "something happened here" about a region with no content to have happened
/// to.
///
/// Trailing is the operative word, and until 2026-09-11 this checked only whether the row's last
/// painted byte was whitespace - which also condemned every run that stops on a space *in the
/// middle* of a line. Those are the ordinary shape of an edit, not a stripe hanging off anything:
/// deleting one of the two spaces in `Team.  All` paints a single mid-row space, and deleting
/// `foo ` from `foo bar` paints a run ending on one. Both are now accepted, as is a run that is
/// *entirely* whitespace - it has no visible character it could have ended on, so there is no
/// spelling of it this rule would take.
///
/// The rendering side of this was fixed in the product on 2026-08-31 (`columns_on_row` bounds
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
            // Whitespace only counts if it is *trailing* - nothing visible after it on the row.
            // A painted run that stops on a space in the middle of a line is the ordinary shape
            // of an edit, not a defect: deleting one of two spaces from `Team.  All` paints a
            // single mid-row space, and deleting `foo ` from `foo bar` paints a run ending on
            // one. Neither renders as a stripe hanging off the end of anything, which is the
            // whole complaint this rule exists to make.
            if line[boundary..].chars().any(|c| !c.is_whitespace()) {
                continue;
            }
            // A run that is *all* whitespace has no visible character it could have ended on, so
            // there is no painting of it this rule would accept. Same exemption, and the same
            // reason, as the blank-row skip above: condemning every possible spelling of an edit
            // is not a useful thing for an invariant to do. A commit that strips trailing spaces
            // is exactly this shape.
            // Both ends are snapped to character boundaries before slicing: `last` is a *byte*
            // index and either end can land inside a multi-byte character (163 corpus files are
            // not ASCII).
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
) -> Result<Vec<GroundTruthViolation>> {
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
///
/// `pub` for `human_solver`, which asks the same question about the painting being edited: invariant
/// 6 below is a rule it can keep for the painter rather than report afterwards (see
/// `action_paint_one_sided`'s leading-whitespace split).
pub fn designates_minimal(name: &str) -> bool {
    super::designates_preset(name, "Minimal")
}

/// Whether a painting's name declares it the `Full` reading. See [`designates_minimal`].
///
/// `pub` for the same reason that one is: `human_solver` asks both questions when it branches a
/// painting, because invariant 4 below is the rule it can keep for the painter on the way from one
/// preset to the other.
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
/// **The condition is "all of them", not "the first one".** Asking only whether the first visible
/// character is `Insert`/`Delete` and requiring the indentation to match is a different and wrong
/// claim, because a *surviving* line can begin with an inserted token and keep every space after
/// it untouched, which no rule should forbid. Requiring the whole line to be one verdict before
/// saying anything about its whitespace is what makes the conclusion follow: there is nothing on
/// the line that survived, so there is nothing for the unpainted whitespace to belong to.
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

/// The three preset-scoped whitespace rules, as `(invariant 4, invariant 5, invariant 6)`.
///
/// [`ground_truth_invariant_violations_for`] calls this and flattens all three into its own list;
/// they stay separate here so a caller sweeping the corpus can say which of the three a fixture is
/// failing rather than only that it failed. The first two read
/// the paintings `FULL` answers to and the third those `MINIMAL` answers to, which on a
/// two-painting fixture are different objects entirely.
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
    // A fixture painted once is asserting that its rendering is *unambiguous*, not that it follows
    // `Full`'s conventions; `paintings_for_mode` hands that one painting to both presets as a
    // grading convenience, and invariant 5 reading it as a `Full` painting is the check borrowing
    // an intention the painter never expressed. `Full`'s "account for every byte whose role
    // changed" is exactly the convention a single painting declines to pick.
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
/// other.
///
/// `Move` is the one verdict that asserts the bytes *survive*: a `Match` whose two spans read
/// byte-identically is the same code somewhere else. `Delete` says those bytes leave the file and
/// `Insert` says they arrive in it. The two presets are two renderings of one edit, so whatever
/// else they may disagree about, they cannot disagree about whether the code is still there.
///
/// **`Move` against `Update` is not a contradiction and is not reported.** That pair is the
/// ordinary difference between the presets, stated in [`full_painting_covers_minimal`]: `Full`
/// routinely widens a `Move` into the `Update` that contains it, and both readings agree the code
/// survived. Only the survive-or-not pair is a contradiction, which is why this rule is narrower
/// than "the two presets label every shared byte the same" - a rule that shape fires on 20
/// fixtures and would be measuring the widening.
///
/// Alternatives are a disjunction, exactly as in invariant 2: a `Minimal (left)` need only be
/// consistent with *some* `Full` alternative, so each `Minimal` is scored against its closest
/// `Full` and only an alternative that contradicts every one of them is reported. A fixture whose
/// single painting answers for both presets has nothing to compare and is skipped.
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
/// An overlap is not representable. `render_paint_side` resolves one per byte by `PaintClass`'s
/// `max`, so the highest-ranked *verdict* wins on screen, while `label_bytes` below fills its array
/// in list order, so the *last entry* wins when the painting is scored. A painting with an overlap
/// therefore looks like one thing and grades as another, and neither reader says so.
///
/// `human_solver` refuses a new one at the keystroke (`overlapping_painted_range`), which is why
/// the corpus holds so few: the seven that remain were painted before that check existed. This is
/// the same rule applied to what is already on disk.
///
/// **Checked against the raw spans, not the projected labels**, which is the one design decision
/// here worth stating. Every other check in this file reads `label_bytes`' output, so that an
/// invariant can never fire on a byte no comparison would look at. That projection is exactly what
/// destroys the evidence for this rule - the later span simply wins and the array cannot tell you
/// anything was ever double-claimed - so this walks the spans themselves.
///
/// A line terminator is never painted, whatever span covers it (see `label_bytes`), so two ranges
/// meeting at a line break share only the newline and do not overlap. On a CRLF file the break is
/// both bytes.
///
/// One violation per range that lands on ground an earlier range of the same painting already
/// claimed, counted per side. A range overlapping two earlier ones still reports once: the repair
/// is the same edit either way.
fn painted_ranges_do_not_overlap(
    named: &NamedTextMapping,
    before: &Code,
    after: &Code,
) -> Vec<GroundTruthViolation> {
    let mut violations = Vec::new();
    for (side, contents) in [(0usize, &before.contents), (1usize, &after.contents)] {
        let bytes = contents.as_bytes();
        // Which entry claimed each byte, in list order - the same order `label_bytes` resolves by.
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
                        // The offending range whole, not just the byte that clashed: shortening
                        // one of the two ranges is the repair, so the range is what to look at.
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
/// The two ground truths are authored independently and are *expected* to differ: they chunk one
/// edit completely differently, which is what [`super::text_mapping_disagreements`] measures and
/// what the paper reports. This is the one thing they cannot differ about. A painted `Move` is a
/// `Match` whose two spans read byte-identically, so the painter has said that code survives; an
/// unmatched node in the tree mapping is one the same person said has no counterpart. Both
/// statements are about the same bytes and only one of them can hold.
///
/// **Only that direction.** The tree side's own `Move` labels come from `TextDiff::from`'s
/// column-shift heuristic rather than from either ground truth - neither expresses `Move`
/// positionally - so a byte the *tree* renders `Move` and the painting calls `Delete` measures the
/// renderer, not the humans. That pair is the larger of the two in the corpus (753 bytes over 8
/// fixtures against 96 over 5) and is deliberately not reported.
///
/// **And only where the mapping really does leave the byte unmatched**, which is checked against
/// [`Caches`] rather than read off the rendering. Until 2026-09-14 this rule took a tree-side
/// `Delete` or `Insert` as proof of an unmatched node, on the strength of a claim in this very
/// comment that turned out to be false: `TextDiff::from` also emits them for the characters that
/// *changed inside a matched-but-edited leaf*. `rust-rust-lang-rust-update-comment` is the case
/// that exposed it - a fixture with no unmatched node at all (187 `Identical` entries and 12
/// `MatchButNotIdentical`, nothing else) that was reported for two bytes of an edited comment,
/// where the painter put a colon at the head of the surviving text and the renderer put it at the
/// tail of the deleted text. A seam, not a contradiction. [`unmatched_bytes`] is the gate, and it
/// drops exactly that one violation: the other four fixtures this fires on sit in nodes the
/// mapping genuinely marks deleted or inserted.
///
/// Every painting is checked rather than the best-matching one. Each named painting asserts that
/// it is a correct rendering of the edit, and a rendering that contradicts the mapping about
/// survival is wrong whether or not a sibling painting agrees.
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
/// **A node its multi-map group leaves free does not count as unmatched**, for the reason
/// [`delimiter_pairs_agree`] gives at length: which member of a group is the one left over is
/// [`representative_entries`](super::representative_entries)' choice, not the human's, so reading
/// it as "the mapping says this is gone" contradicts a painting that simply made the other choice.
/// `java-defects4j-chart-9-timeseries` was exactly that - one `(` of a 1:2 group, painted as
/// surviving by `Full (outer parenthesis)` and inserted by the flattening.
///
/// Painted in preorder so a child overwrites its parent. A `Matched` node inside a
/// `DeleteWithChildren` one therefore reads as matched, while the whitespace *between* that
/// node's children - which no node of its own covers - keeps the parent's answer. That is exactly
/// what `descendant_for_byte_range` would say for each byte, in one walk rather than one lookup
/// per byte, and the inter-token-whitespace case is not a detail: 60 of the bytes this rule still
/// reports are the indentation inside a `block` the mapping deletes, which no leaf covers.
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

/// [`mapping_and_painting_agree_on_what_survives`]' comparison, over the projections it has
/// already built - split out so it can be tested against label vectors directly. Building the tree
/// side needs a real `ASTDiff` and a renderer, and what this rule says is about the labels.
///
/// `unmatched` is [`unmatched_bytes`]' output per side, and is what keeps a tree-side `Delete`
/// over a *matched* node from being read as a missing counterpart.
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
                // Whitespace is not something either ground truth can own. It lives in the gaps
                // between tokens, where the tree has no node at all, so `unmatched_bytes` can only
                // give it the verdict of whatever encloses it - while a `Full` painting takes the
                // indentation along with the construct it belongs to, which is what
                // `leading_whitespace` means. The two are then made to disagree about bytes
                // neither of them is really describing. Both instances this rule reported on
                // 2026-09-15 were exactly that: 20 columns of indentation in front of an `else`
                // whose condition genuinely moved (`java-defects4j-cli-12-gnuparser`), and four
                // runs of indentation inside a deleted block (`rust-next-font-imports-generator`).
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
/// **A half whose multi-map group leaves it free is not a claim either.** A group of one before
/// `(` against two after `(` says one of the two survives without saying which, and the same
/// fixture records its `)` the same way; `representative_entries` has to pick one pairing, and it
/// sorts each side by start byte, which for a nested call takes the *outer* opener and the *inner*
/// closer. Reading statuses off that flattening split every such pair and reported two violations
/// against ground truth that has a consistent reading (outer with outer). So the question asked
/// here is the one `check_group_entry` asks of codediff - does *some* admissible pairing agree -
/// and [`group_leaves_status_open`] answers it: a member the group could leave over intersects
/// both statuses, so it can never disagree with its partner. Seen on
/// `java-defects4j-cli-1-commandline` and `java-defects4j-chart-9-timeseries`, two violations
/// each.
///
/// **Per pair, not across pairs.** Two pairs drawing halves from one group can each be satisfiable
/// on their own and jointly infeasible, because the group's count couples them - one member being
/// the leftover decides that another is not. Answering that exactly is a feasibility problem over
/// the whole set of groups, and this takes the false negative instead: a contradiction reported
/// here is always real, and the shape that would hide one has no instance in the corpus.
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

/// Byte ranges of every `ERROR`/`MISSING` node in `root`'s tree. A `MISSING` node is zero-width,
/// so its range is widened to one byte - otherwise it could never overlap anything and the pairs
/// tree-sitter invented around it would be checked as if they were real.
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

/// What the tree mapping says happened to `node`, or `None` if it says nothing.
/// True when the pairings `node`'s [`MultiMapGroup`](super::MultiMapGroup) admits disagree about
/// whether *this* member is matched - in which case the mapping has made no claim here to
/// contradict.
///
/// A group of N before members and M after members matches `min(N, M)` pairs and leaves the rest
/// of the longer side over. Every member of the shorter side is therefore matched under every
/// pairing, and if the other side is empty nothing is matched at all: both are claims. Strictly
/// between the two, whether this particular member is the one left over is a choice
/// [`representative_entries`](super::representative_entries) makes to have something concrete to
/// hand a caller, not something the human wrote down.
///
/// Nothing propagates to descendants, because a delimiter pair cannot straddle two members: both
/// halves are direct children of one parent, so a member that contains one half contains the
/// other.
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
    let (mine, theirs) = if side == 0 {
        (group.before_paths.len(), group.after_paths.len())
    } else {
        (group.after_paths.len(), group.before_paths.len())
    };
    let matched = mine.min(theirs);
    matched > 0 && matched < mine
}

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

/// One side's sites as a row list: sorted, deduplicated and capped.
///
/// **Capped, at [`MAX_LISTED_ROWS`].** A violation can aggregate hundreds of sites - one fixture
/// here is 7,800 lines long - and a message that lists every one of them stops being readable as a
/// single line of a test failure. Ten is enough to start repairing from, and the remainder is
/// still counted, so nothing is hidden: the count of *sites* lives in the message around this and
/// the count of *rows* lives in the tail here.
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

/// Both sides' sites as one row list, naming each side - `before rows 3, 4 and after row 7`.
///
/// A side with no sites is left out entirely rather than printed empty, so a violation that only
/// ever happens on one side reads as though it were written for one side.
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

/// What the tree mapping says about one leaf once its ancestors have been consulted.
///
/// A mapping speaks about subtrees, not leaves: an `Identical` entry on a function says the whole
/// function is unchanged and records nothing for the tokens under it, and `DeleteWithChildren`
/// does the same for a removal. So a leaf's status is the nearest entry on the path from it to the
/// root - its own, or an ancestor's - read for what it implies about the leaf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LeafStatus<'tree> {
    /// Paired with a leaf that reads the same: the leaf's own `Identical` entry, or the leaf at
    /// the same offset under an `Identical` ancestor - well defined because such subtrees read
    /// token-for-token the same (invariant 14).
    Same(Node<'tree>),
    /// Paired by the leaf's own `Update` or `MatchButNotIdentical` entry.
    Paired(Node<'tree>),
    /// Deleted or inserted, by its own entry or under a `*WithChildren` ancestor.
    Removed,
    /// Under an `Update`/`MatchButNotIdentical` ancestor, or a childless `Delete`/`Insert`, with no
    /// entry of its own, or inside a multi-map group member the group could leave over: the
    /// mapping has not said, and no invariant here asserts anything.
    Undecided,
}

/// The two trees indexed for the leaf-level invariants, built once per fixture.
struct TreeContext<'tree> {
    caches: Caches,
    /// The mapping's multi-map groups, for [`group_leaves_status_open`]: a leaf whose group could
    /// leave it over is `Undecided`, not `Removed`.
    groups: Vec<super::MultiMapGroup>,
    /// Node id to node, per side - how a partner id from [`Caches`] becomes a node again.
    ids: [std::collections::HashMap<usize, Node<'tree>>; 2],
    /// Every leaf per side, in source order.
    leaves: [Vec<Node<'tree>>; 2],
}

impl<'tree> TreeContext<'tree> {
    fn build(
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

    fn status(&self, leaf: Node<'tree>, side: usize) -> LeafStatus<'tree> {
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
                    // The group could just as well have paired this member and left another over,
                    // so "removed" is the flattening talking - see `delimiter_pairs_agree`.
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
fn is_visible_leaf(leaf: Node, contents: &str) -> bool {
    contents
        .get(leaf.byte_range())
        .is_some_and(|text| !text.trim().is_empty())
}

/// A leaf whose text is not its own kind name: identifiers, literals, comments, string contents.
/// Punctuation and keywords are excluded by the invariants that say so, because which of two `}`
/// survives is a choice each ground truth makes on its own - see `delimiter_pairs_agree`.
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

/// The text with every whitespace character removed - what two leaves are compared on when the
/// question is whether they *read* differently, since a reformatting is not an edit to a token.
fn without_whitespace(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Invariant 10: a leaf the mapping pairs with a byte-identical leaf is never painted `Delete`
/// while that partner is painted `Insert`.
///
/// The mirror of invariant 9, asked of the tree side's pairs rather than its rendered labels. A
/// `Delete` says the text left the file and an `Insert` says it arrived new; the mapping's
/// `Identical` says they are one text. Both cannot be so. The painter's chunking is not in
/// question here - a rewritten line painted whole leaves the mapper's `Identical` tokens inside
/// it under `Update`/`MatchButNotIdentical` ancestors, which [`LeafStatus::Undecided`] skips -
/// only the case where the mapper explicitly paired two tokens the painter explicitly called gone
/// and new. Four fixtures break it, in each of which a whole statement is painted as a deletion
/// and an unrelated insertion that the mapping reads as one relocated statement.
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
/// Unpainted text is a positive claim - "unchanged, and in place" - and a leaf the mapping removes
/// is the strongest claim the other record can make against it. Limited to named leaves
/// ([`is_named_leaf`]): a deleted `}` whose partner brace the painter chose to keep instead is
/// the ordinary brace-identity difference, 43 sites across 11 fixtures, and not a contradiction
/// either record would concede. An unpainted deleted identifier is - closure-28 inserts the
/// `Override` of a new `@Override` and neither painting has a byte of it.
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
/// Only *at least one side*, deliberately: `Minimal` paints the deleted `tuple_` of a
/// `tuple_length` renamed to `length` and nothing on the after side, which is exact and fires a
/// one-sided rule 125 times. Nothing on either side is a change nobody painted - jsoup-16 turns
/// `"<!DOCTYPE html"` into `"<!DOCTYPE "` and both paintings walk past it. Whitespace-only
/// differences inside a leaf are exempt, as in invariant 14.
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
/// A mapping whose every entry is `Identical` and whose every group is balanced says the file did
/// not change. A painting with a `Delete` of visible text, an `Insert` of visible text, or a
/// `Match` whose two sides differ beyond whitespace says it did. One of them is wrong - and since
/// an all-`Identical` mapping grades codediff against nothing, it is the one that has been
/// passing vacuously. Whitespace is the exemption because it lives between nodes, where the tree
/// cannot record it and the painting can.
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
        // The spans' own `start_row` is already the row, 0-based - there is no byte offset to
        // convert here, unlike every other rule in this file.
        for (side, spans) in [(0usize, &entry.before), (1usize, &entry.after)] {
            rows[side].extend(spans.iter().map(|span| span.start_row + 1));
            // The painted spans are already in this type - the one rule here that needs no
            // conversion at all.
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

/// Invariant 14: an `Identical` entry's two subtrees carry the same tokens.
///
/// `Identical` is the strongest thing the mapping says - the whole subtree is unchanged - and
/// every leaf-level invariant above builds on it to find a leaf's twin. Tokens rather than text,
/// because 159 `Identical` subtrees across 12 fixtures differ in whitespace alone, which is a
/// reformatting and not an edit; compared token for token, no fixture in the corpus breaks this.
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

/// Invariant 15: a `MatchButNotIdentical` entry's two subtrees do not read byte-identically with
/// every descendant paired inside.
///
/// `MatchButNotIdentical` says the subtree differs somewhere. Two nodes of one kind whose text is
/// the same byte for byte, and whose every descendant with an entry of its own pairs inside the
/// partner, differ nowhere - and the grader is strict about the operation (`check_entry`), so each
/// such entry is a claim codediff can only satisfy by calling an identical subtree not identical.
/// Group members are skipped: a group's operation describes the whole group, and its
/// representative pairing may well put two identical members together.
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

/// Byte offsets inside an identifier at which a word may legally be cut: 0, both ends of any run
/// of `_`, and any uppercase letter that begins a word. The last entry is always the identifier's length,
/// so "the next word start at or after `n`" always has an answer.
///
/// An uppercase letter begins a word when the character before it is not uppercase (`fooBar` ->
/// 3), or when it is uppercase but the character *after* is lowercase - which is how an acronym
/// hands off to the next word (`HTTPServer` -> 0, 4). Without that second clause `HTTPServer`
/// would read as one word and `XMLHttpRequest` as two.
///
/// **Both ends of a `_` run, not just the far end.** `open_read_only` ->
/// `open_read_only_with_config` appends `_with_config`, and the appended text starts *at* the
/// underscore. Offering only the byte after it leaves the nearest boundary four words back, which
/// widens the highlight to `only_with_config` and reports an edit larger than the one that
/// happened.
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

/// The part of `text` that differs from `other`, widened outward to whole identifier words.
///
/// The raw common prefix/suffix is not the answer on its own, and the corpus already recorded why:
/// scoring a rename by bare common suffix makes `calculateArea` and `area` share `rea`, `Box` and
/// `Box<T>` share `Box`, `calculatePerimeter` and `perimeter` share `erimeter` - 15 of 16 false
/// positives in the 2026-08-26 line-tail measurement came from a shared run falling *inside* a
/// word (see `research/data/quality/text_painting_findings.md`, rule 3). Splitting a highlight
/// there is not something a reader would ever do by hand. Widening each end to the nearest word
/// boundary is what makes the narrow reading legible: `EVENT_NEW_FRAME` -> `SC_EVENT_NEW_FRAME`
/// comes out as exactly `SC_`, and `calculateArea` -> `area` widens to the whole of both, which is
/// the honest answer for a rename that shares nothing but three letters in the middle of a word.
///
/// An empty span (`start == end`) means this side has nothing to paint: every byte of it survives
/// into the other, as the before side of a pure insertion does.
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

/// The bare common-prefix/common-suffix span, before any widening.
///
/// **The common suffix is taken first, which places an ambiguous run as far left as it will go.**
/// `last_packet_timestamp` -> `last_filtered_packet_timestamp` can be read as inserting
/// `filtered_` after `last_` or `_filtered` after `last`; both rebuild the same string, and taking
/// the longest common *prefix* first - which is what this did until 2026-09-15 - always picks the
/// rightmost of them. The corpus paints the leftmost: that fixture's `Minimal` painting marks
/// columns 41..50, `_filtered`. Nothing else moves, because the two readings only ever differ when
/// the run's own edges repeat the text beside it.
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

/// Every span a `Minimal` painting may legitimately mark on this side of a rename.
///
/// One candidate when the identifier has a word boundary inside it: the widened span, because
/// that is what the boundary is *for* - `getDeclaredConstructor` -> `getDeclaredConstructors` is
/// read as the word `Constructors` changing, not as an `s` appearing.
///
/// **Two candidates when it has none.** `align` -> `halign` and `v` -> `value` are single words
/// with nothing to orient on, and there the bare affix is as defensible as the whole token: a
/// painter marking just the added `h` is saying something true, and so is one marking `halign`
/// entire. Widening is a rule about where a highlight may be *cut*, and an identifier with no
/// internal boundary offers no cut to prefer - so both readings pass rather than the corpus being
/// told to pick one.
fn minimal_affix_candidates(text: &str, other: &str) -> Vec<(usize, usize)> {
    let widened = differing_affix(text, other);
    if identifier_word_starts(text).len() > 2 {
        return vec![widened];
    }
    let (start, end) = raw_affix(text, other);
    let mut candidates = vec![widened];
    if (start, end) != widened {
        candidates.push((start, end.max(start)));
    }
    candidates
}

/// A leaf that is an identifier: a named leaf whose text reads as one, and which is not a keyword.
///
/// Deliberately not a list of node kinds. Grammars spell them `identifier`, `type_identifier`,
/// `field_identifier`, `property_identifier`, `variable_name`, `word`, ... and a kind list would
/// be a per-language table to keep in step with every grammar upgrade. The text shape plus
/// [`is_named_leaf`] (whose doc comment explains why a leaf whose text *is* its own kind name -
/// `true`, `if`, `}` - is excluded) answers the same question without one.
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
/// only the differing words and `Full` paints the whole identifier on both sides.
///
/// The two presets are not degrees of care, they are two conventions (see
/// `text_painting_findings.md`, rule 1), and this is the one edit where they are *obliged* to
/// differ: `MINIMAL` marks the bytes that carry the change and `FULL` accounts for every byte
/// whose role changed, and a renamed identifier has both readings at once. Today's renderer
/// narrows before either preset is consulted, so `FULL` asks for more paint and gets *less* -
/// the 2026-09-05 census's family D, and 225 runs over 54 fixtures in the 2026-09-15 one.
///
/// **Only paintings named for a preset are checked**, and a fixture painted once is skipped
/// entirely. A single painting is held to *both* presets by `paintings_for_mode`, and the two
/// halves of this rule contradict each other by construction - demanding both of one painting
/// would condemn every single-painting fixture that renames anything, which is not a finding
/// about the data.
///
/// The word-boundary widening is what keeps the `Minimal` half from asking for a highlight no
/// human would draw - see [`differing_affix`].
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
/// Flipping a default, a flag or a guard is one of the commonest one-token commits there is, and
/// both ground truths have a spelling that says so: an `Update` entry on the two literals, and a
/// painted `Match` whose two sides differ. Recording it as a removal plus an arrival instead says
/// the old value went somewhere and the new one came from somewhere, which is a different claim
/// about the same edit and the one a reader is least able to check.
///
/// Two halves, reported under one number because they are one rule:
///
/// * **mapping** - a `true`/`false` leaf the mapping removes, whose partner subtree contains the
///   opposite literal also removed. Correspondence is required, not just co-occurrence: the two
///   leaves must sit under ancestors the mapping pairs with each other, so a `true` deleted in one
///   method and a `false` added in another is left alone.
/// * **painting** - a boolean pair the mapping *does* pair, painted `Delete` on one side and
///   `Insert` on the other. Invariant 10 asks this of byte-identical pairs and so cannot reach
///   these: `true` and `false` are exactly the pair whose two sides differ.
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
        // The nearest ancestor the mapping pairs, and the subtree it pairs with: the flip has to
        // have happened *here*, not anywhere in the file.
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
        // `filtered_` after `last_`. Both rebuild the same identifier; the corpus paints the
        // first (rust-gyulyvgc-sniffnet-rename-one-identifier, Minimal, columns 41..50).
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
        // With a boundary to orient on there is one reading, and it is the widened one.
        assert_eq!(
            minimal_affix_candidates("getDeclaredConstructors", "getDeclaredConstructor"),
            vec![(11, 23)]
        );
    }

    #[test]
    fn a_shared_run_inside_a_word_does_not_split_it() {
        // `calculateArea` and `area` share the bare suffix "rea", which is the trap
        // `text_painting_findings.md` rule 3 records: widening to word boundaries takes the whole
        // of both rather than highlighting `calculateA` against `a`.
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

    /// The case the solver's own help promises is fine, and the reason this reads the bytes rather
    /// than the rows: a range ending at column 0 of the next row swallows the break, and nothing
    /// downstream paints a line terminator.
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

    /// A Windows break is two bytes, and neither is painted - the same fix `label_bytes` carries.
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

    /// The rule is about a stripe hanging off the *end* of a line. A run that stops on a space
    /// with visible text still to come on that row is the ordinary shape of an edit - this is
    /// go-gin-gonic-gin-whitespace-in-comment, where one of two spaces mid-comment is deleted.
    #[test]
    fn a_painted_run_that_ends_on_a_mid_row_space_is_accepted() {
        let before = rust("let x = 1;  // a  b\n");
        let after = rust("\n");
        // `a ` at columns 15..17 - the run ends on the space at 16, with `b` still to come on
        // the row. Deliberately a run with a visible character in it: a run of only whitespace
        // would be exempt under the other rule too, and this test would then pass without
        // exercising the mid-row check it is named for.
        let mapping = painted(vec![("Only one solution", vec![deleted(15, 2)])]);

        assert!(
            violations(&mapping, &before, &after).is_empty(),
            "a mid-row space is not trailing whitespace"
        );
    }

    /// A run with no visible character in it has no spelling this rule would accept, so it is
    /// exempt for the same reason a blank row is - a commit that strips trailing spaces is
    /// exactly this shape.
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

    // ── Invariant 9 ─────────────────────────────────────────────────────────────────────────

    fn survival_across_records(mapping: &HumanMapping, before: &Code, after: &Code) -> Vec<String> {
        violations(mapping, before, after)
            .into_iter()
            .filter(|v| v.contains("the tree mapping leaves unmatched"))
            .collect()
    }

    /// Label vectors rather than a built mapping: the tree side of this rule comes from a real
    /// `ASTDiff` put through `TextDiff::from`, and that renderer reads the *text*, so a synthetic
    /// pair contrived to make it emit a `Delete` ends up testing the renderer instead of the rule.
    /// The five corpus fixtures clamped for this invariant are what exercise the wiring.
    fn labels(before_len: usize, after_len: usize) -> PaintedLabels {
        [vec![None; before_len], vec![None; after_len]]
    }

    /// An `unmatched_bytes` mask saying every byte is inside a node the mapping leaves unmatched
    /// - what the rule's own gate looks like when it is not the thing under test.
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

    /// The regression test for the false positive the gate exists to stop, and the one shape this
    /// rule got wrong for a day: the renderer says these bytes were deleted, but they are inside a
    /// node the mapping *matched* - a character edited out of a surviving leaf, not a leaf with no
    /// counterpart. `rust-rust-lang-rust-update-comment` is the corpus case, where a painter and
    /// `TextDiff` put the same colon on opposite sides of one comment edit.
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

    /// The gate's own rule: the smallest node containing a byte decides, so a node the mapping
    /// matched reads as matched even inside a subtree it deleted - while the whitespace between
    /// that subtree's children, which no node of its own covers, keeps the deleted answer. That
    /// whitespace case is not a corner: 60 of the bytes this rule still reports corpus-wide are
    /// indentation inside a deleted `block`, and a leaf-only lookup would drop every one of them.
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

    /// The direction this rule deliberately does not take: the tree side's `Move` comes from
    /// `TextDiff::from`'s column-shift heuristic rather than from either ground truth, so a byte
    /// the tree renders `Move` and the painting calls `Delete` measures the renderer. It is also
    /// the larger of the two in the corpus, which is what makes reporting it a bad trade.
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
        // The shape that motivated this: one call becomes two nested calls, so the human records
        // the single before `(` against both after `(` and the single before `)` against both
        // after `)`, each as a 1:2 group. Either choice is valid, and pairing outer with outer is
        // consistent - but `representative_entries` sorts each side by start byte, so it takes the
        // outer opener and the *inner* closer and splits both pairs.
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
