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
pub mod actions;
pub mod app;
pub mod components;
pub mod events;
pub mod headless;
pub mod json_output;
pub mod positional;
pub mod screenshot;
pub mod theme;
pub mod ui;
pub mod widgets;

use anyhow::{Context, Result};
use std::path::PathBuf;
use tracing_subscriber::{EnvFilter, Layer, fmt, prelude::*};

/// Where bug reports go; printed by the panic hook and the `?` About text.
pub const ISSUE_TRACKER_URL: &str = "https://github.com/ivankovic/codediff/issues";

/// The environment variable that turns logging on. Its value is the `tracing` filter directive
/// (`info`, `codediff=debug`, ...).
pub const LOG_ENV: &str = "RUST_LOG";

/// Logs to a file, since the terminal is used by the TUI, and only when [`LOG_ENV`] is set: a
/// viewer that silently writes a file on every start is a surprise nobody asked for. The file is
/// truncated on each start.
pub fn initialize_logging() -> Result<()> {
    if std::env::var_os(LOG_ENV).is_none_or(|value| value.is_empty()) {
        return Ok(());
    }

    let path = log_file_path();
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory)
            .with_context(|| format!("failed to create {}", directory.display()))?;
    }
    let log_file = std::fs::File::create(&path)
        .with_context(|| format!("failed to create the log file {}", path.display()))?;

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

/// The log file: `$XDG_STATE_HOME/codediff/log.txt`, else `$HOME/.local/state/codediff/log.txt`
/// (the XDG state directory, where logs belong), else the system temp directory. Per user, never
/// a world-shared path: a shared `/tmp/codediff` is another user's to create first, and to point
/// a symlink from.
pub fn log_file_path() -> PathBuf {
    log_file_path_from(
        std::env::var_os("XDG_STATE_HOME").map(PathBuf::from),
        std::env::var_os("HOME").map(PathBuf::from),
        std::env::temp_dir(),
    )
}

fn log_file_path_from(
    xdg_state_home: Option<PathBuf>,
    home: Option<PathBuf>,
    temp_dir: PathBuf,
) -> PathBuf {
    let base = match (xdg_state_home, home) {
        (Some(xdg), _) if !xdg.as_os_str().is_empty() => xdg,
        (_, Some(home)) if !home.as_os_str().is_empty() => home.join(".local").join("state"),
        _ => temp_dir,
    };
    base.join("codediff").join("log.txt")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_file_prefers_xdg_state_home_then_home_then_the_temp_dir() {
        let temp = PathBuf::from("/tmp");
        assert_eq!(
            log_file_path_from(Some("/xdg".into()), Some("/home/u".into()), temp.clone()),
            PathBuf::from("/xdg/codediff/log.txt")
        );
        assert_eq!(
            log_file_path_from(None, Some("/home/u".into()), temp.clone()),
            PathBuf::from("/home/u/.local/state/codediff/log.txt")
        );
        assert_eq!(
            log_file_path_from(Some("".into()), Some("".into()), temp),
            PathBuf::from("/tmp/codediff/log.txt")
        );
    }
}
