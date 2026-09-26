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
    /// List rows available as of the last render; 0 before the first frame. `reveal_node` reads
    /// it to decide whether a node is on screen.
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

/// The overlay theme, read once at startup from the `.codediff.toml` the `codediff` binary uses.
/// Process-global like `tui::theme`'s palette, so `palette()` stays the one resolution point;
/// human_solver has no theme picker to change it at runtime.
pub(crate) static OVERLAY_THEME: std::sync::OnceLock<OverlayTheme> = std::sync::OnceLock::new();

/// Falls back to the default theme when `main` installed none, which keeps render tests
/// independent of the machine's `.codediff.toml`.
pub(crate) fn overlay_palette() -> OverlayPalette {
    OVERLAY_THEME.get().copied().unwrap_or_default().palette()
}

/// Names offered first in the save-as picker, in this order. Suggestions, not a schema: a fixture
/// may carry any number of paintings under any names, and a painting may be empty.
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

/// The save-as picker's names: existing paintings first (resaving the current one is the common
/// case), then the suggestions not yet used.
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

/// Empty if this fixture has no painting under that name.
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

/// The entries of the painting named `solution`, creating it if absent. Creation is what turns
/// "no painting called X" into "an empty painting called X", so reaching an empty one otherwise
/// takes `Z`.
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

/// `s`: stores the current painting under another name, **keeping the source**: a fixture with
/// more than one defensible rendering needs both on disk. `copy` decides whether a *new* name
/// starts from the current ranges or from nothing. An existing name is only switched to, as `L`
/// would: merging would duplicate ranges and replacing would discard work, with no undo.
pub(crate) fn action_save_solution_as(
    app: &mut App,
    target: &str,
    copy: bool,
    before_src: &str,
    after_src: &str,
) {
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

    let mut entries = if copy {
        solution_entries(&app.mapping, &app.text_solution).to_vec()
    } else {
        Vec::new()
    };
    let count = entries.len();
    // `Minimal` forbids a wholly-changed line's indentation and `Full` requires it, so a branch
    // between those two presets converts (see `expand_leading_whitespace_for_full`). A free-form
    // name states no preset, so nothing is converted.
    let extended = if copy
        && human_mapping::invariants::designates_minimal(&app.text_solution)
        && human_mapping::invariants::designates_full(target)
    {
        expand_leading_whitespace_for_full(&mut entries, before_src, after_src)
    } else {
        0
    };
    app.mapping.text_mappings.push(NamedTextMapping {
        name: target.to_string(),
        mapping: HumanTextMapping { entries },
    });
    app.text_solution = target.to_string();
    app.dirty = true;
    app.status = Some(if copy {
        let widened = match extended {
            0 => String::new(),
            1 => ", 1 line widened to its indentation".to_string(),
            rows => format!(", {rows} lines widened to their indentation"),
        };
        format!(
            "Started '{target}' as a copy of the previous painting ({count} range(s)){widened} - {} now on file",
            app.mapping.text_mappings.len()
        )
    } else {
        format!(
            "Started '{target}' empty - {} painting(s) now on file",
            app.mapping.text_mappings.len()
        )
    });
}

/// Deletes the painting named `target` and picks what to edit next. Deleting the last one leaves
/// the fixture *unpainted* (`text_mappings` empty), which differs from an empty painting (see
/// `HumanMapping::text_mappings`) and is what the `X` filter and `diffs.csv` then report.
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

    // Only deleting the painting being edited moves the reader; switching them silently would
    // invite painting into the wrong one.
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

pub(crate) fn action_load_solution(app: &mut App, target: &str) {
    app.text_solution = target.to_string();
    let count = solution_entries(&app.mapping, target).len();
    app.status = Some(format!("Editing '{target}' ({count} range(s))"));
}

/// What the `t` view paints, cycled by `o`. The disagreement modes exist because the question
/// while painting ground truth is "where do we differ", which flipping by eye answers badly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum TextOverlay {
    /// The human's own painting for the current solution.
    #[default]
    Human,
    /// codediff's diff through the same `TextDiff` projection the TUI renders.
    CodeDiff,
    /// Only bytes where human and codediff disagree, coloured by the human's label. Empty means
    /// they agree.
    Disagreements,
    /// Only bytes where the human's painting and their own tree mapping disagree (the
    /// human-vs-human comparison `text_mapping_disagreements` makes; `diff_code` is not involved).
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

/// Whether the pair has no tree on some side (no grammar, or unrecognised language). One
/// predicate so the whole tool agrees on text-only mode: the panels, `handle_key`, the generated
/// stub and [`codediff_text_spans`] all key on it.
pub(crate) fn is_text_only(before: &Code, after: &Code) -> bool {
    before.ast.is_none() || after.ast.is_none()
}

