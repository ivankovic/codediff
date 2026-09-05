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

use codediff::tui;

mod configure_prompt;
mod git_configure;
mod jj_configure;

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
    /// Jujutsu (jj) integration helpers.
    Jj {
        #[command(subcommand)]
        action: JjAction,
    },
}

#[derive(Subcommand)]
enum GitAction {
    /// Interactively configure codediff as git's diff tool - the `git config` commands README's
    /// "Git integration" section otherwise asks you to run by hand.
    Configure,
}

#[derive(Subcommand)]
enum JjAction {
    /// Interactively configure codediff as jj's diff tool - the `jj config set` commands README's
    /// "Jujutsu (jj) integration" section otherwise asks you to run by hand.
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
    after_help = "Exit codes: 0 on success, 2 on error. Pass --exit-code to additionally get \
    1 when the files differ (the diff(1) convention), which is off by default for the same \
    reason `git diff` defaults to 0: when a VCS drives codediff as a display tool, a non-zero \
    exit means \"the tool failed\", not \"the files differ\"."
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

    /// Deprecated no-op, hidden from `--help` and kept only so existing scripts don't break: the
    /// pipeline runs the same bounded, region-scoped analysis for every diff, so there is no
    /// separate "exact" path to select. Remove at the next minor release.
    #[arg(long, hide = true)]
    exact: bool,

    /// Paint only the ranges that carry meaning: drop standalone brackets/separators and trim
    /// leading whitespace (the TUI's `M` panel's "everything off" preset, and its persisted
    /// setting when neither this flag nor `--full` is given).
    ///
    /// Every reading of a diff this and `--full` produce is faithful to the same mapping - the
    /// corpus's hand-authored ground truth records both extremes as separate paintings rather than
    /// one answer plus a mistake. See `codediff::diff::text::RenderOptions`.
    #[arg(long, conflicts_with = "full")]
    minimal: bool,

    /// The other extreme from `--minimal`: keep standalone brackets/separators and leading
    /// whitespace (the `M` panel's "everything on" preset). Lets a script force the fullest
    /// reading regardless of what's persisted, the same way `--minimal` forces the tightest one.
    #[arg(long)]
    full: bool,

    /// Highlight an `Update` node's own matched pair whole (e.g. both `argument` and
    /// `i_am_an_argument`), instead of narrowing to just the part that actually differs.
    ///
    /// A separate axis from `--minimal`/`--full`, not a third value either one takes: this decides
    /// which ranges the diff itself has (see `codediff::diff::text::RenderOptions::
    /// whole_pair_updates`), not how much of an already-decided range list gets painted. The `M`
    /// panel can toggle it too (it reloads the diff rather than just re-filtering, unlike the
    /// other rows there), so this flag exists purely for batch mode. Combine freely with
    /// `--minimal`/`--full` or neither.
    #[arg(long)]
    whole_updates: bool,

    /// Paint a matched node's relocation as `Move` even when it's known to be a pure reindent
    /// (nesting levels added/removed around otherwise-untouched content, e.g. Rust's `if
    /// let`-chain collapse) - the `M` panel's "Paint reindent-only moves" row, forced on. See
    /// `codediff::diff::text::RenderOptions::paint_reindent_only_moves`.
    ///
    /// A real axis `--minimal`/`--full` already disagree on (`--minimal` leaves it unpainted,
    /// `--full` paints it - measured against the corpus's own separate `Minimal`/`Full` ground
    /// truths for `rust-next-font-imports-generator`), so this flag only ever forces it *on* -
    /// there's no `--minimal`-with-this-off to ask for, since that's already what `--minimal`
    /// means. Combine freely with `--minimal`/`--full`/`--whole-updates` or neither.
    #[arg(long)]
    paint_reindent_moves: bool,

    /// When to ANSI-color headless output: `always`, `never`, or `auto` (the default - color on
    /// unless the `NO_COLOR` environment variable is set; see `use_color` for why auto is not
    /// tied to stdout being a terminal). The flag beats the environment variable in both
    /// directions: `--color always` colors even under `NO_COLOR`, `--color never` suppresses
    /// color without needing the variable.
    #[arg(long, value_enum, value_name = "WHEN", default_value_t = ColorChoice::Auto)]
    color: ColorChoice,

