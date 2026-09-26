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
//! Fitting the TUI's 24-bit colors to what the terminal can show.
//!
//! Every palette and every syntax theme is `Color::Rgb`. A terminal without 24-bit support does
//! not approximate `ESC[48;2;R;G;Bm`: macOS Terminal.app reads the channels as SGR codes of their
//! own, so a `0` channel is a reset and `#ff0000` comes out black. Such terminals get the nearest
//! xterm-256 color instead, substituted in the finished frame so no renderer has to know.

use std::collections::HashMap;

use ratatui::{buffer::Buffer, style::Color};

use crate::tui::theme::OverlayPalette;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorDepth {
    TrueColor,
    /// The xterm-256 palette; only indices 16-255 are used, since 0-15 are the user's own theme.
    Indexed256,
}

impl ColorDepth {
    /// The depth the current terminal advertises. `$COLORTERM` is the convention bat and delta
    /// follow too: terminals with 24-bit color set it, Terminal.app does not. ssh does not forward
    /// it, so a truecolor terminal on the far side of one reads as 256-color; `export
    /// COLORTERM=truecolor` there restores it.
    pub fn detect() -> Self {
        Self::from_env(
            std::env::var("COLORTERM").ok().as_deref(),
            std::env::var("TERM").ok().as_deref(),
        )
    }

    fn from_env(colorterm: Option<&str>, term: Option<&str>) -> Self {
        let colorterm = colorterm.unwrap_or_default();
        // terminfo's own name for a 24-bit entry, e.g. `xterm-direct`.
        let direct_term = term.is_some_and(|term| term.ends_with("-direct"));
        if colorterm.eq_ignore_ascii_case("truecolor")
            || colorterm.eq_ignore_ascii_case("24bit")
            || direct_term
        {
            ColorDepth::TrueColor
        } else {
            ColorDepth::Indexed256
        }
    }

    /// Rewrite every cell of a finished frame to colors this depth can show. `palette` is the
    /// overlay on screen; its backgrounds are fitted together, so no two land on one index.
    pub fn fit(self, buffer: &mut Buffer, palette: &OverlayPalette) {
        if self == ColorDepth::TrueColor {
            return;
        }
        // A frame holds a few dozen distinct colors, and matching one scores the whole cube.
        let mut fitted: HashMap<Color, Color> = fit_palette(palette).into_iter().collect();
        let mut fit = |color: Color| *fitted.entry(color).or_insert_with(|| to_indexed(color));
        for cell in &mut buffer.content {
            cell.fg = fit(cell.fg);
            cell.bg = fit(cell.bg);
            cell.underline_color = fit(cell.underline_color);
        }
    }
}

/// The palette's backgrounds as `(rgb, indexed)`, each different from the ones before it. Fitted
/// one by one, two nearby bands can land on the same cube color (Solarized Light's insert and
/// update both become `(215, 215, 135)`), and a diff whose bands look alike is wrong, not just
/// ugly. On a collision the later role takes its nearest unused color; the order is the roles'
/// precedence.
fn fit_palette(palette: &OverlayPalette) -> Vec<(Color, Color)> {
    let roles = [
        palette.insert_bg,
        palette.delete_bg,
        palette.update_bg,
        palette.move_bg,
        palette.cross_highlight_bg,
        palette.search_bg,
    ];
    let mut fitted: Vec<(Color, Color)> = Vec::with_capacity(roles.len());
    for color in roles {
        let Color::Rgb(r, g, b) = color else {
            continue;
        };
        if fitted.iter().any(|(from, _)| *from == color) {
            continue;
        }
        let taken = |index: u8| fitted.iter().any(|(_, to)| *to == Color::Indexed(index));
        // A grey has one candidate; if it is taken, sharing beats a tinted stand-in.
        let nearest = candidates((r, g, b));
        let index = nearest
            .iter()
            .copied()
            .find(|&index| !taken(index))
            .unwrap_or(nearest[0]);
        fitted.push((color, Color::Indexed(index)));
    }
    fitted
}

/// Channel spread (max - min) below which a color is a grey and goes to the grey ramp. Every
/// preset's move band is under it (`theme.rs` holds them to 12), every other band is over it, down
/// to Nord's muted insert at 14.
const GREY_SPREAD: u8 = 13;

/// The nearest xterm-256 color to an `Rgb`; any other color is returned unchanged.
fn to_indexed(color: Color) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Indexed(candidates((r, g, b))[0]),
        other => other,
    }
}

