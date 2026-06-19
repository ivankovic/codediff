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
use std::sync::OnceLock;

use anyhow::{Context, Result};
use ratatui::{
    buffer::Buffer,
    prelude::*,
    symbols::border,
    text::Line,
    widgets::{Block, Borders, StatefulWidget, Widget},
};
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

use crate::code::language::language_for_path;

/// Static syntax set loaded once
static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();

/// Static theme set loaded once
static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

/// Get or initialize the syntax set
fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// Get or initialize the theme set
fn theme_set() -> &'static ThemeSet {
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

/// Map our internal Language enum to syntect syntax name
fn language_to_syntect(lang: &crate::code::Language) -> Option<&'static str> {
    use crate::code::Language::*;
    
    match lang {
        Bazel => Some("Bazel"),
        C => Some("C"),
        CPP => Some("C++"),
        CSS => Some("CSS"),
        CSharp => Some("C#"),
        Dart => Some("Dart"),
        Go => Some("Go"),
        HTML => Some("HTML"),
        JSON => Some("JSON"),
        Java => Some("Java"),
        JavaScript => Some("JavaScript"),
        Kotlin => Some("Kotlin"),
        LUA => Some("Lua"),
        Lisp => Some("Lisp"),
        MarkDown => Some("Markdown"),
        PHP => Some("PHP"),
        ProtoBuf => Some("Protocol Buffers"),
        Python => Some("Python"),
        R => Some("R"),
        Ruby => Some("Ruby"),
        Rust => Some("Rust"),
        SQL => Some("SQL"),
        Scala => Some("Scala"),
        ShellScript => Some("Bash"),
        Swift => Some("Swift"),
        TSX => Some("TSX"),
        TypeScript => Some("TypeScript"),
        Vimscript => Some("VimL"),
        YAML => Some("YAML"),
        XML => Some("XML"),
        Unknown => None,
    }
}

/// Convert syntect Color to ratatui Color
/// In syntect 5.x, Color is a struct with r, g, b, a fields
fn syntect_color_to_ratatui(color: syntect::highlighting::Color) -> ratatui::style::Color {
    // Syntect 5.x uses RGBA color struct
    // We'll convert to ratatui's color enum
    // For simplicity, we just use the RGB values directly
    ratatui::style::Color::Rgb(color.r, color.g, color.b)
}

/// The state for the CodeViewer widget (scroll position and viewport)
#[derive(Default, Clone)]
pub struct CodeViewerState {
    /// Scroll position (line number)
    pub scroll: usize,
    /// Viewport height in lines
    pub viewport_height: usize,
}

/// A widget that displays source code
///
/// This is a stateful widget that displays file contents with syntax highlighting.
/// It can be reused in multiple components.
#[derive(Default, Clone)]
pub struct CodeViewerWidget {
    /// The path to the file being displayed
    file_path: Option<PathBuf>,
    /// The file contents
    contents: String,
    /// The language of the file
    language: Option<crate::code::Language>,
    /// The title to display (overrides filename if set)
    title: Option<String>,
    /// The theme name to use for syntax highlighting
    theme_name: Option<String>,
    /// Whether syntax highlighting is enabled
    syntax_highlighting: bool,
}

impl CodeViewerWidget {
    /// Create a new CodeViewerWidget
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a CodeViewerWidget for a specific file
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

