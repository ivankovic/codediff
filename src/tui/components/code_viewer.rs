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
use ratatui::{Frame, layout::Rect};
use tokio::sync::mpsc::UnboundedSender;

use super::Component;
use crate::diff::text::RangeMatch;
use crate::diff::text_range::TextRange;
use crate::tui::actions::Action;
use crate::tui::widgets::code_viewer::is_empty_range;

/// A component that displays source code
///
/// This component wraps the CodeViewerWidget and manages its state.
/// It implements the Component trait for integration with the TUI architecture.
#[derive(Default)]
pub struct CodeViewer {
    /// The underlying widget
    widget: crate::tui::widgets::code_viewer::CodeViewerWidget,
    /// The widget state (scroll position and viewport)
    state: crate::tui::widgets::code_viewer::CodeViewerState,
    /// Action sender
    command_tx: Option<UnboundedSender<Action>>,
}

impl CodeViewer {
    /// Create a new CodeViewer
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a CodeViewer for a specific file
    pub fn with_file(path: PathBuf) -> Self {
        Self {
            widget: crate::tui::widgets::code_viewer::CodeViewerWidget::with_file(path),
            ..Default::default()
        }
    }

    /// Load a file into the viewer
    pub fn load_file(&mut self, path: PathBuf) -> Result<()> {
        self.widget.load_file(path)?;
        self.reset_state();
        Ok(())
    }

    /// Load already-read contents into the viewer (no filesystem access).
    pub fn load_contents(&mut self, path: PathBuf, contents: String) {
        self.widget.load_contents(path, contents);
        self.reset_state();
    }

    /// Reset scroll/diff/cursor state, e.g. after loading a new file.
    fn reset_state(&mut self) {
        self.state.scroll = 0;
        self.state.ranges.clear();
        self.state.cursor = 0;
        self.state.highlight_destination = None;
    }

    /// Get the total number of lines
    pub fn line_count(&self) -> usize {
        self.widget.line_count()
    }

    /// Scroll up by one line
    pub fn scroll_up(&mut self) {
        if self.state.scroll > 0 {
            self.state.scroll = self.state.scroll.saturating_sub(1);
        }
    }

    /// Scroll down by one line
    pub fn scroll_down(&mut self) {
        let total = self.line_count();
        if self.state.scroll < total.saturating_sub(1) {
            self.state.scroll = self.state.scroll.saturating_add(1);
        }
    }

    /// Scroll to a specific line
    pub fn scroll_to(&mut self, line: usize) {
        self.state.scroll = line.min(self.line_count().saturating_sub(1));
    }

    /// Set the diff ranges for this side (as returned by `TextDiff::all`), and place the cursor
    /// on the first navigable (non zero-width) range.
    pub fn set_ranges(&mut self, ranges: Vec<RangeMatch>) {
        self.state.ranges = ranges;
        self.state.cursor = self
            .state
            .ranges
            .iter()
            .position(|range_match| !is_empty_range(&range_match.source))
            .unwrap_or(0);
        self.state.highlight_destination = None;
        self.scroll_to_cursor();
    }

    /// The destination range of the range the cursor currently sits on, i.e. the range to
    /// cross-highlight on the other panel.
    pub fn cursor_destination(&self) -> Option<TextRange> {
        self.state
            .ranges
            .get(self.state.cursor)
            .map(|range_match| range_match.destination.clone())
    }

    /// Set (or clear) the cross-highlighted range coming from the other panel's cursor.
    pub fn set_highlight_destination(&mut self, destination: Option<TextRange>) {
        self.state.highlight_destination = destination;
    }

    /// Move the cursor to the previous (`direction < 0`) or next (`direction > 0`) navigable
    /// range, skipping zero-width alignment markers, and scroll to keep it visible.
    pub fn move_cursor(&mut self, direction: i32) {
        if self.state.ranges.is_empty() || direction == 0 {
            return;
        }

        let len = self.state.ranges.len() as isize;
        let mut index = self.state.cursor as isize;
        loop {
            index += direction.signum() as isize;
            if index < 0 || index >= len {
                return;
            }
            if !is_empty_range(&self.state.ranges[index as usize].source) {
                self.state.cursor = index as usize;
                self.scroll_to_cursor();
                return;
            }
        }
    }

