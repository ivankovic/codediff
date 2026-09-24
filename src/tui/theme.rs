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
use std::path::{Path, PathBuf};

use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter};

/// A named palette for the diff overlay and cursor highlight, picked with the `c` theme picker and
/// persisted, since no single palette reads well on both dark and light terminals. Variant order
/// is the picker's display order.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, EnumIter, Display)]
pub enum OverlayTheme {
    #[strum(to_string = "Dark")]
    Dark,
    #[strum(to_string = "Solarized Dark")]
    SolarizedDark,
    #[strum(to_string = "Solarized Light")]
    SolarizedLight,
    #[default]
    #[strum(to_string = "Dracula (default)")]
    Dracula,
    #[strum(to_string = "Nord")]
    Nord,
    #[strum(to_string = "Gruvbox Dark")]
    GruvboxDark,
    #[strum(to_string = "Monokai")]
    Monokai,
    #[strum(to_string = "One Dark")]
    OneDark,
    /// The user's own palette, persisted as [`CustomPalette`]. Unlike the presets its colors come
    /// from process-global state (`custom_palette()`), so `palette()` stays the single resolution
    /// point instead of a palette being threaded through every renderer.
    #[strum(to_string = "Custom")]
    Custom,
}

/// A user-edited palette, stored as `#rrggbb` strings so the config file is hand-editable. An
/// unparseable field falls back to that Dracula color, so a typo costs one color, not the theme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomPalette {
    pub insert_bg: String,
    pub delete_bg: String,
    pub move_bg: String,
    pub update_bg: String,
    pub overlay_fg: String,
    pub cross_highlight_bg: String,
    pub search_bg: String,
    pub before_title_fg: String,
    pub after_title_fg: String,
}

impl Default for CustomPalette {
    /// Dracula, so switching to Custom starts from the default theme's colors.
    fn default() -> Self {
        Self::from_palette(&OverlayTheme::Dracula.palette())
    }
}

impl CustomPalette {
    /// Snapshot an existing palette as editable hex (how editing a preset forks it to Custom).
    pub fn from_palette(palette: &OverlayPalette) -> Self {
        Self {
            insert_bg: format_hex_color(palette.insert_bg),
            delete_bg: format_hex_color(palette.delete_bg),
            move_bg: format_hex_color(palette.move_bg),
            update_bg: format_hex_color(palette.update_bg),
            overlay_fg: format_hex_color(palette.overlay_fg),
            cross_highlight_bg: format_hex_color(palette.cross_highlight_bg),
            search_bg: format_hex_color(palette.search_bg),
            before_title_fg: format_hex_color(palette.before_title_fg),
            after_title_fg: format_hex_color(palette.after_title_fg),
        }
    }

    /// Resolve to concrete colors, falling back per field.
    pub fn to_palette(&self) -> OverlayPalette {
        let fallback = OverlayTheme::Dracula.palette();
        let at = |hex: &str, default: Color| parse_hex_color(hex).unwrap_or(default);
        OverlayPalette {
            insert_bg: at(&self.insert_bg, fallback.insert_bg),
            delete_bg: at(&self.delete_bg, fallback.delete_bg),
            move_bg: at(&self.move_bg, fallback.move_bg),
            update_bg: at(&self.update_bg, fallback.update_bg),
            overlay_fg: at(&self.overlay_fg, fallback.overlay_fg),
            cross_highlight_bg: at(&self.cross_highlight_bg, fallback.cross_highlight_bg),
            search_bg: at(&self.search_bg, fallback.search_bg),
            before_title_fg: at(&self.before_title_fg, fallback.before_title_fg),
            after_title_fg: at(&self.after_title_fg, fallback.after_title_fg),
        }
    }
}

/// `#rrggbb` or bare `rrggbb` to a `Color`; `None` for anything else.
pub fn parse_hex_color(text: &str) -> Option<Color> {
    let hex = text.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |range: std::ops::Range<usize>| u8::from_str_radix(&hex[range], 16).ok();
    Some(Color::Rgb(channel(0..2)?, channel(2..4)?, channel(4..6)?))
}

