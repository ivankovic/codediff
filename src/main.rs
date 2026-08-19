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
use clap::{Parser, Subcommand};

use codediff::diff::DiffMode;
use codediff::tui;

mod git_configure;

/// Top-level subcommands. Optional and coexists with `Args::paths` below (clap resolves this
/// correctly: `codediff a.rs b.rs` still parses as two paths, not an attempt at the `git`
/// subcommand - only `codediff git ...` is - see `main.rs`'s own tests) - `git` becomes a
/// reserved word for the first positional the same way `add`/`commit`/etc. are for git itself,
/// an accepted tradeoff for the vanishingly rare case of a file literally named `git`.
#[derive(Subcommand)]
enum Command {
    /// git integration helpers.
    Git {
        #[command(subcommand)]
        action: GitAction,
    },
}

#[derive(Subcommand)]
enum GitAction {
    /// Interactively configure codediff as git's diff tool - the `git config` commands README's
    /// "Git integration" section otherwise asks you to run by hand.
    Configure,
}

/// When headless output should be ANSI-colored - see `use_color` for how `Auto` resolves.
#[derive(clap::ValueEnum, Clone, Copy, PartialEq, Eq, Debug, Default)]
enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Parser)]
#[command(
    after_help = "Exit codes (headless/JSON modes, direct BEFORE AFTER invocation): \
    0 = no differences, 1 = differences found, 2 = error - same convention as diff(1). \
    The 7-argument GIT_EXTERNAL_DIFF form always exits 0 on success instead, because git \
    treats a non-zero exit from an external diff program as a fatal error, not as \"files \
    differ\". The interactive TUI always exits 0."
)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    /// The files to diff, positionally. Two forms are accepted (see `resolve_before_after`):
    /// `BEFORE AFTER` (also what `git difftool` invokes a `difftool.<tool>.cmd` of
    /// `codediff "$LOCAL" "$REMOTE"` with), or git's 7-argument `GIT_EXTERNAL_DIFF` convention
    /// (`path old-file old-hex old-mode new-file new-hex new-mode`). If either is given, the diff
    /// is computed immediately instead of starting on an empty viewer.
    paths: Vec<PathBuf>,

    /// Mode (TUI/tui, Headless/headless, or Json/json - see `tui::json_output` for the JSON
    /// schema). `json` needs BEFORE and AFTER, same as `headless`; unlike `headless`, it is never
    /// entered implicitly just because stdout isn't a terminal.
    #[arg(long, default_value = "TUI")]
    mode: String,

    /// Shorthand for "mode=headless". `--batch` is a synonym - the two names people reach for
    /// depend on which non-interactive use case brought them here (git integration/piping vs.
    /// scripted/CI invocation), but they mean the same thing.
    #[arg(long, alias = "batch")]
    headless: bool,

    /// Deprecated no-op, kept so existing scripts don't break: since the phases-4-7 pipeline
    /// rearchitecture, the diff pipeline runs the same bounded, region-scoped analysis
    /// regardless of mode (`PendingDiff::finish` ignores `DiffMode` - see its phase-6 comment),
    /// so there is no separate "exact" path for this flag to select anymore. Currently its only
    /// observable effect is suppressing headless mode's large-residual note. Slated for removal
    /// together with `DiffMode` itself once the pipeline cleanup that owns that decision lands.
    #[arg(long)]
    exact: bool,

    /// When to ANSI-color headless output: `always`, `never`, or `auto` (the default - color on
    /// unless the `NO_COLOR` environment variable is set; see `use_color` for why auto is not
    /// tied to stdout being a terminal). The flag beats the environment variable in both
    /// directions: `--color always` colors even under `NO_COLOR`, `--color never` suppresses
    /// color without needing the variable.
    #[arg(long, value_enum, value_name = "WHEN", default_value_t = ColorChoice::Auto)]
    color: ColorChoice,

    /// How many unchanged lines to keep around each change in headless output before collapsing
    /// the rest into an elision marker - same idea as `diff -U N`.
    #[arg(long, value_name = "N", default_value_t = tui::headless::CONTEXT_LINES)]
    context: usize,

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