/// The indices that may stand in for `rgb`, nearest first. A grey gets the grey ramp. A tinted
/// color gets only the cube's tinted colors: the cube's steps are coarse, so a muted band is
/// often nearest a grey, and every muted band would then read as the same grey.
fn candidates((r, g, b): (u8, u8, u8)) -> Vec<u8> {
    let spread = r.max(g).max(b) - r.min(g).min(b);
    if spread < GREY_SPREAD {
        return vec![grey_index(r, g, b)];
    }
    let mut tinted: Vec<(f32, u8)> = (16..=231u8)
        .filter(|&index| {
            let (r, g, b) = xterm_256(index);
            !(r == g && g == b)
        })
        .map(|index| (hue_weighted_distance((r, g, b), xterm_256(index)), index))
        .collect();
    tinted.sort_by(|x, y| x.0.total_cmp(&y.0));
    tinted.into_iter().map(|(_, index)| index).collect()
}

/// Weights of a difference's three parts in [`hue_weighted_distance`]. Plain RGB distance is
/// `3, 1, 1`; these give hue the most say and chroma the least, so an approximation may be more
/// or less saturated than the original, but not a different hue.
const LIGHTNESS_WEIGHT: f32 = 1.0;
const CHROMA_WEIGHT: f32 = 0.25;
const HUE_WEIGHT: f32 = 2.0;

/// A color splits into its mean `m` (lightness) and `c = rgb - m`, whose length is its chroma and
/// whose direction its hue. Squared RGB distance is `3 dm^2 + dC^2 + 2 Ca Cb (1 - cos angle)`;
/// this is the same sum with the terms weighted.
fn hue_weighted_distance(a: (u8, u8, u8), b: (u8, u8, u8)) -> f32 {
    let split = |(r, g, b): (u8, u8, u8)| {
        let rgb = [r as f32, g as f32, b as f32];
        let mean = rgb.iter().sum::<f32>() / 3.0;
        (mean, rgb.map(|channel| channel - mean))
    };
    let length = |c: [f32; 3]| c.iter().map(|x| x * x).sum::<f32>().sqrt();
    let ((mean_a, chroma_a), (mean_b, chroma_b)) = (split(a), split(b));
    let (length_a, length_b) = (length(chroma_a), length(chroma_b));
    let dot: f32 = (0..3).map(|i| chroma_a[i] * chroma_b[i]).sum();
    // `2 Ca Cb (1 - cos)`, which is 0 when either side has no hue.
    let hue = 2.0 * (length_a * length_b - dot);
    LIGHTNESS_WEIGHT * (mean_a - mean_b).powi(2)
        + CHROMA_WEIGHT * (length_a - length_b).powi(2)
        + HUE_WEIGHT * hue
}

/// The xterm value of a 256-color index; the first 16 at their conventional xterm values, like
/// the named colors in `theme::format_hex_color`.
pub fn xterm_256(index: u8) -> (u8, u8, u8) {
    const NAMED: [Color; 16] = [
        Color::Black,
        Color::Red,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Magenta,
        Color::Cyan,
        Color::Gray,
        Color::DarkGray,
        Color::LightRed,
        Color::LightGreen,
        Color::LightYellow,
        Color::LightBlue,
        Color::LightMagenta,
        Color::LightCyan,
        Color::White,
    ];
    match index {
        0..=15 => {
            let hex = crate::tui::theme::format_hex_color(NAMED[index as usize]);
            let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).unwrap_or(0);
            (channel(1), channel(3), channel(5))
        }
        16..=231 => {
            let level = |n: u8| if n == 0 { 0 } else { 55 + 40 * n };
            let cube = index - 16;
            (level(cube / 36), level(cube / 6 % 6), level(cube % 6))
        }
        232..=255 => {
            let grey = 8 + 10 * (index - 232);
            (grey, grey, grey)
        }
    }
}

