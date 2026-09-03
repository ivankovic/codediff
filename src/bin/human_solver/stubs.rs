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
//! Generating and registering the per-fixture test files `src/test/fixtures/` holds.
//!
//! Split out of `main.rs` along the section banner that already separated it: these functions
//! touch the *test tree* rather than the mapping being edited, and nothing here reads `App`.

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

/// `fixtures/` mirrors `diffs/`'s split by dataset (see `DIFF_DATASETS`): `dataset`'s
/// fixtures get their stub test files here, alongside `fixtures/<dataset>.rs`'s mod-list
/// (see `optimal_solutions_mod_file`).
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

/// Creates `fixtures/<dataset>/<name>.rs` if it doesn't already exist, and makes sure
/// it's registered in `fixtures/<dataset>.rs`. Returns whether the stub `.rs` file was
/// newly created. `dataset` is resolved from `name`'s actual location under `diffs/`
/// (`case_dataset`) - every caller (an already-open existing case, or `action_promote`, which
/// creates the diffs/ directory before calling this) runs after that directory already exists, so
/// there's always a real dataset to resolve, no separate parameter needed.
///
/// `comment`, if non-empty once trimmed, is word-wrapped (`wrap_comment_lines`) into a leading `//`
/// block right before the `assert_matches_human_mapping` call - only when the file is actually
/// being created here for the first time; an already-existing stub is never rewritten, so a
/// comment added or edited after promotion has no effect.
pub(crate) fn ensure_stub_test(name: &str, comment: Option<&str>) -> Result<bool> {
    let dataset = case_dataset(name).unwrap_or_else(legacy_dataset);
    let module = module_name(name);
    let dir = fixtures_dir(&dataset);
    let stub_path = dir.join(format!("{module}.rs"));

    let created = if stub_path.exists() {
        false
    } else {
        // `handmade`/`small`/`full` predate this dataset's `fixtures/<dataset>/`
        // directory existing at all, so this was never exercised until `stratified` (or any
        // future dataset) needed it fresh on first promotion - real gap, not defensive
        // programming against something that can't happen.
        fs::create_dir_all(&dir).with_context(|| format!("creating {:?}", dir))?;
        fs::write(&stub_path, stub_test_contents(name, comment))
            .with_context(|| format!("writing stub test to {:?}", stub_path))?;
        true
    };

    insert_mod_declaration(&dataset, &module)?;

    Ok(created)
}

/// Builds the full contents of a freshly-created `fixtures/<dataset>/<name>.rs` stub -
/// split out from `ensure_stub_test` as a pure string-building function (no filesystem access) so
/// it's directly unit-testable without writing into the real repo's `src/test/fixtures/`.
pub(crate) fn stub_test_contents(name: &str, comment: Option<&str>) -> String {
    let comment_block = match comment.map(str::trim) {
        Some(c) if !c.is_empty() => wrap_comment_lines(c),
        _ => String::new(),
    };
    format!(
        "{LICENSE_HEADER}use anyhow::Result;\n\nuse crate::test;\n\n#[test]\nfn mapping() -> Result<()> {{\n{comment_block}    test::helper::human_mapping::assert_matches_human_mapping(\"{name}\")\n}}\n"
    )
}

/// Word-wraps `comment` into `    // <text>\n` lines - 4-space indent matching the generated
/// stub's function body, `//` since this precedes a `#[test]` fn's own statement, not documenting
/// an item (a `///` doc comment there would attach to nothing). Wraps at a width matching this
/// codebase's own prose-comment convention (~96 columns including the prefix). `comment` is
/// assumed already trimmed and non-empty - see `ensure_stub_test`'s only caller.
pub(crate) fn wrap_comment_lines(comment: &str) -> String {
    const WIDTH: usize = 96;
    const PREFIX: &str = "    // ";
    let max_content = WIDTH.saturating_sub(PREFIX.len());

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
        .map(|line| format!("{PREFIX}{line}\n"))
        .collect()
}

/// Appends a `painting()` test to the fixture's own file, the painting counterpart of
/// [`ensure_stub_test`]. Returns whether one was actually added.
///
/// **Appends rather than writing its own file.** These used to be a parallel tree,
/// `src/test/painting_agreement/<name>.rs`, mirroring `optimal_solutions/` one file per fixture.
/// Everything a fixture's two ground truths have been measured to now lives in one place (see
/// `test::fixtures`' module doc), so this edits the file `ensure_stub_test` created rather than
/// starting a second one. Every caller runs after that call, so the file is always there; a fixture
/// file that somehow is not is an error rather than a silent second home.
///
/// Idempotent: a file that already has a `painting()` test is left exactly as it is, which is the
/// same "never rewrite an existing clamp" contract `ensure_stub_test` has - the recorded number and
/// the prose next to it are the human's, not this tool's.
pub(crate) fn ensure_painting_stub_test(name: &str) -> Result<bool> {
    let dataset = case_dataset(name).unwrap_or_else(legacy_dataset);
    let module = module_name(name);
    let path = fixtures_dir(&dataset).join(format!("{module}.rs"));
    let existing = fs::read_to_string(&path)
        .with_context(|| format!("reading the fixture test file {:?}", path))?;
    if existing.contains("fn painting()") {
        return Ok(false);
    }

    // The import goes next to the one `ensure_stub_test` already wrote, not at the end of the
    // file, so rustfmt has nothing to move on the next run.
    const USE_LINE: &str =
        "use crate::test::helper::human_mapping::assert_matches_human_painting_within_limit;\n";
    let anchor = "use crate::test;\n";
    let mut updated = if existing.contains(USE_LINE) {
        existing.clone()
    } else if let Some(at) = existing.find(anchor) {
        let cut = at + anchor.len();
        format!("{}{USE_LINE}{}", &existing[..cut], &existing[cut..])
    } else {
        format!("{existing}{USE_LINE}")
    };
    if !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(&painting_test_block(name));
    fs::write(&path, updated).with_context(|| format!("writing {:?}", path))?;
    Ok(true)
}

/// The `painting()` test appended by [`ensure_painting_stub_test`] - pure string building, no
/// filesystem access, so it is directly unit-testable.
pub(crate) fn painting_test_block(name: &str) -> String {
    format!(
        "\n#[test]\nfn painting() -> Result<()> {{\n\
         \x20   // Not measured yet: 100.0 passes unconditionally. Run this test, read the rate it\n\
         \x20   // reports for both modes, and record that instead.\n\
         \x20   assert_matches_human_painting_within_limit(\"{name}\", 100.0)\n}}\n"
    )
}

/// Adds `#[cfg(test)]\nmod <module>;` to `fixtures/<dataset>.rs`, keeping the list
/// sorted, unless it's already present.
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
