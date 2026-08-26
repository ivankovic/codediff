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
 *  You should have received a copy of the GNU Affero General License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */
use std::path::PathBuf;

use anyhow::Result;
use ratatui::{prelude::*, text::Line, widgets::Paragraph};
use tokio::sync::mpsc::UnboundedSender;

use super::{Component, code_viewer::CodeViewer};
use crate::diff::text::{RangeMatch, RenderMode, TextOperation, ranges_for_mode};
use crate::tui::actions::{Action, DiffSessionData};
use crate::tui::theme::{OverlayTheme, PanelLayout};

/// Below this terminal width, two side-by-side panels would each be too narrow to read code in,
/// so callers fall back to showing a single panel at full width. Shared with `human_solver`'s
/// own before/after panel layout, which has the same readability constraint.
pub const SINGLE_PANEL_THRESHOLD: u16 = 220;

/// A component that displays the before/after files side by side for diffing
#[derive(Default)]
pub struct DiffViewer {
    /// The "Before" code viewer
    left_viewer: CodeViewer,
    /// The "After" code viewer
    right_viewer: CodeViewer,
    /// Action sender
    command_tx: Option<UnboundedSender<Action>>,
    /// Current display mode: dual panel or single panel
    display_mode: DisplayMode,
    /// The user's layout preference (the `v` key, persisted): `Auto` keeps the width-based
    /// choice, `Dual`/`Single` force one mode regardless of width - see `update_display_mode`.
    layout_override: PanelLayout,
    /// Which panel is shown in single panel mode, and which panel's cursor drives navigation
    /// (and which panel `o` opens a file selector for) in dual panel mode.
    active_panel: Panel,
    /// A copy of the active overlay theme (also pushed into both viewers) - kept here for the
    /// minimap strips, which are drawn by this component, not the viewers.
    overlay_theme: OverlayTheme,
    /// The screen rectangles the Before/After content was last drawn into (minimap strip
    /// excluded), recorded by `draw` for mouse hit-testing - a mouse event only carries screen
    /// coordinates, and only `draw` knows where each panel actually landed. `None` for a panel
    /// not currently on screen (the inactive one in single-panel mode, or before the first
    /// frame).
    last_before_content: Option<Rect>,
    last_after_content: Option<Rect>,

    /// The mode the diff is painted in (the `M` key) - see `crate::diff::text::RenderMode`.
    render_mode: RenderMode,
    /// The unfiltered ranges and sources from the last `load_diff`, per side (0 = before).
    ///
    /// Kept so `set_render_mode` can re-apply the filter without a reload: `load_diff` resets the
    /// cursor and the cross-panel highlight, which is right when a new diff arrives and wrong when
    /// the reader merely asked to see more or less of the one already on screen.
    full_ranges: [Vec<RangeMatch>; 2],
    sources: [String; 2],
}

/// Display mode for the diff viewer
#[derive(Debug, Default, Clone, Copy, PartialEq)]
enum DisplayMode {
    /// Show both panels side by side
    #[default]
    Dual,
    /// Show only one panel at a time
    Single,
}

/// A `(row, column)` position, in whichever file's coordinates the context implies.
type Position = (usize, usize);

/// One stop in the merged `n`/`p` walk: which panel it is on, and where.
type ChangeStop = (Panel, Position);

/// Which of the two panels is active.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Panel {
    #[default]
    Before,
    After,
}

impl DiffViewer {
    /// Create a new DiffViewer
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a DiffViewer with specific files
    pub fn with_files(left_path: PathBuf, right_path: PathBuf) -> Self {
        Self {
            left_viewer: CodeViewer::with_file(left_path),
            right_viewer: CodeViewer::with_file(right_path),
            ..Default::default()
        }
    }

    /// Load files into the viewers
    pub fn load_files(&mut self, left_path: PathBuf, right_path: PathBuf) -> Result<()> {
        self.left_viewer.load_file(left_path)?;
        self.right_viewer.load_file(right_path)?;
        Ok(())
    }

    /// Load a completed diff: the file contents (already read, no further disk I/O here) plus
    /// the before/after diff ranges, and reset the cursor/cross-highlight to the start.
    pub fn load_diff(&mut self, data: &DiffSessionData) {
        self.left_viewer
            .load_contents(data.before_path.clone(), data.before_contents.clone());
        self.right_viewer
            .load_contents(data.after_path.clone(), data.after_contents.clone());
        self.full_ranges = [data.before_ranges.clone(), data.after_ranges.clone()];
        self.sources = [data.before_contents.clone(), data.after_contents.clone()];
        self.apply_render_mode(true);
        self.active_panel = Panel::Before;
        self.sync_focus();
        self.sync_cross_highlight();
    }

    /// Flip between `Full` and `Minimal` and remember the choice for the next run - the `M` key.
    pub fn toggle_render_mode(&mut self) {
        let next = match self.render_mode {
            RenderMode::Full => RenderMode::Minimal,
            RenderMode::Minimal => RenderMode::Full,
        };
        self.set_render_mode(next);
        crate::tui::theme::save_render_mode(next);
    }

