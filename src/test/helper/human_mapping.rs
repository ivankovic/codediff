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

/**
* Human-authored ground-truth AST mappings, used to check codediff's output against what a human
* considers the optimal diff.
*
* These are produced by the `human_solver` binary (src/bin/human_solver.rs), which lets a human
* walk the before/after ASTs of a test case side by side and mark nodes as matching, deleted or
* inserted. The result is stored as JSON in
* `src/test/data/diffs/{handmade,small,full,stratified}/<name>/human_mapping.json` - see
* [`super::DIFF_DATASETS`] for what each folder means; [`mapping_path`] is the one place that
* resolves which of them holds a given `name`.
*
* Nodes are identified by *path* (see [`super::path_for_node`] / [`super::node_for_path`]) rather
* than by TreeSitter node ID, because node IDs are arena slots that are not stable across separate
* parses of the same source: the human_solver process parses the code once to build the mapping,
* and the test that later verifies it parses the code again to compute the diff. Paths, being
* derived purely from node kind and sibling position, are stable across both parses.
*/
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tree_sitter::Node;

use crate::code::ASTMetadata;
use crate::diff::cost::operation_cost;
use crate::diff::{ASTDiff, ASTMapping, ASTMappingOperation, ASTMappingReason, NodeCache};
use crate::test::helper::{PathCache, path_for_node};

/// What a human decided should happen to a node (or pair of nodes) between before and after.
///
/// `Identical`, `Update` and `MatchButNotIdentical` all pair a before node with an after node
/// (like the old single `Match` variant did), but also pin down *which* [`ASTMappingOperation`]
/// codediff is expected to have chosen for that pair, not just that the pair is mapped together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanOperation {
    /// The before and after nodes are the same node, with no difference at all: same kind, no
    /// children, and identical text (or, if either has children, the human confirmed the whole
    /// subtree is unchanged). Expects codediff to have chosen [`ASTMappingOperation::Identical`].
    Identical,
    /// The before and after nodes have the same kind and no children, but different text (e.g. a
    /// changed string literal). Expects [`ASTMappingOperation::Update`].
    Update,
    /// The before and after nodes are matched, but not identical: either they have children and
    /// the human confirmed the subtree differs somewhere, or they have different kinds and the
    /// human confirmed the mapping anyway. Expects [`ASTMappingOperation::MatchButNotIdentical`].
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

/// A set of `before_paths` nodes that may map to `after_paths` nodes in *any* consistent pairing
/// -- used when there's genuine, human-confirmed ambiguity about which specific node should pair
/// with which (e.g. several interchangeable/near-duplicate statements). Any pairing codediff's
/// own diff produces counts as correct, as long as it uses `min(before_paths.len(),
/// after_paths.len())` pairs and leaves the rest of the larger side deleted/inserted -- see
/// [`check_group_entry`] for the actual validation, and [`representative_entries`] for the one
/// concrete pairing used for *display*/cost purposes (never for validation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiMapGroup {
    /// Paths to every before-side candidate node (N of them). Order is insertion order only -- it
    /// carries no meaning for validation, since any pairing is valid.
    pub before_paths: Vec<Vec<String>>,
    /// Paths to every after-side candidate node (M of them).
    pub after_paths: Vec<Vec<String>>,
    /// The operation every *realized* pair (whichever specific pairing codediff's diff actually
    /// produces) is expected to have chosen. Deliberately excludes `Update`/`Delete`/`Insert`/
    /// `DeleteWithChildren`/`InsertWithChildren`: `Update` means "same kind, no children,
    /// different text" for one fixed pair, which doesn't have a coherent meaning across an
    /// ambiguous N-to-M group, and the other four aren't matches at all (a group's own leftover
    /// members are how deletion/insertion is expressed -- see [`with_children`](Self::with_children)).
    pub operation: HumanOperation,
    /// Whether a matched pair's entire subtree must also close within itself (every descendant of
    /// one side maps to a descendant of the other, and vice versa -- see
    /// [`check_subtree_maps_within`]), and a leftover (unmatched) member's entire subtree must be
    /// deleted/inserted rather than just its own top node -- the group's equivalent of `M` vs `m`
    /// / of `*WithChildren` vs the bare operation for a plain [`HumanMappingEntry`].
    pub with_children: bool,
}

/// The full set of human decisions for one before/after test case.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HumanMapping {
    pub entries: Vec<HumanMappingEntry>,
    /// Multi-map groups (see [`MultiMapGroup`]) -- absent from any `human_mapping.json` written
    /// before this field existed, and from any current one with no groups in it, so every
    /// existing fixture keeps parsing (and re-saving byte-for-byte) unchanged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<MultiMapGroup>,
    /// The human-painted *text* account of the same diff (see [`HumanTextMapping`]) - a second,
    /// independent ground truth, deliberately not derived from `entries`.
    ///
    /// `Option`, not a plain `Vec` like `groups`, because the empty case is meaningful here and
    /// has to stay distinguishable: `None` is "nobody has painted this fixture yet", while
    /// `Some` with no entries is "somebody painted it and there was nothing to paint" (two
    /// identical files). Collapsing those would make a completeness count silently wrong. As with
    /// `groups`, a file written before this field existed still parses, and one with no text
    /// mapping still re-saves byte-for-byte unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_mapping: Option<HumanTextMapping>,
}

// ─── Human-painted text ranges ──────────────────────────────────────────────────────────────
//
// A second, independent ground truth living in the same file as the tree mapping above, and
// deliberately not derived from it. See [`HumanTextMapping`] for why both exist.

/// One span of source text, in exactly the row/**byte**-column space
/// [`crate::diff::text_range::TextRange`] uses everywhere else in this codebase: rows and columns
/// are 0-based, the end is exclusive, and an end column of 0 means "up to, not including, this
/// row".
///
/// Not a `TextRange` directly, for the same reason `generate_mapping_site`'s `JsonRange` isn't:
/// that keeps `diff::text_range` free of a serde dependency on its public type. Convert with
/// [`HumanTextSpan::to_text_range`].
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

/// What a human said about one painted span, and *only* what a human should have to say about it.
///
/// Deliberately three variants, not the five [`HumanOperation`] carries. A person reading a diff
/// can reliably answer "this text is gone", "this text is new" and "this text corresponds to that
/// text". Asking them to further split a correspondence into moved-versus-updated is asking them
/// to do something a machine does better and more consistently: the two differ precisely by
/// whether the two spans' contents are byte-identical, which [`HumanTextEntry::verdict`] decides
/// by looking. Recording a human judgement there would add a second, less reliable source for a
/// fact already determined by the spans themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HumanTextOperation {
    /// The before span and the after span are the same code. Whether that renders as a move or an
    /// update is derived, not recorded - see [`HumanTextEntry::verdict`].
    Match,
    /// The before span is gone from the after side.
    Delete,
    /// The after span is new.
    Insert,
}

/// One human-painted decision: a `Match` carries both spans, a `Delete` only `before`, an `Insert`
/// only `after`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanTextEntry {
    pub operation: HumanTextOperation,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub before: Option<HumanTextSpan>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub after: Option<HumanTextSpan>,
}

/// What a [`HumanTextEntry`] actually asserts once its spans have been read - the four operations
/// a renderer needs, from the three a human is asked for.
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
    /// Resolves this entry against the actual source, deriving `Move` vs `Update` for a `Match`.
    ///
    /// `Err` if the entry is malformed for its operation (a `Match` missing a side, a `Delete`
    /// with no before span) or if a span doesn't land inside its file - both of which mean the
    /// file was hand-edited or the fixture text changed underneath it, and neither is something
    /// to paper over with a default.
    pub fn verdict(&self, before: &str, after: &str) -> Result<HumanTextVerdict> {
        match self.operation {
            HumanTextOperation::Delete => {
                let span = self.before.context("a Delete entry has no `before` span")?;
                span_text(before, span)
                    .context("a Delete entry's span is outside the before file")?;
                Ok(HumanTextVerdict::Delete)
            }
            HumanTextOperation::Insert => {
                let span = self.after.context("an Insert entry has no `after` span")?;
                span_text(after, span)
                    .context("an Insert entry's span is outside the after file")?;
                Ok(HumanTextVerdict::Insert)
            }
            HumanTextOperation::Match => {
                let before_span = self.before.context("a Match entry has no `before` span")?;
                let after_span = self.after.context("a Match entry has no `after` span")?;
                let before_text = span_text(before, before_span)
                    .context("a Match entry's before span is outside the before file")?;
                let after_text = span_text(after, after_span)
                    .context("a Match entry's after span is outside the after file")?;
                // The whole derivation, and the reason a human is never asked: byte-identical
                // means the code was relocated, anything else means it was edited.
                Ok(if before_text == after_text {
                    HumanTextVerdict::Move
                } else {
                    HumanTextVerdict::Update
                })
            }
        }
    }
}

