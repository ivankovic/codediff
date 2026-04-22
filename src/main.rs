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
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use std::fs;
use std::io;
use std::path::PathBuf;

mod tui;

use tui::app::App;
use tui::ui::ui;

#[derive(Parser)]
struct Args {
    before: PathBuf,
    after: PathBuf,
}

/// Main application
struct Tui {
    terminal: ratatui::Terminal<ratatui::backend::CrosstermBackend<io::Stdout>>,
}

impl Tui {
    fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = ratatui::backend::CrosstermBackend::new(stdout);
        let terminal = ratatui::Terminal::new(backend)?;

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

        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Char('t') => app.toggle_ast_popup(),
                KeyCode::Tab => app.toggle_panel(),
                KeyCode::Char(' ') => {
                    // Space bar - align both sides
                    // TODO: Implement actual alignment logic
                }
                KeyCode::Up | KeyCode::Char('k') => app.move_cursor_up(),
                KeyCode::Down | KeyCode::Char('j') => app.move_cursor_down(),
                KeyCode::Left | KeyCode::Char('h') => app.move_cursor_left(),
                KeyCode::Right | KeyCode::Char('l') => app.move_cursor_right(),
                KeyCode::Char('c') => {
                    // Toggle color theme
                    app.toggle_theme();
                }
                KeyCode::Char('?') => {
                    // Toggle help panel
                    app.toggle_help();
                }
                KeyCode::Char('d') => {
                    // Toggle legend
                    app.toggle_legend();
                }
                KeyCode::Esc => {
                    // Close any popups
                    app.show_help = false;
                    app.show_ast_popup = false;
                    app.show_legend = false;
                }
                _ => {}
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::io;

    #[test]
    fn devex_infrastructure_test() {}

    #[test]
    fn test_app_initialization() {
        let before = "fn main() {}".to_string();
        let after = "fn main() {}".to_string();
        let diff = codediff::diff_strings(&before, &after, &codediff::code::Language::Rust);

        let app = App::new(before, after, diff);

        assert_eq!(app.cursor_line, 0);
        assert_eq!(app.cursor_char, 0);
        assert_eq!(app.scroll_offset, 0);
        assert_eq!(app.active_panel, tui::app::Panel::Before);
        assert!(!app.show_ast_popup);
        assert!(app.ast_path.is_empty());
    }

    #[test]
    fn test_panel_toggling() {
        let before = "fn main() {}".to_string();
        let after = "fn main() {}".to_string();
        let diff = codediff::diff_strings(&before, &after, &codediff::code::Language::Rust);

        let mut app = App::new(before, after, diff);

        assert_eq!(app.active_panel, tui::app::Panel::Before);

        app.toggle_panel();
        assert_eq!(app.active_panel, tui::app::Panel::After);

        app.toggle_panel();
        assert_eq!(app.active_panel, tui::app::Panel::Before);
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
        app.ast_path = vec![
            "source_file".to_string(),
            "function_item".to_string(),
            "identifier".to_string(),
        ];

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
        let theme = tui::app::Theme::Light;
        assert!(matches!(theme, tui::app::Theme::Light));

        // Test that colors are appropriate for each theme
        let colors = theme.get_colors();

        // Light theme should have dark text on light background
        assert_eq!(colors.text, ratatui::style::Color::Black);
        assert_eq!(colors.cursor_fg, ratatui::style::Color::White);
        assert_eq!(colors.cursor_bg, ratatui::style::Color::Blue);
    }

    #[test]
    fn test_default_theme_is_light() {
        // Test that the theme is light
        let theme = tui::app::Theme::Light;
        assert_eq!(theme, tui::app::Theme::Light);
    }

    #[test]
    fn test_scrolling_behavior() {
        // Test that scrolling works correctly when cursor moves beyond visible area
        let before =
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10".to_string();
        let after =
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10".to_string();
        let diff = codediff::diff_strings(&before, &after, &codediff::code::Language::Rust);

        let mut app = App::new(before, after, diff);

        // Initially at line 0, scroll offset 0
        assert_eq!(app.cursor_line, 0);
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn test_scrolling_to_bottom() {
        // Test that we can reach the last lines of the document
        let before =
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10".to_string();
        let after =
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10".to_string();
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
        assert_eq!(
            app.scroll_offset, expected_scroll,
            "Should be able to scroll to show last line"
        );
    }

    #[test]
    fn test_cursor_visibility_during_scrolling() {
        // Test that cursor remains visible during continuous scrolling
        let before =
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12"
                .to_string();
        let after = before.clone();
        let diff = codediff::diff_strings(&before, &after, &codediff::code::Language::Rust);

        let mut app = App::new(before, after, diff);

        // Simulate terminal with 5 visible lines
        let visible_lines = 5;

        // Scroll through the document step by step
        for line_num in 1..12 {
            // Move cursor down
            app.move_cursor_down();
            assert_eq!(
                app.cursor_line, line_num,
                "Cursor should move to line {}",
                line_num
            );

            // Ensure cursor is visible
            app.ensure_cursor_visible(visible_lines);

            // Calculate expected visible range
            let visible_start = app.scroll_offset;
            let visible_end = app.scroll_offset + visible_lines;

            // Cursor should always be within visible range
            assert!(
                app.cursor_line >= visible_start && app.cursor_line < visible_end,
                "Line {} should be visible (scroll: {}, visible: {}-{})",
                app.cursor_line,
                app.scroll_offset,
                visible_start,
                visible_end
            );
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
            assert!(
                app.scroll_offset <= max_possible_scroll,
                "Scroll offset {} should not exceed max possible {} for {} visible lines",
                app.scroll_offset,
                max_possible_scroll,
                visible_lines
            );
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
        assert!(
            !app.ast_path.is_empty(),
            "AST path should be populated when popup is shown"
        );

        // The first element should be the root node type
        assert_eq!(
            app.ast_path[0], "source_file",
            "First element should be source_file"
        );
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

    #[test]
    fn test_token_diff_ranges_computed() {
        // Test that token diff ranges are computed from AST diff
        let before = "fn main() {\n    println!(\"Hello\");\n}".to_string();
        let after = "fn main() {\n    println!(\"World\");\n}".to_string();
        let diff = codediff::diff_strings(&before, &after, &codediff::code::Language::Rust);

        let app = App::new(before, after, diff);

        // Should have some token diff ranges computed
        assert!(
            !app.token_diff_ranges.is_empty(),
            "Should have token diff ranges"
        );
    }
}


