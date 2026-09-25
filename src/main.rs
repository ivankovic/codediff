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
use anyhow::Result;
use clap::{Parser, Subcommand};

use codediff::tui;
use codediff::tui::positional::{invoked_as_git_external_diff, resolve_before_after};

mod configure_prompt;
mod git_configure;
mod jj_configure;

/// The static musl builds only: musl's malloc costs them 20-30% on the largest files, and
/// mimalloc brings the diff-bound ones ahead of the glibc builds. See Cargo.toml's
/// target-specific dependency for the measurement.
#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// Top-level subcommands, coexisting with `Args::paths`: each subcommand name becomes a reserved
/// word for the first positional path, so a file literally named `git` cannot be diffed directly.
#[derive(Subcommand)]
enum Command {
    /// git integration helpers.
    Git {
        #[command(subcommand)]
        action: GitAction,
    },
    /// Jujutsu (jj) integration helpers.
    Jj {
        #[command(subcommand)]
        action: JjAction,
    },
    /// Generate packaging artifacts (shell completions, man page).
    // Nested under `util` so only one implausible filename is reserved, not `completions` and `man`.
    Util {
        #[command(subcommand)]
        action: UtilAction,
    },
}

#[derive(Subcommand)]
enum UtilAction {
    /// Print a shell completion script for SHELL on stdout.
    ///
    /// For example: `source <(codediff util completions bash)`.
    Completions {
        /// The shell to generate for.
        shell: clap_complete::Shell,
    },
    /// Print a roff-formatted man page (section 1) on stdout.
    Man,
}

#[derive(Subcommand)]
enum GitAction {
    /// Interactively configure codediff as git's diff tool.
    Configure,
}

#[derive(Subcommand)]
enum JjAction {
    /// Interactively configure codediff as jj's diff tool.
    Configure,
}

/// When headless output should be ANSI-colored; see `use_color`.
#[derive(clap::ValueEnum, Clone, Copy, PartialEq, Eq, Debug, Default)]
enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

/// How the diff is shown; see `should_run_headless` and `should_run_json`.
#[derive(clap::ValueEnum, Clone, Copy, PartialEq, Eq, Debug, Default)]
enum Mode {
    /// The interactive viewer.
    #[default]
    Tui,
    /// Plain text on stdout.
    Headless,
    /// One JSON object on stdout, for editor integrations.
    Json,
}

/// The exit status of a failed run: what `--help` documents, and distinct from `--exit-code`'s 1.
const EXIT_FAILURE: i32 = 2;

/// A rate for the TUI's tick and render intervals: `Duration::from_secs_f64(1.0 / rate)` panics
/// on zero, a negative or NaN, so those are rejected as arguments.
fn parse_positive_rate(value: &str) -> Result<f64, String> {
    let rate: f64 = value.parse().map_err(|e| format!("{e}"))?;
    if rate.is_finite() && rate > 0.0 {
        Ok(rate)
    } else {
        Err("must be a positive number".to_string())
    }
}

