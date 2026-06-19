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
use crate::tui::actions::Action;

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
        // Reset scroll position
        self.state.scroll = 0;
        Ok(())
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
                self.scroll_up();
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Down => {
                self.scroll_down();
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
