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
use ratatui::{buffer::Buffer, prelude::*, text::Line, widgets::StatefulWidget};
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

use crate::code::language::language_for_path_and_content;
use crate::diff::text::{RangeMatch, TextOperation};
use crate::diff::text_range::TextRange;
use crate::tui::theme::{OverlayPalette, OverlayTheme};

/// Static syntax set loaded once
static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();

/// Static theme set loaded once
static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

/// Get or initialize the syntax set. `two_face::syntax::extra_newlines()` - not plain syntect
/// `SyntaxSet::load_defaults_newlines()` - since syntect's own bundled set (derived from Sublime
/// Text's stock package) has no definition at all for several languages this project actually
/// diffs: Dart, Kotlin, Swift, TypeScript/TSX, and Vimscript all silently fell back to unstyled
/// plain text before this. `two-face` bundles the much larger syntax set `bat` ships with, which
/// covers all of those - confirmed against every `language_to_syntect` name below. Bazel/Starlark
/// still has no definition in either set and remains an unhighlighted gap.
fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(two_face::syntax::extra_newlines)
}

/// Get or initialize the theme set
fn theme_set() -> &'static ThemeSet {
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

/// Every syntax-highlighting theme name syntect ships, sorted, for the theme dialog's dropdown.
/// Deliberately syntect's own built-ins rather than a curated subset: they are what
/// `set_theme`/`get_theme` can actually resolve, so anything else would be an option that
/// silently does nothing.
pub fn syntax_theme_names() -> Vec<String> {
    let mut names: Vec<String> = theme_set().themes.keys().cloned().collect();
    names.sort();
    names
}

