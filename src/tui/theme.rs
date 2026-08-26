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

/// A named palette for the colors used to paint the diff/cursor overlay (the insert/delete/
/// move/update backgrounds, the overlay foreground, and the cross-panel cursor highlight - see
/// `tui/widgets/code_viewer.rs`).
///
/// Picked explicitly by the user via the `c` theme picker (`tui/components/theme_dialog.rs`)
/// and persisted across runs (`tui/app.rs`), since no single hardcoded palette reads well on
/// every terminal: the original all-dark bands are unreadable on a light-background terminal.
///
/// `Dracula` is the `#[default]` (2026-08-24, was `Dark`). The variant order below is the theme
/// picker's display order and is deliberately left alone - only the `#[default]` attribute and
/// the two "(default)" labels moved, so an existing `.codediff.toml` with an explicit `theme` is
/// unaffected. Anyone who never opened the picker gets Dracula on the next run.
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
    /// The user's own palette, edited in the theme dialog and persisted as
    /// [`CustomPalette`] in `.codediff.toml`.
    ///
    /// Unlike every other variant, this one is *not* a pure function of the enum: its colors come
    /// from `custom_palette()`, process-global state loaded once at startup and updated when the
    /// dialog commits an edit. That asymmetry is deliberate and contained - `palette()` stays the
    /// single resolution point, so every existing call site keeps working unchanged rather than
    /// threading a palette through `render_minimap`, the widgets and the help modal.
    #[strum(to_string = "Custom")]
    Custom,
}

/// A user-edited palette, stored as `#rrggbb` strings so the config file is readable and
/// hand-editable. Parsed via [`parse_hex_color`]; anything unparseable falls back to the
/// corresponding Dracula color rather than failing the load, so a typo in a hand-edited config
/// costs one wrong color instead of the whole theme.
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
    /// Dracula, the shipped default theme - so "switch to Custom" starts from what the user was
    /// already looking at rather than from an empty or arbitrary palette.
    fn default() -> Self {
        Self::from_palette(&OverlayTheme::Dracula.palette())
    }
}

impl CustomPalette {
    /// Snapshot an existing palette as editable hex - what "editing a preset forks it to Custom"
    /// does.
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

    /// Resolve back to concrete colors, falling back per field (see the struct's doc comment).
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

/// `#rrggbb` (or bare `rrggbb`) to a `Color`. `None` for anything else - including the named and
/// indexed `Color` variants, which have no hex form; a preset using `Color::Red` for a panel
/// title round-trips through [`format_hex_color`]'s ANSI table instead.
pub fn parse_hex_color(text: &str) -> Option<Color> {
    let hex = text.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |range: std::ops::Range<usize>| u8::from_str_radix(&hex[range], 16).ok();
    Some(Color::Rgb(channel(0..2)?, channel(2..4)?, channel(4..6)?))
}

/// A `Color` as `#rrggbb`, for display and for the config file.
///
/// The 16 named ANSI colors have no true RGB value - the terminal decides what they look like -
/// so they are rendered at their conventional xterm values purely so the dialog has something to
/// show and edit. Editing one produces a real `Color::Rgb`, which is why a preset's panel title
/// (`Color::Red`) becomes a concrete `#cd0000` the moment it is forked into Custom.
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

/// Process-global custom palette - see [`OverlayTheme::Custom`] for why this is not threaded
/// through call sites. Written once at startup from the config and again whenever the theme
/// dialog commits an edit.
static CUSTOM_PALETTE: std::sync::RwLock<Option<CustomPalette>> = std::sync::RwLock::new(None);

/// The current custom palette, defaulting to Dracula's colors if none has been loaded or saved.
pub fn custom_palette() -> CustomPalette {
    CUSTOM_PALETTE
        .read()
        .ok()
        .and_then(|guard| guard.clone())
        .unwrap_or_default()
}

/// Replace the in-memory custom palette (the theme dialog's live preview path). Persisting is
/// separate - see `save_custom_palette`.
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
    /// Search-match highlight (the `/` modal's results). A separate color from
    /// `cross_highlight_bg`: while a search is active, "search hit" and "counterpart of the
    /// cursor" used to be visually identical, which made it impossible to tell which blue block
    /// the `>`/`<` keys would step to next. Every theme uses its own orange accent - the one hue
    /// none of the four diff bands or the blue/cyan cursor highlight occupy.
    pub search_bg: Color,
    /// Foreground for the "Before" panel title, and its "After" counterpart below. Hardcoded as
    /// `Color::Red`/`Color::Green` in `diff_viewer` until 2026-08-24; moved here so the custom
    /// theme can change them. Every *preset* keeps exactly those two values, so presets look
    /// identical to before - only `OverlayTheme::Custom` can vary them.
    pub before_title_fg: Color,
    pub after_title_fg: Color,
}

