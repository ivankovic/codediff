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
use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;

use codediff::diff::DiffMode;
use codediff::tui;

#[derive(Parser)]
struct Args {
    /// The files to diff, positionally. Two forms are accepted (see `resolve_before_after`):
    /// `BEFORE AFTER` (also what `git difftool` invokes a `difftool.<tool>.cmd` of
    /// `codediff "$LOCAL" "$REMOTE"` with), or git's 7-argument `GIT_EXTERNAL_DIFF` convention
    /// (`path old-file old-hex old-mode new-file new-hex new-mode`). If either is given, the diff
    /// is computed immediately instead of starting on an empty viewer.
    paths: Vec<PathBuf>,

    /// Mode (TUI/tui or Headless/headless)
    #[arg(long, default_value = "TUI")]
    mode: String,

    /// Shorthand for "mode=headless". `--batch` is a synonym - the two names people reach for
    /// depend on which non-interactive use case brought them here (git integration/piping vs.
    /// scripted/CI invocation), but they mean the same thing.
    #[arg(long, alias = "batch")]
    headless: bool,

    /// Always run full (exact) tree-edit-distance analysis, even when the residual after the
    /// cheap heuristic passes is large enough that the default fast mode would otherwise
    /// silently substitute a cheaper, less precise fallback for it. Batch/headless only - the
    /// interactive TUI always offers this choice per-diff instead (see `SelectDiffMode`), rather
    /// than fixing it for the whole run via a flag.
    #[arg(long)]
    exact: bool,

    /// Tick rate
    #[arg(long, value_name = "FLOAT", default_value_t = 4.0)]
    tui_tick_rate: f64,

    /// Frame rate, frames per second, fps
    #[arg(long, value_name = "FLOAT", default_value_t = 60.0)]
    tui_frame_rate: f64,
}

/// Resolves the raw positional arguments into a `(before, after)` pair, or `None` if the viewer
/// should start empty. Two calling conventions are supported, disambiguated purely by count:
///
/// * **2 args**: `BEFORE AFTER` directly - today's plain CLI usage, and also what `git difftool`
///   invokes via `difftool.<tool>.cmd = codediff "$LOCAL" "$REMOTE"` (see README's "Git
///   integration" section). No further translation is needed: git already substitutes `$LOCAL`/
///   `$REMOTE` with real file paths (temp copies for blobs, the real working-tree file otherwise)
///   that keep the original extension, so language detection just works.
/// * **7 args**: git's `GIT_EXTERNAL_DIFF` convention, `path old-file old-hex old-mode new-file
///   new-hex new-mode` (see `git help diff` under `GIT_EXTERNAL_DIFF`). Only `old-file` (index 1)
///   and `new-file` (index 4) matter here - the hex/mode fields describe blob identity/perms that
///   codediff has no use for, and `path` (index 0, the logical file path, identical for both
///   sides) isn't needed either since `old-file`/`new-file` already carry the real extension
///   themselves. An add/delete is represented by `old-file`/`new-file` being the literal path
///   `/dev/null`; `compute_diff` (`tui/app.rs`) handles that case specially.
///
/// Any other count is almost certainly a mistake (most likely `GIT_EXTERNAL_DIFF` being invoked
/// with an unexpected git version's argument list) and is rejected with an explanatory error
/// rather than silently misinterpreted.
fn resolve_before_after(paths: &[PathBuf]) -> Result<Option<(PathBuf, PathBuf)>> {
    match paths.len() {
        0 => Ok(None),
        2 => Ok(Some((paths[0].clone(), paths[1].clone()))),
        7 => Ok(Some((paths[1].clone(), paths[4].clone()))),
        n => anyhow::bail!(
            "expected 0 positional arguments (empty viewer), 2 (BEFORE AFTER), or 7 \
            (GIT_EXTERNAL_DIFF's `path old-file old-hex old-mode new-file new-hex new-mode`), \
            got {n}"
        ),
    }
}

async fn tui_main(args: &Args, before_after: Option<(PathBuf, PathBuf)>) -> Result<()> {
    tui::initialize_logging()?;

    let mut app = tui::app::App::new(args.tui_tick_rate, args.tui_frame_rate)?;
    if let Some((before, after)) = before_after {
        app.open_files(before, after)?;
    }
    app.run().await?;

    Ok(())
}

/// Whether to run headless (print text, `tui::headless::run`) instead of starting the
/// interactive TUI: either the caller explicitly asked for it (`--headless`/`--mode headless`),
/// or `stdout_is_terminal` is false. The latter is what makes `GIT_EXTERNAL_DIFF` usable without
/// extra configuration (see `SPECS.md`'s "Git integration" entry): git's default pager leaves our
/// stdout connected to a pipe, not a real terminal, and a full-screen TUI cannot draw onto a pipe
/// regardless of *why* it isn't a terminal - the same fallback also covers `codediff a b > out`,
/// CI, or any other non-interactive invocation, not just this one git-specific scenario.
fn should_run_headless(args: &Args, stdout_is_terminal: bool) -> bool {
    args.headless || args.mode.eq_ignore_ascii_case("headless") || !stdout_is_terminal
}