    /// Switch how much of the diff is painted, keeping the cursor and scroll where they are.
    ///
    /// Re-filters the ranges captured by the last `load_diff` rather than recomputing anything:
    /// the mapping is identical in both modes, and only how much of it is shown differs.
    pub fn set_render_mode(&mut self, mode: RenderMode) {
        self.render_mode = mode;
        self.apply_render_mode(false);
        // The counterpart highlight was dropped with the old ranges; recompute it for whatever the
        // cursor is now sitting on, so the two panels stay in step across the switch.
        self.sync_cross_highlight();
    }

    pub fn render_mode(&self) -> RenderMode {
        self.render_mode
    }

    /// `reset_cursor` distinguishes the two callers: `load_diff` (a new diff arrived - jump to its
    /// first change) from `set_render_mode` (the same diff, painted differently - stay put).
    fn apply_render_mode(&mut self, reset_cursor: bool) {
        for (viewer, side) in [(&mut self.left_viewer, 0), (&mut self.right_viewer, 1)] {
            let ranges = ranges_for_mode(
                &self.full_ranges[side],
                &self.sources[side],
                self.render_mode,
            );
            if reset_cursor {
                viewer.set_ranges(ranges);
            } else {
                viewer.replace_ranges(ranges);
            }
        }
    }

    /// Move the focused panel's cursor vertically (by one line) and push the resulting matched
    /// node onto the other panel's cross-highlight, scrolling it to keep the destination visible.
    pub fn move_cursor_vertical(&mut self, direction: i32) {
        self.focused_viewer().move_cursor_vertical(direction);
        self.sync_cross_highlight();
        self.sync_scroll();
    }

    /// Move the focused panel's cursor horizontally (by one character) and push the resulting
    /// matched node onto the other panel's cross-highlight, scrolling it to keep the destination visible.
    pub fn move_cursor_horizontal(&mut self, direction: i32) {
        self.focused_viewer().move_cursor_horizontal(direction);
        self.sync_cross_highlight();
        self.sync_scroll();
    }

    /// Every change in the pair, once each, ordered as a reader meets them going down the diff.
    ///
    /// `n`/`p` used to walk one panel at a time, which reads badly: a diff is one sequence of
    /// edits that happens to be *displayed* in two columns, and stepping through the before column
    /// to its end before starting on the after column is not the order anybody reads it in. Worse,
    /// a pure insertion put no stops in the before panel at all, so `n` there had nowhere to go and
    /// had to stop and ask whether to switch sides.
    ///
    /// Each change appears exactly once, on the side that actually holds its text:
    ///
    /// * a `Delete` and every paired change (`Update`, `Move`) stop on the **before** side, where
    ///   the cross-panel highlight already shows the counterpart - so visiting the other half
    ///   separately would be the same stop twice;
    /// * an `Insert` has no before-side text, so it stops on the **after** side.
    ///
    /// Ordering is in *before-file* coordinates, which is the one space both sides can be compared
    /// in: a before-side stop sorts by its own position, and an after-side insertion by its
    /// `destination` - the zero-width position in the before file marking where it belongs. That is
    /// exactly the placement `TextRange`'s symmetric placeholders exist to record.
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
            // Only the after-side changes with no before-side text of their own: anything paired
            // was already stopped at on the before side.
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