/// A `Color` as `#rrggbb`. The named ANSI colors have no true RGB value, so they are shown at
/// their conventional xterm values; editing one therefore produces a concrete `Color::Rgb`.
pub fn format_hex_color(color: Color) -> String {
    let (r, g, b) = match color {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Black => (0, 0, 0),
        Color::Red => (205, 0, 0),
        Color::Green => (0, 205, 0),
        Color::Yellow => (205, 205, 0),
        Color::Blue => (0, 0, 238),
        Color::Magenta => (205, 0, 205),
        Color::Cyan => (0, 205, 205),
        Color::Gray => (229, 229, 229),
        Color::DarkGray => (127, 127, 127),
        Color::LightRed => (255, 0, 0),
        Color::LightGreen => (0, 255, 0),
        Color::LightYellow => (255, 255, 0),
        Color::LightBlue => (92, 92, 255),
        Color::LightMagenta => (255, 0, 255),
        Color::LightCyan => (0, 255, 255),
        Color::White => (255, 255, 255),
        _ => (0, 0, 0),
    };
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// See [`OverlayTheme::Custom`] for why this is global.
static CUSTOM_PALETTE: std::sync::RwLock<Option<CustomPalette>> = std::sync::RwLock::new(None);

/// The current custom palette, defaulting to Dracula's colors if none has been loaded or saved.
pub fn custom_palette() -> CustomPalette {
    CUSTOM_PALETTE
        .read()
        .ok()
        .and_then(|guard| guard.clone())
        .unwrap_or_default()
}

/// Replace the in-memory custom palette without persisting it (see `save_custom_palette`).
pub fn set_custom_palette(palette: CustomPalette) {
    if let Ok(mut guard) = CUSTOM_PALETTE.write() {
        *guard = Some(palette);
    }
}

/// The concrete colors making up one [`OverlayTheme`].
pub struct OverlayPalette {
    pub insert_bg: Color,
    pub delete_bg: Color,
    pub move_bg: Color,
    pub update_bg: Color,
    pub overlay_fg: Color,
    pub cross_highlight_bg: Color,
    /// Search-match highlight. Distinct from `cross_highlight_bg`, or a search hit and the
    /// cursor's counterpart would be indistinguishable. Every preset uses an orange, the one hue
    /// no band or cursor highlight occupies.
    pub search_bg: Color,
    /// Panel title colors. Every preset uses the `PRESET_*_TITLE_FG` pair; only Custom varies them.
    pub before_title_fg: Color,
    pub after_title_fg: Color,
}

pub const PRESET_BEFORE_TITLE_FG: Color = Color::Red;
pub const PRESET_AFTER_TITLE_FG: Color = Color::Green;

impl OverlayPalette {
    /// The background that paints `operation`, or `None` for `Identical`/`NotYetSet`. The one
    /// operation-to-colour table for every ratatui renderer.
    pub fn background_for(&self, operation: &crate::diff::text::TextOperation) -> Option<Color> {
        use crate::diff::text::TextOperation;
        match operation {
            TextOperation::Insert => Some(self.insert_bg),
            TextOperation::Delete => Some(self.delete_bg),
            TextOperation::Move => Some(self.move_bg),
            TextOperation::Update => Some(self.update_bg),
            TextOperation::Identical | TextOperation::NotYetSet => None,
        }
    }
}

impl OverlayTheme {
    /// The colors for this theme. Each band is the theme's canonical accent blended 60% toward
    /// its canonical background; `cross_highlight_bg` and `search_bg` are blended only 40% so they
    /// stay more vivid than the bands; `overlay_fg` is the theme's foreground, unblended. The
    /// Solarized variants were hand-blended by the same recipe.
    pub fn palette(self) -> OverlayPalette {
        match self {
            OverlayTheme::Custom => custom_palette().to_palette(),
            OverlayTheme::Dark => OverlayPalette {
                insert_bg: Color::Rgb(20, 60, 20),
                delete_bg: Color::Rgb(70, 20, 20),
                move_bg: Color::Rgb(47, 47, 47), // grey at the purple's own weight
                update_bg: Color::Rgb(70, 60, 10),
                overlay_fg: Color::Rgb(225, 225, 225),
                cross_highlight_bg: Color::Rgb(40, 90, 200),
                search_bg: Color::Rgb(160, 90, 10),
                before_title_fg: PRESET_BEFORE_TITLE_FG,
                after_title_fg: PRESET_AFTER_TITLE_FG,
            },
            OverlayTheme::SolarizedDark => OverlayPalette {
                insert_bg: Color::Rgb(53, 87, 32),
                delete_bg: Color::Rgb(88, 46, 51),
                move_bg: Color::Rgb(72, 72, 72), // grey at the purple's own weight
                update_bg: Color::Rgb(72, 81, 32),
                overlay_fg: Color::Rgb(238, 232, 213),
                cross_highlight_bg: Color::Rgb(23, 101, 148),
                // Solarized orange blended 0.4 toward base03, same vividness as the cursor blue.
                search_bg: Color::Rgb(122, 62, 35),
                before_title_fg: PRESET_BEFORE_TITLE_FG,
                after_title_fg: PRESET_AFTER_TITLE_FG,
            },
            OverlayTheme::SolarizedLight => OverlayPalette {
                insert_bg: Color::Rgb(205, 209, 136),
                delete_bg: Color::Rgb(240, 168, 155),
                move_bg: Color::Rgb(198, 198, 198), // grey at the pink's own weight
                update_bg: Color::Rgb(224, 202, 136),
                overlay_fg: Color::Rgb(7, 54, 66),
                cross_highlight_bg: Color::Rgb(124, 182, 217),
                // Solarized orange blended 0.4 toward base3, same vividness as the cursor blue.
                search_bg: Color::Rgb(223, 143, 104),
                before_title_fg: PRESET_BEFORE_TITLE_FG,
                after_title_fg: PRESET_AFTER_TITLE_FG,
            },
            OverlayTheme::Dracula => {
                // https://draculatheme.com/spec
                let bg = (40, 42, 54);
                OverlayPalette {
                    insert_bg: blend_toward_base((80, 250, 123), bg, 0.6), // green
                    delete_bg: blend_toward_base((255, 85, 85), bg, 0.6),  // red
                    // Moves are grey in every preset rather than an accent: a move changes no
                    // code, and a loud hue pulls the eye to what needs the least attention.
                    move_bg: blend_toward_base((170, 170, 170), bg, 0.6), // grey, #aaaaaa
                    update_bg: blend_toward_base((241, 250, 140), bg, 0.6), // yellow
                    overlay_fg: Color::Rgb(248, 248, 242),                // foreground
                    cross_highlight_bg: blend_toward_base((139, 233, 253), bg, 0.4), // cyan
                    search_bg: blend_toward_base((255, 184, 108), bg, 0.4), // orange
                    before_title_fg: PRESET_BEFORE_TITLE_FG,
                    after_title_fg: PRESET_AFTER_TITLE_FG,
                }
            }
            OverlayTheme::Nord => {
                // https://www.nordtheme.com/docs/colors-and-palettes - nord0 (bg), nord6
                // (brightest snow storm, fg), nord11/13/14/15 (aurora accents), nord9 (frost blue)
                let bg = (46, 52, 64);
                OverlayPalette {
                    insert_bg: blend_toward_base((163, 190, 140), bg, 0.6), // nord14, green
                    delete_bg: blend_toward_base((191, 97, 106), bg, 0.6),  // nord11, red
                    move_bg: blend_toward_base((170, 170, 170), bg, 0.6),   // grey, #aaaaaa
                    update_bg: blend_toward_base((235, 203, 139), bg, 0.6), // nord13, yellow
                    overlay_fg: Color::Rgb(236, 239, 244),                  // nord6
                    cross_highlight_bg: blend_toward_base((129, 161, 193), bg, 0.4), // nord9
                    search_bg: blend_toward_base((208, 135, 112), bg, 0.4), // nord12, orange
                    before_title_fg: PRESET_BEFORE_TITLE_FG,
                    after_title_fg: PRESET_AFTER_TITLE_FG,
                }
            }
            OverlayTheme::GruvboxDark => {
                // https://github.com/morhetz/gruvbox - bg0, fg1, and the "bright" accent row
                let bg = (40, 40, 40);
                OverlayPalette {
                    insert_bg: blend_toward_base((184, 187, 38), bg, 0.6), // bright green
                    delete_bg: blend_toward_base((251, 73, 52), bg, 0.6),  // bright red
                    move_bg: blend_toward_base((170, 170, 170), bg, 0.6),  // grey, #aaaaaa
                    update_bg: blend_toward_base((250, 189, 47), bg, 0.6), // bright yellow
                    overlay_fg: Color::Rgb(235, 219, 178),                 // fg1
                    cross_highlight_bg: blend_toward_base((131, 165, 152), bg, 0.4), // bright blue
                    search_bg: blend_toward_base((254, 128, 25), bg, 0.4), // bright orange
                    before_title_fg: PRESET_BEFORE_TITLE_FG,
                    after_title_fg: PRESET_AFTER_TITLE_FG,
                }
            }
            OverlayTheme::Monokai => {
                // Canonical Sublime Text "Monokai" (monokai.tmTheme) accents and background.
                let bg = (39, 40, 34);
                OverlayPalette {
                    insert_bg: blend_toward_base((166, 226, 46), bg, 0.6), // green
                    delete_bg: blend_toward_base((249, 38, 114), bg, 0.6), // pink/red
                    move_bg: blend_toward_base((170, 170, 170), bg, 0.6),  // grey, #aaaaaa
                    update_bg: blend_toward_base((230, 219, 116), bg, 0.6), // yellow
                    overlay_fg: Color::Rgb(248, 248, 242),                 // foreground
                    cross_highlight_bg: blend_toward_base((102, 217, 239), bg, 0.4), // cyan
                    search_bg: blend_toward_base((253, 151, 31), bg, 0.4), // orange
                    before_title_fg: PRESET_BEFORE_TITLE_FG,
                    after_title_fg: PRESET_AFTER_TITLE_FG,
                }
            }
            OverlayTheme::OneDark => {
                // Atom's "One Dark" (atom-one-dark-syntax) accents and background - one of the
                // most widely ported editor themes, independent of the Atom editor itself.
                let bg = (40, 44, 52);
                OverlayPalette {
                    insert_bg: blend_toward_base((152, 195, 121), bg, 0.6), // green
                    delete_bg: blend_toward_base((224, 108, 117), bg, 0.6), // red
                    move_bg: blend_toward_base((170, 170, 170), bg, 0.6),   // grey, #aaaaaa
                    update_bg: blend_toward_base((229, 192, 123), bg, 0.6), // yellow
                    overlay_fg: Color::Rgb(171, 178, 191),                  // foreground
                    cross_highlight_bg: blend_toward_base((97, 175, 239), bg, 0.4), // blue
                    search_bg: blend_toward_base((209, 154, 102), bg, 0.4), // orange
                    before_title_fg: PRESET_BEFORE_TITLE_FG,
                    after_title_fg: PRESET_AFTER_TITLE_FG,
                }
            }
        }
    }
}

/// Blends `accent` toward `base` by `base_weight` (`0.0` = pure accent, `1.0` = pure base).
fn blend_toward_base(accent: (u8, u8, u8), base: (u8, u8, u8), base_weight: f32) -> Color {
    let mix = |a: u8, b: u8| -> u8 {
        (a as f32 * (1.0 - base_weight) + b as f32 * base_weight).round() as u8
    };
    Color::Rgb(
        mix(accent.0, base.0),
        mix(accent.1, base.1),
        mix(accent.2, base.2),
    )
}

/// How the before/after panels are laid out. Lives here because this module owns the config file.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PanelLayout {
    /// Pick dual/single from the terminal width (`SINGLE_PANEL_THRESHOLD`).
    #[default]
    Auto,
    /// Always side-by-side, regardless of width.
    Dual,
    /// Always one panel at a time (`Tab` switches), regardless of width.
    Single,
}

