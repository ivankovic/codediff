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
use std::path::PathBuf;

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
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, EnumIter, Display)]
pub enum OverlayTheme {
    #[default]
    #[strum(to_string = "Dark (default)")]
    Dark,
    #[strum(to_string = "Solarized Dark")]
    SolarizedDark,
    #[strum(to_string = "Solarized Light")]
    SolarizedLight,
    #[strum(to_string = "Dracula")]
    Dracula,
    #[strum(to_string = "Nord")]
    Nord,
    #[strum(to_string = "Gruvbox Dark")]
    GruvboxDark,
    #[strum(to_string = "Monokai")]
    Monokai,
    #[strum(to_string = "One Dark")]
    OneDark,
}

/// The concrete colors making up one [`OverlayTheme`].
pub struct OverlayPalette {
    pub insert_bg: Color,
    pub delete_bg: Color,
    pub move_bg: Color,
    pub update_bg: Color,
    pub overlay_fg: Color,
    pub cross_highlight_bg: Color,
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
            OverlayTheme::Dark => OverlayPalette {
                insert_bg: Color::Rgb(20, 60, 20),
                delete_bg: Color::Rgb(70, 20, 20),
                move_bg: Color::Rgb(60, 20, 60),
                update_bg: Color::Rgb(70, 60, 10),
                overlay_fg: Color::Rgb(225, 225, 225),
                cross_highlight_bg: Color::Rgb(40, 90, 200),
            },
            OverlayTheme::SolarizedDark => OverlayPalette {
                insert_bg: Color::Rgb(53, 87, 32),
                delete_bg: Color::Rgb(88, 46, 51),
                move_bg: Color::Rgb(84, 47, 84),
                update_bg: Color::Rgb(72, 81, 32),
                overlay_fg: Color::Rgb(238, 232, 213),
                cross_highlight_bg: Color::Rgb(23, 101, 148),
            },
            OverlayTheme::SolarizedLight => OverlayPalette {
                insert_bg: Color::Rgb(205, 209, 136),
                delete_bg: Color::Rgb(240, 168, 155),
                move_bg: Color::Rgb(236, 169, 188),
                update_bg: Color::Rgb(224, 202, 136),
                overlay_fg: Color::Rgb(7, 54, 66),
                cross_highlight_bg: Color::Rgb(124, 182, 217),
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

/// On-disk representation of the persisted theme choice. A dedicated struct (rather than
/// persisting `OverlayTheme` directly) so the config file has a named `theme = "..."` field.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
struct ThemeConfig {
    theme: OverlayTheme,
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
/// directory) are non-fatal: the choice simply won't survive a restart.
pub fn save_overlay_theme(theme: OverlayTheme) {
    save_to(config_path(), ThemeConfig { theme });
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
            },
        );
        assert_eq!(
            load_from(file.path().to_path_buf()).theme,
            OverlayTheme::SolarizedLight
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
}
