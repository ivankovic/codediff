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
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program. If not, see <https://www.gnu.org/licenses/>.
 */
//! The App struct, the panels, and every modal the TUI can be in.
//!
//! Split out of `main.rs` along the section banner that already marked this boundary.

use crate::*;

// ---------------------------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Focus {
    Before,
    After,
}

impl Focus {
    pub(crate) fn toggle(self) -> Self {
        match self {
            Focus::Before => Focus::After,
            Focus::After => Focus::Before,
        }
    }
}

pub(crate) struct PanelState {
    pub(crate) cursor_id: usize,
    pub(crate) collapsed: std::collections::HashSet<usize>,
    pub(crate) scroll: usize,
    /// Number of rows available for list content, as of the last render (`render_panel`'s
    /// `inner_height`). Used by `action_align` to decide whether a node is currently on screen and,
    /// if not, how big a window to center it in -- 0 until the first frame has been drawn.
    pub(crate) viewport_height: usize,
}

impl PanelState {
    pub(crate) fn new(root_id: usize) -> Self {
        Self {
            cursor_id: root_id,
            collapsed: std::collections::HashSet::new(),
            scroll: 0,
            viewport_height: 0,
        }
    }
}

/// The overlay theme this session paints with, read once at startup from the same
/// `.codediff.toml` the `codediff` binary uses - so a theme picked there applies here too, and
/// Dracula (that file's own default) applies when nothing was ever picked.
///
/// A process-wide `OnceLock` rather than a field threaded through every render function, for the
/// same reason `tui::theme` keeps its custom palette process-global: `palette()` stays the single
/// resolution point and the dozen call sites below stay unchanged. human_solver has no theme
/// picker of its own, so there is nothing to change it at runtime and nothing `OnceLock` costs.
pub(crate) static OVERLAY_THEME: std::sync::OnceLock<OverlayTheme> = std::sync::OnceLock::new();

/// The colors to paint with. Falls back to the default theme (Dracula) when `main` hasn't
/// installed one - which is exactly the case in this file's own render tests, keeping them
/// deterministic instead of dependent on whatever `.codediff.toml` the machine happens to have.
pub(crate) fn overlay_palette() -> OverlayPalette {
    OVERLAY_THEME.get().copied().unwrap_or_default().palette()
}

/// Names offered first in the save-as picker, in this order.
///
/// Suggestions, not a schema. `Minimal` and `Full` are the two ends of the commonest genuine
/// ambiguity - the same edit painted as tightly as it can be, or as generously - and
/// `Only one solution` records the opposite finding, that this fixture has exactly one defensible
/// answer. Nothing requires any of them: the picker's last entry takes a free-form name, a fixture
/// may carry one painting or five, and a painting may be empty.
pub(crate) const SUGGESTED_SOLUTION_NAMES: &[&str] = &["Minimal", "Full", "Only one solution"];

/// Which painting to start on when a case is opened: its first existing one, or the first
/// suggestion when it has none.
pub(crate) fn starting_solution(mapping: &HumanMapping) -> String {
    mapping
        .text_mappings
        .first()
        .map(|named| named.name.clone())
        .unwrap_or_else(|| SUGGESTED_SOLUTION_NAMES[0].to_string())
}

/// The names offered by the save-as picker: every painting this fixture already has, then whichever
/// suggestions it doesn't. Existing names come first so the common case - resaving the painting
/// currently being edited - is at the top rather than buried under three constants.
pub(crate) fn solution_picker_names(mapping: &HumanMapping) -> Vec<String> {
    let mut names: Vec<String> = mapping
        .text_mappings
        .iter()
        .map(|named| named.name.clone())
        .collect();
    for suggestion in SUGGESTED_SOLUTION_NAMES {
        if !names.iter().any(|name| name == suggestion) {
            names.push((*suggestion).to_string());
        }
    }
    names
}

/// The entries of the painting named `solution`, or an empty slice if this fixture has no painting
/// under that name yet.
pub(crate) fn solution_entries<'a>(
    mapping: &'a HumanMapping,
    solution: &str,
) -> &'a [HumanTextEntry] {
    mapping
        .text_mappings
        .iter()
        .find(|named| named.name == solution)
        .map(|named| named.mapping.entries.as_slice())
        .unwrap_or(&[])
}

/// The entries of the painting named `solution`, creating it if this is the first range painted
/// under that name.
///
/// Creating it here is what turns "this fixture has no painting called X" into "it has one", which
/// is the distinction `text_mappings` carries in place of an `Option` - so a painting that ends up
/// empty still has to be reached deliberately, via `Z`.
pub(crate) fn solution_entries_mut<'a>(
    mapping: &'a mut HumanMapping,
    solution: &str,
) -> &'a mut Vec<HumanTextEntry> {
    if !mapping
        .text_mappings
        .iter()
        .any(|named| named.name == solution)
    {
        mapping.text_mappings.push(NamedTextMapping {
            name: solution.to_string(),
            mapping: HumanTextMapping::default(),
        });
    }
    &mut mapping
        .text_mappings
        .iter_mut()
        .find(|named| named.name == solution)
        .expect("just inserted if missing")
        .mapping
        .entries
}

/// `s`: stores the current painting under another name, **keeping the one it came from**.
///
/// This is a branch, not a rename, and the difference is the whole point: a fixture whose text
/// rendering has more than one defensible answer needs both answers on disk at once. An earlier
/// version moved the ranges to the target and dropped the source, which meant a fixture could only
/// ever hold one painting however many times you saved - the exact thing named paintings exist to
/// avoid.
///
/// `copy` decides what a *new* name starts from: the current painting's ranges (the usual case -
/// two answers to the same edit normally share most of their spans, so starting from a copy and
/// diverging is far less work than repainting), or nothing.
///
/// Choosing a name that already exists never writes: it just switches to it, exactly as `L` would.
/// Merging would leave overlapping duplicates and replacing would silently discard a painting
/// somebody made, and neither is recoverable here - there is no undo.
pub(crate) fn action_save_solution_as(app: &mut App, target: &str, copy: bool) {
    let target = target.trim();
    if target.is_empty() {
        app.status = Some("A solution needs a name".to_string());
        return;
    }
    if target == app.text_solution {
        app.status = Some(format!("Already painting under '{target}'"));
        return;
    }
    if app
        .mapping
        .text_mappings
        .iter()
        .any(|named| named.name == target)
    {
        action_load_solution(app, target);
        app.status = Some(format!(
            "'{target}' already exists - switched to it, nothing was overwritten"
        ));
        return;
    }

    let entries = if copy {
        solution_entries(&app.mapping, &app.text_solution).to_vec()
    } else {
        Vec::new()
    };
    let count = entries.len();
    app.mapping.text_mappings.push(NamedTextMapping {
        name: target.to_string(),
        mapping: HumanTextMapping { entries },
    });
    app.text_solution = target.to_string();
    app.dirty = true;
    app.status = Some(if copy {
        format!(
            "Started '{target}' as a copy of the previous painting ({count} range(s)) - {} now on file",
            app.mapping.text_mappings.len()
        )
    } else {
        format!(
            "Started '{target}' empty - {} painting(s) now on file",
            app.mapping.text_mappings.len()
        )
    });
}