/// codediff's own text ranges for this case, one list per side, through `TextDiff::from` (what the
/// TUI renders). With no AST this is the `plain_text_line_diff` fallback, not nothing: that is
/// what the product shows and what a text-only fixture's painting is graded against.
pub(crate) fn codediff_text_spans(
    before: &Code,
    after: &Code,
) -> [Vec<(HumanTextSpan, HumanTextVerdict)>; 2] {
    // Keyed on the code, as the product is: `diff_code` returns `Some(ASTDiff)` even with no
    // trees, which would show an empty projection instead of the fallback.
    let sides = if is_text_only(before, after) {
        let (before_ranges, after_ranges) =
            codediff::diff::text::plain_text_line_diff(&before.contents, &after.contents);
        [before_ranges, after_ranges]
    } else {
        let diff = diff_code(before, after);
        match diff.ast.as_ref() {
            Some(ast_diff) => {
                let node_cache = NodeCache::build(before, after);
                let text_diff = TextDiff::from(before, after, ast_diff, &node_cache);
                [text_diff.all(0), text_diff.all(1)]
            }
            None => [Vec::new(), Vec::new()],
        }
    };

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
                    // Identical text is the unpainted background here.
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
    let [before_ranges, after_ranges] = sides;
    [convert(before_ranges), convert(after_ranges)]
}

/// codediff's rendering as painting entries, for `P` to seed an empty painting. Built from
/// `codediff_text_spans`, so seeding then showing codediff's overlay reveals no difference. Move
/// and Update both become `Match`: the verdict is derived from the spans' text, not stored.
pub(crate) fn codediff_text_entries(
    before: &Code,
    after: &Code,
) -> Result<Vec<HumanTextEntry>, &'static str> {
    let [before_spans, after_spans] = codediff_text_spans(before, after);

    // Overlapping ranges are refused: the renderer resolves an overlap by highest verdict but
    // `label_bytes` (what grading reads) by last entry, so it would render as one thing and score
    // as another. codediff's own rendering does produce overlaps.
    if spans_overlap(&before_spans) || spans_overlap(&after_spans) {
        return Err(
            "codediff's own ranges overlap on this pair, which a painting cannot represent",
        );
    }

    // `TextDiff` keeps the two sides' ranges with no cross-reference, so the k-th changed match on
    // one side is paired with the k-th on the other. Unequal counts give up: a wrong `Match` is
    // worse than no seed.
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

/// Whether any two of `spans` claim a common byte. Quadratic; runs once per `P`.
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

/// `P` in the text view: seeds the current painting with codediff's rendering, so a fixture is
/// corrected rather than painted from scratch. Refuses a painting that already has ranges, as
/// `action_paint_mark_empty` does: there is no undo. `s` branches to seed a second reading.
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

/// The human's own tree mapping (`HumanMapping::entries`, never `diff_code`) as painting spans,
/// through the pipeline `text_mapping_disagreements` uses; backs `TextOverlay::TreeDisagreement`.
/// A mapping that fails to load gives empty spans.
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

