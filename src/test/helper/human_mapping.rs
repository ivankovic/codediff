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

/// Human-authored ground truth for a fixture, `<fixture dir>/human_mapping.json`, written by the
/// `human_solver` binary: a node mapping (`entries`, `groups`) and independent text paintings
/// (`text_mappings`), plus the checks that grade codediff against them.
///
/// Nodes are identified by *path* (see [`super::path_for_node`]), not node id: ids are not stable
/// across the separate parses that write and later check a mapping.
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tree_sitter::Node;

use crate::code::ASTMetadata;
use crate::diff::cost::operation_cost;
use crate::diff::{ASTDiff, ASTMapping, ASTMappingOperation, ASTMappingReason, NodeCache};
use crate::test::helper::{PathCache, path_for_node};

/// Properties the ground truth must hold on its own, independently of what codediff does with it.
pub mod invariants;

/// What a human decided should happen to a node (or pair of nodes) between before and after. The
/// three pairing operations also pin *which* [`ASTMappingOperation`] codediff must have chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanOperation {
    /// Same node, no difference at all (for a node with children, the human confirmed the whole
    /// subtree is unchanged). Expects [`ASTMappingOperation::Identical`].
    Identical,
    /// The before and after nodes have the same kind and no children, but different text (e.g. a
    /// changed string literal). Expects [`ASTMappingOperation::Update`].
    Update,
    /// Matched but not identical: the subtree differs somewhere, or the kinds differ and the human
    /// confirmed the pairing anyway. Expects [`ASTMappingOperation::MatchButNotIdentical`].
    MatchButNotIdentical,
    /// The before node was removed; its children, if any, are handled by other entries.
    Delete,
    /// The before node and its entire subtree were removed.
    DeleteWithChildren,
    /// The after node is new; its children, if any, are handled by other entries.
    Insert,
    /// The after node and its entire subtree are new.
    InsertWithChildren,
}

/// One human-authored decision about a node (`Delete`/`Insert`) or a pair of nodes
/// (`Identical`/`Update`/`MatchButNotIdentical`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanMappingEntry {
    pub operation: HumanOperation,
    /// Path to the node in the before tree. Present for `Identical`, `Update`,
    /// `MatchButNotIdentical`, `Delete` and `DeleteWithChildren`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub before_path: Option<Vec<String>>,
    /// Path to the node in the after tree. Present for `Identical`, `Update`,
    /// `MatchButNotIdentical`, `Insert` and `InsertWithChildren`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub after_path: Option<Vec<String>>,
}

/// How the members of a [`MultiMapGroup`] correspond to each other.
///
/// * [`Self::AnyOneToOne`]: several *interchangeable* nodes (three identical `foo()` calls become
///   two). Some one-to-one pairing is the truth and any of them is as good as another.
/// * [`Self::AllToAll`]: one piece of code *became* several, or several became one (a statement
///   split in two). Every before member corresponds to every after member; none is gone or new.
///   The tree counterpart of a painting's N:M `Match`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupPairing {
    /// Some one-to-one pairing of `min(N, M)` pairs is correct, and any of them counts. The rest
    /// of the larger side is deleted/inserted. The default, and left out of the file.
    #[default]
    AnyOneToOne,
    /// Every before member corresponds to every after member. No member is deleted or inserted,
    /// whatever N and M are: a 1:3 group says one node became three, not that two are new.
    AllToAll,
}

impl GroupPairing {
    pub fn is_any_one_to_one(&self) -> bool {
        *self == Self::AnyOneToOne
    }
}

/// A set of `before_paths` nodes that correspond to a set of `after_paths` nodes as a whole, in
/// one of the two senses [`GroupPairing`] names.
///
/// `AnyOneToOne`: any pairing codediff produces counts, as long as it uses `min(N, M)` pairs and
/// leaves the rest deleted/inserted. `AllToAll`: every member must be matched inside the group, so
/// a one-to-one diff necessarily scores at least `|N - M|` mismatches. That is deliberate: it is
/// the distance between what the ground truth says and what the algorithm can express.
///
/// Validated by [`check_group_entry`]; [`representative_entries`] picks one concrete pairing for
/// display and cost only, never for validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiMapGroup {
    /// Paths to every before-side candidate node (N of them). Order carries no meaning.
    pub before_paths: Vec<Vec<String>>,
    /// Paths to every after-side candidate node (M of them).
    pub after_paths: Vec<Vec<String>>,
    /// The operation every realized pair must have chosen. Only `Identical` or
    /// `MatchButNotIdentical`: `Update` has no coherent meaning across an ambiguous group, and
    /// deletion/insertion is expressed by leftover members.
    pub operation: HumanOperation,
    /// Whether matched members' subtrees must close within each other (see
    /// [`check_subtree_maps_within`]) and leftover members' whole subtrees be deleted/inserted - the
    /// group's `*WithChildren`. For `AllToAll` the closure is over the union of the members.
    pub with_children: bool,
    /// How the members correspond - see [`GroupPairing`].
    #[serde(default, skip_serializing_if = "GroupPairing::is_any_one_to_one")]
    pub pairing: GroupPairing,
}

/// The full set of human decisions for one before/after test case.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HumanMapping {
    pub entries: Vec<HumanMappingEntry>,
    /// Multi-map groups (see [`MultiMapGroup`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<MultiMapGroup>,
    /// Named human paintings of the same diff as *text* (see [`HumanTextMapping`]) - an
    /// independent ground truth, not derived from `entries`.
    ///
    /// A list because a text rendering often has several equally defensible answers (two moves
    /// into a new block, or one update of the region); a checker accepts any one of them. Empty
    /// means unpainted; a named painting with no entries means painted and nothing changed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text_mappings: Vec<NamedTextMapping>,
}

// ─── Human-painted text ranges ──────────────────────────────────────────────────────────────

/// One span of source text, in [`crate::diff::text_range::TextRange`]'s space: 0-based rows and
/// **byte** columns, exclusive end, an end column of 0 meaning "up to, not including, this row".
/// A separate type so `diff::text_range` needs no serde.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanTextSpan {
    pub start_row: usize,
    pub start_column: usize,
    pub end_row: usize,
    pub end_column: usize,
}

impl HumanTextSpan {
    pub fn to_text_range(self) -> crate::diff::text_range::TextRange {
        crate::diff::text_range::TextRange::new(
            self.start_row,
            self.start_column,
            self.end_row,
            self.end_column,
        )
    }

    pub fn is_empty(&self) -> bool {
        (self.start_row, self.start_column) >= (self.end_row, self.end_column)
    }
}

/// What a human said about one painted span. Three variants, not five: whether a correspondence is
/// a move or an update is decided by comparing the spans' bytes (see [`HumanTextEntry::verdict`]),
/// which a machine does more reliably than a person.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HumanTextOperation {
    /// The before span and the after span are the same code; move vs update is derived.
    Match,
    /// The before span is gone from the after side.
    Delete,
    /// The after span is new.
    Insert,
}

/// One human-painted decision: a `Match` carries spans on both sides, a `Delete` only `before`, an
/// `Insert` only `after`.
///
/// Both sides are lists, so a `Match` can be N:M: which occurrence pairs with which is left
/// unspecified, which is only sound because every span on one side of a `Match` must cover
/// identical text ([`HumanTextEntry::verdict`] enforces it).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanTextEntry {
    pub operation: HumanTextOperation,
    #[serde(
        default,
        deserialize_with = "spans_from_one_or_many",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub before: Vec<HumanTextSpan>,
    #[serde(
        default,
        deserialize_with = "spans_from_one_or_many",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub after: Vec<HumanTextSpan>,
}

/// Reads a side's spans as either a bare span object (the form older committed fixtures carry) or
/// a list. Always serialized as a list.
fn spans_from_one_or_many<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<HumanTextSpan>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(HumanTextSpan),
        Many(Vec<HumanTextSpan>),
    }

    Ok(match Option::<OneOrMany>::deserialize(deserializer)? {
        None => Vec::new(),
        Some(OneOrMany::One(span)) => vec![span],
        Some(OneOrMany::Many(spans)) => spans,
    })
}

/// What a [`HumanTextEntry`] asserts once its spans have been read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HumanTextVerdict {
    /// A `Match` whose two spans hold byte-identical text: the same code, somewhere else.
    Move,
    /// A `Match` whose two spans differ: the same code, edited in place.
    Update,
    Delete,
    Insert,
}

impl HumanTextEntry {
    /// Resolves this entry against the source, deriving `Move` vs `Update` for a `Match`. `Err` if
    /// the entry is malformed for its operation or a span falls outside its file.
    pub fn verdict(&self, before: &str, after: &str) -> Result<HumanTextVerdict> {
        match self.operation {
            HumanTextOperation::Delete => {
                ensure!(
                    !self.before.is_empty(),
                    "a Delete entry has no `before` span"
                );
                ensure!(
                    self.after.is_empty(),
                    "a Delete entry must not carry an `after` span"
                );
                Self::check_readable(before, &self.before, "Delete", "before")?;
                Ok(HumanTextVerdict::Delete)
            }
            HumanTextOperation::Insert => {
                ensure!(
                    !self.after.is_empty(),
                    "an Insert entry has no `after` span"
                );
                ensure!(
                    self.before.is_empty(),
                    "an Insert entry must not carry a `before` span"
                );
                Self::check_readable(after, &self.after, "Insert", "after")?;
                Ok(HumanTextVerdict::Insert)
            }
            HumanTextOperation::Match => {
                ensure!(
                    !self.before.is_empty(),
                    "a Match entry has no `before` span"
                );
                ensure!(!self.after.is_empty(), "a Match entry has no `after` span");
                let before_text = self.side_text(before, &self.before, "Match", "before")?;
                let after_text = self.side_text(after, &self.after, "Match", "after")?;
                // Byte-identical means relocated, anything else means edited. Sound for N:M
                // because `side_text` checked every span on a side reads the same.
                Ok(if before_text == after_text {
                    HumanTextVerdict::Move
                } else {
                    HumanTextVerdict::Update
                })
            }
        }
    }

    /// Every span reads back from the source. One-sided operations need no identity constraint:
    /// three different tokens deleted together is still one decision.
    fn check_readable(
        source: &str,
        spans: &[HumanTextSpan],
        operation: &str,
        side: &str,
    ) -> Result<()> {
        for (index, span) in spans.iter().enumerate() {
            span_text(source, *span).with_context(|| {
                format!("a {operation} entry's {side} span {index} is outside the {side} file")
            })?;
        }
        Ok(())
    }

    /// The text one side's spans cover, checking they all cover the *same* text.
    fn side_text<'a>(
        &self,
        source: &'a str,
        spans: &[HumanTextSpan],
        operation: &str,
        side: &str,
    ) -> Result<&'a str> {
        let mut text: Option<&str> = None;
        for (index, span) in spans.iter().enumerate() {
            let this = span_text(source, *span).with_context(|| {
                format!("a {operation} entry's {side} span {index} is outside the {side} file")
            })?;
            match text {
                None => text = Some(this),
                Some(first) => ensure!(
                    first == this,
                    "a {operation} entry's {side} spans must all cover identical text, but span 0 \
                     reads {first:?} and span {index} reads {this:?}"
                ),
            }
        }
        text.context("no spans")
    }
}

/// A human-painted account of a diff *as text*, independent of the tree mapping in the same
/// [`HumanMapping`].
///
/// The tree mapping does not determine what a reader should see: many nodes carry no visible text,
/// and a reorder ("one line moved past five" vs "five moved past one") is the same set of matched
/// pairs either way. So this records what the diff *looks like*; checking the two against each
/// other is [`text_mapping_disagreements`].
///
/// **Unpainted text is unchanged.** The absence of a span claims the text is identical and in
/// place, which is what makes a `Match` of identical text mean "moved".
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HumanTextMapping {
    pub entries: Vec<HumanTextEntry>,
}

/// One painting under a free-text name. `Minimal`/`Full` (optionally qualified, see
/// [`designates_preset`]) tie it to a render preset; `Only one solution` is the conventional name
/// for a lone painting.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NamedTextMapping {
    pub name: String,
    #[serde(flatten)]
    pub mapping: HumanTextMapping,
}

/// The text a span covers, or `None` if it falls outside `contents`. A span ending at column 0 of
/// row *r* includes row *r-1*'s newline, as in `line_operations` and `columns_on_row`.
pub fn span_text(contents: &str, span: HumanTextSpan) -> Option<&str> {
    let start = byte_offset(contents, span.start_row, span.start_column)?;
    let end = byte_offset(contents, span.end_row, span.end_column)?;
    if end < start {
        return None;
    }
    contents.get(start..end)
}

/// Absolute byte offset of `(row, column)` in `contents`, or `None` if that position doesn't
/// exist. A column exactly at a row's length is valid (the position just past the last character).
fn byte_offset(contents: &str, row: usize, column: usize) -> Option<usize> {
    let mut offset = 0usize;
    for (index, line) in contents.split('\n').enumerate() {
        if index == row {
            if column > line.len() || !contents.is_char_boundary(offset + column) {
                return None;
            }
            return Some(offset + column);
        }
        offset += line.len() + 1;
    }
    None
}

/// One byte-granular label for a side, shared by the painted text mapping and the tree mapping
/// projected to text so the two compare on equal footing. `None` means unchanged and in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextLabel {
    Move,
    Update,
    Delete,
    Insert,
}

impl TextLabel {
    fn from_verdict(verdict: HumanTextVerdict) -> Self {
        match verdict {
            HumanTextVerdict::Move => TextLabel::Move,
            HumanTextVerdict::Update => TextLabel::Update,
            HumanTextVerdict::Delete => TextLabel::Delete,
            HumanTextVerdict::Insert => TextLabel::Insert,
        }
    }

    fn from_text_operation(operation: &crate::diff::text::TextOperation) -> Option<Self> {
        match operation {
            crate::diff::text::TextOperation::Move => Some(TextLabel::Move),
            crate::diff::text::TextOperation::Update => Some(TextLabel::Update),
            crate::diff::text::TextOperation::Delete => Some(TextLabel::Delete),
            crate::diff::text::TextOperation::Insert => Some(TextLabel::Insert),
            crate::diff::text::TextOperation::Identical
            | crate::diff::text::TextOperation::NotYetSet => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            TextLabel::Move => "move",
            TextLabel::Update => "update",
            TextLabel::Delete => "delete",
            TextLabel::Insert => "insert",
        }
    }
}

/// One place the painted text mapping and the tree mapping disagree about a side's text.
#[derive(Debug, Clone)]
pub struct TextMappingDisagreement {
    /// `0` = before, `1` = after, matching `TextDiff::all`'s own side convention.
    pub side: usize,
    /// Start of a run of bytes over which both sources hold the same pair of opinions.
    pub start_byte: usize,
    pub end_byte: usize,
    /// 0-based row the run starts on, for a human-readable report.
    pub start_row: usize,
    /// What the painted text mapping says. `None` = "unchanged and in place".
    pub painted: Option<TextLabel>,
    /// What the tree mapping says, projected through `TextDiff`.
    pub from_tree: Option<TextLabel>,
}

/// Per-byte labels for one side, from a list of `(span, label)`. Later spans win on overlap, which
/// only a malformed painting has.
///
/// Line terminators are never labeled: the renderer bounds each row's painted columns to the row's
/// content, so a multi-row range never visibly paints the seam between rows. Applies to the human's
/// spans and codediff's ranges alike.
fn label_bytes(contents: &str, spans: &[(HumanTextSpan, TextLabel)]) -> Vec<Option<TextLabel>> {
    let mut labels = vec![None; contents.len()];
    for &(span, label) in spans {
        let (Some(start), Some(end)) = (
            byte_offset(contents, span.start_row, span.start_column),
            byte_offset(contents, span.end_row, span.end_column),
        ) else {
            continue;
        };
        for slot in labels.iter_mut().take(end.min(contents.len())).skip(start) {
            *slot = Some(label);
        }
    }
    // A span ending at column 0 of the next row swallows the break; on a CRLF file that is two
    // bytes, so `\r` is skipped as well as `\n`
    // (`javascript-microsoft-typescript-small-change-2`).
    let bytes = contents.as_bytes();
    for index in 0..labels.len() {
        if bytes[index] == b'\n' {
            labels[index] = None;
            if index > 0 && bytes[index - 1] == b'\r' {
                labels[index - 1] = None;
            }
        }
    }
    labels
}

/// Per-byte labels for one side, from `TextDiff`'s own range list.
fn label_bytes_from_ranges(
    contents: &str,
    ranges: &[crate::diff::text::RangeMatch],
) -> Vec<Option<TextLabel>> {
    let spans: Vec<(HumanTextSpan, TextLabel)> = ranges
        .iter()
        .filter(|range_match| !range_match.source.is_empty())
        .filter_map(|range_match| {
            TextLabel::from_text_operation(&range_match.operation).map(|label| {
                (
                    HumanTextSpan {
                        start_row: range_match.source.start_row,
                        start_column: range_match.source.start_column,
                        end_row: range_match.source.end_row,
                        end_column: range_match.source.end_column,
                    },
                    label,
                )
            })
        })
        .collect();
    label_bytes(contents, &spans)
}

/// How well one named painting agrees with the tree mapping.
#[derive(Debug, Clone)]
pub struct TextMappingCheck {
    /// The name of the painting this result is for.
    pub solution: String,
    pub disagreements: Vec<TextMappingDisagreement>,
}

/// Checks the tree mapping against every painted text solution and returns the one that agrees
/// best - fewest disagreeing **bytes** (the quantity callers report), ties broken by list order.
/// `Ok(None)` when nothing is painted, which is not agreement.
///
/// Best, not all: the paintings are alternatives, like a [`MultiMapGroup`].
///
/// Not mode-aware, unlike [`compare_painting`]: both sides here are human-authored (the tree side
/// is built from `entries`, not `diff_code`), and routing them through
/// [`paintings_for_mode`]/`ranges_for_options` would let a bug in those look like a disagreement
/// between the two ground truths.
///
/// One rendering choice is unavoidable: projecting a node mapping to bytes needs *some* decision
/// about which matched nodes render as `Move`, and `TextDiff::from`'s column-shift heuristic is
/// it. Neither ground truth records `Move` positionally, so use [`disagreement_is_move_only`] to
/// separate that artifact from structural disagreement.
pub fn text_mapping_disagreements(
    mapping: &HumanMapping,
    before: &crate::code::Code,
    after: &crate::code::Code,
) -> Result<Option<TextMappingCheck>> {
    if mapping.text_mappings.is_empty() {
        return Ok(None);
    }

    let ast_diff = as_ast_diff_for_mapping(mapping, before, after)?;
    let node_cache = crate::diff::NodeCache::build(before, after);
    let text_diff = crate::diff::text::TextDiff::from(before, after, &ast_diff, &node_cache);
    let tree_labels = [
        label_bytes_from_ranges(&before.contents, &text_diff.all(0)),
        label_bytes_from_ranges(&after.contents, &text_diff.all(1)),
    ];

    let disagreeing_bytes = |disagreements: &[TextMappingDisagreement]| -> usize {
        disagreements
            .iter()
            .map(|d| d.end_byte - d.start_byte)
            .sum()
    };

    let mut best: Option<TextMappingCheck> = None;
    for named in &mapping.text_mappings {
        let disagreements = disagreements_for(&named.mapping, before, after, &tree_labels)
            .with_context(|| format!("checking the '{}' text painting", named.name))?;
        let better = best.as_ref().is_none_or(|current| {
            disagreeing_bytes(&disagreements) < disagreeing_bytes(&current.disagreements)
        });
        if better {
            best = Some(TextMappingCheck {
                solution: named.name.clone(),
                disagreements,
            });
        }
    }
    Ok(best)
}

/// Whether a disagreement is purely `Move` vs "unchanged in place" - the rendering artifact
/// [`text_mapping_disagreements`] cannot avoid, reported apart from structural disagreement.
pub fn disagreement_is_move_only(d: &TextMappingDisagreement) -> bool {
    matches!(
        (d.painted, d.from_tree),
        (Some(TextLabel::Move), None) | (None, Some(TextLabel::Move))
    )
}