/// Deletes the painting named `target`, and picks something sensible to edit next.
///
/// The way out of a fixture painted `Only one solution` that turns out to need `Minimal` and
/// `Full` after all: the single painting has to go, or it sits there claiming the rendering is
/// unambiguous while two paintings next to it say it is not.
///
/// Deleting the last painting leaves the fixture *unpainted* - `text_mappings` empty, which is a
/// different state from a painting with no ranges in it (see `HumanMapping::text_mappings`). That
/// is the honest outcome and the status line says so, because it is also what the `X` open-picker
/// filter and `diffs.csv` will report from then on.
pub(crate) fn action_delete_solution(app: &mut App, target: &str) {
    let before = app.mapping.text_mappings.len();
    app.mapping
        .text_mappings
        .retain(|named| named.name != target);
    if app.mapping.text_mappings.len() == before {
        app.status = Some(format!("No painting called '{target}'"));
        return;
    }
    app.dirty = true;

    // Only the painting being edited forces a move. Deleting some *other* one must leave the
    // reader where they were - being yanked into a different painting because a third was thrown
    // away is how you paint ranges into the wrong one without noticing.
    let was_editing = app.text_solution == target;
    if was_editing {
        app.text_solution = starting_solution(&app.mapping);
    }
    app.status = Some(if app.mapping.text_mappings.is_empty() {
        format!("Deleted '{target}' - this fixture is now unpainted (save with s)")
    } else if was_editing {
        format!(
            "Deleted '{target}' - now editing '{}' ({} painting(s) left)",
            app.text_solution,
            app.mapping.text_mappings.len()
        )
    } else {
        format!(
            "Deleted '{target}' - still editing '{}' ({} painting(s) left)",
            app.text_solution,
            app.mapping.text_mappings.len()
        )
    });
}

/// Switches which painting the text view is editing, without touching any of them.
pub(crate) fn action_load_solution(app: &mut App, target: &str) {
    app.text_solution = target.to_string();
    let count = solution_entries(&app.mapping, target).len();
    app.status = Some(format!("Editing '{target}' ({count} range(s))"));
}

/// What the `t` view is painting on screen. Cycled by `p`, the same key that runs codediff's diff
/// in the tree view.
///
/// The point of the third mode is that neither of the first two answers the question a person
/// painting ground truth actually has, which is "where do we differ". Flipping between two full
/// renderings and spotting the difference by eye is exactly the job `text_mapping_disagreements`
/// already does per byte.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum TextOverlay {
    /// The human's own painting for the current solution.
    #[default]
    Human,
    /// What codediff's own diff renders, through the same `TextDiff` projection the TUI and the
    /// mapping site use - not a re-derivation, the real thing.
    CodeDiff,
    /// Only the bytes where the two disagree, painted with the *human's* label so the colour still
    /// says what the human claimed. Empty means they agree everywhere.
    Disagreements,
    /// Only the bytes where the human's *painting* and their own *tree mapping* disagree with each
    /// other (see `tree_mapping_text_spans`/`diff_case_disagreement_bytes`) - the same
    /// human-vs-human comparison `text_mapping_disagreements` makes, with `diff_code`'s own
    /// algorithm never in the loop, unlike `Disagreements` above (which compares the painting
    /// against codediff's real output). Lets a fixture flagged by the `o` picker's `s`/`Y` sort and
    /// filter be inspected directly: where, specifically, do the two ground truths disagree.
    TreeDisagreement,
}

impl TextOverlay {
    pub(crate) fn next(self) -> Self {
        match self {
            TextOverlay::Human => TextOverlay::CodeDiff,
            TextOverlay::CodeDiff => TextOverlay::Disagreements,
            TextOverlay::Disagreements => TextOverlay::TreeDisagreement,
            TextOverlay::TreeDisagreement => TextOverlay::Human,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            TextOverlay::Human => "human",
            TextOverlay::CodeDiff => "codediff",
            TextOverlay::Disagreements => "disagreements",
            TextOverlay::TreeDisagreement => "tree vs painting",
        }
    }
}

/// codediff's own text ranges for this case, as painting spans - one list per side.
///
/// Goes through `TextDiff::from`, the same projection the real TUI renders and the mapping site
/// draws, so what shows here is what codediff actually produces rather than a second
/// interpretation of its node mapping that could drift from it.
pub(crate) fn codediff_text_spans(
    before: &Code,
    after: &Code,
) -> [Vec<(HumanTextSpan, HumanTextVerdict)>; 2] {
    let diff = diff_code(before, after);
    let Some(ast_diff) = diff.ast.as_ref() else {
        return [Vec::new(), Vec::new()];
    };
    let node_cache = NodeCache::build(before, after);
    let text_diff = TextDiff::from(before, after, ast_diff, &node_cache);

    let convert = |ranges: Vec<codediff::diff::text::RangeMatch>| {
        ranges
            .into_iter()
            .filter(|range_match| !range_match.source.is_empty())
            .filter_map(|range_match| {
                let verdict = match range_match.operation {
                    codediff::diff::text::TextOperation::Move => HumanTextVerdict::Move,
                    codediff::diff::text::TextOperation::Update => HumanTextVerdict::Update,
                    codediff::diff::text::TextOperation::Delete => HumanTextVerdict::Delete,
                    codediff::diff::text::TextOperation::Insert => HumanTextVerdict::Insert,
                    // Identical text is the unpainted background on both sides of this comparison.
                    _ => return None,
                };
                Some((
                    HumanTextSpan {
                        start_row: range_match.source.start_row,
                        start_column: range_match.source.start_column,
                        end_row: range_match.source.end_row,
                        end_column: range_match.source.end_column,
                    },
                    verdict,
                ))
            })
            .collect()
    };
    [convert(text_diff.all(0)), convert(text_diff.all(1))]
}

/// codediff's own rendering of this pair as human painting *entries* - what `P` copies into an
/// empty painting so a fixture can be corrected from a draft rather than painted from nothing.
///
/// Built from `codediff_text_spans`, so what this writes is exactly what the `p` overlay draws -
/// seeding then showing "codediff's own diff" must not reveal a difference.
///
/// Move and Update both become `Match`: `HumanTextOperation` records only Match/Delete/Insert, and
/// whether a match reads as a move or an update is derived from whether the two spans' text is
/// identical (see `HumanTextEntry::verdict`) rather than stored. So this cannot get that call
/// wrong - there is no call to make.
pub(crate) fn codediff_text_entries(
    before: &Code,
    after: &Code,
) -> Result<Vec<HumanTextEntry>, &'static str> {
    let [before_spans, after_spans] = codediff_text_spans(before, after);

    // A painting cannot represent two ranges claiming the same byte, and nothing downstream
    // agrees on what it would mean: `render_paint_side` resolves an overlap per byte by
    // `PaintClass`'s `max` (so the highest-ranked *verdict* wins), while `label_bytes` - what
    // `compare_painting` grades against - fills its array in list order, so the *last entry* wins.
    // A seeded overlap would therefore render as one thing and score as another, silently, in
    // ground truth a human has no reason to re-check. codediff's own rendering does produce
    // overlapping spans - 22 of the 513 corpus fixtures, across six languages - so this is a real
    // case, not a defensive one.
    if spans_overlap(&before_spans) || spans_overlap(&after_spans) {
        return Err(
            "codediff's own ranges overlap on this pair, which a painting cannot represent",
        );
    }

    // The two sides' matched spans are two views of the same decisions, walked in the same order,
    // so the k-th match on one side is the k-th on the other. `TextDiff` keeps `before_ranges` and
    // `after_ranges` as two independent lists with no cross-reference - `RangeMatch::destination`
    // is only meaningful for `Identical`, and is zero on exactly the changed ranges this needs -
    // so order is the only pairing available. If the two counts ever disagree, the shape is one
    // this cannot pair, and it gives up rather than inventing a pairing: a wrong `Match` is worse
    // than no seed, for the same "nobody will re-check it" reason as the overlap case above.
    let matched = |spans: &[(HumanTextSpan, HumanTextVerdict)]| -> Vec<HumanTextSpan> {
        spans
            .iter()
            .filter(|(_, verdict)| {
                matches!(verdict, HumanTextVerdict::Move | HumanTextVerdict::Update)
            })
            .map(|(span, _)| *span)
            .collect()
    };
    let before_matched = matched(&before_spans);
    let after_matched = matched(&after_spans);
    if before_matched.len() != after_matched.len() {
        return Err("codediff's two sides do not pair up here, so a match cannot be derived");
    }

    let mut entries: Vec<HumanTextEntry> = before_matched
        .into_iter()
        .zip(after_matched)
        .map(|(before_span, after_span)| HumanTextEntry {
            operation: HumanTextOperation::Match,
            before: vec![before_span],
            after: vec![after_span],
        })
        .collect();

    // A deletion only exists on the before side and an insertion only on the after side, so
    // neither needs pairing.
    entries.extend(
        before_spans
            .iter()
            .filter(|(_, verdict)| *verdict == HumanTextVerdict::Delete)
            .map(|(span, _)| HumanTextEntry {
                operation: HumanTextOperation::Delete,
                before: vec![*span],
                after: Vec::new(),
            }),
    );
    entries.extend(
        after_spans
            .iter()
            .filter(|(_, verdict)| *verdict == HumanTextVerdict::Insert)
            .map(|(span, _)| HumanTextEntry {
                operation: HumanTextOperation::Insert,
                before: Vec::new(),
                after: vec![*span],
            }),
    );

    Ok(entries)
}

