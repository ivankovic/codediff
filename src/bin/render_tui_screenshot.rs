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
//! Draws the viewer into an offscreen terminal and prints it as JSON, one styled run at a time,
//! for `scripts/render_tui_screenshot.py` to rasterize. See `tui::screenshot`.
//!
//!     make readme-screenshot

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Parser;

use codediff::tui::screenshot::{overlay_theme_named, render};
use codediff::tui::theme;
use codediff::tui::widgets::code_viewer::syntax_theme_names;

#[derive(Parser)]
#[command(about = "Render the viewer offscreen and print its styled cells as JSON")]
struct Args {
    before: PathBuf,
    after: PathBuf,
    /// Terminal width. Two panels need at least 220 columns.
    #[arg(long, default_value_t = 230)]
    cols: u16,
    /// Terminal height.
    #[arg(long, default_value_t = 36)]
    rows: u16,
    /// Overlay theme, by its picker label or variant name.
    #[arg(long, default_value = "Solarized Light")]
    theme: String,
    /// Syntax highlighting theme, by its syntect name.
    #[arg(long, default_value = "Solarized (light)")]
    syntax_theme: String,
    /// Where to write the JSON; stdout if omitted.
    #[arg(long)]
    out: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let overlay = overlay_theme_named(&args.theme)
        .with_context(|| format!("unknown overlay theme {:?}", args.theme))?;
    let syntax_themes = syntax_theme_names();
    if !syntax_themes.iter().any(|name| name == &args.syntax_theme) {
        bail!(
            "unknown syntax theme {:?}; one of: {}",
            args.syntax_theme,
            syntax_themes.join(", ")
        );
    }

    // Opening a pair records it in the config, like the live viewer. Keep that out of the
    // caller's real config unless they pointed at one themselves.
    let scratch_config =
        std::env::temp_dir().join(format!("codediff-screenshot-{}.toml", std::process::id()));
    if std::env::var_os(theme::CONFIG_ENV).is_none() {
        // SAFETY: before any other thread exists.
        unsafe { std::env::set_var(theme::CONFIG_ENV, &scratch_config) };
    }

    let shot = render(
        &args.before,
        &args.after,
        args.cols,
        args.rows,
        overlay,
        Some(&args.syntax_theme),
    );
    let _ = std::fs::remove_file(&scratch_config);
    let json = serde_json::to_string(&shot?)?;
    match args.out {
        Some(path) => {
            std::fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?
        }
        None => println!("{json}"),
    }
    Ok(())
}
