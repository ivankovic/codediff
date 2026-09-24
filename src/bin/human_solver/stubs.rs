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
//! Generating and registering the per-fixture test files `src/test/fixtures/` holds. Nothing
//! here reads `App`.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::{case_dataset, legacy_dataset};

// ---------------------------------------------------------------------------------------------

pub(crate) const LICENSE_HEADER: &str = "/*  This file is part of the CodeDiff code diffing tool.
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
";

pub(crate) fn module_name(name: &str) -> String {
    name.replace('-', "_")
}

/// `fixtures/` mirrors `diffs/`'s split by dataset (see `DIFF_DATASETS`).
pub(crate) fn fixtures_dir(dataset: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("fixtures")
        .join(dataset)
}

pub(crate) fn fixtures_mod_file(dataset: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("fixtures")
        .join(format!("{dataset}.rs"))
}

/// Creates `fixtures/<dataset>/<name>.rs` if missing and registers it in `fixtures/<dataset>.rs`.
/// Returns whether the file was created. The dataset comes from where `name` already lives under
/// `diffs/`, so the case directory must exist first.
///
/// An existing stub is never rewritten: `comment` only reaches a newly created file. `text_only`
/// is for a language with no tree-sitter grammar (see [`stub_test_contents`]).
pub(crate) fn ensure_stub_test(name: &str, comment: Option<&str>, text_only: bool) -> Result<bool> {
    let dataset = case_dataset(name).unwrap_or_else(legacy_dataset);
    let module = module_name(name);
    let dir = fixtures_dir(&dataset);
    let stub_path = dir.join(format!("{module}.rs"));

    let created = if stub_path.exists() {
        false
    } else {
        // A dataset's first promoted fixture creates its directory.
        fs::create_dir_all(&dir).with_context(|| format!("creating {:?}", dir))?;
        fs::write(&stub_path, stub_test_contents(name, comment, text_only))
            .with_context(|| format!("writing stub test to {:?}", stub_path))?;
        true
    };

    insert_mod_declaration(&dataset, &module)?;

    Ok(created)
}

/// The contents of a new `fixtures/<dataset>/<name>.rs`, without touching the filesystem.
pub(crate) fn stub_test_contents(name: &str, comment: Option<&str>, text_only: bool) -> String {
    if text_only {
        // No `mapping()`: with no tree it could only fail. No `painting()` yet either, as for any
        // fixture; `ensure_painting_stub_test` adds it on the first save with a painting.
        let comment_block = match comment.map(str::trim) {
            Some(c) if !c.is_empty() => {
                format!("//!\n{}", wrap_comment_lines_with_prefix(c, "//! "))
            }
            _ => String::new(),
        };
        // Joined lines, not a `\`-continued literal: rustfmt re-indents those and the indentation
        // lands inside the string. The name is left out so a long one cannot overflow the width.
        let module_doc = [
            "//! This fixture's language has no tree-sitter grammar, so there is no tree to map",
            "//! and no `mapping()` test here. codediff renders the pair with its plain-text",
            "//! fallback diff (`plain_text_line_diff`), and that is what the `painting()` test",
            "//! below is graded against - see `PaintingDiff::PlainText`.",
        ]
        .join("\n");
        return format!("{LICENSE_HEADER}{module_doc}\n{comment_block}\nuse anyhow::Result;\n");
    }
    let comment_block = match comment.map(str::trim) {
        Some(c) if !c.is_empty() => wrap_comment_lines(c),
        _ => String::new(),
    };
    format!(
        "{LICENSE_HEADER}use anyhow::Result;\n\nuse crate::test;\n\n#[test]\nfn mapping() -> Result<()> {{\n{comment_block}    test::helper::human_mapping::assert_matches_human_mapping(\"{name}\")\n}}\n"
    )
}

/// Word-wraps `comment` into `    // ` lines for the stub's function body, 96 columns wide
/// including the prefix. `comment` must already be trimmed and non-empty.
pub(crate) fn wrap_comment_lines(comment: &str) -> String {
    wrap_comment_lines_with_prefix(comment, "    // ")
}

/// [`wrap_comment_lines`] with a caller-chosen prefix, e.g. `//! ` for a text-only stub.
pub(crate) fn wrap_comment_lines_with_prefix(comment: &str, prefix: &str) -> String {
    const WIDTH: usize = 96;
    let max_content = WIDTH.saturating_sub(prefix.len());

    let mut lines = Vec::new();
    let mut current = String::new();
    for word in comment.split_whitespace() {
        let candidate_len = if current.is_empty() {
            word.len()
        } else {
            current.len() + 1 + word.len()
        };
        if candidate_len > max_content && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }

    lines
        .into_iter()
        .map(|line| format!("{prefix}{line}\n"))
        .collect()
}