impl PanelLayout {
    /// The next mode in the `v` key's `Auto -> Dual -> Single -> Auto` cycle.
    pub fn next(self) -> Self {
        match self {
            PanelLayout::Auto => PanelLayout::Dual,
            PanelLayout::Dual => PanelLayout::Single,
            PanelLayout::Single => PanelLayout::Auto,
        }
    }

    /// Short label for the footer/title, e.g. `[layout: dual]`.
    pub fn label(self) -> &'static str {
        match self {
            PanelLayout::Auto => "auto",
            PanelLayout::Dual => "dual",
            PanelLayout::Single => "single",
        }
    }
}

/// One per digit key on the empty-start screen.
const MAX_RECENT_PAIRS: usize = 9;

/// The persisted settings. Every field is `#[serde(default)]` so an older config still parses.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
struct ThemeConfig {
    /// `#[serde(default)]` covers only a *missing* field; an unknown value (newer build, typo)
    /// would still fail the whole document, so `theme_or_default` absorbs it.
    #[serde(default, deserialize_with = "theme_or_default")]
    theme: OverlayTheme,
    #[serde(default)]
    layout: PanelLayout,
    #[serde(default)]
    recent_pairs: Vec<(PathBuf, PathBuf)>,
    /// Kept while a preset is selected, so switching back to Custom restores the edits.
    #[serde(default)]
    custom_palette: CustomPalette,
    /// One of syntect's built-ins; empty means the code viewer's default.
    #[serde(default)]
    syntax_theme: String,
    /// The `H` key; off by default (see `load_node_highlight`).
    #[serde(default)]
    node_highlight: bool,
    /// The `M` key; defaults to `RenderOptions::FULL`.
    #[serde(default)]
    render_options: crate::diff::text::RenderOptions,
}

