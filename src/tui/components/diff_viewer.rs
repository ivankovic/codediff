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
use crate::tui::actions::{Action, DiffSessionData};
use crate::tui::theme::OverlayTheme;

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
    /// Which panel is shown in single panel mode, and which panel's cursor drives navigation
    /// (and which panel `o` opens a file selector for) in dual panel mode.
    active_panel: Panel,
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

/// Which of the two panels is active.
#[derive(Default, Debug, Clone, Copy, PartialEq)]
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
        self.left_viewer.set_ranges(data.before_ranges.clone());
        self.right_viewer.set_ranges(data.after_ranges.clone());
        self.active_panel = Panel::Before;
        self.sync_focus();
        self.sync_cross_highlight();
    }

    /// Move the focused panel's cursor vertically (by one line) and push the resulting matched
    /// node onto the other panel's cross-highlight.
    pub fn move_cursor_vertical(&mut self, direction: i32) {
        self.focused_viewer().move_cursor_vertical(direction);
        self.sync_cross_highlight();
    }

    /// Move the focused panel's cursor horizontally (by one character) and push the resulting
    /// matched node onto the other panel's cross-highlight.
    pub fn move_cursor_horizontal(&mut self, direction: i32) {
        self.focused_viewer().move_cursor_horizontal(direction);
        self.sync_cross_highlight();
    }

    /// Push the focused panel's current cursor destination onto the other panel's
    /// cross-highlight; call after anything that can change the cursor or the focused panel.
    fn sync_cross_highlight(&mut self) {
        let destination = self.focused_viewer().cursor_destination();
        self.other_viewer().set_highlight_destination(destination);
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
        self.left_viewer.set_overlay_theme(theme);
        self.right_viewer.set_overlay_theme(theme);
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

    /// Load a single file (no diff overlay yet) into the "Before" panel.
    pub fn set_before_file(&mut self, path: PathBuf) -> Result<()> {
        self.left_viewer.load_file(path)
    }

    /// Load a single file (no diff overlay yet) into the "After" panel.
    pub fn set_after_file(&mut self, path: PathBuf) -> Result<()> {
        self.right_viewer.load_file(path)
    }

    /// Update display mode based on available width
    pub fn update_display_mode(&mut self, width: u16) {
        // Below this width, two side-by-side panels would each be too narrow to read code in.
        const SINGLE_PANEL_THRESHOLD: u16 = 220;
        self.display_mode = if width < SINGLE_PANEL_THRESHOLD {
            DisplayMode::Single
        } else {
            DisplayMode::Dual
        };
    }

    /// Toggle between the "Before" and "After" panel.
    pub fn toggle_active_panel(&mut self) {
        self.active_panel = match self.active_panel {
            Panel::Before => Panel::After,
            Panel::After => Panel::Before,
        };
        self.sync_focus();
    }

    /// Get the filename of the active viewer
    fn active_filename(&self) -> String {
        match self.active_panel {
            Panel::Before => self.left_viewer.filename(),
            Panel::After => self.right_viewer.filename(),
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

            let left_filename = self.left_viewer.filename_or_hint();
            let left_block = panel_block(
                " Before ",
                Color::Red,
                &left_filename,
                self.active_panel == Panel::Before,
            );
            let left_inner = left_block.inner(left_area);
            frame.render_widget(left_block, left_area);
            self.left_viewer.draw(frame, left_inner)?;

            let right_filename = self.right_viewer.filename_or_hint();
            let right_block = panel_block(
                " After ",
                Color::Green,
                &right_filename,
                self.active_panel == Panel::After,
            );
            let right_inner = right_block.inner(right_area);
            frame.render_widget(right_block, right_area);
            self.right_viewer.draw(frame, right_inner)?;

            let focused_inner = match self.active_panel {
                Panel::Before => left_inner,
                Panel::After => right_inner,
            };
            if let Some((x, y)) = self.focused_viewer().cursor_screen_position(focused_inner) {
                frame.set_cursor(x, y);
            }
        } else {
            // Single panel mode: show only one panel at a time
            // Determine border color based on active panel
            let border_color = match self.active_panel {
                Panel::Before => Color::Red,
                Panel::After => Color::Green,
            };

            let panel_name = match self.active_panel {
                Panel::Before => " Before ",
                Panel::After => " After ",
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
            self.focused_viewer().draw(frame, inner)?;

            if let Some((x, y)) = self.focused_viewer().cursor_screen_position(inner) {
                frame.set_cursor(x, y);
            }
        }

        Ok(())
    }
}

/// Build a dual-mode panel's border block, showing a thicker border and a bold title on
/// whichever side is active so `Tab` (and therefore `o`'s file-selector target) is visible.
fn panel_block<'a>(name: &'static str, color: Color, filename: &str, active: bool) -> Block<'a> {
    let title_style = if active {
        Style::new().bold().fg(Color::Black).bg(color)
    } else {
        Style::new().bold().fg(color)
    };
    Block::default()
        .title(Line::from(vec![
            Span::styled(name, title_style),
            Span::raw(format!(" {filename}")),
        ]))
        .borders(Borders::ALL)
        .border_set(if active {
            border::THICK
        } else {
            border::ROUNDED
        })
        .border_style(Style::new().fg(color))
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
        }
    }

    /// Regression test for the exploratory-testing bug: after `Tab` moves focus to "After",
    /// exactly the "After" side shows its own cursor highlight and the "Before" side shows only
    /// the pushed cross-highlight - never the other way around, and never both blues on one
    /// panel at once.
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
}
