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
//! jj ignores git's `difftool`/`diff.external` settings even in a colocated repo. It writes:
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
//! `diff-invocation-mode = "file-by-file"` is required: jj's default (`"dir"`) passes two directory
//! trees (literally `left` and `right`), which codediff cannot diff; `file-by-file` passes one file
//! pair per change, keeping the repo-relative path and extension (`left/src.rs`), so language
//! detection works. Both verified against jj 0.44.0.
//!
//! Not offered: `jj diff` runs its formatter under a pager, so `should_run_headless` picks the text
//! renderer with no extra config. `ui.diff-editor` is terminal-attached, but jj reads the edited
//! right side back into a commit, which a read-only viewer must not claim to support.

use std::io::{self, IsTerminal};
use std::process::Command;

use anyhow::{Context, Result};

use crate::configure_prompt::{ask_yes_no, read_line, resolve_codediff_path};

/// The tool name codediff registers itself under in `[merge-tools.<name>]`.
const TOOL: &str = "codediff";

/// Whether `jj config set` writes `--user` (all repositories) or `--repo` (this one only).
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

/// Entry point for `codediff jj configure`. Without a terminal on stdin it prints the manual
/// commands and fails rather than block on a read nobody will answer.
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
        // Required; see the module doc.
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

/// `key`'s current value under `scope` as a line to show before the prompt that would overwrite it,
/// or an empty string if unset.
fn existing_value_note(key: &str, scope: Scope) -> String {
    match get_config(scope, key) {
        Some(value) => format!("(currently: {value})\n"),
        None => String::new(),
    }
}

/// `jj config list <scope> <key>`, or `None` when unset.
///
/// Unset is detected by empty stdout, not exit status: jj 0.44 warns on stderr for an unset key
/// and still exits 0.
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

/// Parses `ask_scope`'s answer (`u`/`r`, after jj's own flags); `None` means reprompt.
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
