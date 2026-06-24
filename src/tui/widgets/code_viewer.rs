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

/// Foreground paired with every overlay background below. Plain (un-highlighted) text relies on
/// the terminal's own default foreground, which is fine since it's paired with the terminal's own
/// default background too. But these overlay backgrounds are hardcoded dark colors, so on a
/// light-themed terminal (dark default foreground) they'd render as dark-on-dark; an explicit
/// light foreground keeps them readable regardless of the terminal's color scheme.
const OVERLAY_FG: Color = Color::Rgb(225, 225, 225);

/// Background color used to cross-highlight the node matched to the cursor on the other panel.
/// Deliberately brighter/more saturated than the diff colors above so the cursor stands out
/// against any of them rather than blending in at similar dark luminance.
const CROSS_HIGHLIGHT_BG: Color = Color::Rgb(40, 90, 200);

/// A range is a pure alignment marker (no real text on this side) when it has no width.
pub fn is_empty_range(range: &TextRange) -> bool {
    range.start_row == range.end_row && range.start_column == range.end_column
}

/// Build indices into `ranges`, sorted by source start position (end position as a secondary
/// key, so a zero-width marker sharing a start with a real range sorts *before* it). This is
/// what lets `range_at` binary search instead of scanning every range on every cursor move.
fn build_range_order(ranges: &[RangeMatch]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..ranges.len()).collect();
    order.sort_by_key(|&i| {
        let r = &ranges[i].source;
        (r.start_row, r.start_column, r.end_row, r.end_column)
    });
    order
}

/// Find the range whose source covers `(row, col)`, in O(log n) via binary search over `order`
/// rather than a linear scan over `ranges`. At most one range can cover any given point, since
/// ranges on one side are non-overlapping by construction (see `text_range.rs`); zero-width
/// ranges never match, since `point < point` is never true.
fn range_at(ranges: &[RangeMatch], order: &[usize], row: usize, col: usize) -> Option<usize> {
    let split = order.partition_point(|&i| {
        let r = &ranges[i].source;
        (r.start_row, r.start_column) <= (row, col)
    });
    let candidate = *order[..split].last()?;
    let r = &ranges[candidate].source;
    ((row, col) < (r.end_row, r.end_column)).then_some(candidate)
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
    /// Indices into `ranges`, sorted by source start position; rebuilt by `load_ranges`. Backs
    /// the O(log n) (row, column) -> range lookup in `cursor_destination`/`range_at_cursor`.
    range_order: Vec<usize>,
    /// The cursor's row in the file (0-indexed), like a normal text cursor.
    pub cursor_row: usize,
    /// The cursor's column on `cursor_row` (0-indexed, in characters).
    pub cursor_col: usize,
    /// The range to cross-highlight in blue, set from the matched node on the other panel.
    pub highlight_destination: Option<TextRange>,
}

impl CodeViewerState {
    /// Replace the diff ranges for this side, rebuild the point-lookup index, and place the
    /// cursor on the first navigable (non-zero-width) position.
    pub fn load_ranges(&mut self, ranges: Vec<RangeMatch>) {
        self.ranges = ranges;
        self.range_order = build_range_order(&self.ranges);
        let first_navigable = self
            .range_order
            .iter()
            .map(|&i| &self.ranges[i])
            .find(|range_match| !is_empty_range(&range_match.source));
        let (row, col) = first_navigable
            .map(|range_match| {
                (
                    range_match.source.start_row,
                    range_match.source.start_column,
                )
            })
            .unwrap_or((0, 0));
        self.cursor_row = row;
        self.cursor_col = col;
        self.highlight_destination = None;
    }

    /// The index into `ranges` of the range covering the cursor's current position, if any (the
    /// cursor can sit in a gap with no range under it, e.g. on blank/unmapped text).
    fn range_at_cursor(&self) -> Option<usize> {
        range_at(
            &self.ranges,
            &self.range_order,
            self.cursor_row,
            self.cursor_col,
        )
    }