/// Whether any two of `spans` claim a common byte. Quadratic, but it runs once per `P` on one
/// file's worth of ranges, not per frame.
pub(crate) fn spans_overlap(spans: &[(HumanTextSpan, HumanTextVerdict)]) -> bool {
    let starts_before = |a: &HumanTextSpan, b: &HumanTextSpan| {
        (a.start_row, a.start_column) < (b.end_row, b.end_column)
    };
    spans.iter().enumerate().any(|(i, (a, _))| {
        spans[i + 1..]
            .iter()
            .any(|(b, _)| starts_before(a, b) && starts_before(b, a))
    })
}

/// `P` in the text view: copy codediff's own rendering of this pair into the current painting as a
/// starting point, so a fixture is corrected rather than painted from a blank page.
///
/// Refuses outright on a painting that already has ranges, exactly as `action_paint_mark_empty`
/// does and for the same reason: this replaces the whole painting, and silently overwriting
/// hand-painted work would be unrecoverable in a tool with no undo. `s` branches the current
/// painting to a new name, which is the way to seed a second reading of the same pair.
pub(crate) fn action_paint_seed_from_codediff(app: &mut App, before: &Code, after: &Code) {
    let solution = app.text_solution.clone();
    if !solution_entries(&app.mapping, &solution).is_empty() {
        app.status = Some(format!(
            "'{solution}' already has painted ranges - P only seeds an empty painting (s branches \
             this one to a new name)"
        ));
        return;
    }

    let entries = match codediff_text_entries(before, after) {
        Ok(entries) if entries.is_empty() => {
            app.status = Some("codediff paints nothing on this pair - nothing to copy".to_string());
            return;
        }
        Ok(entries) => entries,
        Err(reason) => {
            app.status = Some(format!(
                "Cannot seed from codediff: {reason} - paint by hand"
            ));
            return;
        }
    };

    let count = entries.len();
    *solution_entries_mut(&mut app.mapping, &solution) = entries;
    app.dirty = true;
    app.status = Some(format!(
        "Copied codediff's {count} range(s) into '{solution}' - correct them from here (u removes one)"
    ));
}

/// The human's own *tree* mapping (`HumanMapping::entries`, never `diff_code`'s output) for this
/// case, as painting spans - the tree-mapping counterpart of `codediff_text_spans`, and what backs
/// `TextOverlay::TreeDisagreement`.
///
/// Goes through the same `as_ast_diff_for_mapping` + `TextDiff::from` pipeline
/// `text_mapping_disagreements` uses for its own tree side (see that function's doc comment for
/// why this - not `diff_code` - is the only input that keeps a tree-vs-painting comparison pure
/// human-vs-human). `None` mapping load failures collapse to an empty pair of spans, same as
/// `codediff_text_spans` does when `diff_code` produces no AST.
pub(crate) fn tree_mapping_text_spans(
    mapping: &HumanMapping,
    before: &Code,
    after: &Code,
) -> [Vec<(HumanTextSpan, HumanTextVerdict)>; 2] {
    let Ok(ast_diff) = human_mapping::as_ast_diff_for_mapping(mapping, before, after) else {
        return [Vec::new(), Vec::new()];
    };
    let node_cache = NodeCache::build(before, after);
    let text_diff = TextDiff::from(before, after, &ast_diff, &node_cache);

    let convert = |ranges: Vec<codediff::diff::text::RangeMatch>| {
        ranges
            .into_iter()
            .filter(|range_match| !range_match.source.is_empty())
            .filter_map(|range_match| {
                let verdict = match range_match.operation {
                    codediff::diff::text::TextOperation::Move => HumanTextVerdict::Move,
                    codediff::diff::text::TextOperation::Update => HumanTextVerdict::Update,
                    codediff::diff::text::TextOperation::Delete => HumanTextVerdict::Delete,
                    codediff::diff::text::TextOperation::Insert => HumanTextVerdict::Insert,
                    _ => return None,
                };
                Some((
                    HumanTextSpan {
                        start_row: range_match.source.start_row,
                        start_column: range_match.source.start_column,
                        end_row: range_match.source.end_row,
                        end_column: range_match.source.end_column,
                    },
                    verdict,
                ))
            })
            .collect()
    };
    [convert(text_diff.all(0)), convert(text_diff.all(1))]
}

/// The spans where the human's painting and some other span source disagree, labelled with the
/// human's verdict. Generic over what `other` actually is - `TextOverlay::Disagreements` passes
/// `codediff_text_spans` (the painting against codediff's own rendering), `TextOverlay::
/// TreeDisagreement` passes `tree_mapping_text_spans` (the painting against the human's own tree
/// mapping - the same comparison `text_mapping_disagreements` makes, computed independently here
/// at row/column granularity rather than reusing that byte-offset-based function, since this is a
/// different call site with its own existing row-by-row span machinery already in hand).
pub(crate) fn overlay_disagreement_spans(
    painted: &[Vec<(HumanTextSpan, HumanTextVerdict)>; 2],
    other: &[Vec<(HumanTextSpan, HumanTextVerdict)>; 2],
    before_src: &str,
    after_src: &str,
) -> [Vec<(HumanTextSpan, HumanTextVerdict)>; 2] {
    let mut out = [Vec::new(), Vec::new()];
    for (side, source) in [(0usize, before_src), (1usize, after_src)] {
        let lines: Vec<&str> = source.split('\n').collect();
        for (row, line) in lines.iter().enumerate() {
            // One pass per row, coalescing adjacent disagreeing columns into a single span so a
            // whole differing line shows as one range rather than eighty.
            let mut run_start: Option<usize> = None;
            let mut run_verdict = HumanTextVerdict::Update;
            let push = |start: usize, end: usize, verdict, out: &mut Vec<_>| {
                out.push((
                    HumanTextSpan {
                        start_row: row,
                        start_column: start,
                        end_row: row,
                        end_column: end,
                    },
                    verdict,
                ));
            };
            for (column, _) in line.char_indices() {
                let human = verdict_at(&painted[side], row, column, line.len());
                let theirs = verdict_at(&other[side], row, column, line.len());
                if human == theirs {
                    if let Some(start) = run_start.take() {
                        push(start, column, run_verdict, &mut out[side]);
                    }
                    continue;
                }
                // Colour by whichever side has an opinion, preferring the human's - the reader is
                // checking their own work, so "what I said" is the more useful signal.
                let verdict = human.or(theirs).unwrap_or(HumanTextVerdict::Update);
                match run_start {
                    Some(_) if run_verdict == verdict => {}
                    Some(start) => {
                        push(start, column, run_verdict, &mut out[side]);
                        run_start = Some(column);
                        run_verdict = verdict;
                    }
                    None => {
                        run_start = Some(column);
                        run_verdict = verdict;
                    }
                }
            }
            if let Some(start) = run_start {
                push(start, line.len(), run_verdict, &mut out[side]);
            }
        }
    }
    out
}

/// The verdict covering `(row, column)`, or `None` where nothing does.
pub(crate) fn verdict_at(
    spans: &[(HumanTextSpan, HumanTextVerdict)],
    row: usize,
    column: usize,
    row_len: usize,
) -> Option<HumanTextVerdict> {
    spans
        .iter()
        .find(|(span, _)| span_covers(*span, row, column, row_len))
        .map(|(_, verdict)| *verdict)
}