#[derive(Parser)]
#[command(
    // Explicit: without it clap's derive takes the nearest preceding doc comment (`Command`'s)
    // as the description in `--help` and the man page.
    about = "Fast, robust, syntax-aware code diffing using tree-sitter ASTs",
    long_about = "Fast, robust, syntax-aware code diffing.\n\n\
        With no arguments, opens an interactive two-panel terminal UI. With BEFORE and AFTER \
        file paths, diffs them directly - as the TUI, as plain text (--headless), or as a single \
        JSON object (--mode json) for editor integrations. Also serves as a `git difftool` \
        backend and a `jj` diff formatter; see `codediff git configure` and `codediff jj \
        configure`.",
    version,
    after_help = "Exit codes: 0 on success, 2 on error. Pass --exit-code to additionally get \
    1 when the files differ (the diff(1) convention), which is off by default for the same \
    reason `git diff` defaults to 0: when a VCS drives codediff as a display tool, a non-zero \
    exit means \"the tool failed\", not \"the files differ\"."
)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    /// The files to diff: `BEFORE AFTER`, or git's 7- or 9-argument `GIT_EXTERNAL_DIFF` list
    /// (`path old-file old-hex old-mode new-file new-hex new-mode [new-path score]`). Without them
    /// the viewer starts empty.
    paths: Vec<PathBuf>,

    /// How to show the diff. `headless` and `json` need BEFORE and AFTER. `headless` is also
    /// chosen when stdout is not a terminal; `json` never is.
    #[arg(long, value_enum, ignore_case = true, default_value_t = Mode::Tui)]
    mode: Mode,

    /// Shorthand for `--mode headless`. `--batch` is a synonym.
    #[arg(long, alias = "batch")]
    headless: bool,

    /// Paint only the ranges that carry meaning: drop standalone brackets/separators and trim
    /// leading whitespace (the TUI's `M` panel's "everything off" preset). Without this or
    /// `--full`, the panel's last saved setting applies.
    ///
    /// `--minimal` and `--full` are two faithful readings of the same diff, not a right and a
    /// wrong one.
    #[arg(long, conflicts_with = "full")]
    minimal: bool,

    /// Keep standalone brackets/separators and leading whitespace (the `M` panel's "everything
    /// on" preset).
    #[arg(long)]
    full: bool,

    /// Highlight an updated node's matched pair whole (e.g. both `argument` and
    /// `i_am_an_argument`), instead of only the part that differs.
    ///
    /// Independent of `--minimal`/`--full`; combine freely.
    #[arg(long)]
    whole_updates: bool,

    /// Paint a node as moved even when it only changed indentation (nesting added or removed
    /// around untouched content) - the `M` panel's "Paint reindent-only moves" row, forced on.
    ///
    /// Combine freely with the other render flags; it can only turn the option on.
    #[arg(long)]
    paint_reindent_moves: bool,

    /// When to ANSI-color headless output. `auto` colors unless `NO_COLOR` is set, even when
    /// stdout is a pipe (git's pager shows color); `always` and `never` override `NO_COLOR`.
    #[arg(long, value_enum, value_name = "WHEN", default_value_t = ColorChoice::Auto)]
    color: ColorChoice,

    /// Exit 1 when the files differ (0 when identical, 2 on error) - the `diff(1)` convention,
    /// for scripts and CI conditionals.
    ///
    /// Off by default because version control systems read any non-zero exit from a display
    /// tool as a failure: jj warns on every file, and `git difftool` with
    /// `difftool.trustExitCode=true` aborts the whole diff. Ignored under git's
    /// `GIT_EXTERNAL_DIFF` argument form for the same reason.
    #[arg(long)]
    exit_code: bool,

    /// Unchanged lines to keep around each change in headless output, like `diff -U N`.
    #[arg(long, value_name = "N", default_value_t = tui::headless::CONTEXT_LINES)]
    context: usize,

    /// Open on the git review picker - the repository around the current directory's unstaged
    /// files, staged files and recent commits - instead of an empty viewer (the `G` key, at
    /// startup). TUI only; takes no BEFORE/AFTER pair.
    #[arg(long, conflicts_with = "headless")]
    review: bool,

    /// The TUI's tick rate, in ticks per second.
    // Hidden: a tuning knob for development, not a user-facing option.
    #[arg(long, hide = true, value_name = "FLOAT", default_value_t = 4.0, value_parser = parse_positive_rate)]
    tui_tick_rate: f64,

    /// The TUI's frame rate, in frames per second.
    #[arg(long, hide = true, value_name = "FLOAT", default_value_t = 60.0, value_parser = parse_positive_rate)]
    tui_frame_rate: f64,
}

async fn tui_main(args: &Args, before_after: Option<(PathBuf, PathBuf)>) -> Result<()> {
    tui::initialize_logging()?;
    tui::ui::install_panic_hook();

    let mut app = tui::app::App::new(args.tui_tick_rate, args.tui_frame_rate)?;
    if let Some((before, after)) = before_after {
        app.open_files(before, after)?;
    }
    if args.review {
        app.start_in_review();
    }
    app.run().await?;

    Ok(())
}

