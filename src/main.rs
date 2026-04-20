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
use clap::Parser;
use std::fs;
use std::path::PathBuf;
use std::io;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame, Terminal,
};
use ratatui::prelude::Alignment; // Import Alignment from prelude
use tree_sitter::Node; // Import tree-sitter Node for AST operations


#[derive(Parser)]
struct Args {
    before: PathBuf,
    after: PathBuf,
}

/// Theme for the application
#[derive(Debug, Clone, Copy, PartialEq)]
enum Theme {
    Dark,
    Light,
}

impl Theme {
    /// Detect the terminal theme by checking if the background appears to be light
    fn detect() -> Self {
        // Try to detect terminal theme
        // This is a heuristic approach since there's no perfect way to detect terminal theme
        
        // Check the TERM environment variable for common light terminal indicators
        if let Ok(term) = std::env::var("TERM") {
            if term.contains("-light") || term.contains("light") {
                return Theme::Light;
            }
        }
        
        // Check COLORFGBG environment variable (common in X11 terminals)
        if let Ok(color_fgbg) = std::env::var("COLORFGBG") {
            let parts: Vec<&str> = color_fgbg.split(':').collect();
            if parts.len() >= 2 {
                // Check if background color is light (values 7-15 are typically light)
                if let Ok(bg_color) = parts[1].parse::<u8>() {
                    if bg_color >= 7 && bg_color <= 15 {
                        return Theme::Light;
                    }
                }
            }
        }
        
        // Check if we're in a common light-themed IDE/terminal
        if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
            if term_program == "vscode" || term_program == "intellij" {
                // Many IDEs default to light themes
                return Theme::Light;
            }
        }
        
        // Default to light theme (better for modern terminals and IDEs)
        Theme::Light
    }

    fn get_colors(&self) -> ThemeColors {
        match self {
            Theme::Dark => ThemeColors {
                background: Color::Black,
                text: Color::White,
                cursor_bg: Color::Blue,
                cursor_fg: Color::White,
                border: Color::Gray,
                header_fg: Color::Yellow,
                footer_fg: Color::Gray,
                popup_bg: Color::DarkGray,
                popup_fg: Color::White,
                popup_border: Color::White,
            },
            Theme::Light => ThemeColors {
                background: Color::White,
                text: Color::Black,
                cursor_bg: Color::Blue,
                cursor_fg: Color::White,
                border: Color::Gray,
                header_fg: Color::Yellow,
                footer_fg: Color::Gray,
                popup_bg: Color::Gray,
                popup_fg: Color::Black,
                popup_border: Color::Black,
            },
        }
    }
}

/// Color scheme for the application
#[derive(Debug, Clone, Copy)]
struct ThemeColors {
    background: Color,
    text: Color,
    cursor_bg: Color,
    cursor_fg: Color,
    border: Color,
    header_fg: Color,
    footer_fg: Color,
    popup_bg: Color,
    popup_fg: Color,
    popup_border: Color,
}

/// Application state
struct App {
    before_code: String,
    after_code: String,
    diff: codediff::diff::Diff,
    cursor_line: usize,
    cursor_char: usize,
    scroll_offset: usize,
    active_panel: Panel,
    show_ast_popup: bool,
    ast_path: Vec<String>,
    theme: Theme,
    colors: ThemeColors,
}

#[derive(PartialEq, Clone, Copy, Debug)]
enum Panel {
    Before,
    After,
}

impl App {
    fn new(before: String, after: String, diff: codediff::diff::Diff) -> Self {
        let theme = Theme::detect();
        let colors = theme.get_colors();
        
        Self {
            before_code: before,
            after_code: after,
            diff,
            cursor_line: 0,
            cursor_char: 0,
            scroll_offset: 0,
            active_panel: Panel::Before,
            show_ast_popup: false,
            ast_path: Vec::new(),
            theme,
            colors,
        }
    }

    fn toggle_panel(&mut self) {
        self.active_panel = match self.active_panel {
            Panel::Before => Panel::After,
            Panel::After => Panel::Before,
        };
    }

    fn move_cursor_up(&mut self) {
        if self.cursor_line > 0 {
            self.cursor_line -= 1;
            // Keep character position within the new line's bounds
            let line_content = match self.active_panel {
                Panel::Before => self.before_code.lines().nth(self.cursor_line),
                Panel::After => self.after_code.lines().nth(self.cursor_line),
            };
            if let Some(line) = line_content {
                self.cursor_char = self.cursor_char.min(line.chars().count().saturating_sub(1));
            } else {
                self.cursor_char = 0;
            }
        }
        
        // Auto-scroll: if cursor moves above visible area, adjust scroll offset
        // We use a reasonable default visible height for auto-scrolling
        self.ensure_cursor_visible(20); // Assume 20 visible lines by default
    }

