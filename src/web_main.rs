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
//! `codediff-web`: the interactive viewer in a browser tab instead of a terminal. Everything
//! after argument parsing is `codediff::web`; this file is the command line and the browser
//! launch.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

use codediff::diff::text::RenderOptions;
use codediff::tui::positional::resolve_before_after;
use codediff::web::server::Server;
use codediff::web::session::Session;

#[derive(Parser)]
#[command(
    name = "codediff-web",
    about = "The codediff viewer, served to a local browser",
    long_about = "Fast, robust, syntax-aware code diffing, in a browser.\n\n\
        Starts a local web server and opens the viewer in your browser. With no arguments the \
        viewer starts empty, with BEFORE and AFTER it opens directly into their diff. Every key \
        the terminal UI understands works in the page (press ? there for the list); q in the \
        page, or Ctrl-C here, stops the server. Also accepts git's GIT_EXTERNAL_DIFF argument \
        list, like codediff itself.",
    version
)]
struct Args {
    /// The files to diff: `BEFORE AFTER`, or git's 7- or 9-argument `GIT_EXTERNAL_DIFF` list.
    paths: Vec<PathBuf>,

    /// Start with the render-options panel's "everything off" preset instead of the persisted
    /// setting. The panel (`M` in the page) can change it afterwards, as in the TUI.
    #[arg(long, conflicts_with = "full")]
    minimal: bool,

    /// Start with the "everything on" preset instead of the persisted setting.
    #[arg(long)]
    full: bool,

    /// Start with whole-pair updates on (see `codediff --help`).
    #[arg(long)]
    whole_updates: bool,

    /// Start with reindent-only moves painted (see `codediff --help`).
    #[arg(long)]
    paint_reindent_moves: bool,

    /// The address to listen on. Anything but a loopback address exposes your files to whoever
    /// can reach it; the page's own protections assume one browser on one machine.
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// The port to listen on; 0 lets the system pick a free one, printed at startup.
    #[arg(long, default_value_t = 0)]
    port: u16,

    /// Print the URL but do not open a browser.
    #[arg(long)]
    no_open: bool,

    /// Open on the git review picker - the repository around the current directory's unstaged
    /// files, staged files and recent commits - instead of an empty viewer (the page's `G` key,
    /// at startup).
    #[arg(long)]
    review: bool,
}

/// The initial render options: a preset when asked for one, otherwise whatever the `M` panel
/// last persisted, with the two single-option flags layered on top either way.
fn initial_render_options(args: &Args) -> Option<RenderOptions> {
    let preset = if args.minimal {
        Some(RenderOptions::MINIMAL)
    } else if args.full {
        Some(RenderOptions::FULL)
    } else {
        None
    };
    if !args.whole_updates && !args.paint_reindent_moves {
        return preset;
    }
    let mut options = preset.unwrap_or_else(codediff::tui::theme::load_render_options);
    options.whole_pair_updates |= args.whole_updates;
    options.paint_reindent_only_moves |= args.paint_reindent_moves;
    Some(options)
}

/// Opens `url` with the platform's opener, detached. Failure is reported and otherwise ignored:
/// the URL was printed, and a machine without a browser (SSH, a container) is a normal place to
/// run this with `--no-open`.
fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = std::process::Command::new("open");
        c.arg(url);
        c
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", "", url]);
        c
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(url);
        c
    };
    let spawned = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    if let Err(err) = spawned {
        eprintln!("codediff-web: could not open a browser ({err}); open {url} yourself");
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let pair = resolve_before_after(&args.paths)?;

    // Same door as `codediff`: a side that is not text has no diff to show in any mode, so say
    // so on stdout and leave, rather than opening a browser onto an error banner.
    if let Some((before, after)) = pair.as_ref() {
        let either_is_binary = codediff::code::is_binary_file(before)
            .and_then(|binary| Ok(binary || codediff::code::is_binary_file(after)?));
        match either_is_binary {
            Ok(true) => {
                let differed = std::fs::read(before)? != std::fs::read(after)?;
                let (before, after) = (before.display(), after.display());
                match differed {
                    true => println!("Binary files {before} and {after} differ"),
                    false => println!("Binary files {before} and {after} are identical"),
                }
                return Ok(());
            }
            Ok(false) => {}
            Err(e) => {
                eprintln!("codediff-web: {e:#}");
                std::process::exit(2);
            }
        }
    }

    let mut session = Session::from_config(initial_render_options(&args));
    if let Some((before, after)) = pair {
        session.set_pair(before, after);
    }
    if args.review {
        session.set_review_on_start();
    }
    let server = Server::bind(&args.host, args.port, session).await?;
    let url = server.url();
    println!("codediff-web: serving on {url}");
    println!("codediff-web: press q in the page, or Ctrl-C here, to stop");
    if !args.no_open {
        open_browser(&url);
    }
    server.run().await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(extra: &[&str]) -> Args {
        Args::parse_from(std::iter::once("codediff-web").chain(extra.iter().copied()))
    }

    #[test]
    fn no_flags_means_the_persisted_options() {
        assert_eq!(initial_render_options(&args(&[])), None);
    }

    #[test]
    fn a_preset_flag_is_the_preset() {
        assert_eq!(
            initial_render_options(&args(&["--minimal"])),
            Some(RenderOptions::MINIMAL)
        );
        assert_eq!(
            initial_render_options(&args(&["--full"])),
            Some(RenderOptions::FULL)
        );
    }

    #[test]
    fn the_single_option_flags_layer_onto_a_preset() {
        let options = initial_render_options(&args(&["--minimal", "--whole-updates"])).unwrap();
        assert!(options.whole_pair_updates);
        assert!(
            !options.leading_whitespace,
            "the rest of the preset is kept"
        );
        let options = initial_render_options(&args(&["--paint-reindent-moves"])).unwrap();
        assert!(options.paint_reindent_only_moves);
    }

    #[test]
    fn minimal_and_full_conflict() {
        assert!(Args::try_parse_from(["codediff-web", "--minimal", "--full"]).is_err());
    }

    #[test]
    fn git_argument_shapes_are_accepted() {
        let seven = args(&["p", "old", "h", "m", "new", "h", "m"]);
        assert_eq!(
            resolve_before_after(&seven.paths).unwrap(),
            Some((PathBuf::from("old"), PathBuf::from("new")))
        );
    }
}
