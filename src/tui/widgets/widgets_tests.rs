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

#[cfg(test)]
mod tests {
    use super::super::code_panel::CodePanel;
    use super::super::ast_popup::AstPopup;
    use super::super::super::app::{ThemeColors, LineDiffStatus};

    #[test]
    fn test_code_panel_creation() {
        let colors = ThemeColors {
            text: ratatui::style::Color::White,
            cursor_bg: ratatui::style::Color::Blue,
            cursor_fg: ratatui::style::Color::Black,
            header_fg: ratatui::style::Color::Yellow,
            footer_fg: ratatui::style::Color::Gray,
            popup_bg: ratatui::style::Color::DarkGray,
            popup_fg: ratatui::style::Color::White,
            popup_border: ratatui::style::Color::White,
            diff_added: ratatui::style::Color::Green,
            diff_removed: ratatui::style::Color::Red,
            diff_changed: ratatui::style::Color::Yellow,
        };

        let code = "fn main() {\n    println!(\"Hello\");\n}".to_string();
        let token_diff_ranges = vec![
            (10, 15, LineDiffStatus::Changed),
            (20, 25, LineDiffStatus::Added),
        ];

        let panel = CodePanel::new(
            "Test Panel".to_string(),
            code,
            0,
            0,
            0,
            0,
            true,
            colors,
            token_diff_ranges,
        );

        assert_eq!(panel.title, "Test Panel");
        assert_eq!(panel.code, "fn main() {\n    println!(\"Hello\");\n}");
        assert_eq!(panel.cursor_line, 0);
        assert_eq!(panel.cursor_char, 0);
        assert_eq!(panel.scroll_offset, 0);
        assert!(panel.is_active);
        assert_eq!(panel.token_diff_ranges.len(), 2);
    }

    #[test]
    fn test_code_panel_widget_creation() {
        let colors = ThemeColors {
            text: ratatui::style::Color::White,
            cursor_bg: ratatui::style::Color::Blue,
            cursor_fg: ratatui::style::Color::Black,
            header_fg: ratatui::style::Color::Yellow,
            footer_fg: ratatui::style::Color::Gray,
            popup_bg: ratatui::style::Color::DarkGray,
            popup_fg: ratatui::style::Color::White,
            popup_border: ratatui::style::Color::White,
            diff_added: ratatui::style::Color::Green,
            diff_removed: ratatui::style::Color::Red,
            diff_changed: ratatui::style::Color::Yellow,
        };

        let code = "line1\nline2\nline3".to_string();
        let token_diff_ranges = vec![
            (6, 11, LineDiffStatus::Changed), // "line2"
        ];

        let panel = CodePanel::new(
            "Code".to_string(),
            code,
            0,
            1,  // cursor at line 1
            2,  // cursor at char 2
            0,
            true,
            colors,
            token_diff_ranges,
        );

        // Should be able to create widget without panicking
        let _widget = panel.to_widget(10);
        // Widget created successfully - test passes if no panic
    }

    #[test]
    fn test_ast_popup_creation() {
        let colors = ThemeColors {
            text: ratatui::style::Color::White,
            cursor_bg: ratatui::style::Color::Blue,
            cursor_fg: ratatui::style::Color::Black,
            header_fg: ratatui::style::Color::Yellow,
            footer_fg: ratatui::style::Color::Gray,
            popup_bg: ratatui::style::Color::DarkGray,
            popup_fg: ratatui::style::Color::White,
            popup_border: ratatui::style::Color::White,
            diff_added: ratatui::style::Color::Green,
            diff_removed: ratatui::style::Color::Red,
            diff_changed: ratatui::style::Color::Yellow,
        };

        let ast_path = vec![
            "source_file".to_string(),
            "function_item".to_string(),
            "identifier".to_string(),
        ];

        let popup = AstPopup::new(ast_path, 0, 0, colors);

        assert_eq!(popup.ast_path.len(), 3);
        assert_eq!(popup.cursor_line, 0);
        assert_eq!(popup.cursor_char, 0);
    }

