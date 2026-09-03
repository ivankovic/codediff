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
//! Drawing the two AST panels, the modals and the status line.
//!
//! Split out of `main.rs` along the section banner that already marked this boundary.

#[allow(unused_imports)]
use crate::*;

// ---------------------------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub(crate) enum Side {
    Before,
    After,
}

pub(crate) fn node_label(node: Node, src: &[u8]) -> String {
    if node.child_count() == 0 {
        let text = node.utf8_text(src).unwrap_or("");
        let truncated: String = text.chars().take(40).collect();
        let ellipsis = if text.chars().count() > 40 { "..." } else { "" };
        format!("{} {:?}{}", node.kind(), truncated, ellipsis)
    } else {
        node.kind().to_string()
    }
}

pub(crate) fn status_glyph_and_style(status: NodeStatus) -> (&'static str, Style) {
    match status {
        NodeStatus::Unmarked => (" ", Style::default().fg(Color::Gray)),
        NodeStatus::Matched => ("M", Style::default().fg(Color::Cyan)),
        NodeStatus::Marked {
            kind: MarkKind::Deleted,
            with_children: false,
            inherited: false,
        } => ("-", Style::default().fg(Color::Red)),
        NodeStatus::Marked {
            kind: MarkKind::Deleted,
            with_children: true,
            inherited: false,
        } => (
            "-",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        NodeStatus::Marked {
            kind: MarkKind::Deleted,
            inherited: true,
            ..
        } => (
            "-",
            Style::default().fg(Color::Red).add_modifier(Modifier::DIM),
        ),
        NodeStatus::Marked {
            kind: MarkKind::Inserted,
            with_children: false,
            inherited: false,
        } => ("+", Style::default().fg(Color::Green)),
        NodeStatus::Marked {
            kind: MarkKind::Inserted,
            with_children: true,
            inherited: false,
        } => (
            "+",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        NodeStatus::Marked {
            kind: MarkKind::Inserted,
            inherited: true,
            ..
        } => (
            "+",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::DIM),
        ),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_panel(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    flat: &FlatIndex,
    panel: &mut PanelState,
    caches: &Caches,
    side: Side,
    src: &[u8],
    focused: bool,
    algo_diff: Option<&ASTDiff>,
    show_reason: bool,
    total_unmarked: usize,
    multi_selected: &std::collections::BTreeSet<usize>,
) {
    let inner_height = area.height.saturating_sub(2) as usize;
    panel.viewport_height = inner_height;
    let cursor_idx = flat.index_of(panel.cursor_id).unwrap_or(0);
    ensure_visible(&mut panel.scroll, cursor_idx, inner_height);

    // Only the rows actually on screen get built into `ListItem`s and have their status computed
    // -- `total_unmarked` (the header's "N unmarked" count) is the caller's `FrameState`'s, built
    // once per `compute_frame_state` call rather than by scanning all of `flat` here on every draw.
    let visible_end = (panel.scroll + inner_height.max(1)).min(flat.len());
    let mut items: Vec<ListItem> = Vec::with_capacity(inner_height.max(1));
    for (idx, &(node, depth)) in flat.iter().enumerate().take(visible_end).skip(panel.scroll) {
        let status = match side {
            Side::Before => status_before(node, caches),
            Side::After => status_after(node, caches),
        };

        let (glyph, mut style) = status_glyph_and_style(status);
        // A "g" suffix marks a node whose match/delete/insert outcome came from a `MultiMapGroup`
        // rather than a plain entry - `caches.before_group`/`after_group` cover every group
        // member (matched *and* leftover), not just whichever pair `representative_entries`
        // realized, so this is accurate for both.
        let in_group = match side {
            Side::Before => caches.before_group.contains_key(&node.id()),
            Side::After => caches.after_group.contains_key(&node.id()),
        };
        let group_marker = if in_group { "g" } else { "" };
        let (algo_glyph, disagrees) = algo_diff
            .map(|diff_ast| {
                let algo_status = match side {
                    Side::Before => algo_status_before(node, diff_ast),
                    Side::After => algo_status_after(node, diff_ast),
                };
                let disagrees = match side {
                    Side::Before => algo_disagrees_before(node, caches, diff_ast),
                    Side::After => algo_disagrees_after(node, caches, diff_ast),
                };
                let reason_suffix = if show_reason {
                    let reason = match side {
                        Side::Before => algo_reason_before(node, diff_ast),
                        Side::After => algo_reason_after(node, diff_ast),
                    };
                    reason
                        .map(|r| format!(" {}", reason_detail(r)))
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                (
                    format!("({}{})", algo_status_glyph(algo_status), reason_suffix),
                    disagrees,
                )
            })
            .unwrap_or_default();
        let indent = "  ".repeat(depth);
        let marker = if disagrees { " *" } else { "" };
        let text = format!(
            "{}{}{}{} {}{}",
            indent,
            glyph,
            group_marker,
            algo_glyph,
            node_label(node, src),
            marker
        );

        // Pending multi-map selection (`x`, not yet committed by `m`/`M`) - a distinct color so
        // it reads as "about to become a group", separate from any already-committed status.
        if multi_selected.contains(&node.id()) {
            style = style.fg(Color::Magenta).add_modifier(Modifier::BOLD);
        }

        if idx == cursor_idx {
            style = style
                .bg(if focused {
                    Color::Yellow
                } else {
                    Color::DarkGray
                })
                .fg(Color::Black);
        }

        items.push(ListItem::new(Line::from(Span::styled(text, style))));
    }

    let border_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            "{} — {} nodes, {} unmarked",
            title,
            flat.len(),
            total_unmarked
        ))
        .border_style(border_style);

    frame.render_widget(List::new(items).block(block), area);
}

/// Below this terminal width, `draw_ui` shows only the focused Before/After panel at full width
/// instead of splitting the screen 50/50 - two half-width panels wrap almost every line and
/// become unreadable on a narrow terminal. Shared with the main TUI's `DiffViewer`, which faces
/// the same readability constraint.
pub(crate) const SINGLE_PANEL_WIDTH_THRESHOLD: u16 =
    codediff::tui::components::diff_viewer::SINGLE_PANEL_THRESHOLD;

// Each parameter is genuinely distinct rendering context (the frame, app state, both sides'
// flattened node lists, the caches, both raw sources, both unmarked counts) - a params struct
// here would just relocate the same fields, not reduce them.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_ui(
    frame: &mut Frame,
    app: &mut App,
    before_flat: &FlatIndex,
    after_flat: &FlatIndex,
    caches: &Caches,
    before_src: &[u8],
    after_src: &[u8],
    before_unmarked: usize,
    after_unmarked: usize,
    name: &str,
) {
    let size = frame.size();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(size);

    let dataset_tag = match &app.origin {
        CaseOrigin::Diffs => case_dataset(name).unwrap_or_else(|| "?".to_string()),
        CaseOrigin::Sample(_) => "sample".to_string(),
        CaseOrigin::GitCommitFile { .. } => "git".to_string(),
    };
    frame.render_widget(
        Paragraph::new(format!(" human_solver — {} [{}] ", name, dataset_tag))
            .style(Style::default().add_modifier(Modifier::BOLD)),
        chunks[0],
    );

    // Below `SINGLE_PANEL_WIDTH_THRESHOLD` columns, two 50%-wide panels wrap every line and become
    // unreadable, so show only the focused panel at full width instead - `Tab` (which already
    // toggles `app.focus`) becomes the way to see the other side.
    let single_panel = size.width < SINGLE_PANEL_WIDTH_THRESHOLD;

    if single_panel {
        let panel_area = chunks[1];
        let (title, flat, panel, side, src, total_unmarked, multi_selected) = match app.focus {
            Focus::Before => (
                "Before",
                before_flat,
                &mut app.before,
                Side::Before,
                before_src,
                before_unmarked,
                &app.before_multi_select,
            ),
            Focus::After => (
                "After",
                after_flat,
                &mut app.after,
                Side::After,
                after_src,
                after_unmarked,
                &app.after_multi_select,
            ),
        };
        render_panel(
            frame,
            panel_area,
            title,
            flat,
            panel,
            caches,
            side,
            src,
            true,
            app.algo_diff.as_ref(),
            app.show_reason,
            total_unmarked,
            multi_selected,
        );
    } else {
        let panels = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[1]);

        render_panel(
            frame,
            panels[0],
            "Before",
            before_flat,
            &mut app.before,
            caches,
            Side::Before,
            before_src,
            app.focus == Focus::Before,
            app.algo_diff.as_ref(),
            app.show_reason,
            before_unmarked,
            &app.before_multi_select,
        );
        render_panel(
            frame,
            panels[1],
            "After",
            after_flat,
            &mut app.after,
            caches,
            Side::After,
            after_src,
            app.focus == Focus::After,
            app.algo_diff.as_ref(),
            app.show_reason,
            after_unmarked,
            &app.after_multi_select,
        );
    }

    let footer = format!(
        "{}{}{}\nm/M match[+children]  x select for multi-map  c clear selection  f match to EOF  d/D delete[+children]  i/I insert[+children]  a/A align (human/codediff)  p run codediff  r toggle reason  n/N next/prev mismatch  t text view  T unix diff  H hide solved  u unmark  h/l ←/→ collapse/expand  j/k ↑/↓ move  g/G top/bottom  Tab switch  s save  ? help  q quit",
        app.status.clone().unwrap_or_default(),
        if app.dirty { "  [UNSAVED]" } else { "" },
        if caches.unresolved > 0 {
            format!(
                "  [{} mapping entries could not be resolved against the current tree and were ignored]",
                caches.unresolved
            )
        } else {
            String::new()
        },
    );
    frame.render_widget(Paragraph::new(footer).wrap(Wrap { trim: true }), chunks[2]);

    if let Some(modal) = &app.modal {
        render_modal(
            frame,
            size,
            modal,
            name,
            promote_target_dataset(&app.origin),
            std::str::from_utf8(before_src).unwrap_or(""),
            std::str::from_utf8(after_src).unwrap_or(""),
            &app.mapping,
            &app.text_solution,
            app.text_overlay,
            app.algo_text_spans.as_ref(),
            app.tree_text_spans.as_ref(),
            DiffPickerData::from_app(app),
            app.diff_comments.as_ref(),
        );
    }
}