/// Spans where the human's painting and `other` disagree, labelled with the human's verdict.
/// `other` is codediff's spans for `Disagreements` and the tree mapping's for `TreeDisagreement`.
pub(crate) fn overlay_disagreement_spans(
    painted: &[Vec<(HumanTextSpan, HumanTextVerdict)>; 2],
    other: &[Vec<(HumanTextSpan, HumanTextVerdict)>; 2],
    before_src: &str,
    after_src: &str,
) -> [Vec<(HumanTextSpan, HumanTextVerdict)>; 2] {
    let mut out = [Vec::new(), Vec::new()];
    for (side, source) in [(0usize, before_src), (1usize, after_src)] {
        // Rows without the CRLF `\r`, so a run cannot start on an invisible byte.
        let lines: Vec<&str> = source
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line))
            .collect();
        for (row, line) in lines.iter().enumerate() {
            // Adjacent disagreeing columns coalesce into one span per run.
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
                // Prefer the human's label: the reader is checking their own work.
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

/// One row of the `V` popup. Site lines are built when the popup opens, because they need the
/// parsed `Code` and the render path only has source text.
#[derive(Debug, Clone)]
pub(crate) struct InvariantEntry {
    pub(crate) violation: human_mapping::invariants::GroundTruthViolation,
    pub(crate) details: Vec<String>,
}

/// Every violation of the *in-memory* mapping, with rendered sites. Not the by-name form, which
/// reads the file on disk: mid-edit that lags the screen and would list already-fixed violations.
pub(crate) fn invariant_entries(
    mapping: &HumanMapping,
    before: &Code,
    after: &Code,
) -> Result<Vec<InvariantEntry>> {
    let violations =
        human_mapping::invariants::ground_truth_invariant_violations_for(mapping, before, after)?;
    Ok(violations
        .into_iter()
        .map(|violation| {
            let details = violation
                .sites
                .iter()
                .map(|site| site_detail(*site, before, after))
                .collect();
            InvariantEntry { violation, details }
        })
        .collect())
}

/// One site as a line: `before 17:54-17:56  ": "  in line_comment 534..606`. The node part is
/// dropped for a side with no tree, and for a span that reads back as nothing.
fn site_detail(
    site: human_mapping::invariants::ViolationSite,
    before: &Code,
    after: &Code,
) -> String {
    let code = if site.side == 0 { before } else { after };
    let span = site.span;
    let mut line = format!(
        "{} {}:{}-{}:{}",
        if site.side == 0 { "before" } else { "after" },
        span.start_row + 1,
        span.start_column,
        span.end_row + 1,
        span.end_column,
    );
    if let Some(text) = human_mapping::span_text(&code.contents, span) {
        let shown: String = text.chars().take(40).collect();
        line.push_str(&format!("  {shown:?}"));
    }
    let offsets = (
        TextPaintState::byte_offset(&code.contents, span.start_row, span.start_column),
        TextPaintState::byte_offset(&code.contents, span.end_row, span.end_column),
    );
    if let (Some(tree), (Some(start), Some(end))) = (code.ast.as_ref(), offsets)
        && let Some(node) = tree
            .root_node()
            .descendant_for_byte_range(start, end.max(start))
    {
        line.push_str(&format!(
            "  in {} {}..{}",
            node.kind(),
            node.start_byte(),
            node.end_byte()
        ));
    }
    line
}

/// `Enter` in the `V` popup: moves every side the violation names, the AST panel onto the site's
/// node (via `reveal_node`) and the text cursor onto its first byte, then opens the text view.
/// Focus follows the *first* site, the before side when a rule names both.
pub(crate) fn action_focus_violation(
    app: &mut App,
    entry: &InvariantEntry,
    before: &Code,
    after: &Code,
) {
    let sites = &entry.violation.sites;
    let Some(first) = sites.first() else {
        app.status = Some("This violation has nowhere to jump to".to_string());
        return;
    };
    let mut state = TextPaintState {
        side: first.side,
        ..Default::default()
    };
    let mut moved: Vec<&str> = Vec::new();
    for side in [0usize, 1] {
        let Some(site) = sites.iter().find(|site| site.side == side) else {
            continue;
        };
        let code = if side == 0 { before } else { after };
        let name = if side == 0 { "before" } else { "after" };
        state.cursor[side] = (site.span.start_row, site.span.start_column);
        moved.push(name);

        let (Some(tree), Some(offset)) = (
            code.ast.as_ref(),
            TextPaintState::byte_offset(
                &code.contents,
                site.span.start_row,
                site.span.start_column,
            ),
        ) else {
            continue;
        };
        let root = tree.root_node();
        let Some(leaf) = first_leaf_from(root, offset) else {
            continue;
        };
        let panel = if side == 0 {
            &mut app.before
        } else {
            &mut app.after
        };
        reveal_node(panel, root, leaf.id());
    }
    app.focus = if first.side == 0 {
        Focus::Before
    } else {
        Focus::After
    };
    state.scroll_into_view(20);
    app.modal = Some(Modal::TextView { state });
    app.status = Some(format!(
        "Invariant {} - {} side(s) moved, Esc for the trees behind this",
        entry.violation.invariant,
        moved.join(" and "),
    ));
}

/// Cursor, selection and scroll for the `t` view, one set per side. Both sides keep a live
/// selection, so `m` pairs them in one keystroke as the tree's `m` pairs the two cursors.
///
/// Columns are **byte** offsets, as in `HumanTextSpan`, but the cursor steps by *characters*
/// ([`TextPaintState::step_column`]): a mid-character column gives a span `span_text` refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextPaintState {
    /// 0 = before, 1 = after, as in `TextDiff::all`.
    pub(crate) side: usize,
    /// `(row, byte column)` per side.
    pub(crate) cursor: [(usize, usize); 2],
    /// Where `v` started a selection, per side.
    pub(crate) anchor: [Option<(usize, usize)>; 2],
    /// The digits typed so far at the `:` line prompt, if open. Kept here rather than as a nested
    /// modal, which would have to carry this whole state through and back.
    pub(crate) line_prompt: Option<String>,
    /// Ranges banked with `x`, per side, for `d`/`i`/`m` to commit together: what makes an N:M
    /// match possible with one live selection.
    pub(crate) pending: [Vec<HumanTextSpan>; 2],
    /// Top visible row per side. Independent: a move's two places are often far apart.
    pub(crate) scroll: [usize; 2],
    /// Multi-row selection shape, toggled with `V`. `true` (default): one span per row over the
    /// same columns. `false`: one full-line sweep, which a moved block needs because `m` requires
    /// every span on a side to read identical text.
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
    /// The row's text without a trailing CRLF `\r`, or `""` past the end. The `\r` is part of the
    /// terminator: kept, it would be a phantom column the cursor, `$` and spans could reach. It is
    /// the row's last byte, so stored columns are unaffected.
    pub(crate) fn row_text(source: &str, row: usize) -> &str {
        source
            .split('\n')
            .nth(row)
            .map(|line| line.strip_suffix('\r').unwrap_or(line))
            .unwrap_or("")
    }

    pub(crate) fn row_count(source: &str) -> usize {
        source.split('\n').count()
    }

    /// Moves `delta` rows, clamping the column to the row and to a character boundary.
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
        // Clamping can land mid-character; walk back to a boundary.
        let column = (0..=column)
            .rev()
            .find(|&c| line.is_char_boundary(c))
            .unwrap_or(0);
        self.cursor[self.side] = (row, column);
    }

    /// Moves one *character* left or right, wrapping across rows.
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

    /// The live selection on `side`: one span per row, or one sweep (see `vertical`); empty if
    /// nothing is selected. Ends are exclusive and include the character under the rightmost
    /// endpoint, since that is what the reader sees highlighted. In vertical mode a row shorter
    /// than the left column contributes nothing.
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
                // Past the row's last character: the span ends at the line break.
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
        // A column set on one row can land mid-character on another; round down to a boundary.
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
                    // Past the row's last character: the span ends at the line break.
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

    /// Every range `side` would commit now: banked plus live, so forgetting the final `x` loses
    /// nothing.
    pub(crate) fn committable(&self, side: usize, source: &str) -> Vec<HumanTextSpan> {
        let mut spans = self.pending[side].clone();
        spans.extend(self.selection(side, source));
        spans
    }

    /// Keeps the focused side's cursor row inside a `height`-row viewport.
    pub(crate) fn scroll_into_view(&mut self, height: usize) {
        self.scroll_side_into_view(self.side, height);
    }

    /// [`Self::scroll_into_view`] for either side: `n`/`p`/`a` move the unfocused cursor too.
    pub(crate) fn scroll_side_into_view(&mut self, side: usize, height: usize) {
        let row = self.cursor[side].0;
        let top = &mut self.scroll[side];
        if row < *top {
            *top = row;
        } else if height > 0 && row >= *top + height {
            *top = row + 1 - height;
        }
    }

    /// The absolute byte offset of `(row, column)`, or `None` past the last row. Rows are summed
    /// raw, not through [`Self::row_text`]: stripping each CRLF `\r` would drift one byte per row.
    pub(crate) fn byte_offset(source: &str, row: usize, column: usize) -> Option<usize> {
        let mut offset = 0usize;
        for (index, line) in source.split('\n').enumerate() {
            if index == row {
                return (column <= line.len()).then_some(offset + column);
            }
            offset += line.len() + 1;
        }
        None
    }

    /// Where `^` lands: the first non-whitespace byte column. A whitespace-only row gives its end,
    /// where code would start (`0` already reaches column 0). Always a char boundary.
    pub(crate) fn first_code_column(source: &str, row: usize) -> usize {
        let line = Self::row_text(source, row);
        line.char_indices()
            .find(|(_, character)| !character.is_whitespace())
            .map_or(line.len(), |(index, _)| index)
    }

    /// Puts `side`'s cursor on `row` (clamped to that side's last row), keeping the column clamped
    /// as `step_row` does. Returns the row landed on.
    pub(crate) fn place_cursor(&mut self, side: usize, row: usize, source: &str) -> usize {
        let last = Self::row_count(source).saturating_sub(1);
        let row = row.min(last);
        let line = Self::row_text(source, row);
        let column = self.cursor[side].1.min(line.len());
        let column = (0..=column)
            .rev()
            .find(|&c| line.is_char_boundary(c))
            .unwrap_or(0);
        self.cursor[side] = (row, column);
        row
    }
}

