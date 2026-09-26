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
//! The viewer's first-frame state (`/api/state`), pinned to the defaults rather than read from a
//! config file, so every build of the showcase is the same page.

use serde::Serialize;
use strum::IntoEnumIterator;

use crate::diff::text::RenderOptions;
use crate::tui::components::diff_viewer::SINGLE_PANEL_THRESHOLD;
use crate::tui::components::help_modal::HELP_TEXT;
use crate::tui::display_columns::TAB_WIDTH;
use crate::tui::theme::{CustomPalette, OverlayTheme, PanelLayout};
use crate::tui::widgets::code_viewer::{DEFAULT_SYNTAX_THEME, syntax_theme_names};

/// One overlay theme for the page's picker, its palette already resolved to hex.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ThemePayload {
    /// serde's name for the variant.
    pub id: String,
    /// strum's label - what the picker shows.
    pub label: String,
    pub palette: CustomPalette,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SettingsPayload {
    pub theme: OverlayTheme,
    pub layout: PanelLayout,
    pub node_highlight: bool,
    pub render_options: RenderOptions,
    pub syntax_theme: Option<String>,
    pub custom_palette: CustomPalette,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderOptionRow {
    pub key: &'static str,
    pub label: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Presets {
    pub minimal: RenderOptions,
    pub full: RenderOptions,
}

/// Everything the page needs to draw its first frame. The field set is the viewer's contract
/// (`assets/viewer/app.js`), including the ones the showcase leaves empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatePayload {
    pub version: &'static str,
    pub before: Option<String>,
    pub after: Option<String>,
    pub settings: SettingsPayload,
    pub themes: Vec<ThemePayload>,
    pub syntax_themes: Vec<String>,
    /// The syntax theme until the user picks one, which the theme dialog opens on.
    pub default_syntax_theme: &'static str,
    pub render_option_rows: Vec<RenderOptionRow>,
    /// `RenderOptions::MINIMAL`/`FULL`, for the `M` panel's `1`/`2` keys and the footer badge -
    /// sent rather than duplicated in JavaScript, so a preset can only mean one thing.
    pub presets: Presets,
    pub recent_pairs: Vec<[String; 2]>,
    pub help_text: &'static str,
    pub config_error: Option<String>,
    /// `DiffViewer`'s dual/single cut-over, in character cells, so the page's auto layout flips
    /// at the same width the terminal's does.
    pub single_panel_threshold: u16,
    /// Columns between tab stops, so the page draws a tab as wide as the terminal does and
    /// scrolls to a cursor after one by the same count.
    pub tab_width: usize,
    /// Whether the page opens the git review picker as soon as it loads; never, here.
    pub review_on_start: bool,
}

/// The serde field names of `RenderOptions`, in declaration order, paired with the labels the
/// TUI's `M` panel shows. Pinned by a test against the struct's real serialization so a renamed
/// or reordered field cannot leave the page toggling the wrong row.
pub fn render_option_rows() -> Vec<RenderOptionRow> {
    const KEYS: [&str; 7] = [
        "leading_whitespace",
        "structural_punctuation",
        "whole_pair_updates",
        "paint_reindent_only_moves",
        "paint_displaced_moves",
        "paint_resized_moves",
        "whole_identifier_updates",
    ];
    KEYS.iter()
        .zip(RenderOptions::FULL.options())
        .map(|(key, (label, _))| RenderOptionRow { key, label })
        .collect()
}

/// The state with every setting at its default and no pair, recent pair or config error.
pub fn default_state() -> StatePayload {
    let theme = OverlayTheme::default();
    StatePayload {
        version: env!("CARGO_PKG_VERSION"),
        before: None,
        after: None,
        settings: SettingsPayload {
            theme,
            layout: PanelLayout::default(),
            node_highlight: false,
            render_options: RenderOptions::default(),
            syntax_theme: None,
            custom_palette: CustomPalette::from_palette(&theme.palette()),
        },
        themes: OverlayTheme::iter()
            .map(|theme| ThemePayload {
                id: serde_json::to_value(theme)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_string))
                    .unwrap_or_default(),
                label: theme.to_string(),
                palette: CustomPalette::from_palette(&theme.palette()),
            })
            .collect(),
        syntax_themes: syntax_theme_names(),
        default_syntax_theme: DEFAULT_SYNTAX_THEME,
        render_option_rows: render_option_rows(),
        presets: Presets {
            minimal: RenderOptions::MINIMAL,
            full: RenderOptions::FULL,
        },
        recent_pairs: Vec::new(),
        help_text: HELP_TEXT,
        config_error: None,
        single_panel_threshold: SINGLE_PANEL_THRESHOLD,
        tab_width: TAB_WIDTH,
        review_on_start: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_option_rows_match_the_structs_own_serialization_order() {
        // The JSON *text*, not a `Value`: serde_json's object map is sorted, and what the page
        // toggles by index is the derive's declaration order, which only the text preserves.
        let text = serde_json::to_string(&RenderOptions::FULL).unwrap();
        let serialized: Vec<&str> = text.split('"').skip(1).step_by(2).collect();
        let rows: Vec<&str> = render_option_rows().iter().map(|row| row.key).collect();
        assert_eq!(rows, serialized);
        assert!(render_option_rows()[1].label.starts_with("Structural"));
    }

    #[test]
    fn the_state_names_every_theme_with_a_round_trippable_id() {
        let state = default_state();
        assert_eq!(state.themes.len(), OverlayTheme::iter().count());
        for theme in &state.themes {
            let parsed: OverlayTheme =
                serde_json::from_value(serde_json::Value::String(theme.id.clone()))
                    .unwrap_or_else(|_| panic!("{} should parse back", theme.id));
            assert_eq!(parsed.to_string(), theme.label);
        }
        assert!(
            state
                .syntax_themes
                .iter()
                .any(|name| name == DEFAULT_SYNTAX_THEME)
        );
        assert_eq!(state.single_panel_threshold, SINGLE_PANEL_THRESHOLD);
        assert_eq!(state.tab_width, TAB_WIDTH);
        assert!(state.help_text.contains("Navigation"));
    }
}