        // Before the after side at the same key: a deletion and the insertion replacing it read in
        // that order. Ties within a side keep document order, which the sort's stability preserves.
        stops.sort_by_key(|(key, (panel, _))| (*key, *panel == Panel::After));
        stops.dedup_by_key(|(key, stop)| (*key, *stop));
        stops.into_iter().map(|(_, stop)| stop).collect()
    }

    /// Where the cursor currently sits in [`change_stops`]' ordering, if it is on a stop.
    fn current_stop_index(&self, stops: &[ChangeStop]) -> Option<usize> {
        let cursor = self.focused_cursor_position()?;
        stops
            .iter()
            .position(|&(panel, at)| panel == self.active_panel && at == cursor)
    }

    /// `n`/`p`: step to the next or previous change anywhere in the pair, switching panels when
    /// that is where the next one is.
    ///
    /// Wraps at both ends, the same convention the single-panel version had. With no stop under
    /// the cursor - the usual case, since the cursor moves freely - this picks the nearest stop in
    /// the direction of travel rather than restarting from the top.
    pub fn jump_to_change(&mut self, forward: bool) {
        let stops = self.change_stops();
        if stops.is_empty() {
            return;
        }
        let next = match self.current_stop_index(&stops) {
            Some(index) if forward => (index + 1) % stops.len(),
            Some(index) => (index + stops.len() - 1) % stops.len(),
            // Not on a stop: find where the cursor sits in the ordering and take the neighbour on
            // the side we are heading for.
            None => {
                let cursor = self.focused_cursor_position().unwrap_or((0, 0));
                let here = (self.active_panel, cursor);
                let after = stops
                    .iter()
                    .position(|&(panel, at)| (panel, at) > here)
                    .unwrap_or(stops.len());
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
        // Centre the focused panel too, not just its counterpart: jumping between scattered
        // changes benefits from context on both sides, and `set_cursor_position` alone only
        // scrolls the minimum needed to bring the row on screen.
        self.focused_viewer().scroll_to_center_row(row);
        self.focused_viewer().scroll_to_show_col(column);
        self.sync_cross_highlight();
        self.sync_scroll_centered();
    }

    /// `(1-based index, total)` over [`change_stops`] - what the footer's `change N/M` reports.
    ///
    /// Counts the *merged* walk, so the total is the number of changes in the diff rather than the
    /// number in whichever panel happens to be focused. A reader stepping with `n` sees the index
    /// climb monotonically to the total, across both panels, which is the whole point.
    pub fn merged_change_count_and_index(&self) -> Option<(usize, usize)> {
        let stops = self.change_stops();
        if stops.is_empty() {
            return None;
        }
        let cursor = self.focused_cursor_position()?;
        let here = (self.active_panel, cursor);
        let passed = stops
            .iter()
            .filter(|&&(panel, at)| (panel, at) <= here)
            .count();
        Some((passed.max(1), stops.len()))
    }

    /// Move the focused panel's cursor by half a viewport (`Ctrl-d`/`Ctrl-u`), the vim
    /// convention for fast-but-not-blind vertical travel - built from single-line moves so the
    /// sticky column, cross-highlight sync, and scroll-following all behave exactly as a run of
    /// `j`/`k` presses would.
    pub fn move_cursor_half_page(&mut self, direction: i32) {
        let half = (self.focused_viewer().viewport_height() / 2).max(1);
        for _ in 0..half {
            self.focused_viewer().move_cursor_vertical(direction);
        }
        self.sync_cross_highlight();
        self.sync_scroll();
    }

    /// Scroll the viewport(s) by one line without moving the cursor (`Ctrl-e`/`Ctrl-y`) - both
    /// panels in dual mode (same convention as PageUp/PageDown), the focused one in single mode.
    /// The cursor deliberately stays put; if it scrolls out of view the terminal cursor simply
    /// isn't drawn until it comes back (see `cursor_screen_position`).
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

    /// `Enter`: jump to the counterpart of whatever the cursor is on - switch the active panel
    /// and place its cursor at the matched destination's start. The cross-highlight already
    /// *shows* that destination; this makes it reachable in one keypress instead of `Tab` plus
    /// manual navigation. Pressing `Enter` again jumps back (the counterpart's own destination
    /// is the original range), giving free round-trip navigation across a long-distance move.
    /// A no-op when the cursor isn't on any range (e.g. no diff loaded).
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

    /// `S`: toggle syntax highlighting on both panels at once - a per-panel toggle would just
    /// leave the two sides looking inconsistent for no benefit.
    pub fn toggle_syntax_highlighting(&mut self) {
        let enable = !self.left_viewer.is_syntax_highlighting_enabled();
        self.left_viewer.set_syntax_highlighting(enable);
        self.right_viewer.set_syntax_highlighting(enable);
    }

    /// Apply a syntax-highlighting theme to both panels (the theme dialog's syntax dropdown).
    pub fn set_syntax_theme(&mut self, name: String) {
        self.left_viewer.set_syntax_theme(name.clone());
        self.right_viewer.set_syntax_theme(name);
    }

    /// `H`: toggle the node highlight on both panels at once and persist the choice, same
    /// both-panels-or-neither reasoning as `toggle_syntax_highlighting` above - the highlight is
    /// one signal spanning the two sides (the node under the cursor, and its counterpart), so
    /// enabling it on one panel alone would show half of it.
    ///
    /// Persisted because it is off by default: a user who wants it wants it every run, and having
    /// to re-enable it on every start would make the feature not worth reaching for.
    pub fn toggle_node_highlight(&mut self) {
        let enable = !self.left_viewer.is_node_highlight_enabled();
        self.set_node_highlight(enable);
        crate::tui::theme::save_node_highlight(enable);
    }

    /// Apply the node-highlight setting to both panels without persisting - the startup path
    /// (`App::run` loads the saved value) and `toggle_node_highlight`'s shared half.
    pub fn set_node_highlight(&mut self, enable: bool) {
        self.left_viewer.set_node_highlight(enable);
        self.right_viewer.set_node_highlight(enable);
    }

    /// Move the focused panel's cursor to the start of a 1-indexed line (the `g` prompt),
    /// clamped to the file, centered in the viewport, and synced onto the other panel like any
    /// other jump.
    pub fn jump_to_line(&mut self, line: usize) {
        let row = line.saturating_sub(1);
        self.focused_viewer().set_cursor_position(row, 0);
        self.focused_viewer().scroll_to_center_row(row);
        self.sync_cross_highlight();
        self.sync_scroll_centered();
    }

    /// Put the cursor back at a remembered position after a reload/exact re-run (`App::
    /// restore_after_reload`): re-activate the panel it was on and reposition, clamped - the
    /// file may have changed underneath a reload.
    pub fn restore_cursor(&mut self, panel: Panel, row: usize, col: usize) {
        if self.active_panel != panel {
            self.toggle_active_panel();
        }
        self.focused_viewer().set_cursor_position(row, col);
        self.sync_cross_highlight();
        self.sync_scroll();
    }

    /// Highlight `query`'s matches on the focused panel *without* moving the cursor - the search
    /// modal's live preview while typing. Returns the match count for the modal's readout.
    /// An empty query clears the preview (and returns 0).
    pub fn preview_search(&mut self, query: &str) -> usize {
        self.focused_viewer().preview_search(query)
    }

    /// Search the focused panel for `query`, replacing any previous search, and jump its cursor to
    /// the nearest match - what pressing Enter in the search modal does, then push the resulting
    /// matched node onto the other panel's cross-highlight like any other cursor movement.
    pub fn search(&mut self, query: &str) {
        self.focused_viewer().search(query);
        self.sync_cross_highlight();
        self.sync_scroll();
    }

    /// Move the focused panel's cursor to the next (`forward = true`) or previous (`forward =
    /// false`) search match (`>`/`<`), and sync the cross-highlight/scroll same as any other
    /// cursor movement.
    pub fn jump_to_search_match(&mut self, forward: bool) {
        self.focused_viewer().jump_to_search_match(forward);
        self.sync_cross_highlight();
        self.sync_scroll();
    }

    /// The focused panel's total search matches and how many are at or before the cursor - shown
    /// in `app.rs`'s footer, in place of `change N/M`, while a search is active.
    pub fn focused_search_match_count_and_index(&self) -> Option<(usize, usize)> {
        let viewer = match self.active_panel {
            Panel::Before => &self.left_viewer,
            Panel::After => &self.right_viewer,
        };
        viewer.search_match_count_and_index()
    }

    /// Push the focused panel's current cursor destination onto the other panel's
    /// cross-highlight, and move the other panel's cursor to follow the matched leaf node;
    /// call after anything that can change the cursor or the focused panel. The cursor always
    /// follows, but the highlight itself is suppressed when the focused side sits on an
    /// `Identical` match - see `cursor_destination_for_highlight`.
    fn sync_cross_highlight(&mut self) {
        let destination = self.focused_viewer().cursor_destination();
        let highlight_destination = self.focused_viewer().cursor_destination_for_highlight();
        self.other_viewer()
            .set_highlight_destination(highlight_destination);

        // Also move the inactive side's cursor to follow the matched leaf node
        if let Some(dest_range) = destination {
            self.other_viewer()
                .set_cursor_position(dest_range.start_row, dest_range.start_column);
        }
    }

    /// Scroll the inactive panel's viewport so the destination row of the focused panel's cursor
    /// stays visible; call after cursor movement.
    fn sync_scroll(&mut self) {
        if let Some(dest) = self.focused_viewer().cursor_destination() {
            self.other_viewer().scroll_to_show_row(dest.start_row);
        }
    }

    /// Same as `sync_scroll`, but centers the inactive panel's viewport on the destination row
    /// instead of just keeping it minimally visible - see `CodeViewer::scroll_to_center_row` and
    /// `jump_to_change` (`n`/`p`), this method's only caller.
    fn sync_scroll_centered(&mut self) {
        if let Some(dest) = self.focused_viewer().cursor_destination() {
            self.other_viewer().scroll_to_center_row(dest.start_row);
        }
    }

    /// Make sure exactly one side is marked focused, matching `active_panel`: the focused side
    /// then shows its own live cursor, and the other side shows only the pushed cross-highlight
    /// (see `CodeViewerState::is_focused`). Call whenever `active_panel` changes.
    fn sync_focus(&mut self) {
        self.left_viewer
            .set_focused(self.active_panel == Panel::Before);
        self.right_viewer
            .set_focused(self.active_panel == Panel::After);
    }

    /// Set the palette used to paint the diff/cursor overlay on both panels, picked via the `c`
    /// theme picker.
    pub fn set_overlay_theme(&mut self, theme: OverlayTheme) {
        self.overlay_theme = theme;
        self.left_viewer.set_overlay_theme(theme);
        self.right_viewer.set_overlay_theme(theme);
    }

    /// The viewer backing `panel` (independent of which one is active) - mouse handling resolves
    /// its target panel by position, not by focus.
    fn viewer_for(&mut self, panel: Panel) -> &mut CodeViewer {
        match panel {
            Panel::Before => &mut self.left_viewer,
            Panel::After => &mut self.right_viewer,
        }
    }

    /// Which panel (and its recorded content rect) the screen position `(column, row)` falls in,
    /// per the rects `draw` last recorded.
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

    /// The panel whose cursor currently drives navigation.
    fn focused_viewer(&mut self) -> &mut CodeViewer {
        match self.active_panel {
            Panel::Before => &mut self.left_viewer,
            Panel::After => &mut self.right_viewer,
        }
    }

    /// The panel that is cross-highlighted from the focused panel's cursor.
    fn other_viewer(&mut self) -> &mut CodeViewer {
        match self.active_panel {
            Panel::Before => &mut self.right_viewer,
            Panel::After => &mut self.left_viewer,
        }
    }

    /// Which panel is currently active, i.e. which one `Tab` last selected.
    pub fn active_panel(&self) -> Panel {
        self.active_panel
    }

    /// The focused panel's cursor position (0-indexed row, col), or `None` if that panel has no
    /// file loaded yet - shown in `app.rs`'s footer line.
    pub fn focused_cursor_position(&self) -> Option<(usize, usize)> {
        let viewer = match self.active_panel {
            Panel::Before => &self.left_viewer,
            Panel::After => &self.right_viewer,
        };
        if viewer.line_count() == 0 {
            return None;
        }
        let state = viewer.state();
        Some((state.cursor_row, state.cursor_col))
    }

    /// The focused panel's total distinct changes and how many are at or before the cursor - see
    /// `CodeViewer::change_count_and_index` - shown in `app.rs`'s footer line after `n`/`p`.
    pub fn focused_change_count_and_index(&self) -> Option<(usize, usize)> {
        let viewer = match self.active_panel {
            Panel::Before => &self.left_viewer,
            Panel::After => &self.right_viewer,
        };
        viewer.change_count_and_index()
    }

    /// Load a single file (no diff overlay yet) into the "Before" panel.
    pub fn set_before_file(&mut self, path: PathBuf) -> Result<()> {
        self.left_viewer.load_file(path)
    }

    /// Load a single file (no diff overlay yet) into the "After" panel.
    pub fn set_after_file(&mut self, path: PathBuf) -> Result<()> {
        self.right_viewer.load_file(path)
    }

    /// Update display mode: the user's explicit `v`-key preference wins; `Auto` falls back to
    /// the width-based choice.
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

    /// Set the layout preference (used at startup to apply the persisted choice) - takes effect
    /// on the next `update_display_mode` call, i.e. the next frame.
    pub fn set_layout_override(&mut self, layout: PanelLayout) {
        self.layout_override = layout;
    }

    /// The current layout preference - `app.rs` shows it in the footer when it isn't `Auto`.
    pub fn layout_override(&self) -> PanelLayout {
        self.layout_override
    }

    /// The `v` key: advance the layout preference through `Auto -> Dual -> Single` and persist
    /// it for future runs.
    fn cycle_layout_override(&mut self) {
        self.layout_override = self.layout_override.next();
        crate::tui::theme::save_panel_layout(self.layout_override);
    }

    /// Toggle between the "Before" and "After" panel.
    pub fn toggle_active_panel(&mut self) {
        self.active_panel = match self.active_panel {
            Panel::Before => Panel::After,
            Panel::After => Panel::Before,
        };
        self.sync_focus();
    }

    /// Get the filename of the active viewer, or a hint to press `o` if it has no file loaded yet
    /// - matches dual-panel mode's own title bar, which uses the same fallback (see `draw`).
    fn active_filename(&self) -> String {
        match self.active_panel {
            Panel::Before => self.left_viewer.filename_or_hint(),
            Panel::After => self.right_viewer.filename_or_hint(),
        }
    }

    /// Get the language name of the active viewer
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
        // Update display mode based on available width
        self.update_display_mode(area.width);

        if self.display_mode == DisplayMode::Dual {
            let (left_area, right_area) = split_panels(area);

            self.left_viewer.init(left_area)?;
            self.right_viewer.init(right_area)?;
        } else {
            // Single panel mode: init both viewers with full area
            // They'll be switched based on active_panel
            self.left_viewer.init(area)?;
            self.right_viewer.init(area)?;
        }

        Ok(())
    }

    fn handle_key_event(&mut self, key: crossterm::event::KeyEvent) -> Result<Option<Action>> {
        use crossterm::event::KeyModifiers;
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        // Handle key events
        match key.code {
            // Vim-convention scrolling: Ctrl-d/Ctrl-u move the cursor half a viewport,
            // Ctrl-e/Ctrl-y scroll the view one line leaving the cursor in place. Checked before
            // the plain-letter arms so a Ctrl-modified key can't fall through to an unrelated
            // binding.
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
            // Enter jumps to the counterpart of the range under the cursor on the other panel -
            // see `jump_to_counterpart`.
            crossterm::event::KeyCode::Enter => {
                self.jump_to_counterpart();
                Ok(Some(Action::Render))
            }
            // v cycles the layout preference (auto -> dual -> single), persisted across runs.
            crossterm::event::KeyCode::Char('v') => {
                self.cycle_layout_override();
                Ok(Some(Action::Render))
            }
            // S (capital - lowercase s stays free) toggles syntax highlighting on both panels.
            crossterm::event::KeyCode::Char('S') => {
                self.toggle_syntax_highlighting();
                Ok(Some(Action::Render))
            }
            // H (capital, pairing with S above) toggles the node highlight, which ships off.
            crossterm::event::KeyCode::Char('H') => {
                self.toggle_node_highlight();
                Ok(Some(Action::Render))
            }
            // Switch how much of the diff is painted. Not a re-diff: the mapping is identical
            // either way, and only how much of it is worth showing differs - which is why the
            // hand-painted ground truth records both readings rather than one plus a mistake (see
            // `research/data/quality/text_painting_findings.md`).
            crossterm::event::KeyCode::Char('M') => {
                self.toggle_render_mode();
                Ok(Some(Action::Render))
            }
            // Tab switches which panel's cursor drives navigation (and, in single panel mode,
            // which panel is shown).
            crossterm::event::KeyCode::Tab => {
                self.toggle_active_panel();
                self.sync_cross_highlight();
                Ok(Some(Action::Render))
            }
            // The cursor is a real (row, column) position (see SPECS.md), so arrows and vim
            // h/j/k/l map to literal left/down/up/right movement, same as any text editor.
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
            // n/p jump the cursor straight to the next/previous actual change, skipping over
            // unchanged content entirely - unlike h/j/k/l, which move one character/line at a
            // time regardless of what's there. See `jump_to_change_or_prompt`'s doc comment for
            // what happens when the focused panel has no changes to jump to at all.
            crossterm::event::KeyCode::Char('n') => {
                self.jump_to_change(true);
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Char('p') => {
                self.jump_to_change(false);
                Ok(Some(Action::Render))
            }
            // >/< jump the cursor between search matches (see the `/` search modal), the same
            // wrap-around convention as n/p - a distinct pair rather than overloading n/p, since
            // help_modal.rs already documents those as change-navigation specifically.
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
            crossterm::event::KeyCode::Home => {
                if self.display_mode == DisplayMode::Dual {
                    self.left_viewer.scroll_to(0);
                    self.right_viewer.scroll_to(0);
                } else {
                    self.focused_viewer().scroll_to(0);
                }
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::End => {
                if self.display_mode == DisplayMode::Dual {
                    let left_lines = self.left_viewer.line_count();
                    let right_lines = self.right_viewer.line_count();
                    let max_lines = std::cmp::max(left_lines, right_lines);
                    self.left_viewer.scroll_to(max_lines.saturating_sub(1));
                    self.right_viewer.scroll_to(max_lines.saturating_sub(1));
                } else {
                    let lines = self.focused_viewer().line_count();
                    self.focused_viewer().scroll_to(lines.saturating_sub(1));
                }
                Ok(Some(Action::Render))
            }
            _ => Ok(None),
        }
    }

    /// Mouse support: the scroll wheel scrolls the panel under the pointer (3 lines a notch,
    /// the common terminal convention), a left click focuses that panel and places the cursor on
    /// the clicked character (translated through the gutter and both scroll offsets). Hit-testing
    /// uses the content rects `draw` last recorded, so this works in both layouts without
    /// re-deriving the frame's geometry.
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
                let col =
                    viewer.state().scroll_col + clicked_col.saturating_sub(viewer.gutter_width());
                viewer.set_cursor_position(row, col);
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
                // Update display mode based on new width
                self.update_display_mode(*w);
            }
            Action::DiffReady(data) => self.load_diff(data),
            _ => {}
        }

        // Forward action to both viewers
        let _ = self.left_viewer.update(action.clone())?;
        let _ = self.right_viewer.update(action)?;

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        // Update display mode based on current width
        self.update_display_mode(area.width);

        // No border anywhere around the code display, in either mode - see `panel_title`'s doc
        // comment for why. Exactly one plain title row, so exactly 1 row reserved here,
        // uniformly, rather than the old per-mode border-row bookkeeping (2 rows for a single
        // nested border, 4 for two).
        let viewport_height = area.height.saturating_sub(1) as usize;
        self.left_viewer.set_viewport_height(viewport_height);
        self.right_viewer.set_viewport_height(viewport_height);

        if self.display_mode == DisplayMode::Dual {
            // Dual panel mode: show both side by side
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
                frame.set_cursor(x, y);
            }
        } else {
            // Single panel mode: show only one panel at a time
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
                frame.set_cursor(x, y);
            }
        }

        Ok(())
    }
}