/// Whether to print text instead of starting the TUI: when asked to, or whenever stdout is not a
/// terminal, which is what makes `GIT_EXTERNAL_DIFF` under git's pager work with no configuration.
fn should_run_headless(args: &Args, stdout_is_terminal: bool) -> bool {
    args.headless || args.mode == Mode::Headless || !stdout_is_terminal
}

/// Whether to print a JSON diff object. Only `--mode json` selects it: a non-terminal stdout must
/// keep getting headless text, or every pipe and git integration would change format.
fn should_run_json(args: &Args) -> bool {
    args.mode == Mode::Json
}

/// The error for a text-mode run with no files, naming the reason text mode was chosen: a user
/// who passed `--headless` on a terminal is told about the missing files, not about the terminal.
fn headless_needs_files_message(args: &Args) -> &'static str {
    if args.headless || args.mode == Mode::Headless {
        "text mode needs BEFORE and AFTER - pass two files to diff"
    } else {
        "stdout is not a terminal and no files were given - pass BEFORE and AFTER to run in text \
         mode, or run from a real terminal to use the interactive viewer"
    }
}

/// The [`RenderOptions`](codediff::diff::text::RenderOptions) to paint with: a preset flag
/// replaces the saved `M` panel setting outright (so a script's output doesn't depend on the
/// machine), and the single-option flags layer on top of whichever applies.
fn render_options(args: &Args) -> codediff::diff::text::RenderOptions {
    let mut options = if args.minimal {
        codediff::diff::text::RenderOptions::MINIMAL
    } else if args.full {
        codediff::diff::text::RenderOptions::FULL
    } else {
        codediff::tui::theme::load_render_options()
    };
    options.whole_pair_updates = args.whole_updates;
    if args.paint_reindent_moves {
        options.paint_reindent_only_moves = true;
    }
    options
}

/// Whether headless output should be ANSI-colored. `Auto` deliberately ignores whether stdout is a
/// terminal: headless mode's main caller is git's pager, a pipe that shows color; `NO_COLOR`
/// (<https://no-color.org>) is the opt-out.
fn use_color(choice: ColorChoice) -> bool {
    match choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => !no_color_is_set(std::env::var_os("NO_COLOR")),
    }
}

/// no-color.org: the variable opts out when present *and non-empty*; `NO_COLOR=` is not an opt-out.
fn no_color_is_set(value: Option<std::ffi::OsString>) -> bool {
    value.is_some_and(|value| !value.is_empty())
}

/// The exit code of a completed non-interactive run (errors exit 2 at the call sites). The
/// `GIT_EXTERNAL_DIFF` form always exits 0: git reads non-zero there as "external diff died" and
/// aborts the entire multi-file diff.
fn exit_code_for(differed: bool, want_exit_code: bool, invoked_as_git_external_diff: bool) -> i32 {
    if differed && want_exit_code && !invoked_as_git_external_diff {
        1
    } else {
        0
    }
}

/// The one-line stand-in for a binary diff, worded like git's. Under `GIT_EXTERNAL_DIFF` both
/// sides are temp blobs, so it names git's repo-relative `path` once instead. `differed` is a
/// byte comparison: a direct invocation may pass identical files.
fn binary_notice(
    paths: &[PathBuf],
    before: &std::path::Path,
    after: &std::path::Path,
    differed: bool,
) -> String {
    if invoked_as_git_external_diff(paths) {
        let name = paths[0].display();
        return match differed {
            true => format!("Binary file {name} differs\n"),
            false => format!("Binary file {name} is unchanged\n"),
        };
    }
    let (before, after) = (before.display(), after.display());
    match differed {
        true => format!("Binary files {before} and {after} differ\n"),
        false => format!("Binary files {before} and {after} are identical\n"),
    }
}