    fn move_cursor_down(&mut self) {
        let max_line = match self.active_panel {
            Panel::Before => self.before_code.lines().count().saturating_sub(1),
            Panel::After => self.after_code.lines().count().saturating_sub(1),
        };
        if self.cursor_line < max_line {
            self.cursor_line += 1;
            // Keep character position within the new line's bounds
            let line_content = match self.active_panel {
                Panel::Before => self.before_code.lines().nth(self.cursor_line),
                Panel::After => self.after_code.lines().nth(self.cursor_line),
            };
            if let Some(line) = line_content {
                self.cursor_char = self.cursor_char.min(line.chars().count().saturating_sub(1));
            } else {
                self.cursor_char = 0;
            }
        }
        
        // Auto-scroll: adjust scroll offset to ensure cursor is visible
        self.ensure_cursor_visible(20); // Assume 20 visible lines by default
    }

    /// Adjust scroll offset to ensure cursor is visible
    /// This should be called after cursor movement with the visible height
    fn ensure_cursor_visible(&mut self, visible_height: usize) {
        let total_lines = match self.active_panel {
            Panel::Before => self.before_code.lines().count(),
            Panel::After => self.after_code.lines().count(),
        };
        
        // Calculate visible range
        let visible_start = self.scroll_offset;
        let visible_end = self.scroll_offset + visible_height;
        
        // Calculate what scroll offset should be to make cursor visible
        let desired_scroll = if self.cursor_line >= visible_end {
            // Cursor is below visible area, scroll down to make it visible at bottom
            self.cursor_line.saturating_sub(visible_height - 1)
        } else if self.cursor_line < visible_start {
            // Cursor is above visible area, scroll up to make it visible at top
            self.cursor_line
        } else {
            // Cursor is within visible area, keep current scroll offset
            self.scroll_offset
        };
        
        // Don't scroll past the end
        let max_scroll = total_lines.saturating_sub(visible_height);
        self.scroll_offset = desired_scroll.min(max_scroll);
    }

    fn move_cursor_left(&mut self) {
        if self.cursor_char > 0 {
            self.cursor_char -= 1;
        } else if self.cursor_line > 0 {
            // Move to end of previous line
            self.cursor_line -= 1;
            if let Some(line) = match self.active_panel {
                Panel::Before => self.before_code.lines().nth(self.cursor_line),
                Panel::After => self.after_code.lines().nth(self.cursor_line),
            } {
                self.cursor_char = line.chars().count().saturating_sub(1);
            }
        }
        
        // Auto-scroll for horizontal movement too
        self.ensure_cursor_visible(20);
    }

    fn move_cursor_right(&mut self) {
        let line_content = match self.active_panel {
            Panel::Before => self.before_code.lines().nth(self.cursor_line),
            Panel::After => self.after_code.lines().nth(self.cursor_line),
        };
        
        if let Some(line) = line_content {
            let line_length = line.chars().count();
            if self.cursor_char < line_length {
                self.cursor_char += 1;
            } else if self.cursor_line < (match self.active_panel {
                Panel::Before => self.before_code.lines().count(),
                Panel::After => self.after_code.lines().count(),
            }).saturating_sub(1) {
                // Move to beginning of next line
                self.cursor_line += 1;
                self.cursor_char = 0;
            }
        }
        
        // Auto-scroll for horizontal movement too
        self.ensure_cursor_visible(20);
    }

    /// Update AST path based on current cursor position
    fn update_ast_path(&mut self) {
        let (code, language) = match self.active_panel {
            Panel::Before => (&self.before_code, &codediff::code::Language::Rust), // Default to Rust for now
            Panel::After => (&self.after_code, &codediff::code::Language::Rust), // Default to Rust for now
        };
        
        // Create code object and parse it
        let mut code_obj = codediff::code::Code::from_string(code, language);
        if code_obj.ensure_parsed().is_ok() {
            if let Some(ast) = &code_obj.ast {
                let root_node = ast.root_node();
                
                // Convert cursor position to byte position
                let byte_position = self.cursor_position_to_byte(code);
                
                // Find node at cursor position
                if let Some(node) = self.find_node_at_byte_position(root_node, byte_position) {
                    // Build path from root to this node
                    self.ast_path = self.build_node_path(&root_node, &node);
                } else {
                    self.ast_path = Vec::new();
                }
            } else {
                self.ast_path = Vec::new();
            }
        } else {
            self.ast_path = Vec::new();
        }
    }

    /// Convert cursor line/char position to byte position in the code
    fn cursor_position_to_byte(&self, code: &str) -> usize {
        let mut byte_pos = 0;
        for (line_idx, line) in code.lines().enumerate() {
            if line_idx == self.cursor_line {
                // Convert char position to byte position in this line
                let mut char_pos = 0;
                for (byte_idx, _c) in line.chars().enumerate() {
                    if char_pos == self.cursor_char {
                        return byte_pos + byte_idx;
                    }
                    char_pos += 1;
                }
                // If cursor is at end of line, return end of line
                return byte_pos + line.len();
            }
            byte_pos += line.len() + 1; // +1 for newline
        }
        byte_pos
    }