/// Splits a panel's content area into `(content, Some(strip))` - the rightmost column becomes
/// the change-overview minimap - or `(area, None)` when there's nothing to show a map *of* (no
/// file loaded) or no room for one.
fn carve_minimap(area: Rect, line_count: usize) -> (Rect, Option<Rect>) {
    if line_count == 0 || area.width < 2 {
        return (area, None);
    }
    let content = Rect::new(area.x, area.y, area.width - 1, area.height);
    let strip = Rect::new(area.x + area.width - 1, area.y, 1, area.height);
    (content, Some(strip))
}

/// Draws the change-overview strip: one cell per band, painted in the band's operation color
/// from the active theme (the same background hues the diff overlay itself uses, applied to a
/// solid block glyph), blank where the band holds no change. See `CodeViewer::change_bands`.
fn render_minimap(
    frame: &mut Frame,
    strip: Rect,
    bands: &[Option<crate::diff::text::TextOperation>],
    theme: OverlayTheme,
) {
    use crate::diff::text::TextOperation;
    let palette = theme.palette();
    let lines: Vec<Line> = bands
        .iter()
        .map(|band| match band {
            Some(op) => {
                let color = match op {
                    TextOperation::Insert => palette.insert_bg,
                    TextOperation::Delete => palette.delete_bg,
                    TextOperation::Move => palette.move_bg,
                    TextOperation::Update => palette.update_bg,
                    TextOperation::Identical | TextOperation::NotYetSet => {
                        return Line::from(" ");
                    }
                };
                Line::from(Span::styled("█", Style::new().fg(color)))
            }
            None => Line::from(" "),
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), strip);
}