/// A `percent_x` x `percent_y` box centered within `area`.
pub(crate) fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

/// Like `centered_rect`, but never shrinks the popup below `min_width`/`min_height` (still capped
/// to `area` itself, since a terminal can be smaller than the popup's actual content needs) -
/// effectively reducing the percentage-based margin/padding on a small terminal instead of just
/// letting the content not fit. Real, not hypothetical: on a small terminal (an SSH client on a
/// phone is the motivating case), `render_text_modal`'s `centered_rect(60, 30, area)` could come
/// out short enough that the `> {input}` line - well past the first couple of lines of
/// instructions - scrolled out of the visible area entirely, with no scroll indicator to hint why,
/// since a plain `Paragraph` has no "not everything fit" affordance of its own.
pub(crate) fn centered_rect_at_least(
    percent_x: u16,
    percent_y: u16,
    min_width: u16,
    min_height: u16,
    area: Rect,
) -> Rect {
    let base = centered_rect(percent_x, percent_y, area);
    let width = base.width.max(min_width).min(area.width);
    let height = base.height.max(min_height).min(area.height);
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_modal(
    frame: &mut Frame,
    area: Rect,
    modal: &Modal,
    current_name: &str,
    promote_dataset: Option<&str>,
    before_src: &str,
    after_src: &str,
    mapping: &HumanMapping,
    text_solution: &str,
    text_overlay: TextOverlay,
    algo_text_spans: Option<&[Vec<(HumanTextSpan, HumanTextVerdict)>; 2]>,
    tree_text_spans: Option<&[Vec<(HumanTextSpan, HumanTextVerdict)>; 2]>,
    diff_data: DiffPickerData<'_>,
    diff_comments: Option<&std::collections::HashMap<String, String>>,
) {
    match modal {
        Modal::ConfirmKindMismatch {
            before_kind,
            after_kind,
            ..
        } => render_text_modal(
            frame,
            area,
            "Node kinds do not match!",
            &format!(
                "Before: {}\nAfter:  {}\n\nAre you sure you want to add this mapping? (y/n)",
                before_kind, after_kind
            ),
        ),
        Modal::ConfirmMultiMapGroup {
            before_ids,
            after_ids,
            operation,
            with_children,
            kinds,
        } => render_text_modal(
            frame,
            area,
            "Multi-map group has mixed node kinds!",
            &format!(
                "{} Before node(s), {} After node(s), kinds: {}\nWill be recorded as {:?}{}.\n\nAre you sure you want to add this group? (y/n)",
                before_ids.len(),
                after_ids.len(),
                kinds.join(", "),
                operation,
                if *with_children { " with children" } else { "" }
            ),
        ),
        Modal::OpenDiffPicker {
            options,
            selected,
            view,
            name_input,
        } => {
            render_open_diff_picker(
                frame,
                area,
                options,
                *selected,
                view,
                name_input.as_deref(),
                diff_data,
                diff_comments,
            );
        }
        Modal::OpenSamplePicker {
            options,
            selected,
            hide_solved,
            sort_order,
        } => {
            render_open_sample_picker(frame, area, options, *selected, *hide_solved, *sort_order);
        }
        Modal::ConfirmDiscardUnsaved { target, can_save } => render_text_modal(
            frame,
            area,
            "Unsaved changes",
            &if *can_save {
                format!(
                    "'{}' has unsaved changes.\n\nSave before opening '{}'?\n\n[s] Save & Open    [d] Discard & Open    [Esc] Cancel",
                    current_name,
                    target.name()
                )
            } else {
                format!(
                    "'{}' has unsaved changes (not a real test case yet; promote it with 's' from the main view to save it).\n\nOpen '{}' anyway?\n\n[d] Discard & Open    [Esc] Cancel",
                    current_name,
                    target.name()
                )
            },
        ),
        Modal::PromptPromoteName { input, error } => render_text_modal(
            frame,
            area,
            "Promote to test case",
            &format!(
                "Enter a name for src/test/data/diffs/{}/<name>/\n(letters, digits, - and _; must not already exist)\n\n> {}\n{}\n[Enter] confirm   [Esc] cancel",
                promote_dataset.unwrap_or("?"),
                input,
                error
                    .as_deref()
                    .map(|e| format!("\n{}\n", e))
                    .unwrap_or_default(),
            ),
        ),
        Modal::PromptRejectReason { input, error } => render_text_modal(
            frame,
            area,
            "Reject sample",
            &format!(
                "Enter a reason this sample is being rejected (recorded as-is in sample.csv)\n\n> {}\n{}\n[Enter] confirm   [Esc] cancel",
                input,
                error
                    .as_deref()
                    .map(|e| format!("\n{}\n", e))
                    .unwrap_or_default(),
            ),
        ),
        Modal::PromptComment { input, error } => render_text_modal(
            frame,
            area,
            "Sample comment",
            &format!(
                "Enter or edit a comment for this sample (recorded as-is in sample.csv;\nempty clears it; written into the generated test stub if present at promote time)\n\n> {}\n{}\n[Enter] confirm   [Esc] cancel",
                input,
                error
                    .as_deref()
                    .map(|e| format!("\n{}\n", e))
                    .unwrap_or_default(),
            ),
        ),
        Modal::PromptSearch { input } => render_text_modal(
            frame,
            area,
            "Search node text",
            &format!(
                "Find the next leaf node (in the focused panel) whose own\ntext contains this (plain substring, no regex)\n\n> {}\n\n[Enter] find next   [Esc] cancel",
                input,
            ),
        ),
        Modal::TextView { state } => {
            render_text_view_modal(
                frame,
                area,
                before_src,
                after_src,
                mapping,
                text_solution,
                text_overlay,
                algo_text_spans,
                tree_text_spans,
                state,
            );
        }
        Modal::SolutionPicker {
            names,
            selected,
            saving,
            new_name,
            confirm_delete,
            ..
        } => {
            render_solution_picker(
                frame,
                area,
                names,
                *selected,
                text_solution,
                *saving,
                new_name.as_deref(),
                confirm_delete.as_deref(),
                mapping,
            );
        }
        Modal::UnixDiffView { output, scroll } => {
            render_unix_diff_modal(frame, area, output, *scroll);
        }
        Modal::Help { scroll } => {
            render_help_modal(frame, area, *scroll);
        }
        Modal::OpenCommitPicker { commits, selected } => {
            render_open_commit_picker(frame, area, commits, *selected);
        }
        Modal::OpenCommitFilePicker {
            summary,
            files,
            selected,
            ..
        } => {
            render_open_commit_file_picker(frame, area, summary, files, *selected);
        }
    }
}

pub(crate) fn render_text_modal(frame: &mut Frame, area: Rect, title: &str, body: &str) {
    // +2 on each for the block's own top/bottom and left/right borders; the width also leaves a
    // little breathing room (+2 more) so text isn't set flush against the border, and considers
    // the title too, since a title longer than the popup is silently truncated by ratatui.
    let min_height = body.lines().count() as u16 + 2;
    let min_width = body
        .lines()
        .map(|line| line.chars().count() as u16)
        .max()
        .unwrap_or(0)
        .max(title.chars().count() as u16)
        + 4;
    let popup_area = centered_rect_at_least(60, 30, min_width, min_height, area);
    frame.render_widget(Clear, popup_area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
    frame.render_widget(
        Paragraph::new(body.to_string())
            .block(block)
            .wrap(Wrap { trim: true }),
        popup_area,
    );
}

/// Every painted span on one side, with the verdict its entry resolves to - the four operations a
/// renderer needs, derived from the three a human paints (see `HumanTextEntry::verdict`).
///
/// A malformed entry is skipped rather than failing the render: the view is how a human would
/// notice and fix it, so refusing to draw would take away the only tool for the job.
pub(crate) fn painted_spans(
    mapping: &HumanMapping,
    solution: &str,
    side: usize,
    before_src: &str,
    after_src: &str,
) -> Vec<(HumanTextSpan, HumanTextVerdict)> {
    solution_entries(mapping, solution)
        .iter()
        .filter_map(|entry| {
            let verdict = entry.verdict(before_src, after_src).ok()?;
            let spans = if side == 0 {
                &entry.before
            } else {
                &entry.after
            };
            Some(spans.iter().map(move |span| (*span, verdict)))
        })
        .flatten()
        .collect()
}

/// The four operation colours, taken from the shared overlay palette rather than hardcoded - so a
/// painted range here looks exactly like the same range does in the `codediff` TUI.
pub(crate) fn verdict_style(verdict: HumanTextVerdict) -> Style {
    let palette = overlay_palette();
    let color = match verdict {
        HumanTextVerdict::Move => palette.move_bg,
        HumanTextVerdict::Update => palette.update_bg,
        HumanTextVerdict::Delete => palette.delete_bg,
        HumanTextVerdict::Insert => palette.insert_bg,
    };
    Style::default().bg(color).fg(palette.overlay_fg)
}

/// What one byte of a row should be drawn as. Ordered so the highest-precedence class wins a
/// simple `max`: the cursor must stay findable on top of a selection, and a selection on top of
/// whatever is already painted underneath it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PaintClass {
    Plain,
    Painted(HumanTextVerdict),
    /// A range banked with `x`, waiting to be committed. Ranked below the live selection so the
    /// one being edited right now stays the one that stands out.
    Banked,
    Selected,
    Cursor,
}

/// Renders one side's source as styled lines, with painted spans, the active selection and the
/// cursor drawn on top of each other in that order.
///
/// Built byte-class-first rather than by splitting on span boundaries: spans, selection and cursor
/// overlap freely, and resolving that as a per-byte precedence is the only version that stays
/// correct when they do. Iterating `char_indices` then groups the classes back into runs, so a
/// multi-byte character is styled as one unit and never split.
pub(crate) fn render_paint_side(
    source: &str,
    spans: &[(HumanTextSpan, HumanTextVerdict)],
    state: &TextPaintState,
    side: usize,
    height: usize,
) -> Vec<Line<'static>> {
    let selection = state.selection(side, source);
    let banked = &state.pending[side];
    let (cursor_row, cursor_column) = state.cursor[side];
    let focused = state.side == side;
    let top = state.scroll[side];

    let lines: Vec<&str> = source.split('\n').collect();
    let gutter_width = lines.len().to_string().len().max(3);

    // Bucketed by row once, rather than scanning every span for every byte of every visible row.
    // A file with a few hundred painted ranges made that inner loop the render's whole cost.
    let mut spans_by_row: HashMap<usize, Vec<&(HumanTextSpan, HumanTextVerdict)>> = HashMap::new();
    for entry in spans {
        for row in entry.0.start_row..=entry.0.end_row {
            spans_by_row.entry(row).or_default().push(entry);
        }
    }
    let no_spans: Vec<&(HumanTextSpan, HumanTextVerdict)> = Vec::new();

    let class_at = |row: usize, column: usize, line: &str| -> PaintClass {
        let mut class = PaintClass::Plain;
        for (span, verdict) in spans_by_row.get(&row).unwrap_or(&no_spans) {
            if span_covers(*span, row, column, line.len()) {
                class = class.max(PaintClass::Painted(*verdict));
            }
        }
        for span in banked {
            if span_covers(*span, row, column, line.len()) {
                class = class.max(PaintClass::Banked);
            }
        }
        for span in &selection {
            if span_covers(*span, row, column, line.len()) {
                class = class.max(PaintClass::Selected);
            }
        }
        if focused && row == cursor_row && column == cursor_column {
            class = PaintClass::Cursor;
        }
        class
    };

    let mut out = Vec::with_capacity(height);
    for (row, line) in lines
        .iter()
        .enumerate()
        .take((top + height).min(lines.len()))
        .skip(top)
    {
        let line = *line;
        let mut spans_out = vec![Span::styled(
            format!("{:>width$} ", row + 1, width = gutter_width),
            Style::default().fg(Color::DarkGray),
        )];

        let mut run = String::new();
        let mut run_class: Option<PaintClass> = None;
        let push_run =
            |run: &mut String, class: Option<PaintClass>, out: &mut Vec<Span<'static>>| {
                if run.is_empty() {
                    return;
                }
                out.push(Span::styled(
                    std::mem::take(run),
                    class.map(paint_class_style).unwrap_or_default(),
                ));
            };

        for (offset, ch) in line.char_indices() {
            let class = class_at(row, offset, line);
            if run_class != Some(class) {
                push_run(&mut run, run_class, &mut spans_out);
                run_class = Some(class);
            }
            run.push(ch);
        }
        push_run(&mut run, run_class, &mut spans_out);

        // A cursor resting at end-of-line, or a blank row caught inside a multi-row span, has no
        // character run to carry it - draw one space for it. `span_covers` stops a painted span
        // one column short of this position on any *non*-empty row, so that space stays plain
        // there: painting past the last real character would read as the trailing whitespace or
        // the newline itself being part of the change.
        let end_class = class_at(row, line.len(), line);
        if end_class != PaintClass::Plain {
            spans_out.push(Span::styled(" ".to_string(), paint_class_style(end_class)));
        }

        out.push(Line::from(spans_out));
    }
    out
}