/// Cursor, selection and scroll for the `t` text-painting view, one set per side.
///
/// Both sides carry a live cursor and an independent selection at all times, mirroring the AST
/// panels: that is what lets `m` pair the two current selections in one keystroke instead of
/// needing a pending-selection handshake, exactly as the tree's own `m` pairs the two panel
/// cursors.
///
/// Columns are **byte** offsets into a row, matching `HumanTextSpan` and `TextRange`, so a painted
/// span needs no conversion on the way out. Cursor movement steps by *characters* even so - see
/// [`TextPaintState::step_column`] - since landing mid-character would produce a span that
/// `span_text` correctly refuses to read back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextPaintState {
    /// 0 = before, 1 = after. Same side convention as `TextDiff::all`.
    pub(crate) side: usize,
    /// `(row, byte column)` per side.
    pub(crate) cursor: [(usize, usize); 2],
    /// Where a selection was started with `v`, per side; `None` when nothing is selected.
    pub(crate) anchor: [Option<(usize, usize)>; 2],
    /// The digits typed so far at the `:` line prompt, if it is open.
    ///
    /// Kept on the paint state rather than raised as its own modal: the text view *is* a modal,
    /// and nesting one inside it to read a number would mean carrying the whole painting state
    /// through the nested modal and back. A file worth jumping around in is exactly one too big to
    /// reach with `j`, which is the case this exists for.
    pub(crate) line_prompt: Option<String>,
    /// Ranges banked with `x`, per side, waiting to be committed by `d`/`i`/`m`.
    ///
    /// This is what makes an N:M match possible: one live selection can only ever describe one
    /// range, so several occurrences on a side have to accumulate somewhere first. Named and keyed
    /// to match the tree panels' own multi-map selection (`x` to bank, `c` to clear), since it is
    /// the same idea one granularity down.
    pub(crate) pending: [Vec<HumanTextSpan>; 2],
    /// Top visible row per side. Independent, unlike the read-only view this replaced: painting a
    /// move means looking at two places that are nowhere near each other.
    pub(crate) scroll: [usize; 2],
    /// How a selection spanning several rows reads, toggled with `V`.
    ///
    /// `true` (the default): vertical - one span per row, all sharing the anchor-to-cursor column
    /// range, for picking the same columns down a stack of lines. `false`: the old full-line
    /// sweep, still needed for a single contiguous multi-line block - e.g. selecting a whole
    /// moved function body, where `m` requires every span on a side to read identical text and a
    /// per-row decomposition of one block would fail that check on every row but the first.
    pub(crate) vertical: bool,
}

impl Default for TextPaintState {
    fn default() -> Self {
        Self {
            side: 0,
            cursor: [(0, 0); 2],
            anchor: [None; 2],
            line_prompt: None,
            pending: [Vec::new(), Vec::new()],
            scroll: [0; 2],
            vertical: true,
        }
    }
}

impl TextPaintState {
    /// The row's text, or `""` past the end of the file.
    pub(crate) fn row_text(source: &str, row: usize) -> &str {
        source.split('\n').nth(row).unwrap_or("")
    }

    pub(crate) fn row_count(source: &str) -> usize {
        source.split('\n').count()
    }

    /// Moves the cursor `delta` rows, clamping the column to the new row and to a character
    /// boundary.
    pub(crate) fn step_row(&mut self, delta: isize, source: &str) {
        let (row, column) = self.cursor[self.side];
        let last = Self::row_count(source).saturating_sub(1);
        let row = if delta < 0 {
            row.saturating_sub(delta.unsigned_abs())
        } else {
            (row + delta as usize).min(last)
        };
        let line = Self::row_text(source, row);
        let column = column.min(line.len());
        // Clamping can land mid-character on a row whose earlier bytes are multi-byte; walk back
        // to the nearest boundary rather than storing a column no span can be read from.
        let column = (0..=column)
            .rev()
            .find(|&c| line.is_char_boundary(c))
            .unwrap_or(0);
        self.cursor[self.side] = (row, column);
    }

    /// Moves the cursor one *character* left or right, wrapping across rows at the ends.
    pub(crate) fn step_column(&mut self, forward: bool, source: &str) {
        let (row, column) = self.cursor[self.side];
        let line = Self::row_text(source, row);
        if forward {
            match line[column..].chars().next() {
                Some(ch) => self.cursor[self.side] = (row, column + ch.len_utf8()),
                None if row + 1 < Self::row_count(source) => self.cursor[self.side] = (row + 1, 0),
                None => {}
            }
        } else if column > 0 {
            let previous = line[..column]
                .char_indices()
                .next_back()
                .map(|(index, _)| index)
                .unwrap_or(0);
            self.cursor[self.side] = (row, previous);
        } else if row > 0 {
            let previous_row = row - 1;
            self.cursor[self.side] = (previous_row, Self::row_text(source, previous_row).len());
        }
    }

    /// The live selection on `side`: one span per row it touches, empty if nothing is selected.
    ///
    /// `self.vertical` (toggled with `V`) picks the shape: vertical is a stack of squares - each
    /// row in range gets its own span sharing the anchor-to-cursor column range, clamped to that
    /// row's own length - for picking the same columns down several lines without also grabbing
    /// the untouched tail of every line in between. Non-vertical is the older full-line sweep -
    /// one span running from the anchor straight through to the cursor, covering every line in
    /// between end to end - which a contiguous multi-line block (an entire moved function body)
    /// still needs: `m` requires every span on a side to read identical text, so decomposing one
    /// block into per-row squares would fail that check on every row but the first. A same-row
    /// selection reads the same either way: with one row in range both shapes collapse to the
    /// single span every earlier version of this method returned.
    ///
    /// Each span's end is exclusive and includes the character *under* whichever endpoint sits
    /// further right, which is what a reader painting a range sees highlighted - a selection that
    /// stopped one character short of its own cursor would be a surprise every time. In vertical
    /// mode, a row shorter than the left column contributes nothing: there is no character there
    /// to select.
    pub(crate) fn selection(&self, side: usize, source: &str) -> Vec<HumanTextSpan> {
        let Some(anchor) = self.anchor[side] else {
            return Vec::new();
        };
        let cursor = self.cursor[side];
        let (row_start, row_end) = if anchor.0 <= cursor.0 {
            (anchor.0, cursor.0)
        } else {
            (cursor.0, anchor.0)
        };

        if !self.vertical {
            let (start, end) = if anchor <= cursor {
                (anchor, cursor)
            } else {
                (cursor, anchor)
            };
            let end_line = Self::row_text(source, end.0);
            let end_column = match end_line[end.1.min(end_line.len())..].chars().next() {
                Some(ch) => end.1 + ch.len_utf8(),
                // The cursor sits past the last character of its row: extend through the newline,
                // so selecting to end-of-line and pressing `d` deletes the line rather than all
                // but its break.
                None => end.1,
            };
            let span = HumanTextSpan {
                start_row: start.0,
                start_column: start.1,
                end_row: end.0,
                end_column,
            };
            return (!span.is_empty()).then_some(span).into_iter().collect();
        }

        let (col_left, col_right) = if anchor.1 <= cursor.1 {
            (anchor.1, cursor.1)
        } else {
            (cursor.1, anchor.1)
        };
        // Rounds `col` down to the nearest character boundary on `line` - the same clamp
        // `step_row` applies, needed here because a column valid on the row it was set on can
        // land mid-character once reused against a different row's different byte content.
        let clamp_boundary = |line: &str, col: usize| -> usize {
            let col = col.min(line.len());
            (0..=col)
                .rev()
                .find(|&c| line.is_char_boundary(c))
                .unwrap_or(0)
        };

        (row_start..=row_end)
            .filter_map(|row| {
                let line = Self::row_text(source, row);
                let start_column = clamp_boundary(line, col_left);
                let right = clamp_boundary(line, col_right);
                let end_column = match line[right..].chars().next() {
                    Some(ch) => right + ch.len_utf8(),
                    // Past the last character of this row: extend through the newline, so
                    // selecting to end-of-line and pressing `d` deletes the line rather than all
                    // but its break.
                    None => right,
                };
                let span = HumanTextSpan {
                    start_row: row,
                    start_column,
                    end_row: row,
                    end_column,
                };
                (!span.is_empty()).then_some(span)
            })
            .collect()
    }