/// Mirrors the library's `pub(crate)` `PLAIN_TEXT_MAX_EDIT`, so navigation finds hunks on exactly
/// the pairs whose plain-text rendering does.
const HUNK_MAX_EDIT: usize = 10_000;

/// Where each differing region of the plain line diff starts, as `(before_row, after_row)`, for
/// `n`/`p`. Read from the gaps between `line_diff_core`'s identical-line pairs, so deleted,
/// inserted and reworded lines all form hunks. Both coordinates strictly increase.
///
/// `None` means the Myers search gave up past `HUNK_MAX_EDIT`; callers must not read that as "no
/// differences".
pub(crate) fn text_diff_hunks(before_src: &str, after_src: &str) -> Option<Vec<(usize, usize)>> {
    let core = codediff::diff::text::line_diff_core(before_src, after_src, HUNK_MAX_EDIT)?;
    let mut hunks = Vec::new();
    let (mut next_before, mut next_after) = (0usize, 0usize);
    for &(before_row, after_row) in &core.pairs {
        if before_row > next_before || after_row > next_after {
            hunks.push((next_before, next_after));
        }
        next_before = before_row + 1;
        next_after = after_row + 1;
    }
    if next_before < core.before_line_count || next_after < core.after_line_count {
        hunks.push((next_before, next_after));
    }
    Some(hunks)
}

/// Invariant 4, applied when branching `Minimal` to `Full`: a line whose every visible character is
/// inserted, or every one deleted, is painted whole, indentation included (the mirror of
/// [`skip_leading_whitespace`]). A row is extended only if it has a visible character, all of it
/// is painted by one one-sided operation (a `Match` means a surviving line), its indentation is
/// unpainted (so no overlap), and a span starts at its first visible character (a row reached by
/// a multi-row span already has its indentation). Returns how many rows it extended.
pub(crate) fn expand_leading_whitespace_for_full(
    entries: &mut [HumanTextEntry],
    before_src: &str,
    after_src: &str,
) -> usize {
    let mut extended = 0;
    for (side, source) in [(0usize, before_src), (1usize, after_src)] {
        let wanted = if side == 0 {
            HumanTextOperation::Delete
        } else {
            HumanTextOperation::Insert
        };
        let spans_on_side = |entries: &[HumanTextEntry]| -> Vec<(usize, usize, HumanTextSpan)> {
            entries
                .iter()
                .enumerate()
                .flat_map(|(entry_index, entry)| {
                    let spans = if side == 0 {
                        &entry.before
                    } else {
                        &entry.after
                    };
                    spans
                        .iter()
                        .enumerate()
                        .map(move |(span_index, span)| (entry_index, span_index, *span))
                        .collect::<Vec<_>>()
                })
                .collect()
        };
        let placed = spans_on_side(entries);
        let mut rows: Vec<usize> = placed
            .iter()
            .flat_map(|(_, _, span)| span.start_row..=span.end_row)
            .collect();
        rows.sort_unstable();
        rows.dedup();

        for row in rows {
            let line = TextPaintState::row_text(source, row);
            let Some(first_visible) = line.find(|c: char| !c.is_whitespace()) else {
                continue;
            };
            let covering = |column: usize| -> Option<usize> {
                placed
                    .iter()
                    .find(|(_, _, span)| span_covers(*span, row, column, line.len()))
                    .map(|(entry_index, _, _)| *entry_index)
            };
            let every_visible_is_wanted = line[first_visible..]
                .char_indices()
                .filter(|(_, c)| !c.is_whitespace())
                .all(|(offset, _)| {
                    covering(first_visible + offset)
                        .is_some_and(|entry_index| entries[entry_index].operation == wanted)
                });
            if !every_visible_is_wanted {
                continue;
            }
            if (0..first_visible).any(|column| covering(column).is_some()) {
                continue;
            }
            let Some((entry_index, span_index, _)) = placed
                .iter()
                .find(|(_, _, span)| span.start_row == row && span.start_column == first_visible)
            else {
                continue;
            };
            let spans = if side == 0 {
                &mut entries[*entry_index].before
            } else {
                &mut entries[*entry_index].after
            };
            spans[*span_index].start_column = 0;
            extended += 1;
        }
    }
    extended
}