/// Deserialize a theme name, falling back to the default for anything this build does not know.
///
/// Re-runs the enum's own `Deserialize` rather than comparing against `Display`: serde writes the
/// variant identifier (`SolarizedLight`), strum's `Display` the picker label (`Solarized Light`).
/// A value that is not a string still fails, and [`update_config`] then refuses to overwrite it.
fn theme_or_default<'de, D>(deserializer: D) -> Result<OverlayTheme, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::IntoDeserializer;
    let name = String::deserialize(deserializer)?;
    let as_value: serde::de::value::StrDeserializer<serde::de::value::Error> =
        name.as_str().into_deserializer();
    Ok(OverlayTheme::deserialize(as_value).unwrap_or_default())
}

/// The environment variable that overrides every other config layer.
pub const CONFIG_ENV: &str = "CODEDIFF_CONFIG";

/// The project-level config file's name, looked for at or above the current directory.
const PROJECT_CONFIG: &str = ".codediff.toml";

/// The config file to read and write, resolved in this order:
///
/// 1. `$CODEDIFF_CONFIG`, if set and non-empty. Authoritative: no walk-up, no fallback.
/// 2. The nearest `.codediff.toml` at or above the current directory, **if one already exists**.
///    It is never created, so codediff does not litter the directories it runs in. The walk-up
///    matters because git runs difftools from the repository root.
/// 3. `$XDG_CONFIG_HOME/codediff/config.toml`, else `$HOME/.config/codediff/config.toml`.
pub(crate) fn config_path() -> PathBuf {
    if let Ok(explicit) = std::env::var(CONFIG_ENV)
        && !explicit.is_empty()
    {
        return PathBuf::from(explicit);
    }
    #[cfg(test)]
    {
        test_config_path()
    }
    #[cfg(not(test))]
    {
        nearest_project_config().unwrap_or_else(user_config_path)
    }
}

/// A throwaway config for the test build. Many tests reach the persisting setters incidentally,
/// so redirecting here is the only way no test writes a developer's real settings. Done in code
/// because nextest ignores an `[env]` table and a Makefile variable misses a bare `cargo test`;
/// keyed by process id so threaded `cargo test` runs do not share a file.
#[cfg(test)]
fn test_config_path() -> PathBuf {
    std::env::temp_dir().join(format!("codediff-test-config-{}.toml", std::process::id()))
}

