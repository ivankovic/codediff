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

//! `codediff jj configure`: the Jujutsu counterpart of `git_configure`.
//!
//! jj does not read git's `difftool`/`diff.external` settings, even in a colocated repo, so a
//! separate wizard is genuinely needed rather than redundant with the git one.
//!
//! # What this writes, and why each key is needed
//!
//! ```toml
//! [merge-tools.codediff]
//! program = "/abs/path/to/codediff"
//! diff-args = ["$left", "$right"]
//! diff-invocation-mode = "file-by-file"
//!
//! [ui]
//! diff-formatter = "codediff"   # only if the user opts in to making it the default
//! ```
//!
//! `diff-invocation-mode = "file-by-file"` is the load-bearing one. jj's default (`"dir"`)
//! materializes each side of the diff as a whole directory tree and passes the tool two
//! *directory* paths - verified against jj 0.44.0, which passes literally `left` and `right` -
//! and codediff takes two files, so without this key every invocation fails. `file-by-file`
//! makes jj invoke the tool once per changed file pair instead, which is the same shape git's
//! `difftool.<tool>.cmd = codediff "$LOCAL" "$REMOTE"` already uses.
//!
//! Language detection survives this: jj's per-file paths keep the repo-relative path, extension
//! included (`left/src.rs`, `right/sub/thing.py` - verified empirically against jj 0.44.0, the
//! same way `main.rs`'s `Args::paths` doc comment records the equivalent check for git 2.43).
//!
//! # What is deliberately *not* offered
//!
//! There is no jj equivalent of `git difftool`'s interactive, terminal-attached per-file viewer.
//! `jj diff` runs its formatter under a pager, so codediff's own tty check (`should_run_headless`)
//! correctly selects the non-interactive text renderer - which is the behavior wanted here, and
//! means no extra configuration is needed to get it. jj's terminal-attached hook is
//! `ui.diff-editor`, but that is for `jj diffedit`/`jj split`, where jj reads the *modified* right
//! side back and turns it into a new commit; codediff is a read-only viewer, so registering it
//! there would misrepresent what it does. Anyone wanting the full-screen TUI on a jj repo can run
//! `codediff BEFORE AFTER` directly.

use std::io::{self, IsTerminal};
use std::process::Command;

use anyhow::{Context, Result};

use crate::configure_prompt::{ask_yes_no, read_line, resolve_codediff_path};

/// The tool name codediff registers itself under in `[merge-tools.<name>]`.
const TOOL: &str = "codediff";

/// Whether to write `jj config set` values with `--user` (all repositories) or `--repo` (this
/// one only) - jj's own spelling of the same distinction git draws with `--global`/`--local`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    User,
    Repo,
}

impl Scope {
    fn flag(self) -> &'static str {
        match self {
            Scope::User => "--user",
            Scope::Repo => "--repo",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Scope::User => "for your user, in every repository",
            Scope::Repo => "for this repository only",
        }
    }
}

/// Entry point for `codediff jj configure`. Interactive only - bails with the manual commands if
/// stdin isn't a real terminal, rather than hanging on a read that will never get an answer.
pub fn run() -> Result<()> {
    if !io::stdin().is_terminal() {
        print_manual_instructions();
        anyhow::bail!("`codediff jj configure` needs an interactive terminal");
    }

    ensure_jj_available()?;

    let codediff_path = resolve_codediff_path();
    println!("Configuring jj to use {codediff_path} as its diff tool.\n");

    let scope = ask_scope()?;
    if scope == Scope::Repo {
        ensure_inside_jj_repo()?;
    }

    let register = ask_yes_no(
        &format!(
            "{}Register codediff as a jj diff tool (`merge-tools.{TOOL}`)? You can then run \
             `jj diff --tool {TOOL}`. [Y/n] ",
            existing_value_note(&format!("merge-tools.{TOOL}.program"), scope)
        ),
        true,
    )?;
    let set_default = ask_yes_no(
        &format!(
            "{}Also make it the default for plain `jj diff` (`ui.diff-formatter`)? [y/N] ",
            existing_value_note("ui.diff-formatter", scope)
        ),
        false,
    )?;

    if !register && !set_default {
        println!("\nNothing selected - no changes made.");
        return Ok(());
    }

    println!();
    if register {
        set_config(
            scope,
            &format!("merge-tools.{TOOL}.program"),
            &codediff_path,
        )?;
        set_config(
            scope,
            &format!("merge-tools.{TOOL}.diff-args"),
            r#"["$left","$right"]"#,
        )?;
        // Without this jj hands the tool two directories instead of a file pair - see this
        // module's own doc comment.
        set_config(
            scope,
            &format!("merge-tools.{TOOL}.diff-invocation-mode"),
            "file-by-file",
        )?;
    }
    if set_default {
        set_config(scope, "ui.diff-formatter", TOOL)?;
    }

    println!("\nDone - configured {}.", scope.label());
    if set_default && !register {
        println!(
            "Note: ui.diff-formatter now names `{TOOL}`, but merge-tools.{TOOL} was not written - \
             jj will not find the tool until it is."
        );
    }
    Ok(())
}

