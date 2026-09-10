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
//! The positional-argument conventions both front-end binaries accept. Lived in `main.rs` until
//! `codediff-web` (`web_main.rs`) needed the same three shapes; the tests stayed in `main.rs`,
//! where the surrounding CLI behaviour they also exercise lives.

use std::path::PathBuf;

use anyhow::Result;

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

/// Whether this invocation came from git's `GIT_EXTERNAL_DIFF` hook, as opposed to the plain
/// `BEFORE AFTER` CLI form (which `git difftool` also uses). Recognized by argument count alone,
/// exactly as `resolve_before_after` picks the pair apart - 7 normally, 9 when git detected a
/// rename or a copy.
///
/// A predicate rather than a `paths.len() == 7` at each site: it is checked in four places
/// (exit code, binary notice wording, and twice on the way to those), and the day git adds
/// another argument they all have to move together.
pub fn invoked_as_git_external_diff(paths: &[PathBuf]) -> bool {
    matches!(paths.len(), 7 | 9)
}