/// `n` / `p`: moves *both* sides' cursors to the next/previous hunk, wrapping. Both, because the
/// panels scroll independently. "Next" is read off the focused side's row. A live selection is
/// left alone.
pub(crate) fn action_paint_next_diff(
    app: &mut App,
    state: &mut TextPaintState,
    before_src: &str,
    after_src: &str,
    forward: bool,
    viewport_rows: usize,
) {
    let Some(hunks) = text_diff_hunks(before_src, after_src) else {
        app.status = Some(
            "This file pair is too different to navigate by hunk (the line diff gave up)"
                .to_string(),
        );
        return;
    };
    if hunks.is_empty() {
        app.status = Some("No differences between the two sides".to_string());
        return;
    }

    let row = state.cursor[state.side].0;
    let key = |hunk: &(usize, usize)| if state.side == 0 { hunk.0 } else { hunk.1 };
    let index = if forward {
        hunks.iter().position(|hunk| key(hunk) > row)
    } else {
        hunks.iter().rposition(|hunk| key(hunk) < row)
    }
    .unwrap_or(if forward { 0 } else { hunks.len() - 1 });

    let (before_row, after_row) = hunks[index];
    let before_row = state.place_cursor(0, before_row, before_src);
    let after_row = state.place_cursor(1, after_row, after_src);
    state.scroll_side_into_view(0, viewport_rows);
    state.scroll_side_into_view(1, viewport_rows);

    app.status = Some(format!(
        "Diff {}/{} - Before line {}, After line {}",
        index + 1,
        hunks.len(),
        before_row + 1,
        after_row + 1
    ));
}

/// `a`: puts the unfocused side on the focused side's line *number* and scroll. Not diff-aware;
/// `n`/`p` are.
pub(crate) fn action_paint_align(
    app: &mut App,
    state: &mut TextPaintState,
    before_src: &str,
    after_src: &str,
    viewport_rows: usize,
) {
    let other = 1 - state.side;
    let (source, name) = if other == 0 {
        (before_src, "Before")
    } else {
        (after_src, "After")
    };
    let row = state.cursor[state.side].0;
    let landed = state.place_cursor(other, row, source);
    // Same top row, so both panels show the same line numbers; the clamp fixes a short side.
    state.scroll[other] = state.scroll[state.side];
    state.scroll_side_into_view(other, viewport_rows);

    let mut status = format!("Aligned the {name} side to line {}", landed + 1);
    if landed != row {
        status.push_str(&format!(
            " - it has no line {}, that side ends there",
            row + 1
        ));
    }
    // Moving a cursor with an anchor grows a selection the reader is not looking at, so say so.
    if state.anchor[other].is_some() {
        status.push_str(" - its selection now reaches there too");
    }
    app.status = Some(status);
}

/// The first leaf of `node`'s subtree not ended by `offset`: the leaf containing it, or in
/// whitespace the next one; `None` in the file's trailing whitespace. Not
/// `descendant_for_byte_range`, which answers with a container in whitespace.
pub(crate) fn first_leaf_from(node: Node, offset: usize) -> Option<Node> {
    if node.child_count() == 0 {
        return (node.end_byte() > offset).then_some(node);
    }
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|child| child.end_byte() > offset)
        .find_map(|child| first_leaf_from(child, offset))
}

