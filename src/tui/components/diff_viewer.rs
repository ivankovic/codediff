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
use std::path::PathBuf;

use anyhow::Result;
use ratatui::{prelude::*, text::Line, widgets::Paragraph};
use tokio::sync::mpsc::UnboundedSender;

use super::{Component, code_viewer::CodeViewer};
use crate::diff::text::{RangeMatch, RenderOptions, TextOperation, ranges_for_options};
use crate::tui::actions::{Action, DiffSessionData};
use crate::tui::theme::{OverlayTheme, PanelLayout};

/// Below this terminal width two side-by-side panels are too narrow to read, so `Auto` layout
/// shows one panel.
pub const SINGLE_PANEL_THRESHOLD: u16 = 220;

/// The TUI's central content pane: the before/after files side by side (or, under
/// `DisplayMode::Single`, one at a time), each half owned by its own [`CodeViewer`].
#[derive(Default)]
pub struct DiffViewer {
    /// The Before side's viewer.
    left_viewer: CodeViewer,
    /// The After side's viewer.
    right_viewer: CodeViewer,
    command_tx: Option<UnboundedSender<Action>>,
    display_mode: DisplayMode,
    /// The persisted `v`-key preference; `Auto` means the width decides.
    layout_override: PanelLayout,
    /// The panel shown in single mode and whose cursor drives navigation in dual mode.
    active_panel: Panel,
    /// Kept here for the minimap strips, which this component draws.
    overlay_theme: OverlayTheme,
    /// Content rects from the last `draw`, for mouse hit-testing; `None` when not on screen.
    last_before_content: Option<Rect>,
    last_after_content: Option<Rect>,

    render_options: RenderOptions,
    /// Unfiltered ranges and sources from the last `load_diff`, per side (0 = before), so
    /// `set_render_options` can re-filter without the cursor reset a reload does.
    full_ranges: [Vec<RangeMatch>; 2],
    sources: [String; 2],
}

/// Both panels side by side, or only `active_panel`'s.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
enum DisplayMode {
    #[default]
    Dual,
    Single,
}

/// A `(row, column)` position, in whichever file's coordinates the context implies.
type Position = (usize, usize);

/// One stop in the merged `n`/`p` walk: which panel it is on, and where.
type ChangeStop = (Panel, Position);

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Panel {
    #[default]
    Before,
    After,
}