/// One painting against already-computed tree labels.
fn disagreements_for(
    text_mapping: &HumanTextMapping,
    before: &crate::code::Code,
    after: &crate::code::Code,
    tree_labels: &[Vec<Option<TextLabel>>; 2],
) -> Result<Vec<TextMappingDisagreement>> {
    let mut painted_before: Vec<(HumanTextSpan, TextLabel)> = Vec::new();
    let mut painted_after: Vec<(HumanTextSpan, TextLabel)> = Vec::new();
    for entry in &text_mapping.entries {
        let label = TextLabel::from_verdict(entry.verdict(&before.contents, &after.contents)?);
        for span in &entry.before {
            painted_before.push((*span, label));
        }
        for span in &entry.after {
            painted_after.push((*span, label));
        }
    }

    let mut disagreements = Vec::new();
    for (side, contents, painted) in [
        (0usize, &before.contents, &painted_before),
        (1usize, &after.contents, &painted_after),
    ] {
        let painted_labels = label_bytes(contents, painted);
        let tree_labels = &tree_labels[side];

        let mut row_of = RowIndex::new(contents);
        let mut byte = 0usize;
        while byte < contents.len() {
            if painted_labels[byte] == tree_labels[byte] {
                byte += 1;
                continue;
            }
            let (painted_label, tree_label) = (painted_labels[byte], tree_labels[byte]);
            let start = byte;
            while byte < contents.len()
                && painted_labels[byte] == painted_label
                && tree_labels[byte] == tree_label
            {
                byte += 1;
            }
            disagreements.push(TextMappingDisagreement {
                side,
                start_byte: start,
                end_byte: byte,
                start_row: row_of.row_at(start),
                painted: painted_label,
                from_tree: tree_label,
            });
        }
    }
    Ok(disagreements)
}

/// How closely codediff's own rendering, under one [`crate::diff::text::RenderOptions`] preset,
/// matches the human painting that preset is answerable to.
#[derive(Debug, Clone)]
pub struct PaintingComparison {
    pub options: crate::diff::text::RenderOptions,
    /// The painting compared against, by name.
    pub solution: String,
    /// Bytes where the two disagree about what happened to that text, summed over both sides.
    pub mismatched_bytes: usize,
    /// Bytes in both files - the denominator.
    pub total_bytes: usize,
}

impl PaintingComparison {
    /// Disagreement as a percentage of the corpus's bytes. `0.0` is exact agreement.
    pub fn percent(&self) -> f64 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        100.0 * self.mismatched_bytes as f64 / self.total_bytes as f64
    }
}

/// The ground-truth painting name `options` is answerable to, or `None` if it is neither of the
/// two presets: ground truth pins only the two extremes.
fn preset_name(options: crate::diff::text::RenderOptions) -> Option<&'static str> {
    if options == crate::diff::text::RenderOptions::MINIMAL {
        Some("Minimal")
    } else if options == crate::diff::text::RenderOptions::FULL {
        Some("Full")
    } else {
        None
    }
}

/// Whether `name` designates the `Minimal` preset - see [`designates_preset`].
fn designates_minimal(name: &str) -> bool {
    designates_preset(name, "Minimal")
}

/// Whether `name` designates the `Full` preset - see [`designates_preset`].
fn designates_full(name: &str) -> bool {
    designates_preset(name, "Full")
}

/// The paintings `options` is answerable to. A lone painting claims the rendering is unambiguous,
/// so both presets are held to it whatever its name. With several, those named for the preset;
/// if none names *any* preset they are alternatives for every preset. A fixture with a misspelled
/// preset (`Minimal` beside `Ful`) is an error, not a set of alternatives.
pub fn paintings_for_mode(
    mapping: &HumanMapping,
    options: crate::diff::text::RenderOptions,
) -> Result<Vec<&NamedTextMapping>> {
    match mapping.text_mappings.len() {
        0 => bail!("this fixture has no text painting yet - paint it in human_solver's `t` view"),
        1 => Ok(vec![&mapping.text_mappings[0]]),
        _ => {
            let wanted = preset_name(options).with_context(|| {
                format!(
                    "{options:?} is neither of the two named presets (Minimal/Full) a \
                     multi-painting fixture can be checked against"
                )
            })?;
            let candidates: Vec<&NamedTextMapping> = mapping
                .text_mappings
                .iter()
                .filter(|named| designates_preset(&named.name, wanted))
                .collect();
            if candidates.is_empty() {
                // Only when *nothing* names a preset: `Minimal` beside a misspelled `Ful` must
                // still fail for `Full`.
                if mapping
                    .text_mappings
                    .iter()
                    .all(|named| !designates_minimal(&named.name) && !designates_full(&named.name))
                {
                    return Ok(mapping.text_mappings.iter().collect());
                }
                let have: Vec<&str> = mapping
                    .text_mappings
                    .iter()
                    .map(|named| named.name.as_str())
                    .collect();
                bail!(
                    "no '{wanted}' painting to hold {options:?} to - this fixture has {have:?}. A \
                     fixture with several paintings needs one named for each preset (optionally \
                     several per preset, as '{wanted} (something)'), or paintings that name no \
                     preset at all, which are read as alternatives for every preset"
                );
            }
            Ok(candidates)
        }
    }
}

/// Whether a painting named `name` is an answer for the `preset` preset: exactly the preset, or
/// the preset, a space and a qualifier (`Minimal (left)`). The qualified form records several
/// defensible renderings under one preset; any of them passes.
pub(crate) fn designates_preset(name: &str, preset: &str) -> bool {
    name == preset
        || name
            .strip_prefix(preset)
            .is_some_and(|rest| rest.starts_with(' '))
}

/// Compares codediff's rendering under `options` against the painting that preset is answerable
/// to, byte for byte (ranges chunk one edit too differently to compare directly). The result is a
/// rate because fixture sizes span three orders of magnitude.
pub fn compare_painting(
    name: &str,
    options: crate::diff::text::RenderOptions,
) -> Result<PaintingComparison> {
    let (before, after) = &*super::handmade_test_code_pair(name)?;
    let diff = codediff_diff_for_painting(before, after)?;
    compare_painting_with_diff(name, options, before, after, &diff)
}

/// codediff's side of a painting comparison, built once per fixture and projected per preset by
/// [`compare_painting_with_diff`].
pub enum PaintingDiff<'code> {
    /// The ordinary case: a tree mapping, projected to text by [`crate::diff::text::TextDiff`].
    Ast {
        ast: crate::diff::ASTDiff,
        node_cache: crate::diff::NodeCache<'code>,
    },
    /// No tree-sitter grammar for the language. Grades against
    /// [`crate::diff::text::plain_text_line_diff`] because that is what the product renders for
    /// such a pair. It never emits `Move`, so the two presets can legitimately coincide.
    PlainText {
        before: Vec<crate::diff::text::RangeMatch>,
        after: Vec<crate::diff::text::RangeMatch>,
    },
}

/// See [`PaintingDiff`].
pub fn codediff_diff_for_painting<'code>(
    before: &'code crate::code::Code,
    after: &'code crate::code::Code,
) -> Result<PaintingDiff<'code>> {
    // Keyed on the code, as the product is: `diff_code` returns `Some(ASTDiff)` even with no
    // trees, which would grade against an empty mapping instead of the fallback a reader sees.
    if before.ast.is_none() || after.ast.is_none() {
        let (before_ranges, after_ranges) =
            crate::diff::text::plain_text_line_diff(&before.contents, &after.contents);
        return Ok(PaintingDiff::PlainText {
            before: before_ranges,
            after: after_ranges,
        });
    }
    let diff = crate::diff::diff_code(before, after);
    let ast = diff
        .ast
        .context("codediff produced no AST diff for a pair that has both trees")?;
    let node_cache = crate::diff::NodeCache::build(before, after);
    Ok(PaintingDiff::Ast { ast, node_cache })
}

/// codediff's own side of a painting comparison, as per-byte labels, `[before, after]`: the ranges
/// the TUI renders under `options`. Built under `options` rather than via `TextDiff::from`
/// (which builds under `FULL`), because some options change the build itself.
pub fn codediff_painting_labels(
    diff: &PaintingDiff,
    before: &crate::code::Code,
    after: &crate::code::Code,
    options: crate::diff::text::RenderOptions,
) -> [Vec<Option<TextLabel>>; 2] {
    let sides: [Vec<crate::diff::text::RangeMatch>; 2] = match diff {
        PaintingDiff::Ast { ast, node_cache } => {
            let text_diff = crate::diff::text::TextDiff::from_with_options(
                before, after, ast, node_cache, options,
            );
            [text_diff.all(0), text_diff.all(1)]
        }
        PaintingDiff::PlainText {
            before: before_ranges,
            after: after_ranges,
        } => [before_ranges.clone(), after_ranges.clone()],
    };

    [0usize, 1usize].map(|side| {
        let contents = if side == 0 {
            &before.contents
        } else {
            &after.contents
        };
        let ranges = crate::diff::text::ranges_for_options(&sides[side], contents, options);
        label_bytes_from_ranges(contents, &ranges)
    })
}

/// [`compare_painting`] over an already computed diff.
pub fn compare_painting_with_diff(
    name: &str,
    options: crate::diff::text::RenderOptions,
    before: &crate::code::Code,
    after: &crate::code::Code,
    diff: &PaintingDiff,
) -> Result<PaintingComparison> {
    let mapping = load(name)?;
    let candidates = paintings_for_mode(&mapping, options)?;

    let ours = codediff_painting_labels(diff, before, after, options);
    let total_bytes = before.contents.len() + after.contents.len();

    // Several paintings under one preset are alternatives: the closest one is the verdict.
    let mut best: Option<PaintingComparison> = None;
    for painting in candidates {
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

        let mut mismatched_bytes = 0usize;
        for (side, contents) in [(0usize, &before.contents), (1usize, &after.contents)] {
            let theirs = label_bytes(contents, &painted[side]);
            mismatched_bytes += ours[side]
                .iter()
                .zip(&theirs)
                .filter(|(ours, theirs)| ours != theirs)
                .count();
        }

        let comparison = PaintingComparison {
            options,
            solution: painting.name.clone(),
            mismatched_bytes,
            total_bytes,
        };
        if best
            .as_ref()
            .is_none_or(|best| comparison.mismatched_bytes < best.mismatched_bytes)
        {
            best = Some(comparison);
        }
    }

    best.context("a preset with no candidate paintings should have been rejected above")
}

/// Asserts codediff's rendering matches the human painting under **both** presets, within
/// `max_percent` of the fixture's bytes - a recorded distance, not a target.
pub fn assert_matches_human_painting_within_limit(name: &str, max_percent: f64) -> Result<()> {
    use crate::diff::text::RenderOptions;

    let (before, after) = &*super::handmade_test_code_pair(name)?;
    let diff = codediff_diff_for_painting(before, after)?;
    let mut failures = Vec::new();
    for options in [RenderOptions::MINIMAL, RenderOptions::FULL] {
        let comparison = compare_painting_with_diff(name, options, before, after, &diff)?;
        if comparison.percent() > max_percent {
            failures.push(format!(
                "  {} vs '{}': {:.3}% ({} of {} bytes) exceeds the {:.3}% limit",
                preset_name(comparison.options).unwrap_or("<custom>"),
                comparison.solution,
                comparison.percent(),
                comparison.mismatched_bytes,
                comparison.total_bytes,
                max_percent,
            ));
        }
    }
    if failures.is_empty() {
        return Ok(());
    }
    bail!(
        "codediff's rendering disagrees with the human painting for '{name}':\n{}",
        failures.join("\n")
    );
}

/// Byte offset -> row, for annotating a disagreement without rescanning the file each time.
struct RowIndex {
    /// Byte offset at which each row starts, ascending.
    starts: Vec<usize>,
}

impl RowIndex {
    fn new(contents: &str) -> Self {
        let mut starts = vec![0usize];
        for (offset, byte) in contents.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(offset + 1);
            }
        }
        Self { starts }
    }

    fn row_at(&mut self, byte: usize) -> usize {
        match self.starts.binary_search(&byte) {
            Ok(row) => row,
            Err(next) => next.saturating_sub(1),
        }
    }
}

/// Path to `name`'s `human_mapping.json`. For a name no dataset holds, a path under `small` that
/// does not exist, which is what a "is this name free" check wants.
pub fn mapping_path(name: &str) -> PathBuf {
    super::diffs_case_dir(name)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("src")
                .join("test")
                .join("data")
                .join("diffs")
                .join("small")
                .join(name)
        })
        .join("human_mapping.json")
}

/// Loads the human mapping for a given test case name.
pub fn load(name: &str) -> Result<HumanMapping> {
    let path = mapping_path(name);
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("reading human mapping at {:?}", path))?;
    let mapping: HumanMapping = serde_json::from_str(&contents)
        .with_context(|| format!("parsing human mapping at {:?}", path))?;
    Ok(mapping)
}

/// Saves the human mapping for a given test case name, overwriting any existing file.
pub fn save(name: &str, mapping: &HumanMapping) -> Result<()> {
    let path = mapping_path(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(mapping)?;
    fs::write(&path, json).with_context(|| format!("writing human mapping to {:?}", path))?;
    Ok(())
}

/// `pub` for `human_solver`, a separate crate.
pub fn path_refs(path: &[String]) -> Vec<&str> {
    path.iter().map(String::as_str).collect()
}

/// What kind of human-authored removal a node is marked with: `Deleted` from the before tree, or
/// `Inserted` in the after tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkKind {
    Deleted,
    Inserted,
}

/// A node's status under a [`HumanMapping`]: unmarked (no entry says anything about it, directly
/// or via an ancestor), matched to a specific counterpart, or marked deleted/inserted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeStatus {
    Unmarked,
    Matched,
    /// `inherited` is true when this node isn't marked directly but an ancestor is marked
    /// with `with_children`, implying this node too.
    Marked {
        kind: MarkKind,
        with_children: bool,
        inherited: bool,
    },
}

/// Resolved node IDs for every entry in a [`HumanMapping`], for O(1) [`NodeStatus`] lookups - see
/// [`rebuild_caches`].
#[derive(Default)]
pub struct Caches {
    pub before_match: HashMap<usize, usize>,
    pub after_match: HashMap<usize, usize>,
    pub before_removed: HashMap<usize, bool>,
    pub after_removed: HashMap<usize, bool>,
    /// The exact [`HumanOperation`] a matched node's pair was recorded as (`before_match` cannot
    /// tell the three match operations apart). A missing key reads as `Identical`.
    pub before_operation: HashMap<usize, HumanOperation>,
    pub after_operation: HashMap<usize, HumanOperation>,
    /// Whether a matched node's `before_path` and `after_path` differ.
    pub before_moved: HashMap<usize, bool>,
    pub after_moved: HashMap<usize, bool>,
    /// Entries that do not resolve against the current trees - counted and reported, not fatal.
    pub unresolved: usize,
    /// Node id -> index into `HumanMapping::groups`, for every member of a group, matched or
    /// leftover (`before_match` only sees the [`representative_entries`] outcome). Only filled
    /// by [`rebuild_caches_for_mapping`].
    pub before_group: HashMap<usize, usize>,
    pub after_group: HashMap<usize, usize>,
}

/// Builds lookup caches from `entries`, skipping (and counting) any entry that doesn't resolve
/// against the current trees rather than failing outright.
pub fn rebuild_caches(
    entries: &[HumanMappingEntry],
    before_root: Node,
    after_root: Node,
) -> Caches {
    let mut caches = Caches::default();
    // A `PathCache` per side: entries sharing high-fanout parents make a fresh scan per entry
    // quadratic, and this runs on every keystroke in `human_solver`.
    let mut before_cache = PathCache::new();
    let mut after_cache = PathCache::new();

    for entry in entries {
        let resolved = match entry.operation {
            HumanOperation::Identical
            | HumanOperation::Update
            | HumanOperation::MatchButNotIdentical => (|| {
                let before_path = entry.before_path.as_ref()?;
                let after_path = entry.after_path.as_ref()?;
                let b = before_cache
                    .resolve(before_root, &path_refs(before_path))
                    .ok()?;
                let a = after_cache
                    .resolve(after_root, &path_refs(after_path))
                    .ok()?;
                caches.before_match.insert(b.id(), a.id());
                caches.after_match.insert(a.id(), b.id());
                caches.before_operation.insert(b.id(), entry.operation);
                caches.after_operation.insert(a.id(), entry.operation);
                let moved = before_path != after_path;
                caches.before_moved.insert(b.id(), moved);
                caches.after_moved.insert(a.id(), moved);
                Some(())
            })(),
            HumanOperation::Delete | HumanOperation::DeleteWithChildren => (|| {
                let before_path = entry.before_path.as_ref()?;
                let b = before_cache
                    .resolve(before_root, &path_refs(before_path))
                    .ok()?;
                caches.before_removed.insert(
                    b.id(),
                    entry.operation == HumanOperation::DeleteWithChildren,
                );
                Some(())
            })(),
            HumanOperation::Insert | HumanOperation::InsertWithChildren => (|| {
                let after_path = entry.after_path.as_ref()?;
                let a = after_cache
                    .resolve(after_root, &path_refs(after_path))
                    .ok()?;
                caches.after_removed.insert(
                    a.id(),
                    entry.operation == HumanOperation::InsertWithChildren,
                );
                Some(())
            })(),
        };

        if resolved.is_none() {
            caches.unresolved += 1;
        }
    }

    caches
}

/// Nodes in `root`'s tree the mapping says nothing about. Pass [`status_before`] or
/// [`status_after`], matching `root`'s side. An empty mapping reports every node.
pub fn unmarked_node_count(
    root: Node,
    caches: &Caches,
    status_fn: fn(Node, &Caches) -> NodeStatus,
) -> usize {
    let mut count = 0;
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if status_fn(node, caches) == NodeStatus::Unmarked {
            count += 1;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    count
}

/// [`rebuild_caches`] with `mapping.groups` folded in through [`representative_entries`], plus
/// `before_group`/`after_group` for every group member. If a group does not resolve, falls back to
/// `mapping.entries` alone rather than failing.
pub fn rebuild_caches_for_mapping(
    mapping: &HumanMapping,
    before_root: Node,
    after_root: Node,
) -> Caches {
    let entries = representative_entries(mapping, before_root, after_root)
        .unwrap_or_else(|_| mapping.entries.clone());
    let mut caches = rebuild_caches(&entries, before_root, after_root);

    let mut before_cache = PathCache::new();
    let mut after_cache = PathCache::new();
    for (idx, group) in mapping.groups.iter().enumerate() {
        for path in &group.before_paths {
            if let Ok(node) = before_cache.resolve(before_root, &path_refs(path)) {
                caches.before_group.insert(node.id(), idx);
            }
        }
        for path in &group.after_paths {
            if let Ok(node) = after_cache.resolve(after_root, &path_refs(path)) {
                caches.after_group.insert(node.id(), idx);
            }
        }
    }

    caches
}

/// True if some strict ancestor of `node` is marked with `with_children = true` in `removed`.
pub fn is_inherited_removed(node: Node, removed: &HashMap<usize, bool>) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if removed.get(&parent.id()) == Some(&true) {
            return true;
        }
        current = parent;
    }
    false
}

/// The exact [`HumanOperation`] a matched before-node's pair was recorded as, or `None` if `node`
/// isn't matched (or `caches` was built by hand).
pub fn match_operation_before(node: Node, caches: &Caches) -> Option<HumanOperation> {
    caches.before_operation.get(&node.id()).copied()
}

/// After-side counterpart of [`match_operation_before`].
pub fn match_operation_after(node: Node, caches: &Caches) -> Option<HumanOperation> {
    caches.after_operation.get(&node.id()).copied()
}

/// Whether a `Matched` before-node's pair was recorded as `Identical`. `true` when
/// [`match_operation_before`] is `None`, so hand-built caches render matched nodes as unchanged.
pub fn is_identical_before(node: Node, caches: &Caches) -> bool {
    match_operation_before(node, caches).is_none_or(|op| op == HumanOperation::Identical)
}

/// After-side counterpart of [`is_identical_before`].
pub fn is_identical_after(node: Node, caches: &Caches) -> bool {
    match_operation_after(node, caches).is_none_or(|op| op == HumanOperation::Identical)
}

/// Whether a matched before-node's path differs from its pair's (see `Caches::before_moved`);
/// `false` when unknown.
pub fn is_moved_before(node: Node, caches: &Caches) -> bool {
    caches
        .before_moved
        .get(&node.id())
        .copied()
        .unwrap_or(false)
}

/// After-side counterpart of [`is_moved_before`].
pub fn is_moved_after(node: Node, caches: &Caches) -> bool {
    caches.after_moved.get(&node.id()).copied().unwrap_or(false)
}