/// Reports a pair with at least one binary side (the other may be git's empty `/dev/null`), in
/// every mode. It must succeed: a UTF-8 decode error would exit 2, and under `GIT_EXTERNAL_DIFF`
/// git then abandons every remaining file in the diff.
fn run_binary(args: &Args, before: &std::path::Path, after: &std::path::Path) -> Result<i32> {
    let differed = std::fs::read(before)? != std::fs::read(after)?;
    if should_run_json(args) {
        // Still a JSON object of the usual shape (flagged `binary`, no hunks): prose would break
        // every `--mode json` consumer.
        let mut json = tui::json_output::binary_diff_json(before, after)?;
        json.push('\n');
        tui::headless::write_stdout(&json)?;
    } else {
        tui::headless::write_stdout(&binary_notice(&args.paths, before, after, differed))?;
    }
    Ok(exit_code_for(
        differed,
        args.exit_code,
        invoked_as_git_external_diff(&args.paths),
    ))
}

/// Writes a shell completion script or a man page to stdout, generated from the clap definition
/// so neither can drift from the real flags. The `packaging/` recipes redirect it.
fn run_util(action: &UtilAction) -> Result<()> {
    use clap::CommandFactory;

    let mut command = Args::command();
    match action {
        UtilAction::Completions { shell } => {
            // Explicit bin name: clap would otherwise use the crate name, which need not match.
            clap_complete::generate(*shell, &mut command, "codediff", &mut std::io::stdout());
        }
        UtilAction::Man => {
            clap_mangen::Man::new(command).render(&mut std::io::stdout())?;
        }
    }
    Ok(())
}

/// Whether `error` is the reader having closed stdout (`codediff a b | head`, a pager quit early).
/// That is the ordinary end of a run, not a failure to report.
fn is_broken_pipe(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::BrokenPipe)
    })
}

/// Every failure exits [`EXIT_FAILURE`] with a `codediff: ...` line, whichever path raised it;
/// clap's own usage errors already exit 2 on their own. The one exception is a broken pipe, which
/// exits 0 without a word.
#[tokio::main]
async fn main() {
    let code = match run().await {
        Ok(code) => code,
        Err(error) if is_broken_pipe(&error) => 0,
        Err(error) => {
            eprintln!("codediff: {error:#}");
            EXIT_FAILURE
        }
    };
    std::process::exit(code);
}

