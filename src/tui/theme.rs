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
                move_bg: Color::Rgb(70, 60, 10),
                update_bg: Color::Rgb(60, 20, 60),
                overlay_fg: Color::Rgb(225, 225, 225),
                cross_highlight_bg: Color::Rgb(40, 90, 200),
            },
            OverlayTheme::SolarizedDark => OverlayPalette {
                insert_bg: Color::Rgb(53, 87, 32),
                delete_bg: Color::Rgb(88, 46, 51),
                move_bg: Color::Rgb(72, 81, 32),
                update_bg: Color::Rgb(84, 47, 84),
                overlay_fg: Color::Rgb(238, 232, 213),
                cross_highlight_bg: Color::Rgb(23, 101, 148),
            },
            OverlayTheme::SolarizedLight => OverlayPalette {
                insert_bg: Color::Rgb(205, 209, 136),
                delete_bg: Color::Rgb(240, 168, 155),
                move_bg: Color::Rgb(224, 202, 136),
                update_bg: Color::Rgb(236, 169, 188),
                overlay_fg: Color::Rgb(7, 54, 66),
                cross_highlight_bg: Color::Rgb(124, 182, 217),
            },
        }
    }
}

/// On-disk representation of the persisted theme choice. A dedicated struct (rather than
/// persisting `OverlayTheme` directly) so the config file has a named `theme = "..."` field.
#[derive(Debug, Default, Serialize, Deserialize)]
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
    load_from(config_path())
}

/// Persist the user's theme choice for future runs. Failures (e.g. a read-only working
/// directory) are non-fatal: the choice simply won't survive a restart.
pub fn save_overlay_theme(theme: OverlayTheme) {
    save_to(config_path(), theme);
}

/// `load_overlay_theme`/`save_overlay_theme`, parameterized by path so tests can exercise the
/// round-trip against a temp file instead of mutating the process's actual working directory.
fn load_from(path: PathBuf) -> OverlayTheme {
    confy::load_path::<ThemeConfig>(path)
        .map(|c| c.theme)
        .unwrap_or_default()
}

fn save_to(path: PathBuf, theme: OverlayTheme) {
    let _ = confy::store_path(path, ThemeConfig { theme });
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    #[test]
    fn save_then_load_round_trips_the_chosen_theme() {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        save_to(file.path().to_path_buf(), OverlayTheme::SolarizedLight);
        assert_eq!(
            load_from(file.path().to_path_buf()),
            OverlayTheme::SolarizedLight
        );
    }

    #[test]
    fn load_from_a_missing_file_falls_back_to_default_without_erroring() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("does-not-exist.toml");
        assert_eq!(load_from(path), OverlayTheme::default());
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