pub(crate) fn paint_class_style(class: PaintClass) -> Style {
    match class {
        PaintClass::Plain => Style::default(),
        PaintClass::Painted(verdict) => verdict_style(verdict),
        // Dimmer than the live selection, and the same hue: banked and selected are the same kind
        // of thing at different stages, not two unrelated states.
        PaintClass::Banked => Style::default()
            .bg(overlay_palette().cross_highlight_bg)
            .add_modifier(Modifier::DIM),
        // The same colour the TUI paints a cursor's counterpart with: both mean "this is the
        // region you are pointing at", one live and one committed.
        PaintClass::Selected => {
            let palette = overlay_palette();
            Style::default()
                .bg(palette.cross_highlight_bg)
                .fg(palette.overlay_fg)
        }
        PaintClass::Cursor => Style::default().add_modifier(Modifier::REVERSED),
    }
}

/// Whether `span` covers `(row, column)`, with `row_len` used to decide whether a span that ends
/// on a later row runs to the end of this one.
pub(crate) fn span_covers(span: HumanTextSpan, row: usize, column: usize, row_len: usize) -> bool {
    if row < span.start_row || row > span.end_row {
        return false;
    }
    let start = if row == span.start_row {
        span.start_column
    } else {
        0
    };
    let end = if row == span.end_row {
        span.end_column
    } else if row_len == 0 {
        // A blank row inside a multi-row span has no character of its own to carry the paint -
        // `render_paint_side`'s one-space fallback is what actually draws it, this just lets that
        // fallback see the row as covered.
        1
    } else {
        // Stop at the row's last real character. No human painting ever means to include a
        // line's trailing whitespace or its newline, so a middle row of a multi-row span must
        // not either - see the (row, len) checks below and in `render_paint_side`.
        row_len
    };
    column >= start && column < end
}