    /// Every range `side` would commit right now: whatever `x` has banked, plus the live selection.
    ///
    /// Banked-plus-live rather than either alone, so `v`-select-`x`-select-`m` works without a
    /// final `x` - forgetting to bank the last range before committing would otherwise silently
    /// drop it, which is exactly the kind of loss this view has no undo for.
    pub(crate) fn committable(&self, side: usize, source: &str) -> Vec<HumanTextSpan> {
        let mut spans = self.pending[side].clone();
        spans.extend(self.selection(side, source));
        spans
    }

    /// Keeps the cursor's row inside a `height`-row viewport.
    pub(crate) fn scroll_into_view(&mut self, height: usize) {
        let row = self.cursor[self.side].0;
        let top = &mut self.scroll[self.side];
        if row < *top {
            *top = row;
        } else if height > 0 && row >= *top + height {
            *top = row + 1 - height;
        }
    }
}

/// `m`: pairs everything selected on the before side with everything selected on the after side,
/// as one `Match`.
///
/// Needs ranges on *both* sides, deliberately: that mirrors the tree's own `m`, which pairs the
/// two panel cursors in one keystroke rather than making the human hold a pending selection in
/// their head across a panel switch.
///
/// N:M falls out of the same key. Bank extra ranges with `x` and they all go into one entry - three
/// occurrences of a token before against two after is a single correspondence, not five. What that
/// entry is worth is then checked immediately rather than at save time: `verdict` refuses a group
/// whose spans disagree within a side, and refusing at the keystroke is the only point where the
/// human still has the selection in front of them to fix.
/// The first span of `entry` that claims a byte some already-painted range in `solution` claims,
/// as `(side, description)` for a status line.
///
/// Overlapping painted ranges are not representable: `render_paint_side` resolves an overlap per
/// byte by `PaintClass`'s `max`, so the highest-ranked *verdict* wins, while `label_bytes` - what
/// `compare_painting` grades against - fills its array in list order, so the *last entry* wins.
/// A painting with one therefore looks like one thing on screen and scores as another, with no
/// diagnostic either side. Both those readers say in their own comments that the solver never
/// produces overlaps; until now nothing enforced it, and a second `v`-`d` over already-painted
/// text was accepted in silence.
///
/// Refused at the keystroke rather than at save time, for the same reason `action_paint_match`
/// resolves its verdict now: this is the one moment the selection is still on screen and the
/// human can see what they just asked for.
fn overlapping_painted_range(
    mapping: &HumanMapping,
    solution: &str,
    entry: &HumanTextEntry,
    before_src: &str,
    after_src: &str,
) -> Option<String> {
    let sides = [
        (0usize, "Before", &entry.before, before_src),
        (1usize, "After", &entry.after, after_src),
    ];
    for (side, panel, spans, source) in sides {
        let painted = painted_spans(mapping, solution, side, before_src, after_src);
        for span in spans {
            for (existing, verdict) in &painted {
                if !spans_share_a_byte(*span, *existing, source) {
                    continue;
                }
                return Some(format!(
                    "{panel} row {} already has a painted {verdict:?} range at columns {}-{}",
                    existing.start_row + 1,
                    existing.start_column,
                    existing.end_column
                ));
            }
        }
    }
    None
}

/// Whether two spans on one side claim a common byte, compared as absolute offsets so the two
/// encodings of a row boundary - `(row + 1, 0)` and `(row, row_len)` - cannot read as an overlap.
/// A shared line terminator does not count: `label_bytes` never labels one, so two ranges meeting
/// at a newline disagree about nothing.
fn spans_share_a_byte(a: HumanTextSpan, b: HumanTextSpan, source: &str) -> bool {
    let text = codediff::diff::text_range::SourceText::new(source);
    let offset = |row: usize, column: usize| -> Option<usize> {
        text.byte_index(
            codediff::diff::text_range::SourceRow::from_raw(row),
            codediff::diff::text_range::SourceColumn::from_raw(column),
        )
        .map(codediff::diff::text_range::SourceOffset::get)
    };
    let (Some(a_start), Some(a_end)) = (
        offset(a.start_row, a.start_column),
        offset(a.end_row, a.end_column),
    ) else {
        return false;
    };
    let (Some(b_start), Some(b_end)) = (
        offset(b.start_row, b.start_column),
        offset(b.end_row, b.end_column),
    ) else {
        return false;
    };
    let lo = a_start.max(b_start);
    let hi = a_end.min(b_end);
    lo < hi && source[lo..hi].chars().any(|c| c != '\n')
}

pub(crate) fn action_paint_match(
    app: &mut App,
    state: &mut TextPaintState,
    before_src: &str,
    after_src: &str,
) {
    let before = state.committable(0, before_src);
    let after = state.committable(1, after_src);
    if before.is_empty() || after.is_empty() {
        app.status =
            Some("Match needs a selection on both sides - press v on each, then m".to_string());
        return;
    }

    let entry = HumanTextEntry {
        operation: HumanTextOperation::Match,
        before,
        after,
    };
    // Resolved now, not at save time: the human is told what they just asserted, and a group whose
    // spans don't hold up is rejected while the selection is still on screen.
    let verdict = match entry.verdict(before_src, after_src) {
        Ok(verdict) => verdict,
        Err(err) => {
            app.status = Some(format!("Not matched: {err:#}"));
            return;
        }
    };

    let shape = format!("{}:{}", entry.before.len(), entry.after.len());
    let solution = app.text_solution.clone();
    if let Some(clash) =
        overlapping_painted_range(&app.mapping, &solution, &entry, before_src, after_src)
    {
        app.status = Some(format!("Not matched: {clash} - u removes it first"));
        return;
    }
    solution_entries_mut(&mut app.mapping, &solution).push(entry);
    app.dirty = true;
    state.anchor = [None; 2];
    state.pending = [Vec::new(), Vec::new()];
    app.status = Some(match verdict {
        HumanTextVerdict::Move => {
            format!("Matched {shape}: identical text, recorded as a move")
        }
        HumanTextVerdict::Update => {
            format!("Matched {shape}: text differs, recorded as an update")
        }
        other => format!("Matched {shape} ({other:?})"),
    });
}

/// `d` / `i`: paints everything selected on the focused side as a one-sided removal or addition.
///
/// Takes banked ranges too, so the same token removed in several places is one decision rather
/// than one per occurrence - but unlike a match, these carry no identity constraint: with nothing
/// to pair against, spans that read differently assert nothing unsound.
pub(crate) fn action_paint_one_sided(
    app: &mut App,
    state: &mut TextPaintState,
    operation: HumanTextOperation,
    before_src: &str,
    after_src: &str,
) {
    let side = match operation {
        HumanTextOperation::Delete => 0,
        HumanTextOperation::Insert => 1,
        HumanTextOperation::Match => return,
    };
    let source = if side == 0 { before_src } else { after_src };
    let spans = state.committable(side, source);
    if spans.is_empty() {
        let (key, what, panel) = match operation {
            HumanTextOperation::Delete => ("d", "delete", "Before"),
            _ => ("i", "insert", "After"),
        };
        app.status = Some(format!(
            "Nothing selected on the {panel} side - press v there, move, then {key} to {what}"
        ));
        return;
    }

    let count = spans.len();
    let entry = if side == 0 {
        HumanTextEntry {
            operation,
            before: spans,
            after: Vec::new(),
        }
    } else {
        HumanTextEntry {
            operation,
            before: Vec::new(),
            after: spans,
        }
    };
    if let Err(err) = entry.verdict(before_src, after_src) {
        app.status = Some(format!("Not painted: {err:#}"));
        return;
    }

    let solution = app.text_solution.clone();
    if let Some(clash) =
        overlapping_painted_range(&app.mapping, &solution, &entry, before_src, after_src)
    {
        app.status = Some(format!("Not painted: {clash} - u removes it first"));
        return;
    }
    solution_entries_mut(&mut app.mapping, &solution).push(entry);
    app.dirty = true;
    state.anchor[side] = None;
    state.pending[side].clear();
    app.status = Some(match operation {
        HumanTextOperation::Delete => format!("Painted {count} deletion(s)"),
        _ => format!("Painted {count} insertion(s)"),
    });
}