impl DiffViewer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load a completed diff and put the cursor on its first change.
    pub fn load_diff(&mut self, data: &DiffSessionData) {
        self.left_viewer
            .load_contents(data.before_path.clone(), data.before_contents.clone());
        self.right_viewer
            .load_contents(data.after_path.clone(), data.after_contents.clone());
        self.full_ranges = [data.before_ranges.clone(), data.after_ranges.clone()];
        self.sources = [data.before_contents.clone(), data.after_contents.clone()];
        self.apply_render_options(true);
        self.active_panel = Panel::Before;
        self.sync_focus();
        self.sync_cross_highlight();
    }

    /// Switch how much of the diff is painted, keeping the cursor and scroll where they are.
    /// A plain re-filter of the last `load_diff`'s ranges; persisting is the caller's job.
    pub fn set_render_options(&mut self, options: RenderOptions) {
        self.render_options = options;
        self.apply_render_options(false);
        self.sync_cross_highlight();
    }

    pub fn render_options(&self) -> RenderOptions {
        self.render_options
    }

    /// `reset_cursor`: a new diff jumps to its first change; a re-filter stays put.
    fn apply_render_options(&mut self, reset_cursor: bool) {
        for (viewer, side) in [(&mut self.left_viewer, 0), (&mut self.right_viewer, 1)] {
            let ranges = ranges_for_options(
                &self.full_ranges[side],
                &self.sources[side],
                self.render_options,
            );
            if reset_cursor {
                viewer.set_ranges(ranges);
            } else {
                viewer.replace_ranges(ranges);
            }
        }
    }

    /// Move the focused cursor one line; the other panel follows its counterpart.
    pub fn move_cursor_vertical(&mut self, direction: i32) {
        self.focused_viewer().move_cursor_vertical(direction);
        self.sync_cross_highlight();
        self.sync_scroll();
    }

    /// Move the focused cursor one character; the other panel follows its counterpart.
    pub fn move_cursor_horizontal(&mut self, direction: i32) {
        self.focused_viewer().move_cursor_horizontal(direction);
        self.sync_cross_highlight();
        self.sync_scroll();
    }

    /// Every change in the pair, once each, as one walk across both panels in reading order: a
    /// per-panel walk reads in no natural order and leaves a pure insertion's before panel with
    /// nowhere to go.
    ///
    /// Paired changes and deletes stop on the before side (the cross-highlight shows the other
    /// half); an `Insert` stops on the after side. Sorted in before-file coordinates, an insertion
    /// by its zero-width `destination` placeholder.
    fn change_stops(&self) -> Vec<ChangeStop> {
        let mut stops: Vec<(Position, ChangeStop)> = Vec::new();

        let interesting = |range_match: &RangeMatch| {
            !matches!(
                range_match.operation,
                TextOperation::Identical | TextOperation::NotYetSet
            ) && !range_match.source.is_empty()
        };

        for range_match in self.left_viewer.ranges().iter().filter(|r| interesting(r)) {
            let at = (
                range_match.source.start_row,
                range_match.source.start_column,
            );
            stops.push((at, (Panel::Before, at)));
        }
        for range_match in self.right_viewer.ranges().iter().filter(|r| interesting(r)) {
            if !range_match.destination.is_empty() {
                continue;
            }
            let at = (
                range_match.source.start_row,
                range_match.source.start_column,
            );
            let key = (
                range_match.destination.start_row,
                range_match.destination.start_column,
            );
            stops.push((key, (Panel::After, at)));
        }

        // At equal keys the deletion reads before the insertion replacing it; the stable sort
        // keeps document order within a side.
        stops.sort_by_key(|(key, (panel, _))| (*key, *panel == Panel::After));
        stops.dedup_by_key(|(key, stop)| (*key, *stop));
        stops.into_iter().map(|(_, stop)| stop).collect()
    }

    /// Where the cursor currently sits in `change_stops`' ordering, if it is on a stop.
    fn current_stop_index(&self, stops: &[ChangeStop]) -> Option<usize> {
        let cursor = self.focused_cursor_position()?;
        stops
            .iter()
            .position(|&(panel, at)| panel == self.active_panel && at == cursor)
    }

    /// `n`/`p`: step to the next or previous change in the merged walk, switching panels as
    /// needed. Wraps at both ends; off a stop, it takes the nearest one in the direction of travel.
    pub fn jump_to_change(&mut self, forward: bool) {
        let stops = self.change_stops();
        if stops.is_empty() {
            return;
        }
        let next = match self.current_stop_index(&stops) {
            Some(index) if forward => (index + 1) % stops.len(),
            Some(index) => (index + stops.len() - 1) % stops.len(),
            None => {
                let after = self.stops_before_cursor(&stops);
                if forward {
                    after % stops.len()
                } else {
                    (after + stops.len() - 1) % stops.len()
                }
            }
        };
        let (panel, (row, column)) = stops[next];
        if self.active_panel != panel {
            self.toggle_active_panel();
        }
        self.focused_viewer().set_cursor_position(row, column);
        self.focused_viewer().scroll_to_center_row(row);
        self.sync_cross_highlight();
        self.sync_scroll_centered();
    }

    /// Off a stop, how many of `change_stops` come before the cursor: the index of the stop `n`
    /// takes next, `stops.len()` past the last one.
    fn stops_before_cursor(&self, stops: &[ChangeStop]) -> usize {
        let cursor = self.focused_cursor_position().unwrap_or((0, 0));
        let here = (self.active_panel, cursor);
        stops
            .iter()
            .position(|&(panel, at)| (panel, at) > here)
            .unwrap_or(stops.len())
    }

    /// `(1-based index, total)` over the merged `change_stops` walk, for the footer's
    /// `change N/M`; `None` when there are no changes. Counted in the order `n` walks, so it
    /// climbs by one per `n` even across a panel switch; off a stop it is the last stop passed.
    pub fn merged_change_count_and_index(&self) -> Option<(usize, usize)> {
        let stops = self.change_stops();
        if stops.is_empty() {
            return None;
        }
        self.focused_cursor_position()?;
        let index = match self.current_stop_index(&stops) {
            Some(index) => index + 1,
            None => self.stops_before_cursor(&stops),
        };
        Some((index.max(1), stops.len()))
    }

    /// `Ctrl-d`/`Ctrl-u`: half a viewport, built from single-line moves so the sticky column
    /// behaves as a run of `j`/`k` would.
    pub fn move_cursor_half_page(&mut self, direction: i32) {
        let half = (self.focused_viewer().viewport_height() / 2).max(1);
        for _ in 0..half {
            self.focused_viewer().move_cursor_vertical(direction);
        }
        self.sync_cross_highlight();
        self.sync_scroll();
    }

    /// `Ctrl-e`/`Ctrl-y`: scroll one line without moving the cursor, both panels in dual mode.
    pub fn scroll_view(&mut self, direction: i32) {
        let scroll_one = |viewer: &mut CodeViewer| {
            if direction < 0 {
                viewer.scroll_up();
            } else {
                viewer.scroll_down();
            }
        };
        if self.display_mode == DisplayMode::Dual {
            scroll_one(&mut self.left_viewer);
            scroll_one(&mut self.right_viewer);
        } else {
            scroll_one(self.focused_viewer());
        }
    }

    /// `Enter`: switch panels and put the cursor on the counterpart's start; a second `Enter`
    /// jumps back. A no-op when the cursor is on no range.
    pub fn jump_to_counterpart(&mut self) {
        let Some(destination) = self.focused_viewer().cursor_destination() else {
            return;
        };
        self.toggle_active_panel();
        self.focused_viewer()
            .set_cursor_position(destination.start_row, destination.start_column);
        self.sync_cross_highlight();
        self.sync_scroll();
    }

    /// `S`: toggle syntax highlighting on both panels at once.
    pub fn toggle_syntax_highlighting(&mut self) {
        let enable = !self.left_viewer.is_syntax_highlighting_enabled();
        self.left_viewer.set_syntax_highlighting(enable);
        self.right_viewer.set_syntax_highlighting(enable);
    }

    pub fn set_syntax_theme(&mut self, name: String) {
        self.left_viewer.set_syntax_theme(name.clone());
        self.right_viewer.set_syntax_theme(name);
    }

    /// `H`: toggle the node highlight on both panels (it is one signal spanning both) and
    /// persist it, since it ships off.
    pub fn toggle_node_highlight(&mut self) {
        let enable = !self.left_viewer.is_node_highlight_enabled();
        self.set_node_highlight(enable);
        crate::tui::theme::save_node_highlight(enable);
    }

    /// Apply the node-highlight setting to both panels without persisting.
    pub fn set_node_highlight(&mut self, enable: bool) {
        self.left_viewer.set_node_highlight(enable);
        self.right_viewer.set_node_highlight(enable);
    }

    /// Move the focused cursor to the start of 1-indexed `line`, clamped and centered.
    pub fn jump_to_line(&mut self, line: usize) {
        let row = line.saturating_sub(1);
        self.focused_viewer().set_cursor_position(row, 0);
        self.focused_viewer().scroll_to_center_row(row);
        self.sync_cross_highlight();
        self.sync_scroll_centered();
    }

    /// Re-activate `panel` and put its cursor at `(row, col)`, clamped, after a reload.
    pub fn restore_cursor(&mut self, panel: Panel, row: usize, col: usize) {
        if self.active_panel != panel {
            self.toggle_active_panel();
        }
        self.focused_viewer().set_cursor_position(row, col);
        self.sync_cross_highlight();
        self.sync_scroll();
    }

    /// Highlight `query`'s matches on the focused panel without moving the cursor; returns the
    /// match count. An empty query clears the preview.
    pub fn preview_search(&mut self, query: &str) -> usize {
        self.focused_viewer().preview_search(query)
    }

    /// Search the focused panel for `query`, replacing any previous search, and jump to the
    /// nearest match.
    pub fn search(&mut self, query: &str) {
        self.focused_viewer().search(query);
        self.sync_cross_highlight();
        self.sync_scroll();
    }

    /// `>`/`<`: the focused panel's next or previous search match.
    pub fn jump_to_search_match(&mut self, forward: bool) {
        self.focused_viewer().jump_to_search_match(forward);
        self.sync_cross_highlight();
        self.sync_scroll();
    }

    /// The focused panel's `(matches at or before the cursor, total)`, or `None` with no search.
    pub fn focused_search_match_count_and_index(&self) -> Option<(usize, usize)> {
        let viewer = match self.active_panel {
            Panel::Before => &self.left_viewer,
            Panel::After => &self.right_viewer,
        };
        viewer.search_match_count_and_index()
    }

    /// Call after anything that moves the cursor or changes focus. The other cursor always
    /// follows the counterpart; the highlight is suppressed on an `Identical` match (see
    /// `cursor_destination_for_highlight`).
    fn sync_cross_highlight(&mut self) {
        let destination = self.focused_viewer().cursor_destination();
        let highlight_destination = self.focused_viewer().cursor_destination_for_highlight();
        self.other_viewer()
            .set_highlight_destination(highlight_destination);

        if let Some(dest_range) = destination {
            self.other_viewer()
                .set_cursor_position(dest_range.start_row, dest_range.start_column);
        }
    }

    /// Keep the counterpart row visible in the other panel.
    fn sync_scroll(&mut self) {
        if let Some(dest) = self.focused_viewer().cursor_destination() {
            self.other_viewer().scroll_to_show_row(dest.start_row);
        }
    }

    /// Like `sync_scroll`, but centers the counterpart row.
    fn sync_scroll_centered(&mut self) {
        if let Some(dest) = self.focused_viewer().cursor_destination() {
            self.other_viewer().scroll_to_center_row(dest.start_row);
        }
    }

    /// Mark exactly `active_panel` focused; call whenever it changes.
    fn sync_focus(&mut self) {
        self.left_viewer
            .set_focused(self.active_panel == Panel::Before);
        self.right_viewer
            .set_focused(self.active_panel == Panel::After);
    }

    pub fn overlay_theme(&self) -> OverlayTheme {
        self.overlay_theme
    }

    pub fn set_overlay_theme(&mut self, theme: OverlayTheme) {
        self.overlay_theme = theme;
        self.left_viewer.set_overlay_theme(theme);
        self.right_viewer.set_overlay_theme(theme);
    }

    fn viewer_for(&mut self, panel: Panel) -> &mut CodeViewer {
        match panel {
            Panel::Before => &mut self.left_viewer,
            Panel::After => &mut self.right_viewer,
        }
    }

    /// The panel and content rect under screen position `(column, row)`, per the last `draw`.
    fn panel_at(&self, column: u16, row: u16) -> Option<(Panel, Rect)> {
        let hit = |rect: Option<Rect>| {
            rect.filter(|r| {
                column >= r.x && column < r.x + r.width && row >= r.y && row < r.y + r.height
            })
        };
        if let Some(rect) = hit(self.last_before_content) {
            return Some((Panel::Before, rect));
        }
        if let Some(rect) = hit(self.last_after_content) {
            return Some((Panel::After, rect));
        }
        None
    }

    fn focused_viewer(&mut self) -> &mut CodeViewer {
        match self.active_panel {
            Panel::Before => &mut self.left_viewer,
            Panel::After => &mut self.right_viewer,
        }
    }

    fn other_viewer(&mut self) -> &mut CodeViewer {
        match self.active_panel {
            Panel::Before => &mut self.right_viewer,
            Panel::After => &mut self.left_viewer,
        }
    }

    pub fn active_panel(&self) -> Panel {
        self.active_panel
    }

    /// The focused cursor's 0-indexed `(row, byte column)`, the unit of the ranges it is
    /// compared with, or `None` with no file loaded.
    pub fn focused_cursor_position(&self) -> Option<(usize, usize)> {
        let viewer = self.focused_viewer_ref()?;
        let state = viewer.state();
        Some((state.cursor_row, state.cursor_col))
    }

    /// The focused cursor's 0-indexed `(row, character column)`, for the footer's `Ln`/`Col`,
    /// or `None` with no file loaded.
    pub fn focused_cursor_character_position(&self) -> Option<(usize, usize)> {
        Some(self.focused_viewer_ref()?.cursor_character_position())
    }

    /// The focused viewer, or `None` with no file loaded.
    fn focused_viewer_ref(&self) -> Option<&CodeViewer> {
        let viewer = match self.active_panel {
            Panel::Before => &self.left_viewer,
            Panel::After => &self.right_viewer,
        };
        (viewer.line_count() > 0).then_some(viewer)
    }

    /// Load a single file, with no diff overlay, into the Before panel.
    pub fn set_before_file(&mut self, path: PathBuf) -> Result<()> {
        self.left_viewer.load_file(path)
    }

    /// Load a single file, with no diff overlay, into the After panel.
    pub fn set_after_file(&mut self, path: PathBuf) -> Result<()> {
        self.right_viewer.load_file(path)
    }

    pub fn update_display_mode(&mut self, width: u16) {
        self.display_mode = match self.layout_override {
            PanelLayout::Dual => DisplayMode::Dual,
            PanelLayout::Single => DisplayMode::Single,
            PanelLayout::Auto => {
                if width < SINGLE_PANEL_THRESHOLD {
                    DisplayMode::Single
                } else {
                    DisplayMode::Dual
                }
            }
        };
    }

    /// Takes effect on the next frame.
    pub fn set_layout_override(&mut self, layout: PanelLayout) {
        self.layout_override = layout;
    }

    pub fn layout_override(&self) -> PanelLayout {
        self.layout_override
    }

    /// `v`: cycle `Auto -> Dual -> Single` and persist.
    fn cycle_layout_override(&mut self) {
        self.layout_override = self.layout_override.next();
        crate::tui::theme::save_panel_layout(self.layout_override);
    }

    pub fn toggle_active_panel(&mut self) {
        self.active_panel = match self.active_panel {
            Panel::Before => Panel::After,
            Panel::After => Panel::Before,
        };
        self.sync_focus();
    }

    fn active_filename(&self) -> String {
        match self.active_panel {
            Panel::Before => self.left_viewer.filename_or_hint(),
            Panel::After => self.right_viewer.filename_or_hint(),
        }
    }

    fn active_language(&self) -> String {
        match self.active_panel {
            Panel::Before => self.left_viewer.language_name(),
            Panel::After => self.right_viewer.language_name(),
        }
    }
}

