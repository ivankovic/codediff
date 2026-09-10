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

impl OverlayPalette {
    /// The background that paints `operation`, or `None` for `Identical` and the `NotYetSet`
    /// sentinel, which keep plain syntax highlighting. The one operation-to-colour table for
    /// every ratatui renderer (the code viewer's overlay, the diff viewer's minimap bands).
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
                    // The one band that is deliberately *not* this theme's canonical accent (see
                    // the note above): a plain grey, because a move is the one operation that
                    // changes no code. Purple read as loud as insert/delete/update and pulled the
                    // eye to the thing that needs the least attention. Blended toward the
                    // background at the same 0.6 as its neighbours, so it sits at their weight
                    // rather than glowing: #aaaaaa over Dracula's base lands on Rgb(92, 93, 100).
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
/// persisting `OverlayTheme` directly) so the config file has named fields. Every field carries
/// `#[serde(default)]` so a config written by an older build still parses.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
struct ThemeConfig {
    /// Falls back to the default theme rather than failing the parse.
    ///
    /// `#[serde(default)]` alone is not enough and was the trap here: it covers a *missing* field,
    /// while an unknown enum *value* - a config written by a newer codediff, or hand-edited with a
    /// typo - is a hard error that fails the whole document. Every other setting in the file then
    /// silently reverted. `theme_or_default` matches the name against the variants this build
    /// actually has and shrugs at anything else.
    #[serde(default, deserialize_with = "theme_or_default")]
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
    /// Which parts of the diff to paint (the `M` key) - see `crate::diff::text::RenderOptions`.
    /// Defaults to `RenderOptions::FULL`, which is what every release before this setting existed
    /// rendered, so an existing config file keeps behaving exactly as it did.
    ///
    /// Supersedes a now-removed `render_mode: RenderMode` field of the same purpose. An existing
    /// config file's old `render_mode` key is simply ignored (serde skips unknown fields by
    /// default) and this field falls back to its own default the first time it loads - a one-time
    /// reset of a single local dotfile setting, not worth a migration shim for.
    #[serde(default)]
    render_options: crate::diff::text::RenderOptions,
}

/// Deserialize a theme name, falling back to the default for anything this build does not know.
///
/// Re-runs the enum's own `Deserialize` over the name rather than comparing against `Display`.
/// The two disagree: serde writes the variant identifier (`SolarizedLight`), while strum's
/// `Display` gives the picker label (`Solarized Light`, `Dracula (default)`). Matching on the
/// label would reject every theme codediff has ever written to disk - which is exactly what the
/// first version of this function did, caught by
/// `a_pre_existing_render_options_table_without_whole_pair_updates_still_loads`.
///
/// A value that is not a string at all still fails, at which point the file is structurally wrong
/// rather than merely naming something unfamiliar, and [`update_config`]'s refusal to overwrite an
/// unparseable file is what protects the user's settings.
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
/// 1. `$CODEDIFF_CONFIG`, if set and non-empty. Authoritative: no walk-up, no fallback. This is
///    the seam tests use, so they never touch a real user's settings.
/// 2. The nearest `.codediff.toml` at or above the current directory, **if one already exists**.
/// 3. `$XDG_CONFIG_HOME/codediff/config.toml`, else `$HOME/.config/codediff/config.toml`.
///
/// Layer 2 is only ever *used*, never *created*. Until 2026-09-10 the path was unconditionally
/// `./.codediff.toml`, so codediff dropped a dotfile into whatever directory it happened to run
/// in - including, memorably, a checkout of the VS Code extension, where its own integration test
/// spawning codediff littered the repository.
///
/// The walk-up matters for `git difftool` and `GIT_EXTERNAL_DIFF`, which git runs with the working
/// directory set to the repository root: without it, a project config would be found only when
/// codediff was invoked from that exact directory and not from any subdirectory of it.
///
/// `pub(crate)` so `app.rs` tests can clean up after a setter that writes for real.
pub(crate) fn config_path() -> PathBuf {
    // Checked before the test redirect below, so a test that wants a specific file still wins.
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

/// A throwaway config for the test build, so no test can write a developer's real settings.
///
/// Needed because the code that persists settings is ordinary production code that many tests
/// reach incidentally - `handle_diff_ready` records a recent pair, `apply_render_options` and the
/// panel-layout and node-highlight toggles each save - so isolating them one at a time misses the
/// ones nobody thought of. On 2026-09-10 a full run rewrote this repository's own
/// `.codediff.toml`, dropping a recent pair and flipping every render option, and only two of the
/// responsible tests had been spotted by inspection.
///
/// Done here rather than in the test harness because `cargo-nextest` 0.9.143 ignores an `[env]`
/// table in `.config/nextest.toml` ("unknown configuration key"), and a `Makefile`-level variable
/// would not cover a bare `cargo test`. Keyed by process id so a threaded `cargo test` run does
/// not have two tests fighting over one file.
#[cfg(test)]
fn test_config_path() -> PathBuf {
    std::env::temp_dir().join(format!("codediff-test-config-{}.toml", std::process::id()))
}

/// The nearest existing `.codediff.toml`, walking up from the current directory to the root.
///
/// Unreachable in the test build, where `config_path` short-circuits to `test_config_path` - the
/// walk itself is covered through `nearest_project_config_from`.
#[cfg_attr(test, allow(dead_code))]
fn nearest_project_config() -> Option<PathBuf> {
    nearest_project_config_from(&std::env::current_dir().ok()?)
}

/// The walk itself, parameterized by starting directory so it is testable without changing the
/// process's working directory out from under every other test.
fn nearest_project_config_from(start: &Path) -> Option<PathBuf> {
    start.ancestors().find_map(|directory| {
        let candidate = directory.join(PROJECT_CONFIG);
        candidate.is_file().then_some(candidate)
    })
}

/// `$XDG_CONFIG_HOME/codediff/config.toml`, else `$HOME/.config/codediff/config.toml`.
///
/// Resolved from the environment rather than through the `dirs`/`directories` crate deliberately:
/// a new dependency means regenerating the 293-crate `CRATES=` block every Gentoo ebuild bump
/// reads, which is a real cost for two `std::env::var` calls. Falling back to `./.codediff.toml`
/// when neither variable is set keeps the old behaviour on a system with no HOME at all, rather
/// than writing to a path that resolves to the filesystem root.
#[cfg_attr(test, allow(dead_code))]
fn user_config_path() -> PathBuf {
    user_config_path_from(
        std::env::var("XDG_CONFIG_HOME").ok(),
        std::env::var("HOME").ok(),
    )
}

/// The resolution itself, taking the two variables as arguments so it is testable without mutating
/// the process environment.
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

/// The last config parse failure, for the TUI to surface. `None` once a load succeeds.
///
/// A parse failure used to be entirely silent: `unwrap_or_default()` turned it into a fresh set of
/// defaults, and the next setting the user changed wrote those defaults over the file. One bad
/// line therefore cost every other setting in it, with nothing on screen to say so.
static CONFIG_ERROR: std::sync::RwLock<Option<String>> = std::sync::RwLock::new(None);

/// The current config parse error, if the last load hit one.
pub fn config_error() -> Option<String> {
    CONFIG_ERROR.read().ok().and_then(|held| held.clone())
}

/// The file exists but does not parse, so its contents must not be overwritten - see
/// [`update_config`]. A unit error rather than a two-variant enum because the `Ok` side carries a
/// `ThemeConfig`, and an enum pairing that with an empty variant is all payload and no tag.
#[derive(Debug)]
struct Unreadable;

/// Read the config, distinguishing "absent" from "present but broken".
///
/// Absent is not an error: confy yields defaults for a file that is not there, and writing over
/// nothing loses nothing. Present-but-unparseable is recorded in [`CONFIG_ERROR`] for the TUI to
/// show and refuses to become a `ThemeConfig` anyone might write back.
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

/// Read-modify-write one setting, resolving the config path exactly once.
///
/// Refuses to write when the existing file does not parse. Every setter used to be
/// `load_from(config_path())` - which silently yielded defaults on a parse error - followed by
/// `save_to(config_path(), ...)`, so changing any single setting overwrote every other one with a
/// default. Resolving the path once also matters now that it is layered: two calls could
/// otherwise read one layer and write another.
fn update_config(mutate: impl FnOnce(&mut ThemeConfig)) {
    let path = config_path();
    let Ok(mut config) = read_config(&path) else {
        return;
    };
    mutate(&mut config);
    save_to(path, config);
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
    update_config(|config| config.theme = theme);
}

/// Load the persisted panel-layout choice (the `v` key), or `PanelLayout::Auto` if the config
/// file doesn't exist yet or fails to parse.
pub fn load_panel_layout() -> PanelLayout {
    load_from(config_path()).layout
}

/// Persist the panel-layout choice, preserving the other settings in the same file - same
/// non-fatal failure semantics as `save_overlay_theme`.
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

/// Persist the render options, preserving the other settings in the same file - same non-fatal
/// failure semantics as `save_overlay_theme`.
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
    update_config(|config| config.node_highlight = enabled);
}

/// Whether a path is one of the throwaway files a VCS materializes to hand to a diff tool.
///
/// `git difftool` and `GIT_EXTERNAL_DIFF` write each side to something like
/// `/tmp/git-blob-AbC123/file.rs` and delete it the moment the tool exits, so recording such a
/// pair produces an entry that is dead before it is ever offered. jj does the same with its own
/// temp directory.
fn is_throwaway(path: &Path) -> bool {
    path.starts_with(std::env::temp_dir())
}

/// The recently diffed file pairs, most recent first - offered on the empty-start screen as
/// digit shortcuts (`tui::app::draw_viewer`).
///
/// Entries whose files have since disappeared are dropped rather than offered: a recents list is
/// only useful if selecting an item works. This also cleans up the `/tmp/git-blob-*` pairs written
/// by builds before `record_recent_pair` learned to refuse them.
///
/// Note that this list became **per-user** on 2026-09-10, along with the rest of the config; it
/// was previously per-directory, because the config file itself was.
pub fn load_recent_pairs() -> Vec<(PathBuf, PathBuf)> {
    let mut pairs = load_from(config_path()).recent_pairs;
    pairs.retain(|(before, after)| before.exists() && after.exists());
    pairs
}

/// Record a successfully diffed pair at the front of the recent list (deduplicated, capped at
/// [`MAX_RECENT_PAIRS`]), preserving the other settings in the same file. Same non-fatal failure
/// semantics as the other save functions.
///
/// A pair with a throwaway side is not recorded at all - see [`is_throwaway`]. Filtering these out
/// only on read would leave every `git difftool` invocation still writing one, and now into the
/// user-level config rather than a directory-local file.
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

/// `load_overlay_theme`/`save_overlay_theme`, parameterized by path so tests can exercise the
/// round-trip against a temp file instead of mutating the process's actual working directory.
///
/// Getters use this and fall back to defaults, which is right for a *read*: showing default colors
/// beats refusing to start. Writes go through [`update_config`] instead, which refuses to clobber
/// a file it could not parse.
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