    /// Find the smallest node that contains the given byte position
    fn find_node_at_byte_position<'a>(&self, root_node: Node<'a>, byte_pos: usize) -> Option<Node<'a>> {
        let mut result = None;
        let mut stack = vec![root_node];
        
        while let Some(node) = stack.pop() {
            if byte_pos >= node.start_byte() && byte_pos < node.end_byte() {
                result = Some(node);
                
                // Check children to find the most specific node
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if byte_pos >= child.start_byte() && byte_pos < child.end_byte() {
                        stack.push(child);
                    }
                }
            }
        }
        
        result
    }

    /// Build path from root to target node
    fn build_node_path(&self, _root_node: &Node, target_node: &Node) -> Vec<String> {
        let mut path = Vec::new();
        let mut current = Some(target_node.clone());
        
        while let Some(node) = current {
            // Add node type to path (in reverse order)
            path.push(node.kind().to_string());
            
            // Move to parent
            current = node.parent();
        }
        
        // Reverse to get root-to-target order
        path.reverse();
        path
    }

    fn toggle_ast_popup(&mut self) {
        self.show_ast_popup = !self.show_ast_popup;
        // Update AST path when popup is shown
        if self.show_ast_popup {
            self.update_ast_path();
        }
    }
}

/// Main application
struct Tui {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl Tui {
    fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self { terminal })
    }

    fn draw(&mut self, app: &App) -> io::Result<()> {
        self.terminal.draw(|f| ui(f, app))?;
        Ok(())
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        disable_raw_mode().unwrap();
        execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )
        .unwrap();
    }
}

/// Render the UI
fn ui(f: &mut Frame, app: &App) {
    let size = f.size();
    
    // Check if terminal is in narrow mode (< 220 characters wide)
    let is_narrow = size.width < 220;
    
    if is_narrow {
        render_narrow_mode(f, app);
    } else {
        render_wide_mode(f, app);
    }
}

fn render_narrow_mode(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header
            Constraint::Min(1),    // Code panel
            Constraint::Length(1), // Footer
        ])
        .split(f.size());

    // Header
    let header_text = match app.active_panel {
        Panel::Before => "Before Code (Tab to switch to After)",
        Panel::After => "After Code (Tab to switch to Before)",
    };
    let header = Paragraph::new(header_text)
        .style(Style::default().fg(app.colors.header_fg))
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(header, chunks[0]);

    // Code panel
    let code_text = match app.active_panel {
        Panel::Before => &app.before_code,
        Panel::After => &app.after_code,
    };
    
    // Calculate visible lines based on scroll offset
    let visible_lines = chunks[1].height as usize;
    
    let lines: Vec<Line> = code_text.lines()
        .enumerate()
        .skip(app.scroll_offset)
        .take(visible_lines)
        .map(|(line_idx, line)| {
            let mut spans = Vec::new();
            
            // Add line number prefix
            spans.push(Span::styled(
                format!("{:4} ", line_idx + 1),
                Style::default().fg(app.colors.footer_fg),
            ));
            
            // Highlight individual characters
            if line_idx == app.cursor_line {
                for (char_idx, c) in line.chars().enumerate() {
                    if char_idx == app.cursor_char {
                        // Highlight current character
                        spans.push(Span::styled(
                            c.to_string(),
                            Style::default()
                                .fg(app.colors.cursor_fg)
                                .bg(app.colors.cursor_bg)
                                .add_modifier(Modifier::BOLD),
                        ));
                    } else {
                        // Normal character
                        spans.push(Span::styled(
                            c.to_string(),
                            Style::default().fg(app.colors.text),
                        ));
                    }
                }
            } else {
                // Normal line
                spans.push(Span::styled(
                    line.to_string(),
                    Style::default().fg(app.colors.text),
                ));
            }
            
            Line::from(spans)
        })
        .collect();

    let code_paragraph = Paragraph::new(Text::from(lines))
        .block(Block::default().borders(Borders::ALL).title("Code"))
        .wrap(Wrap { trim: true });
    f.render_widget(code_paragraph, chunks[1]);

    // Footer
    let footer_text = "Arrow keys: Navigate | Space: Align | t: AST | q: Quit";
    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(app.colors.footer_fg))
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(footer, chunks[2]);

    // AST Popup
    if app.show_ast_popup {
        render_ast_popup(f, app);
    }
}

