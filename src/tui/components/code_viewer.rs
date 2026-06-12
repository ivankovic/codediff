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
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use ratatui::{
    prelude::*,
    symbols::border,
    text::Line,
    widgets::{Block, Borders, Paragraph},
};
use tokio::sync::mpsc::UnboundedSender;

use super::Component;
use crate::code::language::language_for_path;
use crate::tui::actions::Action;

/// A component that displays source code
#[derive(Default)]
pub struct CodeViewer {
    /// The path to the file being displayed
    file_path: Option<PathBuf>,
    /// The file contents
    contents: String,
    /// The language of the file
    language: Option<crate::code::Language>,
    /// Scroll position (line number)
    scroll: usize,
    /// Viewport height in lines
    pub viewport_height: usize,
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
            file_path: Some(path),
            ..Default::default()
        }
    }

    /// Load a file into the viewer
    pub fn load_file(&mut self, path: PathBuf) -> Result<()> {
        self.file_path = Some(path.clone());

        // Determine language from file extension
        self.language = language_for_path(&path);

        // Read file contents
        self.contents = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read file: {:?}", path))?;

        // Reset scroll position
        self.scroll = 0;

        Ok(())
    }

    /// Get the total number of lines
    pub fn line_count(&self) -> usize {
        self.contents.lines().count()
    }

    /// Get visible lines based on scroll position
    fn visible_lines(&self) -> Vec<Line<'static>> {
        let lines: Vec<&str> = self.contents.lines().collect();
        let total_lines = lines.len();

        if total_lines == 0 {
            return vec![Line::from("")];
        }

        // Clamp scroll position
        let scroll = self.scroll.min(total_lines.saturating_sub(1));

        // Get visible lines
        let start = scroll;
        let end = std::cmp::min(start + self.viewport_height, total_lines);

        lines[start..end]
            .iter()
            .map(|&line| Line::from(line.to_string()))
            .collect()
    }

    /// Scroll up by one line
    pub fn scroll_up(&mut self) {
        if self.scroll > 0 {
            self.scroll = self.scroll.saturating_sub(1);
        }
    }

    /// Scroll down by one line
    pub fn scroll_down(&mut self) {
        let total = self.line_count();
        if self.scroll < total.saturating_sub(1) {
            self.scroll = self.scroll.saturating_add(1);
        }
    }

    /// Scroll to a specific line
    pub fn scroll_to(&mut self, line: usize) {
        self.scroll = line.min(self.line_count().saturating_sub(1));
    }

    /// Get the filename for display
    pub fn filename(&self) -> String {
        self.file_path
            .as_ref()
            .map(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            })
            .unwrap_or_else(|| "Untitled".to_string())
    }

    /// Get the language name for display
    pub fn language_name(&self) -> String {
        self.language
            .as_ref()
            .map(|l| format!("{:?}", l))
            .unwrap_or_else(|| "Plain Text".to_string())
    }
}

impl Component for CodeViewer {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.command_tx = Some(tx);
        Ok(())
    }

    fn init(&mut self, area: Rect) -> Result<()> {
        self.viewport_height = area.height.saturating_sub(2) as usize; // -2 for borders
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
                self.scroll = self.scroll.saturating_sub(self.viewport_height);
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(self.viewport_height);
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Home => {
                self.scroll = 0;
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::End => {
                self.scroll = self.line_count().saturating_sub(1);
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
                self.viewport_height = h.saturating_sub(2) as usize;
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let block = Block::default()
            .title(Line::from(vec![
                Span::styled(self.filename(), Style::new().bold().fg(Color::Cyan)),
                Span::raw(" - "),
                Span::styled(self.language_name(), Style::new().fg(Color::Gray)),
            ]))
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .border_style(Style::new().fg(Color::Gray));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Draw the code
        let lines = self.visible_lines();
        let paragraph = Paragraph::new(lines)
            .block(Block::default())
            .scroll((self.scroll as u16, 0));

        frame.render_widget(paragraph, inner);

        Ok(())
    }
}