/// `A` in the text view: puts this side's AST panel on the leaf under the text cursor and focuses
/// it. The view stays open, since repairs check several rows in one pass. Invariants that cross
/// painting and mapping report rows; this turns a row into the node whose entry must change.
pub(crate) fn action_paint_reveal_node(
    app: &mut App,
    state: &TextPaintState,
    before: &Code,
    after: &Code,
) {
    let side = state.side;
    let code = if side == 0 { before } else { after };
    let name = if side == 0 { "Before" } else { "After" };
    // A text-only fixture has no tree to reveal anything in.
    let Some(tree) = code.ast.as_ref() else {
        app.status = Some(format!(
            "The {name} side has no syntax tree - this fixture is text-only"
        ));
        return;
    };
    let (row, column) = state.cursor[side];
    let Some(offset) = TextPaintState::byte_offset(&code.contents, row, column) else {
        app.status = Some(format!("No line {} on the {name} side", row + 1));
        return;
    };

    let root = tree.root_node();
    let Some(leaf) = first_leaf_from(root, offset) else {
        app.status = Some(format!(
            "Nothing but whitespace from line {} on - the {name} tree ends before here",
            row + 1
        ));
        return;
    };
    let exact = leaf.start_byte() <= offset;

    let panel = if side == 0 {
        &mut app.before
    } else {
        &mut app.after
    };
    if reveal_node(panel, root, leaf.id()).is_none() {
        app.status = Some("That leaf is not in this side's tree".to_string());
        return;
    }
    app.focus = if side == 0 {
        Focus::Before
    } else {
        Focus::After
    };

    let text: String = code.contents[leaf.byte_range()].chars().take(30).collect();
    app.status = Some(if exact {
        format!("{name} tree on {:?} `{text}`", leaf.kind())
    } else {
        format!(
            "Nothing on line {} column {} - {name} tree on the next leaf, {:?} `{text}` on line {}",
            row + 1,
            column,
            leaf.kind(),
            leaf.start_position().row + 1,
        )
    });
}

/// The first span of `entry` overlapping a range already painted in `solution`, as
/// `(side, description)`. Overlaps are refused at the keystroke, while the selection is on screen:
/// the renderer resolves them by verdict but grading by entry order, so they would render and
/// score differently.
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

/// Compared as absolute offsets so `(row + 1, 0)` and `(row, row_len)` are one position. A shared
/// line terminator does not count: `label_bytes` never labels one.
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

/// `m`: pairs everything selected on the before side with everything on the after side as one
/// `Match`; banked ranges make it N:M. Needs ranges on both sides, like the tree's `m`.
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
    // Resolved now, so a group whose spans don't hold up is rejected while still on screen.
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

/// `d` / `i`: paints everything selected on the focused side as one removal or addition. Unlike a
/// match, banked spans need not read the same. In a `Minimal` painting a multi-row full-line sweep
/// is committed without indentation, one range per row (invariant 6, see
/// [`skip_leading_whitespace`]); vertical selections and other paintings are left as drawn.
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

    // Counted before the split, so the status line reports what was selected.
    let count = spans.len();
    // Only a multi-row sweep is reshaped: it says "these whole lines", not "these columns".
    let minimal_mode =
        human_mapping::invariants::designates_minimal(&app.text_solution) && !state.vertical;
    let mut split_any = false;
    let spans: Vec<HumanTextSpan> = if minimal_mode {
        spans
            .into_iter()
            .flat_map(|span| {
                if span.end_row > span.start_row {
                    split_any = true;
                    skip_leading_whitespace(span, source)
                } else {
                    vec![span]
                }
            })
            .collect()
    } else {
        spans
    };
    // Only a sweep over blank lines trims to nothing; the painter did select something.
    if spans.is_empty() {
        app.status = Some(
            "Only blank lines selected - Minimal claims no indentation, so nothing to paint"
                .to_string(),
        );
        return;
    }
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
    let note = if split_any {
        " - indentation left unpainted (Minimal)"
    } else {
        ""
    };
    app.status = Some(match operation {
        HumanTextOperation::Delete => format!("Painted {count} deletion(s){note}"),
        _ => format!("Painted {count} insertion(s){note}"),
    });
}

/// One span per row `span` covers, each starting at the row's first code character: a `Minimal`
/// painting never paints leading whitespace (invariant 6). Whitespace-only rows drop out. The
/// newlines between rows are not lost: `label_bytes` never labels a line terminator anyway.
pub(crate) fn skip_leading_whitespace(span: HumanTextSpan, source: &str) -> Vec<HumanTextSpan> {
    (span.start_row..=span.end_row)
        .filter_map(|row| {
            // `row_text` drops a CRLF `\r`, which would otherwise be a phantom last column.
            let length = TextPaintState::row_text(source, row).len();
            let start = if row == span.start_row {
                span.start_column
            } else {
                0
            }
            .max(TextPaintState::first_code_column(source, row));
            let end = if row == span.end_row {
                span.end_column.min(length)
            } else {
                length
            };
            (start < end).then_some(HumanTextSpan {
                start_row: row,
                start_column: start,
                end_row: row,
                end_column: end,
            })
        })
        .collect()
}

/// `u`: removes the painted entry under the cursor, *both* sides of a `Match`: half a match is a
/// malformed entry.
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

/// `!`: clears the tree mapping, its groups and every painting, since stale paintings would
/// describe a discarded mapping. In memory only until `s`; sets `dirty` so switching cases warns.
pub(crate) fn action_reset_case(app: &mut App) -> String {
    let entries = app.mapping.entries.len();
    let groups = app.mapping.groups.len();
    let paintings = app.mapping.text_mappings.len();

    app.mapping.entries.clear();
    app.mapping.groups.clear();
    app.mapping.text_mappings.clear();

    // A stale `tree_text_spans` would draw the discarded mapping, and `text_solution` would name
    // a painting that no longer exists.
    app.tree_text_spans = None;
    app.text_solution = starting_solution(&app.mapping);
    app.clear_multi_select();
    app.dirty = true;

    format!(
        "Reset: cleared {entries} mapping entries, {groups} groups and {paintings} paintings - \
         nothing written until s"
    )
}

