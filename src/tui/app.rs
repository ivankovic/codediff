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
use tree_sitter::Node; // Import tree-sitter Node for AST operations

/// Theme for the application
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    pub fn get_colors(&self) -> ThemeColors {
        match self {
            Theme::Light => ThemeColors {
                text: ratatui::style::Color::Black,
                cursor_bg: ratatui::style::Color::Blue,
                cursor_fg: ratatui::style::Color::White,
                header_fg: ratatui::style::Color::Yellow,
                footer_fg: ratatui::style::Color::Black,
                popup_bg: ratatui::style::Color::Gray,
                popup_fg: ratatui::style::Color::Black,
                popup_border: ratatui::style::Color::Black,
                diff_added: ratatui::style::Color::Green,
                diff_removed: ratatui::style::Color::Red,
                diff_changed: ratatui::style::Color::Yellow,
            },
            Theme::Dark => ThemeColors {
                text: ratatui::style::Color::White,
                cursor_bg: ratatui::style::Color::Blue,
                cursor_fg: ratatui::style::Color::Black,
                header_fg: ratatui::style::Color::LightYellow,
                footer_fg: ratatui::style::Color::DarkGray,
                popup_bg: ratatui::style::Color::DarkGray,
                popup_fg: ratatui::style::Color::White,
                popup_border: ratatui::style::Color::White,
                diff_added: ratatui::style::Color::LightGreen,
                diff_removed: ratatui::style::Color::LightRed,
                diff_changed: ratatui::style::Color::LightYellow,
            },
        }
    }
}

/// Color scheme for the application
#[derive(Debug, Clone, Copy)]
pub struct ThemeColors {
    pub text: ratatui::style::Color,
    pub cursor_bg: ratatui::style::Color,
    pub cursor_fg: ratatui::style::Color,
    pub header_fg: ratatui::style::Color,
    pub footer_fg: ratatui::style::Color,
    pub popup_bg: ratatui::style::Color,
    pub popup_fg: ratatui::style::Color,
    pub popup_border: ratatui::style::Color,
    // Diff colors
    pub diff_added: ratatui::style::Color,
    pub diff_removed: ratatui::style::Color,
    pub diff_changed: ratatui::style::Color,
}