/// The nearest existing `.codediff.toml` at or above the current directory. Unreachable in the
/// test build; the walk is tested through `nearest_project_config_from`.
#[cfg_attr(test, allow(dead_code))]
fn nearest_project_config() -> Option<PathBuf> {
    nearest_project_config_from(&std::env::current_dir().ok()?)
}

fn nearest_project_config_from(start: &Path) -> Option<PathBuf> {
    start.ancestors().find_map(|directory| {
        let candidate = directory.join(PROJECT_CONFIG);
        candidate.is_file().then_some(candidate)
    })
}

/// Resolved from the environment rather than a `dirs` crate: a new dependency costs every Gentoo
/// ebuild bump a regenerated `CRATES=` block. With neither variable set it falls back to
/// `./.codediff.toml` rather than a path under the filesystem root.
#[cfg_attr(test, allow(dead_code))]
fn user_config_path() -> PathBuf {
    user_config_path_from(
        std::env::var("XDG_CONFIG_HOME").ok(),
        std::env::var("HOME").ok(),
    )
}

fn user_config_path_from(xdg_config_home: Option<String>, home: Option<String>) -> PathBuf {
    if let Some(xdg) = xdg_config_home.filter(|value| !value.is_empty()) {
        return PathBuf::from(xdg).join("codediff").join("config.toml");
    }
    if let Some(home) = home.filter(|value| !value.is_empty()) {
        return PathBuf::from(home)
            .join(".config")
            .join("codediff")
            .join("config.toml");
    }
    PathBuf::from(PROJECT_CONFIG)
}

/// The last config parse failure, for the TUI to surface; `None` once a load succeeds. Without it
/// a broken file silently reads as defaults.
static CONFIG_ERROR: std::sync::RwLock<Option<String>> = std::sync::RwLock::new(None);

/// The current config parse error, if the last load hit one.
pub fn config_error() -> Option<String> {
    CONFIG_ERROR.read().ok().and_then(|held| held.clone())
}

/// The file exists but does not parse, so it must not be overwritten (see [`update_config`]).
#[derive(Debug)]
struct Unreadable;

/// Read the config. A missing file yields defaults; a present but unparseable one is recorded in
/// [`CONFIG_ERROR`] and returned as `Unreadable`, so nobody writes defaults back over it.
fn read_config(path: &Path) -> Result<ThemeConfig, Unreadable> {
    match confy::load_path::<ThemeConfig>(path) {
        Ok(config) => {
            if let Ok(mut held) = CONFIG_ERROR.write() {
                *held = None;
            }
            Ok(config)
        }
        Err(error) => {
            if !path.exists() {
                return Ok(ThemeConfig::default());
            }
            if let Ok(mut held) = CONFIG_ERROR.write() {
                *held = Some(format!("{}: {error}", path.display()));
            }
            Err(Unreadable)
        }
    }
}

/// Read-modify-write one setting. Refuses to write when the existing file does not parse, which
/// would replace every other setting with a default. Resolves the layered path once, so the read
/// and the write hit the same file.
fn update_config(mutate: impl FnOnce(&mut ThemeConfig)) {
    let path = config_path();
    let Ok(mut config) = read_config(&path) else {
        return;
    };
    mutate(&mut config);
    save_to(path, config);
}

/// The persisted theme, or the default if the file is missing or broken. `confy` rather than
/// `config-rs`, because `config-rs` cannot write a choice back.
pub fn load_overlay_theme() -> OverlayTheme {
    load_from(config_path()).theme
}

/// Persist the theme, keeping the other settings. Write failures are ignored: the choice just
/// does not survive a restart. The same holds for every `save_*` below.
pub fn save_overlay_theme(theme: OverlayTheme) {
    update_config(|config| config.theme = theme);
}

/// The persisted panel layout (the `v` key).
pub fn load_panel_layout() -> PanelLayout {
    load_from(config_path()).layout
}

pub fn save_panel_layout(layout: PanelLayout) {
    update_config(|config| config.layout = layout);
}

/// The persisted custom palette, or Dracula's colors if none was ever saved.
pub fn load_custom_palette() -> CustomPalette {
    load_from(config_path()).custom_palette
}

/// Persist the custom palette *and* install it as the live one, so the caller cannot save a
/// palette the running process isn't using.
pub fn save_custom_palette(palette: CustomPalette) {
    set_custom_palette(palette.clone());
    update_config(|config| config.custom_palette = palette);
}

/// The persisted render options (the `M` key), or `RenderOptions::FULL` if none was ever chosen.
pub fn load_render_options() -> crate::diff::text::RenderOptions {
    load_from(config_path()).render_options
}

pub fn save_render_options(options: crate::diff::text::RenderOptions) {
    update_config(|config| config.render_options = options);
}

/// The persisted syntax-highlighting theme name, or `None` if the user never picked one.
pub fn load_syntax_theme() -> Option<String> {
    let name = load_from(config_path()).syntax_theme;
    (!name.is_empty()).then_some(name)
}

/// Persist the syntax-highlighting theme choice.
pub fn save_syntax_theme(name: &str) {
    update_config(|config| config.syntax_theme = name.to_string());
}