/// Whether to print a single JSON diff object (`tui::json_output::run`) instead of anything else.
/// Unlike `should_run_headless`, this is opt-in only via `--mode json` - a non-terminal stdout
/// (git's pager, a redirect, CI) must still default to `headless`'s human-readable ANSI text, not
/// silently switch formats just because it isn't a terminal, since that would break every existing
/// `GIT_EXTERNAL_DIFF`/script integration that isn't asking for JSON.
fn should_run_json(args: &Args) -> bool {
    args.mode.eq_ignore_ascii_case("json")
}

/// Whether headless output should be ANSI-colored. `Auto` is deliberately not tied to
/// `stdout_is_terminal`: the whole point of headless mode's main use case (`GIT_EXTERNAL_DIFF`
/// under git's default pager) is that stdout is a pipe, not a terminal, yet the pager on the
/// other end is generally perfectly capable of showing color (git configures `less` with
/// `-R`-equivalent behavior for exactly this reason). Under `Auto`, the `NO_COLOR` convention
/// (<https://no-color.org>) is the escape hatch for callers that don't want that - e.g.
/// redirecting to a file; `--color always`/`--color never` beat the environment variable in
/// either direction.
fn use_color(choice: ColorChoice) -> bool {
    match choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => std::env::var_os("NO_COLOR").is_none(),
    }
}