/// The whole program; the `Ok` value is the exit status of a completed run.
async fn run() -> Result<i32> {
    let args = Args::parse();

    if let Some(Command::Git {
        action: GitAction::Configure,
    }) = &args.command
    {
        return git_configure::run().map(|()| 0);
    }

    if let Some(Command::Jj {
        action: JjAction::Configure,
    }) = &args.command
    {
        return jj_configure::run().map(|()| 0);
    }

    if let Some(Command::Util { action }) = &args.command {
        return run_util(action).map(|()| 0);
    }

    let before_after = resolve_before_after(&args.paths)?;
    let invoked_as_git_external_diff = invoked_as_git_external_diff(&args.paths);

    if let Some((before, after)) = before_after.as_ref() {
        let either_is_binary =
            codediff::code::is_binary_file(before)? || codediff::code::is_binary_file(after)?;
        if either_is_binary {
            return run_binary(&args, before, after);
        }
    }

    if should_run_json(&args) {
        let (before, after) = before_after
            .context("`--mode json` needs BEFORE and AFTER - pass two files to diff")?;
        let differed = tui::json_output::run(&before, &after, render_options(&args))?;
        return Ok(exit_code_for(
            differed,
            args.exit_code,
            invoked_as_git_external_diff,
        ));
    }

    if should_run_headless(&args, std::io::stdout().is_terminal()) {
        let (before, after) = before_after.context(headless_needs_files_message(&args))?;
        let differed = tui::headless::run(
            &before,
            &after,
            use_color(args.color),
            args.context,
            render_options(&args),
        )?;
        return Ok(exit_code_for(
            differed,
            args.exit_code,
            invoked_as_git_external_diff,
        ));
    }

    tui_main(&args, before_after).await?;
    Ok(0)
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

    /// git appends the new path and a similarity score for a rename or copy, and `diff.renames`
    /// is on by default, so rejecting this form would abort the whole `git diff`.
    #[test]
    fn resolve_before_after_accepts_gits_nine_argument_rename_form() {
        let paths = vec![
            PathBuf::from("README.md"),
            PathBuf::from("/tmp/git-blob-AAAA/README.md"),
            PathBuf::from("abc123"),
            PathBuf::from("100644"),
            PathBuf::from("/tmp/git-blob-BBBB/README_tmp.md"),
            PathBuf::from("abc123"),
            PathBuf::from("100644"),
            PathBuf::from("README_tmp.md"),
            PathBuf::from("similarity index 100%"),
        ];
        assert_eq!(
            resolve_before_after(&paths).unwrap(),
            Some((
                PathBuf::from("/tmp/git-blob-AAAA/README.md"),
                PathBuf::from("/tmp/git-blob-BBBB/README_tmp.md")
            ))
        );
        assert!(invoked_as_git_external_diff(&paths));
    }

    #[test]
    fn resolve_before_after_still_rejects_an_unrecognized_argument_count() {
        let paths: Vec<PathBuf> = (0..8).map(|n| PathBuf::from(n.to_string())).collect();
        assert!(resolve_before_after(&paths).is_err());
        assert!(!invoked_as_git_external_diff(&paths));
    }

    /// An added or deleted file arrives as `/dev/null`; it is passed through, not special-cased.
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

    fn args_with(mode: Mode, headless: bool) -> Args {
        Args {
            command: None,
            paths: Vec::new(),
            mode,
            headless,
            minimal: false,
            full: false,
            whole_updates: false,
            paint_reindent_moves: false,
            color: ColorChoice::Auto,
            exit_code: false,
            context: tui::headless::CONTEXT_LINES,
            review: false,
            tui_tick_rate: 4.0,
            tui_frame_rate: 60.0,
        }
    }

    #[test]
    fn whole_updates_flag_layers_onto_minimal_and_full_alike() {
        let mut minimal = args_with(Mode::Tui, false);
        minimal.minimal = true;
        minimal.whole_updates = true;
        assert!(render_options(&minimal).whole_pair_updates);
        assert!(!render_options(&minimal).leading_whitespace);

        let mut full = args_with(Mode::Tui, false);
        full.full = true;
        full.whole_updates = true;
        assert!(render_options(&full).whole_pair_updates);
        assert!(render_options(&full).leading_whitespace);
    }

    #[test]
    fn paint_reindent_moves_flag_layers_onto_minimal_and_full_alike() {
        let mut minimal = args_with(Mode::Tui, false);
        minimal.minimal = true;
        minimal.paint_reindent_moves = true;
        assert!(render_options(&minimal).paint_reindent_only_moves);
        assert!(!render_options(&minimal).leading_whitespace);

        let mut full = args_with(Mode::Tui, false);
        full.full = true;
        full.paint_reindent_moves = true;
        assert!(render_options(&full).paint_reindent_only_moves);
        assert!(render_options(&full).leading_whitespace);

        let mut without_the_flag = args_with(Mode::Tui, false);
        without_the_flag.minimal = true;
        assert!(
            !render_options(&without_the_flag).paint_reindent_only_moves,
            "omitting the flag must not touch this field"
        );
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

    /// A typo in `--mode` used to fall through to the TUI silently; the values are also what
    /// shell completion offers.
    #[test]
    fn mode_accepts_its_three_values_case_insensitively_and_nothing_else() {
        for (value, expected) in [
            ("tui", Mode::Tui),
            ("TUI", Mode::Tui),
            ("headless", Mode::Headless),
            ("Headless", Mode::Headless),
            ("json", Mode::Json),
            ("JSON", Mode::Json),
        ] {
            let args = Args::try_parse_from(["codediff", "--mode", value]).unwrap();
            assert_eq!(args.mode, expected, "--mode {value}");
        }
        assert!(Args::try_parse_from(["codediff", "--mode", "bogus"]).is_err());
        assert!(Args::try_parse_from(["codediff", "--exact"]).is_err());
    }

    /// `Duration::from_secs_f64(1.0 / rate)` panics on these; they are argument errors instead.
    #[test]
    fn tui_rates_reject_zero_negative_and_non_numbers() {
        for value in ["0", "-1", "nan", "inf", "fast"] {
            assert!(
                Args::try_parse_from(["codediff", "--tui-tick-rate", value]).is_err(),
                "--tui-tick-rate {value}"
            );
            assert!(
                Args::try_parse_from(["codediff", "--tui-frame-rate", value]).is_err(),
                "--tui-frame-rate {value}"
            );
        }
        let args = Args::try_parse_from(["codediff", "--tui-tick-rate", "2.5"]).unwrap();
        assert_eq!(args.tui_tick_rate, 2.5);
    }

    /// The rate flags are development knobs, hidden from `--help` and the man page.
    #[test]
    fn tui_rates_are_hidden_from_help() {
        use clap::CommandFactory;

        let help = Args::command().render_long_help().to_string();
        assert!(!help.contains("--tui-tick-rate"), "{help}");
        assert!(!help.contains("--tui-frame-rate"), "{help}");
        assert!(help.contains("--mode"), "{help}");
    }

    /// no-color.org: only a non-empty value opts out.
    #[test]
    fn no_color_opts_out_only_when_set_and_non_empty() {
        assert!(!no_color_is_set(None));
        assert!(!no_color_is_set(Some("".into())));
        assert!(no_color_is_set(Some("1".into())));
    }

    #[test]
    fn the_no_files_message_names_the_missing_files_when_text_mode_was_asked_for() {
        assert!(headless_needs_files_message(&args_with(Mode::Tui, true)).starts_with("text mode"));
        assert!(
            headless_needs_files_message(&args_with(Mode::Headless, false))
                .starts_with("text mode")
        );
        assert!(
            headless_needs_files_message(&args_with(Mode::Tui, false))
                .starts_with("stdout is not a terminal")
        );
    }

    #[test]
    fn a_broken_pipe_anywhere_in_the_chain_is_recognized() {
        let io = std::io::Error::from(std::io::ErrorKind::BrokenPipe);
        assert!(is_broken_pipe(&anyhow::Error::from(io).context("writing")));
        let other = std::io::Error::from(std::io::ErrorKind::NotFound);
        assert!(!is_broken_pipe(&anyhow::Error::from(other)));
    }

    #[test]
    fn always_and_never_beat_the_no_color_environment_variable() {
        // Always/Never never read the environment, so no env mutation (which races) is needed.
        assert!(use_color(ColorChoice::Always));
        assert!(!use_color(ColorChoice::Never));
    }

    #[test]
    fn without_the_flag_a_differing_pair_still_exits_zero() {
        assert_eq!(exit_code_for(true, false, false), 0);
        assert_eq!(exit_code_for(false, false, false), 0);
    }

    #[test]
    fn with_the_flag_exit_codes_follow_the_diff_convention() {
        assert_eq!(exit_code_for(false, true, false), 0);
        assert_eq!(exit_code_for(true, true, false), 1);
    }

    #[test]
    fn the_git_external_diff_form_never_returns_one_even_with_the_flag() {
        assert_eq!(exit_code_for(true, true, true), 0);
        assert_eq!(exit_code_for(false, true, true), 0);
    }

    #[test]
    fn a_binary_pair_under_the_git_external_diff_form_still_exits_zero() {
        assert_eq!(exit_code_for(true, false, true), 0);
        assert_eq!(exit_code_for(true, true, true), 0);
    }

    #[test]
    fn the_binary_notice_names_gits_logical_path_not_its_temp_blobs() {
        let paths = vec![
            PathBuf::from("doc/paper.pdf"),
            PathBuf::from("/tmp/git-blob-AAAA/paper.pdf"),
            PathBuf::from("abc123"),
            PathBuf::from("100644"),
            PathBuf::from("/tmp/git-blob-BBBB/paper.pdf"),
            PathBuf::from("def456"),
            PathBuf::from("100644"),
        ];
        let (before, after) = resolve_before_after(&paths).unwrap().unwrap();
        assert_eq!(
            binary_notice(&paths, &before, &after, true),
            "Binary file doc/paper.pdf differs\n"
        );
    }

    #[test]
    fn the_binary_notice_names_both_sides_for_the_two_argument_form() {
        let paths = vec![PathBuf::from("old.pdf"), PathBuf::from("new.pdf")];
        let (before, after) = resolve_before_after(&paths).unwrap().unwrap();
        assert_eq!(
            binary_notice(&paths, &before, &after, true),
            "Binary files old.pdf and new.pdf differ\n"
        );
    }

    #[test]
    fn the_binary_notice_does_not_claim_identical_files_differ() {
        let paths = vec![PathBuf::from("a.pdf"), PathBuf::from("b.pdf")];
        let (before, after) = resolve_before_after(&paths).unwrap().unwrap();
        assert_eq!(
            binary_notice(&paths, &before, &after, false),
            "Binary files a.pdf and b.pdf are identical\n"
        );
    }

    #[test]
    fn exit_code_flag_defaults_to_off_and_parses() {
        assert!(!Args::try_parse_from(["codediff"]).unwrap().exit_code);
        assert!(
            Args::try_parse_from(["codediff", "--exit-code"])
                .unwrap()
                .exit_code
        );
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

    #[test]
    fn two_paths_still_parse_as_paths_not_a_subcommand() {
        let args = Args::try_parse_from(["codediff", "a.rs", "b.rs"]).unwrap();
        assert!(args.command.is_none());
        assert_eq!(
            args.paths,
            vec![PathBuf::from("a.rs"), PathBuf::from("b.rs")]
        );
    }

    /// The `packaging/` recipes call these, so a rename breaks them silently.
    #[test]
    fn util_generates_completions_and_a_man_page() {
        let args = Args::try_parse_from(["codediff", "util", "completions", "bash"]).unwrap();
        assert!(matches!(
            args.command,
            Some(Command::Util {
                action: UtilAction::Completions {
                    shell: clap_complete::Shell::Bash
                }
            })
        ));

        let args = Args::try_parse_from(["codediff", "util", "man"]).unwrap();
        assert!(matches!(
            args.command,
            Some(Command::Util {
                action: UtilAction::Man
            })
        ));
    }

    #[test]
    fn the_help_text_describes_codediff_rather_than_a_doc_comment() {
        use clap::CommandFactory;

        let about = Args::command().get_about().unwrap().to_string();
        assert!(
            about.starts_with("Fast, robust, syntax-aware code diffing"),
            "{about}"
        );
        assert_eq!(
            Args::command().get_version(),
            Some(env!("CARGO_PKG_VERSION"))
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
        assert!(should_run_headless(&args_with(Mode::Tui, false), false));
    }

    #[test]
    fn should_run_headless_is_false_on_a_real_terminal_with_no_flags() {
        assert!(!should_run_headless(&args_with(Mode::Tui, false), true));
    }

    #[test]
    fn should_run_headless_honors_the_headless_flag_even_on_a_real_terminal() {
        assert!(should_run_headless(&args_with(Mode::Tui, true), true));
    }

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
        assert!(should_run_headless(&args_with(Mode::Headless, false), true));
    }

    #[test]
    fn should_run_json_honors_mode_json_case_insensitively() {
        assert!(should_run_json(&args_with(Mode::Json, false)));
        assert!(should_run_json(&args_with(Mode::Json, false)));
        assert!(!should_run_json(&args_with(Mode::Tui, false)));
        assert!(!should_run_json(&args_with(Mode::Headless, false)));
    }

    #[test]
    fn should_run_json_is_false_on_a_non_terminal_stdout_unless_explicitly_asked_for() {
        assert!(!should_run_json(&args_with(Mode::Tui, false)));
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
