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

use crate::*;
use codediff::diff::text_range::{ScreenColumn, SourceRow, cell_width_of, row_cells_of};

// ---------------------------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------------------------

pub(crate) use codediff::test::helper::human_mapping::Side;

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

/// Replaces an ASCII control character (a tab, a CRLF's `\r`) with one space. Written raw, a tab
/// desyncs ratatui's buffer from the terminal and a `\r` overwrites the row from column 0. One
/// space, not a tab stop, and C0 only (a C1 control is two bytes): a character's screen column
/// must keep matching its byte offset, because paint cursors and `HumanTextSpan`s are stored in
/// those coordinates. The product TUI's `display_safe` makes the same trade.
pub(crate) fn display_safe_char(ch: char) -> char {
    if ch.is_ascii_control() { ' ' } else { ch }
}

pub(crate) fn display_safe_str(text: &str) -> String {
    text.chars().map(display_safe_char).collect()
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
    groups: &[MultiMapGroup],
) {
    let inner_height = area.height.saturating_sub(2) as usize;
    panel.viewport_height = inner_height;
    let cursor_idx = flat.index_of(panel.cursor_id).unwrap_or(0);
    ensure_visible(&mut panel.scroll, cursor_idx, inner_height);

    // Only on-screen rows are built; `total_unmarked` comes from `FrameState`, not from `flat`.
    let visible_end = (panel.scroll + inner_height.max(1)).min(flat.len());
    let mut items: Vec<ListItem> = Vec::with_capacity(inner_height.max(1));
    for (idx, &(node, depth)) in flat.iter().enumerate().take(visible_end).skip(panel.scroll) {
        let status = match side {
            Side::Before => status_before(node, caches),
            Side::After => status_after(node, caches),
        };

        let (glyph, mut style) = status_glyph_and_style(status);
        // "g": the outcome came from a `MultiMapGroup` (the group caches cover every member,
        // matched or leftover). "G": an all-to-all group, where every member is matched.
        let group_index = match side {
            Side::Before => caches.before_group.get(&node.id()),
            Side::After => caches.after_group.get(&node.id()),
        };
        let group_marker = match group_index.and_then(|index| groups.get(*index)) {
            Some(group) if group.pairing == GroupPairing::AllToAll => "G",
            Some(_) => "g",
            None => "",
        };
        let (algo_glyph, disagrees) = algo_diff
            .map(|diff_ast| {
                let algo_status = algo_status(side, node, diff_ast);
                let disagrees = algo_disagrees(side, node, caches, diff_ast);
                let reason_suffix = if show_reason {
                    algo_reason(side, node, diff_ast)
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

        // A pending, uncommitted multi-map selection (`x`).
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

/// Below this width `draw_ui` shows only the focused panel: two half-width panels wrap almost every
/// line. Shared with the main TUI's `DiffViewer`.
pub(crate) const SINGLE_PANEL_WIDTH_THRESHOLD: u16 =
    codediff::tui::components::diff_viewer::SINGLE_PANEL_THRESHOLD;

/// The Before/After panels for a language with no tree-sitter grammar: one row saying why they are
/// empty. Drawn in the panels, not the status line, because two empty panels otherwise read as
/// "nothing matched". The `t` and `T` views still work from the raw text.
fn render_unsupported_language_panels(
    frame: &mut Frame,
    area: Rect,
    single_panel: bool,
    focus: Focus,
) {
    let message = Paragraph::new(NO_GRAMMAR_MESSAGE).wrap(Wrap { trim: false });
    let panel = |title: &str, focused: bool| {
        Block::default()
            .borders(Borders::ALL)
            .title(format!("{title} — no tree-sitter grammar"))
            .border_style(if focused {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            })
    };

    if single_panel {
        // Only the focused panel is on screen at this width; `Tab` swaps sides as with a tree.
        let title = match focus {
            Focus::Before => "Before",
            Focus::After => "After",
        };
        frame.render_widget(message.block(panel(title, true)), area);
        return;
    }

    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    frame.render_widget(
        message
            .clone()
            .block(panel("Before", focus == Focus::Before)),
        panels[0],
    );
    frame.render_widget(
        message.block(panel("After", focus == Focus::After)),
        panels[1],
    );
}

pub(crate) const NO_GRAMMAR_MESSAGE: &str = "<Language not supported by TreeSitter>\n\nThere is \
    no AST for this file pair, so there is no tree mapping to record. Press t to paint the text \
    (which is graded against codediff's own plain-text fallback diff), T for a unix diff, s to \
    save, o to open another case.";

// A params struct would only relocate these fields.
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
    // No grammar, so no nodes (see `FrameState::before_root`).
    text_only: bool,
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

    // `Tab` is how to see the other side in single-panel mode.
    let single_panel = size.width < SINGLE_PANEL_WIDTH_THRESHOLD;

    if text_only {
        render_unsupported_language_panels(frame, chunks[1], single_panel, app.focus);
    } else if single_panel {
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
            &app.mapping.groups,
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
            &app.mapping.groups,
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
            &app.mapping.groups,
        );
    }

    let footer = format!(
        "{}{}{}\nm/M match[+children]  x select for multi-map  X flip all-to-all  c clear selection  f match to EOF  d/D delete[+children]  i/I insert[+children]  a/A align (human/codediff)  p run codediff  r toggle reason  n/N next/prev mismatch  t text view  T unix diff  H hide solved  u unmark  h/l ←/→ collapse/expand  j/k ↑/↓ move  g/G top/bottom  Tab switch  s save  ? help  q quit",
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

/// Like `centered_rect`, but at least `min_width`x`min_height` (capped to `area`). On a small
/// terminal a plain percentage can hide a modal's input line with no sign that anything is cut off.
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
        Modal::ConfirmResetCase {
            entries,
            groups,
            paintings,
        } => render_text_modal(
            frame,
            area,
            "Start this case from scratch?",
            &format!(
                "This throws away everything recorded for this case:\n\n  {entries} mapping entries\n  {groups} multi-map groups\n  {paintings} named paintings\n\nThere is no undo. Nothing is written until you press s, so reopening\nthe case without saving still gets it back.\n\n[y] clear it   [any other key] cancel"
            ),
        ),
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
            pairing,
            kinds,
        } => render_text_modal(
            frame,
            area,
            "Multi-map group has mixed node kinds!",
            &format!(
                "{} Before node(s), {} After node(s), kinds: {}\nWill be recorded as an {} group, {:?}{}.\n\nAre you sure you want to add this group? (y/n)",
                before_ids.len(),
                after_ids.len(),
                kinds.join(", "),
                group_pairing_name(*pairing),
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
            rows,
            selected,
            view,
            name_input,
        } => {
            render_open_sample_picker(frame, area, rows, *selected, view, name_input.as_deref());
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
        Modal::InvariantList { entries, selected } => {
            render_invariant_list(frame, area, entries, *selected);
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
    // +2 for the borders each way, +2 more width for padding. The title counts too: ratatui
    // truncates a title wider than the popup.
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

/// Every painted span on one side, with its verdict (see `HumanTextEntry::verdict`). A malformed
/// entry is skipped rather than failing the render: this view is how a human finds and fixes it.
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

/// From the shared overlay palette, so a painted range looks as it does in the `codediff` TUI.
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

/// What one byte of a row is drawn as. Ordered so `max` picks the winner: the cursor above a
/// selection, a selection above paint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PaintClass {
    Plain,
    Painted(HumanTextVerdict),
    /// Ranked below the live selection, so the one being edited stands out.
    Banked,
    Selected,
    Cursor,
}

/// One side's source as styled lines: painted spans, then the selection, then the cursor on top.
/// Resolved as a per-byte precedence because the three overlap freely; runs are then regrouped by
/// `char_indices`, so a multi-byte character is never split.
pub(crate) fn render_paint_side(
    source: &str,
    spans: &[(HumanTextSpan, HumanTextVerdict)],
    state: &TextPaintState,
    side: usize,
    height: usize,
    width: usize,
) -> Vec<Line<'static>> {
    let selection = state.selection(side, source);
    let banked = &state.pending[side];
    let (cursor_row, cursor_column) = state.cursor[side];
    let focused = state.side == side;
    let top = state.scroll[side];

    // Rows without the CRLF `\r`, as in `TextPaintState::row_text`.
    let lines: Vec<&str> = source
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect();
    let gutter_width = lines.len().to_string().len().max(3);

    // Bucketed by row once; scanning every span per byte dominates render time with many spans.
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

    // No room for content: no wrapping, rather than looping on zero-width chunks.
    let content_width = ScreenColumn::from_raw(width.saturating_sub(gutter_width + 1));

    // Never zero: an empty row still takes a line.
    let wrapped_height = |row: SourceRow| -> usize {
        if content_width.get() == 0 {
            return 1;
        }
        lines
            .get(row.get())
            .map(|line| {
                row_cells_of(line)
                    .get()
                    .div_ceil(content_width.get())
                    .max(1)
            })
            .unwrap_or(1)
    };

    // `scroll_into_view` bounds the cursor in *source* rows, but wrapped rows cost several *screen*
    // rows, so the start row is walked forward until the cursor fits. Display only: `state.scroll`
    // is left alone, so j/k behave the same. `start`/`cursor_row` are source rows, `used`/`height`
    // screen rows.
    let cursor_row = SourceRow::from_raw(cursor_row);
    let mut start = SourceRow::from_raw(top).min(cursor_row);
    if focused && content_width.get() > 0 {
        let mut used: usize = (start.get()..=cursor_row.get())
            .map(|row| wrapped_height(SourceRow::from_raw(row)))
            .sum();
        while used > height && start < cursor_row {
            used -= wrapped_height(start);
            start = SourceRow::from_raw(start.get() + 1);
        }
    }

    let mut out = Vec::with_capacity(height);
    for (row, line) in lines.iter().enumerate().skip(start.get()) {
        if out.len() >= height {
            break;
        }
        let line = *line;
        let mut spans_out: Vec<Span<'static>> = Vec::new();

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
            run.push(display_safe_char(ch));
        }
        push_run(&mut run, run_class, &mut spans_out);

        // A cursor at end of line, or a blank row inside a multi-row span, has no character, so it
        // gets one space. `span_covers` leaves that space plain on a non-empty row: painting past
        // the last character would claim the newline as part of the change.
        let end_class = class_at(row, line.len(), line);
        if end_class != PaintClass::Plain {
            spans_out.push(Span::styled(" ".to_string(), paint_class_style(end_class)));
        }

        // The line number goes on the first screen row only; continuations get a blank gutter.
        let gutter_style = Style::default().fg(Color::DarkGray);
        for (chunk_index, chunk) in wrap_spans(spans_out, content_width).into_iter().enumerate() {
            if out.len() >= height {
                break;
            }
            let gutter = if chunk_index == 0 {
                format!("{:>gutter_width$} ", row + 1)
            } else {
                format!("{:>gutter_width$} ", "")
            };
            let mut screen_row = vec![Span::styled(gutter, gutter_style)];
            screen_row.extend(chunk);
            out.push(Line::from(screen_row));
        }
    }
    out
}

/// Splits one row's styled runs into screen rows of at most `width` columns, keeping each run's
/// style across a split. `width` 0 means no wrapping (no room beside the gutter).
fn wrap_spans(spans: Vec<Span<'static>>, width: ScreenColumn) -> Vec<Vec<Span<'static>>> {
    let width = width.get();
    if width == 0 {
        return vec![spans];
    }
    let mut rows: Vec<Vec<Span<'static>>> = Vec::new();
    let mut current: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;

    for span in spans {
        let style = span.style;
        let owned = span.content.into_owned();
        let mut chunk = String::new();
        for ch in owned.chars() {
            let cells = cell_width_of(ch).get();
            // A wide character that does not fit starts the next row whole; a terminal cannot draw
            // half of one.
            if used + cells > width && !chunk.is_empty() {
                current.push(Span::styled(std::mem::take(&mut chunk), style));
            }
            if used + cells > width {
                rows.push(std::mem::take(&mut current));
                used = 0;
            }
            chunk.push(ch);
            used += cells;
        }
        if !chunk.is_empty() {
            current.push(Span::styled(chunk, style));
        }
    }
    if !current.is_empty() || rows.is_empty() {
        rows.push(current);
    }
    rows
}

pub(crate) fn paint_class_style(class: PaintClass) -> Style {
    match class {
        PaintClass::Plain => Style::default(),
        PaintClass::Painted(verdict) => verdict_style(verdict),
        // Same hue as the live selection, dimmer: banked and selected are stages of one thing.
        PaintClass::Banked => Style::default()
            .bg(overlay_palette().cross_highlight_bg)
            .add_modifier(Modifier::DIM),
        // The TUI's colour for a cursor's counterpart: both mean "the region you point at".
        PaintClass::Selected => {
            let palette = overlay_palette();
            Style::default()
                .bg(palette.cross_highlight_bg)
                .fg(palette.overlay_fg)
        }
        PaintClass::Cursor => Style::default().add_modifier(Modifier::REVERSED),
    }
}

/// Whether `span` covers `(row, column)`; `row_len` bounds a row the span runs past.
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
        // A blank row has no character; `render_paint_side`'s one-space fallback draws it.
        1
    } else {
        // Stop at the row's last real character: a painting never means a line's trailing
        // whitespace or newline.
        row_len
    };
    column >= start && column < end
}

#[allow(clippy::too_many_arguments)]
/// The `t` text-painting modal: both sides' source with painted ranges, and an independent cursor,
/// selection and scroll per side.
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

    let height = popup_area.height.saturating_sub(2) as usize;

    let painted = solution_entries(mapping, solution).len();
    let others = mapping.text_mappings.len().saturating_sub(1);

    // The disagreement overlay needs both sides' human and algo spans together.
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
                "Before [{solution}] {painted} painted{banked} — showing {} (o cycles)",
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
                _ => "After — v sel/i ins/u unmark, n/p diff, a align, s save-as, : jump, Tab, Esc"
                    .to_string(),
            },
        ),
    ] {
        let inner_width = columns[side].width.saturating_sub(2) as usize;
        let lines = render_paint_side(source, &shown[side], state, side, height, inner_width);
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
/// The `s`/`L` solution picker inside the text view.
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

    // The free-form entry is always last; the key handler treats index `names.len()` specially.
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

/// The `T` modal: `diff -u` output, `+`/`-` in the insert/delete colours, `@@` headers highlighted.
pub(crate) fn render_unix_diff_modal(frame: &mut Frame, area: Rect, output: &str, scroll: u16) {
    let popup_area = centered_rect(92, 90, area);
    frame.render_widget(Clear, popup_area);

    // Per-row before/after line numbers tracked across each hunk, so nobody counts from the `@@`
    // header by hand. A deleted line has no after number and an inserted line no before number.
    let mut before_line = 0usize;
    let mut after_line = 0usize;
    let lines: Vec<Line> = output
        .lines()
        .map(|line| {
            let (style, gutter) = if line.starts_with("+++") || line.starts_with("---") {
                (Style::default().add_modifier(Modifier::BOLD), String::new())
            } else if let Some(rest) = line.strip_prefix("@@") {
                (Style::default().fg(Color::Cyan), {
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
                Span::styled(display_safe_str(line), style),
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

/// The `?` help modal (`HELP_TEXT`).
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

/// The `V` popup: every self-contradiction in this case's ground truth, a list above and the
/// selected row's sites below (the messages and site lists are too long for one wide table).
pub(crate) fn render_invariant_list(
    frame: &mut Frame,
    area: Rect,
    entries: &[InvariantEntry],
    selected: usize,
) {
    let popup_area = centered_rect(92, 85, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            "Invariants — {} violation(s), j/k move, Enter jumps, Esc closes",
            entries.len()
        ))
        .border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Bounded, so one violation with many sites cannot push the list off screen.
    let details = entries
        .get(selected)
        .map(|entry| entry.details.len())
        .unwrap_or(0);
    let detail_height = (details as u16 + 2).clamp(3, inner.height.saturating_sub(3).max(3));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(detail_height)])
        .split(inner);

    let rows: Vec<Row> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let style = if index == selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            Row::new(vec![
                Cell::from(entry.violation.invariant.to_string()),
                Cell::from(entry.violation.painting.clone().unwrap_or_else(|| {
                    // The rules that read only the tree mapping have no painting to name.
                    "(mapping)".to_string()
                })),
                Cell::from(entry.violation.message.clone()),
            ])
            .style(style)
        })
        .collect();

    // Scroll derives from `selected` each frame, as in the open pickers: nothing to keep in sync.
    let height = chunks[0].height.saturating_sub(1).max(1) as usize;
    let offset = selected
        .saturating_sub(height / 2)
        .min(entries.len().saturating_sub(height));
    let visible: Vec<Row> = rows.into_iter().skip(offset).collect();

    frame.render_widget(
        Table::new(
            visible,
            [
                Constraint::Length(3),
                Constraint::Length(18),
                Constraint::Min(20),
            ],
        )
        .header(
            Row::new(vec!["#", "Painting", "What it says"])
                .style(Style::default().add_modifier(Modifier::BOLD)),
        ),
        chunks[0],
    );

    let detail = entries
        .get(selected)
        .map(|entry| {
            if entry.details.is_empty() {
                "(this violation names no place to jump to)".to_string()
            } else {
                entry.details.join("\n")
            }
        })
        .unwrap_or_default();
    frame.render_widget(
        Paragraph::new(detail).block(
            Block::default()
                .borders(Borders::TOP)
                .title("Where — Enter puts both trees and both text panels here"),
        ),
        chunks[1],
    );
}

/// The `o` picker: one row per case and one column per `DiffColumn`. The header shows the cursor
/// column in reverse video, the sort with `^`/`v`, and filtered columns with `*`. The view
/// (`visible_diff_options`) and scroll are recomputed from `options`/`view`/`selected` each frame,
/// never stored. `name_input` is `Some` while `f` on `Name` is prompting; the table keeps the old
/// filter until Enter.
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

    // The selected row's note gets a strip at the bottom; the table only has room for a marker.
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

    // From `table_area`, not `popup_area`, or the footer hides the selected row. +1 for the header.
    let inner_height = table_area.height.saturating_sub(3) as usize;
    let max_scroll = visible.len().saturating_sub(inner_height);
    let scroll = selected.saturating_sub(inner_height / 2).min(max_scroll);

    // Datasets are looked up per visible row from `options`.
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
            let invariant_cell = match data.invariants_of(name) {
                Some(count) => count.to_string(),
                None => "?".to_string(),
            };
            let size_cell = match data.size_of(name) {
                Some(lines) => lines.to_string(),
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
                Cell::from(invariant_cell),
                Cell::from(size_cell),
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
            // Abbreviated so the filter list on the left survives ratatui's truncation.
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

    // In `DiffColumn::ALL` order, each wide enough for its header, sort arrow and filter marker.
    let table = Table::new(
        rows,
        [
            Constraint::Min(20),
            Constraint::Length(9),
            Constraint::Length(6),
            Constraint::Length(10),
            Constraint::Length(7),
            Constraint::Length(10),
            Constraint::Length(11),
            Constraint::Length(7),
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

/// The `O` picker: one row per sample, one column per `SampleColumn`, with the same header markers
/// and recompute-every-frame contract as [`render_open_diff_picker`].
pub(crate) fn render_open_sample_picker(
    frame: &mut Frame,
    area: Rect,
    rows: &[SampleRow],
    selected: usize,
    view: &SamplePickerView,
    name_input: Option<&str>,
) {
    let visible = visible_sample_rows(rows, view);

    let popup_area = centered_rect(80, 70, area);
    frame.render_widget(Clear, popup_area);

    let inner_height = popup_area.height.saturating_sub(3) as usize;
    let max_scroll = visible.len().saturating_sub(inner_height);
    let scroll = selected.saturating_sub(inner_height / 2).min(max_scroll);

    let table_rows: Vec<Row> = visible
        .iter()
        .enumerate()
        .skip(scroll)
        .take(inner_height.max(1))
        .map(|(i, row)| {
            let style = if i == selected {
                Style::default().bg(Color::Yellow).fg(Color::Black)
            } else {
                Style::default()
            };
            Row::new(vec![
                Cell::from(row.name.clone()),
                Cell::from(row.language.clone()),
                // `?` for an unknown stratum, as in the `o` picker; never filtered away.
                Cell::from(row.bucket.clone().unwrap_or_else(|| "?".to_string())),
                Cell::from(row.status.label()),
                Cell::from(row.size.to_string()),
            ])
            .style(style)
        })
        .collect();

    let header = Row::new(SampleColumn::ALL.map(|column| {
        let mut label = column.label().to_string();
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
        format!(
            // Abbreviated like the `o` picker's title.
            "Open sample [{}] sort:{}{} ({}/{}) — h/l col, j/k row, s sort, f filter, Esc",
            if view.filters.any_active() {
                view.filters.describe()
            } else {
                "no filters".to_string()
            },
            view.sort.column.label(),
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

    // In `SampleColumn::ALL` order, each wide enough for its header, sort arrow and filter marker.
    let table = Table::new(
        table_rows,
        [
            Constraint::Min(20),
            Constraint::Length(12),
            Constraint::Length(11),
            Constraint::Length(10),
            Constraint::Length(7),
        ],
    )
    .header(header)
    .block(block);

    frame.render_widget(table, popup_area);
}

/// The `C` picker's first step: a commit from this repository's `git log`, newest first.
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

/// The `C` picker's second step: one of the commit's files in a supported language.
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
