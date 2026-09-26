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
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program. If not, see <https://www.gnu.org/licenses/>.
 */
//! A still of the viewer, drawn by the TUI's own widgets into an offscreen terminal, for the
//! README's screenshot (`make readme-screenshot`, via the `render_tui_screenshot` binary and
//! `scripts/render_tui_screenshot.py`). A hand-taken screenshot drifts from the product as the
//! painting changes; this one is regenerated from it.
//!
//! Colours are resolved to `#rrggbb` here, where ratatui's `Color` is in scope. The terminal's own
//! defaults (`Color::Reset`) stay `None` for the rasterizer to fill, since a still has no terminal
//! to inherit them from.

use std::path::Path;

use anyhow::{Context, Result};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};
use serde::Serialize;
use strum::IntoEnumIterator;

use crate::tui::app::App;
use crate::tui::color_depth::{ColorDepth, xterm_256};
use crate::tui::theme::{OverlayTheme, format_hex_color};

/// Consecutive cells of one row that share a style.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Run {
    pub text: String,
    /// `#rrggbb`, or `None` for the terminal's default foreground.
    pub fg: Option<String>,
    /// `#rrggbb`, or `None` for the terminal's default background.
    pub bg: Option<String>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
}

/// One frame of the viewer: `rows` lines of runs, each line `cols` cells wide.
#[derive(Debug, Clone, Serialize)]
pub struct Screenshot {
    pub cols: u16,
    pub rows: u16,
    pub lines: Vec<Vec<Run>>,
}

/// The viewer showing `before` against `after` in a `cols` by `rows` terminal.
///
/// Opening a pair records it as recently used in the config file, as the live viewer does; a
/// caller that must not touch the real config points `theme::CONFIG_ENV` elsewhere first.
pub fn render(
    before: &Path,
    after: &Path,
    cols: u16,
    rows: u16,
    overlay: OverlayTheme,
    syntax_theme: Option<&str>,
    color_depth: ColorDepth,
) -> Result<Screenshot> {
    let area = Rect::new(0, 0, cols, rows);
    let mut app = App::new(4.0, 60.0)?;
    app.load_still(
        before,
        after,
        area,
        overlay,
        syntax_theme.map(str::to_owned),
    )?;

    let mut terminal = Terminal::new(TestBackend::new(cols, rows))?;
    let mut drawn = Ok(());
    terminal.draw(|frame| {
        let area = frame.area();
        drawn = app.draw_viewer(frame, area);
        color_depth.fit(frame.buffer_mut(), &overlay.palette());
    })?;
    drawn?;

    let buffer = terminal.backend().buffer();
    let mut lines = Vec::with_capacity(rows as usize);
    for y in 0..rows {
        let mut runs: Vec<Run> = Vec::new();
        for x in 0..cols {
            let cell = buffer
                .cell((x, y))
                .with_context(|| format!("no cell at column {x}, row {y}"))?;
            let run = Run {
                text: cell.symbol().to_string(),
                fg: hex(cell.fg),
                bg: hex(cell.bg),
                bold: cell.modifier.contains(Modifier::BOLD),
                dim: cell.modifier.contains(Modifier::DIM),
                italic: cell.modifier.contains(Modifier::ITALIC),
                underline: cell.modifier.contains(Modifier::UNDERLINED),
            };
            match runs.last_mut() {
                Some(last) if same_style(last, &run) => last.text.push_str(&run.text),
                _ => runs.push(run),
            }
        }
        lines.push(runs);
    }
    Ok(Screenshot { cols, rows, lines })
}

/// An [`OverlayTheme`] by its picker label (`Solarized Light`) or variant name
/// (`SolarizedLight`), ignoring case and spaces.
pub fn overlay_theme_named(name: &str) -> Option<OverlayTheme> {
    let wanted = fold(name);
    OverlayTheme::iter()
        .find(|theme| fold(&theme.to_string()) == wanted || fold(&format!("{theme:?}")) == wanted)
}

fn fold(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn same_style(a: &Run, b: &Run) -> bool {
    a.fg == b.fg
        && a.bg == b.bg
        && a.bold == b.bold
        && a.dim == b.dim
        && a.italic == b.italic
        && a.underline == b.underline
}

/// `#rrggbb` for every colour but the terminal default. The 256-colour indices follow xterm's
/// table, like the named colours in [`format_hex_color`].
fn hex(color: Color) -> Option<String> {
    match color {
        Color::Reset => None,
        Color::Indexed(index) => {
            let (r, g, b) = xterm_256(index);
            Some(format!("#{r:02x}{g:02x}{b:02x}"))
        }
        other => Some(format_hex_color(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::theme;

    fn pair(dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
        let before = dir.join("before.py");
        let after = dir.join("after.py");
        std::fs::write(
            &before,
            "def total(xs):\n    t = 0\n    for x in xs:\n        t += x\n    return t\n",
        )
        .unwrap();
        std::fs::write(&after, "def total(xs):\n    return sum(xs)\n").unwrap();
        (before, after)
    }

    #[test]
    fn render_fills_every_cell_and_paints_the_change() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let config = dir.path().join("config.toml");
        unsafe { std::env::set_var(theme::CONFIG_ENV, &config) };
        let (before, after) = pair(dir.path());

        let shot = render(
            &before,
            &after,
            230,
            20,
            OverlayTheme::SolarizedLight,
            Some("Solarized (light)"),
            ColorDepth::TrueColor,
        )?;

        assert_eq!((shot.cols, shot.rows), (230, 20));
        assert_eq!(shot.lines.len(), 20);
        for line in &shot.lines {
            let width: usize = line.iter().map(|run| run.text.chars().count()).sum();
            assert_eq!(width, 230, "every row spans the terminal");
        }
        let text: String = shot.lines[0].iter().map(|run| run.text.as_str()).collect();
        assert!(text.contains("before.py"), "left title: {text}");
        assert!(text.contains("after.py"), "right title: {text}");
        let delete_bg = format_hex_color(OverlayTheme::SolarizedLight.palette().delete_bg);
        assert!(
            shot.lines
                .iter()
                .flatten()
                .any(|run| run.bg.as_deref() == Some(delete_bg.as_str())),
            "the removed loop is painted with the theme's delete colour"
        );
        unsafe { std::env::remove_var(theme::CONFIG_ENV) };
        Ok(())
    }

    #[test]
    fn overlay_themes_resolve_by_label_or_variant_name() {
        assert_eq!(
            overlay_theme_named("Solarized Light"),
            Some(OverlayTheme::SolarizedLight)
        );
        assert_eq!(
            overlay_theme_named("solarizedlight"),
            Some(OverlayTheme::SolarizedLight)
        );
        assert_eq!(overlay_theme_named("Nord"), Some(OverlayTheme::Nord));
        assert_eq!(overlay_theme_named("no such theme"), None);
    }

    #[test]
    fn indexed_colours_follow_the_xterm_table() {
        assert_eq!(hex(Color::Indexed(1)).as_deref(), Some("#cd0000"));
        assert_eq!(hex(Color::Indexed(16)).as_deref(), Some("#000000"));
        assert_eq!(hex(Color::Indexed(231)).as_deref(), Some("#ffffff"));
        assert_eq!(hex(Color::Indexed(232)).as_deref(), Some("#080808"));
        assert_eq!(hex(Color::Reset), None);
    }
}