/// A human-painted account of a diff *as text*, stored alongside - and deliberately independent
/// of - the tree mapping in the same [`HumanMapping`].
///
/// **Why a second ground truth rather than one derived from the other.** The tree mapping records
/// which AST nodes correspond. That is not enough to determine what a reader should see, for two
/// separate reasons this project has now hit in practice:
///
/// * Many nodes carry no visible text of their own (`expression_statement` wrappers and the like),
///   so a node-level answer cannot be checked against what appears on screen without an extra,
///   unvalidated projection step - the same visible-versus-scaffolding split `visible_node_ids`
///   draws for mismatch counting.
/// * Even a mapping that is not in doubt anywhere leaves the *rendering* underdetermined. For a
///   reorder, "one line moved past five" and "five lines moved past one" describe the identical
///   set of matched pairs; the mapping, and any cost function over it, is indifferent between
///   them. See `research/data/quality/move_attribution.md`.
///
/// So this records what a person says the diff *looks like*, and the tree mapping records what
/// corresponds to what. Neither is derivable from the other, which is exactly what makes checking
/// one against the other worth doing - see [`text_mapping_disagreements`].
///
/// **Unpainted text is unchanged.** A person paints only what changed, so the absence of a span
/// is a positive claim that the text there is identical and in place. That is what makes a `Match`
/// whose two spans hold identical text meaningful rather than redundant: it says "this same code
/// is somewhere else", which is precisely a move.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HumanTextMapping {
    pub entries: Vec<HumanTextEntry>,
}

/// The text a span covers, or `None` if it falls outside `contents`.
///
/// Rows are split on `'\n'` and columns are byte offsets into a row, matching `TextRange`. A span
/// ending at column 0 of row *r* ends just before row *r* begins, i.e. it includes row *r-1*'s
/// newline - the convention `line_operations` and `columns_on_row` already use.
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

/// One byte-granular label for a side of the diff, shared by both the painted text mapping and
/// the tree mapping projected down to text, so the two can be compared on equal footing.
///
/// `None` (absent from a label vector) is "unchanged and in place", which both sources express by
/// saying nothing about that byte.
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
    /// The run of bytes over which both sources hold the same pair of opinions.
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
/// only matters for a malformed painting - the solver never produces overlapping spans.
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

