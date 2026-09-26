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
//! The positional-argument conventions both front-end binaries accept, and the notice both print
//! for a binary pair. The tests live in `main.rs`, next to the CLI behaviour they also exercise.

use std::path::{Path, PathBuf};

use anyhow::Result;

/// Resolves the positional arguments into a `(before, after)` pair, or `None` for an empty
/// viewer. The shape is chosen by count alone:
///
/// * **2**: `BEFORE AFTER` (plain CLI and `git difftool`).
/// * **7**: git's `GIT_EXTERNAL_DIFF` form, `path old-file old-hex old-mode new-file new-hex
///   new-mode`; only `old-file` and `new-file` are used, since they keep the real extension. An
///   add/delete arrives as `/dev/null` and is passed through for `compute_diff` to handle.
/// * **9**: the same form for a rename or copy, with two extra trailing arguments that are ignored.
///
/// Any other count is an error rather than a guess.
pub fn resolve_before_after(paths: &[PathBuf]) -> Result<Option<(PathBuf, PathBuf)>> {
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

/// Whether the arguments have the `GIT_EXTERNAL_DIFF` shape that `resolve_before_after`
/// recognizes (7 or 9). One predicate so every caller moves together if git adds an argument.
pub fn invoked_as_git_external_diff(paths: &[PathBuf]) -> bool {
    matches!(paths.len(), 7 | 9)
}

/// The one-line stand-in for a binary diff, worded like git's. Under `GIT_EXTERNAL_DIFF` both
/// sides are temp blobs, so it names git's repo-relative `path` once instead. `differed` is a
/// byte comparison: a direct invocation may pass identical files.
pub fn binary_notice(paths: &[PathBuf], before: &Path, after: &Path, differed: bool) -> String {
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
