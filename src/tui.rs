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
pub mod app;
pub mod events;

use anyhow::Result;
use std::path::Path;
use tracing_subscriber::{EnvFilter, Layer, fmt, prelude::*};

/// Log to a file since the terminal is used by the TUI.
pub fn initialize_logging() -> Result<()> {
    let path = Path::new("/tmp/codediff/log.txt");
    let log_file = std::fs::File::create(path)?;

    let env_filter = EnvFilter::builder()
        .with_default_directive(tracing::Level::INFO.into())
        .from_env_lossy();

    let file_subscriber = fmt::layer()
        .with_file(true)
        .with_line_number(true)
        .with_writer(log_file)
        .with_target(false)
        .with_ansi(false)
        .with_filter(env_filter);

    tracing_subscriber::registry()
        .with(file_subscriber)
        .with(tracing_error::ErrorLayer::default())
        .try_init()?;

    Ok(())
}