/// Every byte run where the painted text mapping and the tree mapping disagree about what happened
/// to that text.
///
/// This is the check the two mappings are kept separate *for*. They are independent accounts of
/// the same edit - one recording which AST nodes correspond, the other what a reader should see -
/// so neither can be derived from the other and each can be wrong on its own. Where they disagree,
/// at least one of them is, and the fixture needs a human to look. An empty result is the two
/// agreeing, which is the only evidence available that either is right.
///
/// Compared **per byte**, not per span or per line: the two sources chunk the same edit completely
/// differently (a painted `Match` covering five lines against a dozen small node-derived ranges),
/// so comparing spans would report differences in bookkeeping as differences in opinion. Adjacent
/// bytes carrying the same pair of opinions are then coalesced into one run, so a five-line
/// disagreement is reported once rather than three hundred times.
///
/// `Ok(vec![])` when the fixture has no painted text mapping at all - nothing to check, which is
/// not the same as agreement and callers that count fixtures should test `text_mapping.is_some()`
/// themselves rather than reading an empty result as a pass.
pub fn text_mapping_disagreements(
    mapping: &HumanMapping,
    before: &crate::code::Code,
    after: &crate::code::Code,
) -> Result<Vec<TextMappingDisagreement>> {
    let Some(text_mapping) = mapping.text_mapping.as_ref() else {
        return Ok(Vec::new());
    };

    // The painted side.
    let mut painted_before: Vec<(HumanTextSpan, TextLabel)> = Vec::new();
    let mut painted_after: Vec<(HumanTextSpan, TextLabel)> = Vec::new();
    for entry in &text_mapping.entries {
        let label = TextLabel::from_verdict(entry.verdict(&before.contents, &after.contents)?);
        if let Some(span) = entry.before {
            painted_before.push((span, label));
        }
        if let Some(span) = entry.after {
            painted_after.push((span, label));
        }
    }

    // The tree side, through exactly the projection every renderer uses.
    let ast_diff = as_ast_diff_for_mapping(mapping, before, after)?;
    let node_cache = crate::diff::NodeCache::build(before, after);
    let text_diff = crate::diff::text::TextDiff::from(before, after, &ast_diff, &node_cache);

    let mut disagreements = Vec::new();
    for (side, contents, painted, ranges) in [
        (0usize, &before.contents, &painted_before, text_diff.all(0)),
        (1usize, &after.contents, &painted_after, text_diff.all(1)),
    ] {
        let painted_labels = label_bytes(contents, painted);
        let tree_labels = label_bytes_from_ranges(contents, &ranges);

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

/// Path to the `human_mapping.json` file for a given test case name (e.g. "rust-add-if"),
/// resolved across `DIFF_DATASETS` (`super::diffs_case_dir`) like every other per-name lookup.
/// Every caller (`load`/`save`, and the `.exists()` checks in `benchmark_optimal_solutions`/
/// `benchmark_other`) runs after the case directory already exists - either it's an already-open
/// case, or `human_solver`'s promote flow just created it - except when checking whether a
/// *candidate* name is free of a mapping at all, where "doesn't exist under any dataset" is
/// exactly the desired answer. `small` is an arbitrary but harmless fallback for a name that
/// resolves to nothing: `.exists()` on it is still `false`, the only thing every caller checks.
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

/// `pub`, not private: `src/bin/human_solver.rs` (a separate binary crate that depends on this
/// one, so `pub(crate)` wouldn't reach it) needs the identical `Vec<String>` -> `Vec<&str>`
/// conversion (for the same `node_for_path`/`PathCache::resolve` calls this module makes) and
/// previously carried its own byte-for-byte copy rather than reusing this one.
pub fn path_refs(path: &[String]) -> Vec<&str> {
    path.iter().map(String::as_str).collect()
}

/// What kind of human-authored removal a node is marked with: `Deleted` from the before tree, or
/// `Inserted` in the after tree.
///
/// Shared between `human_solver` (which lets a human create/edit a `HumanMapping`) and any
/// read-only consumer that just needs to interpret one (e.g. a static site generator) - moved
/// here, rather than kept private to `human_solver.rs`, specifically so a second consumer doesn't
/// have to carry its own copy of what matched/deleted/inserted/inherited means and risk it
/// silently drifting from the TUI's.
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

/// Resolved node IDs for every entry in a [`HumanMapping`], used to look up a node's
/// [`NodeStatus`] in O(1) (plus a bounded ancestor walk for inheritance) - see [`rebuild_caches`].
#[derive(Default)]
pub struct Caches {
    pub before_match: HashMap<usize, usize>,
    pub after_match: HashMap<usize, usize>,
    pub before_removed: HashMap<usize, bool>,
    pub after_removed: HashMap<usize, bool>,
    /// The exact [`HumanOperation`] (`Identical`/`Update`/`MatchButNotIdentical`) a matched node's
    /// pair was recorded as. `before_match`/`after_match` alone can't tell these apart - both write
    /// the same node-id pair into those maps regardless of which of the three operations produced
    /// them - so a consumer that cares whether a match is a real edit or genuinely unchanged (e.g.
    /// `generate_mapping_site`'s "hide identical matches" toggle, which treats anything other than
    /// `Identical` as a real edit; or its `Update`-vs-`MatchButNotIdentical` coloring, which needs
    /// the exact operation) reads this map, not `before_match`/`after_match` directly. Absent key
    /// means "not recorded" (e.g. a `Caches` built by hand rather than via `rebuild_caches`), which
    /// [`is_identical_before`]/[`is_identical_after`] treat as identical, to match matched nodes'
    /// pre-existing default rendering/quietness.
    pub before_operation: HashMap<usize, HumanOperation>,
    pub after_operation: HashMap<usize, HumanOperation>,
    /// Whether a matched node's `before_path` and `after_path` differ - i.e. the node sits at a
    /// different position (different ancestor chain and/or sibling index) after the edit than
    /// before it. Populated for every match-type entry, not just `Identical` ones, but the only
    /// caller that reads it (`generate_mapping_site`'s "moved without change" coloring) only ever
    /// consults it for `Identical` pairs - an `Update`/`MatchButNotIdentical` pair that also moved
    /// doesn't get a separate visual treatment, since that hasn't come up in the fixture corpus.
    pub before_moved: HashMap<usize, bool>,
    pub after_moved: HashMap<usize, bool>,
    /// Number of entries that couldn't be resolved against the current trees (e.g. a
    /// hand-edited or stale mapping file). Surfaced by callers (e.g. `human_solver`'s footer)
    /// rather than treated as fatal, so a bad mapping file doesn't block the caller outright.
    pub unresolved: usize,
    /// Node id -> index into `HumanMapping::groups`, for every node listed in a group's
    /// `before_paths` (whichever side the id belongs to has its own map) - both the members a
    /// group's representative pairing actually matched *and* its leftover members, unlike
    /// `before_match`/`before_removed` above, which only see whichever single outcome
    /// [`representative_entries`] picked. Populated only by [`rebuild_caches_for_mapping`] (plain
    /// [`rebuild_caches`] has no `groups` to read), used so a consumer can render a group-derived
    /// node distinctly from a plain one, and so `u` (in `human_solver`) can find and remove a
    /// whole group by any one of its members rather than just the one representative pair.
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
    // A `PathCache` per side, not a fresh `node_for_path` scan per entry: this runs on every
    // fixture, on every keystroke, in `human_solver`'s live editor, and (via `rebuild_caches_for_mapping`)
    // once per fixture across the whole corpus for `o`'s incomplete-only filter - a heavily
    // annotated fixture (tens of thousands of entries sharing high-fanout parents) turned the
    // per-entry rescan into several seconds each, measured up to ~2s on a single ~54k-entry
    // fixture alone.
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

/// Same as [`rebuild_caches`], but also folds in `mapping.groups`: every group's members are
/// flattened into one concrete representative pairing via [`representative_entries`] first, so a
/// grouped node's [`NodeStatus`] (`Matched`, or `Marked` deleted/inserted for a leftover member)
/// comes back exactly like a plain entry's would. On top of that, `before_group`/`after_group` are
/// populated for *every* listed member of every group - not just whichever one
/// `representative_entries` happened to pair - by resolving `before_paths`/`after_paths` directly.
///
/// If a group's paths fail to resolve against the current trees (a stale or hand-edited mapping
/// file), [`representative_entries`] is skipped in favor of `mapping.entries` alone rather than
/// failing outright - same "don't let one bad group block the whole caller" posture
/// [`rebuild_caches`] already takes for a single bad entry (`unresolved`), just without a precise
/// per-group count, since there's no single caller today that needs one.
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
/// isn't matched at all (or `caches` was built by hand rather than via [`rebuild_caches`]).
pub fn match_operation_before(node: Node, caches: &Caches) -> Option<HumanOperation> {
    caches.before_operation.get(&node.id()).copied()
}

/// After-side counterpart of [`match_operation_before`].
pub fn match_operation_after(node: Node, caches: &Caches) -> Option<HumanOperation> {
    caches.after_operation.get(&node.id()).copied()
}

/// Whether a `Matched` before-node's pair was recorded as `Identical` rather than
/// `Update`/`MatchButNotIdentical`. Meaningless (and unconsulted) for any other [`NodeStatus`].
/// Defaults to `true` when [`match_operation_before`] returns `None` - either because `node` isn't
/// matched, or because `caches` was built by hand rather than via [`rebuild_caches`] (as several
/// tests do), in which case treating it as identical preserves those matched nodes' existing
/// "quiet"/undecorated rendering.
pub fn is_identical_before(node: Node, caches: &Caches) -> bool {
    match_operation_before(node, caches).is_none_or(|op| op == HumanOperation::Identical)
}

/// After-side counterpart of [`is_identical_before`].
pub fn is_identical_after(node: Node, caches: &Caches) -> bool {
    match_operation_after(node, caches).is_none_or(|op| op == HumanOperation::Identical)
}

/// Whether a matched before-node's `before_path` differed from its pair's `after_path` - see
/// `Caches::before_moved`. Defaults to `false` (not moved) when absent, the same "assume nothing
/// noteworthy" convention [`is_identical_before`] uses for its own default.
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

/// Helper function to find a node by ID in a tree and return its kind.
/// Returns "None" if the node is not found, "0" if the ID is 0,
/// or the node kind if found.
/// Which side of the diff a [`Mismatch`]'s `node_id` belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Before,
    After,
}

/// One disagreement between the human mapping and codediff's actual output, tagged with the node
/// most directly responsible for it. `node_id` is `0` for a mismatch that isn't about any single
/// node (the nondeterminism check, `ASTDiff::is_valid`, a malformed multi-map group config) - a
/// real tree-sitter `Node::id()` is never `0`, matching the sentinel `before_node_map`/
/// `after_node_map` already use for "no counterpart" elsewhere in this codebase.
///
/// Exists so a caller can cross-reference `node_id`/`side` against
/// [`crate::diff::nodes::structurally_visible_node_ids`] and tell a mismatch on a rendered, user-visible node
/// apart from one on invisible structural scaffolding (a `block`, a `declaration_list`, ...) whose
/// misclassification has no independent effect on what the diff actually shows.
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

/// Pushes a mismatch for every node in `node`'s subtree (inclusive) that isn't mapped to zero
/// (i.e. deleted, if `node` is in the before tree, or inserted, if in the after tree) in `node_map`.
/// `side` is which side `node` (and therefore every descendant tagged here) is on.
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

/// Pushes a mismatch for every node in `subtree_root`'s subtree (inclusive) whose mapped
/// counterpart (via `node_map`) doesn't land inside `counterpart_ids` - the one-sided half of
/// [`check_subtree_maps_within`]'s closure check. `counterpart_lookup_root` is the *other* side's
/// full tree root, used only to describe what a wrongly-mapped-to node actually is (same role
/// `check_subtree_maps_to_zero`'s `lookup_root` plays). `side` is which side `subtree_root` is on.
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

/// Pushes a mismatch for every node in `before_node`'s subtree (inclusive) that isn't mapped to a
/// node within `after_node`'s subtree (inclusive), and vice versa - i.e. the two subtrees form a
/// *closed* pairing under `before_node_map`/`after_node_map`, with no leakage to nodes outside the
/// pair. This is a [`MultiMapGroup`] with `with_children` set's validation for a pair that
/// actually matched: unlike [`check_subtree_maps_to_zero`] (a fixed target, "maps to 0"), *which*
/// pair matched is only known after the fact here - it's whichever specific before/after pair
/// codediff's own diff happened to produce, out of the many a group allows - so this checks a
/// structural closure property instead of a fixed set of expected node ids.
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

/// Formats " (op X, reason Y)" for the mapping the before node actually landed in, so a mismatch
/// message identifies which pass produced the wrong mapping (via `ASTMappingReason`), not just
/// what it mapped to. Empty string when there's no mapping to describe.
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

/**
* Total edit cost of a human-authored `HumanMapping` under the same unit-cost model as
* `crate::diff::cost::diff_cost`, so the two numbers are directly comparable -
* `benchmark_optimal_solutions` prints both per fixture.
*
* Reuses `operation_cost` for the per-entry cost table rather than reimplementing it, so codediff's
* cost and the human's cost can never silently drift apart from having two separate copies of the
* same table. `before_metadata`/`after_metadata` must come from the same parsed `Code` as
* `before_root`/`after_root` (tree-sitter node ids are only stable within one parse - see
* `ASTNodeMetadata::start_byte`'s doc comment) so `node_for_path`'s resolved ids can look up subtree
* sizes in them for `DeleteWithChildren`/`InsertWithChildren` entries.
*
* Only sums entries actually present in `mapping` - an unannotated node contributes nothing. That's
* fine as long as the mapping is *complete over every actual change* (every unannotated node is
* genuinely unchanged, and therefore costs 0 whether or not it's written down): most fixtures'
* `human_mapping.json` only has a few hundred entries against many thousands of nodes precisely
* because the rest is untouched code, not because the human skipped grading real edits. If a fixture
* ever *does* leave a real change unannotated, this will silently undercount the human side and
* inflate `diff_cost - human_mapping_cost` for a reason that has nothing to do with the algorithm -
* worth checking with `--details` before trusting a surprising gap on an unfamiliar fixture.
*/
/// A node's `owned_text_hash`, or 0 when the node has no metadata entry - the same "absent means
/// owns nothing" convention `ASTNodeMetadata::owned_text_hash` uses, so a missing entry can never
/// be mistaken for a change.
fn owned_text_hash(metadata: &ASTMetadata, id: usize) -> u64 {
    metadata
        .node_info
        .get(&id)
        .map(|info| info.owned_text_hash)
        .unwrap_or(0)
}

pub fn human_mapping_cost(
    mapping: &HumanMapping,
    before_root: Node,
    after_root: Node,
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
) -> Result<u64> {
    // Same one-cache-per-root-per-loop pattern as `check_entry`'s caller - see `PathCache`'s own
    // doc comment for why a fixture with many DeleteWithChildren/InsertWithChildren entries under
    // one large flat parent (e.g. a big JSON object) needs this to stay linear.
    let mut before_cache = PathCache::new();
    let mut after_cache = PathCache::new();

    // Groups contribute too, via one concrete (if arbitrary) representative pairing - see
    // `representative_entries`'s own doc comment for why an arbitrary example is fine here even
    // though it would never be for validation.
    //
    // That "arbitrary is fine" became *conditional* when `MatchButNotIdentical` stopped always
    // costing 0 (2026-08-18): for a group over nodes that own text directly, which representative
    // gets paired with which now decides how many pairs are charged `COST_UPDATE`, so the total
    // would depend on a pairing that means nothing. Checked at the time and it does not arise -
    // of the corpus's 55 `match_but_not_identical` groups, zero resolve to a gap-owning kind. If a
    // group over XML `AttValue`s (or CSS values, or comments - see `ASTNodeMetadata::owned_text_
    // hash`) ever appears, revisit this rather than assuming the cost is still well-defined.
    let entries = representative_entries(mapping, before_root, after_root)?;

    let mut total = 0u64;
    for entry in &entries {
        let (operation, subtree_size, owned_text_changed) = match entry.operation {
            HumanOperation::Identical => (ASTMappingOperation::Identical, 1, false),
            HumanOperation::Update => (ASTMappingOperation::Update, 1, false),
            HumanOperation::MatchButNotIdentical => {
                // The one operation that has to look at the nodes themselves: a node owning text
                // directly (an XML attribute value, a YAML quoted scalar - see
                // `ASTNodeMetadata::owned_text_hash`) carries a difference that no descendant
                // entry accounts for, so it must be charged here or it is charged nowhere. Both
                // paths are always present for a match; a missing one can only mean a malformed
                // mapping, and treating it as "unchanged" keeps this a cost function rather than
                // a validator (`check_entry` is what rejects malformed entries).
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

/**
* Loads the human mapping for `name` and computes its total edit cost (see
* [`human_mapping_cost`]), resolving paths against a fresh parse of `before`/`after`.
*
* Convenience wrapper for callers (like `benchmark_optimal_solutions`) that only have a fixture
* name and a `Code` pair, not an already-loaded `HumanMapping`/already-built `ASTMetadata`.
*/
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

/// Builds a synthetic `ASTDiff` from `name`'s human mapping, resolving every entry's path(s)
/// against a fresh parse of `before`/`after` and feeding them through `ASTDiff::add_mapping` -
/// same shape as `human_mapping_cost`, but producing the full `ASTDiff` rather than just a total
/// cost, so any machinery that only knows how to consume a real `ASTDiff` (e.g.
/// `diff::text::TextDiff`) can treat the human-authored mapping exactly like codediff's own
/// output. Used by `benchmark_other` to project the human mapping down to per-line labels via the
/// same `TextDiff`/`line_operations` path codediff's own diff goes through, so the two are
/// comparable on equal footing.
///
/// `cost`/`reason` on the resulting `ASTMapping`s are placeholders - nothing that consumes a
/// synthetic diff built this way needs them, only `operation` and the node-id maps
/// (`ASTDiff::mapping_for_node`, which `diff::text::ranges` walks, only reads `operation`).
pub fn as_ast_diff(
    name: &str,
    before: &crate::code::Code,
    after: &crate::code::Code,
) -> Result<ASTDiff> {
    let mapping = load(name)?;
    as_ast_diff_for_mapping(&mapping, before, after)
}

/// Same as [`as_ast_diff`], but takes an already-loaded [`HumanMapping`] instead of loading (and
/// JSON-parsing) `name`'s file itself. Callers that already have the mapping in hand for another
/// reason (e.g. `generate_mapping_site`'s per-fixture loop, which loads it once to render that
/// fixture's own page) should call this directly rather than [`as_ast_diff`], which would
/// otherwise re-read and re-parse the same `human_mapping.json` a second time.
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

    // Groups contribute too, via one concrete representative pairing - see
    // `representative_entries`'s own doc comment.
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

/**
* `mapping.entries` plus, for each of `mapping.groups`, one *deterministic* representative
* pairing flattened into plain [`HumanMappingEntry`] values - matched pairs (sorted by each
* side's node start byte, then zipped pairwise) become `Identical`/`MatchButNotIdentical` entries
* per the group's own `operation`; any leftover on the larger side becomes
* `Delete`/`DeleteWithChildren` or `Insert`/`InsertWithChildren` per `with_children`.
*
* This is explicitly *a* valid solution, not *the* solution: a [`MultiMapGroup`] exists precisely
* because many pairings are equally correct, and this function has to pick just one to produce
* something concrete. It's used only where a single concrete example is good enough -
* [`human_mapping_cost`] (so a group contributes to the printed cost total) and
* [`as_ast_diff_for_mapping`] (so a group shows up in a synthetic `ASTDiff`, e.g. for
* `benchmark_other`'s comparisons) - **never** for the actual pass/fail check, which is
* [`check_group_entry`] instead: that one checks the real question ("does codediff's actual
* mapping use *some* valid pairing"), not whether it happened to pick this particular one.
*/
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

        // Sorted purely so the representative pairing is deterministic (stable across repeated
        // calls, not dependent on `before_paths`/`after_paths`' original JSON order) - it doesn't
        // need to mean anything beyond that.
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

/**
* Checks one [`MultiMapGroup`] against `diff_ast`'s actual output: *any* pairing between the
* group's before/after nodes counts as correct, as long as it uses exactly `min(N, M)` pairs and
* the rest of the larger side ends up deleted/inserted - see the struct's own doc comment.
*
* 1. Every before-group node must be either matched to an after-group node (recorded as a pair)
*    or mapped to 0 (deleted) - anything else (matched to a node outside the group) is a mismatch.
* 2. Every after-group node not already claimed by a pair from step 1 must be mapped to 0
*    (inserted) - anything else is a mismatch, symmetric to step 1. (A leftover after-node whose
*    actual partner *is* a before-group member can't happen without step 1 already having found
*    that pair, given `ASTDiff`'s own before/after maps agree with each other - see
*    `compute_mismatches_for_with_config`'s separate `is_valid` check.)
* 3. The number of pairs actually found must equal `min(N, M)` exactly - this is what catches
*    codediff deleting *and* inserting instead of matching when it could have: each individual
*    node's fate can look locally valid (deleted is a valid fate, inserted is a valid fate) while
*    the group as a whole still under-matched, which steps 1-2 alone wouldn't catch.
* 4. Every pair found must use the group's declared `operation` - not skipped, so a group can't
*    quietly stop caring whether codediff chose the right kind of match.
* 5. If `with_children`: every matched pair's whole subtree must close within itself
*    ([`check_subtree_maps_within`]), and every leftover member's whole subtree must be
*    deleted/inserted ([`check_subtree_maps_to_zero`]) - not just its own top node.
*/
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

    // A representative node for the group as a whole, used only for mismatches that describe the
    // group's own aggregate state (a malformed `operation`, a wrong pair count) rather than one
    // specific node - the first before-group node if there is one, else the first after-group node.
    // `MultiMapGroup`'s own invariant (at least one of `before_paths`/`after_paths` non-empty) means
    // the `(0, Side::Before)` fallback is unreachable in practice.
    let (group_node_id, group_side) = before_nodes
        .first()
        .map(|n| (n.id(), Side::Before))
        .or_else(|| after_nodes.first().map(|n| (n.id(), Side::After)))
        .unwrap_or((0, Side::Before));

    let context = format!(
        "multi-map group ({} before <-> {} after, {:?}{})",
        before_nodes.len(),
        after_nodes.len(),
        group.operation,
        if group.with_children {
            ", with children"
        } else {
            ""
        }
    );

    let expected_op = match group.operation {
        HumanOperation::Identical => ASTMappingOperation::Identical,
        HumanOperation::MatchButNotIdentical => ASTMappingOperation::MatchButNotIdentical,
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

    let mut matched_pairs: Vec<(Node<'b>, Node<'a>)> = Vec::new();
    let mut leftover_before: Vec<Node<'b>> = Vec::new();
    for &b in &before_nodes {
        let actual = diff_ast.before_node_map.get(&b.id()).copied();
        match actual {
            Some(0) => leftover_before.push(b),
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
                        "{context}: before node '{}' was expected to match within the group or be deleted, but it mapped to {}{}",
                        b.kind(),
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
            Some(0) => leftover_after.push(a),
            other => {
                let mapped_kind = match other {
                    Some(mapped_id) => node_kind_for_id(before_root, mapped_id),
                    None => "None".to_string(),
                };
                mismatches.push(Mismatch {
                    message: format!(
                        "{context}: after node '{}' was expected to match within the group or be inserted, but it mapped to {}{}",
                        a.kind(),
                        mapped_kind,
                        actual_mapping_info_after(diff_ast, a.id(), other)
                    ),
                    node_id: a.id(),
                    side: Side::After,
                });
            }
        }
    }

    let expected_matched = before_nodes.len().min(after_nodes.len());
    if matched_pairs.len() != expected_matched {
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
            Some(actual_mapping) if actual_mapping.operation == expected_op => {}
            Some(actual_mapping) => mismatches.push(Mismatch {
                message: format!(
                    "{context}: pair '{}' <-> '{}' expected codediff operation {expected_op:?}, but it chose {:?}",
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

    if group.with_children {
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

/// One `diff_code` run's mapping, keyed by node *path* rather than node ID.
///
/// Node IDs are tree-sitter arena slots: stable within one parse, but not across separate parses
/// of identical source (allocator/arena layout can differ run to run, even within the same
/// process). A determinism check that reuses a single parse for every run can't see that class of
/// bug at all - both runs would agree on IDs trivially. Keying by path (derived purely from node
/// kind and sibling position, see [`super::path_for_node`]) makes two independently-parsed runs
/// directly comparable.
type PathKeyedMapping = HashMap<(Vec<String>, Vec<String>), ASTMappingOperation>;

/// Runs `diff_code_with_config` on a *fresh* parse of `before_source`/`after_source` and returns
/// its mapping keyed by path. Parsing fresh (rather than reusing an already-parsed `Code`) is the
/// point: it's what actually reproduces the arena-layout variation a separate process launch
/// would see. See [`crate::diff::HeuristicConfig`] for what `config` is for.
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

    // One cache per side, reused across every mapping entry below - see `PathCache`'s own doc
    // comment for why a fresh `path_for_node` per entry would be quadratic here: this walks
    // *every* mapped node in the whole diff (not just human-annotated ones), and for a large flat
    // container (e.g. a big JSON object) many thousands of them share the same huge-fanout parent.
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

/// Compares two path-keyed mappings and describes every pair whose presence or
/// `ASTMappingOperation` differs between them - i.e. every sign that `diff_code` is not a pure
/// function of its inputs. Empty when the runs fully agree.
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

/// Same as [`describe_nondeterminism`], but computes each of the three independent runs via
/// [`diff_paths_with_config`] - see [`crate::diff::HeuristicConfig`] for what `config` is for.
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

/**
* Loads the human mapping for `name`, computes codediff's own diff for the same test case, and
* returns every point of disagreement between the two (empty if they fully agree).
*
* For fixtures in [`crate::test::helper::UNIT_TEST_FIXTURES`], also re-parses the before/after
* source two more times from scratch and re-diffs, comparing all three results by node *path* (not
* ID - see [`describe_nondeterminism`]) against each other: `diff_code` is supposed to be a pure
* function of its source text, so any difference between independently-parsed runs means some pass
* is relying on something other than the source text (e.g. an unordered `HashMap`/`HashSet`
* iteration, or a tree-sitter arena node ID used as a sort key) to pick a winner - which would
* otherwise silently make every mismatch count in this suite, and in `benchmark_optimal_solutions`
* (which shares this function), unreliable from run to run. Sampled rather than run for every
* fixture (2026-08-08): this quadruples the diff pipeline's cost per fixture it runs on, and a
* nondeterminism bug is a property of a code path, not a specific fixture - the per-language sample
* exercises every language's pipeline the same way the full corpus would.
*
* Shared by `assert_matches_human_mapping` (which just turns a non-empty result into a test
* failure) and the `benchmark_optimal_solutions` binary (which wants the raw count across every
* fixture, not a single pass/fail).
*/
pub fn compute_mismatches(name: &str) -> Result<Vec<String>> {
    compute_mismatches_with_config(name, &crate::diff::HeuristicConfig::default())
}

/// Same as [`compute_mismatches`], but forwards `config` to [`compute_mismatches_for_with_config`]
/// - see [`crate::diff::HeuristicConfig`] for what it's for. Used by
///   `benchmark_optimal_solutions --details --no-solver-X`.
pub fn compute_mismatches_with_config(
    name: &str,
    config: &crate::diff::HeuristicConfig,
) -> Result<Vec<String>> {
    let (before, after) = crate::test::helper::handmade_test_code_pair(name)?;
    compute_mismatches_for_with_config(name, &before, &after, config)
}

/// Same as [`compute_mismatches_with_config`], but via [`compute_visible_mismatches_for_with_config`]
/// - loads `name`'s before/after pair itself rather than requiring the caller to already have it.
pub fn compute_visible_mismatches_with_config(
    name: &str,
    config: &crate::diff::HeuristicConfig,
) -> Result<VisibleMismatches> {
    let (before, after) = crate::test::helper::handmade_test_code_pair(name)?;
    compute_visible_mismatches_for_with_config(name, &before, &after, config)
}

/**
* Total number of AST nodes across both `before` and `after` - the denominator
* `benchmark_optimal_solutions` uses to turn a fixture's absolute mismatch count into a relative
* percentage, so a 3-mismatch fixture with 20 nodes and a 3-mismatch fixture with 2000 nodes don't
* read as equally bad.
*/
pub fn total_node_count_for(before: &crate::code::Code, after: &crate::code::Code) -> usize {
    let node_cache = NodeCache::build(before, after);
    node_cache.before.len() + node_cache.after.len()
}

/// Reduces one side's `TextOperation`s to "touched or not" - the only signal comparable against a
/// line-only external tool (e.g. Unix `diff`), which has no notion of an AST node at all, only
/// "this line differs."
fn touched(ops: &[crate::diff::text::TextOperation]) -> Vec<bool> {
    ops.iter()
        .map(|op| *op != crate::diff::text::TextOperation::Identical)
        .collect()
}

/**
* Projects `ast_diff` down to per-line touched masks for both sides, via `TextDiff`/
* `line_operations` - the same path both codediff's own diff and a synthetic human-mapping diff
* (see [`as_ast_diff`]) go through, so any two diffs of the same before/after pair reduce to line
* labels identically and are safe to compare with [`line_disagreement_count`].
*
* Shared by `benchmark_other` (scores several external line-only tools this way) and
* [`line_mismatches_for`] below (the "codediff mismatches"/"unix diff mismatches" columns
* `generate_mapping_site` puts on its index page) - kept in one place so the two can't drift.
*/
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

/// Number of positions where `a` and `b` disagree. Panics on a length mismatch - `a`/`b` always
/// come from splitting the exact same `contents` string on `'\n'`, so their lengths can never
/// legitimately differ.
pub fn line_disagreement_count(a: &[bool], b: &[bool]) -> usize {
    assert_eq!(
        a.len(),
        b.len(),
        "line count mismatch between two labelings of the same file"
    );
    a.iter().zip(b).filter(|(x, y)| x != y).count()
}

/// One AST node's extent, for the node-granularity counterpart of [`touched_lines`].
///
/// `range` is in the same row/column space `TextRange` uses everywhere else in this codebase -
/// tree-sitter's own rows and *byte* columns, passed through unchanged by
/// [`TextRange::from_treesitter_range`].
pub struct NodeExtent {
    pub range: crate::diff::text_range::TextRange,
    /// Whether this node has no children. Leaves are the granularity every AST-aware external
    /// tool in `benchmark_other` actually reports at, and the only nodes whose extents don't
    /// nest, so they're scored separately from the all-nodes count - see
    /// [`nodes_touched_by`]'s doc comment.
    pub is_leaf: bool,
    /// This node's own tree-sitter id, so a caller can cross-reference the extent against an
    /// id-keyed set - specifically [`crate::diff::nodes::structurally_visible_node_ids`], for the
    /// visible-only view of the same scoring (see `benchmark_other`'s `visible_filter`).
    /// Carried on the extent rather than left to the caller to recompute by re-walking in the
    /// same order: [`node_extents`]'s traversal pushes children reversed onto a stack, which is
    /// *not* the order `structurally_visible_node_ids`'s own walk uses, so any attempt to zip the two by
    /// position would silently misattribute visibility to the wrong node.
    pub node_id: usize,
}

/// Every node of `code`'s AST, in a deterministic preorder walk.
///
/// The same node *set* [`crate::diff::NodeCache`] caches (every node, named and anonymous), so
/// `node_extents(before).len() + node_extents(after).len()` equals
/// [`total_node_count_for`]`(before, after)` - the two are used as numerator and denominator of
/// the same ratio, so they must agree on what counts as a node. Deliberately preorder (not
/// `NodeCache`'s own iteration order, which is an unordered `FxHashMap`): two labelings of the
/// same file have to be index-comparable, which a hash map can't guarantee.
///
/// Empty (no AST parsed) for a `Code` with no tree-sitter grammar.
pub fn node_extents(code: &crate::code::Code) -> Vec<NodeExtent> {
    let Some(ast) = code.ast.as_ref() else {
        return Vec::new();
    };
    let columns = crate::code::metadata::compute_columns_per_row(&code.contents);
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

/// One bool per entry of `extents`: whether any range in `spans` overlaps that node's extent -
/// the node-granularity counterpart of [`touched_lines`]'s per-line bools.
///
/// Deliberately a "touched or not" projection, exactly like [`touched_lines`], **not** the
/// mapping-fidelity metric `benchmark_optimal_solutions` reports for codediff. An external tool
/// reports changed *regions* of text, not a node-to-node mapping over this codebase's AST (it
/// has its own tree, with its own node identities), so "which node did this one become" is not a
/// question any of them can be asked. What they can all be asked is "did you consider this node's
/// text changed", and that is what this scores - for codediff and the external tools alike, so
/// the resulting columns are comparable to each other and *not* to the optimal-solutions number.
///
/// Whole-extent overlap, so an interior node counts as touched when a change lands anywhere
/// inside it. That makes every ancestor of a change touched, up to the root - which is why the
/// all-nodes count is reported alongside a leaves-only one (`is_leaf`): the leaf count isolates
/// "which tokens did the tool think changed" from "how deep is this grammar's tree."
pub fn nodes_touched_by(
    extents: &[NodeExtent],
    spans: &[crate::diff::text_range::TextRange],
) -> Vec<bool> {
    extents
        .iter()
        .map(|extent| spans.iter().any(|span| span.intersects(&extent.range)))
        .collect()
}

/// The changed (non-`Identical`) ranges on each side of `ast_diff`, as `(before, after)` -
/// the representation an external tool's own reported regions are normalized into so both can be
/// fed to [`nodes_touched_by`] and compared.
///
/// Used for both the ground truth (a synthetic `ASTDiff` from [`as_ast_diff`]) and codediff's own
/// output, so the two go through identical machinery and any asymmetry is in the diff itself, not
/// in how it was measured.
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

/**
* Shells out to the real `diff`, not a reimplementation - the whole point of comparing against it
* is comparing against the actual tool people run. Writes `before`/`after`'s contents to fresh temp
* files rather than trusting a fixture's on-disk `before.<lang>.test`/`after.<lang>.test` naming, so
* this works for any `Code` pair, not just ones that came from a fixture directory.
*
* Uses GNU diffutils' `--old-line-format`/`--new-line-format`/`--unchanged-line-format` (`%dn`
* prints a line's 1-indexed line number) instead of parsing unified-diff hunk headers by hand - two
* invocations (one per side), each printing exactly the touched line numbers on that side and
* nothing else.
*/
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

/// Runs `diff` with the given `--*-line-format` flags (see [`unix_diff_line_labels`]) and turns
/// its stdout - one 1-indexed line number per line - into a 0-indexed `line_count`-long touched
/// mask.
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
    // diff exits 0 for "no differences" and 1 for "differences found" - both are success for our
    // purposes. 2+ is a real error (bad flags, unreadable file, ...).
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

/// Line-level mismatch counts for one fixture, both against the human-authored mapping's own
/// per-line projection (see [`touched_lines`]) - `codediff` and `unix_diff` are directly
/// comparable to each other (same `total_lines` denominator, same projection method), which is the
/// whole point: unlike an AST-node mismatch count, a line mismatch count is meaningful for a
/// line-only tool like Unix `diff` too.
pub struct LineMismatches {
    pub codediff: usize,
    pub unix_diff: usize,
    /// `before`'s line count plus `after`'s - the denominator both `codediff` and `unix_diff` are
    /// counted out of.
    pub total_lines: usize,
}

/**
* The human mapping's own per-line touched/untouched projection (see [`touched_lines`]), plus the
* [`NodeCache`] built along the way - handed back, not just consumed internally, because every
* caller of this function goes on to project a *second* diff (codediff's own, or an external
* tool's) onto the same `before`/`after` pair via [`touched_lines`], which also needs a
* `NodeCache` - returning the one already built here means that second projection doesn't need its
* own separate `NodeCache::build` call.
*
* Shared by `benchmark_other`'s `score_fixture`/`print_details` and [`line_mismatches_for_mapping`]
* below, which otherwise each repeated this exact "resolve the human mapping against a fresh
* `ASTDiff`, then reduce it to per-line labels" recipe independently.
*/
pub fn human_touched_lines_for_mapping(
    mapping: &HumanMapping,
    before: &crate::code::Code,
    after: &crate::code::Code,
) -> Result<(Vec<bool>, Vec<bool>, NodeCache)> {
    let human_diff = as_ast_diff_for_mapping(mapping, before, after)?;
    let node_cache = NodeCache::build(before, after);
    let (human_before, human_after) = touched_lines(before, after, &human_diff, &node_cache);
    Ok((human_before, human_after, node_cache))
}

/// Same as [`human_touched_lines_for_mapping`], but loads `name`'s `human_mapping.json` itself
/// rather than taking an already-loaded [`HumanMapping`] - see [`as_ast_diff_for_mapping`]'s doc
/// comment for why a caller that already has the mapping in hand should prefer the `_for_mapping`
/// form instead.
pub fn human_touched_lines_for(
    name: &str,
    before: &crate::code::Code,
    after: &crate::code::Code,
) -> Result<(Vec<bool>, Vec<bool>, NodeCache)> {
    let mapping = load(name)?;
    human_touched_lines_for_mapping(&mapping, before, after)
}

/**
* Computes [`LineMismatches`] for one fixture: codediff's own diff and Unix `diff`, each reduced to
* per-line touched/untouched labels and compared against the human mapping's own projection of the
* same shape (see [`touched_lines`]/[`as_ast_diff`]).
*
* This is deliberately narrower than `benchmark_other`'s full `ExternalTool` comparison (which also
* covers GumTree, difftastic, and diffsitter) - those each need a separately-installed, non-Cargo
* binary pointed at by an environment variable, which `generate_mapping_site` (the only caller of
* this function, for its index page's sortable "codediff mismatches"/"unix diff mismatches"
* columns) can't assume is present. Unix `diff` alone needs nothing beyond the `diff` binary every
* CI runner and dev machine already has.
*/
pub fn line_mismatches_for(
    name: &str,
    before: &crate::code::Code,
    after: &crate::code::Code,
) -> Result<LineMismatches> {
    let mapping = load(name)?;
    line_mismatches_for_mapping(&mapping, before, after)
}

/// Same as [`line_mismatches_for`], but takes an already-loaded [`HumanMapping`] instead of
/// loading `name`'s file itself - see [`as_ast_diff_for_mapping`]'s doc comment for why a caller
/// that already has the mapping in hand should prefer this.
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

/**
* Same as [`compute_mismatches`], but takes an already-loaded before/after pair instead of looking
* it up via [`crate::test::helper::handmade_test_code_pair`].
*
* Callers that check many fixtures in a loop (e.g. `benchmark_optimal_solutions`) should load the
* full `handmade_test_code_pairs()` map once and call this directly with a borrowed pair, rather
* than going through `compute_mismatches` once per fixture - that map clone is O(fixture count)
* work just to reach a single entry, whereas `handmade_test_code_pair` only pays for the one
* fixture it's asked for.
*/
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

/// Same as [`compute_mismatches_for`], but computes codediff's diff (and its determinism check)
/// via [`crate::diff::diff_code_with_config`] - see [`crate::diff::HeuristicConfig`] for what
/// `config` is for. Used by `benchmark_optimal_solutions --no-solver-X`'s ablation study.
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

/// Same as [`compute_mismatches_for_with_config`], but keeps each mismatch's [`Mismatch::node_id`]/
/// [`Mismatch::side`] instead of collapsing straight to its message - what
/// `compute_mismatches_for_with_config` itself is now a thin wrapper around. Exists so a caller
/// (e.g. `benchmark_optimal_solutions`) can cross-reference each mismatch against
/// [`crate::diff::nodes::structurally_visible_node_ids`] to separate mismatches on rendered, user-visible nodes
/// from ones on invisible structural scaffolding - see [`Mismatch`]'s own doc comment.
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

/// [`compute_mismatches_detailed_for_with_config`]'s actual body, taking an already-computed
/// `diff_ast`/`node_cache` instead of running `diff_code_with_config` itself - lets
/// [`compute_visible_mismatches_for_with_config`] reuse the one diff run for both the mismatch
/// check and [`crate::diff::nodes::structurally_visible_node_ids`], rather than paying for `diff_code_with_config`
/// twice. `compute_mismatches_detailed_for_with_config` itself stays the entry point for a caller
/// that doesn't already have a diff to hand.
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
    // Sampled, not run for every fixture: the determinism check alone triples the diff pipeline's
    // cost (3 extra full runs on top of the real one above), and a nondeterminism bug (unordered
    // HashMap/HashSet iteration, an arena node ID used as a sort key) is a property of a *code
    // path*, not a specific fixture - the per-language `UNIT_TEST_FIXTURES` sample exercises every
    // language's pipeline the same way the full corpus would, at a fraction of the cost. See
    // TODO.md's 2026-08-08 entry for the corpus-wide timing that motivated this.
    //
    // Neither of these two checks is about any one node, so both get the `(0, Side::Before)`
    // sentinel - see `Mismatch`'s own doc comment.
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

    // Check that the produced diff is valid
    if !diff_ast.is_valid(before, after, node_cache) {
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

/// [`compute_mismatches_detailed_for_with_config`]'s mismatches, split by whether each one's node
/// is *visible* - i.e. it carries text of its own, per
/// [`crate::diff::nodes::is_structurally_visible`] - plus the total visible node count on each
/// side, the denominator a raw visible-mismatch count needs to be read as a rate rather than an
/// absolute. Structural, so nothing about the diff being scored can move either number. A mismatch with the `node_id: 0` sentinel (not about any single node - the
/// nondeterminism check, `ASTDiff::is_valid`, a malformed multi-map group) is never visible.
pub struct VisibleMismatches {
    pub visible: Vec<Mismatch>,
    pub invisible: Vec<Mismatch>,
    pub before_visible_node_count: usize,
    pub after_visible_node_count: usize,
}

/// Runs a single `diff_code_with_config`/`NodeCache::build` and shares it between the mismatch
/// check and [`crate::diff::nodes::structurally_visible_node_ids`] - unlike `benchmark_optimal_solutions`'s own
/// "each computation gets its own independent diff_code call" convention (see that binary's
/// `algorithm_cost_for`/`elapsed_ms_for` doc comments), this function is also the body of every
/// generated `optimal_solutions/<name>.rs` test (via [`assert_matches_human_mapping_within_limit`]),
/// so it runs once per fixture on every `cargo test` - doubling the diff cost there isn't a
/// negotiable "interactive tool" trade-off the way it is in that benchmark binary.
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

/**
* Loads the human mapping for `name`, computes codediff's own diff for the same test case, and
* checks that every human-authored decision holds in codediff's output.
*
* This is the whole body of the generated `optimal_solutions/<name>.rs` tests: `human_solver`
* writes a human_mapping.json file, and each of those tests just calls this function. Reports every
* mismatch at once (rather than failing on the first one), since the point of these tests is to see
* the full extent of any disagreement between codediff and the human-authored optimum.
*/
pub fn assert_matches_human_mapping(name: &str) -> Result<()> {
    assert_matches_human_mapping_within_limit(name, 0, 0)
}

/**
* Same as [`assert_matches_human_mapping`], but allows up to `upper_limit_of_mismatched_nodes`
* total mismatches and up to `upper_limit_of_visible_mismatched_nodes` *visible* mismatches
* (see [`VisibleMismatches`]/[`crate::diff::nodes::structurally_visible_node_ids`]) instead of requiring an exact
* match. Fails if *either* limit is exceeded - the two are independent bars, not one derived from
* the other, since a fixture can regress on one without moving the other at all (e.g. a fix that
* turns invisible mismatches visible without changing the total).
*
* For fixtures where codediff's mapping has a known, understood gap against the human-authored
* mapping (documented in `TODO.md` - an objective-wall gap, a premature-pruning architecture
* issue, etc. - not a bug that's simply unfixed), pin both limits to today's actual counts. The
* test still catches *regressions* (either count increasing past its limit) without blocking the
* suite on a fix that doesn't exist yet. When a fix does land for one of these gaps, lower the
* limit(s) (or switch back to [`assert_matches_human_mapping`] if both reach 0) so the test keeps
* the new bar.
*/
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

    /// Columns are **byte** offsets, matching `TextRange` - the trap a char/byte mix-up sets is
    /// invisible on ASCII and silently shifts every span past the first multi-byte character.
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
        // Past the end of a row, and past the end of the file, are both absent rather than
        // clamped - a span that doesn't fit means the file changed underneath the mapping.
        assert_eq!(span_text(contents, span(2, 0, 2, 99)), None);
        assert_eq!(span_text(contents, span(99, 0, 99, 1)), None);
        // Mid-character is refused rather than panicking on the slice.
        assert_eq!(span_text(contents, span(0, 5, 0, 6)), None);
    }

    /// The derivation that is the whole reason a human is only asked for three operations.
    #[test]
    fn a_match_derives_move_from_identical_text_and_update_from_differing_text() {
        let before = "alpha\nbeta\n";
        let after = "beta\nalpha\n";

        let moved = HumanTextEntry {
            operation: HumanTextOperation::Match,
            before: Some(span(0, 0, 0, 5)),
            after: Some(span(1, 0, 1, 5)),
        };
        assert_eq!(
            moved.verdict(before, after).unwrap(),
            HumanTextVerdict::Move,
            "identical text in two places is a relocation"
        );

        let edited = HumanTextEntry {
            operation: HumanTextOperation::Match,
            before: Some(span(0, 0, 0, 5)),
            after: Some(span(0, 0, 0, 4)),
        };
        assert_eq!(
            edited.verdict(before, after).unwrap(),
            HumanTextVerdict::Update,
            "corresponding text that differs is an edit"
        );
    }

    #[test]
    fn a_malformed_entry_is_an_error_rather_than_a_default() {
        let before = "alpha\n";
        let after = "alpha\n";

        let no_after = HumanTextEntry {
            operation: HumanTextOperation::Match,
            before: Some(span(0, 0, 0, 5)),
            after: None,
        };
        assert!(no_after.verdict(before, after).is_err());

        let outside = HumanTextEntry {
            operation: HumanTextOperation::Delete,
            before: Some(span(9, 0, 9, 1)),
            after: None,
        };
        assert!(outside.verdict(before, after).is_err());
    }

    /// A file with no text mapping must still round-trip byte-for-byte: every fixture in the
    /// corpus predates this field, and re-saving one from the solver must not rewrite them all.
    #[test]
    fn a_mapping_without_a_text_mapping_serializes_without_the_key() {
        let mapping = HumanMapping {
            entries: vec![HumanMappingEntry {
                operation: HumanOperation::Identical,
                before_path: Some(vec!["source_file:1".to_string()]),
                after_path: Some(vec!["source_file:1".to_string()]),
            }],
            groups: vec![],
            text_mapping: None,
        };

        let json = serde_json::to_string_pretty(&mapping).unwrap();

        assert!(!json.contains("text_mapping"), "got: {json}");
        let round_tripped: HumanMapping = serde_json::from_str(&json).unwrap();
        assert!(round_tripped.text_mapping.is_none());
    }

    /// `None` and `Some(empty)` have to stay distinguishable: "nobody has painted this" is not
    /// "somebody painted it and there was nothing to paint".
    #[test]
    fn an_empty_text_mapping_is_distinguishable_from_an_absent_one() {
        let mapping = HumanMapping {
            entries: vec![],
            groups: vec![],
            text_mapping: Some(HumanTextMapping::default()),
        };

        let json = serde_json::to_string_pretty(&mapping).unwrap();
        let round_tripped: HumanMapping = serde_json::from_str(&json).unwrap();

        assert!(json.contains("text_mapping"), "got: {json}");
        assert!(round_tripped.text_mapping.is_some());
        assert!(round_tripped.text_mapping.unwrap().entries.is_empty());
    }

    #[test]
    fn text_mapping_disagreements_reports_nothing_when_the_two_accounts_agree() {
        // A one-token edit: `1` becomes `2`. The tree mapping calls the integer an Update; the
        // painted mapping says the same thing about the same bytes.
        let before = crate::code::Code::from_string("let x = 1;\n", &crate::code::Language::Rust);
        let after = crate::code::Code::from_string("let x = 2;\n", &crate::code::Language::Rust);
        let mut mapping = build_full_identical_mapping(&before, &after);
        mapping.text_mapping = Some(HumanTextMapping {
            entries: vec![HumanTextEntry {
                operation: HumanTextOperation::Match,
                before: Some(span(0, 8, 0, 9)),
                after: Some(span(0, 8, 0, 9)),
            }],
        });

        let disagreements = text_mapping_disagreements(&mapping, &before, &after).unwrap();

        assert!(
            disagreements.is_empty(),
            "expected agreement, got {disagreements:#?}"
        );
    }

    #[test]
    fn text_mapping_disagreements_finds_text_only_one_account_calls_changed() {
        let before = crate::code::Code::from_string("let x = 1;\n", &crate::code::Language::Rust);
        let after = crate::code::Code::from_string("let x = 2;\n", &crate::code::Language::Rust);
        let mut mapping = build_full_identical_mapping(&before, &after);
        // Paint the wrong token: `x` instead of the literal that actually changed.
        mapping.text_mapping = Some(HumanTextMapping {
            entries: vec![HumanTextEntry {
                operation: HumanTextOperation::Match,
                before: Some(span(0, 4, 0, 5)),
                after: Some(span(0, 4, 0, 5)),
            }],
        });

        let disagreements = text_mapping_disagreements(&mapping, &before, &after).unwrap();

        assert!(
            !disagreements.is_empty(),
            "painting the wrong token should disagree with the tree mapping"
        );
        // Both directions show up: text the painter called changed and the tree did not, and text
        // the tree called changed and the painter did not.
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

    /// A tree mapping pairing each side's root, used by the disagreement tests above so they
    /// exercise the real `as_ast_diff_for_mapping` -> `TextDiff` projection rather than a stub.
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
            text_mapping: None,
        }
    }

    /// Same convention as `generate_mapping_site.rs`'s and `human_solver.rs`'s own `parse_rust`
    /// test helpers - a one-line stand-in for the `Parser::new`/`set_language`/`parse` sequence
    /// this module's tests would otherwise repeat by hand.
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
        // The exact shape every `human_mapping.json` had before `groups` existed - every one of
        // the 220+ files on disk right now looks like this.
        let json = r#"{"entries":[]}"#;
        let mapping: HumanMapping = serde_json::from_str(json)?;
        assert!(mapping.groups.is_empty());
        Ok(())
    }

    #[test]
    fn serializing_a_mapping_with_no_groups_omits_the_groups_key() -> Result<()> {
        // The other half of backward compatibility: an untouched fixture must re-save
        // byte-for-byte identical to before `groups` existed, not grow a `"groups": []` no-op.
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
        // The concrete proof that adding `groups` doesn't touch any of the 220+ fixtures that
        // don't use it: load a real one, re-serialize it the same way `save()` does, and diff.
        let original = fs::read_to_string(mapping_path("rust-add-if"))?;
        let mapping: HumanMapping = serde_json::from_str(&original)?;
        assert!(
            mapping.groups.is_empty(),
            "fixture assumption broken: rust-add-if unexpectedly has groups already"
        );
        let resaved = serde_json::to_string_pretty(&mapping)?;
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
            }],
            text_mapping: None,
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

    #[test]
    fn unix_diff_line_labels_marks_only_the_changed_line_on_each_side() {
        let before = crate::code::Code::from_string("a\nb\nc\n", &Language::Unknown);
        let after = crate::code::Code::from_string("a\nx\nc\n", &Language::Unknown);

        let (before_touched, after_touched) = unix_diff_line_labels(&before, &after).unwrap();

        assert_eq!(before_touched, vec![false, true, false, false]);
        assert_eq!(after_touched, vec![false, true, false, false]);
    }

    #[test]
    fn unix_diff_line_labels_marks_nothing_for_identical_files() {
        let before = crate::code::Code::from_string("a\nb\nc\n", &Language::Unknown);
        let after = crate::code::Code::from_string("a\nb\nc\n", &Language::Unknown);

        let (before_touched, after_touched) = unix_diff_line_labels(&before, &after).unwrap();

        assert!(before_touched.iter().all(|&t| !t));
        assert!(after_touched.iter().all(|&t| !t));
    }

    #[test]
    fn line_mismatches_for_is_zero_for_a_fixture_codediff_solves_exactly() -> Result<()> {
        // rust-no-change is fully identical before/after, so codediff and Unix diff both agree
        // with the (trivially all-untouched) human mapping perfectly.
        let (before, after) = crate::test::helper::handmade_test_code_pair("rust-no-change")?;

        let result = line_mismatches_for("rust-no-change", &before, &after)?;

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
        // A node with no entry at all defaults to identical (matches matched nodes' pre-existing
        // quiet/undecorated rendering when a `Caches` is built by hand rather than via
        // `rebuild_caches`).
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
        let (before, after) = crate::test::helper::handmade_test_code_pair("rust-no-change")?;

        let before_ast = before.ast.as_ref().unwrap();
        let root = before_ast.root_node();

        // Build an Identical entry for the root: since before == after, before_root and
        // after_root are the same path ("source_file:1"), and codediff should have hashed the
        // whole tree as an identical match.
        let entries = vec![HumanMappingEntry {
            operation: HumanOperation::Identical,
            before_path: Some(path_for_node(root)),
            after_path: Some(path_for_node(root)),
        }];

        let mapping = HumanMapping {
            entries,
            ..Default::default()
        };

        let diff = crate::diff::diff_code(&before, &after);
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
        let (before, after) = crate::test::helper::handmade_test_code_pair("rust-no-change")?;

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

        let diff = crate::diff::diff_code(&before, &after);
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

    /// The `block` directly inside the first (only) function in `root` - a small, reusable stand
    /// in for "the body of `fn main() { ... }`" that several multi-map tests below need.
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

    /// First node of kind `kind` found by a preorder walk from `root` (`root` itself included).
    /// Panics if there isn't one - a test-only convenience, not a general-purpose lookup.
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

    /// Walks `before`'s and `after`'s subtrees in lockstep (same shape, since both come from
    /// parsing the *same* source text) and adds an `Identical` mapping for every corresponding
    /// node pair - builds a fully self-consistent baseline `ASTDiff` for
    /// `check_subtree_maps_within` tests, which (like a real diff that's already passed
    /// `ASTDiff::is_valid`) need every single descendant, not just the top pair, to have an
    /// entry - not just the ones a test cares about.
    fn map_identical_subtrees(diff: &mut ASTDiff, before: Node, after: Node) {
        let mut stack = vec![(before, after)];
        while let Some((b, a)) = stack.pop() {
            diff.add_mapping(
                b.id(),
                a.id(),
                ASTMapping {
                    cost: 0,
                    operation: ASTMappingOperation::Identical,
                    reason: ASTMappingReason::default(),
                },
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

    /// Marks every node in `node`'s subtree (inclusive) deleted (mapped to 0) - the before-side
    /// counterpart of `map_identical_subtrees`, for building a fully self-consistent "this whole
    /// subtree is gone" baseline.
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
    fn check_group_entry_passes_for_a_real_diff_that_matches_duplicates_within_the_group()
    -> Result<()> {
        // The motivating case: three identical foo() calls before, two after - codediff (the
        // real algorithm, not a hand-rolled diff) has to pick *some* two of the three to match
        // and delete the third, and whichever two it picks should be accepted.
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
        // Both before foo()s deleted, both after foo()s inserted - each individual node's fate is
        // locally valid (deleted and inserted are both allowed fates), but the group as a whole
        // under-matched: with N == M == 2, zero pairs should be left over.
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
        };

        let mut diff = ASTDiff::default();
        diff.add_mapping(
            before_foos[0].id(),
            after_foo.id(),
            ASTMapping {
                cost: 0,
                operation: ASTMappingOperation::Identical,
                reason: ASTMappingReason::default(),
            },
        );
        // The second foo() is a group member too, but here it's mapped to qux() - a real node,
        // just not one that's part of this group - instead of being deleted.
        diff.add_mapping(
            before_foos[1].id(),
            after_qux.id(),
            ASTMapping {
                cost: 0,
                operation: ASTMappingOperation::MatchButNotIdentical,
                reason: ASTMappingReason::default(),
            },
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

        // Corrupt: bar();'s own top-level entry now claims it was deleted, even though it's still
        // sitting inside the matched block's subtree - exactly the leak
        // `check_subtree_maps_within` exists to catch.
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

        // Corrupt: f1's own "foo" identifier is left mapped to the survivor's identifier instead
        // of being deleted along with the rest of f1's subtree - a leftover member under
        // `with_children` must be *fully* swept, not just its own top node.
        let f1_ident = find_first(f1, "identifier");
        let g0_ident = find_first(g0, "identifier");
        diff.add_mapping(
            f1_ident.id(),
            g0_ident.id(),
            ASTMapping {
                cost: 0,
                operation: ASTMappingOperation::Identical,
                reason: ASTMappingReason::default(),
            },
        );

        let group = MultiMapGroup {
            before_paths: vec![path_for_node(f0), path_for_node(f1)],
            after_paths: vec![path_for_node(g0)],
            operation: HumanOperation::Identical,
            with_children: true,
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
            }],
            text_mapping: None,
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
            }],
            text_mapping: None,
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
            }],
            text_mapping: None,
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
        // 3 before foo()s, 2 after foo()s, with_children - one before-foo is a leftover delete,
        // the other two are matched. `rebuild_caches_for_mapping` should report NodeStatus for all
        // three the way a plain entry would (Matched or Marked-deleted), AND list all three (not
        // just the matched pair) in `before_group`/`after_group`.
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
            }],
            text_mapping: None,
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
            }],
            text_mapping: None,
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

    /// End-to-end sanity check for `describe_nondeterminism` itself (not just the pure
    /// comparator): identical source parsed three independent times must fully agree.
    #[test]
    fn describe_nondeterminism_is_empty_for_stable_source() {
        let report =
            describe_nondeterminism("fn f() { 1 + 1; }", "fn f() { 1 + 1; }", &Language::Rust);
        assert!(report.is_empty(), "{report:?}");
    }

    /// `node_extents` must enumerate exactly the node set `total_node_count_for` counts - they're
    /// used as numerator and denominator of the same ratio, so a disagreement would silently
    /// scale every node-mismatch rate.
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

    /// Leaves are a strict, non-empty subset of all nodes - the leaves-only denominator would be
    /// meaningless if `is_leaf` were never (or always) set.
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

    /// A `Code` with no grammar has no AST to walk - empty, not a panic, since the corpus
    /// contains extension-less fixtures (a `Makefile`, say) that reach this code path.
    #[test]
    fn node_extents_is_empty_without_an_ast() {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        std::io::Write::write_all(&mut file.as_file(), b"plain text").expect("write");
        let code = crate::code::Code::from_file(file.path()).expect("read back");
        assert!(code.ast.is_none(), "an extension-less file has no grammar");
        assert!(node_extents(&code).is_empty());
    }

    /// The touched projection's two ends: a span covering a node's text marks it, a span nowhere
    /// near it does not, and no spans at all marks nothing.
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
}