    #[test]
    fn test_ast_popup_widget_creation() {
        let colors = ThemeColors {
            text: ratatui::style::Color::White,
            cursor_bg: ratatui::style::Color::Blue,
            cursor_fg: ratatui::style::Color::Black,
            header_fg: ratatui::style::Color::Yellow,
            footer_fg: ratatui::style::Color::Gray,
            popup_bg: ratatui::style::Color::DarkGray,
            popup_fg: ratatui::style::Color::White,
            popup_border: ratatui::style::Color::White,
            diff_added: ratatui::style::Color::Green,
            diff_removed: ratatui::style::Color::Red,
            diff_changed: ratatui::style::Color::Yellow,
        };

        let ast_path = vec![
            "source_file".to_string(),
            "function_item".to_string(),
            "identifier".to_string(),
        ];

        let popup = AstPopup::new(ast_path, 2, 3, colors);

        // Should be able to create widget without panicking
        let _widget = popup.to_widget();
        // Widget created successfully - test passes if no panic
    }

    #[test]
    fn test_ast_popup_empty_path() {
        let colors = ThemeColors {
            text: ratatui::style::Color::White,
            cursor_bg: ratatui::style::Color::Blue,
            cursor_fg: ratatui::style::Color::Black,
            header_fg: ratatui::style::Color::Yellow,
            footer_fg: ratatui::style::Color::Gray,
            popup_bg: ratatui::style::Color::DarkGray,
            popup_fg: ratatui::style::Color::White,
            popup_border: ratatui::style::Color::White,
            diff_added: ratatui::style::Color::Green,
            diff_removed: ratatui::style::Color::Red,
            diff_changed: ratatui::style::Color::Yellow,
        };

        let empty_ast_path = vec![];
        let popup = AstPopup::new(empty_ast_path, 1, 2, colors);

        // Should handle empty AST path
        let _widget = popup.to_widget();
        // Widget created successfully - test passes if no panic
    }

    #[test]
    fn test_code_panel_with_diff_ranges() {
        let colors = ThemeColors {
            text: ratatui::style::Color::White,
            cursor_bg: ratatui::style::Color::Blue,
            cursor_fg: ratatui::style::Color::Black,
            header_fg: ratatui::style::Color::Yellow,
            footer_fg: ratatui::style::Color::Gray,
            popup_bg: ratatui::style::Color::DarkGray,
            popup_fg: ratatui::style::Color::White,
            popup_border: ratatui::style::Color::White,
            diff_added: ratatui::style::Color::Green,
            diff_removed: ratatui::style::Color::Red,
            diff_changed: ratatui::style::Color::Yellow,
        };

        let code = "fn main() {\n    println!(\"Hello\");\n}".to_string();
        let diff_ranges = vec![
            (12, 17, LineDiffStatus::Added),    // "Hello"
            (20, 25, LineDiffStatus::Removed),  // hypothetical removed text
        ];

        let panel = CodePanel::new(
            "Rust Code".to_string(),
            code,
            0,
            0,
            0,
            0,
            false,
            colors,
            diff_ranges,
        );

        // Should handle diff ranges without panicking
        let _widget = panel.to_widget(5);
        // Widget created successfully - test passes if no panic
    }

    #[test]
    fn test_code_panel_scrolling() {
        let colors = ThemeColors {
            text: ratatui::style::Color::White,
            cursor_bg: ratatui::style::Color::Blue,
            cursor_fg: ratatui::style::Color::Black,
            header_fg: ratatui::style::Color::Yellow,
            footer_fg: ratatui::style::Color::Gray,
            popup_bg: ratatui::style::Color::DarkGray,
            popup_fg: ratatui::style::Color::White,
            popup_border: ratatui::style::Color::White,
            diff_added: ratatui::style::Color::Green,
            diff_removed: ratatui::style::Color::Red,
            diff_changed: ratatui::style::Color::Yellow,
        };

        let long_code = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10".to_string();

        let panel = CodePanel::new(
            "Long Code".to_string(),
            long_code,
            0,
            0,
            0,
            5,  // scrolled down by 5 lines
            false,
            colors,
            vec![],
        );

        // Should handle scrolling without panicking
        let _widget = panel.to_widget(3); // only show 3 lines
        // Widget created successfully - test passes if no panic
    }
}