        Ok(())
    }

    /// Set the contents directly (for testing or custom content)
    pub fn with_contents(mut self, contents: String) -> Self {
        self.contents = contents;
        self
    }

    /// Set the file path
    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.file_path = Some(path.clone());
        self.language = language_for_path(&path);
        self
    }

    /// Set a custom title (overrides filename)
    pub fn with_title(mut self, title: String) -> Self {
        self.title = Some(title);
        self
    }

    /// Set the theme for syntax highlighting
    pub fn with_theme(mut self, theme_name: String) -> Self {
        self.theme_name = Some(theme_name);
        self
    }

    /// Enable or disable syntax highlighting
    pub fn with_syntax_highlighting(mut self, enabled: bool) -> Self {
        self.syntax_highlighting = enabled;
        self
    }

    /// Enable syntax highlighting
    pub fn enable_syntax_highlighting(&mut self) {
        self.syntax_highlighting = true;
    }

    /// Disable syntax highlighting
    pub fn disable_syntax_highlighting(&mut self) {
        self.syntax_highlighting = false;
    }

    /// Check if syntax highlighting is enabled
    pub fn is_syntax_highlighting_enabled(&self) -> bool {
        self.syntax_highlighting
    }

    /// Set the theme for syntax highlighting
    pub fn set_theme(&mut self, theme_name: String) {
        self.theme_name = Some(theme_name);
    }

    /// Get the total number of lines
    pub fn line_count(&self) -> usize {
        self.contents.lines().count()
    }

    /// Get the syntax for highlighting based on language
    fn get_syntax(&self) -> Option<&'static SyntaxReference> {
        let lang_name = self.language.as_ref()?;
        let syntect_name = language_to_syntect(lang_name)?;
        syntax_set().find_syntax_by_name(syntect_name)
    }

    /// Get the theme for highlighting
    fn get_theme(&self) -> Theme {
        let theme_set = theme_set();
        let theme_name = self.theme_name.as_deref().unwrap_or("base16-ocean.dark");
        theme_set.themes[theme_name].clone()
    }

    /// Get visible lines based on scroll position with optional syntax highlighting
    pub fn visible_lines(&self, state: &CodeViewerState) -> Vec<Line<'static>> {
        let lines: Vec<&str> = self.contents.lines().collect();
        let total_lines = lines.len();

        if total_lines == 0 {
            return vec![Line::from("")];
        }

        // Clamp scroll position
        let scroll = state.scroll.min(total_lines.saturating_sub(1));

        // Get visible lines
        let start = scroll;
        let end = std::cmp::min(start + state.viewport_height, total_lines);

        // Try to highlight the lines if syntax highlighting is enabled
        if self.syntax_highlighting {
            if let Ok(highlighted) = self.highlight_lines(&lines[start..end]) {
                return highlighted;
            }
        }

        // Fallback to plain text (either syntax highlighting disabled or failed)
        lines[start..end]
            .iter()
            .map(|&line| Line::from(line.to_string()))
            .collect()
    }

    /// Highlight lines using syntect directly
    fn highlight_lines(&self, lines: &[&str]) -> Result<Vec<Line<'static>>> {
        let syntax = match self.get_syntax() {
            Some(s) => s,
            None => return Ok(lines.iter().map(|&line| Line::from(line.to_string())).collect()),
        };
        
        let theme = self.get_theme();
        
        // Create a highlighter
        let mut highlighter = syntect::easy::HighlightLines::new(syntax, &theme);
        
        // Highlight each line individually
        let mut result = Vec::with_capacity(lines.len());
        
        for &line in lines {
            let regions: Vec<(syntect::highlighting::Style, &str)> = highlighter.highlight_line(line, syntax_set())?;
            
            // Build a Line from the highlighted regions
            let spans: Vec<Span> = regions.into_iter().map(|(style, text)| {
                let color = syntect_color_to_ratatui(style.foreground);
                Span::styled(text.to_string(), Style::new().fg(color))
            }).collect();
            
            result.push(Line::from(spans));
        }
        
        Ok(result)
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

    /// Get the display title (custom title or filename)
    pub fn display_title(&self) -> String {
        self.title.clone().unwrap_or_else(|| self.filename())
    }
}

impl Widget for &CodeViewerWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(Line::from(vec![
                Span::styled(self.display_title(), Style::new().bold().fg(Color::Cyan)),
                Span::raw(" - "),
                Span::styled(self.language_name(), Style::new().fg(Color::Gray)),
            ]))
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .border_style(Style::new().fg(Color::Gray));

        let inner = block.inner(area);
        block.render(area, buf);

        // For widget rendering, we just show all lines
        // The scroll state is managed by the component
        let lines: Vec<&str> = self.contents.lines().collect();

        if lines.is_empty() {
            buf.set_line(inner.x, inner.y, &Line::from(""), inner.width);
        } else {
            // Render all lines that fit in the area
            let max_lines = inner.height as usize;
            let lines_to_render = &lines[..max_lines.min(lines.len())];
            
            // Try to highlight if syntax highlighting is enabled
            if self.syntax_highlighting {
                if let Ok(highlighted) = self.highlight_lines(lines_to_render) {
                    for (i, line) in highlighted.iter().enumerate() {
                        let y = inner.y + i as u16;
                        if y < inner.y + inner.height {
                            buf.set_line(inner.x, y, line, inner.width);
                        }
                    }
                    return;
                }
            }

            // Fallback to plain text
            for (i, &line) in lines_to_render.iter().enumerate() {
                let y = inner.y + i as u16;
                if y < inner.y + inner.height {
                    buf.set_line(inner.x, y, &Line::from(line.to_string()), inner.width);
                }
            }
        }
    }
}

impl StatefulWidget for CodeViewerWidget {
    type State = CodeViewerState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let block = Block::default()
            .title(Line::from(vec![
                Span::styled(self.display_title(), Style::new().bold().fg(Color::Cyan)),
                Span::raw(" - "),
                Span::styled(self.language_name(), Style::new().fg(Color::Gray)),
            ]))
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .border_style(Style::new().fg(Color::Gray));

        let inner = block.inner(area);
        block.render(area, buf);

        // Get visible lines based on scroll state with syntax highlighting
        let lines = self.visible_lines(state);

        if lines.is_empty() {
            buf.set_line(inner.x, inner.y, &Line::from(""), inner.width);
        } else {
            // Render visible lines
            for (i, line) in lines.iter().enumerate() {
                let y = inner.y + i as u16;
                if y < inner.y + inner.height {
                    buf.set_line(inner.x, y, line, inner.width);
                }
            }
        }
    }
}

impl StatefulWidget for &CodeViewerWidget {
    type State = CodeViewerState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        // Clone and delegate to the owned version
        self.clone().render(area, buf, state)
    }
}