/// The `before_title_fg`/`after_title_fg` every preset uses - the colors the panel titles had
/// when they were hardcoded in `diff_viewer::draw`.
pub const PRESET_BEFORE_TITLE_FG: Color = Color::Red;
pub const PRESET_AFTER_TITLE_FG: Color = Color::Green;

impl OverlayTheme {
    /// The colors for this theme.
    ///
    /// `SolarizedDark`/`SolarizedLight` aren't invented RGB literals: each band color is the
    /// canonical Solarized (Ethan Schoonover) accent - green/red/yellow/magenta - alpha-blended
    /// toward that variant's own Solarized base color (`base03` for dark, `base3` for light), so
    /// the result is still recognizably "Solarized" rather than a clashing overlay. `overlay_fg`
    /// is likewise a Solarized base shade chosen for contrast against its own bands: light text
    /// (`base2`) for the dark variant, dark text (`base02`) for the light one - the light variant
    /// is the actual fix for "too dark on a light terminal", since every other color here was
    /// previously a fixed dark RGB triple regardless of terminal background.
    pub fn palette(self) -> OverlayPalette {
        match self {
            OverlayTheme::Custom => custom_palette().to_palette(),
            OverlayTheme::Dark => OverlayPalette {
                insert_bg: Color::Rgb(20, 60, 20),
                delete_bg: Color::Rgb(70, 20, 20),
                move_bg: Color::Rgb(60, 20, 60),
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
                move_bg: Color::Rgb(84, 47, 84),
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
                move_bg: Color::Rgb(236, 169, 188),
                update_bg: Color::Rgb(224, 202, 136),
                overlay_fg: Color::Rgb(7, 54, 66),
                cross_highlight_bg: Color::Rgb(124, 182, 217),
                // Solarized orange blended 0.4 toward base3, same vividness as the cursor blue.
                search_bg: Color::Rgb(223, 143, 104),
                before_title_fg: PRESET_BEFORE_TITLE_FG,
                after_title_fg: PRESET_AFTER_TITLE_FG,
            },
            // The five palettes below all follow the same recipe, reverse-engineered from the
            // Solarized variants above (whose values were hand-picked before this helper existed):
            // each band is that theme's own canonical accent color (from its official public
            // spec/palette - not invented) blended 60% toward the theme's own background via
            // `blend_toward_base`, and `cross_highlight_bg` is blended only 40% toward it so it
            // stays visibly more vivid than the bands - reserved for "where the cursor is," not
            // just "what changed." `overlay_fg` is always the theme's own canonical foreground
            // color, unblended, since text needs to stay maximally readable.
            OverlayTheme::Dracula => {
                // https://draculatheme.com/spec
                let bg = (40, 42, 54);
                OverlayPalette {
                    insert_bg: blend_toward_base((80, 250, 123), bg, 0.6), // green
                    delete_bg: blend_toward_base((255, 85, 85), bg, 0.6),  // red
                    move_bg: blend_toward_base((189, 147, 249), bg, 0.6),  // purple
                    update_bg: blend_toward_base((241, 250, 140), bg, 0.6), // yellow
                    overlay_fg: Color::Rgb(248, 248, 242),                 // foreground
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
                    move_bg: blend_toward_base((180, 142, 173), bg, 0.6),   // nord15, purple
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
                    move_bg: blend_toward_base((211, 134, 155), bg, 0.6),  // bright purple
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
                    move_bg: blend_toward_base((174, 129, 255), bg, 0.6),  // purple
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
                    move_bg: blend_toward_base((198, 120, 221), bg, 0.6),   // purple
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

/// Blends `accent` toward `base` by `base_weight` (`0.0` = pure accent, `1.0` = pure base). Both
/// are `(r, g, b)` triples rather than `Color`, since every caller works from a plain canonical
/// hex triple. See `OverlayTheme::palette`'s doc comment on the five themes that use this.
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

/// How the before/after panels should be laid out - the persisted counterpart of
/// `DiffViewer`'s width-based auto choice. Lives here (not in `diff_viewer.rs`) because this
/// module owns the config file both settings persist to; `DiffViewer` consumes it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PanelLayout {
    /// Pick dual/single from the terminal width (`SINGLE_PANEL_THRESHOLD`) - today's behavior.
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

/// How many recently diffed file pairs to remember (see `record_recent_pair`) - capped at the
/// nine digit keys the empty-start screen offers for reopening them.
const MAX_RECENT_PAIRS: usize = 9;

/// On-disk representation of the persisted settings. A dedicated struct (rather than
/// persisting `OverlayTheme` directly) so the config file has named fields. Every field after
/// `theme` carries `#[serde(default)]` so a config written by an older build still parses.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
struct ThemeConfig {
    theme: OverlayTheme,
    #[serde(default)]
    layout: PanelLayout,
    #[serde(default)]
    recent_pairs: Vec<(PathBuf, PathBuf)>,
    /// The user's edited palette, used when `theme` is `OverlayTheme::Custom`. Kept even while a
    /// preset is selected, so switching back to Custom restores the edits rather than resetting.
    #[serde(default)]
    custom_palette: CustomPalette,
    /// Syntax-highlighting theme name, one of syntect's built-ins (see `syntax_theme_names`).
    /// Empty means "whatever the code viewer defaults to".
    #[serde(default)]
    syntax_theme: String,
    /// Whether the node highlight (the `H` key) is on. `bool`'s `Default` is `false`, which is
    /// deliberately also this feature's shipped default - see `load_node_highlight`.
    #[serde(default)]
    node_highlight: bool,
    /// How much of the diff to paint (the `M` key) - see `crate::diff::text::RenderMode`. Defaults
    /// to `Full`, which is what every release before this setting existed rendered, so an existing
    /// config file keeps behaving exactly as it did.
    #[serde(default)]
    render_mode: crate::diff::text::RenderMode,
}

/// The config file's path: a dotfile in the current working directory, per the exploratory-
/// testing request to store the choice "in a config file in the current working directory".
/// `pub(crate)` only so `app.rs` tests can clean up the file a real `save_overlay_theme` call
/// writes into the test process's actual cwd.
pub(crate) fn config_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".codediff.toml")
}

/// Load the persisted theme choice, or `OverlayTheme::default()` if the config file doesn't
/// exist yet or fails to parse.
///
/// Uses `confy` rather than `config-rs` (the most-downloaded Rust config crate by a wide
/// margin): `config-rs` is read-only and has no way to write a choice back to disk, which
/// `save_overlay_theme` below needs to do. `confy` exists specifically for this round-trip -
/// load a small struct, store it back to an exact path - at the cost of being a much smaller,
/// less general-purpose library.
pub fn load_overlay_theme() -> OverlayTheme {
    load_from(config_path()).theme
}

/// Persist the user's theme choice for future runs. Failures (e.g. a read-only working
/// directory) are non-fatal: the choice simply won't survive a restart. Load-modify-save so the
/// other persisted settings in the same file survive the write.
pub fn save_overlay_theme(theme: OverlayTheme) {
    let mut config = load_from(config_path());
    config.theme = theme;
    save_to(config_path(), config);
}

/// Load the persisted panel-layout choice (the `v` key), or `PanelLayout::Auto` if the config
/// file doesn't exist yet or fails to parse.
pub fn load_panel_layout() -> PanelLayout {
    load_from(config_path()).layout
}

/// Persist the panel-layout choice, preserving the other settings in the same file - same
/// non-fatal failure semantics as `save_overlay_theme`.
pub fn save_panel_layout(layout: PanelLayout) {
    let mut config = load_from(config_path());
    config.layout = layout;
    save_to(config_path(), config);
}

/// The persisted custom palette, or Dracula's colors if none was ever saved.
pub fn load_custom_palette() -> CustomPalette {
    load_from(config_path()).custom_palette
}

/// Persist the custom palette *and* install it as the live one, so the caller cannot save a
/// palette the running process isn't using.
pub fn save_custom_palette(palette: CustomPalette) {
    set_custom_palette(palette.clone());
    let mut config = load_from(config_path());
    config.custom_palette = palette;
    save_to(config_path(), config);
}

/// The persisted render mode (the `M` key), or `RenderMode::Full` if none was ever chosen.
pub fn load_render_mode() -> crate::diff::text::RenderMode {
    load_from(config_path()).render_mode
}

/// Persist the render mode, preserving the other settings in the same file - same non-fatal
/// failure semantics as `save_overlay_theme`.
pub fn save_render_mode(mode: crate::diff::text::RenderMode) {
    let mut config = load_from(config_path());
    config.render_mode = mode;
    save_to(config_path(), config);
}

/// The persisted syntax-highlighting theme name, or `None` if the user never picked one.
pub fn load_syntax_theme() -> Option<String> {
    let name = load_from(config_path()).syntax_theme;
    (!name.is_empty()).then_some(name)
}

/// Persist the syntax-highlighting theme choice.
pub fn save_syntax_theme(name: &str) {
    let mut config = load_from(config_path());
    config.syntax_theme = name.to_string();
    save_to(config_path(), config);
}

/// Whether the node highlight is enabled (the `H` key), defaulting to **off**.
///
/// Off by default because it is a constant, cursor-following repaint: every cursor movement
/// recolors the range under the cursor and its counterpart on the other panel, which reads as
/// flicker while navigating and obscures the diff coloring underneath it - the thing the user is
/// actually there to read. It stays available for the case it was built for, answering "what does
/// this specific node map to", which is a question you ask occasionally rather than continuously.
pub fn load_node_highlight() -> bool {
    load_from(config_path()).node_highlight
}

/// Persist the node-highlight toggle, preserving the other settings in the same file - same
/// non-fatal failure semantics as `save_overlay_theme`.
pub fn save_node_highlight(enabled: bool) {
    let mut config = load_from(config_path());
    config.node_highlight = enabled;
    save_to(config_path(), config);
}

/// The recently diffed file pairs, most recent first - offered on the empty-start screen as
/// digit shortcuts (`tui::app::draw_viewer`).
pub fn load_recent_pairs() -> Vec<(PathBuf, PathBuf)> {
    load_from(config_path()).recent_pairs
}

/// Record a successfully diffed pair at the front of the recent list (deduplicated, capped at
/// [`MAX_RECENT_PAIRS`]), preserving the other settings in the same file. Same non-fatal failure
/// semantics as the other save functions.
pub fn record_recent_pair(before: &Path, after: &Path) {
    let mut config = load_from(config_path());
    let pair = (before.to_path_buf(), after.to_path_buf());
    config.recent_pairs.retain(|existing| existing != &pair);
    config.recent_pairs.insert(0, pair);
    config.recent_pairs.truncate(MAX_RECENT_PAIRS);
    save_to(config_path(), config);
}

/// `load_overlay_theme`/`save_overlay_theme`, parameterized by path so tests can exercise the
/// round-trip against a temp file instead of mutating the process's actual working directory.
fn load_from(path: PathBuf) -> ThemeConfig {
    confy::load_path::<ThemeConfig>(path).unwrap_or_default()
}

fn save_to(path: PathBuf, config: ThemeConfig) {
    let _ = confy::store_path(path, config);
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

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

    /// The node highlight persists, and - the part that matters - a config file written before
    /// the setting existed loads as **off**, not as "unset means the old always-on behaviour".
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

        // Exactly what a config written by a build predating this setting looks like.
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

    /// Every theme picker option (including the five palettes derived via `blend_toward_base`)
    /// must actually resolve to a distinct set of colors from the built-in `Dark` theme -
    /// otherwise "more color schemes" would just be more names for the same look.
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

    /// Every theme's bands must actually be distinct colors - otherwise the picker would offer
    /// a "choice" that doesn't change anything visible.
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

    /// The search highlight exists precisely to be distinguishable from both the four diff bands
    /// and the cursor cross-highlight (see `OverlayPalette::search_bg`'s doc comment) - a theme
    /// where it collides with any of them has silently reintroduced the ambiguity it fixes.
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

    /// `save_overlay_theme`/`save_panel_layout` are load-modify-save specifically so one setting's
    /// write can't clobber the other back to default - exercised here via the path-parameterized
    /// helpers they both delegate to.
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
}
