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
//! What the server sends the page: a computed diff, and a re-highlighting of one.
//!
//! **Every column here is a UTF-16 code unit, not a byte.** The rest of this crate works in bytes
//! (`diff::text_range::SourceColumn`, and `tui::json_output` says so in its schema), because that
//! is what tree-sitter reports and what a terminal renderer slices by. A browser indexes a string
//! by UTF-16 code units, and `model.js` does every cursor step, range lookup and paint against
//! the string it holds - so the conversion happens once, here, against the real line text, rather
//! than on every lookup in JavaScript. `tui::json_output` deliberately does *not* do this (Neovim
//! consumes bytes directly); the two outputs serve different consumers and stay different.
//!
//! Syntax highlighting rides along as colour spans per line, from the same syntect setup
//! `tui::widgets::code_viewer` uses (same syntax set, same theme set, same language table), so
//! the browser paints what the terminal would have. Only the foreground colour is carried, which
//! is also all the TUI applies.

use serde::Serialize;
use std::path::Path;

use crate::code::{Language, language::language_for_path_and_content};
use crate::diff::text::{
    ChangeCounts, DiffSummary, RangeMatch, RenderOptions, TextOperation, change_counts,
    ranges_for_options, summarize_diff_with_comment_check,
};
use crate::diff::text_range::{TextRange, floor_char_boundary};
use crate::tui::actions::DiffSessionData;
use crate::tui::widgets::code_viewer::{language_to_syntect, syntax_set, theme_set};

/// syntect's own fallback, the one `tui::widgets::code_viewer::CodeViewerWidget::get_theme` also
/// reaches for when the configured name is unknown.
pub const DEFAULT_SYNTAX_THEME: &str = "base16-ocean.dark";

/// The UTF-16 index of `byte_column` in `line`. A column past the end of the line (a range that
/// runs to the row's end, or the `(next row, 0)` normalization landing on an absent row) clamps
/// to the line's length; one inside a multi-byte character rounds down to that character's start,
/// the way every renderer in this crate does.
pub fn utf16_column(line: &str, byte_column: usize) -> usize {
    let end = floor_char_boundary(line, byte_column.min(line.len()));
    line[..end].encode_utf16().count()
}

fn convert_range(range: &TextRange, lines: &[&str]) -> [usize; 4] {
    let column = |row: usize, byte_column: usize| match lines.get(row) {
        Some(line) => utf16_column(line, byte_column),
        None => 0,
    };
    [
        range.start_row,
        column(range.start_row, range.start_column),
        range.end_row,
        column(range.end_row, range.end_column),
    ]
}

fn operation_name(operation: &TextOperation) -> &'static str {
    match operation {
        TextOperation::Insert => "insert",
        TextOperation::Delete => "delete",
        TextOperation::Update => "update",
        TextOperation::Move => "move",
        TextOperation::Identical => "identical",
        TextOperation::NotYetSet => "unset",
    }
}

/// One `RangeMatch`, both ranges as `[start_row, start_column, end_row, end_column]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RangePayload {
    pub op: &'static str,
    pub source: [usize; 4],
    pub destination: [usize; 4],
}

/// `[start, end, "#rrggbb"]` - a run of one foreground colour within a line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpanPayload(pub usize, pub usize, pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SidePayload {
    pub path: String,
    /// The basename, what the panel title shows.
    pub name: String,
    /// The detected language's name, or `Plain Text` - the TUI's `language_name`.
    pub language: String,
    pub lines: Vec<String>,
    /// Already filtered through `ranges_for_options`, so the page paints them as they are.
    pub ranges: Vec<RangePayload>,
    /// One entry per line.
    pub spans: Vec<Vec<SpanPayload>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CountsPayload {
    pub insertions: usize,
    pub deletions: usize,
    pub updates: usize,
    pub moves: usize,
}