pub fn status_before(node: Node, caches: &Caches) -> NodeStatus {
    if caches.before_match.contains_key(&node.id()) {
        return NodeStatus::Matched;
    }
    if let Some(&with_children) = caches.before_removed.get(&node.id()) {
        return NodeStatus::Marked {
            kind: MarkKind::Deleted,
            with_children,
            inherited: false,
        };
    }
    if is_inherited_removed(node, &caches.before_removed) {
        return NodeStatus::Marked {
            kind: MarkKind::Deleted,
            with_children: true,
            inherited: true,
        };
    }
    NodeStatus::Unmarked
}

pub fn status_after(node: Node, caches: &Caches) -> NodeStatus {
    if caches.after_match.contains_key(&node.id()) {
        return NodeStatus::Matched;
    }
    if let Some(&with_children) = caches.after_removed.get(&node.id()) {
        return NodeStatus::Marked {
            kind: MarkKind::Inserted,
            with_children,
            inherited: false,
        };
    }
    if is_inherited_removed(node, &caches.after_removed) {
        return NodeStatus::Marked {
            kind: MarkKind::Inserted,
            with_children: true,
            inherited: true,
        };
    }
    NodeStatus::Unmarked
}

/// Which side of the diff a [`Mismatch`]'s `node_id` belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Before,
    After,
}

/// One disagreement between the human mapping and codediff's output, tagged with the node most
/// responsible, so callers can tell a mismatch on a visible node from one on invisible scaffolding
/// (see [`crate::diff::nodes::structurally_visible_node_ids`]). `node_id` is `0` (never a real
/// id) for a mismatch about no single node.
#[derive(Debug, Clone)]
pub struct Mismatch {
    pub message: String,
    pub node_id: usize,
    pub side: Side,
}

fn node_kind_for_id(root: Node, node_id: usize) -> String {
    if node_id == 0 {
        return "0".to_string();
    }

    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.id() == node_id {
            return n.kind().to_string();
        }

        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }

    "None".to_string()
}

/// Pushes a mismatch for every node in `node`'s subtree (inclusive) that `node_map` does not map
/// to zero. `side` is `node`'s side.
fn check_subtree_maps_to_zero(
    node: Node,
    node_map: &rustc_hash::FxHashMap<usize, usize>,
    context: &str,
    mismatches: &mut Vec<Mismatch>,
    lookup_root: Node,
    side: Side,
) {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        match node_map.get(&n.id()) {
            Some(0) => {}
            other => {
                let mapped_kind = match other {
                    Some(&mapped_id) => node_kind_for_id(lookup_root, mapped_id),
                    None => "None".to_string(),
                };
                mismatches.push(Mismatch {
                    message: format!(
                        "{}: descendant node '{}' was expected to be removed (mapped to 0), but was mapped to {}",
                        context,
                        n.kind(),
                        mapped_kind
                    ),
                    node_id: n.id(),
                    side,
                })
            }
        }

        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
}

/// Every node id in `node`'s subtree, `node` included.
fn subtree_ids(node: Node) -> std::collections::HashSet<usize> {
    let mut ids = std::collections::HashSet::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        ids.insert(n.id());
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    ids
}

