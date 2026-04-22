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
    use super::super::app::{App, Panel};
    use super::super::ui::ui;
    use codediff::diff_strings;
    use codediff::code::Language;
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn test_narrow_mode_rendering() -> Result<(), Box<dyn std::error::Error>> {
        let before = "fn main() {\n    println!(\"Hello\");\n}".to_string();
        let after = "fn main() {\n    println!(\"World\");\n}".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let app = App::new(before, after, diff);

        // Create a narrow terminal (less than 220 chars wide)
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend)?;

        // This should not panic and should render in narrow mode
        terminal.draw(|f| ui(f, &app))?;

        Ok(())
    }

    #[test]
    fn test_wide_mode_rendering() -> Result<(), Box<dyn std::error::Error>> {
        let before = "fn main() {\n    println!(\"Hello\");\n}".to_string();
        let after = "fn main() {\n    println!(\"World\");\n}".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let app = App::new(before, after, diff);

        // Create a wide terminal (220+ chars wide)
        let backend = TestBackend::new(250, 24);
        let mut terminal = Terminal::new(backend)?;

        // This should not panic and should render in wide mode
        terminal.draw(|f| ui(f, &app))?;

        Ok(())
    }

    #[test]
    fn test_cursor_position_highlighting() -> Result<(), Box<dyn std::error::Error>> {
        let before = "line1\nline2\nline3".to_string();
        let after = "line1\nline2\nline3".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

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
    fn test_ast_popup_rendering() -> Result<(), Box<dyn std::error::Error>> {
        let before = "fn main() {}".to_string();
        let after = "fn main() {}".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

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
    fn test_panel_switching_rendering() -> Result<(), Box<dyn std::error::Error>> {
        let before = "fn main() {\n    println!(\"Hello\");\n}".to_string();
        let after = "fn main() {\n    println!(\"World\");\n}".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

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
    fn test_before_panel_active() -> Result<(), Box<dyn std::error::Error>> {
        let before = "fn main() {\n    println!(\"Hello\");\n}".to_string();
        let after = "fn main() {\n    println!(\"World\");\n}".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let mut app = App::new(before, after, diff);
        app.active_panel = Panel::Before;

        let backend = TestBackend::new(250, 24);
        let mut terminal = Terminal::new(backend)?;

        // Should render with before panel highlighted
        terminal.draw(|f| ui(f, &app))?;

        Ok(())
    }

    #[test]
    fn test_after_panel_active() -> Result<(), Box<dyn std::error::Error>> {
        let before = "fn main() {\n    println!(\"Hello\");\n}".to_string();
        let after = "fn main() {\n    println!(\"World\");\n}".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let mut app = App::new(before, after, diff);
        app.active_panel = Panel::After;

        let backend = TestBackend::new(250, 24);
        let mut terminal = Terminal::new(backend)?;

        // Should render with after panel highlighted
        terminal.draw(|f| ui(f, &app))?;

        Ok(())
    }

    #[test]
    fn test_token_diff_highlighting() -> Result<(), Box<dyn std::error::Error>> {
        let before = "fn main() {\n    println!(\"Hello\");\n}".to_string();
        let after = "fn main() {\n    println!(\"World\");\n}".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let app = App::new(before, after, diff);

        // Should have token diff ranges
        assert!(!app.token_diff_ranges.is_empty());

        let backend = TestBackend::new(250, 24);
        let mut terminal = Terminal::new(backend)?;

        // Should render with token-based diff highlighting
        terminal.draw(|f| ui(f, &app))?;

        Ok(())
    }

    #[test]
    fn test_empty_code_rendering() -> Result<(), Box<dyn std::error::Error>> {
        let before = "".to_string();
        let after = "".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let app = App::new(before, after, diff);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend)?;

        // Should handle empty code without panicking
        terminal.draw(|f| ui(f, &app))?;

        Ok(())
    }

    #[test]
    fn test_single_line_code_rendering() -> Result<(), Box<dyn std::error::Error>> {
        let before = "fn main() {}".to_string();
        let after = "fn main() {}".to_string();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let app = App::new(before, after, diff);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend)?;

        // Should handle single line code
        terminal.draw(|f| ui(f, &app))?;

        Ok(())
    }

    #[test]
    fn test_long_code_scrolling() -> Result<(), Box<dyn std::error::Error>> {
        let before = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12".to_string();
        let after = before.clone();
        let diff = diff_strings(&before, &after, &Language::Rust);

        let mut app = App::new(before, after, diff);
        app.scroll_offset = 5; // Scroll down

        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend)?;

        // Should handle scrolling
        terminal.draw(|f| ui(f, &app))?;

        Ok(())
    }
}