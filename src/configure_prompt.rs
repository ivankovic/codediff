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

//! The interactive-prompt plumbing `git_configure` and `jj_configure` share.
//!
//! Scope types and config keys stay in each wizard: they are exactly where git and jj differ, so a
//! shared abstraction would have to be undone at every call site.

use std::io::{self, Write};

use anyhow::{Context, Result};

/// The absolute path to the running binary, so the written config points at *this* build rather
/// than whatever `codediff` resolves to on PATH. Falls back to the bare name if it can't be resolved.
pub fn resolve_codediff_path() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.to_str().map(str::to_string))
        .unwrap_or_else(|| "codediff".to_string())
}

/// Parses a yes/no answer; an empty line is `default`, anything unrecognized is `None` (reprompt).
pub fn parse_yes_no(input: &str, default: bool) -> Option<bool> {
    match input.trim().to_lowercase().as_str() {
        "" => Some(default),
        "y" | "yes" => Some(true),
        "n" | "no" => Some(false),
        _ => None,
    }
}

pub fn ask_yes_no(prompt: &str, default: bool) -> Result<bool> {
    loop {
        let input = read_line(prompt)?;
        match parse_yes_no(&input, default) {
            Some(answer) => return Ok(answer),
            None => println!("'{input}' - please answer 'y' or 'n'."),
        }
    }
}

pub fn read_line(prompt: &str) -> Result<String> {
    print!("{prompt}");
    io::stdout().flush().context("failed to write prompt")?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("failed to read from stdin")?;
    Ok(input.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_codediff_path_never_returns_an_empty_string() {
        assert!(!resolve_codediff_path().is_empty());
    }

    #[test]
    fn parse_yes_no_accepts_y_n_and_their_full_spellings_case_insensitively() {
        assert_eq!(parse_yes_no("y", false), Some(true));
        assert_eq!(parse_yes_no("Y", false), Some(true));
        assert_eq!(parse_yes_no("yes", false), Some(true));
        assert_eq!(parse_yes_no("n", true), Some(false));
        assert_eq!(parse_yes_no("no", true), Some(false));
    }

    #[test]
    fn parse_yes_no_falls_back_to_the_default_on_an_empty_line() {
        assert_eq!(parse_yes_no("", true), Some(true));
        assert_eq!(parse_yes_no("  ", false), Some(false));
    }

    #[test]
    fn parse_yes_no_rejects_anything_else() {
        assert_eq!(parse_yes_no("maybe", true), None);
        assert_eq!(parse_yes_no("ye", true), None);
    }
}