/// Map our internal Language enum to syntect syntax name
fn language_to_syntect(lang: &crate::code::Language) -> Option<&'static str> {
    use crate::code::Language::*;

    match lang {
        // No Bazel/Starlark definition in either syntect's own bundled set or two-face's
        // extended one - see `syntax_set`'s doc comment.
        Bazel => None,
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
        // Singular, not "Protocol Buffers" - that's how two-face's extra set names it.
        ProtoBuf => Some("Protocol Buffer"),
        Python => Some("Python"),
        R => Some("R"),
        Ruby => Some("Ruby"),
        Rust => Some("Rust"),
        SQL => Some("SQL"),
        Scala => Some("Scala"),
        // Not "Bash" - neither set has a syntax literally named that.
        ShellScript => Some("Bourne Again Shell (bash)"),
        Swift => Some("Swift"),
        // Not "TSX" - two-face's extra set names it "TypeScriptReact".
        TSX => Some("TypeScriptReact"),
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

/// Background color used to paint a given diff operation from `palette`, or `None` for
/// `Identical`/`NotYetSet` ranges which keep plain syntax highlighting.
fn background_for_operation(operation: &TextOperation, palette: &OverlayPalette) -> Option<Color> {
    match operation {
        TextOperation::Insert => Some(palette.insert_bg),
        TextOperation::Delete => Some(palette.delete_bg),
        TextOperation::Move => Some(palette.move_bg),
        TextOperation::Update => Some(palette.update_bg),
        TextOperation::Identical | TextOperation::NotYetSet => None,
    }
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

/// The nearest position in `positions` (assumed sorted) strictly after (`forward = true`) or
/// before (`forward = false`) `cursor`, wrapping around at the ends rather than stopping - shared
/// by change navigation (`n`/`p`) and search navigation (`>`/`<`), which both jump in exactly this
/// way, just over a different position list.
fn next_position(
    positions: &[(usize, usize)],
    cursor: (usize, usize),
    forward: bool,
) -> Option<(usize, usize)> {
    if forward {
        positions
            .iter()
            .find(|&&pos| pos > cursor)
            .or_else(|| positions.first())
            .copied()
    } else {
        positions
            .iter()
            .rev()
            .find(|&&pos| pos < cursor)
            .or_else(|| positions.last())
            .copied()
    }
}

/// Total entries in `positions`, and how many sit at or before `cursor` (1-indexed) - shared by
/// `change_count_and_index` and `search_match_count_and_index`. `None` if `positions` is empty.
fn count_and_index(positions: &[(usize, usize)], cursor: (usize, usize)) -> Option<(usize, usize)> {
    if positions.is_empty() {
        return None;
    }
    let index = positions
        .iter()
        .filter(|&&pos| pos <= cursor)
        .count()
        .max(1);
    Some((index, positions.len()))
}

/// `line`'s character count with any trailing whitespace run excluded - what `overlay_row` passes
/// as `columns_on_row`'s `row_len`, so a range that spans a row completely paints only up to its
/// last real character. No human painting a diff by hand ever marks a line's trailing whitespace,
/// least of all the newline past it, and the algorithmic rendering should read the same way -
/// see `TextPaintState::selection`/`span_covers` in `human_solver`, which hold the same rule for
/// the hand-painted ground truth this rendering is checked against.
///
/// Only ever narrows the *fallback* width used for a row a range doesn't end on - a range's own
/// `start_column`/`end_column` come straight from the diff and are untouched.
fn trailing_whitespace_trimmed_len(line: &Line<'_>) -> usize {
    // Bytes, matching the columns `TextRange` carries (see `text_range::SourceColumn`). This
    // counted characters, which on any row holding a multi-byte character clamped
    // `columns_on_row` to the wrong column and shifted every painted span after it.
    let total: usize = line.spans.iter().map(|s| s.content.len()).sum();
    let trailing_whitespace: usize = line
        .spans
        .iter()
        .rev()
        .flat_map(|span| span.content.chars().rev())
        .take_while(|c| c.is_whitespace())
        .map(char::len_utf8)
        .sum();
    total - trailing_whitespace
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
        let text: &str = span.content.as_ref();
        // Bytes, not characters: `start_col`/`end_col` are the byte columns `TextRange` carries.
        // Splitting on character indices read them as character offsets, so a row containing any
        // multi-byte character had its highlight shifted right by the accumulated difference.
        let span_len = text.len();
        let span_start = col;
        let span_end = col + span_len;
        col = span_end;

        if span_end <= start_col || span_start >= end_col {
            spans.push(span.clone());
            continue;
        }

        // Rounded back to a character boundary so a column landing inside a multi-byte character
        // splits before it rather than panicking the slice.
        let boundary = |mut index: usize| -> usize {
            index = index.min(span_len);
            while index > 0 && !text.is_char_boundary(index) {
                index -= 1;
            }
            index
        };
        let local_start = boundary(start_col.saturating_sub(span_start));
        let local_end = boundary(end_col.saturating_sub(span_start));

        if local_start > 0 {
            spans.push(Span::styled(text[0..local_start].to_string(), span.style));
        }
        if local_end > local_start {
            spans.push(Span::styled(
                text[local_start..local_end].to_string(),
                span.style.patch(style),
            ));
        }
        if local_end < span_len {
            spans.push(Span::styled(text[local_end..].to_string(), span.style));
        }
    }

    Line::from(spans)
}

/// The state for the CodeViewer widget: scroll position, viewport, and the diff/cursor overlay.
#[derive(Default, Clone)]
pub struct CodeViewerState {
    /// Scroll position (line number)
    pub scroll: usize,
    /// Horizontal scroll position (character column into every line). Nonzero only after the
    /// cursor has been driven past the right edge of the viewport - `CodeViewer::
    /// scroll_to_show_col` keeps the cursor's column inside `[scroll_col, scroll_col +
    /// viewport_width)` the same way `scroll_to_show_row` does rows.
    pub scroll_col: usize,
    /// Viewport height in lines
    pub viewport_height: usize,
    /// Viewport width in characters, *excluding* the line-number gutter - i.e. how many content
    /// columns are actually visible. Recorded by the widget's own `render` (the only place the
    /// real area width is known), consumed by `CodeViewer::scroll_to_show_col`. 0 until the
    /// first frame has rendered, which callers must treat as "unknown, don't scroll".
    pub viewport_width: usize,
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
    /// Whether this side's cursor is the one currently driving navigation (i.e. it's the side
    /// `Tab` last selected). Gates which of the two blue-highlight mechanisms below applies: the
    /// focused side shows the node under its own (live) cursor, while the unfocused side shows
    /// only `highlight_destination` pushed from the focused side - never both on the same panel,
    /// and never the unfocused side's own stale cursor position.
    pub is_focused: bool,
    /// Every current search match (from the `/` search modal), painted in the same blue as
    /// `highlight_destination` - see `CodeViewerWidget::find_matches`. Empty when no search is
    /// active.
    pub search_matches: Vec<TextRange>,
    /// Whether to paint the node highlight at all - the `H` toggle, **off by default** (which
    /// `#[derive(Default)]` gives for free on a `bool`, and which `theme::load_node_highlight`
    /// deliberately matches).
    ///
    /// Gates only the painting, never the navigation. `cursor_destination` - which drives
    /// cursor-following and scroll-syncing between the panels - is a separate lookup from
    /// `cursor_destination_for_highlight`, so turning this off changes what is drawn and nothing
    /// about how the cursor moves.
    pub node_highlight: bool,
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
            .find(|range_match| !range_match.source.is_empty());
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
        self.search_matches = Vec::new();
    }

    /// Swap the ranges without moving the cursor - for a change in how the *same* diff is painted,
    /// as opposed to a new diff arriving.
    ///
    /// `load_ranges` jumping to the first change is right when a diff is loaded and wrong when the
    /// reader merely asked to see more or less of the one already on screen: pressing `M` at line
    /// 400 and landing back at the top loses their place for no reason. The cursor is clamped
    /// rather than trusted, since the caller could in principle pass ranges for a different file.
    pub fn replace_ranges(&mut self, ranges: Vec<RangeMatch>, line_count: usize) {
        self.ranges = ranges;
        self.range_order = build_range_order(&self.ranges);
        self.cursor_row = self.cursor_row.min(line_count.saturating_sub(1));
        // The cross-panel highlight points at a range that may no longer exist; the search hits
        // are still valid, since they index text rather than ranges.
        self.highlight_destination = None;
    }

    /// Every change's `(row, column)` start position (anything but `Identical`/the `NotYetSet`
    /// sentinel, and not a zero-width placeholder), in document order - the ordered list
    /// `next_change_position`/`change_count_and_index` both walk.
    ///
    /// Consecutive per-row pieces of one `leading_whitespace: false` split
    /// (`split_into_per_row_pieces`) collapse to a single position here - same operation, same
    /// (always-placeholder, for the `Insert`/`Delete` shapes that split produces) destination,
    /// and contiguous rows, exactly the signature that split leaves and nothing else naturally
    /// produces. Without this, one multi-line insert would cost `n`/`p` one stop per row instead
    /// of one stop for the whole change, and the "change N/M" counter would over-count it the
    /// same way.
    fn change_positions(&self) -> Vec<(usize, usize)> {
        // `range_order` is already sorted by source start position, and filtering preserves that
        // order, so this comes out sorted with no extra work.
        let mut positions = Vec::new();
        let mut previous_group: Option<(TextOperation, TextRange, usize)> = None;
        for range_match in self.range_order.iter().map(|&i| &self.ranges[i]) {
            if matches!(
                range_match.operation,
                TextOperation::Identical | TextOperation::NotYetSet
            ) || range_match.source.is_empty()
            {
                continue;
            }
            let row = range_match.source.start_row;
            let continues_previous_group =
                previous_group
                    .as_ref()
                    .is_some_and(|(operation, destination, previous_row)| {
                        *operation == range_match.operation
                            && *destination == range_match.destination
                            && row == previous_row + 1
                    });
            previous_group = Some((
                range_match.operation.clone(),
                range_match.destination.clone(),
                row,
            ));
            if continues_previous_group {
                continue;
            }
            positions.push((row, range_match.source.start_column));
        }
        positions
    }

    /// Every current search match's `(row, column)` start position, in document order -
    /// `CodeViewerWidget::find_matches` already produces them row-major, left-to-right, so this is
    /// just a projection.
    fn search_positions(&self) -> Vec<(usize, usize)> {
        self.search_matches
            .iter()
            .map(|range| (range.start_row, range.start_column))
            .collect()
    }

    /// The `(row, column)` of the start of the nearest actual change strictly after (`forward =
    /// true`) or before (`forward = false`) the cursor's current position - what `n`/`p` jump to.
    /// Wraps around (forward past the last change goes to the first, and vice versa) rather than
    /// stopping at the ends, same convention as a search's `n`/`N`. `None` if there are no changes
    /// at all (e.g. two identical files).
    pub fn next_change_position(&self, forward: bool) -> Option<(usize, usize)> {
        next_position(
            &self.change_positions(),
            (self.cursor_row, self.cursor_col),
            forward,
        )
    }

    /// Total distinct changes, and how many of them sit at or before the cursor's current
    /// position (1-indexed) - the "change N/M" the footer shows after `n`/`p`. `None` if there are
    /// no changes at all, same condition as `next_change_position`. Landing exactly on a change
    /// (as `next_change_position` always does) counts that change itself, so pressing `n`
    /// repeatedly counts 1, 2, 3, ... in step with each jump.
    pub fn change_count_and_index(&self) -> Option<(usize, usize)> {
        count_and_index(&self.change_positions(), (self.cursor_row, self.cursor_col))
    }

    /// The nearest search match at or after the cursor, wrapping to the very first match if the
    /// cursor is past every match - what pressing Enter in the search modal jumps to. Unlike
    /// `next_search_match_position` (strictly after, so repeated `>` presses always advance),
    /// landing exactly on a match counts here: this is the *first* jump for a fresh search, not a
    /// step from a previous one, so a match right under the cursor should still be found.
    pub fn nearest_search_match_position(&self) -> Option<(usize, usize)> {
        let cursor = (self.cursor_row, self.cursor_col);
        let positions = self.search_positions();
        positions
            .iter()
            .find(|&&pos| pos >= cursor)
            .or_else(|| positions.first())
            .copied()
    }

    /// The `(row, column)` of the start of the nearest search match strictly after (`forward =
    /// true`) or before (`forward = false`) the cursor - what `>`/`<` jump to. Same wrap-around
    /// convention as `next_change_position`.
    pub fn next_search_match_position(&self, forward: bool) -> Option<(usize, usize)> {
        next_position(
            &self.search_positions(),
            (self.cursor_row, self.cursor_col),
            forward,
        )
    }

    /// Total current search matches, and how many sit at or before the cursor (1-indexed) - the
    /// "match N/M" the footer shows in place of "change N/M" while a search is active. `None` when
    /// there are no matches (including when no search has been run yet).
    pub fn search_match_count_and_index(&self) -> Option<(usize, usize)> {
        count_and_index(&self.search_positions(), (self.cursor_row, self.cursor_col))
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

    /// The destination range matched to whatever the cursor is currently on, i.e. the range the
    /// other panel's cursor should follow. Returned regardless of the underlying match's
    /// operation - unlike `cursor_destination_for_highlight`, this drives cursor/scroll
    /// following, which should track the cursor even over unchanged content.
    pub fn cursor_destination(&self) -> Option<TextRange> {
        self.range_at_cursor()
            .map(|i| self.ranges[i].destination.clone())
    }

    /// Same as `cursor_destination`, but `None` when the range under the cursor is an `Identical`
    /// match - used to decide whether to paint the cross-highlight on the other panel, since an
    /// unchanged region shouldn't be highlighted there any more than it is on this side (see
    /// `overlay_row`'s matching check on the focused side's own cursor range).
    pub fn cursor_destination_for_highlight(&self) -> Option<TextRange> {
        let i = self.range_at_cursor()?;
        if self.ranges[i].operation == TextOperation::Identical {
            return None;
        }
        Some(self.ranges[i].destination.clone())
    }
}

/// A widget that displays source code
///
/// This is a stateful widget that displays file contents with syntax highlighting plus an
/// optional diff/cursor overlay. Syntax highlighting is computed once per loaded file and
/// cached, since re-highlighting on every render frame is too slow for real-time scrolling.
#[derive(Clone)]
pub struct CodeViewerWidget {
    /// The path to the file being displayed
    file_path: Option<PathBuf>,
    /// The file contents
    contents: String,
    /// The language of the file
    language: Option<crate::code::Language>,
    /// The theme name to use for syntax highlighting
    theme_name: Option<String>,
    /// Whether syntax highlighting is enabled
    syntax_highlighting: bool,
    /// The full file, syntax-highlighted once and cached; rebuilt whenever the content, language,
    /// theme, or highlighting toggle changes.
    highlighted_lines: Vec<Line<'static>>,
    /// The palette used to paint the diff/cursor overlay (not the syntax-highlighting theme
    /// above); user-selectable via the `c` theme picker, see `tui/theme.rs`.
    overlay_theme: OverlayTheme,
}

impl Default for CodeViewerWidget {
    // Hand-written rather than `#[derive(Default)]` specifically so `syntax_highlighting`
    // defaults to `true`: the whole syntect-backed highlighting engine below was fully wired up
    // (`get_syntax`/`get_theme`/`rebuild_highlight_cache`) but nothing anywhere ever called
    // `enable_syntax_highlighting` - found dead in a 2026-07 code-health pass. Every other field
    // keeps the same default a derive would have given it.
    fn default() -> Self {
        Self {
            file_path: None,
            contents: String::new(),
            language: None,
            theme_name: None,
            syntax_highlighting: true,
            highlighted_lines: Vec::new(),
            overlay_theme: OverlayTheme::default(),
        }
    }
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
        self.language = language_for_path_and_content(&path, &contents);
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
        self.language = language_for_path_and_content(&path, &contents);
        self.file_path = Some(path);
        self.contents = contents;
        self.rebuild_highlight_cache();
    }

    /// Set the contents directly (for testing or custom content)
    #[cfg(test)]
    pub fn with_contents(mut self, contents: String) -> Self {
        self.contents = contents;
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

    /// Set the palette used to paint the diff/cursor overlay (distinct from the syntax-
    /// highlighting theme above). No cache rebuild needed: unlike syntax highlighting, the
    /// overlay is painted fresh on every frame in `overlay_row`, not cached in
    /// `highlighted_lines`.
    pub fn set_overlay_theme(&mut self, theme: OverlayTheme) {
        self.overlay_theme = theme;
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

    /// The raw text of `row` (syntax-highlighting spans concatenated back into plain text - they
    /// wrap the same characters, never add or remove any), or empty if `row` is out of bounds.
    /// Same indexing/cost as `line_len` - used to find whitespace boundaries for "sticky column"
    /// vertical cursor movement (see `CodeViewer::move_cursor_vertical`).
    pub fn line_text(&self, row: usize) -> String {
        self.highlighted_lines
            .get(row)
            .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
            .unwrap_or_default()
    }

    /// Every occurrence of `query` in the file, in document order, as `TextRange`s (always
    /// `start_row == end_row`: a search match never spans a line break, unlike diff ranges).
    /// Empty for an empty query. Columns are character offsets into the *original* line,
    /// matching every other column in this module (`cursor_col`, `columns_on_row`).
    ///
    /// Smart-case, the vim/ripgrep convention: an all-lowercase query matches
    /// case-insensitively; a query containing any uppercase character matches exactly - typing
    /// the capital is read as deliberately asking for it.
    ///
    /// The insensitive path matches char-by-char against the original line rather than
    /// lowercasing the whole line and searching that: `str::to_lowercase` isn't
    /// length-preserving for every character (e.g. 'İ' becomes two characters), so a byte offset
    /// found in a lowercased copy doesn't reliably map back to a column in the original - it
    /// would shift every match after such a character by however many characters the lowercasing
    /// added, misaligning the highlight.
    pub fn find_matches(&self, query: &str) -> Vec<TextRange> {
        if query.is_empty() {
            return Vec::new();
        }
        let case_sensitive = query.chars().any(char::is_uppercase);
        let query_chars: Vec<char> = if case_sensitive {
            query.chars().collect()
        } else {
            query.chars().flat_map(char::to_lowercase).collect()
        };
        let query_len = query_chars.len();
        let mut matches = Vec::new();
        for (row, line) in self.contents.lines().enumerate() {
            let chars: Vec<char> = line.chars().collect();
            if chars.len() < query_len {
                continue;
            }
            'starts: for start in 0..=(chars.len() - query_len) {
                for (offset, &query_char) in query_chars.iter().enumerate() {
                    let candidate = chars[start + offset];
                    let matched = if case_sensitive {
                        candidate == query_char
                    } else {
                        let mut lowered = candidate.to_lowercase();
                        lowered.next() == Some(query_char) && lowered.next().is_none()
                    };
                    if !matched {
                        continue 'starts;
                    }
                }
                matches.push(TextRange::new(row, start, row, start + query_len));
            }
        }
        matches
    }

    /// Get the syntax for highlighting based on language
    fn get_syntax(&self) -> Option<&'static SyntaxReference> {
        let lang_name = self.language.as_ref()?;
        let syntect_name = language_to_syntect(lang_name)?;
        syntax_set().find_syntax_by_name(syntect_name)
    }

    /// Get the theme for highlighting. Falls back to `base16-ocean.dark` (one of syntect's own
    /// bundled default themes, always present in `ThemeSet::load_defaults()`) if `theme_name` is
    /// unset or names a theme that doesn't exist - `set_theme` takes an arbitrary caller-supplied
    /// `String` with no validation, so indexing `theme_set.themes` directly on that name (as this
    /// used to) panicked on any unrecognized name instead of degrading gracefully.
    fn get_theme(&self) -> Theme {
        let theme_set = theme_set();
        let theme_name = self.theme_name.as_deref().unwrap_or("base16-ocean.dark");
        theme_set
            .themes
            .get(theme_name)
            .or_else(|| theme_set.themes.get("base16-ocean.dark"))
            .expect("base16-ocean.dark is one of syntect's own bundled default themes")
            .clone()
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
        let palette = self.overlay_theme.palette();
        let mut line = self.highlighted_lines[row].clone();
        let row_len = trailing_whitespace_trimmed_len(&line);
        let cursor_range = range_at(
            &state.ranges,
            &state.range_order,
            state.cursor_row,
            state.cursor_col,
        );

        for (index, range_match) in state.ranges.iter().enumerate() {
            let Some((start_col, end_col)) = range_match.source.columns_on_row(row, row_len) else {
                continue;
            };

            if let Some(bg) = background_for_operation(&range_match.operation, &palette) {
                line = paint_columns(
                    &line,
                    start_col,
                    end_col,
                    Style::new().fg(palette.overlay_fg).bg(bg),
                );
            }

            // The leaf range under the cursor's exact (row, column) position, and the matching
            // range on the other panel (below), both render in the same blue: they're the same
            // visual signal ("this is the node under/matched to the cursor"), just on different
            // panels. The literal cursor position itself is drawn as the real terminal cursor
            // (see `CodeViewer::cursor_screen_position`), not by this overlay. Only the focused
            // side draws this: an unfocused side's own `cursor_row`/`cursor_col` is just wherever
            // it was left, not a live cursor, so painting it here would show a stale highlight.
            // Skipped for an `Identical` match - highlighting a range that isn't part of any
            // change at all doesn't tell the user anything (see `sync_cross_highlight`'s matching
            // suppression on the other panel's side of this same signal).
            if state.node_highlight
                && state.is_focused
                && cursor_range == Some(index)
                && range_match.operation != TextOperation::Identical
            {
                line = paint_columns(
                    &line,
                    start_col,
                    end_col,
                    Style::new()
                        .fg(palette.overlay_fg)
                        .bg(palette.cross_highlight_bg),
                );
            }
        }

        // Search matches (from the `/` modal), painted in the theme's dedicated search color -
        // previously they shared the cross-highlight blue, which made a search hit and the
        // cursor's counterpart indistinguishable while a search was active (see
        // `OverlayPalette::search_bg`). Usually empty (no active search), so this loop is a
        // no-op on every other frame.
        for search_match in &state.search_matches {
            if let Some((start_col, end_col)) = search_match.columns_on_row(row, row_len) {
                line = paint_columns(
                    &line,
                    start_col,
                    end_col,
                    Style::new().fg(palette.overlay_fg).bg(palette.search_bg),
                );
            }
        }

        // The cross-highlight pushed from the focused side's cursor; only relevant on the
        // unfocused side (the focused side already shows its own cursor highlight above), so
        // switching focus can never paint both blues onto the same panel at once. Already `None`
        // for an `Identical` match - `DiffViewer::sync_cross_highlight` pushes
        // `cursor_destination_for_highlight` (not `cursor_destination`), which is `None` in that
        // case, so there's no operation to check here the way the focused side's own paint above
        // does.
        if state.node_highlight
            && !state.is_focused
            && let Some(destination) = &state.highlight_destination
            && let Some((start_col, end_col)) = destination.columns_on_row(row, row_len)
        {
            line = paint_columns(
                &line,
                start_col,
                end_col,
                Style::new()
                    .fg(palette.overlay_fg)
                    .bg(palette.cross_highlight_bg),
            );
        }

        line
    }

    /// Width of the line-number gutter in characters: the widest 1-indexed line number plus one
    /// trailing separator space. 0 (no gutter at all) when nothing is loaded, so the empty-panel
    /// hint state doesn't render a stray "1 " margin.
    pub fn gutter_width(&self) -> usize {
        if self.highlighted_lines.is_empty() {
            return 0;
        }
        self.highlighted_lines.len().to_string().len() + 1
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
}

/// The style for the line-number gutter and the `…` truncation markers - deliberately not part
/// of `OverlayPalette` (it isn't a diff signal, just chrome), and `DarkGray` reads as "dimmed"
/// against both light and dark terminal backgrounds.
const GUTTER_STYLE: Style = Style::new().fg(Color::DarkGray);

/// The horizontal window of `line` from character column `from`, `width` characters wide,
/// preserving each span's styling. When content is cut off at either edge, the outermost visible
/// character on that side is replaced with a dimmed `…` so the truncation is visible at all -
/// previously long lines were silently hard-cut at the panel edge with no indication anything was
/// missing.
fn slice_columns(line: &Line<'static>, from: usize, width: usize) -> Line<'static> {
    if width == 0 {
        return Line::from("");
    }
    let total: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
    let to = from + width;
    let mut out: Vec<Span<'static>> = Vec::new();

    if from > 0 {
        out.push(Span::styled("…", GUTTER_STYLE));
    }
    let content_from = if from > 0 { from + 1 } else { from };
    let content_to = if total > to { to - 1 } else { to };

    let mut col = 0usize;
    for span in &line.spans {
        let len = span.content.chars().count();
        let span_start = col;
        col += len;
        if col <= content_from {
            continue;
        }
        if span_start >= content_to {
            break;
        }
        let take_from = content_from.saturating_sub(span_start);
        let take_to = (content_to - span_start).min(len);
        let content: String = span
            .content
            .chars()
            .skip(take_from)
            .take(take_to - take_from)
            .collect();
        if !content.is_empty() {
            out.push(Span::styled(content, span.style));
        }
    }
    if total > to {
        out.push(Span::styled("…", GUTTER_STYLE));
    }
    Line::from(out)
}

impl StatefulWidget for &CodeViewerWidget {
    type State = CodeViewerState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        // No border, no title of its own - `DiffViewer` (the only caller) always draws exactly
        // one title line above whatever area it hands this widget, in both dual- and single-panel
        // mode. This widget used to draw a second one (filename + language) inside its own
        // bordered block, which in dual-panel mode just repeated the filename `DiffViewer`'s own
        // outer block already showed.
        let inner = area;

        let gutter_width = self.gutter_width();
        let content_width = (inner.width as usize).saturating_sub(gutter_width);
        // Recorded here because render is the only place the real area width is known - the
        // component layer reads it back for cursor-following horizontal scroll
        // (`CodeViewer::scroll_to_show_col`).
        state.viewport_width = content_width;

        let lines = self.visible_lines(state);

        if lines.is_empty() {
            buf.set_line(inner.x, inner.y, &Line::from(""), inner.width);
        } else {
            for (i, line) in lines.iter().enumerate() {
                let y = inner.y + i as u16;
                if y >= inner.y + inner.height {
                    continue;
                }
                let mut composed: Vec<Span<'static>> = Vec::new();
                if gutter_width > 0 {
                    composed.push(Span::styled(
                        format!("{:>w$} ", state.scroll + i + 1, w = gutter_width - 1),
                        GUTTER_STYLE,
                    ));
                }
                composed.extend(slice_columns(line, state.scroll_col, content_width).spans);
                buf.set_line(inner.x, y, &Line::from(composed), inner.width);
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

    /// Regression test: syntax highlighting was fully implemented (`get_syntax`/`get_theme`/
    /// `rebuild_highlight_cache`) but `syntax_highlighting` defaulted to `false` and nothing
    /// anywhere ever called `enable_syntax_highlighting` - the TUI never actually highlighted
    /// anything. Confirms it's on by default now, and that a real file with a language actually
    /// gets multiple differently-styled spans (not just one plain, unstyled span per line).
    #[test]
    fn syntax_highlighting_is_enabled_by_default_and_actually_highlights() {
        let mut widget = CodeViewerWidget::default();
        assert!(widget.is_syntax_highlighting_enabled());

        widget.load_contents(
            PathBuf::from("test.rs"),
            "fn main() { let x = 1; }".to_string(),
        );
        let line = &widget.highlighted_lines[0];
        assert!(
            line.spans.len() > 1,
            "a real Rust line should be split into multiple differently-styled spans by syntax \
             highlighting, got {} span(s): {:?}",
            line.spans.len(),
            line.spans
        );
    }

    /// Regression test: `language_to_syntect` named several languages using syntect's own default
    /// (Sublime stock) syntax names, which don't exist in that set at all (Dart, Kotlin, Swift,
    /// TypeScript, TSX) or exist under a different name (`ShellScript` as "Bash" instead of
    /// "Bourne Again Shell (bash)", `ProtoBuf` as plural instead of singular) - `find_syntax_by_name`
    /// silently returned `None` for all of these, so every file in those languages rendered fully
    /// unstyled with no error or indication anything was wrong. Confirms every language this
    /// project's own `Language` enum can name resolves to a real syntax now, except the one
    /// documented, genuine gap (Bazel/Starlark, in neither syntect's nor two-face's bundled sets).
    #[test]
    fn every_language_except_the_documented_bazel_gap_resolves_to_a_real_syntax() {
        use crate::code::Language;

        // `Language` doesn't derive an iterator (adding one purely for this one test wasn't
        // worth widening a core, widely-used enum's surface) - listed by hand instead, alongside
        // `Language::Unknown` and `Bazel`, the two that are expected to still have no syntax.
        let languages = [
            Language::Bazel,
            Language::C,
            Language::CPP,
            Language::CSS,
            Language::CSharp,
            Language::Dart,
            Language::Go,
            Language::HTML,
            Language::JSON,
            Language::Java,
            Language::JavaScript,
            Language::Kotlin,
            Language::LUA,
            Language::Lisp,
            Language::MarkDown,
            Language::PHP,
            Language::ProtoBuf,
            Language::Python,
            Language::R,
            Language::Ruby,
            Language::Rust,
            Language::SQL,
            Language::Scala,
            Language::ShellScript,
            Language::Swift,
            Language::TSX,
            Language::TypeScript,
            Language::Vimscript,
            Language::YAML,
            Language::XML,
            Language::Unknown,
        ];

        for language in languages {
            let widget = CodeViewerWidget {
                language: Some(language),
                ..CodeViewerWidget::default()
            };
            let resolved = widget.get_syntax().is_some();
            if matches!(language, Language::Bazel | Language::Unknown) {
                assert!(
                    !resolved,
                    "{language:?} was expected to still have no syntax definition"
                );
            } else {
                assert!(resolved, "{language:?} should resolve to a real syntax");
            }
        }
    }

    /// Regression test: `get_theme` used to index `theme_set.themes[name]` directly, which
    /// panics for any name that isn't a real syntect theme - `set_theme` takes an arbitrary
    /// caller-supplied `String` with no validation, so this was trivially reachable.
    #[test]
    fn set_theme_with_an_unknown_name_falls_back_instead_of_panicking() {
        let mut widget = CodeViewerWidget::default();
        widget.set_theme("this-theme-does-not-exist".to_string());
        widget.load_contents(PathBuf::from("test.rs"), "fn main() {}".to_string());
        // Reaching here at all (no panic) is the actual assertion; also confirm it still produced
        // real output rather than silently going blank.
        assert!(!widget.highlighted_lines.is_empty());
    }

    #[test]
    fn find_matches_finds_case_insensitive_occurrences_in_document_order() {
        let widget = widget_with_line("Hello world\nworld hello WORLD\n");
        let matches = widget.find_matches("world");
        assert_eq!(
            matches,
            vec![
                TextRange::new(0, 6, 0, 11),
                TextRange::new(1, 0, 1, 5),
                TextRange::new(1, 12, 1, 17),
            ]
        );
    }

    #[test]
    fn find_matches_is_empty_for_an_empty_query_or_no_occurrences() {
        let widget = widget_with_line("hello world\n");
        assert_eq!(widget.find_matches(""), Vec::new());
        assert_eq!(widget.find_matches("xyz"), Vec::new());
    }

    /// U+0130 (Turkish dotted capital 'İ') lowercases to *two* characters ('i' plus a combining
    /// dot above) under Rust's locale-independent `to_lowercase`. Naively lowercasing the whole
    /// line before searching, then mapping a byte offset back through the *lowercased* copy, would
    /// shift every match after it by one column - matching char-by-char against the original line
    /// avoids that entirely, so "world" must still be found at its true column (2), not one column
    /// later.
    #[test]
    fn find_matches_columns_are_correct_when_lowercasing_changes_character_count() {
        let widget = widget_with_line("İ world\n");
        assert_eq!(
            widget.find_matches("world"),
            vec![TextRange::new(0, 2, 0, 7)]
        );
    }

    /// The palette `widget_with_line`'s widget uses, since it never overrides `overlay_theme`.
    fn default_palette() -> OverlayPalette {
        OverlayTheme::default().palette()
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
        let palette = default_palette();
        assert_eq!(span.style.fg, Some(palette.overlay_fg));
        assert_eq!(
            span.style.bg,
            background_for_operation(&TextOperation::Insert, &palette)
        );
    }

    /// A multi-row range's middle row is painted only up to its last real character - never the
    /// trailing whitespace after it, and least of all the newline past that. No human painting a
    /// diff by hand ever marks either, so the algorithmic rendering must not appear to either.
    #[test]
    fn multi_row_range_does_not_paint_a_middle_rows_trailing_whitespace() {
        let widget = widget_with_line("foo   \nbar");
        let ranges = vec![RangeMatch {
            source: TextRange::new(0, 0, 1, 3),
            destination: TextRange::zero(),
            operation: TextOperation::Move,
        }];
        let range_order = build_range_order(&ranges);
        let state = CodeViewerState {
            ranges,
            range_order,
            cursor_row: 0,
            cursor_col: 99,
            viewport_height: 2,
            ..Default::default()
        };

        let first_row = widget.overlay_row(0, &state);
        let palette = default_palette();
        let painted_bg = background_for_operation(&TextOperation::Move, &palette);

        // "foo" is painted, the three trailing spaces are not.
        assert_eq!(first_row.spans[0].content, "foo");
        assert_eq!(first_row.spans[0].style.bg, painted_bg);
        let trailing: String = first_row.spans[1..]
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(trailing, "   ");
        for span in &first_row.spans[1..] {
            assert_ne!(
                span.style.bg, painted_bg,
                "trailing whitespace must not carry the range's background: {first_row:?}"
            );
        }
    }

    /// A fresh widget must paint **nothing** extra over the focused side's cursor range: the node
    /// highlight is off until `H` turns it on.
    ///
    /// This test asserted the exact opposite until 2026-08-23, and the flip is deliberate, so the
    /// full history is worth keeping in one place. The highlight was originally behind an `x`
    /// toggle; that toggle was removed on 2026-08-08 because it "didn't improve the UX" and the
    /// highlight became unconditional. It is now toggleable again, under `H` - but off by
    /// default, which is the part the first attempt got wrong. On by default, a toggle only helps
    /// a user who already knows the key; the highlight repaints on every cursor movement and
    /// covers the diff-operation color underneath it, so the default is what determines whether
    /// it reads as a feature or as interference.
    #[test]
    fn node_highlight_is_off_until_enabled() {
        let widget = widget_with_line("hello world");
        let ranges = vec![range_match(TextOperation::Insert, 0, 5)];
        let range_order = build_range_order(&ranges);
        let mut focused_state = CodeViewerState {
            ranges,
            range_order,
            cursor_row: 0,
            cursor_col: 0,
            viewport_height: 1,
            is_focused: true,
            ..Default::default()
        };
        let palette = default_palette();

        let focused_span = &widget.overlay_row(0, &focused_state).spans[0];
        assert_eq!(
            focused_span.style.bg,
            background_for_operation(&TextOperation::Insert, &palette),
            "with the highlight off, the range keeps its own diff color"
        );

        // ...and `H` brings it back, unchanged from what it always painted.
        focused_state.node_highlight = true;
        let focused_span = &widget.overlay_row(0, &focused_state).spans[0];
        assert_eq!(
            focused_span.style.bg,
            Some(palette.cross_highlight_bg),
            "once enabled, a real change under the cursor is painted blue as before"
        );
    }

    /// The whole point of removing the toggle in favor of always-on: an `Identical` match (the
    /// cursor sitting on unchanged content) must *not* be painted blue - highlighting a range
    /// that isn't part of any change doesn't tell the user anything.
    #[test]
    fn cross_highlight_is_suppressed_for_an_identical_match() {
        let widget = widget_with_line("hello world");
        let ranges = vec![range_match(TextOperation::Identical, 0, 5)];
        let range_order = build_range_order(&ranges);
        let state = CodeViewerState {
            ranges,
            range_order,
            cursor_row: 0,
            cursor_col: 0,
            viewport_height: 1,
            is_focused: true,
            ..Default::default()
        };
        let palette = default_palette();
        assert_ne!(
            widget.overlay_row(0, &state).spans[0].style.bg,
            Some(palette.cross_highlight_bg),
            "the cursor sitting on unchanged content should not be painted blue"
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
            is_focused: true,
            // Opt in: the highlight ships off (see `CodeViewerState::node_highlight`), so a
            // test of what it paints has to enable it, exactly as a user pressing `H` does.
            node_highlight: true,
            ..Default::default()
        };

        let line = widget.overlay_row(0, &state);
        let span = &line.spans[0];
        assert_eq!(span.content, "hello");
        let palette = default_palette();
        assert_eq!(span.style.fg, Some(palette.overlay_fg));
        assert_eq!(span.style.bg, Some(palette.cross_highlight_bg));
    }

    /// An unfocused panel must not highlight its own (stale) cursor position: that mechanism is
    /// reserved for whichever side `Tab` last selected. This is the bug from exploratory
    /// testing, where the "after" side's first node stayed highlighted blue forever because its
    /// own never-moving cursor kept matching this check regardless of focus.
    #[test]
    fn unfocused_panel_does_not_highlight_its_own_cursor_range() {
        let widget = widget_with_line("hello world");
        let ranges = vec![range_match(TextOperation::Insert, 0, 5)];
        let range_order = build_range_order(&ranges);
        let state = CodeViewerState {
            ranges,
            range_order,
            cursor_row: 0,
            cursor_col: 0,
            viewport_height: 1,
            is_focused: false,
            ..Default::default()
        };

        let line = widget.overlay_row(0, &state);
        let span = &line.spans[0];
        assert_eq!(span.content, "hello");
        // Still gets the diff color (Insert), just not the cross-highlight blue.
        assert_eq!(
            span.style.bg,
            background_for_operation(&TextOperation::Insert, &default_palette())
        );
    }

    /// The focused panel must not also show a stale `highlight_destination` left over from
    /// before it gained focus: after `Tab`, the newly-focused side shows only its own live
    /// cursor, never both blues at once.
    #[test]
    fn focused_panel_ignores_stale_highlight_destination() {
        let widget = widget_with_line("hello world");
        let state = CodeViewerState {
            is_focused: true,
            // Left over from when this side was the unfocused cross-highlight target.
            highlight_destination: Some(TextRange::new(0, 6, 0, 11)),
            viewport_height: 1,
            ..Default::default()
        };

        let line = widget.overlay_row(0, &state);
        assert!(
            line.spans.iter().all(|span| span.style.bg.is_none()),
            "no span should be painted: highlight_destination is stale once focused"
        );
    }

    /// The whole point of the toggle: with it off - the shipped default - the focused panel's
    /// cursor range is left showing its diff-operation color, not repainted blue.
    #[test]
    fn node_highlight_off_leaves_the_cursor_range_showing_its_diff_color() {
        let widget = widget_with_line("hello world");
        let ranges = vec![range_match(TextOperation::Insert, 0, 5)];
        let range_order = build_range_order(&ranges);
        let state = CodeViewerState {
            ranges,
            range_order,
            cursor_row: 0,
            cursor_col: 0,
            viewport_height: 1,
            is_focused: true,
            // Not set: `Default` is `false`, which is the shipped default.
            ..Default::default()
        };

        let line = widget.overlay_row(0, &state);
        let palette = default_palette();
        let span = &line.spans[0];
        assert_eq!(span.content, "hello");
        assert_ne!(
            span.style.bg,
            Some(palette.cross_highlight_bg),
            "the node highlight must not paint while it is toggled off"
        );
        assert_eq!(
            span.style.bg,
            background_for_operation(&TextOperation::Insert, &palette),
            "the underlying diff color must survive - that is what the highlight was covering"
        );
    }

    /// The other panel's half of the same signal, also gated.
    #[test]
    fn node_highlight_off_leaves_the_counterpart_unpainted() {
        let widget = widget_with_line("hello world");
        let state = CodeViewerState {
            is_focused: false,
            highlight_destination: Some(TextRange::new(0, 6, 0, 11)),
            viewport_height: 1,
            ..Default::default()
        };

        let line = widget.overlay_row(0, &state);
        assert!(
            line.spans.iter().all(|span| span.style.bg.is_none()),
            "the counterpart highlight must not paint while the toggle is off"
        );
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

    /// Three changes on rows 2, 5, and 9, with `Identical` ranges filling the gaps between them -
    /// shared setup for `next_change_position`'s tests.
    fn state_with_three_changes_on_rows_2_5_and_9() -> CodeViewerState {
        let mut state = CodeViewerState::default();
        state.load_ranges(vec![
            RangeMatch {
                source: TextRange::new(0, 0, 2, 0),
                destination: TextRange::zero(),
                operation: TextOperation::Identical,
            },
            RangeMatch {
                source: TextRange::new(2, 0, 2, 4),
                destination: TextRange::zero(),
                operation: TextOperation::Delete,
            },
            RangeMatch {
                source: TextRange::new(2, 4, 5, 0),
                destination: TextRange::zero(),
                operation: TextOperation::Identical,
            },
            RangeMatch {
                source: TextRange::new(5, 0, 5, 4),
                destination: TextRange::zero(),
                operation: TextOperation::Update,
            },
            RangeMatch {
                source: TextRange::new(5, 4, 9, 0),
                destination: TextRange::zero(),
                operation: TextOperation::Identical,
            },
            RangeMatch {
                source: TextRange::new(9, 0, 9, 4),
                destination: TextRange::zero(),
                operation: TextOperation::Insert,
            },
        ]);
        state
    }

    #[test]
    fn next_change_position_finds_the_next_change_forward() {
        let mut state = state_with_three_changes_on_rows_2_5_and_9();
        state.cursor_row = 0;
        state.cursor_col = 0;
        assert_eq!(state.next_change_position(true), Some((2, 0)));

        state.cursor_row = 2;
        state.cursor_col = 0;
        assert_eq!(
            state.next_change_position(true),
            Some((5, 0)),
            "sitting exactly on a change should jump to the *next* one, not stay put"
        );
    }

    #[test]
    fn next_change_position_finds_the_previous_change_backward() {
        let mut state = state_with_three_changes_on_rows_2_5_and_9();
        state.cursor_row = 9;
        state.cursor_col = 0;
        assert_eq!(
            state.next_change_position(false),
            Some((5, 0)),
            "sitting exactly on a change should jump to the *previous* one, not stay put"
        );

        state.cursor_row = 7;
        state.cursor_col = 0;
        assert_eq!(state.next_change_position(false), Some((5, 0)));
    }

    #[test]
    fn next_change_position_wraps_around_at_the_ends() {
        let mut state = state_with_three_changes_on_rows_2_5_and_9();

        state.cursor_row = 9;
        state.cursor_col = 4; // past the last change
        assert_eq!(
            state.next_change_position(true),
            Some((2, 0)),
            "forward past the last change should wrap to the first"
        );

        state.cursor_row = 0;
        state.cursor_col = 0; // before the first change
        assert_eq!(
            state.next_change_position(false),
            Some((9, 0)),
            "backward before the first change should wrap to the last"
        );
    }

    #[test]
    fn next_change_position_is_none_when_the_file_has_no_changes() {
        let mut state = CodeViewerState::default();
        state.load_ranges(vec![RangeMatch {
            source: TextRange::new(0, 0, 5, 0),
            destination: TextRange::zero(),
            operation: TextOperation::Identical,
        }]);
        assert_eq!(state.next_change_position(true), None);
        assert_eq!(state.next_change_position(false), None);
    }

    #[test]
    fn change_count_and_index_is_none_when_the_file_has_no_changes() {
        let mut state = CodeViewerState::default();
        state.load_ranges(vec![RangeMatch {
            source: TextRange::new(0, 0, 5, 0),
            destination: TextRange::zero(),
            operation: TextOperation::Identical,
        }]);
        assert_eq!(state.change_count_and_index(), None);
    }

    /// The exact shape `split_into_per_row_pieces` (`RenderOptions::leading_whitespace:
    /// false`) produces for one multi-line insert: same operation, same (placeholder) destination,
    /// contiguous rows. `n`/`p` and the "change N/M" counter must treat all three rows as one stop,
    /// not three - otherwise turning that option on would make navigating a multi-line insert three
    /// times slower for no reason a reader asked for.
    #[test]
    fn change_positions_collapses_a_multi_row_insert_split_into_one_stop() {
        let mut state = CodeViewerState::default();
        let destination = TextRange::new(4, 0, 4, 0);
        state.load_ranges(vec![
            RangeMatch {
                source: TextRange::new(0, 4, 0, 25),
                destination: destination.clone(),
                operation: TextOperation::Insert,
            },
            RangeMatch {
                source: TextRange::new(1, 8, 1, 22),
                destination: destination.clone(),
                operation: TextOperation::Insert,
            },
            RangeMatch {
                source: TextRange::new(2, 8, 2, 15),
                destination,
                operation: TextOperation::Insert,
            },
        ]);

        assert_eq!(
            state.change_count_and_index(),
            Some((1, 1)),
            "three per-row pieces of one insert must count as a single change"
        );
        state.cursor_row = 0;
        state.cursor_col = 0;
        assert_eq!(
            state.next_change_position(true),
            Some((0, 4)),
            "pressing n from before the group should land on its first row and go nowhere else"
        );
    }

    /// The signal `change_positions` groups on (same operation, same destination, contiguous
    /// rows) must not fire for two genuinely separate changes that happen to land on adjacent
    /// rows with different destinations - each stays its own stop.
    #[test]
    fn change_positions_does_not_collapse_adjacent_but_unrelated_changes() {
        let mut state = CodeViewerState::default();
        state.load_ranges(vec![
            RangeMatch {
                source: TextRange::new(0, 0, 0, 4),
                destination: TextRange::new(4, 0, 4, 0),
                operation: TextOperation::Insert,
            },
            RangeMatch {
                source: TextRange::new(1, 0, 1, 4),
                destination: TextRange::new(9, 0, 9, 0),
                operation: TextOperation::Insert,
            },
        ]);

        assert_eq!(
            state.change_count_and_index(),
            Some((1, 2)),
            "different destinations mean these are two separate changes, not a split group"
        );
    }

    /// Landing exactly on a change (as `next_change_position` always does) must count that change
    /// itself, so repeatedly pressing `n` counts 1, 2, 3 in step with each jump rather than lagging
    /// or double-counting.
    #[test]
    fn change_count_and_index_counts_changes_at_or_before_the_cursor() {
        let mut state = state_with_three_changes_on_rows_2_5_and_9();

        state.cursor_row = 0;
        state.cursor_col = 0;
        assert_eq!(
            state.change_count_and_index(),
            Some((1, 3)),
            "before the first change, index should still report 1, not 0"
        );

        state.cursor_row = 2;
        state.cursor_col = 0;
        assert_eq!(state.change_count_and_index(), Some((1, 3)));

        state.cursor_row = 5;
        state.cursor_col = 0;
        assert_eq!(state.change_count_and_index(), Some((2, 3)));

        state.cursor_row = 9;
        state.cursor_col = 0;
        assert_eq!(state.change_count_and_index(), Some((3, 3)));
    }

    /// Three search matches on rows 1, 4, and 8 - shared setup for the search-navigation tests,
    /// mirroring `state_with_three_changes_on_rows_2_5_and_9` above.
    fn state_with_three_search_matches_on_rows_1_4_and_8() -> CodeViewerState {
        CodeViewerState {
            search_matches: vec![
                TextRange::new(1, 0, 1, 4),
                TextRange::new(4, 2, 4, 6),
                TextRange::new(8, 1, 8, 5),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn nearest_search_match_position_finds_the_match_at_or_after_the_cursor() {
        let mut state = state_with_three_search_matches_on_rows_1_4_and_8();
        state.cursor_row = 0;
        state.cursor_col = 0;
        assert_eq!(state.nearest_search_match_position(), Some((1, 0)));

        state.cursor_row = 4;
        state.cursor_col = 2;
        assert_eq!(
            state.nearest_search_match_position(),
            Some((4, 2)),
            "sitting exactly on a match should find that match, unlike next_change_position's \
             strictly-after semantics - this is the first jump for a fresh search"
        );

        state.cursor_row = 9;
        state.cursor_col = 0;
        assert_eq!(
            state.nearest_search_match_position(),
            Some((1, 0)),
            "past the last match should wrap to the first"
        );
    }

    #[test]
    fn nearest_search_match_position_is_none_with_no_matches() {
        let state = CodeViewerState::default();
        assert_eq!(state.nearest_search_match_position(), None);
    }

    #[test]
    fn next_search_match_position_wraps_around_at_the_ends() {
        let mut state = state_with_three_search_matches_on_rows_1_4_and_8();

        state.cursor_row = 4;
        state.cursor_col = 2;
        assert_eq!(
            state.next_search_match_position(true),
            Some((8, 1)),
            "sitting exactly on a match should jump to the *next* one, not stay put"
        );

        state.cursor_row = 8;
        state.cursor_col = 5; // past the last match
        assert_eq!(
            state.next_search_match_position(true),
            Some((1, 0)),
            "forward past the last match should wrap to the first"
        );

        state.cursor_row = 0;
        state.cursor_col = 0; // before the first match
        assert_eq!(
            state.next_search_match_position(false),
            Some((8, 1)),
            "backward before the first match should wrap to the last"
        );
    }

    #[test]
    fn search_match_count_and_index_counts_matches_at_or_before_the_cursor() {
        let mut state = state_with_three_search_matches_on_rows_1_4_and_8();

        state.cursor_row = 0;
        state.cursor_col = 0;
        assert_eq!(state.search_match_count_and_index(), Some((1, 3)));

        state.cursor_row = 4;
        state.cursor_col = 2;
        assert_eq!(state.search_match_count_and_index(), Some((2, 3)));

        state.cursor_row = 8;
        state.cursor_col = 5;
        assert_eq!(state.search_match_count_and_index(), Some((3, 3)));
    }

    #[test]
    fn search_match_count_and_index_is_none_with_no_matches() {
        let state = CodeViewerState::default();
        assert_eq!(state.search_match_count_and_index(), None);
    }

    #[test]
    fn load_ranges_clears_any_previous_search_matches() {
        let mut state = state_with_three_search_matches_on_rows_1_4_and_8();
        state.load_ranges(Vec::new());
        assert_eq!(state.search_matches, Vec::new());
    }

    /// `overlay_row` paints search matches in the same blue as the cross-highlight - see
    /// `cross_highlight_destination_uses_bright_blue_with_explicit_foreground` below for the
    /// non-search case this mirrors.
    #[test]
    fn overlay_row_paints_search_matches_in_the_dedicated_search_color() {
        let widget = widget_with_line("hello world");
        let state = CodeViewerState {
            search_matches: vec![TextRange::new(0, 6, 0, 11)],
            viewport_height: 1,
            ..Default::default()
        };

        let line = widget.overlay_row(0, &state);
        let span = line
            .spans
            .iter()
            .find(|span| span.content == "world")
            .expect("highlighted span");
        let palette = default_palette();
        assert_eq!(
            span.style.bg,
            Some(palette.search_bg),
            "search matches must use the dedicated search color, not the cursor blue"
        );
        assert_eq!(span.style.fg, Some(palette.overlay_fg));
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
            is_focused: false,
            highlight_destination: Some(TextRange::new(0, 6, 0, 11)),
            viewport_height: 1,
            node_highlight: true,
            ..Default::default()
        };

        let line = widget.overlay_row(0, &state);
        let span = line
            .spans
            .iter()
            .find(|span| span.content == "world")
            .expect("highlighted span");
        let palette = default_palette();
        assert_eq!(span.style.fg, Some(palette.overlay_fg));
        assert_eq!(span.style.bg, Some(palette.cross_highlight_bg));
    }

    /// `set_overlay_theme` must actually change what gets painted, with no rebuild step
    /// required: that's the whole point of the `c` theme picker.
    #[test]
    fn set_overlay_theme_changes_painted_colors() {
        let mut widget = widget_with_line("hello world");
        let ranges = vec![range_match(TextOperation::Insert, 0, 5)];
        let range_order = build_range_order(&ranges);
        let state = CodeViewerState {
            ranges,
            range_order,
            cursor_row: 0,
            cursor_col: 99,
            viewport_height: 1,
            ..Default::default()
        };

        let before = widget.overlay_row(0, &state).spans[0].style;
        widget.set_overlay_theme(OverlayTheme::SolarizedLight);
        let after = widget.overlay_row(0, &state).spans[0].style;

        assert_ne!(before.bg, after.bg);
        assert_eq!(
            after.fg,
            Some(OverlayTheme::SolarizedLight.palette().overlay_fg)
        );
    }

    /// This widget never draws its own border or title (that used to be togglable via
    /// `hide_border`/`set_hide_border`, removed once `DiffViewer` - the only caller - was made to
    /// always draw its own single title line instead, so nothing ever needed the widget's own
    /// border shown). Content should be flush against the top-left corner of whatever area it's
    /// given, with no title text drawn anywhere in the buffer.
    #[test]
    fn render_never_draws_its_own_border_or_title() {
        let area = Rect::new(0, 0, 20, 5);
        let mut state = CodeViewerState {
            viewport_height: 1,
            ..Default::default()
        };

        let mut buf = Buffer::empty(area);
        let widget = widget_with_line("hello");
        (&widget).render(area, &mut buf, &mut state);

        let text: String = buf.content().iter().map(|cell| cell.symbol()).collect();
        assert!(
            !text.contains(&widget.filename()),
            "no title should ever be drawn: {text}"
        );
        // Flush against the top-left corner - no border row/column to skip. The first cells are
        // the line-number gutter ("1 " for a 1-line file), then the content itself.
        assert_eq!(buf.get(0, 0).symbol(), "1", "gutter line number first");
        assert_eq!(
            buf.get(widget.gutter_width() as u16, 0).symbol(),
            "h",
            "content immediately after the gutter, no border row/column to skip"
        );
    }

    /// The gutter is exactly wide enough for the file's largest line number plus one separator
    /// space, and absent entirely (width 0) when nothing is loaded - the empty-panel hint state
    /// must not show a stray "1 " margin.
    #[test]
    fn gutter_width_tracks_line_count_and_disappears_when_empty() {
        assert_eq!(CodeViewerWidget::default().gutter_width(), 0);
        assert_eq!(widget_with_line("one line").gutter_width(), 2);
        let ninety_nine_lines = widget_with_line(&vec!["x"; 99].join("\n"));
        assert_eq!(ninety_nine_lines.gutter_width(), 3);
    }

    /// `slice_columns` is the horizontal-scroll window: it must preserve span styling, and mark
    /// a cut edge with a dimmed `…` so truncation is visible at all.
    #[test]
    fn slice_columns_windows_a_line_and_marks_cut_edges() {
        let line = Line::from(vec![
            Span::styled("abcde".to_string(), Style::new().fg(Color::Red)),
            Span::styled("fghij".to_string(), Style::new().fg(Color::Green)),
        ]);

        // Full width: no markers.
        let full = slice_columns(&line, 0, 20);
        let text: String = full.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "abcdefghij");

        // Cut on both sides: a width-5 window is two `…` markers plus three content characters.
        let window = slice_columns(&line, 2, 5);
        let text: String = window.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "…def…");
        assert_eq!(window.spans.first().unwrap().style, GUTTER_STYLE);
        assert_eq!(window.spans.last().unwrap().style, GUTTER_STYLE);
        // The styles of the surviving characters are preserved.
        assert!(
            window
                .spans
                .iter()
                .any(|s| s.content.contains('d') && s.style.fg == Some(Color::Red)),
            "red span styling should survive slicing: {window:?}"
        );
        assert!(
            window
                .spans
                .iter()
                .any(|s| s.content.contains('f') && s.style.fg == Some(Color::Green)),
            "green span styling should survive slicing: {window:?}"
        );

        // Cut only on the right.
        let prefix = slice_columns(&line, 0, 4);
        let text: String = prefix.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "abc…");

        // Zero width renders nothing rather than panicking.
        let empty = slice_columns(&line, 3, 0);
        let text: String = empty.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "");
    }
}