/// `u`: removes whichever painted entry covers the focused cursor.
///
/// Removes the *whole entry*, both sides of a `Match` included. A half-removed match would be a
/// malformed entry, which `HumanTextEntry::verdict` rightly refuses to read - so the alternative
/// to removing both is not a smaller edit, it is a broken file.
pub(crate) fn action_paint_unmark(
    app: &mut App,
    state: &TextPaintState,
    before_src: &str,
    after_src: &str,
) {
    let side = state.side;
    let source = if side == 0 { before_src } else { after_src };
    let (row, column) = state.cursor[side];
    let row_len = TextPaintState::row_text(source, row).len();

    let solution = app.text_solution.clone();
    let entries = solution_entries_mut(&mut app.mapping, &solution);
    let before_count = entries.len();
    entries.retain(|entry| {
        let spans = if side == 0 {
            &entry.before
        } else {
            &entry.after
        };
        !spans
            .iter()
            .any(|span| span_covers(*span, row, column, row_len))
    });
    let removed = before_count - entries.len();
    if removed == 0 {
        app.status = Some("Nothing painted here".to_string());
        return;
    }
    app.dirty = true;
    app.status = Some(format!("Removed {removed} painted range(s)"));
}

/// `Z`: marks this fixture's painting as complete even though nothing was painted.
///
/// Only reachable when there is genuinely nothing to paint - two identical files. Without it that
/// case is indistinguishable from an unvisited fixture, since both would leave `text_mapping` at
/// `None`, and a completeness count would quietly under-report forever.
/// `!`: throw away everything recorded for this case and start over.
///
/// Clears all three grounds truth at once - `entries`, `groups` and every named painting in
/// `text_mappings` - because "start fresh" means the case as a whole, and clearing the tree
/// mapping while leaving paintings behind would leave a fixture asserting things about a mapping
/// that no longer exists.
///
/// Only the in-memory case: nothing is written until `s`, so a reset entered by mistake is undone
/// by reopening the case without saving. `dirty` is set so the usual unsaved-changes guard on
/// switching cases still fires.
pub(crate) fn action_reset_case(app: &mut App) -> String {
    let entries = app.mapping.entries.len();
    let groups = app.mapping.groups.len();
    let paintings = app.mapping.text_mappings.len();

    app.mapping.entries.clear();
    app.mapping.groups.clear();
    app.mapping.text_mappings.clear();

    // Everything derived from the mapping goes with it, exactly as when a different case is
    // opened: a stale `tree_text_spans` would keep drawing the mapping that was just discarded,
    // and `text_solution` would name a painting that no longer exists.
    app.tree_text_spans = None;
    app.text_solution = starting_solution(&app.mapping);
    app.before_multi_select.clear();
    app.after_multi_select.clear();
    app.dirty = true;

    format!(
        "Reset: cleared {entries} mapping entries, {groups} groups and {paintings} paintings - \
         nothing written until s"
    )
}

pub(crate) fn action_paint_mark_empty(app: &mut App) {
    let solution = app.text_solution.clone();
    if !solution_entries(&app.mapping, &solution).is_empty() {
        app.status = Some(format!(
            "'{solution}' already has painted ranges - u removes them one at a time"
        ));
        return;
    }
    solution_entries_mut(&mut app.mapping, &solution);
    app.dirty = true;
    app.status = Some(format!("Marked '{solution}' as painted with no changes"));
}