/// Whether the node highlight (the `H` key) is on. Off by default: it repaints on every cursor
/// move, which reads as flicker and hides the diff colors; it answers an occasional question.
pub fn load_node_highlight() -> bool {
    load_from(config_path()).node_highlight
}

pub fn save_node_highlight(enabled: bool) {
    update_config(|config| config.node_highlight = enabled);
}

/// A temp file a VCS materializes for a diff tool (`/tmp/git-blob-*`, jj's equivalent), deleted
/// as soon as the tool exits.
fn is_throwaway(path: &Path) -> bool {
    path.starts_with(std::env::temp_dir())
}

/// The recently diffed pairs, most recent first. Pairs whose files are gone are dropped, since
/// selecting one would fail.
pub fn load_recent_pairs() -> Vec<(PathBuf, PathBuf)> {
    let mut pairs = load_from(config_path()).recent_pairs;
    pairs.retain(|(before, after)| before.exists() && after.exists());
    pairs
}

/// Record a diffed pair at the front of the recent list (deduplicated, capped at
/// [`MAX_RECENT_PAIRS`]). A pair with a throwaway side is never recorded; filtering only on read
/// would still write one per `git difftool` run.
pub fn record_recent_pair(before: &Path, after: &Path) {
    if is_throwaway(before) || is_throwaway(after) {
        return;
    }
    let pair = (before.to_path_buf(), after.to_path_buf());
    update_config(|config| {
        config.recent_pairs.retain(|existing| existing != &pair);
        config.recent_pairs.insert(0, pair);
        config.recent_pairs.truncate(MAX_RECENT_PAIRS);
    });
}

/// Falls back to defaults, which is right for a read; writes go through [`update_config`].
fn load_from(path: PathBuf) -> ThemeConfig {
    read_config(&path).unwrap_or_default()
}