/// Pushes a mismatch for every node in `subtree_root`'s subtree (inclusive) whose counterpart via
/// `node_map` is not in `counterpart_ids` - one half of [`check_subtree_maps_within`].
/// `counterpart_lookup_root` is only for describing the wrongly-mapped-to node.
fn check_subtree_closed_within(
    subtree_root: Node,
    node_map: &rustc_hash::FxHashMap<usize, usize>,
    counterpart_ids: &std::collections::HashSet<usize>,
    counterpart_lookup_root: Node,
    context: &str,
    mismatches: &mut Vec<Mismatch>,
    side: Side,
) {
    let mut stack = vec![subtree_root];
    while let Some(n) = stack.pop() {
        match node_map.get(&n.id()) {
            Some(mapped_id) if counterpart_ids.contains(mapped_id) => {}
            other => {
                let mapped_kind = match other {
                    Some(&mapped_id) => node_kind_for_id(counterpart_lookup_root, mapped_id),
                    None => "None".to_string(),
                };
                mismatches.push(Mismatch {
                    message: format!(
                        "{}: descendant node '{}' was expected to map within the matched pair's counterpart subtree, but was mapped to {}",
                        context,
                        n.kind(),
                        mapped_kind
                    ),
                    node_id: n.id(),
                    side,
                });
            }
        }

        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
}

/// Pushes a mismatch for every node in either subtree that is not mapped into the other subtree:
/// the two form a *closed* pairing. For a [`MultiMapGroup`] with `with_children`, where which pair
/// matched is only known after the fact, so a closure property replaces fixed expected ids.
#[allow(clippy::too_many_arguments)]
fn check_subtree_maps_within(
    before_node: Node,
    after_node: Node,
    before_node_map: &rustc_hash::FxHashMap<usize, usize>,
    after_node_map: &rustc_hash::FxHashMap<usize, usize>,
    before_root: Node,
    after_root: Node,
    context: &str,
    mismatches: &mut Vec<Mismatch>,
) {
    let before_ids = subtree_ids(before_node);
    let after_ids = subtree_ids(after_node);
    check_subtree_closed_within(
        before_node,
        before_node_map,
        &after_ids,
        after_root,
        context,
        mismatches,
        Side::Before,
    );
    check_subtree_closed_within(
        after_node,
        after_node_map,
        &before_ids,
        before_root,
        context,
        mismatches,
        Side::After,
    );
}

/// " (op X, reason Y)" for the mapping the before node landed in, naming the pass responsible.
/// Empty when there is none.
fn actual_mapping_info(
    diff_ast: &ASTDiff,
    before_id: usize,
    actual_partner: Option<usize>,
) -> String {
    let Some(partner) = actual_partner else {
        return String::new();
    };
    match diff_ast.mapping.get(&(before_id, partner)) {
        Some(m) => format!(" (op {:?}, reason {:?})", m.operation, m.reason),
        None => String::new(),
    }
}

/// After-side counterpart of `actual_mapping_info` (mapping keys are `(before, after)`).
fn actual_mapping_info_after(
    diff_ast: &ASTDiff,
    after_id: usize,
    actual_partner: Option<usize>,
) -> String {
    let Some(partner) = actual_partner else {
        return String::new();
    };
    match diff_ast.mapping.get(&(partner, after_id)) {
        Some(m) => format!(" (op {:?}, reason {:?})", m.operation, m.reason),
        None => String::new(),
    }
}

/// The [`ASTMappingOperation`] codediff is expected to have chosen for a matched pair, given the
/// human's [`HumanOperation`] for that pair.
fn expected_ast_operation(operation: HumanOperation) -> Option<ASTMappingOperation> {
    match operation {
        HumanOperation::Identical => Some(ASTMappingOperation::Identical),
        HumanOperation::Update => Some(ASTMappingOperation::Update),
        HumanOperation::MatchButNotIdentical => Some(ASTMappingOperation::MatchButNotIdentical),
        HumanOperation::Delete
        | HumanOperation::DeleteWithChildren
        | HumanOperation::Insert
        | HumanOperation::InsertWithChildren => None,
    }
}

/// A node's `owned_text_hash`, or 0 without a metadata entry ("owns nothing", so a missing entry
/// never reads as a change).
fn owned_text_hash(metadata: &ASTMetadata, id: usize) -> u64 {
    metadata
        .node_info
        .get(&id)
        .map(|info| info.owned_text_hash)
        .unwrap_or(0)
}

/// Total edit cost of a `HumanMapping` under the unit-cost model of `crate::diff::cost::diff_cost`,
/// via the same `operation_cost` table so the two stay comparable. The metadata must come from the
/// same parse as the roots (node ids are per-parse).
///
/// Sums only annotated entries, which is correct only while the mapping covers every real change:
/// an unannotated edit silently undercounts the human side.
pub fn human_mapping_cost(
    mapping: &HumanMapping,
    before_root: Node,
    after_root: Node,
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
) -> Result<u64> {
    let mut before_cache = PathCache::new();
    let mut after_cache = PathCache::new();

    // Groups contribute via one representative pairing, which is only well defined while no
    // `MatchButNotIdentical` group spans nodes that own text directly (see
    // `ASTNodeMetadata::owned_text_hash`): there the pairing would decide how many pairs pay
    // `COST_UPDATE`. An `AllToAll` group's surplus members are charged as matches, not as
    // deletes/inserts: the group says they are not gone.
    let entries = representative_entries(mapping, before_root, after_root)?;

    let mut total = 0u64;
    for entry in &entries {
        let (operation, subtree_size, owned_text_changed) = match entry.operation {
            HumanOperation::Identical => (ASTMappingOperation::Identical, 1, false),
            HumanOperation::Update => (ASTMappingOperation::Update, 1, false),
            HumanOperation::MatchButNotIdentical => {
                // A node owning text directly carries a difference no descendant entry accounts
                // for, so it is charged here or nowhere. A missing path reads as unchanged: this is
                // a cost function, and `check_entry` rejects malformed entries.
                let changed = match (entry.before_path.as_ref(), entry.after_path.as_ref()) {
                    (Some(before_path), Some(after_path)) => {
                        let before = before_cache.resolve(before_root, &path_refs(before_path));
                        let after = after_cache.resolve(after_root, &path_refs(after_path));
                        match (before, after) {
                            (Ok(before), Ok(after)) => {
                                owned_text_hash(before_metadata, before.id())
                                    != owned_text_hash(after_metadata, after.id())
                            }
                            _ => false,
                        }
                    }
                    _ => false,
                };
                (ASTMappingOperation::MatchButNotIdentical, 1, changed)
            }
            HumanOperation::Delete => (ASTMappingOperation::Delete, 1, false),
            HumanOperation::Insert => (ASTMappingOperation::Insert, 1, false),
            HumanOperation::DeleteWithChildren => {
                let path = entry
                    .before_path
                    .as_ref()
                    .context("DeleteWithChildren entry is missing before_path")?;
                let node = before_cache
                    .resolve(before_root, &path_refs(path))
                    .with_context(|| format!("resolving before_path {:?}", path))?;
                let size = before_metadata
                    .node_to_subtree_size
                    .get(&node.id())
                    .copied()
                    .unwrap_or(1);
                (ASTMappingOperation::DeleteWithChildren, size, false)
            }
            HumanOperation::InsertWithChildren => {
                let path = entry
                    .after_path
                    .as_ref()
                    .context("InsertWithChildren entry is missing after_path")?;
                let node = after_cache
                    .resolve(after_root, &path_refs(path))
                    .with_context(|| format!("resolving after_path {:?}", path))?;
                let size = after_metadata
                    .node_to_subtree_size
                    .get(&node.id())
                    .copied()
                    .unwrap_or(1);
                (ASTMappingOperation::InsertWithChildren, size, false)
            }
        };
        total += operation_cost(&operation, subtree_size, owned_text_changed);
    }
    Ok(total)
}

/// [`human_mapping_cost`] for fixture `name`, resolved against a fresh parse of `before`/`after`.
pub fn human_mapping_cost_for(
    name: &str,
    before: &crate::code::Code,
    after: &crate::code::Code,
) -> Result<u64> {
    let mapping = load(name)?;
    let before_ast = before.ast.as_ref().context("Before code has no AST")?;
    let after_ast = after.ast.as_ref().context("After code has no AST")?;
    let before_metadata = crate::code::metadata::metadata_of(before);
    let after_metadata = crate::code::metadata::metadata_of(after);
    human_mapping_cost(
        &mapping,
        before_ast.root_node(),
        after_ast.root_node(),
        &before_metadata,
        &after_metadata,
    )
}

/// A synthetic `ASTDiff` from `name`'s human mapping, so machinery that consumes an `ASTDiff`
/// (e.g. `diff::text::TextDiff`) treats the human mapping like codediff's output.
///
/// `cost`/`reason` are placeholders; the human format records neither. `reason` is not inert:
/// `diff::text`'s `identical_or_move` reads it, so `painting_failure_census` borrows codediff's
/// reason for shared pairs before rendering.
pub fn as_ast_diff(
    name: &str,
    before: &crate::code::Code,
    after: &crate::code::Code,
) -> Result<ASTDiff> {
    let mapping = load(name)?;
    as_ast_diff_for_mapping(&mapping, before, after)
}

/// [`as_ast_diff`] for an already-loaded [`HumanMapping`].
pub fn as_ast_diff_for_mapping(
    mapping: &HumanMapping,
    before: &crate::code::Code,
    after: &crate::code::Code,
) -> Result<ASTDiff> {
    let before_ast = before.ast.as_ref().context("Before code has no AST")?;
    let after_ast = after.ast.as_ref().context("After code has no AST")?;
    let before_root = before_ast.root_node();
    let after_root = after_ast.root_node();

    let mut before_cache = PathCache::new();
    let mut after_cache = PathCache::new();

    let entries = representative_entries(mapping, before_root, after_root)?;

    let mut diff = ASTDiff::default();
    for entry in &entries {
        let before_id = match &entry.before_path {
            Some(path) => before_cache
                .resolve(before_root, &path_refs(path))
                .with_context(|| format!("resolving before_path {:?}", path))?
                .id(),
            None => 0,
        };
        let after_id = match &entry.after_path {
            Some(path) => after_cache
                .resolve(after_root, &path_refs(path))
                .with_context(|| format!("resolving after_path {:?}", path))?
                .id(),
            None => 0,
        };
        let operation = match entry.operation {
            HumanOperation::Identical => ASTMappingOperation::Identical,
            HumanOperation::Update => ASTMappingOperation::Update,
            HumanOperation::MatchButNotIdentical => ASTMappingOperation::MatchButNotIdentical,
            HumanOperation::Delete => ASTMappingOperation::Delete,
            HumanOperation::DeleteWithChildren => ASTMappingOperation::DeleteWithChildren,
            HumanOperation::Insert => ASTMappingOperation::Insert,
            HumanOperation::InsertWithChildren => ASTMappingOperation::InsertWithChildren,
        };
        diff.add_mapping(
            before_id,
            after_id,
            ASTMapping {
                cost: 0,
                operation,
                reason: ASTMappingReason::default(),
            },
        );
    }
    Ok(diff)
}

/// `mapping.entries` plus one *deterministic* representative pairing per group, flattened into
/// plain entries: members sorted by start byte and zipped pairwise under the group's `operation`,
/// the larger side's leftovers deleted/inserted per `with_children`.
///
/// An [`GroupPairing::AllToAll`] group has no leftovers: its surplus members each pair with the last
/// member of the shorter side, putting one node in several entries. Deleting them would misstate
/// the ground truth.
///
/// *A* valid solution, not *the* solution: used for cost and display ([`human_mapping_cost`],
/// [`as_ast_diff_for_mapping`]), **never** for pass/fail, which is [`check_group_entry`].
pub fn representative_entries(
    mapping: &HumanMapping,
    before_root: Node,
    after_root: Node,
) -> Result<Vec<HumanMappingEntry>> {
    let mut entries = mapping.entries.clone();
    if mapping.groups.is_empty() {
        return Ok(entries);
    }

    let mut before_cache = PathCache::new();
    let mut after_cache = PathCache::new();

    for group in &mapping.groups {
        let mut before_nodes: Vec<Node> = group
            .before_paths
            .iter()
            .map(|path| {
                before_cache
                    .resolve(before_root, &path_refs(path))
                    .with_context(|| format!("resolving multi-map before_path {:?}", path))
            })
            .collect::<Result<_>>()?;
        let mut after_nodes: Vec<Node> = group
            .after_paths
            .iter()
            .map(|path| {
                after_cache
                    .resolve(after_root, &path_refs(path))
                    .with_context(|| format!("resolving multi-map after_path {:?}", path))
            })
            .collect::<Result<_>>()?;

        // Sorted only for determinism.
        before_nodes.sort_by_key(|n| n.start_byte());
        after_nodes.sort_by_key(|n| n.start_byte());

        let paired = before_nodes.len().min(after_nodes.len());
        for i in 0..paired {
            entries.push(HumanMappingEntry {
                operation: group.operation,
                before_path: Some(path_for_node(before_nodes[i])),
                after_path: Some(path_for_node(after_nodes[i])),
            });
        }
        if group.pairing == GroupPairing::AllToAll && paired > 0 {
            // Nothing is left over: the surplus pairs with the last member of the shorter side.
            for &b in &before_nodes[paired..] {
                entries.push(HumanMappingEntry {
                    operation: group.operation,
                    before_path: Some(path_for_node(b)),
                    after_path: Some(path_for_node(after_nodes[paired - 1])),
                });
            }
            for &a in &after_nodes[paired..] {
                entries.push(HumanMappingEntry {
                    operation: group.operation,
                    before_path: Some(path_for_node(before_nodes[paired - 1])),
                    after_path: Some(path_for_node(a)),
                });
            }
            continue;
        }
        let delete_op = if group.with_children {
            HumanOperation::DeleteWithChildren
        } else {
            HumanOperation::Delete
        };
        for &b in &before_nodes[paired..] {
            entries.push(HumanMappingEntry {
                operation: delete_op,
                before_path: Some(path_for_node(b)),
                after_path: None,
            });
        }
        let insert_op = if group.with_children {
            HumanOperation::InsertWithChildren
        } else {
            HumanOperation::Insert
        };
        for &a in &after_nodes[paired..] {
            entries.push(HumanMappingEntry {
                operation: insert_op,
                before_path: None,
                after_path: Some(path_for_node(a)),
            });
        }
    }

    Ok(entries)
}

fn check_entry<'b, 'a>(
    entry: &HumanMappingEntry,
    before_root: Node<'b>,
    after_root: Node<'a>,
    diff_ast: &ASTDiff,
    mismatches: &mut Vec<Mismatch>,
    before_cache: &mut PathCache<'b>,
    after_cache: &mut PathCache<'a>,
) -> Result<()> {
    match entry.operation {
        HumanOperation::Identical
        | HumanOperation::Update
        | HumanOperation::MatchButNotIdentical => {
            let before_path = entry
                .before_path
                .as_ref()
                .with_context(|| format!("{:?} entry is missing before_path", entry.operation))?;
            let after_path = entry
                .after_path
                .as_ref()
                .with_context(|| format!("{:?} entry is missing after_path", entry.operation))?;

            let before_node = before_cache
                .resolve(before_root, &path_refs(before_path))
                .with_context(|| format!("resolving before_path {:?}", before_path))?;
            let after_node = after_cache
                .resolve(after_root, &path_refs(after_path))
                .with_context(|| format!("resolving after_path {:?}", after_path))?;

            let actual_partner = diff_ast.before_node_map.get(&before_node.id()).copied();
            if actual_partner != Some(after_node.id()) {
                let mapped_kind = match actual_partner {
                    Some(mapped_id) => node_kind_for_id(after_root, mapped_id),
                    None => "None".to_string(),
                };
                mismatches.push(Mismatch {
                    message: format!(
                        "{:?} {:?} <-> {:?}: expected before node '{}' to map to after node '{}', but it mapped to {}{}",
                        entry.operation,
                        before_path,
                        after_path,
                        before_node.kind(),
                        after_node.kind(),
                        mapped_kind,
                        actual_mapping_info(diff_ast, before_node.id(), actual_partner)
                    ),
                    node_id: before_node.id(),
                    side: Side::Before,
                });
                return Ok(());
            }

            let expected_op = expected_ast_operation(entry.operation).expect(
                "Identical/Update/MatchButNotIdentical always have an expected ASTMappingOperation",
            );
            match diff_ast.mapping.get(&(before_node.id(), after_node.id())) {
                Some(actual_mapping) if actual_mapping.operation == expected_op => {}
                Some(actual_mapping) => mismatches.push(Mismatch {
                    message: format!(
                        "{:?} {:?} <-> {:?}: expected codediff operation {:?}, but it chose {:?}",
                        entry.operation, before_path, after_path, expected_op, actual_mapping.operation
                    ),
                    node_id: before_node.id(),
                    side: Side::Before,
                }),
                None => mismatches.push(Mismatch {
                    message: format!(
                        "{:?} {:?} <-> {:?}: nodes are mapped to each other but have no ASTMapping entry (unexpected)",
                        entry.operation, before_path, after_path
                    ),
                    node_id: before_node.id(),
                    side: Side::Before,
                }),
            }
        }
        HumanOperation::Delete | HumanOperation::DeleteWithChildren => {
            let before_path = entry
                .before_path
                .as_ref()
                .context("Delete entry is missing before_path")?;
            let before_node = before_cache
                .resolve(before_root, &path_refs(before_path))
                .with_context(|| format!("resolving before_path {:?}", before_path))?;

            if entry.operation == HumanOperation::DeleteWithChildren {
                check_subtree_maps_to_zero(
                    before_node,
                    &diff_ast.before_node_map,
                    &format!("Delete (with children) {:?}", before_path),
                    mismatches,
                    after_root,
                    Side::Before,
                );
            } else {
                let actual = diff_ast.before_node_map.get(&before_node.id()).copied();
                if actual != Some(0) {
                    let mapped_kind = match actual {
                        Some(mapped_id) => node_kind_for_id(after_root, mapped_id),
                        None => "None".to_string(),
                    };
                    mismatches.push(Mismatch {
                        message: format!(
                            "Delete {:?}: expected before node '{}' to be removed (mapped to 0), but it mapped to {}{}",
                            before_path,
                            before_node.kind(),
                            mapped_kind,
                            actual_mapping_info(diff_ast, before_node.id(), actual)
                        ),
                        node_id: before_node.id(),
                        side: Side::Before,
                    });
                }
            }
        }
        HumanOperation::Insert | HumanOperation::InsertWithChildren => {
            let after_path = entry
                .after_path
                .as_ref()
                .context("Insert entry is missing after_path")?;
            let after_node = after_cache
                .resolve(after_root, &path_refs(after_path))
                .with_context(|| format!("resolving after_path {:?}", after_path))?;

            if entry.operation == HumanOperation::InsertWithChildren {
                check_subtree_maps_to_zero(
                    after_node,
                    &diff_ast.after_node_map,
                    &format!("Insert (with children) {:?}", after_path),
                    mismatches,
                    before_root,
                    Side::After,
                );
            } else {
                let actual = diff_ast.after_node_map.get(&after_node.id()).copied();
                if actual != Some(0) {
                    let mapped_kind = match actual {
                        Some(mapped_id) => node_kind_for_id(before_root, mapped_id),
                        None => "None".to_string(),
                    };
                    mismatches.push(Mismatch {
                        message: format!(
                            "Insert {:?}: expected after node '{}' to be new (mapped to 0), but it mapped to {}{}",
                            after_path,
                            after_node.kind(),
                            mapped_kind,
                            actual_mapping_info_after(diff_ast, after_node.id(), actual)
                        ),
                        node_id: after_node.id(),
                        side: Side::After,
                    });
                }
            }
        }
    }

    Ok(())
}

/// Checks one [`MultiMapGroup`] against `diff_ast`. For `AnyOneToOne`:
///
/// 1. Every before member is matched to an after member or deleted; matched outside the group is a
///    mismatch.
/// 2. Every unclaimed after member is inserted.
/// 3. Exactly `min(N, M)` pairs are found. This catches deleting *and* inserting where a match was
///    possible, which steps 1-2 accept node by node.
/// 4. Every pair uses an operation the group's `operation` allows.
/// 5. With `with_children`: matched pairs close within each other ([`check_subtree_maps_within`])
///    and leftovers' whole subtrees are deleted/inserted.
///
/// For `AllToAll`, deleted and inserted are not valid fates in steps 1-2, step 3 does not apply,
/// and step 5's closure is over the union of the members (no leftovers exist). A one-to-one diff
/// therefore always reports at least `|N - M|` mismatches for such a group.
fn check_group_entry<'b, 'a>(
    group: &MultiMapGroup,
    before_root: Node<'b>,
    after_root: Node<'a>,
    diff_ast: &ASTDiff,
    mismatches: &mut Vec<Mismatch>,
    before_cache: &mut PathCache<'b>,
    after_cache: &mut PathCache<'a>,
) -> Result<()> {
    let before_nodes: Vec<Node<'b>> = group
        .before_paths
        .iter()
        .map(|path| {
            before_cache
                .resolve(before_root, &path_refs(path))
                .with_context(|| format!("resolving multi-map before_path {:?}", path))
        })
        .collect::<Result<_>>()?;
    let after_nodes: Vec<Node<'a>> = group
        .after_paths
        .iter()
        .map(|path| {
            after_cache
                .resolve(after_root, &path_refs(path))
                .with_context(|| format!("resolving multi-map after_path {:?}", path))
        })
        .collect::<Result<_>>()?;

    // Stands for the group as a whole in aggregate mismatches (bad `operation`, wrong pair count).
    let (group_node_id, group_side) = before_nodes
        .first()
        .map(|n| (n.id(), Side::Before))
        .or_else(|| after_nodes.first().map(|n| (n.id(), Side::After)))
        .unwrap_or((0, Side::Before));

    let all_to_all = group.pairing == GroupPairing::AllToAll;
    let context = format!(
        "{} group ({} before <-> {} after, {:?}{})",
        if all_to_all {
            "all-to-all"
        } else {
            "multi-map"
        },
        before_nodes.len(),
        after_nodes.len(),
        group.operation,
        if group.with_children {
            ", with children"
        } else {
            ""
        }
    );

    // A *set*: which operation is right depends on which pairing was realized. An `Identical`
    // group means every member hashes equal, so any pair must be `Identical`. A
    // `MatchButNotIdentical` group only says the members are not all equal, so a realized pair may
    // be `Identical`, and a childless pair can only ever be `Identical`/`Update` (`classify_match`).
    let accepted_ops: &[ASTMappingOperation] = match group.operation {
        HumanOperation::Identical => &[ASTMappingOperation::Identical],
        HumanOperation::MatchButNotIdentical => &[
            ASTMappingOperation::Identical,
            ASTMappingOperation::Update,
            ASTMappingOperation::MatchButNotIdentical,
        ],
        other => {
            mismatches.push(Mismatch {
                message: format!(
                    "{context}: operation must be Identical or MatchButNotIdentical, got {other:?}"
                ),
                node_id: group_node_id,
                side: group_side,
            });
            return Ok(());
        }
    };

    let after_ids: std::collections::HashSet<usize> = after_nodes.iter().map(Node::id).collect();

    // What a member mapped to 0 is, in words: a valid fate for an `AnyOneToOne` leftover, a
    // mismatch for an `AllToAll` member.
    let or_removed = |removed: &str| -> String {
        if all_to_all {
            String::new()
        } else {
            format!(" or be {removed}")
        }
    };

    let mut matched_pairs: Vec<(Node<'b>, Node<'a>)> = Vec::new();
    let mut leftover_before: Vec<Node<'b>> = Vec::new();
    for &b in &before_nodes {
        let actual = diff_ast.before_node_map.get(&b.id()).copied();
        match actual {
            Some(0) if !all_to_all => leftover_before.push(b),
            Some(a_id) if after_ids.contains(&a_id) => {
                let a = *after_nodes
                    .iter()
                    .find(|n| n.id() == a_id)
                    .expect("a_id came from after_ids, which is built from after_nodes");
                matched_pairs.push((b, a));
            }
            other => {
                let mapped_kind = match other {
                    Some(mapped_id) => node_kind_for_id(after_root, mapped_id),
                    None => "None".to_string(),
                };
                mismatches.push(Mismatch {
                    message: format!(
                        "{context}: before node '{}' was expected to match within the group{}, but it mapped to {}{}",
                        b.kind(),
                        or_removed("deleted"),
                        mapped_kind,
                        actual_mapping_info(diff_ast, b.id(), other)
                    ),
                    node_id: b.id(),
                    side: Side::Before,
                });
            }
        }
    }

    let matched_after_ids: std::collections::HashSet<usize> =
        matched_pairs.iter().map(|(_, a)| a.id()).collect();
    let mut leftover_after: Vec<Node<'a>> = Vec::new();
    for &a in &after_nodes {
        if matched_after_ids.contains(&a.id()) {
            continue;
        }
        let actual = diff_ast.after_node_map.get(&a.id()).copied();
        match actual {
            Some(0) if !all_to_all => leftover_after.push(a),
            other => {
                let mapped_kind = match other {
                    Some(mapped_id) => node_kind_for_id(before_root, mapped_id),
                    None => "None".to_string(),
                };
                mismatches.push(Mismatch {
                    message: format!(
                        "{context}: after node '{}' was expected to match within the group{}, but it mapped to {}{}",
                        a.kind(),
                        or_removed("inserted"),
                        mapped_kind,
                        actual_mapping_info_after(diff_ast, a.id(), other)
                    ),
                    node_id: a.id(),
                    side: Side::After,
                });
            }
        }
    }

    // An all-to-all group has no expected pair count: each member's own fate was checked above,
    // and a one-to-one output has no number of pairs that would be "right" for N ≠ M.
    let expected_matched = before_nodes.len().min(after_nodes.len());
    if !all_to_all && matched_pairs.len() != expected_matched {
        mismatches.push(Mismatch {
            message: format!(
                "{context}: expected exactly {expected_matched} pair(s) matched within the group, but codediff matched {}",
                matched_pairs.len()
            ),
            node_id: group_node_id,
            side: group_side,
        });
    }

    for &(b, a) in &matched_pairs {
        match diff_ast.mapping.get(&(b.id(), a.id())) {
            Some(actual_mapping) if accepted_ops.contains(&actual_mapping.operation) => {}
            Some(actual_mapping) => mismatches.push(Mismatch {
                message: format!(
                    "{context}: pair '{}' <-> '{}' expected codediff operation {accepted_ops:?}, but it chose {:?}",
                    b.kind(),
                    a.kind(),
                    actual_mapping.operation
                ),
                node_id: b.id(),
                side: Side::Before,
            }),
            None => mismatches.push(Mismatch {
                message: format!(
                    "{context}: pair '{}' <-> '{}' are mapped to each other but have no ASTMapping entry (unexpected)",
                    b.kind(),
                    a.kind()
                ),
                node_id: b.id(),
                side: Side::Before,
            }),
        }
    }

    if group.with_children && all_to_all {
        // Closure over the union: a descendant of any before member may land in any after member's
        // subtree, and vice versa, since the members themselves are not paired off.
        let before_union: std::collections::HashSet<usize> =
            before_nodes.iter().flat_map(|n| subtree_ids(*n)).collect();
        let after_union: std::collections::HashSet<usize> =
            after_nodes.iter().flat_map(|n| subtree_ids(*n)).collect();
        for &b in &before_nodes {
            check_subtree_closed_within(
                b,
                &diff_ast.before_node_map,
                &after_union,
                after_root,
                &context,
                mismatches,
                Side::Before,
            );
        }
        for &a in &after_nodes {
            check_subtree_closed_within(
                a,
                &diff_ast.after_node_map,
                &before_union,
                before_root,
                &context,
                mismatches,
                Side::After,
            );
        }
    } else if group.with_children {
        for &(b, a) in &matched_pairs {
            check_subtree_maps_within(
                b,
                a,
                &diff_ast.before_node_map,
                &diff_ast.after_node_map,
                before_root,
                after_root,
                &context,
                mismatches,
            );
        }
        for &b in &leftover_before {
            check_subtree_maps_to_zero(
                b,
                &diff_ast.before_node_map,
                &format!("{context} (leftover delete)"),
                mismatches,
                after_root,
                Side::Before,
            );
        }
        for &a in &leftover_after {
            check_subtree_maps_to_zero(
                a,
                &diff_ast.after_node_map,
                &format!("{context} (leftover insert)"),
                mismatches,
                before_root,
                Side::After,
            );
        }
    }

    Ok(())
}

/// One `diff_code` run's mapping, keyed by node *path*: node ids can differ between separate
/// parses of identical source, so only paths make independently parsed runs comparable.
type PathKeyedMapping = HashMap<(Vec<String>, Vec<String>), ASTMappingOperation>;

/// Runs `diff_code_with_config` on a *fresh* parse, which is the point: it reproduces the
/// arena-layout variation a separate process would see.
fn diff_paths_with_config(
    before_source: &str,
    after_source: &str,
    language: &crate::code::Language,
    config: &crate::diff::HeuristicConfig,
) -> PathKeyedMapping {
    let before = crate::code::Code::from_string(before_source, language);
    let after = crate::code::Code::from_string(after_source, language);
    let diff = crate::diff::diff_code_with_config(&before, &after, config);
    let node_cache = NodeCache::build(&before, &after);
    let diff_ast = diff.ast.expect("Diff has no AST");

    let mut before_cache = PathCache::new();
    let mut after_cache = PathCache::new();

    diff_ast
        .mapping
        .iter()
        .filter_map(|(&(b, a), m)| {
            let before_path = before_cache.path_of(*node_cache.before.get(&b)?);
            let after_path = after_cache.path_of(*node_cache.after.get(&a)?);
            Some(((before_path, after_path), m.operation.clone()))
        })
        .collect()
}

/// Describes every pair whose presence or operation differs between two runs. Empty when they
/// agree.
fn describe_path_map_differences(
    run_number: usize,
    baseline: &PathKeyedMapping,
    repeat: &PathKeyedMapping,
) -> Vec<String> {
    let mut keys: Vec<&(Vec<String>, Vec<String>)> = baseline.keys().chain(repeat.keys()).collect();
    keys.sort_unstable();
    keys.dedup();

    keys.into_iter()
        .filter_map(|key| {
            let base_entry = baseline.get(key);
            let repeat_entry = repeat.get(key);
            if base_entry == repeat_entry {
                return None;
            }
            let describe = |entry: Option<&ASTMappingOperation>| match entry {
                Some(op) => format!("{op:?}"),
                None => "unmapped".to_string(),
            };
            Some(format!(
                "Non-deterministic diff across independent parses: run 1 and run {run_number} disagree on {:?} <-> {:?}: {} vs {}",
                key.0,
                key.1,
                describe(base_entry),
                describe(repeat_entry),
            ))
        })
        .collect()
}

/// Compares three independently-parsed `diff_code` runs of the same before/after source and
/// describes every point of disagreement (empty if all three fully agree).
#[cfg(test)]
fn describe_nondeterminism(
    before_source: &str,
    after_source: &str,
    language: &crate::code::Language,
) -> Vec<String> {
    describe_nondeterminism_with_config(
        before_source,
        after_source,
        language,
        &crate::diff::HeuristicConfig::default(),
    )
}

/// [`describe_nondeterminism`] with `config`.
fn describe_nondeterminism_with_config(
    before_source: &str,
    after_source: &str,
    language: &crate::code::Language,
    config: &crate::diff::HeuristicConfig,
) -> Vec<String> {
    let baseline = diff_paths_with_config(before_source, after_source, language, config);
    let mut mismatches = Vec::new();
    for run_number in 2..=3 {
        let repeat = diff_paths_with_config(before_source, after_source, language, config);
        mismatches.extend(describe_path_map_differences(
            run_number, &baseline, &repeat,
        ));
    }
    mismatches
}

/// Every disagreement between `name`'s human mapping and codediff's diff (empty if they agree).
///
/// For [`crate::test::helper::UNIT_TEST_FIXTURES`], also diffs two more fresh parses and compares
/// all three runs by path: `diff_code` must be a pure function of its source, and a difference
/// means some pass depends on hash iteration order or node ids. Sampled because it quadruples the
/// cost, and nondeterminism belongs to a code path, which the per-language sample exercises.
pub fn compute_mismatches(name: &str) -> Result<Vec<String>> {
    compute_mismatches_with_config(name, &crate::diff::HeuristicConfig::default())
}

/// [`compute_mismatches`] with `config` (`benchmark_optimal_solutions --no-solver-X`).
pub fn compute_mismatches_with_config(
    name: &str,
    config: &crate::diff::HeuristicConfig,
) -> Result<Vec<String>> {
    let (before, after) = &*crate::test::helper::handmade_test_code_pair(name)?;
    compute_mismatches_for_with_config(name, before, after, config)
}

/// [`compute_mismatches_with_config`] reporting only visible mismatches.
pub fn compute_visible_mismatches_with_config(
    name: &str,
    config: &crate::diff::HeuristicConfig,
) -> Result<VisibleMismatches> {
    let (before, after) = &*crate::test::helper::handmade_test_code_pair(name)?;
    compute_visible_mismatches_for_with_config(name, before, after, config)
}

/// Total AST nodes across `before` and `after` - the denominator of a mismatch percentage.
pub fn total_node_count_for(before: &crate::code::Code, after: &crate::code::Code) -> usize {
    let node_cache = NodeCache::build(before, after);
    node_cache.before.len() + node_cache.after.len()
}

/// How many node slots the human mapping actually *grades*, in the unit of
/// [`total_node_count_for`].
///
/// The two differ because grading is asymmetric: `*WithChildren` entries are checked over their
/// whole subtree, pair entries only for the pair named. One `identical` entry over a large
/// function grades one pair and puts the whole function in the denominator, so a low mismatch rate
/// can mean "barely graded" rather than "nearly perfect".
///
/// Counted per entry as grading counts it: pairs 2 slots, `Delete`/`Insert` 1, `*WithChildren`
/// their subtree; groups via representative entries. A coverage measure, not a validator: an
/// unresolvable path contributes what it can.
pub fn graded_node_count(
    mapping: &HumanMapping,
    before_root: Node,
    after_root: Node,
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
) -> Result<usize> {
    let mut before_cache = PathCache::new();
    let mut after_cache = PathCache::new();
    let entries = representative_entries(mapping, before_root, after_root)?;

    let mut graded = 0usize;
    for entry in &entries {
        graded += match entry.operation {
            HumanOperation::Identical
            | HumanOperation::Update
            | HumanOperation::MatchButNotIdentical => 2,
            HumanOperation::Delete | HumanOperation::Insert => 1,
            // Inlined: `PathCache` is invariant over its tree lifetime, so a shared closure would
            // need the lifetime threaded by hand.
            HumanOperation::DeleteWithChildren => match entry.before_path.as_ref() {
                None => 1,
                Some(path) => match before_cache.resolve(before_root, &path_refs(path)) {
                    Ok(node) => before_metadata
                        .node_to_subtree_size
                        .get(&node.id())
                        .copied()
                        .unwrap_or(1),
                    Err(_) => 1,
                },
            },
            HumanOperation::InsertWithChildren => match entry.after_path.as_ref() {
                None => 1,
                Some(path) => match after_cache.resolve(after_root, &path_refs(path)) {
                    Ok(node) => after_metadata
                        .node_to_subtree_size
                        .get(&node.id())
                        .copied()
                        .unwrap_or(1),
                    Err(_) => 1,
                },
            },
        };
    }
    Ok(graded)
}

/// [`graded_node_count`] for fixture `name`.
pub fn graded_node_count_for(
    name: &str,
    before: &crate::code::Code,
    after: &crate::code::Code,
) -> Result<usize> {
    let mapping = load(name)?;
    let before_ast = before.ast.as_ref().context("Before code has no AST")?;
    let after_ast = after.ast.as_ref().context("After code has no AST")?;
    let before_metadata = crate::code::metadata::metadata_of(before);
    let after_metadata = crate::code::metadata::metadata_of(after);
    graded_node_count(
        &mapping,
        before_ast.root_node(),
        after_ast.root_node(),
        &before_metadata,
        &after_metadata,
    )
}

/// Reduces one side's `TextOperation`s to "touched or not", the only signal a line-only tool
/// also has.
fn touched(ops: &[crate::diff::text::TextOperation]) -> Vec<bool> {
    ops.iter()
        .map(|op| *op != crate::diff::text::TextOperation::Identical)
        .collect()
}

/// Per-line touched masks for both sides of `ast_diff`, via `TextDiff`/`line_operations`, so any
/// two diffs of one pair reduce to line labels identically (see [`line_disagreement_count`]).
pub fn touched_lines(
    before: &crate::code::Code,
    after: &crate::code::Code,
    ast_diff: &ASTDiff,
    node_cache: &NodeCache,
) -> (Vec<bool>, Vec<bool>) {
    let text_diff = crate::diff::text::TextDiff::from(before, after, ast_diff, node_cache);
    let before_ops =
        crate::diff::text::line_operations(&text_diff.all(0), before.contents.split('\n').count());
    let after_ops =
        crate::diff::text::line_operations(&text_diff.all(1), after.contents.split('\n').count());
    (touched(&before_ops), touched(&after_ops))
}

/// Number of positions where `a` and `b` disagree. Panics on a length mismatch.
pub fn line_disagreement_count(a: &[bool], b: &[bool]) -> usize {
    assert_eq!(
        a.len(),
        b.len(),
        "line count mismatch between two labelings of the same file"
    );
    a.iter().zip(b).filter(|(x, y)| x != y).count()
}

/// One AST node's extent, in `TextRange`'s row/byte-column space, for the node-granularity
/// counterpart of [`touched_lines`].
pub struct NodeExtent {
    pub range: crate::diff::text_range::TextRange,
    /// Whether this node has no children. Leaves are what AST-aware external tools report at and
    /// the only extents that do not nest, so they are scored separately.
    pub is_leaf: bool,
    /// This node's tree-sitter id, for cross-referencing
    /// [`crate::diff::nodes::structurally_visible_node_ids`]. Carried here because that walk's order
    /// differs from [`node_extents`]', so zipping by position would misattribute visibility.
    pub node_id: usize,
}

/// Every node of `code`'s AST, in deterministic preorder (so two labelings are index-comparable).
/// The same node set as [`crate::diff::NodeCache`], so both sides' lengths sum to
/// [`total_node_count_for`]. Empty without an AST.
pub fn node_extents(code: &crate::code::Code) -> Vec<NodeExtent> {
    let Some(ast) = code.ast.as_ref() else {
        return Vec::new();
    };
    let columns = crate::code::metadata::compute_row_byte_lengths(&code.contents);
    let mut extents = Vec::new();
    let mut stack = vec![ast.root_node()];
    while let Some(node) = stack.pop() {
        extents.push(NodeExtent {
            range: crate::diff::text_range::TextRange::from_treesitter_range(
                node.range(),
                &columns,
            ),
            is_leaf: node.child_count() == 0,
            node_id: node.id(),
        });
        let mut cursor = node.walk();
        for child in node
            .children(&mut cursor)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            stack.push(child);
        }
    }
    extents
}

