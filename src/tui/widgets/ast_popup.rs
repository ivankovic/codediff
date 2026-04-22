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
use ratatui::{
    style::{Style},
    widgets::{Block, Borders, Paragraph},
    prelude::Alignment,
};

use crate::tui::app::ThemeColors;

/// AST Popup widget
pub struct AstPopup {
    pub ast_path: Vec<String>,
    pub cursor_line: usize,
    pub cursor_char: usize,
    pub colors: ThemeColors,
}

impl AstPopup {
    pub fn new(ast_path: Vec<String>, cursor_line: usize, cursor_char: usize, colors: ThemeColors) -> Self {
        Self {
            ast_path,
            cursor_line,
            cursor_char,
            colors,
        }
    }

    pub fn to_widget(&self) -> Paragraph<'static> {
        // Build AST tree content
        let ast_content = if self.ast_path.is_empty() {
            format!("Move cursor to see AST path\nCurrent position: Line {}, Char {}", self.cursor_line, self.cursor_char)
        } else {
            // Show proper tree structure
            let mut tree_text = String::new();
            for (i, node) in self.ast_path.iter().enumerate() {
                // Add indentation based on depth
                let indent = "  ".repeat(i);
                if i == self.ast_path.len() - 1 {
                    tree_text.push_str(&format!("{}└─ {}\n", indent, node));
                } else {
                    tree_text.push_str(&format!("{}├─ {}\n", indent, node));
                }
            }
            tree_text
        };

        Paragraph::new(ast_content)
            .block(
                Block::default()
                    .title("AST Tree")
                    .borders(Borders::ALL)
                    .style(Style::default().fg(self.colors.popup_border)),
            )
            .style(
                Style::default()
                    .fg(self.colors.popup_fg)
                    .bg(self.colors.popup_bg),
            )
            .alignment(Alignment::Left)
    }
}