/// Application state
pub struct App {
    pub before_code: String,
    pub after_code: String,
    pub cursor_line: usize,
    pub cursor_char: usize,
    pub scroll_offset: usize,
    pub active_panel: Panel,
    pub show_ast_popup: bool,
    pub ast_path: Vec<String>,
    pub theme: Theme,
    pub colors: ThemeColors,
    pub show_help: bool,
    pub show_legend: bool,
    // Token-level diff information: (start_byte, end_byte, status)
    pub token_diff_ranges: Vec<(usize, usize, LineDiffStatus)>,
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum Panel {
    Before,
    After,
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum LineDiffStatus {
    Unchanged,
    Added,
    Removed,
    Changed,
}

impl App {
    pub fn new(before: String, after: String, _diff: codediff::diff::Diff) -> Self {
        let theme = Theme::Light;
        let colors = theme.get_colors();
        let token_diff_ranges = Self::compute_token_diff_ranges(&before, &after, &_diff);

        Self {
            before_code: before,
            after_code: after,
            cursor_line: 0,
            cursor_char: 0,
            scroll_offset: 0,
            active_panel: Panel::Before,
            show_ast_popup: false,
            ast_path: Vec::new(),
            theme,
            colors,
            show_help: false,
            show_legend: false,
            token_diff_ranges,
        }
    }

    /// Compute token-based diff ranges using AST diff information
    /// Returns vector of (start_byte, end_byte, status) tuples for character-level coloring
    pub fn compute_token_diff_ranges(
        before: &str,
        after: &str,
        diff: &codediff::diff::Diff,
    ) -> Vec<(usize, usize, LineDiffStatus)> {
        let mut ranges = Vec::new();

        // Debug: log if we have AST diff
        // eprintln!("compute_token_diff_ranges: has_ast_diff = {}", diff.ast.is_some());

        // Use AST diff if available
        if let Some(ast_diff) = &diff.ast {
            // eprintln!("AST diff has {} mappings", ast_diff.mapping.len());
            // Create temporary Code objects to access ASTs
            let mut before_code_obj = codediff::code::Code::from_string(before, &diff.language);
            let mut after_code_obj = codediff::code::Code::from_string(after, &diff.language);

            // Parse both codes
            let before_parsed = before_code_obj.ensure_parsed().is_ok();
            let after_parsed = after_code_obj.ensure_parsed().is_ok();

            if before_parsed && after_parsed
                && let (Some(before_ast), Some(after_ast)) =
                    (before_code_obj.ast.as_ref(), after_code_obj.ast.as_ref())
            {
                // Process mappings to create byte-level diff ranges
                for ((before_id, after_id), mapping) in &ast_diff.mapping {
                    let status = match mapping.operation {
                        codediff::diff::ASTMappingOperation::Identical => {
                            LineDiffStatus::Unchanged
                        }
                        codediff::diff::ASTMappingOperation::Insert => LineDiffStatus::Added,
                        codediff::diff::ASTMappingOperation::Delete => LineDiffStatus::Removed,
                        codediff::diff::ASTMappingOperation::Update => LineDiffStatus::Changed,
                        codediff::diff::ASTMappingOperation::Move => LineDiffStatus::Changed,
                        _ => LineDiffStatus::Unchanged,
                    };

                    // Debug output for mapping
                    // eprintln!("Mapping: before_id={}, after_id={}, operation={:?}", before_id, after_id, mapping.operation);

                    // Get node ranges for the before code (used when showing before panel)
                    if *before_id != 0
                        && let Some(node) = before_ast
                            .root_node()
                            .descendant_for_byte_range(*before_id, *before_id + 1)
                    {
                        // eprintln!("Before node found: {}..{}", node.start_byte(), node.end_byte());
                        ranges.push((node.start_byte(), node.end_byte(), status));
                    } else if *before_id != 0 {
                        // eprintln!("Before node NOT found for id {}", before_id);
                    }

                    // Get node ranges for the after code (used when showing after panel)
                    // Handle InsertWithChildren operations specially
                    if *after_id != 0 {
                        // For InsertWithChildren, we need to handle the inserted nodes
                        let status = if matches!(mapping.operation, codediff::diff::ASTMappingOperation::InsertWithChildren) {
                            LineDiffStatus::Added
                        } else {
                            status
                        };
                        
                        if let Some(node) = after_ast
                            .root_node()
                            .descendant_for_byte_range(*after_id, *after_id + 1)
                        {
                            // eprintln!("After node found: {}..{}", node.start_byte(), node.end_byte());
                            ranges.push((node.start_byte(), node.end_byte(), status));
                        } else {
                            // Try to find the node by searching for nodes that contain this ID
                            // eprintln!("After node NOT found for id {}, trying alternative search", after_id);
                            let mut cursor = after_ast.root_node().walk();
                            let mut found = false;
                            let mut stack = vec![after_ast.root_node()];
                            
                            while let Some(node) = stack.pop() {
                                if node.id() == *after_id {
                                    // eprintln!("After node found via search: {}..{}", node.start_byte(), node.end_byte());
                                    ranges.push((node.start_byte(), node.end_byte(), status));
                                    found = true;
                                    break;
                                }
                                
                                // Add children to stack
                                for child in node.children(&mut cursor) {
                                    stack.push(child);
                                }
                            }
                            
                            if !found {
                                // eprintln!("After node STILL NOT found for id {}", after_id);
                            }
                        }
                    }
                }
            }
        }

        ranges
    }

    pub fn toggle_panel(&mut self) {
        self.active_panel = match self.active_panel {
            Panel::Before => Panel::After,
            Panel::After => Panel::Before,
        };
    }

    pub fn move_cursor_up(&mut self) {
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

    pub fn move_cursor_down(&mut self) {
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
    pub fn ensure_cursor_visible(&mut self, visible_height: usize) {
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

    pub fn move_cursor_left(&mut self) {
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

    pub fn move_cursor_right(&mut self) {
        let line_content = match self.active_panel {
            Panel::Before => self.before_code.lines().nth(self.cursor_line),
            Panel::After => self.after_code.lines().nth(self.cursor_line),
        };

        if let Some(line) = line_content {
            let line_length = line.chars().count();
            if self.cursor_char < line_length {
                self.cursor_char += 1;
            } else if self.cursor_line
                < (match self.active_panel {
                    Panel::Before => self.before_code.lines().count(),
                    Panel::After => self.after_code.lines().count(),
                })
                .saturating_sub(1)
            {
                // Move to beginning of next line
                self.cursor_line += 1;
                self.cursor_char = 0;
            }
        }

        // Auto-scroll for horizontal movement too
        self.ensure_cursor_visible(20);
    }

    /// Update AST path based on current cursor position
    pub fn update_ast_path(&mut self) {
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
    pub fn cursor_position_to_byte(&self, code: &str) -> usize {
        let mut byte_pos = 0;
        for (line_idx, line) in code.lines().enumerate() {
            if line_idx == self.cursor_line {
                // Convert char position to byte position in this line
                for (byte_idx, _c) in line.char_indices() {
                    if byte_idx == self.cursor_char {
                        return byte_pos + byte_idx;
                    }
                }
                // If cursor is at end of line, return end of line
                return byte_pos + line.len();
            }
            byte_pos += line.len() + 1; // +1 for newline
        }
        byte_pos
    }

    /// Find the smallest node that contains the given byte position
    pub fn find_node_at_byte_position<'a>(
        &self,
        root_node: Node<'a>,
        byte_pos: usize,
    ) -> Option<Node<'a>> {
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
    pub fn build_node_path(&self, _root_node: &Node, target_node: &Node) -> Vec<String> {
        let mut path = Vec::new();
        let mut current = Some(*target_node);

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

    pub fn toggle_ast_popup(&mut self) {
        self.show_ast_popup = !self.show_ast_popup;
        // Update AST path when popup is shown
        if self.show_ast_popup {
            self.update_ast_path();
        }
    }

    pub fn toggle_theme(&mut self) {
        self.theme = match self.theme {
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::Light,
        };
        self.colors = self.theme.get_colors();
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    pub fn toggle_legend(&mut self) {
        self.show_legend = !self.show_legend;
    }
}