#[allow(clippy::too_many_arguments)]
/// Renders the `t` text-painting modal: both sides' source, side by side, with the human's painted
/// ranges on top and an independent cursor, selection and scroll per side.
pub(crate) fn render_text_view_modal(
    frame: &mut Frame,
    area: Rect,
    before_src: &str,
    after_src: &str,
    mapping: &HumanMapping,
    solution: &str,
    overlay: TextOverlay,
    algo_spans: Option<&[Vec<(HumanTextSpan, HumanTextVerdict)>; 2]>,
    tree_spans: Option<&[Vec<(HumanTextSpan, HumanTextVerdict)>; 2]>,
    state: &TextPaintState,
) {
    let popup_area = centered_rect(96, 92, area);
    frame.render_widget(Clear, popup_area);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(popup_area);

    // Two rows go to the block's own borders.
    let height = popup_area.height.saturating_sub(2) as usize;

    let painted = solution_entries(mapping, solution).len();
    let others = mapping.text_mappings.len().saturating_sub(1);

    // Built once for both panels: the disagreement overlay needs each side's human *and* algo
    // spans together, so it can't be derived per panel inside the loop below.
    let human_spans = [
        painted_spans(mapping, solution, 0, before_src, after_src),
        painted_spans(mapping, solution, 1, before_src, after_src),
    ];
    let empty = [Vec::new(), Vec::new()];
    let algo = algo_spans.unwrap_or(&empty);
    let tree = tree_spans.unwrap_or(&empty);
    let shown = match overlay {
        TextOverlay::Human => human_spans,
        TextOverlay::CodeDiff => algo.clone(),
        TextOverlay::Disagreements => {
            overlay_disagreement_spans(&human_spans, algo, before_src, after_src)
        }
        TextOverlay::TreeDisagreement => {
            overlay_disagreement_spans(&human_spans, tree, before_src, after_src)
        }
    };

    for (side, source, title) in [
        (0usize, before_src, {
            let pending =
                state.committable(0, before_src).len() + state.committable(1, after_src).len();
            let banked = if pending > 0 {
                format!(
                    " — {}:{} pending",
                    state.committable(0, before_src).len(),
                    state.committable(1, after_src).len()
                )
            } else {
                String::new()
            };
            format!(
                "Before [{solution}] {painted} painted{banked} — showing {} (p cycles)",
                overlay.label()
            )
        }),
        (
            1usize,
            after_src,
            match (&state.line_prompt, state.side) {
                (Some(typed), 1) => format!("After — jump to line: {typed}_"),
                _ if others > 0 => {
                    format!("After — s save-as, L load ({others} other) — u/Tab/Esc")
                }
                _ => "After — v sel/i ins/u unmark, s save-as, : jump, Tab, Esc".to_string(),
            },
        ),
    ] {
        let lines = render_paint_side(source, &shown[side], state, side, height);
        let border_style = if state.side == side {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        frame.render_widget(
            Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(border_style),
            ),
            columns[side],
        );
    }
}