    /// Exit 1 when the files differ (0 when identical, 2 on error) - the `diff(1)` convention,
    /// for scripts and CI conditionals.
    ///
    /// Off by default, deliberately, and for the same reason `git diff` defaults to exiting 0
    /// even when files differ: codediff's main non-interactive callers are version control
    /// systems driving it as a *display* tool, and every one of them reads a non-zero exit as
    /// "the tool failed" rather than "the files differ". Measured consequences of defaulting
    /// this on (which v0.0.8 briefly did): `jj` prints a `Tool exited with exit status: 1`
    /// warning on every single file it renders, and `git difftool` with
    /// `difftool.trustExitCode=true` aborts the whole diff after the first differing file with
    /// "fatal: external diff died". Both go away when the default is 0; a script that wants the
    /// `diff(1)` behavior asks for it explicitly.
    #[arg(long)]
    exit_code: bool,

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
/// * **9 args**: the same convention when git detected the change as a rename or a copy, which
///   appends `other` (the destination path) and a rename/copy score to the seven above. `git
///   help diff` documents this as "when the diff is about a rename or copy"; `diff.renames`
///   defaults to on for `git diff`, so it is an ordinary case rather than an exotic one. The two
///   extra arguments are ignored the same way the hex/mode fields are, and `old-file`/`new-file`
///   stay at indices 1 and 4 - the shape is a suffix of the 7-argument one, not a rearrangement.
///
/// Any other count is almost certainly a mistake (most likely `GIT_EXTERNAL_DIFF` being invoked
/// with an unexpected git version's argument list) and is rejected with an explanatory error
/// rather than silently misinterpreted.
fn resolve_before_after(paths: &[PathBuf]) -> Result<Option<(PathBuf, PathBuf)>> {
    match paths.len() {
        0 => Ok(None),
        2 => Ok(Some((paths[0].clone(), paths[1].clone()))),
        7 | 9 => Ok(Some((paths[1].clone(), paths[4].clone()))),
        n => anyhow::bail!(
            "expected 0 positional arguments (empty viewer), 2 (BEFORE AFTER), or 7 \
            (GIT_EXTERNAL_DIFF's `path old-file old-hex old-mode new-file new-hex new-mode`, \
            or 9 with git's two extra rename/copy arguments), got {n}"
        ),
    }
}

/// Whether this invocation came from git's `GIT_EXTERNAL_DIFF` hook, as opposed to the plain
/// `BEFORE AFTER` CLI form (which `git difftool` also uses). Recognized by argument count alone,
/// exactly as `resolve_before_after` picks the pair apart - 7 normally, 9 when git detected a
/// rename or a copy.
///
/// A predicate rather than a `paths.len() == 7` at each site: it is checked in four places
/// (exit code, binary notice wording, and twice on the way to those), and the day git adds
/// another argument they all have to move together.
fn invoked_as_git_external_diff(paths: &[PathBuf]) -> bool {
    matches!(paths.len(), 7 | 9)
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

/// Which [`RenderOptions`](codediff::diff::text::RenderOptions) preset to paint with: `--minimal`
/// or `--full` if either is given (`clap`'s `conflicts_with` rules out both at once), otherwise
/// whatever the TUI's `M` panel last persisted - then `--whole-updates` layered on top
/// independently of that choice, since it isn't part of the `--minimal`/`--full` axis at all (see
/// `RenderOptions::whole_pair_updates`'s own doc comment).
///
/// A flag wins over the saved setting rather than toggling it, so a script that passes `--minimal`
/// gets minimal regardless of how the machine it runs on happens to be configured - and passing
/// neither flag keeps the two front ends agreeing about what the user last chose. `--whole-updates`
/// has no saved-setting counterpart to defer to either way: it is never written by the `M` panel
/// (see its own doc comment), so a script has to ask for it every time it wants it.
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

/// Turns a completed non-interactive run into a process exit code. 0 unless the caller opted
/// into the `diff(1)` convention with `--exit-code` (`want_exit_code`) *and* the files differ;
/// errors exit 2, handled at the call sites. See `Args::exit_code` for why opt-in is the right
/// default rather than the timid one.
///
/// The 7-argument `GIT_EXTERNAL_DIFF` form (recognized by `invoked_as_git_external_diff`) stays
/// at 0 even under `--exit-code`: git reads a non-zero exit there as "external diff died" and
/// aborts the *entire* multi-file diff, so honoring the flag in that position would break the
/// caller rather than inform it. A script wanting per-file differ/same status has the direct
/// `BEFORE AFTER` form available, which does honor it.
fn exit_code_for(differed: bool, want_exit_code: bool, invoked_as_git_external_diff: bool) -> i32 {
    if differed && want_exit_code && !invoked_as_git_external_diff {
        1
    } else {
        0
    }
}

/// The one-line stand-in for a diff of a file codediff cannot parse as text. Wording follows
/// `diff(1)`/git's own binary notice, since that is what a reader piping this through git's pager
/// is used to seeing there.
///
/// Under the `GIT_EXTERNAL_DIFF` form both sides are git temp blobs
/// (`/tmp/git-blob-XXXXXX/main.pdf`) - not paths the reader recognizes or can act on, and not two
/// distinct files in any sense that matters - so that form names `path` (index 0, the
/// repo-relative path git is actually diffing) once, exactly as git's own binary line does. Every
/// other invocation has two real paths and names both.
///
/// `differed` is a raw byte comparison, not an assumption: git only ever calls an external diff
/// for a pair it already knows differs, but `codediff a.pdf b.pdf` is under no such obligation,
/// and reporting two identical files as differing because nothing could be parsed would be a
/// wrong answer rather than an unavailable one.
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

/// Handles a pair where at least one side is binary, for every mode at once - this runs before
/// the json/headless/TUI split below.
///
/// Why it has to exist: `Code::from_file` reads with `read_to_string`, so a binary side fails the
/// UTF-8 decode and the error propagates out as exit 2. Under `GIT_EXTERNAL_DIFF` git reads any
/// non-zero exit as "external diff died" and abandons the *whole* run, so one PDF in a commit
/// used to take every remaining file's diff down with it - `git diff` printed a `fatal:` and
/// stopped, leaving the source files after it in the sort order undiffed. There is nothing useful
/// to show for a binary file, but "nothing useful" has to be reported as a successful diff of an
/// unshowable file, not as a crash.
///
/// Only *one* side needs to be binary: git represents an added or deleted file with `/dev/null`
/// on the missing side, which reads back as empty, perfectly valid UTF-8.
///
/// The exit code goes through `exit_code_for` like every other non-interactive path, so the
/// `GIT_EXTERNAL_DIFF`-never-returns-non-zero invariant holds here too.
fn run_binary(args: &Args, before: &std::path::Path, after: &std::path::Path) -> Result<i32> {
    let differed = std::fs::read(before)? != std::fs::read(after)?;
    if should_run_json(args) {
        // A prose sentence on stdout would break every consumer of `--mode json` (see
        // `json_output`'s module doc comment), so this stays a JSON object of the same shape,
        // flagged with `binary` and carrying no hunks.
        println!("{}", tui::json_output::binary_diff_json(before, after)?);
    } else {
        print!("{}", binary_notice(&args.paths, before, after, differed));
    }
    Ok(exit_code_for(
        differed,
        args.exit_code,
        invoked_as_git_external_diff(&args.paths),
    ))
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

    if let Some(Command::Jj {
        action: JjAction::Configure,
    }) = &args.command
    {
        return jj_configure::run();
    }

    let before_after = resolve_before_after(&args.paths)?;
    let invoked_as_git_external_diff = invoked_as_git_external_diff(&args.paths);

    // Ahead of the json/headless/TUI split: a binary side has the same non-answer in all three,
    // and the interactive viewer cannot show one either. See `run_binary`.
    if let Some((before, after)) = before_after.as_ref() {
        // Not `?`: a file that cannot be opened at all is a real error, and it has to keep
        // leaving by the same door as every other non-interactive failure here ("codediff: ..."
        // on stderr, exit 2). Propagating instead would hand it to `main`'s own `Result`, which
        // prints a differently-formatted `Error:` and exits 1.
        let either_is_binary = codediff::code::is_binary_file(before)
            .and_then(|binary| Ok(binary || codediff::code::is_binary_file(after)?));
        match either_is_binary
            .and_then(|binary| binary.then(|| run_binary(&args, before, after)).transpose())
        {
            Ok(Some(code)) => std::process::exit(code),
            Ok(None) => {}
            Err(e) => {
                eprintln!("codediff: {e:#}");
                std::process::exit(2);
            }
        }
    }

    if should_run_json(&args) {
        let (before, after) = before_after
            .context("`--mode json` needs BEFORE and AFTER - pass two files to diff")?;
        match tui::json_output::run(&before, &after, render_options(&args)) {
            Ok(differed) => std::process::exit(exit_code_for(
                differed,
                args.exit_code,
                invoked_as_git_external_diff,
            )),
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
        match tui::headless::run(
            &before,
            &after,
            use_color(args.color),
            args.context,
            render_options(&args),
        ) {
            Ok(differed) => std::process::exit(exit_code_for(
                differed,
                args.exit_code,
                invoked_as_git_external_diff,
            )),
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

    /// git appends `other` (the destination path) and a rename/copy score to the seven when it
    /// detected the change as a rename or a copy - `diff.renames` defaults to on for `git diff`,
    /// so this is the ordinary shape for any commit containing one. Rejecting it used to exit
    /// non-zero, which git reads as "external diff died", abandoning the whole run: a single
    /// rename took every file after it down with it. `old-file`/`new-file` stay at indices 1
    /// and 4.
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
        // Same form, so the same exit-code and notice-wording rules apply as for 7 arguments.
        assert!(invoked_as_git_external_diff(&paths));
    }

    /// A count that matches no convention is still an error rather than a silent
    /// misinterpretation - the 9-argument form widened the accepted set, it did not remove the
    /// guard.
    #[test]
    fn resolve_before_after_still_rejects_an_unrecognized_argument_count() {
        let paths: Vec<PathBuf> = (0..8).map(|n| PathBuf::from(n.to_string())).collect();
        assert!(resolve_before_after(&paths).is_err());
        assert!(!invoked_as_git_external_diff(&paths));
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
            minimal: false,
            full: false,
            whole_updates: false,
            paint_reindent_moves: false,
            color: ColorChoice::Auto,
            exit_code: false,
            context: tui::headless::CONTEXT_LINES,
            tui_tick_rate: 4.0,
            tui_frame_rate: 60.0,
        }
    }

    /// `--whole-updates` layers onto whichever `--minimal`/`--full`/persisted choice is otherwise
    /// in effect, rather than being folded into either preset - see `render_options`'s own doc
    /// comment for why the two axes are independent.
    #[test]
    fn whole_updates_flag_layers_onto_minimal_and_full_alike() {
        let mut minimal = args_with("TUI", false);
        minimal.minimal = true;
        minimal.whole_updates = true;
        assert!(render_options(&minimal).whole_pair_updates);
        assert!(!render_options(&minimal).leading_whitespace);

        let mut full = args_with("TUI", false);
        full.full = true;
        full.whole_updates = true;
        assert!(render_options(&full).whole_pair_updates);
        assert!(render_options(&full).leading_whitespace);
    }

    #[test]
    fn paint_reindent_moves_flag_layers_onto_minimal_and_full_alike() {
        let mut minimal = args_with("TUI", false);
        minimal.minimal = true;
        minimal.paint_reindent_moves = true;
        assert!(render_options(&minimal).paint_reindent_only_moves);
        assert!(!render_options(&minimal).leading_whitespace);

        let mut full = args_with("TUI", false);
        full.full = true;
        full.paint_reindent_moves = true;
        assert!(render_options(&full).paint_reindent_only_moves);
        assert!(render_options(&full).leading_whitespace);

        let mut without_the_flag = args_with("TUI", false);
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

    #[test]
    fn always_and_never_beat_the_no_color_environment_variable() {
        // `use_color` never reads the environment for Always/Never, so this needs no env
        // manipulation (which would race other tests in the same process).
        assert!(use_color(ColorChoice::Always));
        assert!(!use_color(ColorChoice::Never));
    }

    /// Without `--exit-code`, a differing pair still exits 0 - the default that keeps `jj` and
    /// `git difftool --trust-exit-code` from treating a normal diff as a tool failure.
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

    /// Even opted in, the GIT_EXTERNAL_DIFF form stays at 0: git reads non-zero there as
    /// "external diff died" and aborts the whole multi-file diff.
    #[test]
    fn the_git_external_diff_form_never_returns_one_even_with_the_flag() {
        assert_eq!(exit_code_for(true, true, true), 0);
        assert_eq!(exit_code_for(false, true, true), 0);
    }

    /// The regression this whole binary path exists for: `git diff` over a commit touching a PDF
    /// used to die on the PDF and abandon every file after it. Whatever else changes, the git
    /// form must keep exiting 0 for a binary pair.
    #[test]
    fn a_binary_pair_under_the_git_external_diff_form_still_exits_zero() {
        assert_eq!(exit_code_for(true, false, true), 0);
        assert_eq!(exit_code_for(true, true, true), 0);
    }

    /// Under `GIT_EXTERNAL_DIFF` both sides are temp blob copies with generated directory names,
    /// so the notice names `path` (index 0) instead - one file, one name, singular wording.
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

    /// The plain `BEFORE AFTER` form (also what `difftool.<tool>.cmd` uses) has two real paths
    /// and names both.
    #[test]
    fn the_binary_notice_names_both_sides_for_the_two_argument_form() {
        let paths = vec![PathBuf::from("old.pdf"), PathBuf::from("new.pdf")];
        let (before, after) = resolve_before_after(&paths).unwrap().unwrap();
        assert_eq!(
            binary_notice(&paths, &before, &after, true),
            "Binary files old.pdf and new.pdf differ\n"
        );
    }

    /// git never asks an external diff about a pair it considers identical, but `codediff a.pdf
    /// a.pdf` reaches the same code path, and "differ" would be a wrong answer there rather than
    /// an unavailable one.
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