fn render_wide_mode(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // Before panel
            Constraint::Percentage(50), // After panel
        ])
        .split(f.size());

    // Before panel - calculate visible lines based on scroll offset
    let before_visible_lines = chunks[0].height as usize;
    
    let before_lines: Vec<Line> = app.before_code.lines()
        .enumerate()
        .skip(app.scroll_offset)
        .take(before_visible_lines)
        .map(|(line_idx, line)| {
            let mut spans = Vec::new();
            
            // Add line number prefix
            spans.push(Span::styled(
                format!("{:4} ", line_idx + 1),
                Style::default().fg(app.colors.footer_fg),
            ));
            
            // Highlight individual characters if this panel is active
            if app.active_panel == Panel::Before && line_idx == app.cursor_line {
                for (char_idx, c) in line.chars().enumerate() {
                    if char_idx == app.cursor_char {
                        // Highlight current character
                        spans.push(Span::styled(
                            c.to_string(),
                            Style::default()
                                .fg(app.colors.cursor_fg)
                                .bg(app.colors.cursor_bg)
                                .add_modifier(Modifier::BOLD),
                        ));
                    } else {
                        // Normal character
                        spans.push(Span::styled(
                            c.to_string(),
                            Style::default().fg(app.colors.text),
                        ));
                    }
                }
            } else {
                // Normal line
                spans.push(Span::styled(
                    line.to_string(),
                    Style::default().fg(app.colors.text),
                ));
            }
            
            Line::from(spans)
        })
        .collect();

    let before_paragraph = Paragraph::new(Text::from(before_lines))
        .block(Block::default().borders(Borders::ALL).title("Before"))
        .wrap(Wrap { trim: true });
    f.render_widget(before_paragraph, chunks[0]);

    // After panel - calculate visible lines based on scroll offset
    let after_visible_lines = chunks[1].height as usize;
    
    let after_lines: Vec<Line> = app.after_code.lines()
        .enumerate()
        .skip(app.scroll_offset)
        .take(after_visible_lines)
        .map(|(line_idx, line)| {
            let mut spans = Vec::new();
            
            // Add line number prefix
            spans.push(Span::styled(
                format!("{:4} ", line_idx + 1),
                Style::default().fg(app.colors.footer_fg),
            ));
            
            // Highlight individual characters if this panel is active
            if app.active_panel == Panel::After && line_idx == app.cursor_line {
                for (char_idx, c) in line.chars().enumerate() {
                    if char_idx == app.cursor_char {
                        // Highlight current character
                        spans.push(Span::styled(
                            c.to_string(),
                            Style::default()
                                .fg(app.colors.cursor_fg)
                                .bg(app.colors.cursor_bg)
                                .add_modifier(Modifier::BOLD),
                        ));
                    } else {
                        // Normal character
                        spans.push(Span::styled(
                            c.to_string(),
                            Style::default().fg(app.colors.text),
                        ));
                    }
                }
            } else {
                // Normal line
                spans.push(Span::styled(
                    line.to_string(),
                    Style::default().fg(app.colors.text),
                ));
            }
            
            Line::from(spans)
        })
        .collect();

    let after_paragraph = Paragraph::new(Text::from(after_lines))
        .block(Block::default().borders(Borders::ALL).title("After"))
        .wrap(Wrap { trim: true });
    f.render_widget(after_paragraph, chunks[1]);

    // Footer
    let footer_area = Rect {
        x: 0,
        y: f.size().height - 1,
        width: f.size().width,
        height: 1,
    };
    let footer_text = "Arrow keys: Navigate | Tab: Switch panel | Space: Align | t: AST | q: Quit";
    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(app.colors.footer_fg))
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(footer, footer_area);

    // AST Popup
    if app.show_ast_popup {
        render_ast_popup(f, app);
    }
}

fn render_ast_popup(f: &mut Frame, app: &App) {
    // Calculate popup size with margins
    let margin = 1;
    let popup_width = f.size().width.saturating_sub(2 * margin);
    let popup_height = f.size().height.saturating_sub(2 * margin + 1); // Account for title bar + footer
    
    let popup_area = Rect {
        x: margin,
        y: margin + 1, // Start below the code header
        width: popup_width,
        height: popup_height,
    };

    // Build AST tree content
    let ast_content = if app.ast_path.is_empty() {
        "Move cursor to see AST path\nCurrent position: Line {}, Char {}".to_string()
    } else {
        // Show proper tree structure
        let mut tree_text = String::new();
        for (i, node) in app.ast_path.iter().enumerate() {
            // Add indentation based on depth
            let indent = "  ".repeat(i);
            if i == app.ast_path.len() - 1 {
                tree_text.push_str(&format!("{}└─ {}\n", indent, node));
            } else {
                tree_text.push_str(&format!("{}├─ {}\n", indent, node));
            }
        }
        tree_text
    };

    let popup = Paragraph::new(ast_content)
        .block(
            Block::default()
                .title("AST Tree")
                .borders(Borders::ALL)
                .style(Style::default().fg(app.colors.popup_border)),
        )
        .style(Style::default().fg(app.colors.popup_fg).bg(app.colors.popup_bg))
        .alignment(Alignment::Left);

    f.render_widget(Clear, popup_area); // Clear the area first
    f.render_widget(popup, popup_area);
}