/// One bool per entry of `extents`: whether any range in `spans` overlaps that node's extent.
///
/// A "touched or not" projection, **not** the mapping-fidelity metric: external tools report
/// changed regions over their own trees, so "which node did this become" cannot be asked of them.
/// Whole-extent overlap makes every ancestor of a change touched, which is why a leaves-only
/// count is reported beside it.
pub fn nodes_touched_by(
    extents: &[NodeExtent],
    spans: &[crate::diff::text_range::TextRange],
) -> Vec<bool> {
    extents
        .iter()
        .map(|extent| spans.iter().any(|span| span.intersects(&extent.range)))
        .collect()
}

/// The changed (non-`Identical`) ranges on each side of `ast_diff`, as `(before, after)`, for
/// [`nodes_touched_by`]. Used for the ground truth and codediff alike, so any asymmetry is in the
/// diff, not the measurement.
pub fn changed_spans(
    before: &crate::code::Code,
    after: &crate::code::Code,
    ast_diff: &ASTDiff,
    node_cache: &NodeCache,
) -> (
    Vec<crate::diff::text_range::TextRange>,
    Vec<crate::diff::text_range::TextRange>,
) {
    let text_diff = crate::diff::text::TextDiff::from(before, after, ast_diff, node_cache);
    let changed = |ranges: Vec<crate::diff::text::RangeMatch>| {
        ranges
            .into_iter()
            .filter(|rm| {
                !matches!(
                    rm.operation,
                    crate::diff::text::TextOperation::Identical
                        | crate::diff::text::TextOperation::NotYetSet
                ) && !rm.source.is_empty()
            })
            .map(|rm| rm.source)
            .collect()
    };
    (changed(text_diff.all(0)), changed(text_diff.all(1)))
}

/// Per-line touched masks from the real GNU `diff` (the tool people actually run), via
/// `--*-line-format` with `%dn` rather than parsing hunk headers. Writes `before`/`after` to temp
/// files, so any `Code` pair works.
pub fn unix_diff_line_labels(
    before: &crate::code::Code,
    after: &crate::code::Code,
) -> Result<(Vec<bool>, Vec<bool>)> {
    let mut before_file = tempfile::NamedTempFile::new().context("creating before temp file")?;
    let mut after_file = tempfile::NamedTempFile::new().context("creating after temp file")?;
    std::io::Write::write_all(&mut before_file, before.contents.as_bytes())
        .context("writing before temp file")?;
    std::io::Write::write_all(&mut after_file, after.contents.as_bytes())
        .context("writing after temp file")?;

    let before_line_count = before.contents.split('\n').count();
    let after_line_count = after.contents.split('\n').count();

    let before_touched = touched_line_numbers(
        &[
            "--old-line-format=%dn\n",
            "--new-line-format=",
            "--unchanged-line-format=",
        ],
        before_file.path(),
        after_file.path(),
        before_line_count,
    )?;
    let after_touched = touched_line_numbers(
        &[
            "--old-line-format=",
            "--new-line-format=%dn\n",
            "--unchanged-line-format=",
        ],
        before_file.path(),
        after_file.path(),
        after_line_count,
    )?;

    Ok((before_touched, after_touched))
}

/// Runs `diff` with the given `--*-line-format` flags and turns its 1-indexed line numbers into a
/// 0-indexed `line_count`-long touched mask.
fn touched_line_numbers(
    format_flags: &[&str],
    before_path: &std::path::Path,
    after_path: &std::path::Path,
    line_count: usize,
) -> Result<Vec<bool>> {
    let output = std::process::Command::new("diff")
        .args(format_flags)
        .arg(before_path)
        .arg(after_path)
        .output()
        .context("running `diff` - is diffutils installed?")?;
    // Exit 1 means "differences found"; 2+ is a real error.
    if output.status.code().is_none_or(|c| c > 1) {
        bail!(
            "diff exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let mut touched = vec![false; line_count];
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let line_number: usize = line
            .trim()
            .parse()
            .with_context(|| format!("parsing diff output line {:?}", line))?;
        if let Some(slot) = line_number
            .checked_sub(1)
            .and_then(|idx| touched.get_mut(idx))
        {
            *slot = true;
        }
    }
    Ok(touched)
}

/// Line-level mismatch counts for one fixture against the human mapping's per-line projection.
/// Unlike node mismatches, meaningful for a line-only tool like Unix `diff` too.
pub struct LineMismatches {
    pub codediff: usize,
    pub unix_diff: usize,
    /// `before`'s line count plus `after`'s - the denominator of both counts.
    pub total_lines: usize,
}

/// The human mapping's per-line projection (see [`touched_lines`]), plus the [`NodeCache`] built
/// for it, which the caller reuses to project a second diff onto the same pair.
pub fn human_touched_lines_for_mapping<'code>(
    mapping: &HumanMapping,
    before: &'code crate::code::Code,
    after: &'code crate::code::Code,
) -> Result<(Vec<bool>, Vec<bool>, NodeCache<'code>)> {
    let human_diff = as_ast_diff_for_mapping(mapping, before, after)?;
    let node_cache = NodeCache::build(before, after);
    let (human_before, human_after) = touched_lines(before, after, &human_diff, &node_cache);
    Ok((human_before, human_after, node_cache))
}

/// [`human_touched_lines_for_mapping`] loading `name`'s mapping itself.
pub fn human_touched_lines_for<'code>(
    name: &str,
    before: &'code crate::code::Code,
    after: &'code crate::code::Code,
) -> Result<(Vec<bool>, Vec<bool>, NodeCache<'code>)> {
    let mapping = load(name)?;
    human_touched_lines_for_mapping(&mapping, before, after)
}

/// [`LineMismatches`] for one fixture: codediff and Unix `diff` against the human mapping. Only
/// Unix `diff`, since other external tools need binaries a caller cannot assume.
pub fn line_mismatches_for(
    name: &str,
    before: &crate::code::Code,
    after: &crate::code::Code,
) -> Result<LineMismatches> {
    let mapping = load(name)?;
    line_mismatches_for_mapping(&mapping, before, after)
}

/// [`line_mismatches_for`] for an already-loaded [`HumanMapping`].
pub fn line_mismatches_for_mapping(
    mapping: &HumanMapping,
    before: &crate::code::Code,
    after: &crate::code::Code,
) -> Result<LineMismatches> {
    let (human_before, human_after, node_cache) =
        human_touched_lines_for_mapping(mapping, before, after)?;
    let total_lines = human_before.len() + human_after.len();

    let codediff_diff = crate::diff::diff_code(before, after);
    let codediff_ast = codediff_diff
        .ast
        .context("codediff produced no AST mapping")?;
    let (codediff_before, codediff_after) =
        touched_lines(before, after, &codediff_ast, &node_cache);
    let codediff = line_disagreement_count(&human_before, &codediff_before)
        + line_disagreement_count(&human_after, &codediff_after);

    let (unix_before, unix_after) = unix_diff_line_labels(before, after)?;
    let unix_diff = line_disagreement_count(&human_before, &unix_before)
        + line_disagreement_count(&human_after, &unix_after);

    Ok(LineMismatches {
        codediff,
        unix_diff,
        total_lines,
    })
}

/// [`compute_mismatches`] for an already-loaded before/after pair.
pub fn compute_mismatches_for(
    name: &str,
    before: &crate::code::Code,
    after: &crate::code::Code,
) -> Result<Vec<String>> {
    compute_mismatches_for_with_config(
        name,
        before,
        after,
        &crate::diff::HeuristicConfig::default(),
    )
}

/// [`compute_mismatches_for`] with `config` (`benchmark_optimal_solutions --no-solver-X`).
pub fn compute_mismatches_for_with_config(
    name: &str,
    before: &crate::code::Code,
    after: &crate::code::Code,
    config: &crate::diff::HeuristicConfig,
) -> Result<Vec<String>> {
    Ok(
        compute_mismatches_detailed_for_with_config(name, before, after, config)?
            .into_iter()
            .map(|m| m.message)
            .collect(),
    )
}

/// [`compute_mismatches_for_with_config`] keeping each mismatch's [`Mismatch::node_id`] and
/// [`Mismatch::side`], so a caller can separate visible mismatches from scaffolding.
pub fn compute_mismatches_detailed_for_with_config(
    name: &str,
    before: &crate::code::Code,
    after: &crate::code::Code,
    config: &crate::diff::HeuristicConfig,
) -> Result<Vec<Mismatch>> {
    let diff = crate::diff::diff_code_with_config(before, after, config);
    let diff_ast = diff.ast.context("Diff has no AST")?;
    let node_cache = NodeCache::build(before, after);
    compute_mismatches_detailed_with_diff(name, before, after, &diff_ast, &node_cache, config)
}

/// [`compute_mismatches_detailed_for_with_config`]'s body over an already computed diff, so
/// [`compute_visible_mismatches_for_with_config`] diffs once.
fn compute_mismatches_detailed_with_diff(
    name: &str,
    before: &crate::code::Code,
    after: &crate::code::Code,
    diff_ast: &ASTDiff,
    node_cache: &NodeCache,
    config: &crate::diff::HeuristicConfig,
) -> Result<Vec<Mismatch>> {
    let mapping = load(name)?;
    let language = before.metadata.language.unwrap_or_default();
    // Determinism check sampled - see `compute_mismatches`. Neither check is about one node, so
    // both use the `(0, Side::Before)` sentinel.
    let mut mismatches: Vec<Mismatch> = if crate::test::helper::UNIT_TEST_FIXTURES.contains(&name) {
        describe_nondeterminism_with_config(&before.contents, &after.contents, &language, config)
            .into_iter()
            .map(|message| Mismatch {
                message,
                node_id: 0,
                side: Side::Before,
            })
            .collect()
    } else {
        Vec::new()
    };

    if !diff_ast.is_valid(before, node_cache) {
        mismatches.push(Mismatch {
            message: "The produced diff is not valid according to ASTDiff::is_valid".to_string(),
            node_id: 0,
            side: Side::Before,
        });
    }

    let before_ast = before.ast.as_ref().context("Before code has no AST")?;
    let after_ast = after.ast.as_ref().context("After code has no AST")?;
    let before_root = before_ast.root_node();
    let after_root = after_ast.root_node();

    let mut before_cache = PathCache::new();
    let mut after_cache = PathCache::new();
    for entry in &mapping.entries {
        check_entry(
            entry,
            before_root,
            after_root,
            diff_ast,
            &mut mismatches,
            &mut before_cache,
            &mut after_cache,
        )?;
    }
    for group in &mapping.groups {
        check_group_entry(
            group,
            before_root,
            after_root,
            diff_ast,
            &mut mismatches,
            &mut before_cache,
            &mut after_cache,
        )?;
    }

    Ok(mismatches)
}

/// [`compute_mismatches_detailed_for_with_config`]'s mismatches split by whether each one's node
/// is *visible* (carries text of its own, per [`crate::diff::nodes::is_structurally_visible`]),
/// plus each side's visible node count as the denominator. A `node_id: 0` mismatch is never
/// visible.
pub struct VisibleMismatches {
    pub visible: Vec<Mismatch>,
    pub invisible: Vec<Mismatch>,
    pub before_visible_node_count: usize,
    pub after_visible_node_count: usize,
}

/// Runs one diff and shares it between the mismatch check and
/// [`crate::diff::nodes::structurally_visible_node_ids`]: this is the body of every fixture test,
/// so a second diff would double the suite's cost.
pub fn compute_visible_mismatches_for_with_config(
    name: &str,
    before: &crate::code::Code,
    after: &crate::code::Code,
    config: &crate::diff::HeuristicConfig,
) -> Result<VisibleMismatches> {
    let diff = crate::diff::diff_code_with_config(before, after, config);
    let diff_ast = diff.ast.context("Diff has no AST")?;
    let node_cache = NodeCache::build(before, after);

    let mismatches =
        compute_mismatches_detailed_with_diff(name, before, after, &diff_ast, &node_cache, config)?;
    let before_visible = crate::diff::nodes::structurally_visible_node_ids(before);
    let after_visible = crate::diff::nodes::structurally_visible_node_ids(after);

    let mut visible = Vec::new();
    let mut invisible = Vec::new();
    for mismatch in mismatches {
        let is_visible = match mismatch.side {
            Side::Before => before_visible.contains(&mismatch.node_id),
            Side::After => after_visible.contains(&mismatch.node_id),
        };
        if is_visible {
            visible.push(mismatch);
        } else {
            invisible.push(mismatch);
        }
    }

    Ok(VisibleMismatches {
        visible,
        invisible,
        before_visible_node_count: before_visible.len(),
        after_visible_node_count: after_visible.len(),
    })
}

/// Checks that every decision in `name`'s human mapping holds in codediff's diff, reporting every
/// mismatch at once.
pub fn assert_matches_human_mapping(name: &str) -> Result<()> {
    assert_matches_human_mapping_within_limit(name, 0, 0)
}

/// [`assert_matches_human_mapping`] allowing up to `upper_limit_of_mismatched_nodes` total and
/// `upper_limit_of_visible_mismatched_nodes` *visible* mismatches (see [`VisibleMismatches`]).
/// Either limit failing fails: they are independent, since a change can turn invisible mismatches
/// visible without moving the total.
///
/// A clamp records today's count for a known gap, so the test still catches regressions. Lower it
/// when a fix lands, and switch to [`assert_matches_human_mapping`] at zero.
pub fn assert_matches_human_mapping_within_limit(
    name: &str,
    upper_limit_of_mismatched_nodes: usize,
    upper_limit_of_visible_mismatched_nodes: usize,
) -> Result<()> {
    let visible =
        compute_visible_mismatches_with_config(name, &crate::diff::HeuristicConfig::default())?;
    let total = visible.visible.len() + visible.invisible.len();
    let visible_count = visible.visible.len();

    let total_exceeded = total > upper_limit_of_mismatched_nodes;
    let visible_exceeded = visible_count > upper_limit_of_visible_mismatched_nodes;

    if total_exceeded || visible_exceeded {
        let messages: Vec<&str> = visible
            .visible
            .iter()
            .map(|m| m.message.as_str())
            .chain(visible.invisible.iter().map(|m| m.message.as_str()))
            .collect();
        bail!(
            "{} mismatch(es) ({} visible) between the human mapping and codediff's diff for '{}' \
             (allowed up to {} total, {} visible){}{}:\n{}",
            total,
            visible_count,
            name,
            upper_limit_of_mismatched_nodes,
            upper_limit_of_visible_mismatched_nodes,
            if total_exceeded {
                " [TOTAL LIMIT EXCEEDED]"
            } else {
                ""
            },
            if visible_exceeded {
                " [VISIBLE LIMIT EXCEEDED]"
            } else {
                ""
            },
            messages.join("\n")
        );
    }

    Ok(())
}

/// The mapping limit each fixture stub records, read from the stub sources - the single source
/// of truth, which `benchmark_optimal_solutions --write-baseline` projects into
/// `quality_baseline.csv` so a limit only moves by editing its stub.
///
/// Parses the two call shapes `human_solver`'s `ensure_stub_test` writes:
/// `assert_matches_human_mapping("name")` -> `(0, 0)`, and
/// `assert_matches_human_mapping_within_limit("name", total, visible)`. A stub matching neither
/// (the hand-written `rust_hash_optimization.rs`) is skipped, not guessed at.
pub fn stub_mapping_limits() -> Result<HashMap<String, (usize, usize)>> {
    // Whitespace-tolerant: rustfmt breaks long calls across lines.
    let exact = regex::Regex::new(r#"assert_matches_human_mapping\(\s*"([^"]+)"\s*,?\s*\)"#)
        .expect("valid regex");
    let clamped = regex::Regex::new(
        r#"assert_matches_human_mapping_within_limit\(\s*"([^"]+)"\s*,\s*(\d+)\s*,\s*(\d+)\s*,?\s*\)"#,
    )
    .expect("valid regex");

    let mut out = HashMap::new();
    for dataset in crate::test::helper::DIFF_DATASETS {
        let dir = optimal_solutions_dir(dataset);
        if !dir.exists() {
            continue;
        }
        for entry in std::fs::read_dir(&dir).with_context(|| format!("reading {dir:?}"))? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path)
                .with_context(|| format!("reading the stub {path:?}"))?;
            if let Some(caps) = clamped.captures(&source) {
                out.insert(
                    caps[1].to_string(),
                    (caps[2].parse().unwrap_or(0), caps[3].parse().unwrap_or(0)),
                );
            } else if let Some(caps) = exact.captures(&source) {
                out.insert(caps[1].to_string(), (0, 0));
            }
        }
    }
    Ok(out)
}