    /// The destination range matched to whatever the cursor is currently on, i.e. the range to
    /// cross-highlight on the other panel.
    pub fn cursor_destination(&self) -> Option<TextRange> {
        self.range_at_cursor()
            .map(|i| self.ranges[i].destination.clone())
    }
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

    /// Number of characters on `row`, or 0 if `row` is out of bounds. Used to clamp the cursor's
    /// column so it never lands past the end of a (possibly shorter) line.
    pub fn line_len(&self, row: usize) -> usize {
        self.highlighted_lines
            .get(row)
            .map(|line| line.spans.iter().map(|s| s.content.chars().count()).sum())
            .unwrap_or(0)
    }

    /// The area inside this widget's own border, given the full area it would be rendered into.
    /// Exposed so callers (e.g. terminal-cursor placement in `CodeViewer`) can compute screen
    /// coordinates without duplicating the border's inset.
    pub fn inner_area(area: Rect) -> Rect {
        Block::default().borders(Borders::ALL).inner(area)
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
        let cursor_range = range_at(
            &state.ranges,
            &state.range_order,
            state.cursor_row,
            state.cursor_col,
        );

        for (index, range_match) in state.ranges.iter().enumerate() {
            let Some((start_col, end_col)) = columns_on_row(&range_match.source, row, row_len)
            else {
                continue;
            };

            if let Some(bg) = background_for_operation(&range_match.operation) {
                line = paint_columns(
                    &line,
                    start_col,
                    end_col,
                    Style::new().fg(OVERLAY_FG).bg(bg),
                );
            }

            // The leaf range under the cursor's exact (row, column) position, and the matching
            // range on the other panel (below), both render in the same blue: they're the same
            // visual signal ("this is the node under/matched to the cursor"), just on different
            // panels. The literal cursor position itself is drawn as the real terminal cursor
            // (see `CodeViewer::cursor_screen_position`), not by this overlay.
            if cursor_range == Some(index) {
                line = paint_columns(
                    &line,
                    start_col,
                    end_col,
                    Style::new().fg(OVERLAY_FG).bg(CROSS_HIGHLIGHT_BG),
                );
            }
        }

        if let Some(destination) = &state.highlight_destination
            && let Some((start_col, end_col)) = columns_on_row(destination, row, row_len)
        {
            line = paint_columns(
                &line,
                start_col,
                end_col,
                Style::new().fg(OVERLAY_FG).bg(CROSS_HIGHLIGHT_BG),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::text::RangeMatch;

    fn widget_with_line(text: &str) -> CodeViewerWidget {
        CodeViewerWidget::default().with_contents(text.to_string())
    }

    fn range_match(operation: TextOperation, start_col: usize, end_col: usize) -> RangeMatch {
        RangeMatch {
            source: TextRange::new(0, start_col, 0, end_col),
            destination: TextRange::zero(),
            operation,
        }
    }

    /// A diff-colored span must carry an explicit foreground, not just a background: plain text
    /// has no fg override (syntax highlighting is off by default) and relies on the terminal's
    /// own default, which is unreadable against a hardcoded dark diff background on a
    /// light-themed terminal.
    #[test]
    fn diff_overlay_pairs_explicit_foreground_with_background() {
        let widget = widget_with_line("hello world");
        let ranges = vec![range_match(TextOperation::Insert, 0, 5)];
        let range_order = build_range_order(&ranges);
        let state = CodeViewerState {
            ranges,
            range_order,
            // Far outside the only range, so it's never also treated as the cursor's range.
            cursor_row: 0,
            cursor_col: 99,
            viewport_height: 1,
            ..Default::default()
        };

        let line = widget.overlay_row(0, &state);
        let span = &line.spans[0];
        assert_eq!(span.content, "hello");
        assert_eq!(span.style.fg, Some(OVERLAY_FG));
        assert_eq!(
            span.style.bg,
            background_for_operation(&TextOperation::Insert)
        );
    }

    /// The range under the cursor's exact (row, column) position likewise needs the explicit
    /// foreground, and its background must be the brighter cross-highlight blue rather than the
    /// (dimmer) diff color underneath it.
    #[test]
    fn cursor_overlay_uses_bright_blue_with_explicit_foreground() {
        let widget = widget_with_line("hello world");
        let ranges = vec![range_match(TextOperation::Insert, 0, 5)];
        let range_order = build_range_order(&ranges);
        let state = CodeViewerState {
            ranges,
            range_order,
            cursor_row: 0,
            cursor_col: 0,
            viewport_height: 1,
            ..Default::default()
        };

        let line = widget.overlay_row(0, &state);
        let span = &line.spans[0];
        assert_eq!(span.content, "hello");
        assert_eq!(span.style.fg, Some(OVERLAY_FG));
        assert_eq!(span.style.bg, Some(CROSS_HIGHLIGHT_BG));
    }

    /// Binary-search lookup: finds the covering range, picks the real range over a zero-width
    /// marker that shares its start position, and returns `None` in gaps and on other rows.
    #[test]
    fn range_at_finds_covering_range_and_resolves_ties_and_gaps() {
        let ranges = vec![
            range_match(TextOperation::Delete, 0, 3),
            RangeMatch {
                source: TextRange::new(0, 3, 0, 3),
                destination: TextRange::zero(),
                operation: TextOperation::Insert,
            },
            range_match(TextOperation::Identical, 3, 7),
        ];
        let order = build_range_order(&ranges);

        assert_eq!(range_at(&ranges, &order, 0, 1), Some(0));
        assert_eq!(range_at(&ranges, &order, 0, 3), Some(2));
        assert_eq!(range_at(&ranges, &order, 0, 6), Some(2));
        assert_eq!(range_at(&ranges, &order, 0, 7), None);
        assert_eq!(range_at(&ranges, &order, 1, 0), None);
    }

    /// Loading ranges places the cursor on the first navigable position, skipping any leading
    /// zero-width marker.
    #[test]
    fn load_ranges_places_cursor_on_first_navigable_position() {
        let mut state = CodeViewerState::default();
        state.load_ranges(vec![
            RangeMatch {
                source: TextRange::new(0, 0, 0, 0),
                destination: TextRange::zero(),
                operation: TextOperation::Insert,
            },
            range_match(TextOperation::Delete, 2, 4),
        ]);
        assert_eq!((state.cursor_row, state.cursor_col), (0, 2));
    }

    /// `cursor_destination` resolves the cursor's current position to the matched range's
    /// destination, which is what drives the other panel's cross-highlight.
    #[test]
    fn cursor_destination_returns_matched_range_for_current_position() {
        let mut state = CodeViewerState::default();
        let dest = TextRange::new(5, 1, 5, 9);
        state.load_ranges(vec![RangeMatch {
            source: TextRange::new(0, 2, 0, 4),
            destination: dest.clone(),
            operation: TextOperation::Update,
        }]);
        assert_eq!(state.cursor_destination(), Some(dest));
    }

    /// The cross-highlighted destination on the *other* panel gets the same treatment, even when
    /// that range isn't a diff (e.g. an `Identical` range with no background of its own).
    #[test]
    fn cross_highlight_destination_uses_bright_blue_with_explicit_foreground() {
        let widget = widget_with_line("hello world");
        let state = CodeViewerState {
            highlight_destination: Some(TextRange::new(0, 6, 0, 11)),
            viewport_height: 1,
            ..Default::default()
        };

        let line = widget.overlay_row(0, &state);
        let span = line
            .spans
            .iter()
            .find(|span| span.content == "world")
            .expect("highlighted span");
        assert_eq!(span.style.fg, Some(OVERLAY_FG));
        assert_eq!(span.style.bg, Some(CROSS_HIGHLIGHT_BG));
    }
}