/// The nearest neutral: the 24-step ramp (232-255, `8 + 10 * step`), or the cube's black and
/// white beyond its ends.
fn grey_index(r: u8, g: u8, b: u8) -> u8 {
    let average = (r as u16 + g as u16 + b as u16) / 3;
    match average {
        0..=3 => 16,
        247.. => 231,
        _ => 232 + ((average - 3) / 10).min(23) as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::theme::OverlayTheme;
    use strum::IntoEnumIterator;

    #[test]
    fn only_an_advertised_24_bit_terminal_gets_true_color() {
        assert_eq!(
            ColorDepth::from_env(Some("truecolor"), Some("xterm-256color")),
            ColorDepth::TrueColor
        );
        assert_eq!(
            ColorDepth::from_env(Some("24bit"), None),
            ColorDepth::TrueColor
        );
        assert_eq!(
            ColorDepth::from_env(None, Some("xterm-direct")),
            ColorDepth::TrueColor
        );
        // What macOS Terminal.app sets.
        assert_eq!(
            ColorDepth::from_env(None, Some("xterm-256color")),
            ColorDepth::Indexed256
        );
        assert_eq!(ColorDepth::from_env(None, None), ColorDepth::Indexed256);
    }

    #[test]
    fn a_pure_color_maps_to_its_cube_corner() {
        assert_eq!(to_indexed(Color::Rgb(255, 0, 0)), Color::Indexed(196));
        assert_eq!(to_indexed(Color::Rgb(0, 255, 0)), Color::Indexed(46));
        assert_eq!(to_indexed(Color::Rgb(0, 0, 255)), Color::Indexed(21));
    }

    #[test]
    fn greys_use_the_ramp_and_its_ends_the_cube() {
        assert_eq!(to_indexed(Color::Rgb(0, 0, 0)), Color::Indexed(16));
        assert_eq!(to_indexed(Color::Rgb(255, 255, 255)), Color::Indexed(231));
        assert_eq!(to_indexed(Color::Rgb(8, 8, 8)), Color::Indexed(232));
        assert_eq!(to_indexed(Color::Rgb(48, 48, 48)), Color::Indexed(236));
        assert_eq!(to_indexed(Color::Rgb(238, 238, 238)), Color::Indexed(255));
    }

    #[test]
    fn a_dark_tinted_band_keeps_its_hue() {
        // Dark's insert and delete bands; the nearest grey would make them one color.
        assert_eq!(to_indexed(Color::Rgb(20, 60, 20)), Color::Indexed(22));
        assert_eq!(to_indexed(Color::Rgb(70, 20, 20)), Color::Indexed(52));
    }

    #[test]
    fn named_and_indexed_colors_pass_through() {
        for color in [Color::Reset, Color::Red, Color::Indexed(3)] {
            assert_eq!(to_indexed(color), color);
        }
    }

    /// The same guarantees `theme.rs` asserts at 24 bits, after the approximation.
    #[test]
    fn every_presets_bands_cursor_and_search_stay_distinct_in_256_colors() {
        for theme in OverlayTheme::iter() {
            let fitted = fit_palette(&theme.palette());
            assert_eq!(fitted.len(), 6, "{theme:?}: two roles share an rgb color");
            for (i, (from_a, a)) in fitted.iter().enumerate() {
                for (from_b, b) in &fitted[i + 1..] {
                    assert_ne!(
                        a, b,
                        "{theme:?}: {from_a:?} and {from_b:?} both became {a:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_colliding_band_takes_the_next_nearest_tinted_color() {
        let palette = OverlayPalette {
            insert_bg: Color::Rgb(20, 60, 20),
            update_bg: Color::Rgb(21, 60, 20),
            ..OverlayTheme::Dark.palette()
        };
        assert_eq!(to_indexed(palette.insert_bg), to_indexed(palette.update_bg));

        let fitted = fit_palette(&palette);
        let (insert, update) = (fitted[0].1, fitted[2].1);
        assert_eq!(insert, to_indexed(palette.insert_bg));
        assert_ne!(update, insert);
        let Color::Indexed(update) = update else {
            panic!("expected an index, got {update:?}");
        };
        let (r, g, b) = xterm_256(update);
        assert!(!(r == g && g == b), "a tinted band fell back to a grey");
    }

    #[test]
    fn fitting_a_frame_rewrites_every_rgb_cell() {
        let mut buffer = Buffer::empty(ratatui::layout::Rect::new(0, 0, 2, 1));
        buffer.content[0].set_bg(Color::Rgb(255, 0, 0));
        buffer.content[1].set_fg(Color::Rgb(0, 0, 255));

        let palette = OverlayTheme::default().palette();

        ColorDepth::TrueColor.fit(&mut buffer, &palette);
        assert_eq!(buffer.content[0].bg, Color::Rgb(255, 0, 0));

        ColorDepth::Indexed256.fit(&mut buffer, &palette);
        assert_eq!(buffer.content[0].bg, Color::Indexed(196));
        assert_eq!(buffer.content[1].fg, Color::Indexed(21));
    }
}
