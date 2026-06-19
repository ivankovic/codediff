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
use ratatui::{
    prelude::*,
    symbols::border,
    text::Line,
    widgets::{Block, Borders},
};
use tokio::sync::mpsc::UnboundedSender;

use super::{Component, code_viewer::CodeViewer};
use crate::tui::actions::Action;

/// A component that displays two files side by side for diffing
#[derive(Default)]
pub struct DiffViewer {
    /// The left (before) code viewer
    left_viewer: CodeViewer,
    /// The right (after) code viewer
    right_viewer: CodeViewer,
    /// Action sender
    command_tx: Option<UnboundedSender<Action>>,
    /// Current display mode: dual panel or single panel
    display_mode: DisplayMode,
    /// Which panel is shown in single panel mode
    active_panel: ActivePanel,
}

/// Display mode for the diff viewer
#[derive(Default, Clone, Copy, PartialEq)]
enum DisplayMode {
    /// Show both panels side by side
    #[default]
    Dual,
    /// Show only one panel at a time
    Single,
}

/// Which panel is active in single panel mode
#[derive(Default, Clone, Copy, PartialEq)]
enum ActivePanel {
    /// Showing the left (before) panel
    Left,
    /// Showing the right (after) panel
    #[default]
    Right,
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

    /// Set the file for the left viewer
    pub fn set_left_file(&mut self, path: PathBuf) -> Result<()> {
        self.left_viewer.load_file(path)
    }

    /// Set the file for the right viewer
    pub fn set_right_file(&mut self, path: PathBuf) -> Result<()> {
        self.right_viewer.load_file(path)
    }

    /// Scroll both viewers synchronously
    pub fn scroll_both(&mut self, lines: i32) {
        if lines > 0 {
            for _ in 0..lines {
                self.left_viewer.scroll_down();
                self.right_viewer.scroll_down();
            }
        } else if lines < 0 {
            for _ in 0..(-lines) {
                self.left_viewer.scroll_up();
                self.right_viewer.scroll_up();
            }
        }
    }

    /// Update display mode based on available width
    pub fn update_display_mode(&mut self, width: u16) {
        // Threshold: if width < 200, switch to single panel mode
        const SINGLE_PANEL_THRESHOLD: u16 = 200;
        self.display_mode = if width < SINGLE_PANEL_THRESHOLD {
            DisplayMode::Single
        } else {
            DisplayMode::Dual
        };
    }

    /// Toggle between left and right panel in single panel mode
    pub fn toggle_active_panel(&mut self) {
        self.active_panel = match self.active_panel {
            ActivePanel::Left => ActivePanel::Right,
            ActivePanel::Right => ActivePanel::Left,
        };
    }

    /// Get the currently active viewer for single panel mode
    fn active_viewer(&mut self) -> &mut CodeViewer {
        match self.active_panel {
            ActivePanel::Left => &mut self.left_viewer,
            ActivePanel::Right => &mut self.right_viewer,
        }
    }

    /// Get the filename of the active viewer
    fn active_filename(&self) -> String {
        match self.active_panel {
            ActivePanel::Left => self.left_viewer.filename(),
            ActivePanel::Right => self.right_viewer.filename(),
        }
    }