/// Turns a completed non-interactive run into a `diff`-style process exit code: 0 = no
/// differences, 1 = differences found (errors exit 2, handled at the call sites). Only for the
/// direct `BEFORE AFTER` calling convention: git treats a non-zero exit from a
/// `GIT_EXTERNAL_DIFF` program as a fatal error ("external diff died"), not as "files differ",
/// so the 7-argument git form (recognized by `invoked_as_git_external_diff`) always maps to 0.
fn exit_code_for(differed: bool, invoked_as_git_external_diff: bool) -> i32 {
    if differed && !invoked_as_git_external_diff {
        1
    } else {
        0
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if let Some(Command::Git {
        action: GitAction::Configure,
    }) = &args.command
    {
        return git_configure::run();
    }

    let before_after = resolve_before_after(&args.paths)?;
    let invoked_as_git_external_diff = args.paths.len() == 7;

    if should_run_json(&args) {
        let (before, after) = before_after
            .context("`--mode json` needs BEFORE and AFTER - pass two files to diff")?;
        let mode = if args.exact {
            DiffMode::Exact
        } else {
            DiffMode::Fast
        };
        match tui::json_output::run(&before, &after, mode) {
            Ok(differed) => {
                std::process::exit(exit_code_for(differed, invoked_as_git_external_diff))
            }
            Err(e) => {
                eprintln!("codediff: {e:#}");
                std::process::exit(2);
            }
        }
    }

    if should_run_headless(&args, std::io::stdout().is_terminal()) {
        let (before, after) = before_after.context(
            "stdout is not a terminal and no files were given - pass BEFORE and AFTER to run in \
            text mode, or run from a real terminal to use the interactive viewer",
        )?;
        let mode = if args.exact {
            DiffMode::Exact
        } else {
            DiffMode::Fast
        };
        match tui::headless::run(&before, &after, use_color(args.color), mode, args.context) {
            Ok(differed) => {
                std::process::exit(exit_code_for(differed, invoked_as_git_external_diff))
            }
            Err(e) => {
                eprintln!("codediff: {e:#}");
                std::process::exit(2);
            }
        }
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
            Some((
                PathBuf::from("/dev/null"),
                PathBuf::from("/tmp/git-blob-BBBB/foo.rs")
            ))
        );
    }

    fn args_with(mode: &str, headless: bool) -> Args {
        Args {
            command: None,
            paths: Vec::new(),
            mode: mode.to_string(),
            headless,
            exact: false,
            color: ColorChoice::Auto,
            context: tui::headless::CONTEXT_LINES,
            tui_tick_rate: 4.0,
            tui_frame_rate: 60.0,
        }
    }

    #[test]
    fn color_flag_parses_all_three_choices_and_defaults_to_auto() {
        for (argv, expected) in [
            (vec!["codediff"], ColorChoice::Auto),
            (vec!["codediff", "--color", "always"], ColorChoice::Always),
            (vec!["codediff", "--color", "never"], ColorChoice::Never),
            (vec!["codediff", "--color", "auto"], ColorChoice::Auto),
        ] {
            let args = Args::try_parse_from(argv.clone())
                .unwrap_or_else(|e| panic!("{argv:?} should parse: {e}"));
            assert_eq!(args.color, expected, "for {argv:?}");
        }
    }

    #[test]
    fn context_flag_defaults_to_the_headless_default_and_accepts_overrides() {
        assert_eq!(
            Args::try_parse_from(["codediff"]).unwrap().context,
            tui::headless::CONTEXT_LINES
        );
        assert_eq!(
            Args::try_parse_from(["codediff", "--context", "0"])
                .unwrap()
                .context,
            0
        );
    }

    #[test]
    fn always_and_never_beat_the_no_color_environment_variable() {
        // `use_color` never reads the environment for Always/Never, so this needs no env
        // manipulation (which would race other tests in the same process).
        assert!(use_color(ColorChoice::Always));
        assert!(!use_color(ColorChoice::Never));
    }

    #[test]
    fn exit_code_is_diff_style_for_direct_invocations_only() {
        assert_eq!(exit_code_for(false, false), 0);
        assert_eq!(exit_code_for(true, false), 1);
        // The GIT_EXTERNAL_DIFF form must always exit 0 - git treats non-zero as fatal.
        assert_eq!(exit_code_for(true, true), 0);
        assert_eq!(exit_code_for(false, true), 0);
    }

    #[test]
    fn git_configure_parses_as_a_subcommand() {
        let args = Args::try_parse_from(["codediff", "git", "configure"]).unwrap();
        assert!(matches!(
            args.command,
            Some(Command::Git {
                action: GitAction::Configure
            })
        ));
        assert!(args.paths.is_empty());
    }

    /// Regression guard for the `Option<Command>` + `Vec<PathBuf>` coexistence this binary
    /// relies on: an ordinary two-file diff must still parse as `paths`, not get swallowed
    /// attempting to match the `git` subcommand (clap resolves this correctly on its own, but
    /// nothing else in this file would catch it silently breaking on a future clap upgrade).
    #[test]
    fn two_paths_still_parse_as_paths_not_a_subcommand() {
        let args = Args::try_parse_from(["codediff", "a.rs", "b.rs"]).unwrap();
        assert!(args.command.is_none());
        assert_eq!(
            args.paths,
            vec![PathBuf::from("a.rs"), PathBuf::from("b.rs")]
        );
    }

    #[test]
    fn no_args_still_opens_empty_viewer() {
        let args = Args::try_parse_from(["codediff"]).unwrap();
        assert!(args.command.is_none());
        assert!(args.paths.is_empty());
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
        assert!(
            args.headless,
            "--batch should set the same field as --headless"
        );
    }

    #[test]
    fn should_run_headless_honors_mode_headless_case_insensitively() {
        assert!(should_run_headless(&args_with("Headless", false), true));
    }

    #[test]
    fn should_run_json_honors_mode_json_case_insensitively() {
        assert!(should_run_json(&args_with("Json", false)));
        assert!(should_run_json(&args_with("json", false)));
        assert!(!should_run_json(&args_with("TUI", false)));
        assert!(!should_run_json(&args_with("Headless", false)));
    }

    #[test]
    fn should_run_json_is_false_on_a_non_terminal_stdout_unless_explicitly_asked_for() {
        // Unlike should_run_headless, a piped/redirected stdout must not silently switch to JSON
        // - only an explicit `--mode json` does that (see should_run_json's own doc comment).
        assert!(!should_run_json(&args_with("TUI", false)));
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