/// `src/test/fixtures/<dataset>/`, the directory `stub_mapping_limits` reads.
fn optimal_solutions_dir(dataset: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("fixtures")
        .join(dataset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::Language;
    use crate::test::helper::path_for_node;

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

    /// Columns are **byte** offsets: a char/byte mix-up is invisible on ASCII.
    #[test]
    fn span_text_uses_byte_columns_and_an_exclusive_end() {
        let contents = "let é = 1;\nsecond line\nthird\n";

        // "é" is two bytes, so byte 4..6 is the character itself, not "é" plus one.
        assert_eq!(span_text(contents, span(0, 4, 0, 6)), Some("é"));
        assert_eq!(span_text(contents, span(0, 0, 0, 3)), Some("let"));
        // A whole row, ending at column 0 of the next one, includes the newline.
        assert_eq!(span_text(contents, span(1, 0, 2, 0)), Some("second line\n"));
        // Multi-row.
        assert_eq!(span_text(contents, span(0, 9, 1, 6)), Some("1;\nsecond"));
        // Absent rather than clamped: a span that does not fit means the file changed.
        assert_eq!(span_text(contents, span(2, 0, 2, 99)), None);
        assert_eq!(span_text(contents, span(99, 0, 99, 1)), None);
        // Mid-character is refused rather than panicking on the slice.
        assert_eq!(span_text(contents, span(0, 5, 0, 6)), None);
    }

    #[test]
    fn a_match_derives_move_from_identical_text_and_update_from_differing_text() {
        let before = "alpha\nbeta\n";
        let after = "beta\nalpha\n";

        let moved = HumanTextEntry {
            operation: HumanTextOperation::Match,
            before: vec![span(0, 0, 0, 5)],
            after: vec![span(1, 0, 1, 5)],
        };
        assert_eq!(
            moved.verdict(before, after).unwrap(),
            HumanTextVerdict::Move,
            "identical text in two places is a relocation"
        );

        let edited = HumanTextEntry {
            operation: HumanTextOperation::Match,
            before: vec![span(0, 0, 0, 5)],
            after: vec![span(0, 0, 0, 4)],
        };
        assert_eq!(
            edited.verdict(before, after).unwrap(),
            HumanTextVerdict::Update,
            "corresponding text that differs is an edit"
        );
    }

    #[test]
    fn a_match_may_be_n_to_m_when_each_side_reads_the_same() {
        let before = "foo\nfoo\nfoo\n";
        let after = "bar\nbar\n";

        let group = HumanTextEntry {
            operation: HumanTextOperation::Match,
            before: vec![span(0, 0, 0, 3), span(1, 0, 1, 3), span(2, 0, 2, 3)],
            after: vec![span(0, 0, 0, 3), span(1, 0, 1, 3)],
        };

        assert_eq!(
            group.verdict(before, after).unwrap(),
            HumanTextVerdict::Update,
            "3 'foo' corresponding to 2 'bar' is one edit, not five"
        );
    }

    #[test]
    fn an_n_to_m_match_whose_sides_read_the_same_is_a_move() {
        let before = "foo\nfoo\n";
        let after = "x\nfoo\nfoo\n";

        let group = HumanTextEntry {
            operation: HumanTextOperation::Match,
            before: vec![span(0, 0, 0, 3), span(1, 0, 1, 3)],
            after: vec![span(1, 0, 1, 3), span(2, 0, 2, 3)],
        };

        assert_eq!(
            group.verdict(before, after).unwrap(),
            HumanTextVerdict::Move
        );
    }

    /// The invariant that makes an unspecified pairing sound: an error, not a silently picked
    /// first span.
    #[test]
    fn a_match_whose_spans_disagree_within_one_side_is_rejected() {
        let before = "foo\nqux\n";
        let after = "bar\n";

        let group = HumanTextEntry {
            operation: HumanTextOperation::Match,
            before: vec![span(0, 0, 0, 3), span(1, 0, 1, 3)],
            after: vec![span(0, 0, 0, 3)],
        };

        let err = group.verdict(before, after).unwrap_err().to_string();
        assert!(
            err.contains("identical text"),
            "the error must say why, got: {err}"
        );
        assert!(err.contains("foo") && err.contains("qux"), "got: {err}");
    }

    #[test]
    fn a_delete_may_cover_several_spans() {
        let before = "foo\nbar\n";
        let after = "\n";

        let entry = HumanTextEntry {
            operation: HumanTextOperation::Delete,
            before: vec![span(0, 0, 0, 3), span(1, 0, 1, 3)],
            after: vec![],
        };

        // Differing text: the Match-only identity invariant does not apply here.
        assert_eq!(
            entry.verdict(before, after).unwrap(),
            HumanTextVerdict::Delete
        );
    }

    #[test]
    fn a_malformed_entry_is_an_error_rather_than_a_default() {
        let before = "alpha\n";
        let after = "alpha\n";

        let no_after = HumanTextEntry {
            operation: HumanTextOperation::Match,
            before: vec![span(0, 0, 0, 5)],
            after: vec![],
        };
        assert!(no_after.verdict(before, after).is_err());

        let outside = HumanTextEntry {
            operation: HumanTextOperation::Delete,
            before: vec![span(9, 0, 9, 1)],
            after: vec![],
        };
        assert!(outside.verdict(before, after).is_err());
    }

    /// A file with no text mapping round-trips byte-for-byte.
    #[test]
    fn a_mapping_without_a_text_painting_serializes_without_the_key() {
        let mapping = HumanMapping {
            entries: vec![HumanMappingEntry {
                operation: HumanOperation::Identical,
                before_path: Some(vec!["source_file:1".to_string()]),
                after_path: Some(vec!["source_file:1".to_string()]),
            }],
            groups: vec![],
            text_mappings: vec![],
        };

        let json = serde_json::to_string_pretty(&mapping).unwrap();

        assert!(!json.contains("text_mapping"), "got: {json}");
        let round_tripped: HumanMapping = serde_json::from_str(&json).unwrap();
        assert!(round_tripped.text_mappings.is_empty());
    }

    fn named(names: &[&str]) -> HumanMapping {
        HumanMapping {
            entries: vec![],
            groups: vec![],
            text_mappings: names
                .iter()
                .map(|name| NamedTextMapping {
                    name: (*name).to_string(),
                    mapping: HumanTextMapping::default(),
                })
                .collect(),
        }
    }

    /// A preset may carry several *alternative* readings (`Minimal (left)`, `Minimal (right)`),
    /// e.g. deleting one of two identical substrings.
    #[test]
    fn a_preset_collects_every_alternative_named_for_it() {
        let mapping = named(&["Minimal (left)", "Minimal (right)", "Full"]);

        let minimal = paintings_for_mode(&mapping, crate::diff::text::RenderOptions::MINIMAL)
            .expect("both Minimal alternatives should be found");
        let names: Vec<&str> = minimal.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["Minimal (left)", "Minimal (right)"]);

        let full = paintings_for_mode(&mapping, crate::diff::text::RenderOptions::FULL)
            .expect("the unqualified Full should be found");
        assert_eq!(full.len(), 1);
        assert_eq!(full[0].name, "Full");
    }

    #[test]
    fn a_preset_name_must_be_the_whole_name_or_be_followed_by_a_qualifier() {
        assert!(designates_preset("Minimal", "Minimal"));
        assert!(designates_preset("Minimal (left)", "Minimal"));
        assert!(designates_preset("Minimal but only the strings", "Minimal"));
        // Not a qualifier: a different word that starts the same way.
        assert!(!designates_preset("Minimalist", "Minimal"));
        assert!(!designates_preset("Full", "Minimal"));
        assert!(!designates_preset("", "Minimal"));
    }

    #[test]
    fn a_single_painting_answers_for_either_preset_whatever_it_is_called() {
        let mapping = named(&["Only one solution"]);
        for options in [
            crate::diff::text::RenderOptions::MINIMAL,
            crate::diff::text::RenderOptions::FULL,
        ] {
            let found = paintings_for_mode(&mapping, options).expect("the single painting");
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].name, "Only one solution");
        }
    }

    #[test]
    fn a_preset_with_no_painting_named_for_it_says_which_names_the_fixture_has() {
        let mapping = named(&["Minimal", "Something else"]);
        let error = paintings_for_mode(&mapping, crate::diff::text::RenderOptions::FULL)
            .expect_err("there is no Full painting here");
        let message = format!("{error:#}");
        assert!(message.contains("no 'Full' painting"), "got: {message}");
        assert!(message.contains("Something else"), "got: {message}");
    }

    /// "Nobody has painted this" is not "painted, and nothing to paint".
    #[test]
    fn a_named_but_empty_painting_is_distinguishable_from_no_painting_at_all() {
        let mapping = HumanMapping {
            entries: vec![],
            groups: vec![],
            text_mappings: vec![NamedTextMapping {
                name: "Minimal".to_string(),
                mapping: HumanTextMapping::default(),
            }],
        };

        let json = serde_json::to_string_pretty(&mapping).unwrap();
        let round_tripped: HumanMapping = serde_json::from_str(&json).unwrap();

        assert!(json.contains("text_mappings"), "got: {json}");
        assert_eq!(round_tripped.text_mappings.len(), 1);
        assert_eq!(round_tripped.text_mappings[0].name, "Minimal");
        assert!(round_tripped.text_mappings[0].mapping.entries.is_empty());
    }

    #[test]
    fn text_mapping_disagreements_reports_nothing_when_the_two_accounts_agree() {
        // `1` becomes `2`: the tree mapping and the painting both call the integer an Update.
        let before = crate::code::Code::from_string("let x = 1;\n", &crate::code::Language::Rust);
        let after = crate::code::Code::from_string("let x = 2;\n", &crate::code::Language::Rust);
        let mut mapping = build_full_identical_mapping(&before, &after);
        mapping.text_mappings = vec![NamedTextMapping {
            name: "Only one solution".to_string(),
            mapping: HumanTextMapping {
                entries: vec![HumanTextEntry {
                    operation: HumanTextOperation::Match,
                    before: vec![span(0, 8, 0, 9)],
                    after: vec![span(0, 8, 0, 9)],
                }],
            },
        }];

        let check = text_mapping_disagreements(&mapping, &before, &after)
            .unwrap()
            .expect("a painting exists");

        assert_eq!(check.solution, "Only one solution");
        assert!(
            check.disagreements.is_empty(),
            "expected agreement, got {:#?}",
            check.disagreements
        );
    }

    #[test]
    fn text_mapping_disagreements_finds_text_only_one_account_calls_changed() {
        let before = crate::code::Code::from_string("let x = 1;\n", &crate::code::Language::Rust);
        let after = crate::code::Code::from_string("let x = 2;\n", &crate::code::Language::Rust);
        let mut mapping = build_full_identical_mapping(&before, &after);
        // Paint the wrong token: `x` instead of the literal that actually changed.
        mapping.text_mappings = vec![NamedTextMapping {
            name: "Minimal".to_string(),
            mapping: HumanTextMapping {
                entries: vec![HumanTextEntry {
                    operation: HumanTextOperation::Match,
                    before: vec![span(0, 4, 0, 5)],
                    after: vec![span(0, 4, 0, 5)],
                }],
            },
        }];

        let disagreements = text_mapping_disagreements(&mapping, &before, &after)
            .unwrap()
            .expect("a painting exists")
            .disagreements;

        assert!(
            !disagreements.is_empty(),
            "painting the wrong token should disagree with the tree mapping"
        );
        // Both directions: changed per the painter only, and per the tree only.
        assert!(
            disagreements
                .iter()
                .any(|d| d.painted.is_some() && d.from_tree.is_none()),
            "got {disagreements:#?}"
        );
        assert!(
            disagreements
                .iter()
                .any(|d| d.painted.is_none() && d.from_tree.is_some()),
            "got {disagreements:#?}"
        );
    }

    /// A tree mapping pairing each side's root, so the disagreement tests exercise the real
    /// `as_ast_diff_for_mapping` -> `TextDiff` projection.
    fn build_full_identical_mapping(
        before: &crate::code::Code,
        after: &crate::code::Code,
    ) -> HumanMapping {
        let before_root = before.ast.as_ref().unwrap().root_node();
        let after_root = after.ast.as_ref().unwrap().root_node();
        let mut entries = Vec::new();
        let mut before_stack = vec![before_root];
        let mut after_stack = vec![after_root];
        while let (Some(b), Some(a)) = (before_stack.pop(), after_stack.pop()) {
            let before_path = Some(path_for_node(b));
            let after_path = Some(path_for_node(a));
            let (Some(before_path), Some(after_path)) = (before_path, after_path) else {
                continue;
            };
            let b_text = b.utf8_text(before.contents.as_bytes()).unwrap_or("");
            let a_text = a.utf8_text(after.contents.as_bytes()).unwrap_or("");
            entries.push(HumanMappingEntry {
                operation: if b_text == a_text {
                    HumanOperation::Identical
                } else if b.child_count() == 0 {
                    HumanOperation::Update
                } else {
                    HumanOperation::MatchButNotIdentical
                },
                before_path: Some(before_path),
                after_path: Some(after_path),
            });
            let mut b_cursor = b.walk();
            let mut a_cursor = a.walk();
            for (bc, ac) in b.children(&mut b_cursor).zip(a.children(&mut a_cursor)) {
                before_stack.push(bc);
                after_stack.push(ac);
            }
        }
        HumanMapping {
            entries,
            groups: vec![],
            text_mappings: vec![],
        }
    }

    fn parse_rust(source: &str) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&crate::code::language::to_treesitter(&Language::Rust).unwrap())
            .unwrap();
        parser.parse(source, None).unwrap()
    }

    #[test]
    fn round_trips_through_json() -> Result<()> {
        let mapping = HumanMapping {
            entries: vec![
                HumanMappingEntry {
                    operation: HumanOperation::Identical,
                    before_path: Some(vec!["function_item:1".to_string()]),
                    after_path: Some(vec!["function_item:1".to_string()]),
                },
                HumanMappingEntry {
                    operation: HumanOperation::DeleteWithChildren,
                    before_path: Some(vec!["function_item:1".to_string(), "block:1".to_string()]),
                    after_path: None,
                },
            ],
            ..Default::default()
        };

        let json = serde_json::to_string_pretty(&mapping)?;
        let round_tripped: HumanMapping = serde_json::from_str(&json)?;

        assert_eq!(round_tripped.entries.len(), 2);
        assert_eq!(
            round_tripped.entries[0].operation,
            HumanOperation::Identical
        );
        assert!(round_tripped.entries[0].after_path.is_some());
        assert_eq!(
            round_tripped.entries[1].operation,
            HumanOperation::DeleteWithChildren
        );
        assert!(round_tripped.entries[1].after_path.is_none());

        Ok(())
    }

    #[test]
    fn deserializes_legacy_json_with_no_groups_key_as_empty_groups() -> Result<()> {
        // A mapping file without `groups`, as most fixtures' are.
        let json = r#"{"entries":[]}"#;
        let mapping: HumanMapping = serde_json::from_str(json)?;
        assert!(mapping.groups.is_empty());
        Ok(())
    }

    #[test]
    fn serializing_a_mapping_with_no_groups_omits_the_groups_key() -> Result<()> {
        // An untouched fixture must not grow a `"groups": []` on re-save.
        let mapping = HumanMapping::default();
        let json = serde_json::to_string(&mapping)?;
        assert!(
            !json.contains("groups"),
            "expected no \"groups\" key when empty: {json}"
        );
        Ok(())
    }

    #[test]
    fn resaving_an_existing_fixture_produces_byte_identical_json() -> Result<()> {
        // A real fixture re-serialized the way `save()` does.
        let original = fs::read_to_string(mapping_path("rust-add-if"))?;
        let mapping: HumanMapping = serde_json::from_str(&original)?;
        assert!(
            mapping.groups.is_empty(),
            "fixture assumption broken: rust-add-if unexpectedly has groups already"
        );
        let resaved = serde_json::to_string_pretty(&mapping)?;
        // A Windows checkout with git's default autocrlf reads the fixture back with CRLF; the
        // serialization is what this test measures, not git's line-ending translation.
        let original = original.replace("\r\n", "\n");
        assert_eq!(resaved.trim_end(), original.trim_end());
        Ok(())
    }

    #[test]
    fn round_trips_a_multi_map_group_through_json() -> Result<()> {
        let mapping = HumanMapping {
            entries: vec![],
            groups: vec![MultiMapGroup {
                before_paths: vec![vec!["a:1".to_string()], vec!["a:2".to_string()]],
                after_paths: vec![vec!["b:1".to_string()]],
                operation: HumanOperation::Identical,
                with_children: true,
                pairing: GroupPairing::AnyOneToOne,
            }],
            text_mappings: vec![],
        };

        let json = serde_json::to_string_pretty(&mapping)?;
        let round_tripped: HumanMapping = serde_json::from_str(&json)?;

        assert_eq!(round_tripped.groups.len(), 1);
        assert_eq!(round_tripped.groups[0].before_paths.len(), 2);
        assert_eq!(round_tripped.groups[0].after_paths.len(), 1);
        assert_eq!(round_tripped.groups[0].operation, HumanOperation::Identical);
        assert!(round_tripped.groups[0].with_children);

        Ok(())
    }

    #[test]
    fn line_disagreement_count_counts_positions_where_the_two_slices_differ() {
        assert_eq!(
            line_disagreement_count(&[true, false, true], &[true, false, true]),
            0
        );
        assert_eq!(
            line_disagreement_count(&[true, false, true], &[false, false, true]),
            1
        );
        assert_eq!(
            line_disagreement_count(&[true, true, true], &[false, false, false]),
            3
        );
    }

    // Apple's diff has no `--*-line-format`, so the GNU-only runner cannot work there.
    #[cfg_attr(target_os = "macos", ignore = "needs GNU diff")]
    #[test]
    fn unix_diff_line_labels_marks_only_the_changed_line_on_each_side() {
        let before = crate::code::Code::from_string("a\nb\nc\n", &Language::Unknown);
        let after = crate::code::Code::from_string("a\nx\nc\n", &Language::Unknown);

        let (before_touched, after_touched) = unix_diff_line_labels(&before, &after).unwrap();

        assert_eq!(before_touched, vec![false, true, false, false]);
        assert_eq!(after_touched, vec![false, true, false, false]);
    }

    // Apple's diff has no `--*-line-format`, so the GNU-only runner cannot work there.
    #[cfg_attr(target_os = "macos", ignore = "needs GNU diff")]
    #[test]
    fn unix_diff_line_labels_marks_nothing_for_identical_files() {
        let before = crate::code::Code::from_string("a\nb\nc\n", &Language::Unknown);
        let after = crate::code::Code::from_string("a\nb\nc\n", &Language::Unknown);

        let (before_touched, after_touched) = unix_diff_line_labels(&before, &after).unwrap();

        assert!(before_touched.iter().all(|&t| !t));
        assert!(after_touched.iter().all(|&t| !t));
    }

    // Apple's diff has no `--*-line-format`, so the GNU-only runner cannot work there.
    #[cfg_attr(target_os = "macos", ignore = "needs GNU diff")]
    #[test]
    fn line_mismatches_for_is_zero_for_a_fixture_codediff_solves_exactly() -> Result<()> {
        // rust-no-change is fully identical, so both agree with the all-untouched human mapping.
        let (before, after) = &*crate::test::helper::handmade_test_code_pair("rust-no-change")?;

        let result = line_mismatches_for("rust-no-change", before, after)?;

        assert_eq!(result.codediff, 0);
        assert_eq!(result.unix_diff, 0);
        assert!(result.total_lines > 0);

        Ok(())
    }

    #[test]
    fn rebuild_caches_distinguishes_identical_from_update_and_match_but_not_identical() -> Result<()>
    {
        let source = "fn f() {\n    let a = 1;\n    let b = 2;\n    let c = 3;\n}\n";
        let before_tree = parse_rust(source);
        let after_tree = parse_rust(source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        let function_item = before_root.child(0).unwrap();
        let block = {
            let mut c = function_item.walk();
            function_item
                .children(&mut c)
                .find(|n| n.kind() == "block")
                .unwrap()
        };
        let mut stmt_cursor = block.walk();
        let statements: Vec<Node> = block
            .children(&mut stmt_cursor)
            .filter(|n| n.kind() == "let_declaration")
            .collect();
        let identical_stmt = statements[0];
        let update_stmt = statements[1];
        let match_but_not_identical_stmt = statements[2];

        let after_function_item = after_root.child(0).unwrap();
        let after_block = {
            let mut c = after_function_item.walk();
            after_function_item
                .children(&mut c)
                .find(|n| n.kind() == "block")
                .unwrap()
        };
        let mut after_stmt_cursor = after_block.walk();
        let after_statements: Vec<Node> = after_block
            .children(&mut after_stmt_cursor)
            .filter(|n| n.kind() == "let_declaration")
            .collect();

        let entries = vec![
            HumanMappingEntry {
                operation: HumanOperation::Identical,
                before_path: Some(path_for_node(identical_stmt)),
                after_path: Some(path_for_node(after_statements[0])),
            },
            HumanMappingEntry {
                operation: HumanOperation::Update,
                before_path: Some(path_for_node(update_stmt)),
                after_path: Some(path_for_node(after_statements[1])),
            },
            HumanMappingEntry {
                operation: HumanOperation::MatchButNotIdentical,
                before_path: Some(path_for_node(match_but_not_identical_stmt)),
                after_path: Some(path_for_node(after_statements[2])),
            },
        ];

        let caches = rebuild_caches(&entries, before_root, after_root);

        assert!(is_identical_before(identical_stmt, &caches));
        assert!(is_identical_after(after_statements[0], &caches));
        assert!(!is_identical_before(update_stmt, &caches));
        assert!(!is_identical_after(after_statements[1], &caches));
        assert!(!is_identical_before(match_but_not_identical_stmt, &caches));
        assert!(!is_identical_after(after_statements[2], &caches));
        // A node with no entry defaults to identical.
        assert!(is_identical_before(before_root, &caches));

        assert_eq!(
            match_operation_before(identical_stmt, &caches),
            Some(HumanOperation::Identical)
        );
        assert_eq!(
            match_operation_before(update_stmt, &caches),
            Some(HumanOperation::Update)
        );
        assert_eq!(
            match_operation_before(match_but_not_identical_stmt, &caches),
            Some(HumanOperation::MatchButNotIdentical)
        );
        assert_eq!(match_operation_before(before_root, &caches), None);

        // None of these three moved - before/after paths line up 1:1 since nothing reordered.
        assert!(!is_moved_before(identical_stmt, &caches));
        assert!(!is_moved_after(after_statements[0], &caches));
        assert!(!is_moved_before(update_stmt, &caches));
        assert!(!is_moved_before(match_but_not_identical_stmt, &caches));

        Ok(())
    }

    #[test]
    fn rebuild_caches_flags_an_identical_match_at_a_different_path_as_moved() -> Result<()> {
        let before_source = "fn f() {\n    a();\n    b();\n}\n";
        let after_source = "fn f() {\n    b();\n    a();\n}\n";
        let before_tree = parse_rust(before_source);
        let after_tree = parse_rust(after_source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        fn find_call<'a>(root: Node<'a>, text: &str, src: &str) -> Node<'a> {
            let function_item = root.child(0).unwrap();
            let mut c = function_item.walk();
            let block = function_item
                .children(&mut c)
                .find(|n| n.kind() == "block")
                .unwrap();
            let mut sc = block.walk();
            block
                .children(&mut sc)
                .find(|n| {
                    n.kind() == "expression_statement"
                        && n.utf8_text(src.as_bytes()).unwrap().starts_with(text)
                })
                .unwrap()
        }
        let before_a = find_call(before_root, "a", before_source);
        let after_a = find_call(after_root, "a", after_source);

        let entries = vec![HumanMappingEntry {
            operation: HumanOperation::Identical,
            before_path: Some(path_for_node(before_a)),
            after_path: Some(path_for_node(after_a)),
        }];
        let caches = rebuild_caches(&entries, before_root, after_root);

        assert_ne!(
            path_for_node(before_a),
            path_for_node(after_a),
            "fixture assumption broken: swapping a();/b(); should change a()'s occurrence path"
        );
        assert!(is_moved_before(before_a, &caches));
        assert!(is_moved_after(after_a, &caches));
        assert!(
            is_identical_before(before_a, &caches),
            "moved but content-identical is still identical, not changed"
        );

        Ok(())
    }

    #[test]
    fn detects_a_correct_hand_written_mapping_for_rust_no_change() -> Result<()> {
        // rust-no-change is fully identical before/after, so every node should match itself.
        let (before, after) = &*crate::test::helper::handmade_test_code_pair("rust-no-change")?;

        let before_ast = before.ast.as_ref().unwrap();
        let root = before_ast.root_node();

        // before == after, so the root is "source_file:1" on both sides.
        let entries = vec![HumanMappingEntry {
            operation: HumanOperation::Identical,
            before_path: Some(path_for_node(root)),
            after_path: Some(path_for_node(root)),
        }];

        let mapping = HumanMapping {
            entries,
            ..Default::default()
        };

        let diff = crate::diff::diff_code(before, after);
        let diff_ast = diff.ast.unwrap();
        let after_ast = after.ast.as_ref().unwrap();

        let mut mismatches = Vec::new();
        let mut before_cache = PathCache::new();
        let mut after_cache = PathCache::new();
        for entry in &mapping.entries {
            check_entry(
                entry,
                root,
                after_ast.root_node(),
                &diff_ast,
                &mut mismatches,
                &mut before_cache,
                &mut after_cache,
            )?;
        }

        assert!(mismatches.is_empty(), "{:?}", mismatches);

        Ok(())
    }

    #[test]
    fn detects_an_incorrect_hand_written_mapping() -> Result<()> {
        // Deliberately claim the root is deleted, which is false for rust-no-change.
        let (before, after) = &*crate::test::helper::handmade_test_code_pair("rust-no-change")?;

        let before_ast = before.ast.as_ref().unwrap();
        let root = before_ast.root_node();

        let entries = vec![HumanMappingEntry {
            operation: HumanOperation::DeleteWithChildren,
            before_path: Some(path_for_node(root)),
            after_path: None,
        }];

        let mapping = HumanMapping {
            entries,
            ..Default::default()
        };

        let diff = crate::diff::diff_code(before, after);
        let diff_ast = diff.ast.unwrap();
        let after_ast = after.ast.as_ref().unwrap();

        let mut mismatches = Vec::new();
        let mut before_cache = PathCache::new();
        let mut after_cache = PathCache::new();
        for entry in &mapping.entries {
            check_entry(
                entry,
                root,
                after_ast.root_node(),
                &diff_ast,
                &mut mismatches,
                &mut before_cache,
                &mut after_cache,
            )?;
        }

        assert!(!mismatches.is_empty());

        Ok(())
    }

    /// The `block` of the first function in `root`.
    fn function_block(root: Node) -> Node {
        let function_item = root.child(0).unwrap();
        let mut c = function_item.walk();
        function_item
            .children(&mut c)
            .find(|n| n.kind() == "block")
            .unwrap()
    }

    /// Every `expression_statement` directly inside `function_block(root)`.
    fn function_body_statements(root: Node) -> Vec<Node> {
        let block = function_block(root);
        let mut c = block.walk();
        block
            .children(&mut c)
            .filter(|n| n.kind() == "expression_statement")
            .collect()
    }

    /// First node of `kind` in preorder from `root` (inclusive). Panics if there is none.
    fn find_first<'a>(root: Node<'a>, kind: &str) -> Node<'a> {
        let mut stack = vec![root];
        while let Some(n) = stack.pop() {
            if n.kind() == kind {
                return n;
            }
            let mut c = n.walk();
            for child in n.children(&mut c) {
                stack.push(child);
            }
        }
        panic!("no node of kind {kind:?} found under {:?}", root.kind());
    }

    /// Maps `before`'s and `after`'s same-shaped subtrees `Identical` node by node, a fully
    /// self-consistent baseline for closure tests, which need every descendant mapped.
    fn map_identical_subtrees(diff: &mut ASTDiff, before: Node, after: Node) {
        let mut stack = vec![(before, after)];
        while let Some((b, a)) = stack.pop() {
            diff.add_mapping(
                b.id(),
                a.id(),
                ASTMapping::identical(ASTMappingReason::default()),
            );
            let mut bc = b.walk();
            let mut ac = a.walk();
            let b_children: Vec<Node> = b.children(&mut bc).collect();
            let a_children: Vec<Node> = a.children(&mut ac).collect();
            assert_eq!(
                b_children.len(),
                a_children.len(),
                "map_identical_subtrees requires before/after of identical shape"
            );
            stack.extend(b_children.into_iter().zip(a_children));
        }
    }

    /// Maps every node in `node`'s subtree (inclusive) to 0.
    fn map_before_subtree_deleted(diff: &mut ASTDiff, node: Node) {
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            diff.add_mapping(
                n.id(),
                0,
                ASTMapping {
                    cost: 0,
                    operation: ASTMappingOperation::Delete,
                    reason: ASTMappingReason::default(),
                },
            );
            let mut c = n.walk();
            for child in n.children(&mut c) {
                stack.push(child);
            }
        }
    }

    #[test]
    fn an_all_to_all_group_round_trips_and_a_default_pairing_leaves_the_file_unchanged()
    -> Result<()> {
        let group = |pairing| MultiMapGroup {
            before_paths: vec![vec!["expression_statement:1".to_string()]],
            after_paths: vec![
                vec!["expression_statement:1".to_string()],
                vec!["expression_statement:2".to_string()],
            ],
            operation: HumanOperation::Identical,
            with_children: true,
            pairing,
        };

        let plain = serde_json::to_string(&group(GroupPairing::AnyOneToOne))?;
        assert!(
            !plain.contains("pairing"),
            "the default pairing must not appear in the file, so every existing group re-saves unchanged: {plain}"
        );

        let all = serde_json::to_string(&group(GroupPairing::AllToAll))?;
        assert!(all.contains(r#""pairing":"all_to_all""#), "{all}");
        let round_tripped: MultiMapGroup = serde_json::from_str(&all)?;
        assert_eq!(round_tripped.pairing, GroupPairing::AllToAll);

        // A group written before the field existed.
        let legacy: MultiMapGroup = serde_json::from_str(
            r#"{"before_paths":[["a:1"]],"after_paths":[["a:1"]],"operation":"identical","with_children":false}"#,
        )?;
        assert_eq!(legacy.pairing, GroupPairing::AnyOneToOne);
        Ok(())
    }

    /// One `foo();` before, three after: the shape of a body duplicated, where an all-to-all
    /// group says every copy corresponds to the original and none is new.
    fn one_foo_becoming_three() -> (crate::code::Code, crate::code::Code) {
        (
            crate::code::Code::from_string("fn main() {\n    foo();\n}\n", &Language::Rust),
            crate::code::Code::from_string(
                "fn main() {\n    foo();\n    foo();\n    foo();\n}\n",
                &Language::Rust,
            ),
        )
    }

    fn all_to_all_over_statements(
        before_root: Node,
        after_root: Node,
        with_children: bool,
    ) -> MultiMapGroup {
        MultiMapGroup {
            before_paths: function_body_statements(before_root)
                .iter()
                .map(|n| path_for_node(*n))
                .collect(),
            after_paths: function_body_statements(after_root)
                .iter()
                .map(|n| path_for_node(*n))
                .collect(),
            operation: HumanOperation::Identical,
            with_children,
            pairing: GroupPairing::AllToAll,
        }
    }

    #[test]
    fn representative_entries_pairs_every_member_of_an_all_to_all_group() -> Result<()> {
        let (before, after) = one_foo_becoming_three();
        let before_root = before.ast.as_ref().unwrap().root_node();
        let after_root = after.ast.as_ref().unwrap().root_node();
        let mapping = HumanMapping {
            groups: vec![all_to_all_over_statements(before_root, after_root, false)],
            ..Default::default()
        };

        let entries = representative_entries(&mapping, before_root, after_root)?;
        assert_eq!(entries.len(), 3, "{entries:?}");
        assert!(
            entries
                .iter()
                .all(|e| e.operation == HumanOperation::Identical),
            "no member of an all-to-all group is inserted: {entries:?}"
        );
        let before_path = path_for_node(function_body_statements(before_root)[0]);
        assert!(
            entries
                .iter()
                .all(|e| e.before_path.as_ref() == Some(&before_path)),
            "every copy pairs with the one original: {entries:?}"
        );

        let caches = rebuild_caches_for_mapping(&mapping, before_root, after_root);
        for statement in function_body_statements(after_root) {
            assert_eq!(status_after(statement, &caches), NodeStatus::Matched);
        }
        Ok(())
    }

    #[test]
    fn check_group_entry_reports_each_all_to_all_member_a_one_to_one_diff_leaves_out() -> Result<()>
    {
        // codediff can only pair the one original with one copy; the other two come out inserted,
        // and that is exactly what the group must report - once per copy, nothing else.
        let (before, after) = one_foo_becoming_three();
        let before_root = before.ast.as_ref().unwrap().root_node();
        let after_root = after.ast.as_ref().unwrap().root_node();
        let group = all_to_all_over_statements(before_root, after_root, false);

        let diff = crate::diff::diff_code(&before, &after);
        let diff_ast = diff.ast.context("Diff has no AST")?;

        let mut mismatches = Vec::new();
        check_group_entry(
            &group,
            before_root,
            after_root,
            &diff_ast,
            &mut mismatches,
            &mut PathCache::new(),
            &mut PathCache::new(),
        )?;

        assert_eq!(mismatches.len(), 2, "{mismatches:?}");
        for mismatch in &mismatches {
            assert_eq!(mismatch.side, Side::After);
            assert!(
                mismatch
                    .message
                    .starts_with("all-to-all group (1 before <-> 3 after")
                    && mismatch
                        .message
                        .contains("expected to match within the group, but"),
                "{}",
                mismatch.message
            );
            assert!(
                !mismatch.message.contains("or be inserted"),
                "inserted is not a valid fate for an all-to-all member: {}",
                mismatch.message
            );
        }
        Ok(())
    }

    #[test]
    fn check_group_entry_accepts_a_one_to_one_diff_that_pairs_every_all_to_all_member() -> Result<()>
    {
        // Two before, two after: the one-to-one output can cover every member, so nothing is
        // reported - and, unlike an any-one-to-one group, there is no pair count to get wrong.
        let before = crate::code::Code::from_string(
            "fn main() {\n    foo();\n    foo();\n}\n",
            &Language::Rust,
        );
        let after = crate::code::Code::from_string(
            "fn main() {\n    foo();\n    foo();\n}\n",
            &Language::Rust,
        );
        let before_root = before.ast.as_ref().unwrap().root_node();
        let after_root = after.ast.as_ref().unwrap().root_node();
        let group = all_to_all_over_statements(before_root, after_root, true);

        let diff = crate::diff::diff_code(&before, &after);
        let diff_ast = diff.ast.context("Diff has no AST")?;

        let mut mismatches = Vec::new();
        check_group_entry(
            &group,
            before_root,
            after_root,
            &diff_ast,
            &mut mismatches,
            &mut PathCache::new(),
            &mut PathCache::new(),
        )?;
        assert!(mismatches.is_empty(), "{mismatches:?}");
        Ok(())
    }

    #[test]
    fn check_group_entry_closes_an_all_to_all_group_over_the_union_of_its_members() -> Result<()> {
        // The same hand-rolled diff, graded as each kind of group: each statement pairs with its
        // counterpart, but the two `;` tokens are swapped across the pair. Per-pair closure
        // rejects that; closure over the union accepts it, because an all-to-all group does not
        // say which member a descendant belongs with.
        let before = crate::code::Code::from_string(
            "fn main() {\n    foo();\n    bar();\n}\n",
            &Language::Rust,
        );
        let after = crate::code::Code::from_string(
            "fn main() {\n    foo();\n    bar();\n}\n",
            &Language::Rust,
        );
        let before_root = before.ast.as_ref().unwrap().root_node();
        let after_root = after.ast.as_ref().unwrap().root_node();
        let before_statements = function_body_statements(before_root);
        let after_statements = function_body_statements(after_root);
        fn semicolon<'t>(statement: Node<'t>) -> Node<'t> {
            let mut cursor = statement.walk();
            statement
                .children(&mut cursor)
                .find(|c| c.kind() == ";")
                .expect("an expression statement ends in ;")
        }

        let mut diff = ASTDiff::default();
        map_identical_subtrees(&mut diff, before_root, after_root);
        for (mine, theirs) in [(0, 1), (1, 0)] {
            diff.add_mapping(
                semicolon(before_statements[mine]).id(),
                semicolon(after_statements[theirs]).id(),
                ASTMapping::identical(ASTMappingReason::default()),
            );
        }

        let mut group = MultiMapGroup {
            before_paths: before_statements
                .iter()
                .map(|n| path_for_node(*n))
                .collect(),
            after_paths: after_statements.iter().map(|n| path_for_node(*n)).collect(),
            operation: HumanOperation::MatchButNotIdentical,
            with_children: true,
            pairing: GroupPairing::AllToAll,
        };

        let mismatches_for = |group: &MultiMapGroup, diff: &ASTDiff| -> Result<Vec<Mismatch>> {
            let mut mismatches = Vec::new();
            check_group_entry(
                group,
                before_root,
                after_root,
                diff,
                &mut mismatches,
                &mut PathCache::new(),
                &mut PathCache::new(),
            )?;
            Ok(mismatches)
        };

        assert!(
            mismatches_for(&group, &diff)?.is_empty(),
            "a descendant landing in the other member is inside the union"
        );
        group.pairing = GroupPairing::AnyOneToOne;
        assert!(
            !mismatches_for(&group, &diff)?.is_empty(),
            "the same diff leaks across a per-pair closure"
        );

        // And a descendant leaving the union is still caught.
        group.pairing = GroupPairing::AllToAll;
        diff.add_mapping(
            semicolon(before_statements[0]).id(),
            0,
            ASTMapping {
                cost: 0,
                operation: ASTMappingOperation::Delete,
                reason: ASTMappingReason::default(),
            },
        );
        let leaked = mismatches_for(&group, &diff)?;
        assert!(
            leaked
                .iter()
                .any(|m| m.message.contains("descendant node ';'")),
            "{leaked:?}"
        );
        Ok(())
    }

    #[test]
    fn check_group_entry_passes_for_a_real_diff_that_matches_duplicates_within_the_group()
    -> Result<()> {
        // Three identical foo() calls before, two after: whichever two the real algorithm
        // matches must be accepted.
        let before_source = "fn main() {\n    foo();\n    foo();\n    foo();\n    bar();\n}\n";
        let after_source = "fn main() {\n    bar();\n    foo();\n    foo();\n}\n";
        let before = crate::code::Code::from_string(before_source, &Language::Rust);
        let after = crate::code::Code::from_string(after_source, &Language::Rust);
        let before_root = before.ast.as_ref().unwrap().root_node();
        let after_root = after.ast.as_ref().unwrap().root_node();

        let before_foos: Vec<Node> = function_body_statements(before_root)
            .into_iter()
            .filter(|n| {
                n.utf8_text(before_source.as_bytes())
                    .unwrap()
                    .starts_with("foo")
            })
            .collect();
        let after_foos: Vec<Node> = function_body_statements(after_root)
            .into_iter()
            .filter(|n| {
                n.utf8_text(after_source.as_bytes())
                    .unwrap()
                    .starts_with("foo")
            })
            .collect();
        assert_eq!(before_foos.len(), 3);
        assert_eq!(after_foos.len(), 2);

        let group = MultiMapGroup {
            before_paths: before_foos.iter().map(|n| path_for_node(*n)).collect(),
            after_paths: after_foos.iter().map(|n| path_for_node(*n)).collect(),
            operation: HumanOperation::Identical,
            with_children: true,
            pairing: GroupPairing::AnyOneToOne,
        };

        let diff = crate::diff::diff_code(&before, &after);
        let diff_ast = diff.ast.context("Diff has no AST")?;

        let mut mismatches = Vec::new();
        let mut before_cache = PathCache::new();
        let mut after_cache = PathCache::new();
        check_group_entry(
            &group,
            before_root,
            after_root,
            &diff_ast,
            &mut mismatches,
            &mut before_cache,
            &mut after_cache,
        )?;

        assert!(mismatches.is_empty(), "{:?}", mismatches);
        Ok(())
    }

    #[test]
    fn check_group_entry_fails_when_codediff_deletes_and_inserts_instead_of_matching() -> Result<()>
    {
        // Every node's fate is locally valid, but with N == M == 2 the group under-matched.
        let source = "fn main() {\n    foo();\n    foo();\n}\n";
        let before_tree = parse_rust(source);
        let after_tree = parse_rust(source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        let before_foos = function_body_statements(before_root);
        let after_foos = function_body_statements(after_root);

        let group = MultiMapGroup {
            before_paths: before_foos.iter().map(|n| path_for_node(*n)).collect(),
            after_paths: after_foos.iter().map(|n| path_for_node(*n)).collect(),
            operation: HumanOperation::Identical,
            with_children: false,
            pairing: GroupPairing::AnyOneToOne,
        };

        let mut diff = ASTDiff::default();
        for &b in &before_foos {
            diff.add_mapping(
                b.id(),
                0,
                ASTMapping {
                    cost: 0,
                    operation: ASTMappingOperation::Delete,
                    reason: ASTMappingReason::default(),
                },
            );
        }
        for &a in &after_foos {
            diff.add_mapping(
                0,
                a.id(),
                ASTMapping {
                    cost: 0,
                    operation: ASTMappingOperation::Insert,
                    reason: ASTMappingReason::default(),
                },
            );
        }

        let mut mismatches = Vec::new();
        let mut before_cache = PathCache::new();
        let mut after_cache = PathCache::new();
        check_group_entry(
            &group,
            before_root,
            after_root,
            &diff,
            &mut mismatches,
            &mut before_cache,
            &mut after_cache,
        )?;

        assert_eq!(mismatches.len(), 1, "{:?}", mismatches);
        assert!(
            mismatches[0]
                .message
                .contains("expected exactly 2 pair(s) matched"),
            "{}",
            mismatches[0].message
        );
        Ok(())
    }

    #[test]
    fn check_group_entry_fails_when_a_member_matches_outside_the_group() -> Result<()> {
        let before_source = "fn main() {\n    foo();\n    foo();\n}\n";
        let after_source = "fn main() {\n    foo();\n    qux();\n}\n";
        let before_tree = parse_rust(before_source);
        let after_tree = parse_rust(after_source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        let before_foos = function_body_statements(before_root);
        let after_stmts = function_body_statements(after_root);
        let after_foo = after_stmts[0];
        let after_qux = after_stmts[1];

        // Group covers both before foo()s but only the real after foo() - qux() is deliberately
        // left out of the group entirely.
        let group = MultiMapGroup {
            before_paths: before_foos.iter().map(|n| path_for_node(*n)).collect(),
            after_paths: vec![path_for_node(after_foo)],
            operation: HumanOperation::Identical,
            with_children: false,
            pairing: GroupPairing::AnyOneToOne,
        };

        let mut diff = ASTDiff::default();
        diff.add_mapping(
            before_foos[0].id(),
            after_foo.id(),
            ASTMapping::identical(ASTMappingReason::default()),
        );
        // The second foo() is a group member too, but here it's mapped to qux() - a real node,
        // just not one that's part of this group - instead of being deleted.
        diff.add_mapping(
            before_foos[1].id(),
            after_qux.id(),
            ASTMapping::matched_not_identical(ASTMappingReason::default()),
        );

        let mut mismatches = Vec::new();
        let mut before_cache = PathCache::new();
        let mut after_cache = PathCache::new();
        check_group_entry(
            &group,
            before_root,
            after_root,
            &diff,
            &mut mismatches,
            &mut before_cache,
            &mut after_cache,
        )?;

        assert_eq!(mismatches.len(), 1, "{:?}", mismatches);
        assert!(
            mismatches[0]
                .message
                .contains("expected to match within the group or be deleted"),
            "{}",
            mismatches[0].message
        );
        Ok(())
    }

    #[test]
    fn check_group_entry_with_children_fails_when_a_descendant_leaks_outside_the_matched_subtree()
    -> Result<()> {
        let source = "fn main() {\n    foo();\n    bar();\n}\n";
        let before_tree = parse_rust(source);
        let after_tree = parse_rust(source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        let block_before = function_block(before_root);
        let block_after = function_block(after_root);

        let mut diff = ASTDiff::default();
        map_identical_subtrees(&mut diff, block_before, block_after);

        // Corrupt: bar(); is marked deleted while still inside the matched block's subtree.
        let bar_before = function_body_statements(before_root)[1];
        diff.add_mapping(
            bar_before.id(),
            0,
            ASTMapping {
                cost: 0,
                operation: ASTMappingOperation::Delete,
                reason: ASTMappingReason::default(),
            },
        );

        let group = MultiMapGroup {
            before_paths: vec![path_for_node(block_before)],
            after_paths: vec![path_for_node(block_after)],
            operation: HumanOperation::Identical,
            with_children: true,
            pairing: GroupPairing::AnyOneToOne,
        };

        let mut mismatches = Vec::new();
        let mut before_cache = PathCache::new();
        let mut after_cache = PathCache::new();
        check_group_entry(
            &group,
            before_root,
            after_root,
            &diff,
            &mut mismatches,
            &mut before_cache,
            &mut after_cache,
        )?;

        assert!(!mismatches.is_empty());
        assert!(
            mismatches
                .iter()
                .any(|m| m.message.contains("counterpart subtree")),
            "{:?}",
            mismatches
        );
        Ok(())
    }

    #[test]
    fn check_group_entry_with_children_fails_when_a_leftover_members_descendant_is_not_deleted()
    -> Result<()> {
        let before_source = "fn main() {\n    foo();\n    foo();\n}\n";
        let after_source = "fn main() {\n    foo();\n}\n";
        let before_tree = parse_rust(before_source);
        let after_tree = parse_rust(after_source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        let before_foos = function_body_statements(before_root);
        let g0 = function_body_statements(after_root)[0];
        let f0 = before_foos[0];
        let f1 = before_foos[1];

        let mut diff = ASTDiff::default();
        map_identical_subtrees(&mut diff, f0, g0);
        map_before_subtree_deleted(&mut diff, f1);

        // Corrupt: a leftover member under `with_children` must be *fully* deleted, not just its
        // top node.
        let f1_ident = find_first(f1, "identifier");
        let g0_ident = find_first(g0, "identifier");
        diff.add_mapping(
            f1_ident.id(),
            g0_ident.id(),
            ASTMapping::identical(ASTMappingReason::default()),
        );

        let group = MultiMapGroup {
            before_paths: vec![path_for_node(f0), path_for_node(f1)],
            after_paths: vec![path_for_node(g0)],
            operation: HumanOperation::Identical,
            with_children: true,
            pairing: GroupPairing::AnyOneToOne,
        };

        let mut mismatches = Vec::new();
        let mut before_cache = PathCache::new();
        let mut after_cache = PathCache::new();
        check_group_entry(
            &group,
            before_root,
            after_root,
            &diff,
            &mut mismatches,
            &mut before_cache,
            &mut after_cache,
        )?;

        assert!(!mismatches.is_empty());
        assert!(
            mismatches
                .iter()
                .any(|m| m.message.contains("expected to be removed")),
            "{:?}",
            mismatches
        );
        Ok(())
    }

    #[test]
    fn representative_entries_pairs_equal_sized_groups_by_start_byte() -> Result<()> {
        let source = "fn main() {\n    foo();\n    foo();\n}\n";
        let before_tree = parse_rust(source);
        let after_tree = parse_rust(source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        let before_foos = function_body_statements(before_root);
        let after_foos = function_body_statements(after_root);

        let mapping = HumanMapping {
            entries: vec![],
            groups: vec![MultiMapGroup {
                before_paths: before_foos.iter().map(|n| path_for_node(*n)).collect(),
                after_paths: after_foos.iter().map(|n| path_for_node(*n)).collect(),
                operation: HumanOperation::Identical,
                with_children: false,
                pairing: GroupPairing::AnyOneToOne,
            }],
            text_mappings: vec![],
        };

        let entries = representative_entries(&mapping, before_root, after_root)?;
        assert_eq!(entries.len(), 2);
        for entry in &entries {
            assert_eq!(entry.operation, HumanOperation::Identical);
            assert!(entry.before_path.is_some());
            assert!(entry.after_path.is_some());
        }
        Ok(())
    }

    #[test]
    fn representative_entries_puts_the_surplus_before_nodes_on_delete_with_children() -> Result<()>
    {
        let before_source = "fn main() {\n    foo();\n    foo();\n    foo();\n}\n";
        let after_source = "fn main() {\n    foo();\n    foo();\n}\n";
        let before_tree = parse_rust(before_source);
        let after_tree = parse_rust(after_source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        let before_foos = function_body_statements(before_root);
        let after_foos = function_body_statements(after_root);
        assert_eq!(before_foos.len(), 3);
        assert_eq!(after_foos.len(), 2);

        let mapping = HumanMapping {
            entries: vec![],
            groups: vec![MultiMapGroup {
                before_paths: before_foos.iter().map(|n| path_for_node(*n)).collect(),
                after_paths: after_foos.iter().map(|n| path_for_node(*n)).collect(),
                operation: HumanOperation::Identical,
                with_children: true,
                pairing: GroupPairing::AnyOneToOne,
            }],
            text_mappings: vec![],
        };

        let entries = representative_entries(&mapping, before_root, after_root)?;
        let matched: Vec<_> = entries
            .iter()
            .filter(|e| e.operation == HumanOperation::Identical)
            .collect();
        let deleted: Vec<_> = entries
            .iter()
            .filter(|e| e.operation == HumanOperation::DeleteWithChildren)
            .collect();
        assert_eq!(matched.len(), 2, "{:?}", entries);
        assert_eq!(deleted.len(), 1, "{:?}", entries);
        assert!(deleted[0].after_path.is_none());
        Ok(())
    }

    #[test]
    fn representative_entries_puts_the_surplus_after_nodes_on_plain_insert() -> Result<()> {
        let before_source = "fn main() {\n    foo();\n}\n";
        let after_source = "fn main() {\n    foo();\n    foo();\n}\n";
        let before_tree = parse_rust(before_source);
        let after_tree = parse_rust(after_source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        let before_foos = function_body_statements(before_root);
        let after_foos = function_body_statements(after_root);
        assert_eq!(before_foos.len(), 1);
        assert_eq!(after_foos.len(), 2);

        let mapping = HumanMapping {
            entries: vec![],
            groups: vec![MultiMapGroup {
                before_paths: before_foos.iter().map(|n| path_for_node(*n)).collect(),
                after_paths: after_foos.iter().map(|n| path_for_node(*n)).collect(),
                operation: HumanOperation::Identical,
                with_children: false,
                pairing: GroupPairing::AnyOneToOne,
            }],
            text_mappings: vec![],
        };

        let entries = representative_entries(&mapping, before_root, after_root)?;
        let matched: Vec<_> = entries
            .iter()
            .filter(|e| e.operation == HumanOperation::Identical)
            .collect();
        let inserted: Vec<_> = entries
            .iter()
            .filter(|e| e.operation == HumanOperation::Insert)
            .collect();
        assert_eq!(matched.len(), 1, "{:?}", entries);
        assert_eq!(inserted.len(), 1, "{:?}", entries);
        assert!(inserted[0].before_path.is_none());
        Ok(())
    }

    #[test]
    fn rebuild_caches_for_mapping_reports_group_membership_and_status_for_every_member()
    -> Result<()> {
        // 3 before, 2 after, with_children: one before-foo is a leftover delete. Every member gets
        // a plain-entry NodeStatus and appears in `before_group`/`after_group`.
        let before_source = "fn main() {\n    foo();\n    foo();\n    foo();\n}\n";
        let after_source = "fn main() {\n    foo();\n    foo();\n}\n";
        let before_tree = parse_rust(before_source);
        let after_tree = parse_rust(after_source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        let before_foos = function_body_statements(before_root);
        let after_foos = function_body_statements(after_root);
        assert_eq!(before_foos.len(), 3);
        assert_eq!(after_foos.len(), 2);

        let mapping = HumanMapping {
            entries: vec![],
            groups: vec![MultiMapGroup {
                before_paths: before_foos.iter().map(|n| path_for_node(*n)).collect(),
                after_paths: after_foos.iter().map(|n| path_for_node(*n)).collect(),
                operation: HumanOperation::Identical,
                with_children: true,
                pairing: GroupPairing::AnyOneToOne,
            }],
            text_mappings: vec![],
        };

        let caches = rebuild_caches_for_mapping(&mapping, before_root, after_root);

        for node in &before_foos {
            assert_eq!(
                caches.before_group.get(&node.id()),
                Some(&0),
                "every before group member should be listed, matched or not"
            );
            assert_ne!(
                status_before(*node, &caches),
                NodeStatus::Unmarked,
                "every before group member should have a real status, not Unmarked"
            );
        }
        for node in &after_foos {
            assert_eq!(caches.after_group.get(&node.id()), Some(&0));
            assert_ne!(status_after(*node, &caches), NodeStatus::Unmarked);
        }

        let matched_count = before_foos
            .iter()
            .filter(|n| status_before(**n, &caches) == NodeStatus::Matched)
            .count();
        let deleted_count = before_foos
            .iter()
            .filter(|n| {
                matches!(
                    status_before(**n, &caches),
                    NodeStatus::Marked {
                        kind: MarkKind::Deleted,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(matched_count, 2);
        assert_eq!(deleted_count, 1);
        Ok(())
    }

    #[test]
    fn rebuild_caches_for_mapping_keeps_plain_entries_when_a_group_does_not_resolve() {
        let source = "fn main() {\n    foo();\n}\n";
        let before_tree = parse_rust(source);
        let after_tree = parse_rust(source);
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();
        let statement_path = path_for_node(function_body_statements(before_root)[0]);

        let mapping = HumanMapping {
            entries: vec![HumanMappingEntry {
                before_path: Some(statement_path.clone()),
                after_path: Some(statement_path),
                operation: HumanOperation::Identical,
            }],
            groups: vec![MultiMapGroup {
                before_paths: vec![vec!["no_such_kind:1".to_string()]],
                after_paths: vec![],
                operation: HumanOperation::Identical,
                with_children: false,
                pairing: GroupPairing::AnyOneToOne,
            }],
            text_mappings: vec![],
        };

        let caches = rebuild_caches_for_mapping(&mapping, before_root, after_root);
        let statement = function_body_statements(before_root)[0];
        assert_eq!(status_before(statement, &caches), NodeStatus::Matched);
        assert!(caches.before_group.is_empty());
    }

    #[test]
    fn unmarked_node_count_of_an_empty_mapping_is_every_node() {
        let tree = parse_rust("fn main() {\n    foo();\n}\n");
        let root = tree.root_node();
        let caches = rebuild_caches(&[], root, root);

        let mut every_node = 0;
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            every_node += 1;
            let mut cursor = node.walk();
            stack.extend(node.children(&mut cursor));
        }
        assert_eq!(
            unmarked_node_count(root, &caches, status_before),
            every_node
        );
    }

    #[test]
    fn as_ast_diff_for_mapping_projects_a_group_through_its_representative_pairing() -> Result<()> {
        let before_source = "fn main() {\n    foo();\n    foo();\n    foo();\n}\n";
        let after_source = "fn main() {\n    foo();\n    foo();\n}\n";
        let before = crate::code::Code::from_string(before_source, &Language::Rust);
        let after = crate::code::Code::from_string(after_source, &Language::Rust);
        let before_root = before.ast.as_ref().unwrap().root_node();
        let after_root = after.ast.as_ref().unwrap().root_node();

        let before_foos = function_body_statements(before_root);
        let after_foos = function_body_statements(after_root);
        assert_eq!(before_foos.len(), 3);
        assert_eq!(after_foos.len(), 2);

        let mapping = HumanMapping {
            entries: vec![],
            groups: vec![MultiMapGroup {
                before_paths: before_foos.iter().map(|n| path_for_node(*n)).collect(),
                after_paths: after_foos.iter().map(|n| path_for_node(*n)).collect(),
                operation: HumanOperation::Identical,
                with_children: true,
                pairing: GroupPairing::AnyOneToOne,
            }],
            text_mappings: vec![],
        };

        let diff = as_ast_diff_for_mapping(&mapping, &before, &after)?;

        let matched_pairs = before_foos
            .iter()
            .filter(|n| {
                diff.before_node_map
                    .get(&n.id())
                    .is_some_and(|&after_id| after_id != 0)
            })
            .count();
        let deleted = before_foos
            .iter()
            .filter(|n| diff.before_node_map.get(&n.id()) == Some(&0))
            .count();
        assert_eq!(matched_pairs, 2, "{:?}", diff.before_node_map);
        assert_eq!(deleted, 1, "{:?}", diff.before_node_map);
        for n in &after_foos {
            assert_ne!(
                diff.after_node_map.get(&n.id()),
                None,
                "every after foo() should appear in the synthetic diff's after_node_map"
            );
        }
        Ok(())
    }

    fn path_map(entries: &[((&str, &str), ASTMappingOperation)]) -> PathKeyedMapping {
        entries
            .iter()
            .map(|((b, a), op)| ((vec![b.to_string()], vec![a.to_string()]), op.clone()))
            .collect()
    }

    #[test]
    fn describe_path_map_differences_is_empty_when_runs_agree() {
        let baseline = path_map(&[(("b", "a"), ASTMappingOperation::Identical)]);
        let repeat = baseline.clone();

        assert!(describe_path_map_differences(2, &baseline, &repeat).is_empty());
    }

    #[test]
    fn describe_path_map_differences_reports_a_differing_operation() {
        let baseline = path_map(&[(("b", "a"), ASTMappingOperation::Identical)]);
        let repeat = path_map(&[(("b", "a"), ASTMappingOperation::Update)]);

        let report = describe_path_map_differences(3, &baseline, &repeat);
        assert_eq!(report.len(), 1, "{report:?}");
        assert!(report[0].contains("run 1 and run 3"), "{}", report[0]);
        assert!(report[0].contains("Identical"), "{}", report[0]);
        assert!(report[0].contains("Update"), "{}", report[0]);
    }

    #[test]
    fn describe_path_map_differences_reports_a_pair_missing_from_one_run() {
        let baseline = path_map(&[(("b", "a"), ASTMappingOperation::Identical)]);
        let repeat = PathKeyedMapping::new();

        let report = describe_path_map_differences(2, &baseline, &repeat);
        assert_eq!(report.len(), 1, "{report:?}");
        assert!(report[0].contains("unmapped"), "{}", report[0]);
    }

    /// Identical source parsed three independent times must fully agree.
    #[test]
    fn describe_nondeterminism_is_empty_for_stable_source() {
        let report =
            describe_nondeterminism("fn f() { 1 + 1; }", "fn f() { 1 + 1; }", &Language::Rust);
        assert!(report.is_empty(), "{report:?}");
    }

    /// `node_extents` and `total_node_count_for` are numerator and denominator of one ratio.
    #[test]
    fn node_extents_matches_the_total_node_count_denominator() {
        let before = crate::code::Code::from_string(
            "fn main() {\n    let a = 1;\n    println!(\"{}\", a);\n}\n",
            &crate::code::Language::Rust,
        );
        let after = crate::code::Code::from_string(
            "fn main() {\n    let b = 2;\n    println!(\"{}\", b);\n}\n",
            &crate::code::Language::Rust,
        );
        assert_eq!(
            node_extents(&before).len() + node_extents(&after).len(),
            total_node_count_for(&before, &after)
        );
    }

    #[test]
    fn node_extents_marks_a_real_subset_as_leaves() {
        let code = crate::code::Code::from_string(
            "fn main() { let a = 1; }",
            &crate::code::Language::Rust,
        );
        let extents = node_extents(&code);
        let leaves = extents.iter().filter(|e| e.is_leaf).count();
        assert!(leaves > 0, "a parsed file must have leaf nodes");
        assert!(
            leaves < extents.len(),
            "a parsed file must also have interior nodes"
        );
    }

    /// The corpus has grammar-less fixtures (a `Makefile`), so this is empty, not a panic.
    #[test]
    fn node_extents_is_empty_without_an_ast() {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        std::io::Write::write_all(&mut file.as_file(), b"plain text").expect("write");
        let code = crate::code::Code::from_file(file.path()).expect("read back");
        assert!(code.ast.is_none(), "an extension-less file has no grammar");
        assert!(node_extents(&code).is_empty());
    }

    #[test]
    fn nodes_touched_by_marks_only_overlapping_nodes() {
        let code = crate::code::Code::from_string(
            "fn main() {\n    let a = 1;\n}\n",
            &crate::code::Language::Rust,
        );
        let extents = node_extents(&code);

        assert_eq!(
            nodes_touched_by(&extents, &[])
                .iter()
                .filter(|t| **t)
                .count(),
            0,
            "no spans must touch nothing"
        );

        // Row 1 is `    let a = 1;` - the whole line.
        let touched = nodes_touched_by(
            &extents,
            &[crate::diff::text_range::TextRange::new(1, 0, 1, 14)],
        );
        let count = touched.iter().filter(|t| **t).count();
        assert!(count > 0, "a span over a real line must touch some nodes");
        assert!(
            count < extents.len(),
            "a one-line span must not touch every node in the file"
        );
    }

    /// A pair with no tree-sitter grammar (a Bazel `BUILD` file) is graded against the product's
    /// own `plain_text_line_diff` fallback, compared directly so the two cannot drift.
    #[test]
    fn a_pair_with_no_grammar_paints_from_the_plain_text_fallback() -> Result<()> {
        let before_text = "cc_library(\n    name = \"a\",\n    srcs = [\"a.cc\"],\n)\n";
        let after_text = "cc_library(\n    name = \"a\",\n    srcs = [\"b.cc\"],\n)\n";
        let before = crate::code::Code::from_string(before_text, &Language::Unknown);
        let after = crate::code::Code::from_string(after_text, &Language::Unknown);
        assert!(
            before.ast.is_none() && after.ast.is_none(),
            "the premise of this test is a pair tree-sitter cannot parse"
        );

        let diff = codediff_diff_for_painting(&before, &after)?;
        let PaintingDiff::PlainText {
            before: before_ranges,
            after: after_ranges,
        } = &diff
        else {
            panic!("a pair with no AST must fall back to the plain-text diff");
        };
        let (expected_before, expected_after) =
            crate::diff::text::plain_text_line_diff(before_text, after_text);
        assert_eq!(before_ranges.len(), expected_before.len());
        assert_eq!(after_ranges.len(), expected_after.len());

        // The fallback reaches the scorer: the one changed character is labelled on both sides
        // (`intra_line_ranges` narrows the line). Both presets agree because every option that
        // could differ governs moves, which the fallback never emits.
        for options in [
            crate::diff::text::RenderOptions::MINIMAL,
            crate::diff::text::RenderOptions::FULL,
        ] {
            let labels = codediff_painting_labels(&diff, &before, &after, options);
            for (side, contents) in [(0usize, before_text), (1usize, after_text)] {
                let painted: String = contents
                    .char_indices()
                    .filter(|(index, _)| labels[side][*index].is_some())
                    .map(|(_, character)| character)
                    .collect();
                assert_eq!(
                    painted,
                    if side == 0 { "a" } else { "b" },
                    "side {side} under {options:?} should paint exactly the one changed \
                     character, and nothing else in the file"
                );
            }
        }
        Ok(())
    }

    mod exploratory;
}
