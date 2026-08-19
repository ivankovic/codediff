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
 *  You should have received a copy of the GNU Affero General License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! `codediff git configure`: an interactive wizard for the `git config` commands README's "Git
//! integration" section otherwise asks the user to run by hand.

use std::io::{self, IsTerminal};
use std::process::Command;

use anyhow::{Context, Result};

use crate::configure_prompt::{ask_yes_no, read_line, resolve_codediff_path};

/// Whether to write `git config` values with `--global` or to the current repository only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Global,
    Local,
}

impl Scope {
    /// Always explicit, even for `Local` - `git config <key> <value>` with no scope flag at all
    /// happens to *write* to the local repo config by default, but `git config --get <key>`
    /// with no flag reads the *effective* (local-overrides-global) value instead. Passing
    /// `--local` explicitly for both keeps reads and writes consistently scoped to the same
    /// file - without it, `existing_value_note` would show an inherited global value as if it
    /// were already set locally, when nothing local exists at all yet.
    fn flag(self) -> &'static str {
        match self {
            Scope::Global => "--global",
            Scope::Local => "--local",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Scope::Global => "globally, for every repository",
            Scope::Local => "for this repository only",
        }
    }
}

/// Entry point for `codediff git configure`. Interactive only - bails with the manual commands
/// (same ones README's "Git integration" section documents) if stdin isn't a real terminal,
/// rather than hanging on a read that will never get an answer.
pub fn run() -> Result<()> {
    if !io::stdin().is_terminal() {
        print_manual_instructions();
        anyhow::bail!("`codediff git configure` needs an interactive terminal");
    }

    let codediff_path = resolve_codediff_path();
    println!("Configuring git to use {codediff_path} as its diff tool.\n");

    let scope = ask_scope()?;
    if scope == Scope::Local {
        ensure_inside_git_repo()?;
    }

    let difftool_cmd = format!("{codediff_path} \"$LOCAL\" \"$REMOTE\"");
    let set_difftool = ask_yes_no(
        &format!(
            "{}Set codediff as the default `git difftool`? [Y/n] (`difftool.codediff.cmd` = \
             `{difftool_cmd}`) ",
            existing_value_note("difftool.codediff.cmd", scope)
        ),
        true,
    )?;
    let skip_prompt = set_difftool
        && ask_yes_no(
            "Skip git's \"view diff ... [Y/n]?\" confirmation before each file? [Y/n] ",
            true,
        )?;
    let set_external = ask_yes_no(
        &format!(
            "{}Also use codediff for plain `git diff`/`git log -p` (via `diff.external`)? This \
             makes every git diff go through codediff, always in its non-interactive text mode. \
             [y/N] ",
            existing_value_note("diff.external", scope)
        ),
        false,
    )?;

    if !set_difftool && !set_external {
        println!("\nNothing selected - no changes made.");
        return Ok(());
    }

    println!();
    if set_difftool {
        set_config(scope, "difftool.codediff.cmd", &difftool_cmd)?;
        set_config(scope, "diff.tool", "codediff")?;
        if skip_prompt {
            set_config(scope, "difftool.prompt", "false")?;
        }
    }
    if set_external {
        set_config(scope, "diff.external", "codediff")?;
    }

    println!("\nDone - configured {}.", scope.label());
    Ok(())
}

/// One line describing `key`'s current value under `scope`, or an empty string if it isn't set -
/// shown before a prompt that would overwrite it, so re-running this wizard is safe rather than
/// silently clobbering an existing setting without saying so.
fn existing_value_note(key: &str, scope: Scope) -> String {
    match get_config(scope, key) {
        Some(value) => format!("(currently: {key} = {value})\n"),
        None => String::new(),
    }
}

fn get_config(scope: Scope, key: &str) -> Option<String> {
    let output = Command::new("git")
        .arg("config")
        .arg(scope.flag())
        .arg("--get")
        .arg(key)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn set_config(scope: Scope, key: &str, value: &str) -> Result<()> {
    let status = Command::new("git")
        .arg("config")
        .arg(scope.flag())
        .arg(key)
        .arg(value)
        .status()
        .with_context(|| format!("failed to run `git config {} {key} {value}`", scope.flag()))?;
    if !status.success() {
        anyhow::bail!("`git config {} {key} {value}` failed", scope.flag());
    }
    println!("  git config {} {key} {value}", scope.flag());
    Ok(())
}

fn ensure_inside_git_repo() -> Result<()> {
    let status = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .status()
        .context("failed to run `git rev-parse --git-dir` - is git installed?")?;
    if !status.success() {
        anyhow::bail!(
            "not inside a git repository - run this from within one, or choose global scope"
        );
    }
    Ok(())
}

fn print_manual_instructions() {
    eprintln!(
        "Run these manually instead (see README's \"Git integration\" section):\n\n\
         git config difftool.codediff.cmd 'codediff \"$LOCAL\" \"$REMOTE\"'\n\
         git config diff.tool codediff\n\
         git config difftool.prompt false\n\n\
         Add --global to any of these to apply them to every repository instead of just this \
         one. For plain `git diff`/`git log -p` too (always non-interactive):\n\n\
         git config diff.external codediff"
    );
}

/// Parses `ask_scope`'s prompt input - `None` for anything that isn't a recognized answer, so the
/// caller knows to reprompt rather than silently guessing.
fn parse_scope(input: &str) -> Option<Scope> {
    match input.trim().to_lowercase().as_str() {
        "" | "g" | "global" => Some(Scope::Global),
        "l" | "local" => Some(Scope::Local),
        _ => None,
    }
}

fn ask_scope() -> Result<Scope> {
    loop {
        let input = read_line(
            "Configure globally (every repository) or just this one? [g/l] (default: g) ",
        )?;
        match parse_scope(&input) {
            Some(scope) => return Ok(scope),
            None => println!("'{input}' - please answer 'g' or 'l'."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_flag_is_always_explicit() {
        assert_eq!(Scope::Global.flag(), "--global");
        assert_eq!(Scope::Local.flag(), "--local");
    }

    #[test]
    fn parse_scope_accepts_g_l_and_their_full_spellings_case_insensitively() {
        assert_eq!(parse_scope("g"), Some(Scope::Global));
        assert_eq!(parse_scope("G"), Some(Scope::Global));
        assert_eq!(parse_scope("global"), Some(Scope::Global));
        assert_eq!(parse_scope("Global"), Some(Scope::Global));
        assert_eq!(parse_scope("l"), Some(Scope::Local));
        assert_eq!(parse_scope("local"), Some(Scope::Local));
    }

    #[test]
    fn parse_scope_defaults_to_global_on_an_empty_line() {
        assert_eq!(parse_scope(""), Some(Scope::Global));
        assert_eq!(parse_scope("   "), Some(Scope::Global));
    }

    #[test]
    fn parse_scope_rejects_anything_else() {
        assert_eq!(parse_scope("yes"), None);
        assert_eq!(parse_scope("globalx"), None);
    }
}
