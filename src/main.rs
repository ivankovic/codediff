use anyhow::Result;
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
use clap::Parser;

use codediff::tui;

#[derive(Parser)]
struct Args {
    /// Mode (TUI/tui or Headless/headless)
    #[arg(long, default_value = "TUI")]
    mode: String,

    /// Shorthand for "mode=headless"
    #[arg(long)]
    headless: bool,

    /// Tick rate
    #[arg(long, value_name = "FLOAT", default_value_t = 4.0)]
    tui_tick_rate: f64,

    /// Frame rate, frames per second, fps
    #[arg(long, value_name = "FLOAT", default_value_t = 60.0)]
    tui_frame_rate: f64,
}

async fn tui_main() -> Result<()> {
    tui::initialize_logging()?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if args.headless || args.mode.to_lowercase() == "headless" {
        unimplemented!("TODO: implement headless mode");
    } else if let Err(e) = tui_main().await {
        eprintln!("something went wrong");
        return Err(e);
    }

    Ok(())
}