/// One line describing `key`'s current value under `scope`, or an empty string if it isn't set -
/// shown before a prompt that would overwrite it, so re-running this wizard is safe rather than
/// silently clobbering an existing setting without saying so.
fn existing_value_note(key: &str, scope: Scope) -> String {
    match get_config(scope, key) {
        Some(value) => format!("(currently: {value})\n"),
        None => String::new(),
    }
}

/// `jj config list <scope> <key>`, or `None` when unset.
///
/// Detected by empty stdout, deliberately not by exit status: jj 0.44 reports an unset key with a
/// `Warning: No matching config key` on *stderr* and still exits 0, so trusting the status would
/// report every unset key as set-to-empty.
fn get_config(scope: Scope, key: &str) -> Option<String> {
    let output = Command::new("jj")
        .arg("config")
        .arg("list")
        .arg(scope.flag())
        .arg(key)
        .output()
        .ok()?;
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn set_config(scope: Scope, key: &str, value: &str) -> Result<()> {
    let status = Command::new("jj")
        .arg("config")
        .arg("set")
        .arg(scope.flag())
        .arg(key)
        .arg(value)
        .status()
        .with_context(|| {
            format!(
                "failed to run `jj config set {} {key} {value}`",
                scope.flag()
            )
        })?;
    if !status.success() {
        anyhow::bail!("`jj config set {} {key} {value}` failed", scope.flag());
    }
    println!("  jj config set {} {key} {value}", scope.flag());
    Ok(())
}

fn ensure_jj_available() -> Result<()> {
    Command::new("jj")
        .arg("--version")
        .output()
        .context("failed to run `jj --version` - is jj installed and on PATH?")?;
    Ok(())
}

fn ensure_inside_jj_repo() -> Result<()> {
    let output = Command::new("jj")
        .args(["root"])
        .output()
        .context("failed to run `jj root` - is jj installed?")?;
    if !output.status.success() {
        anyhow::bail!(
            "not inside a jj repository - run this from within one, or choose user-wide scope"
        );
    }
    Ok(())
}

fn print_manual_instructions() {
    eprintln!(
        "Run these manually instead (see README's \"Jujutsu (jj) integration\" section):\n\n\
         jj config set --user merge-tools.codediff.program codediff\n\
         jj config set --user merge-tools.codediff.diff-args '[\"$left\",\"$right\"]'\n\
         jj config set --user merge-tools.codediff.diff-invocation-mode file-by-file\n\n\
         Use --repo instead of --user to apply them to the current repository only. To make \
         codediff the default for plain `jj diff` as well:\n\n\
         jj config set --user ui.diff-formatter codediff\n\n\
         diff-invocation-mode is required: without it jj passes two directories, which codediff \
         cannot diff."
    );
}

/// Parses `ask_scope`'s prompt input - `None` for anything that isn't a recognized answer, so the
/// caller knows to reprompt rather than silently guessing. `u`/`r` rather than git's `g`/`l`,
/// matching the flags jj itself uses.
fn parse_scope(input: &str) -> Option<Scope> {
    match input.trim().to_lowercase().as_str() {
        "" | "u" | "user" => Some(Scope::User),
        "r" | "repo" => Some(Scope::Repo),
        _ => None,
    }
}

fn ask_scope() -> Result<Scope> {
    loop {
        let input = read_line(
            "Configure for your user (every repository) or just this one? [u/r] (default: u) ",
        )?;
        match parse_scope(&input) {
            Some(scope) => return Ok(scope),
            None => println!("'{input}' - please answer 'u' or 'r'."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_flags_match_jjs_own_spelling() {
        assert_eq!(Scope::User.flag(), "--user");
        assert_eq!(Scope::Repo.flag(), "--repo");
    }

    #[test]
    fn parse_scope_accepts_u_r_and_their_full_spellings_case_insensitively() {
        assert_eq!(parse_scope("u"), Some(Scope::User));
        assert_eq!(parse_scope("U"), Some(Scope::User));
        assert_eq!(parse_scope("user"), Some(Scope::User));
        assert_eq!(parse_scope("r"), Some(Scope::Repo));
        assert_eq!(parse_scope("Repo"), Some(Scope::Repo));
    }

    #[test]
    fn parse_scope_defaults_to_user_on_an_empty_line() {
        assert_eq!(parse_scope(""), Some(Scope::User));
        assert_eq!(parse_scope("   "), Some(Scope::User));
    }

    #[test]
    fn parse_scope_rejects_anything_else() {
        assert_eq!(parse_scope("yes"), None);
        assert_eq!(
            parse_scope("global"),
            None,
            "that's git's spelling, not jj's"
        );
    }
}
