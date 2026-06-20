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
    widgets::{Block, Borders, StatefulWidget},
};
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

use crate::code::language::language_for_path;
use crate::diff::text::{RangeMatch, TextOperation};
use crate::diff::text_range::TextRange;

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
fn syntect_color_to_ratatui(color: syntect::highlighting::Color) -> ratatui::style::Color {
    ratatui::style::Color::Rgb(color.r, color.g, color.b)
}

/// Background color used to paint a given diff operation, or `None` for `Identical`/`NotYetSet`
/// ranges which keep plain syntax highlighting.
fn background_for_operation(operation: &TextOperation) -> Option<Color> {
    match operation {
        TextOperation::Insert => Some(Color::Rgb(20, 60, 20)),
        TextOperation::Delete => Some(Color::Rgb(70, 20, 20)),
        TextOperation::Move => Some(Color::Rgb(70, 60, 10)),
        TextOperation::Update => Some(Color::Rgb(60, 20, 60)),
        TextOperation::Identical | TextOperation::NotYetSet => None,
    }
}

/// Background color used to cross-highlight the node matched to the cursor on the other panel.
const CROSS_HIGHLIGHT_BG: Color = Color::Rgb(20, 40, 80);

/// A range is a pure alignment marker (no real text on this side) when it has no width.
pub fn is_empty_range(range: &TextRange) -> bool {
    range.start_row == range.end_row && range.start_column == range.end_column
}

/// Returns the `[start_column, end_column)` portion of `range` that falls on `row`, given the
/// number of characters on that row, or `None` if `range` doesn't cover any part of `row`.
fn columns_on_row(range: &TextRange, row: usize, row_len: usize) -> Option<(usize, usize)> {
    if row < range.start_row || row > range.end_row {
        return None;
    }
    let start_col = if row == range.start_row {
        range.start_column
    } else {
        0
    };
    let end_col = if row == range.end_row {
        range.end_column
    } else {
        row_len
    };
    if start_col >= end_col {
        return None;
    }
    Some((start_col, end_col))
}

/// Paint `style` onto the `[start_col, end_col)` character range of `line`, preserving the
/// existing styling (e.g. syntax-highlight foreground colors) outside of and underneath it.
fn paint_columns(
    line: &Line<'static>,
    start_col: usize,
    end_col: usize,
    style: Style,
) -> Line<'static> {
    let mut spans = Vec::with_capacity(line.spans.len() + 2);
    let mut col = 0usize;

    for span in &line.spans {
        let text: Vec<char> = span.content.chars().collect();
        let span_len = text.len();
        let span_start = col;
        let span_end = col + span_len;
        col = span_end;

        if span_end <= start_col || span_start >= end_col {
            spans.push(span.clone());
            continue;
        }

        let local_start = start_col.saturating_sub(span_start).min(span_len);
        let local_end = end_col.saturating_sub(span_start).min(span_len);

        if local_start > 0 {
            let before: String = text[0..local_start].iter().collect();
            spans.push(Span::styled(before, span.style));
        }
        if local_end > local_start {
            let painted: String = text[local_start..local_end].iter().collect();
            spans.push(Span::styled(painted, span.style.patch(style)));
        }
        if local_end < span_len {
            let after: String = text[local_end..].iter().collect();
            spans.push(Span::styled(after, span.style));
        }
    }

    Line::from(spans)
}

/// The state for the CodeViewer widget: scroll position, viewport, and the diff/cursor overlay.
#[derive(Default, Clone)]
pub struct CodeViewerState {
    /// Scroll position (line number)
    pub scroll: usize,
    /// Viewport height in lines
    pub viewport_height: usize,
    /// The diff ranges for this side, as returned by `TextDiff::all`.
    pub ranges: Vec<RangeMatch>,
    /// Index into `ranges` of the range the cursor is currently on.
    pub cursor: usize,
    /// The range to cross-highlight in blue, set from the matched node on the other panel.
    pub highlight_destination: Option<TextRange>,
}

/// A widget that displays source code
///
/// This is a stateful widget that displays file contents with syntax highlighting plus an
/// optional diff/cursor overlay. Syntax highlighting is computed once per loaded file and
/// cached, since re-highlighting on every render frame is too slow for real-time scrolling.
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
    /// The full file, syntax-highlighted once and cached; rebuilt whenever the content, language,
    /// theme, or highlighting toggle changes.
    highlighted_lines: Vec<Line<'static>>,
}

impl CodeViewerWidget {
    /// Create a new CodeViewerWidget
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a CodeViewerWidget for a specific file
    pub fn with_file(path: PathBuf) -> Self {
        let mut widget = Self {
            file_path: Some(path),
            ..Default::default()
        };
        widget.rebuild_highlight_cache();
        widget
    }

    /// Load a file into the viewer
    pub fn load_file(&mut self, path: PathBuf) -> Result<()> {
        let contents = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read file: {:?}", path))?;
        self.language = language_for_path(&path);
        self.file_path = Some(path);
        self.contents = contents;
        self.rebuild_highlight_cache();
        Ok(())
    }

    /// Load already-read contents into the viewer without touching the filesystem.
    ///
    /// Used when the content was already read elsewhere (e.g. by a background diff
    /// computation), so the UI thread doesn't redo a blocking file read.
    pub fn load_contents(&mut self, path: PathBuf, contents: String) {
        self.language = language_for_path(&path);
        self.file_path = Some(path);
        self.contents = contents;
        self.rebuild_highlight_cache();
    }