    /// A config naming a theme this build does not know must not cost every other setting in the
    /// file. Before `theme` gained `#[serde(default)]`, the unknown value failed the whole parse,
    /// `unwrap_or_default()` produced a blank config, and the next setting the user touched wrote
    /// that blank over their file.
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

    /// The load/save asymmetry that made one bad line destroy a whole config: every setter used to
    /// read with `unwrap_or_default()` and then write the result back.
    #[test]
    fn a_file_that_does_not_parse_is_never_overwritten() {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        let garbage = "this is not toml = = =\n";
        std::fs::write(file.path(), garbage).expect("write config");

        let path = file.path().to_path_buf();
        assert!(read_config(&path).is_err());
        assert!(config_error().is_some(), "the failure must be reportable");

        // What a setter does now.
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

    /// `$CODEDIFF_CONFIG` is authoritative: no walk-up, no user-level fallback. Tests rely on this
    /// to stay off a real user's settings.
    #[test]
    fn the_environment_override_wins_over_every_other_layer() {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        unsafe { std::env::set_var(CONFIG_ENV, file.path()) };
        assert_eq!(config_path(), file.path());
        unsafe { std::env::remove_var(CONFIG_ENV) };
    }

    /// A pair whose files no longer exist is dead weight in a recents list - selecting it fails.
    /// This is also the migration that clears the `/tmp/git-blob-*` entries written by builds
    /// before `record_recent_pair` learned to refuse them.
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

    /// The fix, as opposed to the migration above: a VCS temp file is never recorded in the first
    /// place. Filtering only on read would leave every `git difftool` run still writing one.
    #[test]
    fn a_throwaway_vcs_path_is_recognised() {
        let temp = std::env::temp_dir().join("git-blob-AbC123").join("main.rs");
        assert!(is_throwaway(&temp));
        assert!(!is_throwaway(Path::new(
            "/home/someone/src/project/main.rs"
        )));
    }

    /// The walk-up is what makes a project config work under `git difftool` and from any
    /// subdirectory. git runs a difftool with the working directory set to the repository root,
    /// but codediff is just as often invoked from somewhere below it.
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

    /// The nearest one wins, so a project can override a config further up the tree.
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

    /// A directory with no config above it must not invent one - that is what dropped a
    /// `.codediff.toml` into every directory codediff was ever run in, including a checkout of the
    /// VS Code extension, where codediff's own integration test littered the repository.
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

    /// The user-level config lives two directories deep in a path that will not exist on a fresh
    /// machine (`~/.config/codediff/config.toml`). If saving did not create those directories,
    /// every setting would silently fail to persist for anyone without a project config - which is
    /// now the default case, since a project config is never created implicitly.
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
        // An empty variable is not a choice; treating it as one would produce a path rooted at the
        // filesystem root.
        assert_eq!(
            user_config_path_from(Some(String::new()), Some("/home/me".into())),
            PathBuf::from("/home/me/.config/codediff/config.toml")
        );
        // No HOME at all: keep the pre-2026-09-10 behaviour rather than writing to `/`.
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

    /// The node highlight persists, and - the part that matters - a config file written before
    /// the setting existed loads as **off**, not as "unset means the old always-on behaviour".
    /// The exact hazard `RenderOptions::whole_pair_updates`'s own `#[serde(default)]` exists to
    /// prevent: a config written before that field existed - which every `.codediff.toml` on disk
    /// today is - has a `[render_options]` table with the two older keys but not this one.
    /// Without the attribute, `confy`'s deserialization of the whole file fails on the missing
    /// key, and `load_from`'s `.unwrap_or_default()` would silently reset *everything* - theme,
    /// syntax theme, node highlight, all of it - not just this one option.
    #[test]
    fn a_pre_existing_render_options_table_without_whole_pair_updates_still_loads() {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        // What `save_to` would have written before `whole_pair_updates` existed - a real
        // `[render_options]` table missing only the new key, not a file missing the table
        // entirely (`render_options` itself is already `#[serde(default)]`, which is a different,
        // already-covered case).
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
            "the missing key must default to true (paint it) - every release before this field \
             existed always painted a reindented Move, same polarity reasoning as \
             whole_pair_updates's own default above"
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

    /// A move changes no code - it is the one band that reports relocation rather than an edit -
    /// so the shipped default paints it neutral instead of giving it a fourth loud hue competing
    /// with insert/delete/update for the eye. Asserted as "reads as grey" rather than as an exact
    /// triple, so the value can still be retuned against the background without the intent
    /// silently reverting to a color.
    #[test]
    fn every_themes_move_band_is_grey_rather_than_a_hue() {
        for theme in OverlayTheme::iter() {
            // `Custom` is whatever the user saved; it defaults to Dracula but they may have
            // deliberately painted moves any colour they like, and that is theirs to choose.
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