#[allow(clippy::too_many_arguments)]
/// Renders the solution picker raised by `s`/`L` inside the text view: which named painting to
/// save the current ranges under, or to switch to editing.
pub(crate) fn render_solution_picker(
    frame: &mut Frame,
    area: Rect,
    names: &[String],
    selected: usize,
    current: &str,
    saving: bool,
    new_name: Option<&str>,
    confirm_delete: Option<&str>,
    mapping: &HumanMapping,
) {
    let popup_area = centered_rect(56, 50, area);
    frame.render_widget(Clear, popup_area);

    let mut items: Vec<ListItem> = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let count = solution_entries(mapping, name).len();
            let exists = mapping
                .text_mappings
                .iter()
                .any(|named| named.name == *name);
            let label = if !exists {
                format!("{name}  (new)")
            } else if name == current {
                format!("{name}  ({count} range(s), editing now)")
            } else {
                format!("{name}  ({count} range(s))")
            };
            let style = if confirm_delete == Some(name.as_str()) {
                Style::default()
                    .bg(Color::Red)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else if index == selected {
                Style::default().bg(Color::Yellow).fg(Color::Black)
            } else if name == current {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(label, style)))
        })
        .collect();

    // The free-form entry always sits last, so its index is `names.len()` - the one position the
    // key handler treats specially.
    let typing = new_name.is_some();
    let free_form = new_name.unwrap_or("");
    let free_label = if typing {
        format!("New name: {free_form}_")
    } else {
        "New name...".to_string()
    };
    items.push(ListItem::new(Line::from(Span::styled(
        free_label,
        if selected == names.len() {
            Style::default().bg(Color::Yellow).fg(Color::Black)
        } else {
            Style::default()
        },
    ))));

    let title = if let Some(doomed) = confirm_delete {
        format!("Delete '{doomed}'? D again confirms, any other key cancels")
    } else if typing {
        "Type a name — Enter confirm, Esc back".to_string()
    } else if saving {
        format!("Branch '{current}' to — Enter copy, e empty, D delete, Esc cancel")
    } else {
        "Switch to painting — j/k, Enter, D delete, Esc cancel".to_string()
    };

    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(
                    Style::default()
                        .fg(if confirm_delete.is_some() {
                            Color::Red
                        } else {
                            Color::Yellow
                        })
                        .add_modifier(Modifier::BOLD),
                ),
        ),
        popup_area,
    );
}

