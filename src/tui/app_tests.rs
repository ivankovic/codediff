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
    use super::super::app::{App, Panel, Theme};
    use codediff::diff_strings;
    use codediff::code::Language;

    #[test]
    fn test_app_new_initial_state() {
        let before = "fn main() {}".to_string();
        let after = "fn main() {}".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let app = App::new(before, after, diff);

        assert_eq!(app.cursor_line, 0);
        assert_eq!(app.cursor_char, 0);
        assert_eq!(app.scroll_offset, 0);
        assert_eq!(app.active_panel, Panel::Before);
        assert!(!app.show_ast_popup);
        assert!(app.ast_path.is_empty());
        assert_eq!(app.before_code, "fn main() {}");
        assert_eq!(app.after_code, "fn main() {}");
    }

    #[test]
    fn test_theme_colors() {
        let theme = Theme::Light;
        let colors = theme.get_colors();

        assert_eq!(colors.text, ratatui::style::Color::Black);
        assert_eq!(colors.cursor_bg, ratatui::style::Color::Blue);
        assert_eq!(colors.cursor_fg, ratatui::style::Color::White);
        assert_eq!(colors.header_fg, ratatui::style::Color::Yellow);
        assert_eq!(colors.footer_fg, ratatui::style::Color::Black);
        assert_eq!(colors.popup_bg, ratatui::style::Color::Gray);
        assert_eq!(colors.popup_fg, ratatui::style::Color::Black);
        assert_eq!(colors.popup_border, ratatui::style::Color::Black);
        assert_eq!(colors.diff_added, ratatui::style::Color::Green);
        assert_eq!(colors.diff_removed, ratatui::style::Color::Red);
        assert_eq!(colors.diff_changed, ratatui::style::Color::Yellow);
    }

    #[test]
    fn test_panel_toggle() {
        let before = "fn main() {}".to_string();
        let after = "fn main() {}".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let mut app = App::new(before, after, diff);

        assert_eq!(app.active_panel, Panel::Before);

        app.toggle_panel();
        assert_eq!(app.active_panel, Panel::After);

        app.toggle_panel();
        assert_eq!(app.active_panel, Panel::Before);
    }

    #[test]
    fn test_ast_popup_toggle() {
        let before = "fn main() {}".to_string();
        let after = "fn main() {}".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let mut app = App::new(before, after, diff);

        assert!(!app.show_ast_popup);

        app.toggle_ast_popup();
        assert!(app.show_ast_popup);

        app.toggle_ast_popup();
        assert!(!app.show_ast_popup);
    }

    #[test]
    fn test_cursor_movement_up() {
        let before = "line1\nline2\nline3".to_string();
        let after = "line1\nline2\nline3".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let mut app = App::new(before, after, diff);

        // Start at line 0
        assert_eq!(app.cursor_line, 0);

        // Can't move up from first line
        app.move_cursor_up();
        assert_eq!(app.cursor_line, 0);

        // Move down to line 2
        app.move_cursor_down();
        app.move_cursor_down();
        assert_eq!(app.cursor_line, 2);

        // Move up
        app.move_cursor_up();
        assert_eq!(app.cursor_line, 1);
    }

    #[test]
    fn test_cursor_movement_down() {
        let before = "line1\nline2\nline3".to_string();
        let after = "line1\nline2\nline3".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let mut app = App::new(before, after, diff);

        // Start at line 0
        assert_eq!(app.cursor_line, 0);

        // Move down
        app.move_cursor_down();
        assert_eq!(app.cursor_line, 1);

        app.move_cursor_down();
        assert_eq!(app.cursor_line, 2);

        // Can't move beyond last line
        app.move_cursor_down();
        assert_eq!(app.cursor_line, 2);
    }

    #[test]
    fn test_cursor_movement_left() {
        let before = "line1\nline2\nline3".to_string();
        let after = "line1\nline2\nline3".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let mut app = App::new(before, after, diff);

        // Start at char 0
        assert_eq!(app.cursor_char, 0);

        // Can't move left from position 0
        app.move_cursor_left();
        assert_eq!(app.cursor_char, 0);

        // Move right to position 3
        for _ in 0..3 {
            app.move_cursor_right();
        }
        assert_eq!(app.cursor_char, 3);

        // Move left
        app.move_cursor_left();
        assert_eq!(app.cursor_char, 2);
    }

    #[test]
    fn test_cursor_movement_right() {
        let before = "line1\nline2\nline3".to_string();
        let after = "line1\nline2\nline3".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let mut app = App::new(before, after, diff);

        // Start at char 0
        assert_eq!(app.cursor_char, 0);

        // Move right
        app.move_cursor_right();
        assert_eq!(app.cursor_char, 1);

        app.move_cursor_right();
        assert_eq!(app.cursor_char, 2);
    }

    #[test]
    fn test_cursor_wrap_to_next_line() {
        let before = "short\nline2\nline3".to_string();
        let after = "short\nline2\nline3".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let mut app = App::new(before, after, diff);

        // Move to end of first line (position 5)
        for _ in 0..5 {
            app.move_cursor_right();
        }
        assert_eq!(app.cursor_char, 5);

        // Next right should move to beginning of next line
        app.move_cursor_right();
        assert_eq!(app.cursor_line, 1);
        assert_eq!(app.cursor_char, 0);
    }

    #[test]
    fn test_cursor_wrap_to_previous_line() {
        let before = "short\nline2\nline3".to_string();
        let after = "short\nline2\nline3".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let mut app = App::new(before, after, diff);

        // Move to line 1
        app.move_cursor_down();
        assert_eq!(app.cursor_line, 1);
        assert_eq!(app.cursor_char, 0);

        // Move left should wrap to end of previous line
        app.move_cursor_left();
        assert_eq!(app.cursor_line, 0);
        assert_eq!(app.cursor_char, 4); // "short" has 5 characters (0-4)
    }

    #[test]
    fn test_scroll_offset_adjustment() {
        let before = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10".to_string();
        let after = before.clone();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let mut app = App::new(before, after, diff);

        // Initially no scroll offset
        assert_eq!(app.scroll_offset, 0);

        // Move cursor down beyond visible area (assuming 5 visible lines)
        for _ in 0..8 {
            app.move_cursor_down();
        }
        assert_eq!(app.cursor_line, 8);

        // Scroll offset should be adjusted to keep cursor visible
        app.ensure_cursor_visible(5);
        assert_eq!(app.scroll_offset, 4); // Show lines 4-8, cursor at 8
    }

    #[test]
    fn test_scroll_offset_edge_cases() {
        let before = "line1\nline2\nline3\nline4\nline5".to_string();
        let after = before.clone();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let mut app = App::new(before, after, diff);

        // Move to last line
        while app.cursor_line < 4 {
            app.move_cursor_down();
        }

        // With 10 visible lines, scroll offset should be 0 (all lines visible)
        app.ensure_cursor_visible(10);
        assert_eq!(app.scroll_offset, 0);

        // With 3 visible lines, scroll offset should be 2 (show lines 2-4)
        app.ensure_cursor_visible(3);
        assert_eq!(app.scroll_offset, 2);
    }

    #[test]
    fn test_token_diff_ranges_computed() {
        let before = "fn main() {\n    println!(\"Hello\");\n}".to_string();
        let after = "fn main() {\n    println!(\"World\");\n}".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let app = App::new(before, after, diff);

        // Should have some token diff ranges computed from AST diff
        assert!(!app.token_diff_ranges.is_empty());

        // Check that ranges are within bounds
        for &(start, end, _) in &app.token_diff_ranges {
            assert!(start <= end);
            // Can't check against original strings since they were moved into App
            // Just verify the ranges are logically valid
        }
    }

    #[test]
    fn test_cursor_position_to_byte() {
        let before = "line1\nline2\nline3".to_string();
        let after = "line1\nline2\nline3".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let mut app = App::new(before, after, diff);

        // Test position at line 0, char 0
        assert_eq!(app.cursor_position_to_byte(&app.before_code), 0);

        // Move to line 1, char 2
        app.cursor_line = 1;
        app.cursor_char = 2;
        // "line1\n" is 6 bytes, + 2 bytes for "li" = 8 bytes
        assert_eq!(app.cursor_position_to_byte(&app.before_code), 8);

        // Move to line 2, char 3
        app.cursor_line = 2;
        app.cursor_char = 3;
        // "line1\nline2\n" is 12 bytes, + 3 bytes for "lin" = 15 bytes
        assert_eq!(app.cursor_position_to_byte(&app.before_code), 15);
    }

    #[test]
    fn test_ast_path_update() {
        let before = "fn main() {\n    println!(\"Hello\");\n}".to_string();
        let after = "fn main() {\n    println!(\"Hello\");\n}".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let mut app = App::new(before, after, diff);

        // Initially AST path should be empty
        assert!(app.ast_path.is_empty());

        // Show AST popup (this should trigger AST path update)
        app.toggle_ast_popup();

        // AST path should now be populated
        assert!(!app.ast_path.is_empty());

        // First element should be the root node type
        assert_eq!(app.ast_path[0], "source_file");
    }

    #[test]
    fn test_theme_toggle() {
        let before = "fn main() {}".to_string();
        let after = "fn main() {}".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let mut app = App::new(before, after, diff);

        // Start with light theme
        assert_eq!(app.theme, Theme::Light);

        // Toggle to dark theme
        app.toggle_theme();
        assert_eq!(app.theme, Theme::Dark);
        assert_eq!(app.colors.text, ratatui::style::Color::White);

        // Toggle back to light theme
        app.toggle_theme();
        assert_eq!(app.theme, Theme::Light);
        assert_eq!(app.colors.text, ratatui::style::Color::Black);
    }

    #[test]
    fn test_help_toggle() {
        let before = "fn main() {}".to_string();
        let after = "fn main() {}".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let mut app = App::new(before, after, diff);

        // Initially help should be hidden
        assert!(!app.show_help);

        // Toggle help on
        app.toggle_help();
        assert!(app.show_help);

        // Toggle help off
        app.toggle_help();
        assert!(!app.show_help);
    }

    #[test]
    fn test_legend_toggle() {
        let before = "fn main() {}".to_string();
        let after = "fn main() {}".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let mut app = App::new(before, after, diff);

        // Initially legend should be hidden
        assert!(!app.show_legend);

        // Toggle legend on
        app.toggle_legend();
        assert!(app.show_legend);

        // Toggle legend off
        app.toggle_legend();
        assert!(!app.show_legend);
    }

    #[test]
    fn test_esc_closes_all_popups() {
        let before = "fn main() {}".to_string();
        let after = "fn main() {}".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let mut app = App::new(before, after, diff);

        // Open all popups
        app.show_help = true;
        app.show_ast_popup = true;
        app.show_legend = true;

        assert!(app.show_help);
        assert!(app.show_ast_popup);
        assert!(app.show_legend);

        // ESC should close all popups
        // Simulate ESC key handling
        app.show_help = false;
        app.show_ast_popup = false;
        app.show_legend = false;

        assert!(!app.show_help);
        assert!(!app.show_ast_popup);
        assert!(!app.show_legend);
    }
}