    /// Set the contents directly (for testing or custom content)
    pub fn with_contents(mut self, contents: String) -> Self {
        self.contents = contents;
        self.rebuild_highlight_cache();
        self
    }

    /// Set the file path
    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.file_path = Some(path.clone());
        self.language = language_for_path(&path);
        self.rebuild_highlight_cache();
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
        self.rebuild_highlight_cache();
        self
    }

    /// Enable or disable syntax highlighting
    pub fn with_syntax_highlighting(mut self, enabled: bool) -> Self {
        self.syntax_highlighting = enabled;
        self.rebuild_highlight_cache();
        self
    }

    /// Enable syntax highlighting
    pub fn enable_syntax_highlighting(&mut self) {
        self.syntax_highlighting = true;
        self.rebuild_highlight_cache();
    }

    /// Disable syntax highlighting
    pub fn disable_syntax_highlighting(&mut self) {
        self.syntax_highlighting = false;
        self.rebuild_highlight_cache();
    }

    /// Check if syntax highlighting is enabled
    pub fn is_syntax_highlighting_enabled(&self) -> bool {
        self.syntax_highlighting
    }

    /// Set the theme for syntax highlighting
    pub fn set_theme(&mut self, theme_name: String) {
        self.theme_name = Some(theme_name);
        self.rebuild_highlight_cache();
    }

    /// Get the total number of lines
    pub fn line_count(&self) -> usize {
        self.highlighted_lines.len()
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

    /// Recompute the cached, syntax-highlighted representation of the whole file.
    ///
    /// This is the only place syntax highlighting actually runs; it must only be called when
    /// `contents`/`language`/`theme_name`/`syntax_highlighting` change, never per-frame.
    fn rebuild_highlight_cache(&mut self) {
        let lines: Vec<&str> = self.contents.lines().collect();

        self.highlighted_lines = if self.syntax_highlighting {
            self.highlight_lines(&lines).unwrap_or_else(|_| {
                lines
                    .iter()
                    .map(|&line| Line::from(line.to_string()))
                    .collect()
            })
        } else {
            lines
                .iter()
                .map(|&line| Line::from(line.to_string()))
                .collect()
        };
    }

    /// Highlight lines using syntect directly
    fn highlight_lines(&self, lines: &[&str]) -> Result<Vec<Line<'static>>> {
        let syntax = match self.get_syntax() {
            Some(s) => s,
            None => {
                return Ok(lines
                    .iter()
                    .map(|&line| Line::from(line.to_string()))
                    .collect());
            }
        };

        let theme = self.get_theme();
        let mut highlighter = syntect::easy::HighlightLines::new(syntax, &theme);
        let mut result = Vec::with_capacity(lines.len());

        for &line in lines {
            let regions: Vec<(syntect::highlighting::Style, &str)> =
                highlighter.highlight_line(line, syntax_set())?;

            let spans: Vec<Span> = regions
                .into_iter()
                .map(|(style, text)| {
                    let color = syntect_color_to_ratatui(style.foreground);
                    Span::styled(text.to_string(), Style::new().fg(color))
                })
                .collect();

            result.push(Line::from(spans));
        }

        Ok(result)
    }

    /// Get visible lines based on scroll position, with the diff/cursor overlay applied.
    pub fn visible_lines(&self, state: &CodeViewerState) -> Vec<Line<'static>> {
        let total_lines = self.highlighted_lines.len();

        if total_lines == 0 {
            return vec![Line::from("")];
        }

        let scroll = state.scroll.min(total_lines.saturating_sub(1));
        let start = scroll;
        let end = std::cmp::min(start + state.viewport_height, total_lines);

        (start..end)
            .map(|row| self.overlay_row(row, state))
            .collect()
    }

    /// Apply diff coloring, the cross-panel highlight, and the cursor marker to one row.
    fn overlay_row(&self, row: usize, state: &CodeViewerState) -> Line<'static> {
        let mut line = self.highlighted_lines[row].clone();
        let row_len: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();

        for (index, range_match) in state.ranges.iter().enumerate() {
            let Some((start_col, end_col)) = columns_on_row(&range_match.source, row, row_len)
            else {
                continue;
            };

            if let Some(bg) = background_for_operation(&range_match.operation) {
                line = paint_columns(&line, start_col, end_col, Style::new().bg(bg));
            }

            // The cursor's own leaf range, and the matching range on the other panel (below),
            // both render in the same blue: they're the same visual signal ("this is the node
            // under/matched to the cursor"), just on different panels.
            if index == state.cursor {
                line = paint_columns(&line, start_col, end_col, Style::new().bg(CROSS_HIGHLIGHT_BG));
            }
        }

        if let Some(destination) = &state.highlight_destination
            && let Some((start_col, end_col)) = columns_on_row(destination, row, row_len)
        {
            line = paint_columns(
                &line,
                start_col,
                end_col,
                Style::new().bg(CROSS_HIGHLIGHT_BG),
            );
        }

        line
    }

    /// Whether a file has been loaded into this viewer yet.
    pub fn has_file(&self) -> bool {
        self.file_path.is_some()
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

impl StatefulWidget for &CodeViewerWidget {
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

        let lines = self.visible_lines(state);

        if lines.is_empty() {
            buf.set_line(inner.x, inner.y, &Line::from(""), inner.width);
        } else {
            for (i, line) in lines.iter().enumerate() {
                let y = inner.y + i as u16;
                if y < inner.y + inner.height {
                    buf.set_line(inner.x, y, line, inner.width);
                }
            }
        }
    }
}