impl From<ChangeCounts> for CountsPayload {
    fn from(counts: ChangeCounts) -> Self {
        Self {
            insertions: counts.insertions,
            deletions: counts.deletions,
            updates: counts.updates,
            moves: counts.moves,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SummaryPayload {
    /// The same snake_case tag `tui::json_output` uses for its `summary` field.
    pub kind: &'static str,
    pub label: &'static str,
}

fn summary_payload(summary: DiffSummary) -> SummaryPayload {
    let kind = match summary {
        DiffSummary::NoChanges => "no_changes",
        DiffSummary::NewFile => "new_file",
        DiffSummary::DeletedFile => "deleted_file",
        DiffSummary::WhitespaceOnly => "whitespace_only",
        DiffSummary::CommentOnly => "comment_only",
        DiffSummary::RefactorMovedOnly => "refactor_moved_only",
    };
    SummaryPayload {
        kind,
        label: summary.label(),
    }
}

/// The syntect theme a highlighting was done with, and its page colours: a light theme's code
/// is unreadable on a dark page, so the page takes its base colours from the theme it shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SyntaxPayload {
    pub theme: String,
    pub background: String,
    pub foreground: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiffPayload {
    pub before: SidePayload,
    pub after: SidePayload,
    pub summary: Option<SummaryPayload>,
    pub change_counts: CountsPayload,
    pub plain_text_fallback: bool,
    pub large_residual: bool,
    /// The options the ranges were filtered with, so the page's badge and `M` panel agree with
    /// what it is looking at.
    pub render_options: RenderOptions,
    pub syntax: SyntaxPayload,
}

/// A re-highlighting of the loaded pair in another syntect theme - the theme dialog's live
/// preview. Nothing but the spans changes, so nothing but the spans is sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HighlightPayload {
    pub before: Vec<Vec<SpanPayload>>,
    pub after: Vec<Vec<SpanPayload>>,
    pub syntax: SyntaxPayload,
}

fn hex(color: syntect::highlighting::Color) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

fn resolve_theme(name: Option<&str>) -> (String, &'static syntect::highlighting::Theme) {
    let themes = &theme_set().themes;
    let chosen = name
        .filter(|name| themes.contains_key(*name))
        .unwrap_or(DEFAULT_SYNTAX_THEME);
    let theme = themes
        .get(chosen)
        .expect("base16-ocean.dark is one of syntect's own bundled default themes");
    (chosen.to_string(), theme)
}

/// The page colours of a theme. syntect themes without an explicit background or foreground
/// (there are none among the bundled ones, but the fields are optional) fall back to the
/// conventional dark-on-light pair rather than to nothing.
pub fn syntax_payload(name: Option<&str>) -> SyntaxPayload {
    let (theme, resolved) = resolve_theme(name);
    SyntaxPayload {
        theme,
        background: resolved
            .settings
            .background
            .map(hex)
            .unwrap_or_else(|| "#1e1e1e".to_string()),
        foreground: resolved
            .settings
            .foreground
            .map(hex)
            .unwrap_or_else(|| "#d4d4d4".to_string()),
    }
}

/// Highlights `lines` the way `CodeViewerWidget::highlight_lines` does - same syntax lookup, same
/// theme fallback, foreground only - and returns one span list per line, in UTF-16 columns.
/// Adjacent runs of the same colour are merged, which shrinks the payload several-fold on real
/// code without changing what is painted. A language syntect has no definition for (or none at
/// all) yields empty span lists, and the page paints the theme's plain foreground.
pub fn highlight(
    lines: &[&str],
    language: Option<Language>,
    theme_name: Option<&str>,
) -> Vec<Vec<SpanPayload>> {
    let Some(syntax) = language
        .and_then(|language| language_to_syntect(&language))
        .and_then(|name| syntax_set().find_syntax_by_name(name))
    else {
        return vec![Vec::new(); lines.len()];
    };
    let (_, theme) = resolve_theme(theme_name);
    let mut highlighter = syntect::easy::HighlightLines::new(syntax, theme);
    lines
        .iter()
        .map(|&line| {
            let Ok(regions) = highlighter.highlight_line(line, syntax_set()) else {
                return Vec::new();
            };
            let mut spans: Vec<SpanPayload> = Vec::new();
            let mut byte = 0;
            for (style, text) in regions {
                let start = utf16_column(line, byte);
                byte += text.len();
                let end = utf16_column(line, byte);
                if start == end {
                    continue;
                }
                let color = hex(style.foreground);
                match spans.last_mut() {
                    Some(last) if last.1 == start && last.2 == color => last.1 = end,
                    _ => spans.push(SpanPayload(start, end, color)),
                }
            }
            spans
        })
        .collect()
}

/// The language a side is shown as. `/dev/null` (empty, no extension) takes the other side's,
/// exactly as `tui::app::substitute_missing_language` re-parses it - the diff was computed that
/// way, so the panel should say so too.
pub fn side_language(path: &Path, contents: &str, other: Option<Language>) -> Option<Language> {
    match language_for_path_and_content(path, contents) {
        None if contents.is_empty() => other,
        detected => detected,
    }
}

fn language_name(language: Option<Language>) -> String {
    language
        .map(|language| format!("{language:?}"))
        .unwrap_or_else(|| "Plain Text".to_string())
}

fn side_payload(
    path: &Path,
    contents: &str,
    full_ranges: &[RangeMatch],
    language: Option<Language>,
    options: RenderOptions,
    syntax_theme: Option<&str>,
) -> SidePayload {
    let lines: Vec<&str> = contents.lines().collect();
    let ranges = ranges_for_options(full_ranges, contents, options)
        .iter()
        .map(|range_match| RangePayload {
            op: operation_name(&range_match.operation),
            source: convert_range(&range_match.source, &lines),
            destination: convert_range(&range_match.destination, &lines),
        })
        .collect();
    SidePayload {
        path: path.display().to_string(),
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_string()),
        language: language_name(language),
        spans: highlight(&lines, language, syntax_theme),
        lines: lines.iter().map(|line| line.to_string()).collect(),
        ranges,
    }
}

/// Both sides' languages, each falling back to the other's for an empty `/dev/null` side.
pub fn pair_languages(data: &DiffSessionData) -> (Option<Language>, Option<Language>) {
    let before_detected = language_for_path_and_content(&data.before_path, &data.before_contents);
    let after_detected = language_for_path_and_content(&data.after_path, &data.after_contents);
    (
        side_language(&data.before_path, &data.before_contents, after_detected),
        side_language(&data.after_path, &data.after_contents, before_detected),
    )
}

/// The whole diff as the page wants it. `data` holds the *unfiltered* ranges, as
/// `DiffViewer::load_diff` keeps them; the summary and the change counts read those (as
/// `App::handle_diff_ready` does), while the ranges sent are filtered through `options`.
pub fn diff_payload(
    data: &DiffSessionData,
    large_residual: bool,
    options: RenderOptions,
    syntax_theme: Option<&str>,
) -> DiffPayload {
    let (before_language, after_language) = pair_languages(data);
    let summary = summarize_diff_with_comment_check(
        &data.before_contents,
        &data.after_contents,
        &data.before_ranges,
        &data.after_ranges,
        data.comment_only,
    )
    .map(summary_payload);
    let counts = change_counts(
        &data.before_contents,
        &data.after_contents,
        &data.before_ranges,
        &data.after_ranges,
    );
    DiffPayload {
        before: side_payload(
            &data.before_path,
            &data.before_contents,
            &data.before_ranges,
            before_language,
            options,
            syntax_theme,
        ),
        after: side_payload(
            &data.after_path,
            &data.after_contents,
            &data.after_ranges,
            after_language,
            options,
            syntax_theme,
        ),
        summary,
        change_counts: counts.into(),
        plain_text_fallback: data.plain_text_fallback,
        large_residual,
        render_options: options,
        syntax: syntax_payload(syntax_theme),
    }
}

/// Just the spans, for a syntax-theme preview.
pub fn highlight_payload(data: &DiffSessionData, syntax_theme: &str) -> HighlightPayload {
    let (before_language, after_language) = pair_languages(data);
    let before: Vec<&str> = data.before_contents.lines().collect();
    let after: Vec<&str> = data.after_contents.lines().collect();
    HighlightPayload {
        before: highlight(&before, before_language, Some(syntax_theme)),
        after: highlight(&after, after_language, Some(syntax_theme)),
        syntax: syntax_payload(Some(syntax_theme)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn utf16_columns_count_code_units_not_bytes() {
        // 'é' is two bytes, one code unit; '😀' is four bytes, two code units.
        let line = "aé😀b";
        assert_eq!(utf16_column(line, 0), 0);
        assert_eq!(utf16_column(line, 1), 1);
        assert_eq!(utf16_column(line, 3), 2, "after the two-byte é");
        assert_eq!(
            utf16_column(line, 7),
            4,
            "after the four-byte emoji: a surrogate pair"
        );
        assert_eq!(utf16_column(line, 8), 5);
        assert_eq!(utf16_column(line, 99), 5, "past the end clamps");
        assert_eq!(
            utf16_column(line, 2),
            1,
            "inside é rounds down to its start"
        );
    }

    #[test]
    fn ranges_convert_each_end_against_its_own_row() {
        let lines = ["é=1", "x"];
        let range = TextRange::new(0, 3, 1, 1);
        assert_eq!(convert_range(&range, &lines), [0, 2, 1, 1]);
        let to_eof = TextRange::new(0, 0, 2, 0);
        assert_eq!(
            convert_range(&to_eof, &lines),
            [0, 0, 2, 0],
            "a row past the end stays 0"
        );
    }

    #[test]
    fn highlighting_covers_a_rust_line_in_utf16_columns_with_merged_runs() {
        let spans = highlight(&["let é = 1;"], Some(Language::Rust), None);
        assert_eq!(spans.len(), 1);
        let line = &spans[0];
        assert!(
            !line.is_empty(),
            "rust has a syntect definition, so something is coloured"
        );
        assert_eq!(line[0].0, 0);
        assert_eq!(line.last().unwrap().1, "let é = 1;".encode_utf16().count());
        for pair in line.windows(2) {
            assert_eq!(pair[0].1, pair[1].0, "spans tile the line without gaps");
            assert_ne!(
                pair[0].2, pair[1].2,
                "adjacent same-colour runs were merged"
            );
        }
    }

    #[test]
    fn a_language_without_a_syntax_yields_empty_spans_not_an_error() {
        let spans = highlight(&["a", "b"], None, None);
        assert_eq!(spans, vec![Vec::new(), Vec::new()]);
    }

    #[test]
    fn an_unknown_syntax_theme_falls_back_to_syntects_default() {
        assert_eq!(
            syntax_payload(Some("no-such-theme")).theme,
            DEFAULT_SYNTAX_THEME
        );
        assert_eq!(
            syntax_payload(Some("Solarized (light)")).theme,
            "Solarized (light)"
        );
        let payload = syntax_payload(None);
        assert!(payload.background.starts_with('#') && payload.background.len() == 7);
    }

    #[test]
    fn a_dev_null_side_reports_the_other_sides_language() {
        let dev_null = PathBuf::from("/dev/null");
        assert_eq!(
            side_language(&dev_null, "", Some(Language::Rust)),
            Some(Language::Rust)
        );
        assert_eq!(
            side_language(&dev_null, "not empty", Some(Language::Rust)),
            None,
            "only an empty side borrows a language"
        );
    }

    fn sample_data() -> DiffSessionData {
        let before = "fn main() {\n    let x = 1;\n}\n".to_string();
        let after = "fn main() {\n    let y = 1;\n}\n".to_string();
        let before_ranges = vec![
            RangeMatch {
                source: TextRange::new(1, 8, 1, 9),
                destination: TextRange::new(1, 8, 1, 9),
                operation: TextOperation::Update,
            },
            RangeMatch {
                source: TextRange::new(0, 0, 0, 2),
                destination: TextRange::new(0, 0, 0, 2),
                operation: TextOperation::Identical,
            },
        ];
        let after_ranges = vec![RangeMatch {
            source: TextRange::new(1, 8, 1, 9),
            destination: TextRange::new(1, 8, 1, 9),
            operation: TextOperation::Update,
        }];
        DiffSessionData {
            before_path: PathBuf::from("a/before.rs"),
            after_path: PathBuf::from("b/after.rs"),
            before_contents: before,
            after_contents: after,
            before_ranges,
            after_ranges,
            comment_only: false,
            plain_text_fallback: false,
        }
    }

    #[test]
    fn the_diff_payload_carries_lines_ranges_counts_and_the_syntax_theme() {
        let payload = diff_payload(&sample_data(), false, RenderOptions::FULL, None);
        assert_eq!(payload.before.name, "before.rs");
        assert_eq!(payload.before.language, "Rust");
        assert_eq!(payload.before.lines.len(), 3);
        assert_eq!(payload.before.spans.len(), 3, "one span list per line");
        assert!(payload.before.ranges.iter().any(|r| r.op == "identical"));
        assert!(payload.before.ranges.iter().any(|r| r.op == "update"));
        assert_eq!(payload.change_counts.updates, 1);
        assert_eq!(payload.summary, None, "an ordinary edit has no summary");
        assert_eq!(payload.syntax.theme, DEFAULT_SYNTAX_THEME);
        assert_eq!(payload.render_options, RenderOptions::FULL);
    }

    #[test]
    fn identical_files_summarize_as_no_changes() {
        let mut data = sample_data();
        data.after_contents = data.before_contents.clone();
        data.before_ranges.clear();
        data.after_ranges.clear();
        let payload = diff_payload(&data, false, RenderOptions::FULL, None);
        let summary = payload
            .summary
            .expect("identical content is worth a summary");
        assert_eq!(summary.kind, "no_changes");
        assert_eq!(summary.label, DiffSummary::NoChanges.label());
    }

    #[test]
    fn the_highlight_payload_only_reswaps_spans() {
        let data = sample_data();
        let payload = highlight_payload(&data, "Solarized (light)");
        assert_eq!(payload.before.len(), 3);
        assert_eq!(payload.after.len(), 3);
        assert_eq!(payload.syntax.theme, "Solarized (light)");
    }

    #[test]
    fn serialized_spans_are_compact_triples() {
        let json = serde_json::to_string(&SpanPayload(1, 4, "#aabbcc".to_string())).unwrap();
        assert_eq!(json, r##"[1,4,"#aabbcc"]"##);
    }
}
