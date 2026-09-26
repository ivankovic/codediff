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
//! Syntax-aware code diffing on tree-sitter ASTs.
//!
//! [`diff_strings`] is the entry point: it parses both sides into [`code::Code`] and returns a
//! [`diff::Diff`]: the mapping between the two syntax trees plus the changed text ranges it implies.
//! The `tui` feature adds the terminal viewer built on it.
//!
//! # Stability
//!
//! This crate is published for the `codediff` binary. Its stable contracts are the command line
//! and the `--mode json` output, which the editor integrations consume. The library API - the
//! `diff` module's solver passes, the `tui` module, and the `tree_sitter` and `ratatui`
//! types in their signatures - is the binary's internals made visible, and changes between 0.x
//! minor versions without notice. [`diff_strings`] and the [`code`] and [`diff`] module roots are
//! the parts meant for use from another crate.
#[cfg(feature = "stats")]
pub mod anomalous_paths;
pub mod code;
pub mod diff;
#[cfg(feature = "stats")]
pub mod stats;
// Needs no TUI dependency itself, but its one consumer (the TUI's review picker) is behind `tui`, so
// it shares that gate rather than widen a `default-features = false` consumer's surface.
#[cfg(feature = "tui")]
pub mod review;
#[cfg(feature = "tui")]
pub mod tui;
// The GitHub Pages showcase's data, for src/bin/generate_showcase.rs alone.
#[cfg(feature = "test-fixtures")]
pub mod showcase;

// Unit tests across the crate use `crate::test::helper` whatever the features, so `cfg(test)` always
// sees it; `test-fixtures` exposes it to the src/bin/ tools that depend on it outside tests.
#[cfg(any(test, feature = "test-fixtures"))]
pub mod test;

use crate::{
    code::{Code, Language},
    diff::Diff,
    diff::diff_code,
};

/// Diffs two programs given as source strings in `language`.
// TODO: auto-detect an Unknown language (in `Code::from_string`), checking both sides agree.
pub fn diff_strings(before: &str, after: &str, language: &Language) -> Diff {
    let code_before = Code::from_string(before, language);
    let code_after = Code::from_string(after, language);

    let mut diff = diff_code(&code_before, &code_after);
    diff.language = *language;
    diff
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_empty_strings() {
        let d = diff_strings("", "", &Language::Rust);

        assert!(d.ast.is_some());
    }
}