/// Whether headless output should be ANSI-colored. Deliberately not tied to `stdout_is_terminal`:
/// the whole point of headless mode's main use case (`GIT_EXTERNAL_DIFF` under git's default
/// pager) is that stdout is a pipe, not a terminal, yet the pager on the other end is generally
/// perfectly capable of showing color (git configures `less` with `-R`-equivalent behavior for
/// exactly this reason). Respects the `NO_COLOR` convention (<https://no-color.org>) as the escape
/// hatch for callers that don't want that - e.g. redirecting to a file.
fn use_color() -> bool {
    std::env::var_os("NO_COLOR").is_none()
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let before_after = resolve_before_after(&args.paths)?;

    if should_run_headless(&args, std::io::stdout().is_terminal()) {
        let (before, after) = before_after.context(
            "stdout is not a terminal and no files were given - pass BEFORE and AFTER to run in \
            text mode, or run from a real terminal to use the interactive viewer",
        )?;
        let mode = if args.exact { DiffMode::Exact } else { DiffMode::Fast };
        return tui::headless::run(&before, &after, use_color(), mode);
    }

    if let Err(e) = tui_main(&args, before_after).await {
        eprintln!("something went wrong");
        return Err(e);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_before_after_with_no_args_starts_an_empty_viewer() {
        assert_eq!(resolve_before_after(&[]).unwrap(), None);
    }

    #[test]
    fn resolve_before_after_with_two_args_is_before_after_directly() {
        let paths = vec![PathBuf::from("before.rs"), PathBuf::from("after.rs")];
        assert_eq!(
            resolve_before_after(&paths).unwrap(),
            Some((PathBuf::from("before.rs"), PathBuf::from("after.rs")))
        );
    }

    /// The `GIT_EXTERNAL_DIFF` convention: `path old-file old-hex old-mode new-file new-hex
    /// new-mode`. `old-file`/`new-file` (indices 1 and 4) become before/after; `path` and the
    /// hex/mode fields are ignored.
    #[test]
    fn resolve_before_after_with_seven_args_picks_out_old_file_and_new_file() {
        let paths = vec![
            PathBuf::from("src/foo.rs"),
            PathBuf::from("/tmp/git-blob-AAAA/foo.rs"),
            PathBuf::from("abc123"),
            PathBuf::from("100644"),
            PathBuf::from("/tmp/git-blob-BBBB/foo.rs"),
            PathBuf::from("def456"),
            PathBuf::from("100644"),
        ];
        assert_eq!(
            resolve_before_after(&paths).unwrap(),
            Some((
                PathBuf::from("/tmp/git-blob-AAAA/foo.rs"),
                PathBuf::from("/tmp/git-blob-BBBB/foo.rs")
            ))
        );
    }

    /// `GIT_EXTERNAL_DIFF` represents an added or deleted file with `old-file`/`new-file` set to
    /// `/dev/null` - `resolve_before_after` just passes it through; `compute_diff` is what
    /// actually special-cases it.
    #[test]
    fn resolve_before_after_with_seven_args_passes_dev_null_through_for_add_delete() {
        let paths = vec![
            PathBuf::from("src/foo.rs"),
            PathBuf::from("/dev/null"),
            PathBuf::from("."),
            PathBuf::from("."),
            PathBuf::from("/tmp/git-blob-BBBB/foo.rs"),
            PathBuf::from("def456"),
            PathBuf::from("100644"),
        ];
        assert_eq!(
            resolve_before_after(&paths).unwrap(),
            Some((PathBuf::from("/dev/null"), PathBuf::from("/tmp/git-blob-BBBB/foo.rs")))
        );
    }

    fn args_with(mode: &str, headless: bool) -> Args {
        Args {
            paths: Vec::new(),
            mode: mode.to_string(),
            headless,
            exact: false,
            tui_tick_rate: 4.0,
            tui_frame_rate: 60.0,
        }
    }

    #[test]
    fn should_run_headless_when_stdout_is_not_a_terminal_even_without_any_flag() {
        assert!(should_run_headless(&args_with("TUI", false), false));
    }

    #[test]
    fn should_run_headless_is_false_on_a_real_terminal_with_no_flags() {
        assert!(!should_run_headless(&args_with("TUI", false), true));
    }

    #[test]
    fn should_run_headless_honors_the_headless_flag_even_on_a_real_terminal() {
        assert!(should_run_headless(&args_with("TUI", true), true));
    }

    /// Exercises real clap parsing (unlike `args_with`, which builds `Args` directly) - `alias`
    /// is a clap-level detail that a struct built by hand can't accidentally get wrong, so this
    /// is the only test that would actually catch `--batch` silently not working.
    #[test]
    fn batch_flag_is_a_clap_alias_for_headless() {
        let args = Args::try_parse_from(["codediff", "--batch"]).expect("--batch should parse");
        assert!(args.headless, "--batch should set the same field as --headless");
    }

    #[test]
    fn should_run_headless_honors_mode_headless_case_insensitively() {
        assert!(should_run_headless(&args_with("Headless", false), true));
    }

    #[test]
    fn resolve_before_after_rejects_any_other_count() {
        for n in [1, 3, 4, 5, 6, 8] {
            let paths: Vec<PathBuf> = (0..n).map(|i| PathBuf::from(format!("p{i}"))).collect();
            assert!(
                resolve_before_after(&paths).is_err(),
                "{n} positional arguments should be rejected"
            );
        }
    }
}