fn save_to(path: PathBuf, config: ThemeConfig) {
    let _ = confy::store_path(path, config);
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    #[test]
    fn an_unknown_theme_name_does_not_discard_the_rest_of_the_config() {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        std::fs::write(
            file.path(),
            "theme = \"NotATheme\"\nsyntax_theme = \"base16-ocean.dark\"\nnode_highlight = true\n",
        )
        .expect("write config");

        let config = load_from(file.path().to_path_buf());

        assert_eq!(config.theme, OverlayTheme::default());
        assert_eq!(config.syntax_theme, "base16-ocean.dark");
        assert!(config.node_highlight);
    }

    /// Reading with `unwrap_or_default()` and writing the result back would destroy the file.
    #[test]
    fn a_file_that_does_not_parse_is_never_overwritten() {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        let garbage = "this is not toml = = =\n";
        std::fs::write(file.path(), garbage).expect("write config");

        let path = file.path().to_path_buf();
        assert!(read_config(&path).is_err());
        assert!(config_error().is_some(), "the failure must be reportable");

        // What `update_config` does.
        if let Ok(mut config) = read_config(&path) {
            config.node_highlight = true;
            save_to(path.clone(), config);
        }

        assert_eq!(
            std::fs::read_to_string(file.path()).expect("read back"),
            garbage,
            "an unparseable config must be left exactly as the user wrote it"
        );
    }

    /// Tests rely on this to stay off a real user's settings.
    #[test]
    fn the_environment_override_wins_over_every_other_layer() {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        unsafe { std::env::set_var(CONFIG_ENV, file.path()) };
        assert_eq!(config_path(), file.path());
        unsafe { std::env::remove_var(CONFIG_ENV) };
    }

    #[test]
    fn recent_pairs_drops_entries_whose_files_are_gone() {
        let alive = tempfile::NamedTempFile::new().expect("temp file");
        let file = tempfile::NamedTempFile::new().expect("temp config");
        save_to(
            file.path().to_path_buf(),
            ThemeConfig {
                recent_pairs: vec![
                    (alive.path().to_path_buf(), alive.path().to_path_buf()),
                    (
                        PathBuf::from("/tmp/git-blob-deleted/before.rs"),
                        PathBuf::from("/tmp/git-blob-deleted/after.rs"),
                    ),
                ],
                ..Default::default()
            },
        );

        unsafe { std::env::set_var(CONFIG_ENV, file.path()) };
        let pairs = load_recent_pairs();
        unsafe { std::env::remove_var(CONFIG_ENV) };

        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, alive.path());
    }

    #[test]
    fn a_throwaway_vcs_path_is_recognised() {
        let temp = std::env::temp_dir().join("git-blob-AbC123").join("main.rs");
        assert!(is_throwaway(&temp));
        assert!(!is_throwaway(Path::new(
            "/home/someone/src/project/main.rs"
        )));
    }

    #[test]
    fn a_project_config_is_found_from_a_subdirectory() {
        let root = tempfile::tempdir().expect("temp dir");
        let nested = root.path().join("src").join("tui");
        std::fs::create_dir_all(&nested).expect("create dirs");
        let config = root.path().join(PROJECT_CONFIG);
        std::fs::write(&config, "node_highlight = true\n").expect("write config");

        assert_eq!(nearest_project_config_from(&nested), Some(config.clone()));
        assert_eq!(nearest_project_config_from(root.path()), Some(config));
    }

    #[test]
    fn the_nearest_project_config_wins() {
        let root = tempfile::tempdir().expect("temp dir");
        let inner = root.path().join("inner");
        std::fs::create_dir_all(&inner).expect("create dirs");
        std::fs::write(root.path().join(PROJECT_CONFIG), "").expect("outer");
        std::fs::write(inner.join(PROJECT_CONFIG), "").expect("inner");

        assert_eq!(
            nearest_project_config_from(&inner),
            Some(inner.join(PROJECT_CONFIG))
        );
    }

    #[test]
    fn no_project_config_means_none_is_created() {
        let root = tempfile::tempdir().expect("temp dir");
        let nested = root.path().join("a").join("b");
        std::fs::create_dir_all(&nested).expect("create dirs");

        // `/tmp` itself could in principle hold one, so only assert about the temp tree.
        let found = nearest_project_config_from(&nested);
        assert!(
            found.is_none_or(|path| !path.starts_with(root.path())),
            "nothing under the temp root should have been found or created"
        );
        assert!(!nested.join(PROJECT_CONFIG).exists());
    }

    /// `~/.config/codediff/` does not exist on a fresh machine, and the user config is the
    /// default destination.
    #[test]
    fn saving_creates_the_directories_the_user_config_lives_in() {
        let home = tempfile::tempdir().expect("temp dir");
        let path = home
            .path()
            .join(".config")
            .join("codediff")
            .join("config.toml");
        assert!(!path.exists());

        save_to(
            path.clone(),
            ThemeConfig {
                node_highlight: true,
                ..Default::default()
            },
        );

        assert!(
            path.is_file(),
            "config was not written to {}",
            path.display()
        );
        assert!(load_from(path).node_highlight);
    }

    #[test]
    fn the_user_config_path_follows_xdg_then_home() {
        assert_eq!(
            user_config_path_from(Some("/x/config".into()), Some("/home/me".into())),
            PathBuf::from("/x/config/codediff/config.toml"),
            "XDG_CONFIG_HOME wins when it is set"
        );
        assert_eq!(
            user_config_path_from(None, Some("/home/me".into())),
            PathBuf::from("/home/me/.config/codediff/config.toml")
        );
        // An empty variable is not a choice.
        assert_eq!(
            user_config_path_from(Some(String::new()), Some("/home/me".into())),
            PathBuf::from("/home/me/.config/codediff/config.toml")
        );
        // No HOME at all: a relative path rather than one under `/`.
        assert_eq!(
            user_config_path_from(None, None),
            PathBuf::from(PROJECT_CONFIG)
        );
    }

    #[test]
    fn save_then_load_round_trips_the_chosen_theme() {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        save_to(
            file.path().to_path_buf(),
            ThemeConfig {
                theme: OverlayTheme::SolarizedLight,
                ..Default::default()
            },
        );
        assert_eq!(
            load_from(file.path().to_path_buf()).theme,
            OverlayTheme::SolarizedLight
        );
    }

    /// A `[render_options]` table missing a newer key must still load; otherwise the whole file
    /// fails and every setting silently resets.
    #[test]
    fn a_pre_existing_render_options_table_without_whole_pair_updates_still_loads() {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        // A real table missing only the newer keys, not a file missing the table.
        std::fs::write(
            file.path(),
            "theme = \"SolarizedLight\"\n\n[render_options]\nleading_whitespace = true\nstructural_punctuation = true\n",
        )
        .expect("write legacy config");

        let loaded = load_from(file.path().to_path_buf());

        assert_eq!(
            loaded.theme,
            OverlayTheme::SolarizedLight,
            "the rest of the config must survive, not silently reset"
        );
        assert!(loaded.render_options.leading_whitespace);
        assert!(loaded.render_options.structural_punctuation);
        assert!(
            !loaded.render_options.whole_pair_updates,
            "the missing key must default to false (narrow), not fail the whole file"
        );
        assert!(
            loaded.render_options.paint_reindent_only_moves,
            "the missing key must default to true: older configs always painted a reindented Move"
        );
    }

    #[test]
    fn node_highlight_round_trips_and_defaults_to_off_for_an_older_config() {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        save_to(
            file.path().to_path_buf(),
            ThemeConfig {
                node_highlight: true,
                ..Default::default()
            },
        );
        assert!(load_from(file.path().to_path_buf()).node_highlight);

        // A config written before this setting existed.
        std::fs::write(file.path(), "theme = \"Default\"\n").expect("write legacy config");
        assert!(
            !load_from(file.path().to_path_buf()).node_highlight,
            "a config with no node_highlight key must load as off"
        );
    }

    #[test]
    fn load_from_a_missing_file_falls_back_to_default_without_erroring() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("does-not-exist.toml");
        assert_eq!(load_from(path), ThemeConfig::default());
    }

    #[test]
    fn blend_toward_base_interpolates_correctly() {
        let accent = (200, 100, 0);
        let base = (0, 100, 200);

        assert_eq!(
            blend_toward_base(accent, base, 0.0),
            Color::Rgb(200, 100, 0)
        );
        assert_eq!(
            blend_toward_base(accent, base, 1.0),
            Color::Rgb(0, 100, 200)
        );
        assert_eq!(
            blend_toward_base(accent, base, 0.5),
            Color::Rgb(100, 100, 100)
        );
    }

    #[test]
    fn every_added_theme_is_visually_distinct_from_dark() {
        let dark = OverlayTheme::Dark.palette();
        for theme in OverlayTheme::iter().filter(|&t| t != OverlayTheme::Dark) {
            let p = theme.palette();
            assert!(
                p.insert_bg != dark.insert_bg
                    || p.delete_bg != dark.delete_bg
                    || p.move_bg != dark.move_bg
                    || p.update_bg != dark.update_bg,
                "{theme:?} is identical to Dark"
            );
        }
    }

    /// Asserted as "reads as grey" rather than an exact triple, so the value can be retuned.
    #[test]
    fn every_themes_move_band_is_grey_rather_than_a_hue() {
        for theme in OverlayTheme::iter() {
            // Custom moves are the user's to colour.
            if theme == OverlayTheme::Custom {
                continue;
            }
            let Color::Rgb(r, g, b) = theme.palette().move_bg else {
                panic!("{theme:?}: expected an rgb move band");
            };
            let spread = r.max(g).max(b) - r.min(g).min(b);
            assert!(
                spread <= 12,
                "{theme:?}: move_bg should read as grey, got rgb({r}, {g}, {b}) with channel \
                 spread {spread}"
            );
        }
    }

    #[test]
    fn every_theme_has_visually_distinct_bands() {
        for theme in OverlayTheme::iter() {
            let p = theme.palette();
            let bands = [p.insert_bg, p.delete_bg, p.move_bg, p.update_bg];
            for (i, a) in bands.iter().enumerate() {
                for b in &bands[i + 1..] {
                    assert_ne!(a, b, "{theme:?}: two bands share a color");
                }
            }
        }
    }

    #[test]
    fn every_themes_search_color_is_distinct_from_bands_and_cursor_highlight() {
        for theme in OverlayTheme::iter() {
            let p = theme.palette();
            for (name, other) in [
                ("insert_bg", p.insert_bg),
                ("delete_bg", p.delete_bg),
                ("move_bg", p.move_bg),
                ("update_bg", p.update_bg),
                ("cross_highlight_bg", p.cross_highlight_bg),
            ] {
                assert_ne!(
                    p.search_bg, other,
                    "{theme:?}: search_bg collides with {name}"
                );
            }
        }
    }

    #[test]
    fn panel_layout_cycle_visits_all_three_modes_and_returns() {
        assert_eq!(PanelLayout::Auto.next(), PanelLayout::Dual);
        assert_eq!(PanelLayout::Dual.next(), PanelLayout::Single);
        assert_eq!(PanelLayout::Single.next(), PanelLayout::Auto);
    }

    #[test]
    fn saving_one_setting_preserves_the_other() {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        let path = file.path().to_path_buf();
        save_to(
            path.clone(),
            ThemeConfig {
                theme: OverlayTheme::Nord,
                layout: PanelLayout::Single,
                ..Default::default()
            },
        );

        let mut config = load_from(path.clone());
        config.theme = OverlayTheme::Dracula;
        save_to(path.clone(), config);

        let reloaded = load_from(path);
        assert_eq!(reloaded.theme, OverlayTheme::Dracula);
        assert_eq!(
            reloaded.layout,
            PanelLayout::Single,
            "changing the theme must not reset the layout"
        );
    }

    #[test]
    fn an_unparseable_custom_color_falls_back_to_that_dracula_color_only() {
        let custom = CustomPalette {
            insert_bg: "not a color".to_string(),
            delete_bg: "#010203".to_string(),
            ..CustomPalette::default()
        };
        let resolved = custom.to_palette();
        assert_eq!(
            resolved.insert_bg,
            OverlayTheme::Dracula.palette().insert_bg
        );
        assert_eq!(resolved.delete_bg, Color::Rgb(1, 2, 3));
    }

    #[test]
    fn parse_hex_color_accepts_a_bare_hex_and_rejects_other_shapes() {
        assert_eq!(
            parse_hex_color("a0b1c2"),
            Some(Color::Rgb(0xa0, 0xb1, 0xc2))
        );
        assert_eq!(
            parse_hex_color(" #A0B1C2 "),
            Some(Color::Rgb(0xa0, 0xb1, 0xc2))
        );
        assert_eq!(parse_hex_color("#abc"), None);
        assert_eq!(parse_hex_color("#gggggg"), None);
    }

    #[test]
    fn a_named_ansi_color_formats_as_its_xterm_value_and_forks_to_rgb() {
        assert_eq!(format_hex_color(Color::Red), "#cd0000");
        assert_eq!(
            parse_hex_color(&format_hex_color(Color::Red)),
            Some(Color::Rgb(205, 0, 0))
        );
    }
}