/// Renders the `T` (unix diff) modal: the already-computed output of `diff -u` between the before
/// and after content, with `+`/`-` lines colored to match the rest of the UI's insert/delete
/// convention and `@@` hunk headers highlighted.
pub(crate) fn render_unix_diff_modal(frame: &mut Frame, area: Rect, output: &str, scroll: u16) {
    let popup_area = centered_rect(92, 90, area);
    frame.render_widget(Clear, popup_area);

    // `diff -u` reports positions only in its `@@ -a,b +c,d @@` headers, so a reader counting to
    // the line a hunk mentions has to do it by hand. Tracking the two counters across the hunk and
    // printing them per row turns that into reading. A deleted line has no after-side number and
    // an inserted one has no before-side number, which the blank half says directly.
    let mut before_line = 0usize;
    let mut after_line = 0usize;
    let lines: Vec<Line> = output
        .lines()
        .map(|line| {
            let (style, gutter) = if line.starts_with("+++") || line.starts_with("---") {
                (Style::default().add_modifier(Modifier::BOLD), String::new())
            } else if let Some(rest) = line.strip_prefix("@@") {
                (Style::default().fg(Color::Cyan), {
                    // `@@ -a,b +c,d @@` - the two starting positions, which reset both counters.
                    let mut numbers = rest.split_whitespace();
                    for (target, sign) in [(&mut before_line, '-'), (&mut after_line, '+')] {
                        if let Some(start) = numbers.next().and_then(|token| {
                            token
                                .strip_prefix(sign)?
                                .split(',')
                                .next()?
                                .parse::<usize>()
                                .ok()
                        }) {
                            *target = start;
                        }
                    }
                    String::new()
                })
            } else if line.starts_with('+') {
                let g = format!("{:>6} {:>6} ", "", after_line);
                after_line += 1;
                (Style::default().fg(Color::Green), g)
            } else if line.starts_with('-') {
                let g = format!("{:>6} {:>6} ", before_line, "");
                before_line += 1;
                (Style::default().fg(Color::Red), g)
            } else {
                let g = format!("{:>6} {:>6} ", before_line, after_line);
                before_line += 1;
                after_line += 1;
                (Style::default(), g)
            };
            Line::from(vec![
                Span::styled(gutter, Style::default().fg(Color::DarkGray)),
                Span::styled(line.to_string(), style),
            ])
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title("unix `diff -u` — before/after line numbers — j/k scroll, t text view, Esc close")
        .border_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(
        Paragraph::new(lines).block(block).scroll((scroll, 0)),
        popup_area,
    );
}

/// Renders the `?` help modal: a static reference sheet of every keybinding (`HELP_TEXT`).
pub(crate) fn render_help_modal(frame: &mut Frame, area: Rect, scroll: u16) {
    let popup_area = centered_rect(90, 90, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title("Keybindings — j/k scroll, ? or Esc to close")
        .border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(
        Paragraph::new(HELP_TEXT).block(block).scroll((scroll, 0)),
        popup_area,
    );
}

/// Renders the `o` picker as a table, one row per case and one column per dimension of the corpus
/// worth triaging on (see `DiffColumn`) - so what a filter is doing, and what the sort is ranking
/// by, are both readable off the table itself rather than only inferable from the title bar. The
/// header row carries the interaction state: the cursor column (what `s`/`f` act on) is shown in
/// reverse video, the sorted column gets a `^`/`v` arrow, and a filtered column is marked with `*`
/// and coloured, with the filters spelled out in full in the title.
///
/// Like `render_open_sample_picker`, the filtered/sorted view (`visible_diff_options`) is
/// recomputed here from `options`/`view` rather than carried on the modal itself, so the two can
/// never drift out of sync. Scroll position is recomputed fresh each frame from `selected` (no
/// persisted state needed) by roughly centering it in the viewport, clamped to the list's extent.
///
/// `name_input` being `Some` means `f` on the `Name` column is mid-prompt: the title is replaced
/// by the prompt, since that is where the reader's attention is and the table underneath still
/// shows the pre-prompt filter until Enter commits.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_open_diff_picker(
    frame: &mut Frame,
    area: Rect,
    options: &[(String, &'static str)],
    selected: usize,
    view: &DiffPickerView,
    name_input: Option<&str>,
    data: DiffPickerData<'_>,
    comments: Option<&std::collections::HashMap<String, String>>,
) {
    let visible = visible_diff_options(options, view, data);

    let popup_area = centered_rect(80, 70, area);
    frame.render_widget(Clear, popup_area);

    // The note of whatever row is selected gets its own strip along the bottom. A note is
    // free-form prose and the names here already run to sixty-odd characters, so there is no room
    // to show one inline - the table carries a marker column saying a note exists, and this says
    // what it is for the one row the reader is actually on.
    let note = comments.and_then(|map| visible.get(selected).and_then(|name| map.get(name)));
    let (table_area, note_area) = if note.is_some() {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(4)])
            .split(popup_area);
        (split[0], Some(split[1]))
    } else {
        (popup_area, None)
    };

    // Derived from `table_area`, not `popup_area`: the footer takes rows away from the table, and
    // scrolling computed against the full popup would push the selected row off the bottom by
    // exactly the footer's height. One extra row for the header, on top of the two border rows.
    let inner_height = table_area.height.saturating_sub(3) as usize;
    let max_scroll = visible.len().saturating_sub(inner_height);
    let scroll = selected.saturating_sub(inner_height / 2).min(max_scroll);

    // `options`, not `visible`, carries each name's dataset - looked up per row rather than
    // threading a second parallel list through `visible_diff_options`, since the table only ever
    // needs it for the rows actually on screen (at most a few dozen), not the whole corpus.
    let dataset_of = |name: &str| -> &'static str {
        options
            .iter()
            .find(|(candidate, _)| candidate == name)
            .map(|(_, dataset)| *dataset)
            .unwrap_or("?")
    };

    let rows: Vec<Row> = visible
        .iter()
        .enumerate()
        .skip(scroll)
        .take(inner_height.max(1))
        .map(|(i, name)| {
            let style = if i == selected {
                Style::default().bg(Color::Yellow).fg(Color::Black)
            } else {
                Style::default()
            };
            let noted = comments.is_some_and(|map| map.contains_key(name));
            let unmarked = data.unmarked_of(name);
            let complete_mark = match unmarked {
                Some(0) => "✓",
                Some(_) => "•",
                None => "?",
            };
            let unmarked_cell = match unmarked {
                Some(count) => count.to_string(),
                None => "?".to_string(),
            };
            let painted_mark = match data.painted_of(name) {
                Some(true) => "✓",
                Some(false) => "•",
                None => "?",
            };
            let disagree_cell = match data.disagreement_of(name) {
                Some(bytes) => bytes.to_string(),
                None => "?".to_string(),
            };
            Row::new(vec![
                Cell::from(if noted {
                    format!("* {name}")
                } else {
                    format!("  {name}")
                }),
                Cell::from(dataset_of(name)),
                Cell::from(complete_mark),
                Cell::from(unmarked_cell),
                Cell::from(painted_mark),
                Cell::from(disagree_cell),
            ])
            .style(style)
        })
        .collect();

    let header = Row::new(DiffColumn::ALL.map(|column| {
        let mut label = column.header().to_string();
        if view.sort.column == column {
            label.push_str(view.sort.arrow());
        }
        if view.filters.is_active(column) {
            label.push('*');
        }
        let mut style = Style::default().add_modifier(Modifier::BOLD);
        if view.filters.is_active(column) {
            style = style.fg(Color::Cyan);
        }
        if view.column == column {
            style = style.add_modifier(Modifier::REVERSED);
        }
        Cell::from(label).style(style)
    }))
    .style(Style::default().add_modifier(Modifier::BOLD));

    let title = if let Some(input) = name_input {
        format!("Filter Name by substring: {input}_ — [Enter] apply (empty clears), [Esc] cancel")
    } else {
        let filters = view.filters.labels();
        format!(
            // Abbreviated deliberately: the popup is 80% of the terminal, so a fuller legend gets
            // truncated by ratatui at ordinary widths - and the filter list on the left, which is
            // the part that changes, is what must survive the truncation.
            "Open diff [{}] sort:{}{} ({}/{}) — h/l col, j/k row, s sort, f filter, Esc",
            if filters.is_empty() {
                "no filters".to_string()
            } else {
                filters.join(" AND ")
            },
            view.sort.column.header(),
            view.sort.arrow(),
            selected + 1,
            visible.len()
        )
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    // Same left-to-right order as `DiffColumn::ALL`, one width per column. Each is wide enough for
    // its header plus the sort arrow and filter marker the header row can append to it.
    let table = Table::new(
        rows,
        [
            Constraint::Min(20),
            Constraint::Length(9),
            Constraint::Length(6),
            Constraint::Length(10),
            Constraint::Length(7),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(block);

    frame.render_widget(table, table_area);

    if let (Some(note_area), Some(note)) = (note_area, note) {
        frame.render_widget(
            Paragraph::new(note.as_str())
                .wrap(Wrap { trim: true })
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("description.md — e to edit")
                        .border_style(Style::default().fg(Color::DarkGray)),
                ),
            note_area,
        );
    }
}

/// Like `render_open_diff_picker`, but for `O`'s sample picker: handled (already-promoted or
/// -rejected) entries are shown in green (" - SOLVED") or red (" - REJECTED"), both left out of
/// the list entirely when `hide_solved` is set, and ordered per `sort_order` (cycled by `s` - see
/// `SampleSortOrder`). Each entry also shows its `sample_diff_line_count` in parentheses, so the
/// effect of switching to a diff-size order is visible directly, not just trusted.
pub(crate) fn render_open_sample_picker(
    frame: &mut Frame,
    area: Rect,
    options: &[(String, SampleTriageStatus, usize)],
    selected: usize,
    hide_solved: bool,
    sort_order: SampleSortOrder,
) {
    let visible = visible_sample_options(options, hide_solved, sort_order);

    let popup_area = centered_rect(60, 70, area);
    frame.render_widget(Clear, popup_area);

    let inner_height = popup_area.height.saturating_sub(2) as usize;
    let max_scroll = visible.len().saturating_sub(inner_height);
    let scroll = selected.saturating_sub(inner_height / 2).min(max_scroll);

    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .skip(scroll)
        .take(inner_height.max(1))
        .map(|(i, (name, status, size))| {
            let style = if i == selected {
                Style::default().bg(Color::Yellow).fg(Color::Black)
            } else {
                match status {
                    SampleTriageStatus::Promoted => Style::default().fg(Color::Green),
                    SampleTriageStatus::Rejected => Style::default().fg(Color::Red),
                    SampleTriageStatus::Sampled => Style::default(),
                }
            };
            let suffix = match status {
                SampleTriageStatus::Promoted => " - SOLVED",
                SampleTriageStatus::Rejected => " - REJECTED",
                SampleTriageStatus::Sampled => "",
            };
            let label = format!("{name} ({size}){suffix}");
            ListItem::new(Line::from(Span::styled(label, style)))
        })
        .collect();

    let handled_count = options
        .iter()
        .filter(|(_, status, _)| status.is_handled())
        .count();
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            "Open sample ({}/{}) — j/k move, Enter open, H {} solved/rejected ({} total), s sort: {}, Esc cancel",
            if visible.is_empty() { 0 } else { selected + 1 },
            visible.len(),
            if hide_solved { "show" } else { "hide" },
            handled_count,
            sort_order.label(),
        ))
        .border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(List::new(items).block(block), popup_area);
}

/// Renders the `C` picker's first step: pick a commit from this repository's own `git log`
/// (`list_repo_commits`'s `(hash, summary)` pairs, newest first).
pub(crate) fn render_open_commit_picker(
    frame: &mut Frame,
    area: Rect,
    commits: &[(String, String)],
    selected: usize,
) {
    let popup_area = centered_rect(70, 70, area);
    frame.render_widget(Clear, popup_area);

    let inner_height = popup_area.height.saturating_sub(2) as usize;
    let max_scroll = commits.len().saturating_sub(inner_height);
    let scroll = selected.saturating_sub(inner_height / 2).min(max_scroll);

    let items: Vec<ListItem> = commits
        .iter()
        .enumerate()
        .skip(scroll)
        .take(inner_height.max(1))
        .map(|(i, (hash, summary))| {
            let style = if i == selected {
                Style::default().bg(Color::Yellow).fg(Color::Black)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(
                format!("{} {}", short_hash(hash), summary),
                style,
            )))
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            "Open commit ({}/{}) — j/k move, Enter pick a file it changed, Esc cancel",
            if commits.is_empty() { 0 } else { selected + 1 },
            commits.len()
        ))
        .border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(List::new(items).block(block), popup_area);
}

/// Renders the `C` picker's second step: pick which of `summary`'s changed files (only ones with
/// a supported language - see `list_commit_files`) to open.
pub(crate) fn render_open_commit_file_picker(
    frame: &mut Frame,
    area: Rect,
    summary: &str,
    files: &[String],
    selected: usize,
) {
    let popup_area = centered_rect(70, 70, area);
    frame.render_widget(Clear, popup_area);

    let inner_height = popup_area.height.saturating_sub(2) as usize;
    let max_scroll = files.len().saturating_sub(inner_height);
    let scroll = selected.saturating_sub(inner_height / 2).min(max_scroll);

    let items: Vec<ListItem> = files
        .iter()
        .enumerate()
        .skip(scroll)
        .take(inner_height.max(1))
        .map(|(i, path)| {
            let style = if i == selected {
                Style::default().bg(Color::Yellow).fg(Color::Black)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(path.clone(), style)))
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            "{} ({}/{}) — j/k move, Enter open, Esc cancel",
            summary,
            if files.is_empty() { 0 } else { selected + 1 },
            files.len()
        ))
        .border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(List::new(items).block(block), popup_area);
}