impl Component for DiffViewer {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.command_tx = Some(tx.clone());
        self.left_viewer.register_action_handler(tx.clone())?;
        self.right_viewer.register_action_handler(tx)?;
        Ok(())
    }

    fn init(&mut self, area: Rect) -> Result<()> {
        self.update_display_mode(area.width);

        if self.display_mode == DisplayMode::Dual {
            let (left_area, right_area) = split_panels(area);

            self.left_viewer.init(left_area)?;
            self.right_viewer.init(right_area)?;
        } else {
            self.left_viewer.init(area)?;
            self.right_viewer.init(area)?;
        }

        Ok(())
    }

    fn handle_key_event(&mut self, key: crossterm::event::KeyEvent) -> Result<Option<Action>> {
        use crossterm::event::KeyModifiers;
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            // Before the plain-letter arms, so a Ctrl chord never falls through to one.
            crossterm::event::KeyCode::Char('d') if ctrl => {
                self.move_cursor_half_page(1);
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Char('u') if ctrl => {
                self.move_cursor_half_page(-1);
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Char('e') if ctrl => {
                self.scroll_view(1);
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Char('y') if ctrl => {
                self.scroll_view(-1);
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Enter => {
                self.jump_to_counterpart();
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Char('v') => {
                self.cycle_layout_override();
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Char('S') => {
                self.toggle_syntax_highlighting();
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Char('H') => {
                self.toggle_node_highlight();
                Ok(Some(Action::Render))
            }
            // Keys that open a dialog (`M`, `c`, `o`, `?`, `/`) are handled by `App`, not here.
            crossterm::event::KeyCode::Tab => {
                self.toggle_active_panel();
                self.sync_cross_highlight();
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Up | crossterm::event::KeyCode::Char('k') => {
                self.move_cursor_vertical(-1);
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Down | crossterm::event::KeyCode::Char('j') => {
                self.move_cursor_vertical(1);
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Left | crossterm::event::KeyCode::Char('h') => {
                self.move_cursor_horizontal(-1);
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Right | crossterm::event::KeyCode::Char('l') => {
                self.move_cursor_horizontal(1);
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Char('n') => {
                self.jump_to_change(true);
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Char('p') => {
                self.jump_to_change(false);
                Ok(Some(Action::Render))
            }
            // Not overloaded onto n/p, which always mean change navigation.
            crossterm::event::KeyCode::Char('>') => {
                self.jump_to_search_match(true);
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Char('<') => {
                self.jump_to_search_match(false);
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::PageUp => {
                if self.display_mode == DisplayMode::Dual {
                    let left_lines = self.left_viewer.viewport_height();
                    let right_lines = self.right_viewer.viewport_height();
                    for _ in 0..left_lines {
                        self.left_viewer.scroll_up();
                    }
                    for _ in 0..right_lines {
                        self.right_viewer.scroll_up();
                    }
                } else {
                    let lines = self.focused_viewer().viewport_height();
                    for _ in 0..lines {
                        self.focused_viewer().scroll_up();
                    }
                }
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::PageDown => {
                if self.display_mode == DisplayMode::Dual {
                    let left_lines = self.left_viewer.viewport_height();
                    let right_lines = self.right_viewer.viewport_height();
                    for _ in 0..left_lines {
                        self.left_viewer.scroll_down();
                    }
                    for _ in 0..right_lines {
                        self.right_viewer.scroll_down();
                    }
                } else {
                    let lines = self.focused_viewer().viewport_height();
                    for _ in 0..lines {
                        self.focused_viewer().scroll_down();
                    }
                }
                Ok(Some(Action::Render))
            }
            // The cursor moves too, as in an editor: scrolling alone left it off screen, and the
            // next `j`/`k` snapped the view back to it.
            // In dual mode the other panel shows its own top or bottom too, since the last line
            // often has no counterpart for it to follow.
            crossterm::event::KeyCode::Home => {
                self.jump_to_line(1);
                if self.display_mode == DisplayMode::Dual {
                    self.left_viewer.scroll_to_center_row(0);
                    self.right_viewer.scroll_to_center_row(0);
                }
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::End => {
                let last_line = self.focused_viewer().line_count();
                self.jump_to_line(last_line);
                if self.display_mode == DisplayMode::Dual {
                    let (left, right) = (
                        self.left_viewer.line_count(),
                        self.right_viewer.line_count(),
                    );
                    self.left_viewer.scroll_to_center_row(left);
                    self.right_viewer.scroll_to_center_row(right);
                }
                Ok(Some(Action::Render))
            }
            _ => Ok(None),
        }
    }

    /// The wheel scrolls the panel under the pointer 3 lines a notch; a left click focuses it and
    /// puts the cursor on the clicked character. Hit-tests against the rects `draw` recorded.
    fn handle_mouse_event(
        &mut self,
        mouse: crossterm::event::MouseEvent,
    ) -> Result<Option<Action>> {
        use crossterm::event::{MouseButton, MouseEventKind};
        let Some((panel, rect)) = self.panel_at(mouse.column, mouse.row) else {
            return Ok(None);
        };
        match mouse.kind {
            MouseEventKind::ScrollDown => {
                for _ in 0..3 {
                    self.viewer_for(panel).scroll_down();
                }
                Ok(Some(Action::Render))
            }
            MouseEventKind::ScrollUp => {
                for _ in 0..3 {
                    self.viewer_for(panel).scroll_up();
                }
                Ok(Some(Action::Render))
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if self.active_panel != panel {
                    self.toggle_active_panel();
                }
                let viewer = self.viewer_for(panel);
                let row = viewer.state().scroll + (mouse.row - rect.y) as usize;
                let clicked_col = (mouse.column - rect.x) as usize;
                let display_col =
                    viewer.state().scroll_col + clicked_col.saturating_sub(viewer.gutter_width());
                viewer.set_cursor_at_display_col(row, display_col);
                self.sync_cross_highlight();
                self.sync_scroll();
                Ok(Some(Action::Render))
            }
            _ => Ok(None),
        }
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match &action {
            Action::Resize(w, _h) => {
                self.update_display_mode(*w);
            }
            Action::DiffReady(data) => self.load_diff(data),
            _ => {}
        }

        let _ = self.left_viewer.update(action.clone())?;
        let _ = self.right_viewer.update(action)?;

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        self.update_display_mode(area.width);

        // Both modes draw exactly one borderless title row (see `panel_title`).
        let viewport_height = area.height.saturating_sub(1) as usize;
        self.left_viewer.set_viewport_height(viewport_height);
        self.right_viewer.set_viewport_height(viewport_height);

        if self.display_mode == DisplayMode::Dual {
            let (left_area, right_area) = split_panels(area);

            let left_filename = self.left_viewer.filename_or_hint();
            let left_below_title = panel_title(
                frame,
                left_area,
                " Before ",
                self.overlay_theme.palette().before_title_fg,
                &left_filename,
                self.active_panel == Panel::Before,
            );
            let (left_content, left_strip) =
                carve_minimap(left_below_title, self.left_viewer.line_count());
            let left_bands = self
                .left_viewer
                .change_bands(left_strip.map_or(0, |s| s.height as usize));
            self.left_viewer.draw(frame, left_content)?;
            if let Some(strip) = left_strip {
                render_minimap(frame, strip, &left_bands, self.overlay_theme);
            }

            let right_filename = self.right_viewer.filename_or_hint();
            let right_below_title = panel_title(
                frame,
                right_area,
                " After ",
                self.overlay_theme.palette().after_title_fg,
                &right_filename,
                self.active_panel == Panel::After,
            );
            let (right_content, right_strip) =
                carve_minimap(right_below_title, self.right_viewer.line_count());
            let right_bands = self
                .right_viewer
                .change_bands(right_strip.map_or(0, |s| s.height as usize));
            self.right_viewer.draw(frame, right_content)?;
            if let Some(strip) = right_strip {
                render_minimap(frame, strip, &right_bands, self.overlay_theme);
            }

            self.last_before_content = Some(left_content);
            self.last_after_content = Some(right_content);

            let focused_content = match self.active_panel {
                Panel::Before => left_content,
                Panel::After => right_content,
            };
            if let Some((x, y)) = self
                .focused_viewer()
                .cursor_screen_position(focused_content)
            {
                frame.set_cursor_position((x, y));
            }
        } else {
            let palette = self.overlay_theme.palette();
            let title_color = match self.active_panel {
                Panel::Before => palette.before_title_fg,
                Panel::After => palette.after_title_fg,
            };

            let panel_name = match self.active_panel {
                Panel::Before => " Before ",
                Panel::After => " After ",
            };

            let filename = self.active_filename();
            let language = self.active_language();

            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(0)])
                .split(area);
            let (title_area, below_title) = (layout[0], layout[1]);

            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(panel_name, Style::new().bold().fg(title_color)),
                    Span::raw(" - "),
                    Span::styled(&filename, Style::new().bold().fg(Color::Cyan)),
                    Span::raw(" - "),
                    Span::styled(&language, Style::new().fg(Color::Gray)),
                    Span::raw(" (Tab to switch)"),
                ])),
                title_area,
            );
            let line_count = self.focused_viewer().line_count();
            let (content_area, strip) = carve_minimap(below_title, line_count);
            let bands = self
                .focused_viewer()
                .change_bands(strip.map_or(0, |s| s.height as usize));
            self.focused_viewer().draw(frame, content_area)?;
            if let Some(strip) = strip {
                render_minimap(frame, strip, &bands, self.overlay_theme);
            }

            // Only the visible panel is clickable; the hidden one must not keep a stale rect.
            match self.active_panel {
                Panel::Before => {
                    self.last_before_content = Some(content_area);
                    self.last_after_content = None;
                }
                Panel::After => {
                    self.last_after_content = Some(content_area);
                    self.last_before_content = None;
                }
            }

            if let Some((x, y)) = self.focused_viewer().cursor_screen_position(content_area) {
                frame.set_cursor_position((x, y));
            }
        }

        Ok(())
    }
}

/// Splits off the rightmost column as the change minimap: `(content, Some(strip))`, or
/// `(area, None)` with no file loaded or no room.
fn carve_minimap(area: Rect, line_count: usize) -> (Rect, Option<Rect>) {
    if line_count == 0 || area.width < 2 {
        return (area, None);
    }
    let content = Rect::new(area.x, area.y, area.width - 1, area.height);
    let strip = Rect::new(area.x + area.width - 1, area.y, 1, area.height);
    (content, Some(strip))
}

/// Draws the minimap: one cell per `CodeViewer::change_bands` band, in the overlay's background
/// color for its operation, blank where there is no change.
fn render_minimap(
    frame: &mut Frame,
    strip: Rect,
    bands: &[Option<crate::diff::text::TextOperation>],
    theme: OverlayTheme,
) {
    let palette = theme.palette();
    let lines: Vec<Line> = bands
        .iter()
        .map(|band| match band {
            Some(op) => match palette.background_for(op) {
                Some(color) => Line::from(Span::styled("█", Style::new().fg(color))),
                None => Line::from(" "),
            },
            None => Line::from(" "),
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), strip);
}

/// Left and right halves with a one-column divider gap; shared by `init` and `draw` so their
/// layouts cannot diverge.
fn split_panels(area: Rect) -> (Rect, Rect) {
    let divider = area.width / 2;
    let left_area = Rect::new(area.x, area.y, divider, area.height);
    let right_area = Rect::new(
        area.x + divider + 1,
        area.y,
        area.width - divider - 1,
        area.height,
    );
    (left_area, right_area)
}

/// Draws a dual-mode panel's borderless title line, highlighted on the active side, and returns
/// the area below it. No `Block` borders anywhere: nested titled borders show the filename twice.
fn panel_title(
    frame: &mut Frame,
    area: Rect,
    name: &'static str,
    color: Color,
    filename: &str,
    active: bool,
) -> Rect {
    let title_style = if active {
        Style::new().bold().fg(Color::Black).bg(color)
    } else {
        Style::new().bold().fg(color)
    };
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(name, title_style),
            Span::raw(format!(" {filename}")),
        ])),
        layout[0],
    );
    layout[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_diff_data() -> DiffSessionData {
        DiffSessionData {
            before_path: PathBuf::from("before.txt"),
            after_path: PathBuf::from("after.txt"),
            before_contents: "before".to_string(),
            after_contents: "after".to_string(),
            before_ranges: Vec::new(),
            after_ranges: Vec::new(),
            comment_only: false,
            plain_text_fallback: false,
        }
    }

    #[test]
    fn switching_render_options_keeps_the_cursor_where_it_is() {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&sample_diff_data());

        for _ in 0..3 {
            viewer.move_cursor_vertical(1);
        }
        let before = viewer.focused_cursor_position();
        assert!(before.is_some());

        viewer.set_render_options(RenderOptions::MINIMAL);

        assert_eq!(
            viewer.focused_cursor_position(),
            before,
            "the cursor should survive a mode switch"
        );
    }

    #[test]
    fn loading_a_diff_still_jumps_to_the_first_change() {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&sample_diff_data());
        let at_load = viewer.focused_cursor_position();

        for _ in 0..3 {
            viewer.move_cursor_vertical(1);
        }
        viewer.load_diff(&sample_diff_data());

        assert_eq!(viewer.focused_cursor_position(), at_load);
    }

    #[test]
    fn tab_moves_focus_exclusively_to_the_other_panel() {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&sample_diff_data());

        assert!(viewer.left_viewer.state().is_focused);
        assert!(!viewer.right_viewer.state().is_focused);

        viewer.toggle_active_panel();

        assert!(
            !viewer.left_viewer.state().is_focused,
            "Before must lose focus once Tab moves it to After"
        );
        assert!(
            viewer.right_viewer.state().is_focused,
            "After must gain focus after Tab"
        );

        viewer.toggle_active_panel();
        assert!(viewer.left_viewer.state().is_focused);
        assert!(!viewer.right_viewer.state().is_focused);
    }

    fn rendered_text(terminal: &ratatui::Terminal<ratatui::backend::TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn draw_dual_panel_shows_each_filename_exactly_once() -> Result<()> {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&sample_diff_data());

        let backend = ratatui::backend::TestBackend::new(240, 24);
        let mut terminal = ratatui::Terminal::new(backend)?;
        terminal.draw(|f| {
            let area = f.area();
            viewer.draw(f, area).unwrap();
        })?;
        assert_eq!(
            viewer.display_mode,
            DisplayMode::Dual,
            "240 columns should be wide enough for dual-panel mode"
        );

        let text = rendered_text(&terminal);
        assert_eq!(
            text.matches("before.txt").count(),
            1,
            "the Before filename should appear exactly once: {text}"
        );
        assert_eq!(
            text.matches("after.txt").count(),
            1,
            "the After filename should appear exactly once: {text}"
        );
        Ok(())
    }

    #[test]
    fn draw_never_draws_a_border_around_either_panel_in_either_mode() -> Result<()> {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&sample_diff_data());
        let border_glyphs = ['╭', '╮', '╰', '╯', '━', '┏', '┓', '┗', '┛', '┃'];

        for (width, expected_mode) in [(240u16, DisplayMode::Dual), (100, DisplayMode::Single)] {
            let backend = ratatui::backend::TestBackend::new(width, 24);
            let mut terminal = ratatui::Terminal::new(backend)?;
            terminal.draw(|f| {
                let area = f.area();
                viewer.draw(f, area).unwrap();
            })?;
            assert_eq!(viewer.display_mode, expected_mode);

            let text = rendered_text(&terminal);
            assert!(
                !text.contains(border_glyphs),
                "found a border glyph in {expected_mode:?} mode: {text}"
            );
        }
        Ok(())
    }

    #[test]
    fn focused_cursor_position_is_none_before_any_file_is_loaded() {
        let viewer = DiffViewer::new();
        assert_eq!(viewer.focused_cursor_position(), None);
    }

    #[test]
    fn focused_cursor_position_reflects_the_active_panels_cursor() {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&sample_diff_data());

        viewer.focused_viewer().set_cursor_position(0, 3);
        assert_eq!(viewer.focused_cursor_position(), Some((0, 3)));

        viewer.toggle_active_panel();
        viewer.focused_viewer().set_cursor_position(0, 2);
        assert_eq!(viewer.focused_cursor_position(), Some((0, 2)));
    }

    #[test]
    fn moving_cursor_on_active_side_moves_inactive_side_cursor_to_matched_node() {
        use crate::diff::text::TextOperation;
        use crate::diff::text_range::TextRange;

        let mut viewer = DiffViewer::new();

        let data = DiffSessionData {
            before_path: PathBuf::from("before.txt"),
            after_path: PathBuf::from("after.txt"),
            before_contents: "abc\ndef\nghi".to_string(),
            after_contents: "ABC\nDEF\nGHI".to_string(),
            before_ranges: vec![
                RangeMatch {
                    source: TextRange::new(0, 0, 0, 3),
                    destination: TextRange::new(0, 0, 0, 3),
                    operation: TextOperation::Update,
                },
                RangeMatch {
                    source: TextRange::new(1, 0, 1, 3),
                    destination: TextRange::new(1, 0, 1, 3),
                    operation: TextOperation::Update,
                },
            ],
            after_ranges: vec![
                RangeMatch {
                    source: TextRange::new(0, 0, 0, 3),
                    destination: TextRange::new(0, 0, 0, 3),
                    operation: TextOperation::Update,
                },
                RangeMatch {
                    source: TextRange::new(1, 0, 1, 3),
                    destination: TextRange::new(1, 0, 1, 3),
                    operation: TextOperation::Update,
                },
            ],
            comment_only: false,
            plain_text_fallback: false,
        };

        viewer.load_diff(&data);

        assert_eq!(viewer.left_viewer.state().cursor_row, 0);
        assert_eq!(viewer.left_viewer.state().cursor_col, 0);

        assert_eq!(viewer.right_viewer.state().cursor_row, 0);
        assert_eq!(viewer.right_viewer.state().cursor_col, 0);

        viewer.move_cursor_vertical(1);

        assert_eq!(viewer.left_viewer.state().cursor_row, 1);

        assert_eq!(viewer.right_viewer.state().cursor_row, 1);
        assert_eq!(viewer.right_viewer.state().cursor_col, 0);
    }

    #[test]
    fn jump_to_change_skips_unchanged_lines_and_syncs_the_other_panel() {
        use crate::diff::text::{RangeMatch, TextOperation};
        use crate::diff::text_range::TextRange;

        let mut viewer = DiffViewer::new();
        let data = DiffSessionData {
            before_path: PathBuf::from("before.txt"),
            after_path: PathBuf::from("after.txt"),
            before_contents: "same0\nsame1\nchanged\nsame3\nsame4".to_string(),
            after_contents: "same0\nsame1\nCHANGED\nsame3\nsame4".to_string(),
            before_ranges: vec![
                RangeMatch {
                    source: TextRange::new(0, 0, 2, 0),
                    destination: TextRange::new(0, 0, 2, 0),
                    operation: TextOperation::Identical,
                },
                RangeMatch {
                    source: TextRange::new(2, 0, 2, 7),
                    destination: TextRange::new(2, 0, 2, 7),
                    operation: TextOperation::Update,
                },
                RangeMatch {
                    source: TextRange::new(2, 7, 5, 0),
                    destination: TextRange::new(2, 7, 5, 0),
                    operation: TextOperation::Identical,
                },
            ],
            after_ranges: vec![
                RangeMatch {
                    source: TextRange::new(0, 0, 2, 0),
                    destination: TextRange::new(0, 0, 2, 0),
                    operation: TextOperation::Identical,
                },
                RangeMatch {
                    source: TextRange::new(2, 0, 2, 7),
                    destination: TextRange::new(2, 0, 2, 7),
                    operation: TextOperation::Update,
                },
                RangeMatch {
                    source: TextRange::new(2, 7, 5, 0),
                    destination: TextRange::new(2, 7, 5, 0),
                    operation: TextOperation::Identical,
                },
            ],
            comment_only: false,
            plain_text_fallback: false,
        };
        viewer.load_diff(&data);

        // `load_diff` already sits on the only change.
        viewer.left_viewer.set_cursor_position(0, 0);

        viewer.jump_to_change(true);
        assert_eq!(
            viewer.left_viewer.state().cursor_row,
            2,
            "should land on the changed line"
        );
        assert_eq!(
            viewer.right_viewer.state().cursor_row,
            2,
            "the other panel's cursor should follow to the matched destination"
        );

        viewer.jump_to_change(true);
        assert_eq!(viewer.left_viewer.state().cursor_row, 2);

        viewer.jump_to_change(false);
        assert_eq!(viewer.left_viewer.state().cursor_row, 2);
    }

    #[test]
    fn jump_to_change_centers_both_panels() {
        use crate::diff::text::{RangeMatch, TextOperation};
        use crate::diff::text_range::TextRange;

        let line_count = 100;
        let change_row = 50;
        let contents: String = (0..line_count).map(|i| format!("line{i}\n")).collect();
        let ranges = vec![
            RangeMatch {
                source: TextRange::new(0, 0, change_row, 0),
                destination: TextRange::new(0, 0, change_row, 0),
                operation: TextOperation::Identical,
            },
            RangeMatch {
                source: TextRange::new(change_row, 0, change_row, 4),
                destination: TextRange::new(change_row, 0, change_row, 4),
                operation: TextOperation::Update,
            },
            RangeMatch {
                source: TextRange::new(change_row + 1, 0, line_count, 0),
                destination: TextRange::new(change_row + 1, 0, line_count, 0),
                operation: TextOperation::Identical,
            },
        ];

        let mut viewer = DiffViewer::new();
        let data = DiffSessionData {
            before_path: PathBuf::from("before.txt"),
            after_path: PathBuf::from("after.txt"),
            before_contents: contents.clone(),
            after_contents: contents,
            before_ranges: ranges.clone(),
            after_ranges: ranges,
            comment_only: false,
            plain_text_fallback: false,
        };
        viewer.load_diff(&data);
        viewer.left_viewer.set_viewport_height(10);
        viewer.right_viewer.set_viewport_height(10);
        viewer.left_viewer.set_cursor_position(0, 0);

        viewer.jump_to_change(true);

        assert_eq!(viewer.left_viewer.state().scroll, 45);
        assert_eq!(
            viewer.right_viewer.state().scroll,
            45,
            "the other panel should center on its matched destination the same way"
        );
    }

    /// A pure insertion: the before side has no stop of its own.
    fn pure_insertion_diff_data() -> DiffSessionData {
        use crate::diff::text::{RangeMatch, TextOperation};
        use crate::diff::text_range::TextRange;

        DiffSessionData {
            before_path: PathBuf::from("before.txt"),
            after_path: PathBuf::from("after.txt"),
            before_contents: "a\nb\nc\n".to_string(),
            after_contents: "a\nb\nc\nd\n".to_string(),
            before_ranges: vec![
                RangeMatch {
                    source: TextRange::new(0, 0, 3, 0),
                    destination: TextRange::new(0, 0, 3, 0),
                    operation: TextOperation::Identical,
                },
                RangeMatch {
                    // Zero-width `source`: `change_stops` skips it.
                    source: TextRange::new(3, 0, 3, 0),
                    destination: TextRange::new(3, 0, 4, 0),
                    operation: TextOperation::Insert,
                },
            ],
            after_ranges: vec![
                RangeMatch {
                    source: TextRange::new(0, 0, 3, 0),
                    destination: TextRange::new(0, 0, 3, 0),
                    operation: TextOperation::Identical,
                },
                RangeMatch {
                    source: TextRange::new(3, 0, 4, 0),
                    destination: TextRange::new(3, 0, 3, 0),
                    operation: TextOperation::Insert,
                },
            ],
            comment_only: false,
            plain_text_fallback: false,
        }
    }

    #[test]
    fn n_crosses_to_the_other_panel_when_that_is_where_the_next_change_is() {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&pure_insertion_diff_data());
        assert_eq!(viewer.active_panel, Panel::Before);

        viewer.jump_to_change(true);

        assert_eq!(
            viewer.active_panel,
            Panel::After,
            "the only change is on the after side, so that is where n should land"
        );
    }

    #[test]
    fn the_change_counter_reports_the_merged_total() {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&pure_insertion_diff_data());

        viewer.jump_to_change(true);

        let (_, total) = viewer
            .merged_change_count_and_index()
            .expect("a diff with one change should report a total");
        assert_eq!(total, 1);
    }

    /// Four stops alternating panels: a deletion and its replacing insertion at row 5, a paired
    /// update at row 12, an insertion at row 20. The same diff as the showcase viewer's `pairModel`
    /// (assets/viewer/model.test.js).
    fn alternating_panels_diff_data() -> DiffSessionData {
        use crate::diff::text::{RangeMatch, TextOperation};
        use crate::diff::text_range::TextRange;

        let range = |operation,
                     source: (usize, usize, usize, usize),
                     destination: (usize, usize, usize, usize)| RangeMatch {
            source: TextRange::new(source.0, source.1, source.2, source.3),
            destination: TextRange::new(destination.0, destination.1, destination.2, destination.3),
            operation,
        };
        let contents: String = (0..30).map(|i| format!("line {i}\n")).collect();
        DiffSessionData {
            before_path: PathBuf::from("before.txt"),
            after_path: PathBuf::from("after.txt"),
            before_contents: contents.clone(),
            after_contents: contents,
            before_ranges: vec![
                range(TextOperation::Identical, (0, 0, 0, 6), (0, 0, 0, 6)),
                range(TextOperation::Delete, (5, 0, 5, 6), (5, 0, 5, 0)),
                range(TextOperation::Update, (12, 5, 12, 6), (12, 5, 12, 6)),
            ],
            after_ranges: vec![
                range(TextOperation::Identical, (0, 0, 0, 6), (0, 0, 0, 6)),
                range(TextOperation::Insert, (5, 0, 5, 6), (5, 0, 5, 0)),
                range(TextOperation::Update, (12, 5, 12, 6), (12, 5, 12, 6)),
                range(TextOperation::Insert, (20, 0, 20, 7), (19, 0, 19, 0)),
            ],
            comment_only: false,
            plain_text_fallback: false,
        }
    }

    /// Counted by (panel, position) instead of in walk order, these four `n`s read 1, 3, 2, 4.
    #[test]
    fn the_change_counter_climbs_by_one_per_n_across_panel_switches() {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&alternating_panels_diff_data());

        let mut walked = Vec::new();
        for _ in 0..4 {
            viewer.jump_to_change(true);
            walked.push((
                viewer.active_panel,
                viewer.focused_cursor_position(),
                viewer.merged_change_count_and_index(),
            ));
        }

        assert_eq!(
            walked,
            vec![
                (Panel::Before, Some((5, 0)), Some((1, 4))),
                (Panel::After, Some((5, 0)), Some((2, 4))),
                (Panel::Before, Some((12, 5)), Some((3, 4))),
                (Panel::After, Some((20, 0)), Some((4, 4))),
            ]
        );
    }

    #[test]
    fn end_and_home_move_the_cursor_to_the_last_and_first_line() {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&alternating_panels_diff_data());
        let press = |viewer: &mut DiffViewer, code| {
            viewer
                .handle_key_event(crossterm::event::KeyEvent::new(
                    code,
                    crossterm::event::KeyModifiers::NONE,
                ))
                .unwrap();
        };

        press(&mut viewer, crossterm::event::KeyCode::End);
        assert_eq!(viewer.focused_cursor_position(), Some((29, 0)));
        press(&mut viewer, crossterm::event::KeyCode::Home);
        assert_eq!(viewer.focused_cursor_position(), Some((0, 0)));
    }

    #[test]
    fn p_crosses_to_the_other_panel_too() {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&pure_insertion_diff_data());

        viewer.jump_to_change(false);

        assert_eq!(viewer.active_panel, Panel::After);
    }

    #[test]
    fn crossing_panels_lands_on_the_change() {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&pure_insertion_diff_data());

        viewer.jump_to_change(true);

        assert_eq!(viewer.right_viewer.state().cursor_row, 3);
    }

    #[test]
    fn jumping_with_no_changes_anywhere_does_nothing() {
        let mut viewer = DiffViewer::new();
        let mut data = sample_diff_data();
        data.before_contents = "same\n".to_string();
        data.after_contents = "same\n".to_string();
        data.before_ranges = Vec::new();
        data.after_ranges = Vec::new();
        viewer.load_diff(&data);
        let before = (viewer.active_panel, viewer.focused_cursor_position());

        viewer.jump_to_change(true);

        assert_eq!(
            (viewer.active_panel, viewer.focused_cursor_position()),
            before
        );
        assert_eq!(viewer.merged_change_count_and_index(), None);
    }

    #[test]
    fn search_jumps_the_focused_panel_and_syncs_the_other_panel() {
        let mut viewer = DiffViewer::new();
        let mut data = sample_diff_data();
        data.before_contents = "foo\nbar\nfoo bar\n".to_string();
        data.after_contents = "xyz\n".to_string();
        viewer.load_diff(&data);

        viewer.search("bar");
        assert_eq!(viewer.left_viewer.state().cursor_row, 1);
        assert_eq!(viewer.focused_search_match_count_and_index(), Some((1, 2)));
    }

    #[test]
    fn jump_to_search_match_steps_through_matches_on_the_focused_panel() {
        let mut viewer = DiffViewer::new();
        let mut data = sample_diff_data();
        data.before_contents = "bar\nfoo\nbar\n".to_string();
        viewer.load_diff(&data);

        viewer.search("bar");
        assert_eq!(viewer.left_viewer.state().cursor_row, 0);

        viewer.jump_to_search_match(true);
        assert_eq!(viewer.left_viewer.state().cursor_row, 2);

        viewer.jump_to_search_match(true);
        assert_eq!(
            viewer.left_viewer.state().cursor_row,
            0,
            "forward past the last match should wrap to the first"
        );
    }

    #[test]
    fn focused_search_match_count_and_index_is_none_before_any_search() {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&sample_diff_data());
        assert_eq!(viewer.focused_search_match_count_and_index(), None);
    }

    #[test]
    fn node_highlight_is_off_by_default_and_h_enables_both_panels() {
        let mut viewer = DiffViewer::new();
        assert!(
            !viewer.left_viewer.is_node_highlight_enabled()
                && !viewer.right_viewer.is_node_highlight_enabled(),
            "the node highlight must ship off on both panels"
        );

        viewer.set_node_highlight(true);
        assert!(
            viewer.left_viewer.is_node_highlight_enabled()
                && viewer.right_viewer.is_node_highlight_enabled(),
            "enabling must apply to both panels at once"
        );

        viewer.set_node_highlight(false);
        assert!(
            !viewer.left_viewer.is_node_highlight_enabled()
                && !viewer.right_viewer.is_node_highlight_enabled(),
        );
    }

    fn range(
        source: crate::diff::text_range::TextRange,
        destination: crate::diff::text_range::TextRange,
        operation: TextOperation,
    ) -> RangeMatch {
        RangeMatch {
            source,
            destination,
            operation,
        }
    }

    #[test]
    fn change_stops_visit_a_paired_change_once_on_the_before_side() {
        use crate::diff::text_range::TextRange;
        let mut data = sample_diff_data();
        data.before_contents = "abc\n".to_string();
        data.after_contents = "xyz\n".to_string();
        data.before_ranges = vec![range(
            TextRange::new(0, 0, 0, 3),
            TextRange::new(0, 0, 0, 3),
            TextOperation::Update,
        )];
        data.after_ranges = vec![range(
            TextRange::new(0, 0, 0, 3),
            TextRange::new(0, 0, 0, 3),
            TextOperation::Update,
        )];
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&data);

        assert_eq!(viewer.change_stops(), vec![(Panel::Before, (0, 0))]);
    }

    #[test]
    fn a_deletion_is_visited_before_the_insertion_that_replaces_it() {
        use crate::diff::text_range::TextRange;
        let mut data = sample_diff_data();
        data.before_contents = "a\nold\nz\n".to_string();
        data.after_contents = "a\nnew\nz\n".to_string();
        data.before_ranges = vec![range(
            TextRange::new(1, 0, 2, 0),
            TextRange::new(1, 0, 1, 0),
            TextOperation::Delete,
        )];
        data.after_ranges = vec![range(
            TextRange::new(1, 0, 2, 0),
            TextRange::new(1, 0, 1, 0),
            TextOperation::Insert,
        )];
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&data);

        assert_eq!(
            viewer.change_stops(),
            vec![(Panel::Before, (1, 0)), (Panel::After, (1, 0))]
        );
    }

    #[test]
    fn enter_jumps_to_the_counterpart_and_back() {
        use crate::diff::text_range::TextRange;
        let mut data = sample_diff_data();
        data.before_contents = "moved\na\nb\n".to_string();
        data.after_contents = "a\nb\nmoved\n".to_string();
        data.before_ranges = vec![range(
            TextRange::new(0, 0, 0, 5),
            TextRange::new(2, 0, 2, 5),
            TextOperation::Move,
        )];
        data.after_ranges = vec![range(
            TextRange::new(2, 0, 2, 5),
            TextRange::new(0, 0, 0, 5),
            TextOperation::Move,
        )];
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&data);
        viewer.left_viewer.set_cursor_position(0, 0);

        viewer.jump_to_counterpart();
        assert_eq!(viewer.active_panel, Panel::After);
        assert_eq!(viewer.focused_cursor_position(), Some((2, 0)));

        viewer.jump_to_counterpart();
        assert_eq!(viewer.active_panel, Panel::Before);
        assert_eq!(viewer.focused_cursor_position(), Some((0, 0)));
    }

    /// What a terminal shows after drawing `viewer`: the focused panel's cursor row, as its text
    /// after the gutter, and the symbol in the cell under the cursor.
    fn drawn_cursor(viewer: &mut DiffViewer) -> (String, String) {
        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(240, 12))
            .expect("a test terminal");
        terminal
            .draw(|frame| {
                let area = frame.area();
                viewer.draw(frame, area).unwrap();
            })
            .expect("draw");
        let backend = terminal.backend();
        assert!(backend.cursor_visible(), "the cursor should be on screen");
        let cursor = backend.cursor_position();
        let content = match viewer.active_panel {
            Panel::Before => viewer.last_before_content,
            Panel::After => viewer.last_after_content,
        }
        .expect("the focused panel was drawn");
        let text_start = content.x + viewer.focused_viewer().gutter_width() as u16;
        let buffer = backend.buffer();
        // A wide character's second cell holds a blank, which is not part of the text.
        let mut row = String::new();
        let mut x = text_start;
        while x < content.x + content.width {
            let symbol = buffer[(x, cursor.y)].symbol();
            row.push_str(symbol);
            x += crate::diff::text_range::row_cells_of(symbol).get().max(1) as u16;
        }
        (
            row.trim_end().to_string(),
            buffer[(cursor.x, cursor.y)].symbol().to_string(),
        )
    }

    /// One `Update` between unchanged text on each side's single line, at byte columns
    /// `[start, end)` of the before line and `[after_start, after_end)` of the after line.
    fn one_update_on_one_line(
        before: &str,
        (start, end): (usize, usize),
        after: &str,
        (after_start, after_end): (usize, usize),
    ) -> DiffSessionData {
        use crate::diff::text_range::TextRange;
        let side = |start: usize, end: usize, other_start: usize, other_end: usize| {
            vec![
                range(
                    TextRange::new(0, 0, 0, start),
                    TextRange::new(0, 0, 0, other_start),
                    TextOperation::Identical,
                ),
                range(
                    TextRange::new(0, start, 0, end),
                    TextRange::new(0, other_start, 0, other_end),
                    TextOperation::Update,
                ),
                range(
                    TextRange::new(0, end, 1, 0),
                    TextRange::new(0, other_end, 1, 0),
                    TextOperation::Identical,
                ),
            ]
        };
        DiffSessionData {
            before_contents: format!("{before}\n"),
            after_contents: format!("{after}\n"),
            before_ranges: side(start, end, after_start, after_end),
            after_ranges: side(after_start, after_end, start, end),
            ..sample_diff_data()
        }
    }

    #[test]
    fn n_puts_the_cursor_on_a_change_after_non_ascii_text() {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&one_update_on_one_line(
            "let é = old;",
            (9, 12),
            "let é = new;",
            (9, 12),
        ));

        viewer.jump_to_change(true);

        assert_eq!(
            drawn_cursor(&mut viewer),
            ("let é = old;".to_string(), "o".to_string())
        );
        viewer.toggle_active_panel();
        assert_eq!(
            drawn_cursor(&mut viewer),
            ("let é = new;".to_string(), "n".to_string()),
            "the other panel's cursor follows to the same change"
        );
    }

    #[test]
    fn n_and_p_put_the_cursor_on_a_change_after_tab_indentation() {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&one_update_on_one_line(
            "\t\told();",
            (2, 5),
            "\t\tnew();",
            (2, 5),
        ));

        viewer.jump_to_change(true);
        assert_eq!(
            drawn_cursor(&mut viewer),
            ("        old();".to_string(), "o".to_string())
        );
        viewer.left_viewer.set_cursor_position(0, 0);
        viewer.jump_to_change(false);
        assert_eq!(
            drawn_cursor(&mut viewer),
            ("        old();".to_string(), "o".to_string())
        );
    }

    #[test]
    fn search_puts_the_cursor_on_a_match_after_non_ascii_text_or_tabs() {
        for (line, shown) in [
            ("é = world", "é = world"),
            ("漢字 = world", "漢字 = world"),
            ("\t\tworld", "        world"),
        ] {
            let mut viewer = DiffViewer::new();
            let mut data = sample_diff_data();
            data.before_contents = format!("{line}\n");
            viewer.load_diff(&data);

            viewer.search("world");

            assert_eq!(
                drawn_cursor(&mut viewer),
                (shown.to_string(), "w".to_string()),
                "{line:?}"
            );
        }
    }

    #[test]
    fn enter_puts_the_cursor_on_a_counterpart_after_non_ascii_text_or_tabs() {
        for (after, (start, end), shown) in [
            ("é = new;", (5, 8), "é = new;"),
            ("\tnew;", (1, 4), "    new;"),
        ] {
            let mut viewer = DiffViewer::new();
            viewer.load_diff(&one_update_on_one_line("old;", (0, 3), after, (start, end)));
            viewer.left_viewer.set_cursor_position(0, 0);

            viewer.jump_to_counterpart();

            assert_eq!(viewer.active_panel, Panel::After);
            assert_eq!(
                drawn_cursor(&mut viewer),
                (shown.to_string(), "n".to_string()),
                "{after:?}"
            );
        }
    }

    /// The cursor, once moved there by keys, sits on `ab`: the range under it is `ab`'s, and the
    /// node highlight paints the cell under it.
    #[test]
    fn the_range_under_a_cursor_moved_past_non_ascii_text_is_the_one_it_is_drawn_on() {
        let mut viewer = DiffViewer::new();
        let data = one_update_on_one_line("é = ab", (5, 7), "x = ab", (4, 6));
        viewer.load_diff(&data);
        viewer.set_node_highlight(true);
        viewer.left_viewer.set_cursor_position(0, 0);

        for _ in 0.."é = ".chars().count() {
            viewer.move_cursor_horizontal(1);
        }

        assert_eq!(
            drawn_cursor(&mut viewer),
            ("é = ab".to_string(), "a".to_string())
        );
        assert_eq!(
            viewer.left_viewer.cursor_destination(),
            Some(data.before_ranges[1].destination.clone())
        );
        assert_eq!(
            viewer.right_viewer.state().cursor_col,
            4,
            "the other panel follows to `ab`"
        );
    }

    /// A click lands on the character drawn in the clicked cell, and the range lookup and node
    /// highlight follow it.
    #[test]
    fn a_click_on_a_tab_indented_row_selects_the_range_drawn_under_it() {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
        let mut viewer = DiffViewer::new();
        let data = one_update_on_one_line("\tx = ab", (5, 7), "x = ab", (4, 6));
        viewer.load_diff(&data);
        viewer.set_node_highlight(true);
        drawn_cursor(&mut viewer);
        let content = viewer.last_before_content.expect("drawn");
        let text_start = content.x + viewer.left_viewer.gutter_width() as u16;

        // `    x = ab`: the `b` is in display column 9.
        viewer
            .handle_mouse_event(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: text_start + 9,
                row: content.y,
                modifiers: KeyModifiers::NONE,
            })
            .unwrap();

        assert_eq!(
            drawn_cursor(&mut viewer),
            ("    x = ab".to_string(), "b".to_string())
        );
        assert_eq!(
            viewer.left_viewer.cursor_destination(),
            Some(data.before_ranges[1].destination.clone())
        );
    }

    #[test]
    fn the_node_highlight_paints_the_range_under_a_cursor_past_non_ascii_text() {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&one_update_on_one_line("é = ab", (5, 7), "x = ab", (4, 6)));
        viewer.set_node_highlight(true);
        viewer.left_viewer.set_cursor_position(0, 0);
        for _ in 0..4 {
            viewer.move_cursor_horizontal(1);
        }

        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(240, 12)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                viewer.draw(frame, area).unwrap();
            })
            .unwrap();
        let cursor = terminal.backend().cursor_position();
        assert_eq!(
            terminal.backend().buffer()[(cursor.x, cursor.y)].bg,
            viewer.overlay_theme.palette().cross_highlight_bg,
            "the cell under the cursor carries the node highlight"
        );
    }
}