/// Appends a `painting()` test to the file [`ensure_stub_test`] created; a missing file is an
/// error, since a fixture's results live in one file. Returns whether one was added. A file that
/// already has one is left alone: its recorded limit and prose belong to the human.
pub(crate) fn ensure_painting_stub_test(name: &str) -> Result<bool> {
    let dataset = case_dataset(name).unwrap_or_else(legacy_dataset);
    let module = module_name(name);
    let path = fixtures_dir(&dataset).join(format!("{module}.rs"));
    let existing = fs::read_to_string(&path)
        .with_context(|| format!("reading the fixture test file {:?}", path))?;
    if existing.contains("fn painting()") {
        return Ok(false);
    }

    const USE_LINE: &str =
        "use crate::test::helper::human_mapping::assert_matches_human_painting_within_limit;\n";
    let mut updated = insert_use_line(&existing, USE_LINE);
    if !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(&painting_test_block(name));
    fs::write(&path, updated).with_context(|| format!("writing {:?}", path))?;
    Ok(true)
}

/// Puts `use_line` beside the stub's existing imports, not after its tests; a no-op if present.
/// A text-only stub has no `use crate::test;`, hence the `use anyhow::Result;` fallback anchor.
pub(crate) fn insert_use_line(existing: &str, use_line: &str) -> String {
    if existing.contains(use_line) {
        return existing.to_string();
    }
    for anchor in ["use crate::test;\n", "use anyhow::Result;\n"] {
        if let Some(at) = existing.find(anchor) {
            let cut = at + anchor.len();
            return format!("{}{use_line}{}", &existing[..cut], &existing[cut..]);
        }
    }
    format!("{existing}{use_line}")
}

pub(crate) fn painting_test_block(name: &str) -> String {
    format!(
        "\n#[test]\nfn painting() -> Result<()> {{\n\
         \x20   // Not measured yet: 100.0 passes unconditionally. Run this test and record the\n\
         \x20   // limit it reports instead.\n\
         \x20   assert_matches_human_painting_within_limit(\"{name}\", 100.0)\n}}\n"
    )
}

/// Appends an `invariants()` test to every fixture, painted or not; an existing one is left alone.
/// Returns whether one was added. Unlike the painting stub it is strict from the start, with no
/// placeholder limit: self-contradicting ground truth is a defect, not a distance.
pub(crate) fn ensure_invariants_stub_test(name: &str) -> Result<bool> {
    let dataset = case_dataset(name).unwrap_or_else(legacy_dataset);
    let module = module_name(name);
    let path = fixtures_dir(&dataset).join(format!("{module}.rs"));
    let existing = fs::read_to_string(&path)
        .with_context(|| format!("reading the fixture test file {:?}", path))?;
    if existing.contains("fn invariants()") {
        return Ok(false);
    }

    const USE_LINE: &str =
        "use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants;\n";
    let mut updated = insert_use_line(&existing, USE_LINE);
    if !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(&invariants_test_block(name));
    fs::write(&path, updated).with_context(|| format!("writing {:?}", path))?;
    Ok(true)
}

pub(crate) fn invariants_test_block(name: &str) -> String {
    format!(
        "\n#[test]\nfn invariants() -> Result<()> {{\n    assert_ground_truth_invariants(\"{name}\")\n}}\n"
    )
}

/// Adds `#[cfg(test)] mod <module>;` to `fixtures/<dataset>.rs` if absent, keeping the list sorted.
pub(crate) fn insert_mod_declaration(dataset: &str, module: &str) -> Result<()> {
    let mod_file = fixtures_mod_file(dataset);
    let content =
        fs::read_to_string(&mod_file).with_context(|| format!("reading {:?}", mod_file))?;

    let mut lines = content.lines().peekable();
    let mut header_lines = Vec::new();
    while let Some(&line) = lines.peek() {
        if line.trim() == "#[cfg(test)]" {
            break;
        }
        header_lines.push(line.to_string());
        lines.next();
    }

    let mut entries: Vec<String> = Vec::new();
    while let Some(line) = lines.next() {
        if line.trim() != "#[cfg(test)]" {
            continue;
        }
        let mod_line = lines.next().with_context(|| {
            format!(
                "'#[cfg(test)]' not followed by a mod line in {:?}",
                mod_file
            )
        })?;
        let trimmed = mod_line.trim();
        let mod_name = trimmed
            .strip_prefix("mod ")
            .and_then(|rest| rest.strip_suffix(';'))
            .with_context(|| {
                format!(
                    "unexpected line after '#[cfg(test)]' in {:?}: {:?}",
                    mod_file, mod_line
                )
            })?;
        entries.push(mod_name.to_string());
    }

    if !entries.iter().any(|e| e == module) {
        entries.push(module.to_string());
        entries.sort();
    }

    let mut out = header_lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    for entry in &entries {
        out.push_str("#[cfg(test)]\n");
        out.push_str(&format!("mod {entry};\n"));
    }

    fs::write(&mod_file, out).with_context(|| format!("writing {:?}", mod_file))?;
    Ok(())
}