/// `Z`: marks this fixture's painting complete with nothing painted, for identical files, which
/// would otherwise look unvisited.
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

/// A blocking prompt. While `App::modal` is `Some`, keys go to `handle_modal_key`.
#[derive(Debug, Clone)]
pub(crate) enum Modal {
    /// The two cursor nodes have different kinds. codediff pairs different kinds only where
    /// `nodes::kinds_update_allowed` permits, so this is usually a mismatch; confirmed explicitly.
    ConfirmKindMismatch {
        before_id: usize,
        after_id: usize,
        before_kind: String,
        after_kind: String,
        /// From `M`: confirming also auto-matches the rest of the subtree.
        recursive: bool,
    },
    /// Raised by `m`/`M` when the multi-map selection mixes AST kinds; the set form of
    /// `ConfirmKindMismatch`.
    ConfirmMultiMapGroup {
        before_ids: Vec<usize>,
        after_ids: Vec<usize>,
        operation: HumanOperation,
        with_children: bool,
        /// Carried so the confirmation names, and commits, exactly what was selected.
        pairing: GroupPairing,
        kinds: Vec<String>,
    },
    /// Raised by `o`: pick a case under src/test/data/diffs/, each paired with its dataset. A
    /// table (see `DiffColumn`): `h`/`l` pick a column, `s` sorts, `f` filters, and filters AND
    /// together. `view` persists on `App::diff_view`. `selected` indexes `visible_diff_options`.
    OpenDiffPicker {
        options: Vec<(String, &'static str)>,
        selected: usize,
        view: DiffPickerView,
        /// `Some` while `f` on `Name` is collecting its substring. Every key goes into it, so a
        /// name containing `j`, `s` or `f` is not read as commands. Enter commits (blank clears
        /// the filter), Esc leaves the filter as it was.
        name_input: Option<String>,
    },
    /// Raised by `O`: pick a sample under src/test/data/samples/, each with its
    /// `SampleTriageStatus` and `sample_diff_line_count`. `selected` indexes
    /// `visible_sample_rows`.
    OpenSamplePicker {
        rows: Vec<SampleRow>,
        selected: usize,
        view: SamplePickerView,
        /// Same contract as `OpenDiffPicker::name_input`.
        name_input: Option<String>,
    },
    /// Raised by `!`: confirms throwing away everything recorded for this case. There is no
    /// undo.
    ConfirmResetCase {
        entries: usize,
        groups: usize,
        paintings: usize,
    },
    /// A picker selection with unsaved changes: save the current case before switching to
    /// `target`? `can_save` is false for a sample, which needs a name (`PromptPromoteName`).
    ConfirmDiscardUnsaved { target: OpenTarget, can_save: bool },
    /// Raised by `s` on a sample: the name to promote it under in `src/test/data/diffs/`.
    /// Re-raised with `error` (input kept) if the name is invalid or taken.
    PromptPromoteName {
        input: String,
        error: Option<String>,
    },
    /// Raised by `R` on a sample: a reason to reject it, recorded verbatim in sample.csv with
    /// status `REJECTED`. Re-raised with `error` if empty or the row is missing.
    PromptRejectReason {
        input: String,
        error: Option<String>,
    },
    /// Raised by `e` on a sample: edits its sample.csv comment (empty clears it; `status` is
    /// untouched). Promotion writes the comment into the generated stub. Re-raised with `error`
    /// if the row is missing.
    PromptComment {
        input: String,
        error: Option<String>,
    },
    /// Raised by `/`, pre-filled with `App::last_search`. Enter closes it whether or not anything
    /// matched; a miss is reported on the status line, not re-prompted.
    PromptSearch { input: String },
    /// Raised by `t`: both sides' source, for reading and for painting the text-range ground truth
    /// (`HumanTextMapping`). `T` switches to `UnixDiffView`.
    TextView { state: TextPaintState },
    /// Raised by `V`: every self-contradiction in this case's ground truth; Enter moves both trees
    /// and text panels onto one. A snapshot of the in-memory mapping, taken on open.
    InvariantList {
        entries: Vec<InvariantEntry>,
        selected: usize,
    },
    /// Raised by `s`/`L` in the text view: which painting to save under or switch to. `names` is
    /// `solution_picker_names`; one extra row past its end takes a free-form `new_name`. `state`
    /// returns the text view to where it was.
    SolutionPicker {
        names: Vec<String>,
        selected: usize,
        /// `true` for `s` (save the current ranges), `false` for `L` (switch only).
        saving: bool,
        new_name: Option<String>,
        /// The painting a first `D` was pressed on, awaiting the second. Carried by name, so the
        /// confirmation is about the painting shown, not whichever row the cursor is on by then.
        confirm_delete: Option<String>,
        state: TextPaintState,
    },
    /// Raised by `T`: `diff -u` of before and after. `t` switches to `TextView`.
    UnixDiffView { output: String, scroll: u16 },
    /// Raised by `?`: every keybinding. `?` or `Esc` closes it.
    Help { scroll: u16 },
    /// Raised by `C`: pick a commit from `list_repo_commits` (newest first). Enter lists its
    /// files (`OpenCommitFilePicker`).
    OpenCommitPicker {
        commits: Vec<(String, String)>,
        selected: usize,
    },
    /// Raised by choosing a commit in `OpenCommitPicker`: pick one of its changed files. `Esc`
    /// cancels entirely; no modal here has a "back" step.
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
    /// From `C`'s file picker: `path` at commit `hash`; `summary` is only for the status message.
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

/// Where the open case's content lives. Decides what `s` does and what switching away with
/// unsaved changes has to offer.
#[derive(Debug, Clone)]
pub(crate) enum CaseOrigin {
    Diffs,
    Sample(SampleSource),
    /// `path` at the commit `C` opened it from. The commit is not kept: `App::name` carries the
    /// short hash, and promotion writes the content already loaded.
    GitCommitFile {
        path: String,
    },
}

pub(crate) struct App {
    /// A directory under src/test/data/diffs/ (`Diffs`) or samples/ (`Sample`), or a
    /// `<path>@<short hash>` label (`GitCommitFile`). Changes on open and on promotion.
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
    /// codediff's own diff, from `p`. `None` until `p` runs for this case.
    pub(crate) algo_diff: Option<ASTDiff>,
    /// `H`: hides fully marked subtrees in both panels, recomputed every frame.
    pub(crate) hide_solved: bool,
    /// `r`: shows each node's `ASTMappingReason` label after its codediff glyph (needs `p`).
    pub(crate) show_reason: bool,
    /// The `O` picker's column, sort and filters, persisted so they stick across reopening.
    pub(crate) sample_view: SamplePickerView,
    /// `sample_diff_line_count` per sample, taken once per session: each costs an external
    /// `diff` and sample files never change. Only names missing here are scanned.
    pub(crate) sample_diff_sizes: std::collections::HashMap<String, usize>,
    /// The `o` picker's column, sort and filters, persisted so they stick across reopening.
    pub(crate) diff_view: DiffPickerView,
    /// Per painted case, the bytes its tree mapping and painting disagree on
    /// (`diff_case_disagreement_bytes`). Lazy: `None` until the `Disagree` column is sorted or
    /// filtered.
    pub(crate) diff_disagreement: Option<std::collections::HashMap<String, usize>>,
    /// Invariant violations per case, for the `Invariant` column. Lazy like the caches above; a
    /// case with no mapping is absent rather than 0.
    pub(crate) diff_invariants: Option<std::collections::HashMap<String, usize>>,
    /// Changed-line count per case (`diff_case_size`), for the `Size` column. Lazy.
    pub(crate) diff_sizes: Option<std::collections::HashMap<String, usize>>,
    /// Whether each case has a painting (`diff_case_has_text_mapping`), for the `Paint` column.
    /// Lazy.
    pub(crate) diff_text_painted: Option<std::collections::HashMap<String, bool>>,
    /// Every case's `description.md`, loaded on the first `o` since it is displayed, not just
    /// filtered on. Cases with no note are absent.
    pub(crate) diff_comments: Option<std::collections::HashMap<String, String>>,
    /// What the `t` view paints (see `TextOverlay`), cycled by `o`.
    pub(crate) text_overlay: TextOverlay,
    /// codediff's text ranges per side, computed on first use and dropped on case change.
    pub(crate) algo_text_spans: Option<[Vec<(HumanTextSpan, HumanTextVerdict)>; 2]>,
    /// The human's tree mapping as text ranges (`tree_mapping_text_spans`), for
    /// `TextOverlay::TreeDisagreement`; cached like `algo_text_spans`.
    pub(crate) tree_text_spans: Option<[Vec<(HumanTextSpan, HumanTextVerdict)>; 2]>,
    /// The painting the `t` view edits; see `starting_solution`. Changed by `s`/`L`.
    pub(crate) text_solution: String,
    /// `NodeStatus::Unmarked` count per case across both trees (`diff_case_unmarked_count`), for
    /// the `Unmarked` and `Cmpl` columns. Lazy, since it parses the whole corpus. Cases that fail
    /// to load are absent, so they read as `?` rather than finished. After `s`, only the saved
    /// case's entry is refreshed.
    pub(crate) diff_unmarked: Option<std::collections::HashMap<String, usize>>,
    /// The last `/` query, pre-filled next time.
    pub(crate) last_search: Option<String>,
    /// Node ids pending a multi-map group (`x` toggles, `c` or `m`/`M` clear). Ids, not `Node`s,
    /// since `App` outlives a parse; cleared on every case switch, where an old id could name an
    /// unrelated new node.
    pub(crate) before_multi_select: std::collections::BTreeSet<usize>,
    pub(crate) after_multi_select: std::collections::BTreeSet<usize>,
    /// How the pending selection's members correspond, flipped by `X`. Reset whenever the
    /// selection is cleared, so all-to-all is always deliberate.
    pub(crate) multi_select_pairing: GroupPairing,
}

impl App {
    pub(crate) fn new(
        name: String,
        origin: CaseOrigin,
        before_root_id: usize,
        after_root_id: usize,
        mapping: HumanMapping,
    ) -> Self {
        // Resume the fixture's existing painting rather than start a near-duplicate one.
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
            diff_invariants: None,
            diff_sizes: None,
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
            multi_select_pairing: GroupPairing::default(),
        }
    }

    /// Drops the pending selection and its pairing: the only way it is cleared, so no `X` leaks
    /// into the next selection.
    pub(crate) fn clear_multi_select(&mut self) {
        self.before_multi_select.clear();
        self.after_multi_select.clear();
        self.multi_select_pairing = GroupPairing::default();
    }
}