    /// Get the language name of the active viewer
    fn active_language(&self) -> String {
        match self.active_panel {
            ActivePanel::Left => self.left_viewer.language_name(),
            ActivePanel::Right => self.right_viewer.language_name(),
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
            // Split the area into two halves
            let divider = area.width / 2;
            let left_area = Rect::new(area.x, area.y, divider, area.height);
            let right_area = Rect::new(
                area.x + divider + 1,
                area.y,
                area.width - divider - 1,
                area.height,
            );

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
        // Handle key events
        match key.code {
            // Tab key toggles between panels in single panel mode
            crossterm::event::KeyCode::Tab => {
                if self.display_mode == DisplayMode::Single {
                    self.toggle_active_panel();
                    Ok(Some(Action::Render))
                } else {
                    Ok(None)
                }
            }
            crossterm::event::KeyCode::Up => {
                if self.display_mode == DisplayMode::Dual {
                    self.scroll_both(-1);
                } else {
                    self.active_viewer().scroll_up();
                }
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Down => {
                if self.display_mode == DisplayMode::Dual {
                    self.scroll_both(1);
                } else {
                    self.active_viewer().scroll_down();
                }
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::PageUp => {
                if self.display_mode == DisplayMode::Dual {
                    let lines = self.left_viewer.viewport_height() as i32;
                    self.scroll_both(-lines);
                } else {
                    let lines = self.active_viewer().viewport_height() as i32;
                    for _ in 0..lines {
                        self.active_viewer().scroll_up();
                    }
                }
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::PageDown => {
                if self.display_mode == DisplayMode::Dual {
                    let lines = self.left_viewer.viewport_height() as i32;
                    self.scroll_both(lines);
                } else {
                    let lines = self.active_viewer().viewport_height() as i32;
                    for _ in 0..lines {
                        self.active_viewer().scroll_down();
                    }
                }
                Ok(Some(Action::Render))
            }
            crossterm::event::KeyCode::Home => {
                if self.display_mode == DisplayMode::Dual {
                    self.left_viewer.scroll_to(0);
                    self.right_viewer.scroll_to(0);
                } else {
                    self.active_viewer().scroll_to(0);
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
                    let lines = self.active_viewer().line_count();
                    self.active_viewer().scroll_to(lines.saturating_sub(1));
                }
                Ok(Some(Action::Render))
            }
            _ => {
                // Let individual viewers handle other keys
                // For now, just forward to both
                let _ = self.left_viewer.handle_key_event(key)?;
                let _ = self.right_viewer.handle_key_event(key)?;
                Ok(None)
            }
        }
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Tick => {}
            Action::Render => {}
            Action::Resize(w, _h) => {
                // Update display mode based on new width
                self.update_display_mode(w);
            }
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

        // Update viewport heights based on current area
        let viewport_height = area.height.saturating_sub(4) as usize; // -4 for borders and padding
        self.left_viewer.set_viewport_height(viewport_height);
        self.right_viewer.set_viewport_height(viewport_height);

        if self.display_mode == DisplayMode::Dual {
            // Dual panel mode: show both side by side
            let divider_x = area.width / 2;
            let left_area = Rect::new(area.x, area.y, divider_x, area.height);
            let right_area = Rect::new(
                area.x + divider_x + 1,
                area.y,
                area.width - divider_x - 1,
                area.height,
            );

            // Draw left viewer (before)
            let left_block = Block::default()
                .title(Line::from(vec![Span::styled(
                    " Before ",
                    Style::new().bold().fg(Color::Red),
                )]))
                .borders(Borders::ALL)
                .border_set(border::ROUNDED)
                .border_style(Style::new().fg(Color::Red));

            let left_inner = left_block.inner(left_area);
            frame.render_widget(left_block, left_area);
            self.left_viewer.draw(frame, left_inner)?;

            // Draw right viewer (after)
            let right_block = Block::default()
                .title(Line::from(vec![Span::styled(
                    " After ",
                    Style::new().bold().fg(Color::Green),
                )]))
                .borders(Borders::ALL)
                .border_set(border::ROUNDED)
                .border_style(Style::new().fg(Color::Green));

            let right_inner = right_block.inner(right_area);
            frame.render_widget(right_block, right_area);
            self.right_viewer.draw(frame, right_inner)?;
        } else {
            // Single panel mode: show only one panel at a time
            // Determine border color based on active panel
            let border_color = match self.active_panel {
                ActivePanel::Left => Color::Red,
                ActivePanel::Right => Color::Green,
            };

            let panel_name = match self.active_panel {
                ActivePanel::Left => " Before ",
                ActivePanel::Right => " After ",
            };

            let filename = self.active_filename();
            let language = self.active_language();

            // Draw active panel with title showing filename and language
            let block = Block::default()
                .title(Line::from(vec![
                    Span::styled(panel_name, Style::new().bold().fg(border_color)),
                    Span::raw(" - "),
                    Span::styled(&filename, Style::new().bold().fg(Color::Cyan)),
                    Span::raw(" - "),
                    Span::styled(&language, Style::new().fg(Color::Gray)),
                    Span::raw(" (Tab to switch)"),
                ]))
                .borders(Borders::ALL)
                .border_set(border::ROUNDED)
                .border_style(Style::new().fg(border_color));

            let inner = block.inner(area);
            frame.render_widget(block, area);
            self.active_viewer().draw(frame, inner)?;
        }

        Ok(())
    }
}