    /// Scroll the viewport so the cursor's range is visible.
    fn scroll_to_cursor(&mut self) {
        let Some(range_match) = self.state.ranges.get(self.state.cursor) else {
            return;
        };
        let row = range_match.source.start_row;
        if row < self.state.scroll {
            self.state.scroll = row;
        } else if self.state.viewport_height > 0
            && row >= self.state.scroll + self.state.viewport_height
        {
            self.state.scroll = row.saturating_sub(self.state.viewport_height - 1);
        }
    }

    /// Get the filename for display
    pub fn filename(&self) -> String {
        self.widget.filename()
    }

    /// Get the language name for display
    pub fn language_name(&self) -> String {
        self.widget.language_name()
    }

    /// Get the viewport height
    pub fn viewport_height(&self) -> usize {
        self.state.viewport_height
    }

    /// Set the viewport height
    pub fn set_viewport_height(&mut self, height: usize) {
        self.state.viewport_height = height;
    }

    /// Enable syntax highlighting with optional theme
    pub fn enable_syntax_highlighting(&mut self, theme: Option<String>) {
        self.widget.enable_syntax_highlighting();
        if let Some(t) = theme {
            self.widget.set_theme(t);
        }
    }

    /// Disable syntax highlighting
    pub fn disable_syntax_highlighting(&mut self) {
        self.widget.disable_syntax_highlighting();
    }

    /// Check if syntax highlighting is enabled
    pub fn is_syntax_highlighting_enabled(&self) -> bool {
        self.widget.is_syntax_highlighting_enabled()
    }

    /// Get mutable reference to the widget for customization
    pub fn widget_mut(&mut self) -> &mut crate::tui::widgets::code_viewer::CodeViewerWidget {
        &mut self.widget
    }

    /// Get reference to the widget
    pub fn widget(&self) -> &crate::tui::widgets::code_viewer::CodeViewerWidget {
        &self.widget
    }

    /// Get mutable reference to the state
    pub fn state_mut(&mut self) -> &mut crate::tui::widgets::code_viewer::CodeViewerState {
        &mut self.state
    }

    /// Get reference to the state
    pub fn state(&self) -> &crate::tui::widgets::code_viewer::CodeViewerState {
        &self.state
    }
}

impl Component for CodeViewer {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.command_tx = Some(tx);
        Ok(())
    }

    fn init(&mut self, area: Rect) -> Result<()> {
        self.state.viewport_height = area.height.saturating_sub(2) as usize; // -2 for borders
        Ok(())
    }

    fn handle_key_event(&mut self, key: crossterm::event::KeyEvent) -> Result<Option<Action>> {
        match key.code {
            crossterm::event::KeyCode::Up => {
                self.move_cursor(-1);
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Down => {
                self.move_cursor(1);
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::PageUp => {
                self.state.scroll = self.state.scroll.saturating_sub(self.state.viewport_height);
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::PageDown => {
                self.state.scroll = self.state.scroll.saturating_add(self.state.viewport_height);
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Home => {
                self.state.scroll = 0;
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::End => {
                self.state.scroll = self.line_count().saturating_sub(1);
                Ok(Some(Action::Render))
            }
            _ => Ok(None),
        }
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Tick => {}
            Action::Render => {}
            Action::Resize(_w, h) => {
                // Update viewport height
                self.state.viewport_height = h.saturating_sub(2) as usize;
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        // Use the StatefulWidget render method via frame.render_stateful_widget
        // We need to pass self.widget by value, but we can't move out of self
        // So we use a reference and implement StatefulWidget for &CodeViewerWidget
        frame.render_stateful_widget(&self.widget, area, &mut self.state);
        Ok(())
    }
}