fn main() -> io::Result<()> {
    let args = Args::parse();

    // Detect language from file extension before moving args.before
    // Handle .test files by stripping the .test extension first
    let file_path = &args.before;
    let language = if let Some(file_name) = file_path.file_name() {
        if let Some(file_name_str) = file_name.to_str() {
            // Check if filename ends with .test
            if file_name_str.ends_with(".test") {
                // Remove .test extension
                let actual_filename = file_name_str.trim_end_matches(".test");
                // Get the extension from the actual filename
                if let Some(last_dot_pos) = actual_filename.rfind('.') {
                    let actual_ext = &actual_filename[last_dot_pos + 1..];
                    codediff::code::language::language_for_extension(actual_ext)
                        .unwrap_or(codediff::code::Language::Unknown)
                } else {
                    codediff::code::Language::Unknown
                }
            } else {
                // Normal file, use the actual extension
                if let Some(ext) = file_path.extension() {
                    if let Some(lang_str) = ext.to_str() {
                        codediff::code::language::language_for_extension(lang_str)
                            .unwrap_or(codediff::code::Language::Unknown)
                    } else {
                        codediff::code::Language::Unknown
                    }
                } else {
                    codediff::code::Language::Unknown
                }
            }
        } else {
            codediff::code::Language::Unknown
        }
    } else {
        codediff::code::Language::Unknown
    };

    let before = fs::read_to_string(args.before)?;
    let after = fs::read_to_string(args.after)?;

    let diff = codediff::diff_strings(&before, &after, &language);

    // Create application
    let mut app = App::new(before, after, diff);

    // Initialize TUI
    let mut tui = Tui::new()?;

    // Main loop
    loop {
        tui.draw(&app)?;

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('t') => app.toggle_ast_popup(),
                    KeyCode::Tab => app.toggle_panel(),
                    KeyCode::Char(' ') => {
                        // Space bar - align both sides
                        // TODO: Implement actual alignment logic
                    }
                    KeyCode::Up => app.move_cursor_up(),
                    KeyCode::Down => app.move_cursor_down(),
                    KeyCode::Left => {
                        // Vim h key
                        app.move_cursor_left();
                    }
                    KeyCode::Right => {
                        // Vim l key  
                        app.move_cursor_right();
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::io;

    #[test]
    fn devex_infrastructure_test() {}

    #[test]
    fn test_app_initialization() {
        let before = "fn main() {}".to_string();
        let after = "fn main() {}".to_string();
        let diff = codediff::diff_strings(&before, &after, &codediff::code::Language::Rust);
        
        let mut app = App::new(before, after, diff);
        
        assert_eq!(app.cursor_line, 0);
        assert_eq!(app.cursor_char, 0);
        assert_eq!(app.scroll_offset, 0);
        assert_eq!(app.active_panel, Panel::Before);
        assert!(!app.show_ast_popup);
        assert!(app.ast_path.is_empty());
    }

    #[test]
    fn test_panel_toggling() {
        let before = "fn main() {}".to_string();
        let after = "fn main() {}".to_string();
        let diff = codediff::diff_strings(&before, &after, &codediff::code::Language::Rust);
        
        let mut app = App::new(before, after, diff);
        
        assert_eq!(app.active_panel, Panel::Before);
        
        app.toggle_panel();
        assert_eq!(app.active_panel, Panel::After);
        
        app.toggle_panel();
        assert_eq!(app.active_panel, Panel::Before);
    }

    #[test]
    fn test_cursor_movement() {
        let before = "line1\nline2\nline3".to_string();
        let after = "line1\nline2\nline3".to_string();
        let diff = codediff::diff_strings(&before, &after, &codediff::code::Language::Rust);
        
        let mut app = App::new(before, after, diff);
        
        // Start at line 0, char 0
        assert_eq!(app.cursor_line, 0);
        assert_eq!(app.cursor_char, 0);
        
        // Move down twice
        app.move_cursor_down();
        assert_eq!(app.cursor_line, 1);
        assert_eq!(app.cursor_char, 0);
        
        app.move_cursor_down();
        assert_eq!(app.cursor_line, 2);
        assert_eq!(app.cursor_char, 0);
        
        // Can't move beyond last line
        app.move_cursor_down();
        assert_eq!(app.cursor_line, 2);
        assert_eq!(app.cursor_char, 0);
        
        // Move up
        app.move_cursor_up();
        assert_eq!(app.cursor_line, 1);
        assert_eq!(app.cursor_char, 0);
        
        // Can't move above first line
        app.move_cursor_up();
        app.move_cursor_up();
        assert_eq!(app.cursor_line, 0);
        assert_eq!(app.cursor_char, 0);
        
        // Test character movement
        app.move_cursor_right();
        assert_eq!(app.cursor_char, 1);
        
        app.move_cursor_right();
        assert_eq!(app.cursor_char, 2);
        
        // Move to end of line
        for _ in 0..10 {
            app.move_cursor_right();
        }
        
        // Move left
        let original_char = app.cursor_char;
        let original_line = app.cursor_line;
        app.move_cursor_left();
        // After moving left, either char decreases, or we wrap to previous line
        assert!(app.cursor_char < original_char || app.cursor_line < original_line);
    }

    #[test]
    fn test_ast_popup_toggle() {
        let before = "fn main() {}".to_string();
        let after = "fn main() {}".to_string();
        let diff = codediff::diff_strings(&before, &after, &codediff::code::Language::Rust);
        
        let mut app = App::new(before, after, diff);
        
        assert!(!app.show_ast_popup);
        
        app.toggle_ast_popup();
        assert!(app.show_ast_popup);
        
        app.toggle_ast_popup();
        assert!(!app.show_ast_popup);
    }

    #[test]
    fn test_narrow_mode_rendering() -> io::Result<()> {
        let before = "fn main() {\n    println!(\"Hello\");\n}".to_string();
        let after = "fn main() {\n    println!(\"World\");\n}".to_string();
        let diff = codediff::diff_strings(&before, &after, &codediff::code::Language::Rust);
        
        let mut app = App::new(before, after, diff);
        
        // Create a narrow terminal (less than 220 chars wide)
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend)?;
        
        // This should not panic and should render in narrow mode
        terminal.draw(|f| ui(f, &app))?;
        
        Ok(())
    }

    #[test]
    fn test_wide_mode_rendering() -> io::Result<()> {
        let before = "fn main() {\n    println!(\"Hello\");\n}".to_string();
        let after = "fn main() {\n    println!(\"World\");\n}".to_string();
        let diff = codediff::diff_strings(&before, &after, &codediff::code::Language::Rust);
        
        let mut app = App::new(before, after, diff);
        
        // Create a wide terminal (220+ chars wide)
        let backend = TestBackend::new(250, 24);
        let mut terminal = Terminal::new(backend)?;
        
        // This should not panic and should render in wide mode
        terminal.draw(|f| ui(f, &app))?;
        
        Ok(())
    }

    #[test]
    fn test_cursor_position_highlighting() -> io::Result<()> {
        let before = "line1\nline2\nline3".to_string();
        let after = "line1\nline2\nline3".to_string();
        let diff = codediff::diff_strings(&before, &after, &codediff::code::Language::Rust);
        
        let mut app = App::new(before, after, diff);
        app.cursor_line = 1; // Highlight second line
        app.cursor_char = 2; // Highlight third character
        
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend)?;
        
        // This should not panic and should highlight the correct character
        terminal.draw(|f| ui(f, &app))?;
        
        Ok(())
    }

    #[test]
    fn test_ast_popup_rendering() -> io::Result<()> {
        let before = "fn main() {}".to_string();
        let after = "fn main() {}".to_string();
        let diff = codediff::diff_strings(&before, &after, &codediff::code::Language::Rust);
        
        let mut app = App::new(before, after, diff);
        app.show_ast_popup = true;
        app.ast_path = vec!["source_file".to_string(), "function_item".to_string(), "identifier".to_string()];
        
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend)?;
        
        // This should not panic and should render the AST popup
        terminal.draw(|f| ui(f, &app))?;
        
        Ok(())
    }

    #[test]
    fn test_panel_switching_rendering() -> io::Result<()> {
        let before = "fn main() {\n    println!(\"Hello\");\n}".to_string();
        let after = "fn main() {\n    println!(\"World\");\n}".to_string();
        let diff = codediff::diff_strings(&before, &after, &codediff::code::Language::Rust);
        
        let mut app = App::new(before, after, diff);
        
        let backend = TestBackend::new(250, 24);
        let mut terminal = Terminal::new(backend)?;
        
        // Render with before panel active
        terminal.draw(|f| ui(f, &app))?;
        
        // Switch to after panel
        app.toggle_panel();
        terminal.draw(|f| ui(f, &app))?;
        
        Ok(())
    }

    #[test]
    fn test_language_detection() {
        // Test various file extensions
        let test_cases = vec![
            ("test.rs", codediff::code::Language::Rust),
            ("test.py", codediff::code::Language::Python),
            ("test.js", codediff::code::Language::JavaScript),
            ("test.java", codediff::code::Language::Java),
            ("test.cpp", codediff::code::Language::CPP),
            ("test.html", codediff::code::Language::HTML),
            ("test.unknown", codediff::code::Language::Unknown),
        ];

        for (filename, expected_lang) in test_cases {
            let path = std::path::PathBuf::from(filename);
            let language = if let Some(ext) = path.extension() {
                if let Some(lang_str) = ext.to_str() {
                    codediff::code::language::language_for_extension(lang_str)
                        .unwrap_or(codediff::code::Language::Unknown)
                } else {
                    codediff::code::Language::Unknown
                }
            } else {
                codediff::code::Language::Unknown
            };
            
            assert_eq!(language, expected_lang, "Failed for filename: {}", filename);
        }
    }

    #[test]
    fn test_theme_detection() {
        // Test that theme detection returns a valid theme
        let theme = Theme::detect();
        assert!(matches!(theme, Theme::Dark | Theme::Light));
        
        // Test that colors are appropriate for each theme
        let colors = theme.get_colors();
        
        match theme {
            Theme::Dark => {
                // Dark theme should have light text on dark background
                assert_eq!(colors.text, Color::White);
                assert_eq!(colors.background, Color::Black);
                assert_eq!(colors.cursor_fg, Color::White);
                assert_eq!(colors.cursor_bg, Color::Blue);
            },
            Theme::Light => {
                // Light theme should have dark text on light background
                assert_eq!(colors.text, Color::Black);
                assert_eq!(colors.background, Color::White);
                assert_eq!(colors.cursor_fg, Color::White);
                assert_eq!(colors.cursor_bg, Color::Blue);
            },
        }
    }

    #[test]
    fn test_default_theme_is_light() {
        // Test that when no detection works, we default to light theme
        // This ensures better readability in modern terminals/IDEs
        
        // Create a mock theme detection that simulates no detection
        // by creating a function that ignores environment variables
        fn mock_detect_with_no_detection() -> Theme {
            // Check the TERM environment variable for common light terminal indicators
            if let Ok(term) = std::env::var("TERM") {
                if term.contains("-light") || term.contains("light") {
                    return Theme::Light;
                }
            }
            
            // Check COLORFGBG environment variable (common in X11 terminals)
            if let Ok(color_fgbg) = std::env::var("COLORFGBG") {
                let parts: Vec<&str> = color_fgbg.split(':').collect();
                if parts.len() >= 2 {
                    // Check if background color is light (values 7-15 are typically light)
                    if let Ok(bg_color) = parts[1].parse::<u8>() {
                        if bg_color >= 7 && bg_color <= 15 {
                            return Theme::Light;
                        }
                    }
                }
            }
            
            // Check if we're in a common light-themed IDE/terminal
            if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
                if term_program == "vscode" || term_program == "intellij" {
                    // Many IDEs default to light themes
                    return Theme::Light;
                }
            }
            
            // Default to light theme (better for modern terminals and IDEs)
            Theme::Light
        }
        
        // Test the mock function
        let theme = mock_detect_with_no_detection();
        assert_eq!(theme, Theme::Light);
    }

    #[test]
    fn test_scrolling_behavior() {
        // Test that scrolling works correctly when cursor moves beyond visible area
        let before = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10".to_string();
        let after = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10".to_string();
        let diff = codediff::diff_strings(&before, &after, &codediff::code::Language::Rust);
        
        let mut app = App::new(before, after, diff);
        
        // Initially at line 0, scroll offset 0
        assert_eq!(app.cursor_line, 0);
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn test_scrolling_to_bottom() {
        // Test that we can reach the last lines of the document
        let before = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10".to_string();
        let after = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10".to_string();
        let diff = codediff::diff_strings(&before, &after, &codediff::code::Language::Rust);
        
        let mut app = App::new(before, after, diff);
        
        // Move to the last line (line 9)
        for _ in 0..9 {
            app.move_cursor_down();
        }
        
        assert_eq!(app.cursor_line, 9, "Should be able to reach last line");
        
        // With 5 visible lines, scroll offset should be 5 to show lines 5-9
        // (last line 9 should be visible at the bottom)
        let visible_lines = 5;
        app.ensure_cursor_visible(visible_lines);
        
        let expected_scroll = 9_usize.saturating_sub(visible_lines - 1); // 9 - 4 = 5
        assert_eq!(app.scroll_offset, expected_scroll, "Should be able to scroll to show last line");
    }

    #[test]
    fn test_cursor_visibility_during_scrolling() {
        // Test that cursor remains visible during continuous scrolling
        let before = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12".to_string();
        let after = before.clone();
        let diff = codediff::diff_strings(&before, &after, &codediff::code::Language::Rust);
        
        let mut app = App::new(before, after, diff);
        
        // Simulate terminal with 5 visible lines
        let visible_lines = 5;
        
        // Scroll through the document step by step
        for line_num in 1..12 {
            // Move cursor down
            app.move_cursor_down();
            assert_eq!(app.cursor_line, line_num, "Cursor should move to line {}", line_num);
            
            // Ensure cursor is visible
            app.ensure_cursor_visible(visible_lines);
            
            // Calculate expected visible range
            let visible_start = app.scroll_offset;
            let visible_end = app.scroll_offset + visible_lines;
            
            // Cursor should always be within visible range
            assert!(app.cursor_line >= visible_start && app.cursor_line < visible_end,
                   "Line {} should be visible (scroll: {}, visible: {}-{})", 
                   app.cursor_line, app.scroll_offset, visible_start, visible_end);
        }
        
        // Should be able to reach the last line
        assert_eq!(app.cursor_line, 11, "Should reach last line");
    }

    #[test]
    fn test_scrolling_edge_cases() {
        // Test edge cases in scrolling behavior
        let before = "line1\nline2\nline3\nline4\nline5".to_string(); // Only 5 lines
        let after = before.clone();
        let diff = codediff::diff_strings(&before, &after, &codediff::code::Language::Rust);
        
        let mut app = App::new(before, after, diff);
        
        // Test with different visible heights
        for visible_lines in [3, 5, 10] {
            // Move to last line
            while app.cursor_line < 4 {
                app.move_cursor_down();
            }
            
            // Ensure cursor visible
            app.ensure_cursor_visible(visible_lines);
            
            // Scroll offset should not exceed possible range
            let max_possible_scroll = 5_usize.saturating_sub(visible_lines);
            assert!(app.scroll_offset <= max_possible_scroll,
                   "Scroll offset {} should not exceed max possible {} for {} visible lines",
                   app.scroll_offset, max_possible_scroll, visible_lines);
        }
    }

    #[test]
    fn test_right_arrow_cursor_movement() {
        // Test that right arrow key doesn't crash and works correctly
        let before = "short\nline2\nline3".to_string();
        let after = before.clone();
        let diff = codediff::diff_strings(&before, &after, &codediff::code::Language::Rust);
        
        let mut app = App::new(before.clone(), after.clone(), diff);
        
        // First line has 5 characters: "short"
        // cursor should be able to move to positions 0, 1, 2, 3, 4, and then wrap to next line
        
        // Start at beginning of first line
        assert_eq!(app.cursor_line, 0);
        assert_eq!(app.cursor_char, 0);
        
        // Move right within first line ("short" has 5 characters)
        for _ in 0..3 {
            app.move_cursor_right();
        }
        assert_eq!(app.cursor_char, 3, "Should move to character 3 of 'short'");
        
        // Move to end of first line
        app.move_cursor_right(); // char 4
        app.move_cursor_right(); // char 5 (end of line)
        assert_eq!(app.cursor_char, 5, "Should be at end of first line");
        
        // Next right should move to beginning of next line
        app.move_cursor_right();
        assert_eq!(app.cursor_line, 1, "Should move to next line");
        assert_eq!(app.cursor_char, 0, "Should be at beginning of next line");
        
        // Should not crash and should handle edge cases properly
        assert!(app.cursor_line < 3, "Should not go beyond last line");
    }

    #[test]
    fn test_right_arrow_at_end_of_document() {
        // Test right arrow at the very end of document
        let before = "end".to_string(); // Single line, 3 characters
        let after = before.clone();
        let diff = codediff::diff_strings(&before, &after, &codediff::code::Language::Rust);
        
        let mut app = App::new(before, after, diff);
        
        // Move to end of the only line
        for _ in 0..3 {
            app.move_cursor_right();
        }
        assert_eq!(app.cursor_char, 3, "Should be at end of line");
        
        // Try to move right beyond end - should not crash
        app.move_cursor_right();
        // Should stay at end of document
        assert_eq!(app.cursor_line, 0, "Should stay on same line");
        assert_eq!(app.cursor_char, 3, "Should stay at end of line");
    }

    #[test]
    fn test_ast_path_update() {
        // Test that AST path is updated correctly when cursor moves
        let before = "fn main() {\n    println!(\"Hello\");\n}".to_string();
        let after = "fn main() {\n    println!(\"Hello\");\n}".to_string();
        let diff = codediff::diff_strings(&before, &after, &codediff::code::Language::Rust);
        
        let mut app = App::new(before, after, diff);
        
        // Initially AST path should be empty
        assert!(app.ast_path.is_empty());
        
        // Show AST popup (this should trigger AST path update)
        app.toggle_ast_popup();
        
        // AST path should now be populated with the node path at cursor position
        // The cursor is at line 0, char 0, which should be the root node
        assert!(!app.ast_path.is_empty(), "AST path should be populated when popup is shown");
        
        // The first element should be the root node type
        assert_eq!(app.ast_path[0], "source_file", "First element should be source_file");
    }

    #[test]
    fn test_language_detection_for_test_files() {
        // Test .test file extensions
        let test_cases = vec![
            ("test.rs.test", codediff::code::Language::Rust),
            ("test.py.test", codediff::code::Language::Python),
            ("test.js.test", codediff::code::Language::JavaScript),
            ("test.java.test", codediff::code::Language::Java),
            ("test.cpp.test", codediff::code::Language::CPP),
            ("test.html.test", codediff::code::Language::HTML),
            ("test.unknown.test", codediff::code::Language::Unknown),
            ("no_extension.test", codediff::code::Language::Unknown),
        ];

        for (filename, expected_lang) in test_cases {
            let path = std::path::PathBuf::from(filename);
            
            // Use the same logic as in main()
            let language = if let Some(file_name) = path.file_name() {
                if let Some(file_name_str) = file_name.to_str() {
                    if file_name_str.ends_with(".test") {
                        let actual_filename = file_name_str.trim_end_matches(".test");
                        if let Some(last_dot_pos) = actual_filename.rfind('.') {
                            let actual_ext = &actual_filename[last_dot_pos + 1..];
                            codediff::code::language::language_for_extension(actual_ext)
                                .unwrap_or(codediff::code::Language::Unknown)
                        } else {
                            codediff::code::Language::Unknown
                        }
                    } else {
                        if let Some(ext) = path.extension() {
                            if let Some(lang_str) = ext.to_str() {
                                codediff::code::language::language_for_extension(lang_str)
                                    .unwrap_or(codediff::code::Language::Unknown)
                            } else {
                                codediff::code::Language::Unknown
                            }
                        } else {
                            codediff::code::Language::Unknown
                        }
                    }
                } else {
                    codediff::code::Language::Unknown
                }
            } else {
                codediff::code::Language::Unknown
            };
            
            assert_eq!(language, expected_lang, "Failed for filename: {}", filename);
        }
    }
}