/// A blocking prompt raised by `m`/`M` that needs a direct human answer before the mapping entry
/// can be finalized. While `App::modal` is `Some`, the event loop routes keys to
/// `handle_modal_key` instead of the normal keybindings.
#[derive(Debug, Clone)]
pub(crate) enum Modal {
    /// The before and after cursor nodes have different kinds. Shown before adding any mapping
    /// for them, since codediff itself never maps nodes of different kinds together (see
    /// `ASTDiff::is_valid`), so confirming this will always show up as a mismatch against
    /// codediff's actual diff -- which is fine for exploration, but worth confirming explicitly.
    ConfirmKindMismatch {
        before_id: usize,
        after_id: usize,
        before_kind: String,
        after_kind: String,
        /// Whether this originated from `M` (recursive), in which case confirming also
        /// auto-matches the rest of the subtree.
        recursive: bool,
    },
    /// Raised by `m`/`M` when a multi-map selection (see `App::before_multi_select`/
    /// `after_multi_select`, toggled by `x`) is non-empty but its members don't all share one AST
    /// node kind. Same "confirm before doing something codediff's own diff would never produce"
    /// posture as `ConfirmKindMismatch`, generalized to a set of ids instead of a single pair.
    ConfirmMultiMapGroup {
        before_ids: Vec<usize>,
        after_ids: Vec<usize>,
        operation: HumanOperation,
        with_children: bool,
        kinds: Vec<String>,
    },
    /// Raised by `o`: pick a test case (a directory under src/test/data/diffs/) to open. Each
    /// option is paired with which of `DIFF_DATASETS` it lives under.
    ///
    /// Rendered as a table (`render_open_diff_picker`), one column per dimension of the corpus a
    /// reader might want to triage on (see `DiffColumn`). `h`/`l` move a cursor between columns,
    /// `s` sorts by the cursor column (again to flip direction), and `f` filters on it; filters
    /// combine as an AND across columns, so compound queries ("still needs nodes marked AND
    /// disagrees with its own painting") are expressible without a key per combination. All of
    /// that lives in `view`, persisted on `App::diff_view` so it survives closing and reopening.
    ///
    /// Like `OpenSamplePicker`, `selected` indexes into the filtered view
    /// (`visible_diff_options`), not `options` itself.
    OpenDiffPicker {
        options: Vec<(String, &'static str)>,
        selected: usize,
        view: DiffPickerView,
        /// `Some` while `f` on the `Name` column is collecting its substring. While it is, the
        /// picker's key handler routes *every* key into this buffer rather than treating it as a
        /// command - otherwise typing a name containing `j`, `s` or `f` would move the selection
        /// and re-sort the table mid-word. Enter commits (an empty or blank entry clears the
        /// filter rather than matching nothing), Esc abandons it and leaves the filter as it was.
        name_input: Option<String>,
    },
    /// Raised by `O`: pick a sampled candidate (a directory under src/test/data/samples/) to
    /// open. Each option is paired with its `SampleTriageStatus` (per the matching sample.csv
    /// row's `status` column) -- a promotion is shown as " - SOLVED" and a rejection as
    /// " - REJECTED", and both are left out of the list entirely when `hide_solved` is set --
    /// and with its `sample_diff_line_count` (computed once when the picker opens, not on every
    /// `s` press). `selected` indexes into `visible_sample_options(&options, hide_solved,
    /// sort_order)`, not `options` itself.
    OpenSamplePicker {
        rows: Vec<SampleRow>,
        selected: usize,
        view: SamplePickerView,
        /// `Some` while `f` on the `Name` column is mid-prompt, holding what has been typed so
        /// far - same contract as `OpenDiffPicker::name_input`.
        name_input: Option<String>,
    },
    /// Raised by `!`: confirms throwing away everything recorded for this case - the tree
    /// mapping, its multi-map groups, and every named painting - so it can be re-solved from
    /// nothing. Behind a confirmation because there is no undo and no partial form of it: the
    /// alternative to getting this wrong is re-solving a fixture by hand.
    ConfirmResetCase {
        entries: usize,
        groups: usize,
        paintings: usize,
    },
    /// Raised when a picker's selection is confirmed while the current mapping has unsaved
    /// changes: asks whether to save the *current* case before switching to `target`.
    /// `can_save` is false when the current case is a sample, since promoting one needs a name
    /// (see `PromptPromoteName`) rather than being a single-key save.
    ConfirmDiscardUnsaved { target: OpenTarget, can_save: bool },
    /// Raised by `s` when the current case is a sample: asks for the name to promote it under in
    /// `src/test/data/diffs/`. Re-raised with `error` set (input preserved) if the name is
    /// invalid or already in use.
    PromptPromoteName {
        input: String,
        error: Option<String>,
    },
    /// Raised by `R` when the current case is a sample: asks for a reason to reject it instead of
    /// promoting it. Recorded as-is in sample.csv's `comment` column, with `status` set to
    /// `REJECTED` and `promoted_to` left untouched (empty). Re-raised with `error` set (input
    /// preserved) if the reason is empty or the sample.csv row can't be found - same posture as
    /// `PromptPromoteName`.
    PromptRejectReason {
        input: String,
        error: Option<String>,
    },
    /// Raised by `e` when the current case is a sample: enters or edits a free-form comment on it,
    /// pre-filled with whatever's already recorded (if anything). Recorded as-is in sample.csv's
    /// `comment` column - unlike `PromptRejectReason`, doesn't touch `status`, and an empty
    /// submission is valid (clears the comment). If a comment is present when the sample is later
    /// promoted (`s`), it's also written as a leading doc comment in the generated
    /// optimal_solutions test stub - see `action_promote`/`ensure_stub_test`. Re-raised with
    /// `error` set (input preserved) if the sample.csv row can't be found - same posture as
    /// `PromptPromoteName`.
    PromptComment {
        input: String,
        error: Option<String>,
    },
    /// Raised by `/`: asks for text to search for. Pre-filled with `App::last_search`, if any.
    /// `Enter` runs the search (`action_search`) and closes the modal either way (found or not) -
    /// unlike `PromptPromoteName`, a failed search isn't invalid input to correct, just "nothing
    /// found from here", reported on the status line instead of re-prompting.
    PromptSearch { input: String },
    /// Raised by `t`: both sides' source side by side, for reading the actual code instead of
    /// navigating the AST tree - and for *painting* the human's text-range ground truth onto it
    /// (see `HumanTextMapping`), which is a second, independent account of the same diff that the
    /// tree mapping cannot supply. `T` while open switches to `UnixDiffView` instead.
    TextView { state: TextPaintState },
    /// Raised by `s` (saving) or `L` (loading) inside the text view: which named painting
    /// (`HumanMapping::text_mappings`) to store the current ranges under, or to switch to editing.
    ///
    /// `names` is `solution_picker_names`' output - this fixture's existing paintings first, then
    /// whichever of `SUGGESTED_SOLUTION_NAMES` it doesn't have - and the list always renders one
    /// extra row past its end for a free-form name, which `new_name` fills in once typing starts.
    /// `state` is carried so closing this picker returns to the text view exactly where it was.
    SolutionPicker {
        names: Vec<String>,
        selected: usize,
        /// `true` for `s` (save the current ranges under the chosen name), `false` for `L` (just
        /// switch which painting is being edited). The two differ only in what `Enter` does.
        saving: bool,
        new_name: Option<String>,
        /// The painting `D` has been pressed once on, awaiting a second `D` to actually delete it.
        ///
        /// Two keystrokes rather than one, and the name carried through rather than an index:
        /// deleting a painting throws away work that may have taken an hour and there is no undo
        /// here, so the confirmation has to be about the *painting the reader saw named on screen*
        /// - not about whichever row the cursor happens to be on by the time the second key lands.
        confirm_delete: Option<String>,
        state: TextPaintState,
    },
    /// Raised by `T`: shows the output of running the system `diff -u` between the before and
    /// after content -- a plain line-based diff, as a point of comparison against codediff's own
    /// AST-based diff (`p`). `t` while open switches to `TextView` instead.
    UnixDiffView { output: String, scroll: u16 },
    /// Raised by `?`: lists every keybinding. `?` or `Esc` while open closes it.
    Help { scroll: u16 },
    /// Raised by `C`: pick a commit from this repository's own `git log` (see `list_repo_commits`)
    /// to open. `j`/`k` move, `Enter` lists the files it changed (`OpenCommitFilePicker`), `Esc`
    /// cancels. `(hash, summary)` pairs, newest first, same order `list_repo_commits` returns.
    OpenCommitPicker {
        commits: Vec<(String, String)>,
        selected: usize,
    },
    /// Raised when a commit is chosen in `OpenCommitPicker`: pick which of the files it changed to
    /// open. `hash`/`summary` are carried along from that commit, just for display and to build
    /// the `OpenTarget` on `Enter`. Unlike the picker it was raised from, `Esc` here cancels
    /// entirely rather than returning to `OpenCommitPicker` - consistent with every other modal in
    /// this file, none of which have a "back" step either.
    OpenCommitFilePicker {
        hash: String,
        summary: String,
        files: Vec<String>,
        selected: usize,
    },
}

/// Which open picker (`o`, `O`, or `C`) a pending switch came from, and enough to load it.
#[derive(Debug, Clone)]
pub(crate) enum OpenTarget {
    Diffs(String),
    Sample(String),
    /// From `C`'s file picker: `path` as changed by commit `hash` (`summary` is only carried
    /// along for the status message once it's opened - see `run_event_loop`).
    GitCommitFile {
        hash: String,
        summary: String,
        path: String,
    },
}

impl OpenTarget {
    pub(crate) fn name(&self) -> &str {
        match self {
            OpenTarget::Diffs(name) | OpenTarget::Sample(name) => name,
            OpenTarget::GitCommitFile { path, .. } => path,
        }
    }
}

/// Where the currently open case's content lives: a committed test case, a not-yet-promoted
/// sample, or a file read straight out of this repository's own git history (`C`). Determines what
/// `s` does (see `Modal::PromptPromoteName`) and what `o`/`O`/`C` need to know before switching
/// away with unsaved changes.
#[derive(Debug, Clone)]
pub(crate) enum CaseOrigin {
    Diffs,
    Sample(SampleSource),
    /// `path` as it stood in whichever commit `C` opened it from - the commit's own hash/summary
    /// aren't kept here since nothing after load needs them again: `App::name` already carries a
    /// short hash (set once, in `run_event_loop`) for display, and promoting writes straight from
    /// `before_src`/`after_src` (the content already on screen), not by re-reading git.
    GitCommitFile {
        path: String,
    },
}

pub(crate) struct App {
    /// Name of the currently open case: a directory under src/test/data/diffs/ (if `origin` is
    /// `Diffs`), src/test/data/samples/ (if `origin` is `Sample`), or a `<path>@<short hash>`
    /// display label with no directory of its own (if `origin` is `GitCommitFile`). Can change at
    /// runtime via the `o`/`O`/`C` (open) pickers, or via promoting a sample or git-commit-sourced
    /// case with `s`.
    pub(crate) name: String,
    pub(crate) origin: CaseOrigin,
    pub(crate) focus: Focus,
    pub(crate) before: PanelState,
    pub(crate) after: PanelState,
    pub(crate) mapping: HumanMapping,
    pub(crate) dirty: bool,
    pub(crate) status: Option<String>,
    pub(crate) modal: Option<Modal>,
    pub(crate) should_quit: bool,
    /// codediff's own diff, computed on demand by `p` and rendered in parentheses next to each
    /// node's human-marked status glyph for a quick visual diff against the human mapping. `None`
    /// until `p` has been pressed at least once for the current case.
    pub(crate) algo_diff: Option<ASTDiff>,
    /// Toggled by `H`: when true, a subtree is left out of both panels' flattened view entirely
    /// once every node in it (the root and all descendants) has `NodeStatus` other than
    /// `Unmarked` -- i.e. nothing left in it to review. Recomputed fresh each frame from the
    /// current mapping, so it can't drift out of sync with what's actually marked.
    pub(crate) hide_solved: bool,
    /// Toggled by `r`: when true, each node's algo-verdict glyph (see `algo_diff`) is followed by
    /// the short label of the `ASTMappingReason` codediff recorded for it (e.g. "IdHash", "APTED")
    /// -- which pass is responsible for that mapping, not just what the mapping is. Has no effect
    /// until `algo_diff` is populated (`p`).
    pub(crate) show_reason: bool,
    /// The `O` picker's cursor column, sort and per-column filters (see `SamplePickerView`),
    /// persisted here rather than rebuilt every time a fresh `Modal::OpenSamplePicker` is built --
    /// so narrowing to e.g. Go samples in the 1000-3000 stratum once, then closing the picker to
    /// work through a few, sticks for the next `O` instead of reverting to the whole list. The
    /// same contract `App::diff_view` has for `o`. (Not to be confused with `hide_solved` above,
    /// which hides solved *subtrees* in the AST panels, not rows in this list.)
    pub(crate) sample_view: SamplePickerView,
    /// Cached `sample_diff_line_count` per sample name, for the `O` picker's size column and its
    /// two size-based sort orders. Cached because that count costs an external `diff` per sample
    /// and the picker needs *every* sample's before it can draw: measured at 3.9s for the 1489
    /// samples the stratified draw produced, paid on every single `O` press, where a few dozen
    /// samples used to make it imperceptible. A materialized sample's before/after files never
    /// change (promotion copies them out, it does not rewrite them), so a count only ever has to
    /// be taken once per session; new samples appearing on disk mid-session are still picked up,
    /// because only the names missing from this map get scanned.
    pub(crate) sample_diff_sizes: std::collections::HashMap<String, usize>,
    /// The `o` picker's cursor column, sort and per-column filters (see `DiffPickerView`),
    /// persisted here rather than rebuilt from scratch on every `o`, for the same reason as
    /// `sample_hide_solved`/`sample_sort_order` above: narrowing to e.g. just `handmade` fixtures
    /// that still have unmarked nodes, ordered by how many, should stick across closing the picker
    /// to work through a few of them.
    pub(crate) diff_view: DiffPickerView,
    /// Cache of, for every case `list_available_cases` lists that already has a text painting, how
    /// many bytes its tree mapping and painting disagree about (see
    /// `diff_case_disagreement_bytes`). `None` until the first `s`/`f` on the picker's `Disagree`
    /// column, the same lazy-once-per-session contract `diff_unmarked` has - and the most
    /// expensive of the three scans (see that function's own doc comment).
    pub(crate) diff_disagreement: Option<std::collections::HashMap<String, usize>>,
    /// Cache of, for every case `list_available_cases` lists, whether it already has a painted
    /// text mapping (see `diff_case_has_text_mapping`). `None` until the first `s`/`f` on the
    /// picker's `Paint` column, the same lazy-once-per-session contract `diff_unmarked` has -
    /// though this scan is much cheaper, since it skims JSON rather than parsing source with
    /// tree-sitter.
    pub(crate) diff_text_painted: Option<std::collections::HashMap<String, bool>>,
    /// Every case's `description.md`, for the `o` picker's note marker and footer. `None` until
    /// the first `o` press, the same lazy-once-per-session contract its two neighbours have - but
    /// unlike them this is loaded on `o` itself rather than on the key that filters by it, since
    /// it is displayed rather than filtered on and has to be there the first time the list is
    /// drawn. Cases with no note are absent from the map, not present-and-empty.
    pub(crate) diff_comments: Option<std::collections::HashMap<String, String>>,
    /// What the `t` view is painting on screen (see `TextOverlay`) - the human's own ranges,
    /// codediff's, or only where they differ. Cycled by `p` inside that view.
    pub(crate) text_overlay: TextOverlay,
    /// codediff's own text ranges for the open case, per side, computed on first use and dropped
    /// when the case changes. `None` until `p` has cycled past `Human` at least once - running the
    /// diff costs real time on a large fixture, and the default view never needs it.
    pub(crate) algo_text_spans: Option<[Vec<(HumanTextSpan, HumanTextVerdict)>; 2]>,
    /// The open case's *tree mapping*, rendered as text ranges via `tree_mapping_text_spans` - the
    /// human-vs-human counterpart of `algo_text_spans` (that one is `diff_code`'s output;  this one
    /// is `as_ast_diff_for_mapping`'s, i.e. the human's own `entries`, never `diff_code`). Backs
    /// `TextOverlay::TreeDisagreement`, computed and dropped the same way `algo_text_spans` is.
    pub(crate) tree_text_spans: Option<[Vec<(HumanTextSpan, HumanTextVerdict)>; 2]>,
    /// Which named text painting (`HumanMapping::text_mappings`) the `t` view is editing. Starts
    /// at the fixture's first existing painting, or `SUGGESTED_SOLUTION_NAMES[0]` when it has
    /// none, and is changed by `s` (save-as) / `L` (load) inside that view.
    pub(crate) text_solution: String,
    /// Cache of, for every case `list_available_cases` lists, how many `NodeStatus::Unmarked`
    /// nodes it still has across both trees (see `diff_case_unmarked_count`) - what the `o`
    /// picker's `Unmarked` column shows and its `Cmpl` column reduces to a glyph. `None` until the
    /// first `s`/`f` on either of those columns, since scanning the whole corpus (parsing every
    /// case's before/after code, not just listing directory names) takes real wall-clock time -
    /// ~12s across this repo's 513 fixtures on a 4-core machine, measured 2026-09-02; see
    /// `compute_diff_unmarked` for the full numbers. Cases that fail to load are absent from
    /// the map rather than present with a made-up count, which is what makes them read as `?`
    /// rather than as finished. Kept for the rest of the session once built; refreshed for just
    /// the current case's own entry after `s` saves it, rather than dropped and rebuilt from
    /// scratch, so repeated saves while triaging incomplete cases don't each cost a fresh full
    /// scan.
    pub(crate) diff_unmarked: Option<std::collections::HashMap<String, usize>>,
    /// The last text searched for with `/` (`Modal::PromptSearch`), if any - pre-fills the prompt
    /// next time, so `/` then `Enter` repeats the same search from wherever the cursor landed,
    /// without retyping it.
    pub(crate) last_search: Option<String>,
    /// Node ids pending inclusion in a multi-map group, toggled by `x` (and cleared by `c` or by
    /// `m`/`M` committing them). Plain node ids rather than borrowed `Node`s, the same convention
    /// `PanelState::cursor_id` already uses, since `App` outlives any one parse of `before`/
    /// `after` (a case switch reparses both trees under the same `App`). Cleared on every case
    /// switch (see `run_event_loop`'s three `SessionEnd::Open` arms) since an id from the old
    /// trees could otherwise collide with an unrelated node in the new ones.
    pub(crate) before_multi_select: std::collections::BTreeSet<usize>,
    pub(crate) after_multi_select: std::collections::BTreeSet<usize>,
}

impl App {
    pub(crate) fn new(
        name: String,
        origin: CaseOrigin,
        before_root_id: usize,
        after_root_id: usize,
        mapping: HumanMapping,
    ) -> Self {
        // Start on whichever painting this fixture already has, so reopening a case resumes where
        // it was left rather than silently starting a second, near-duplicate solution.
        let text_solution = starting_solution(&mapping);
        Self {
            name,
            origin,
            focus: Focus::Before,
            before: PanelState::new(before_root_id),
            after: PanelState::new(after_root_id),
            mapping,
            dirty: false,
            status: Some(
                "Loaded. m match, d/D delete, i/I insert, u unmark, s save, q quit, o open."
                    .to_string(),
            ),
            modal: None,
            should_quit: false,
            algo_diff: None,
            hide_solved: false,
            show_reason: false,
            sample_view: SamplePickerView::default(),
            sample_diff_sizes: std::collections::HashMap::new(),
            diff_view: DiffPickerView::default(),
            diff_disagreement: None,
            diff_text_painted: None,
            diff_comments: None,
            text_solution,
            text_overlay: TextOverlay::default(),
            algo_text_spans: None,
            tree_text_spans: None,
            diff_unmarked: None,
            last_search: None,
            before_multi_select: std::collections::BTreeSet::new(),
            after_multi_select: std::collections::BTreeSet::new(),
        }
    }
}