/// Splits a dual-panel area into (left, right) halves with a one-column gap for the divider,
/// shared by [`DiffViewer::init`] (sizing viewports before anything is drawn) and
/// [`DiffViewer::draw`] (the actual layout) so the two can never silently diverge.
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

/// Draws a dual-mode panel's plain, borderless title line - bold (and highlighted on whichever
/// side is active, so `Tab`, and therefore `o`'s file-selector target, stays visible without a
/// border to distinguish it) - and returns the remaining area below it for the panel's own
/// content.
///
/// No `Block`/`Borders::ALL` here (nor in single-panel mode, nor in `CodeViewerWidget::render`
/// anymore either): this and the widget's own border used to *both* draw, each with their own
/// title showing the filename, so dual-panel mode showed it twice; single-panel mode hid the
/// widget's own border but not consistently, leaving the two modes' framing visibly different for
/// no reason. One plain title line, always drawn the same way in both modes, replaces both.
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
            mode: crate::diff::DiffMode::Fast,
            plain_text_fallback: false,
        }
    }

    /// Regression test for the exploratory-testing bug: after `Tab` moves focus to "After",
    /// exactly the "After" side shows its own cursor highlight and the "Before" side shows only
    /// the pushed cross-highlight - never the other way around, and never both blues on one
    /// panel at once.
    /// Pressing `M` at line 400 must not land you back at the top: it changes how the same diff is
    /// painted, not which diff is loaded.
    #[test]
    fn switching_render_mode_keeps_the_cursor_where_it_is() {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&sample_diff_data());

        for _ in 0..3 {
            viewer.move_cursor_vertical(1);
        }
        let before = viewer.focused_cursor_position();
        assert!(before.is_some());

        viewer.set_render_mode(RenderMode::Minimal);

        assert_eq!(
            viewer.focused_cursor_position(),
            before,
            "the cursor should survive a mode switch"
        );
    }

    /// The other half of the same rule: a *new* diff should still jump to its first change.
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

        // Toggling back must restore focus to "Before" alone.
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

    /// Regression test: dual-panel mode used to draw *two* titles per side - `panel_title`'s own
    /// (" Before  before.txt") plus the (since-removed) `CodeViewerWidget`'s own border/title,
    /// which repeated the filename a second time. One title per side, one filename each.
    #[test]
    fn draw_dual_panel_shows_each_filename_exactly_once() -> Result<()> {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&sample_diff_data());

        let backend = ratatui::backend::TestBackend::new(240, 24);
        let mut terminal = ratatui::Terminal::new(backend)?;
        terminal.draw(|f| {
            let area = f.size();
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

    /// Regression test: neither dual- nor single-panel mode should draw a border around the code
    /// display anymore - see `panel_title`'s doc comment. Checks for the actual border-drawing
    /// glyphs the old `Block`s used (`border::ROUNDED`/`border::THICK`), not just the filename
    /// duplication the other regression test above already covers.
    #[test]
    fn draw_never_draws_a_border_around_either_panel_in_either_mode() -> Result<()> {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&sample_diff_data());
        let border_glyphs = ['╭', '╮', '╰', '╯', '━', '┏', '┓', '┗', '┛', '┃'];

        for (width, expected_mode) in [(240u16, DisplayMode::Dual), (100, DisplayMode::Single)] {
            let backend = ratatui::backend::TestBackend::new(width, 24);
            let mut terminal = ratatui::Terminal::new(backend)?;
            terminal.draw(|f| {
                let area = f.size();
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

        // Tab moves focus to "After" - the footer should now follow that panel's cursor instead.
        viewer.toggle_active_panel();
        viewer.focused_viewer().set_cursor_position(0, 2);
        assert_eq!(viewer.focused_cursor_position(), Some((0, 2)));
    }

    /// Test that moving the cursor on the active side moves the inactive side's cursor to
    /// follow the matched leaf node.
    #[test]
    fn moving_cursor_on_active_side_moves_inactive_side_cursor_to_matched_node() {
        use crate::diff::text::TextOperation;
        use crate::diff::text_range::TextRange;

        let mut viewer = DiffViewer::new();

        // Create sample diff data with two ranges
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
            mode: crate::diff::DiffMode::Fast,
            plain_text_fallback: false,
        };

        viewer.load_diff(&data);

        // Initially, cursor should be on first range (0, 0)
        assert_eq!(viewer.left_viewer.state().cursor_row, 0);
        assert_eq!(viewer.left_viewer.state().cursor_col, 0);

        // The right side's cursor should also be at (0, 0) to follow the matched node
        assert_eq!(viewer.right_viewer.state().cursor_row, 0);
        assert_eq!(viewer.right_viewer.state().cursor_col, 0);

        // Move cursor down on left side
        viewer.move_cursor_vertical(1);

        // Left cursor should now be on row 1
        assert_eq!(viewer.left_viewer.state().cursor_row, 1);

        // Right cursor should follow to the matched destination (row 1, col 0)
        assert_eq!(viewer.right_viewer.state().cursor_row, 1);
        assert_eq!(viewer.right_viewer.state().cursor_col, 0);
    }

    /// `n`/`p` (`jump_to_change`) must skip straight over unchanged lines to the next/previous
    /// real change, and - like every other cursor movement - push the result onto the other
    /// panel's cross-highlight so both sides stay in sync.
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
            mode: crate::diff::DiffMode::Fast,
            plain_text_fallback: false,
        };
        viewer.load_diff(&data);

        // load_diff already places the cursor on the first (only) change, so back it off first.
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

        // Only one change exists, so jumping forward again must wrap back to the same spot.
        viewer.jump_to_change(true);
        assert_eq!(viewer.left_viewer.state().cursor_row, 2);

        viewer.jump_to_change(false);
        assert_eq!(viewer.left_viewer.state().cursor_row, 2);
    }

    /// `n`/`p` must center *both* panels' viewports on the matched change, not just the focused
    /// one (`sync_scroll_centered`) - otherwise the other panel could show its matched content
    /// pinned awkwardly at the very top/bottom of the viewport even though the focused side is
    /// nicely centered.
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
            mode: crate::diff::DiffMode::Fast,
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

    /// A diff of pure insertions: the "before" side is 100% `Identical` (nothing was removed or
    /// changed there), so its `n`/`p` history is empty even though the "after" side has a real
    /// change. This is the exact scenario `Action::NoChangesPromptNeeded` exists for.
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
                    // `source` is *this side's own* position - zero-width here since there is no
                    // "before" location for an insertion. `change_positions` filters zero-width
                    // ranges out via `source.is_empty()`, which is exactly why the "before" panel
                    // has zero navigable changes despite this entry existing at all.
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
                    // `source` is *this side's own* position - real and non-zero-width here
                    // (the new "d" line genuinely exists in `after_contents`), so this one *is* a
                    // navigable change on the "after" side.
                    source: TextRange::new(3, 0, 4, 0),
                    destination: TextRange::new(3, 0, 3, 0),
                    operation: TextOperation::Insert,
                },
            ],
            comment_only: false,
            mode: crate::diff::DiffMode::Fast,
            plain_text_fallback: false,
        }
    }

    /// `n` on the empty "before" side of a pure-insertion diff walks straight to the change on the
    /// "after" side, switching panels on the way.
    ///
    /// This used to stop and ask, because the walk was per-panel and the before side had no stops
    /// at all. Merging the two sides into one ordered walk makes the question moot: there is one
    /// sequence of changes, and `n` goes to the next one wherever it lives.
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

    /// The footer's `change N/M` counts the merged walk, so M is the number of changes in the
    /// diff rather than the number in whichever panel happens to be focused.
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

    /// `p` crosses panels the same way `n` does - the walk is one ordered sequence in both
    /// directions.
    #[test]
    fn p_crosses_to_the_other_panel_too() {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&pure_insertion_diff_data());

        viewer.jump_to_change(false);

        assert_eq!(viewer.active_panel, Panel::After);
    }

    /// Landing on the change itself, not merely on the right panel.
    #[test]
    fn crossing_panels_lands_on_the_change() {
        let mut viewer = DiffViewer::new();
        viewer.load_diff(&pure_insertion_diff_data());

        viewer.jump_to_change(true);

        assert_eq!(viewer.right_viewer.state().cursor_row, 3);
    }

    /// Neither side has any changes at all (e.g. two identical files) - genuinely nothing to go
    /// to, so this stays the silent no-op it always was.
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

    /// `search` (the `/` modal's Enter) must operate on the focused panel and, like every other
    /// cursor movement, sync the resulting position onto the other panel's cross-highlight.
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

    /// `>`/`<` (`jump_to_search_match`) step through matches on the focused panel and wrap around,
    /// same convention as `n`/`p`.
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

    /// The node highlight ships off, and `H` turns it on for *both* panels - half of a
    /// two-panel signal is worse than none of it.
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
}
