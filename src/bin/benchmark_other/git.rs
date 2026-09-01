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

// Split out of benchmark_other.rs (the `git`-cluster functions) purely to shrink that
// file's visible size - no behavior change.

use anyhow::{Context, Result, bail};
use codediff::code::Code;
use std::process::Command;

use super::write_temp_pair;

/// Neutralizes the user's git configuration for a child process that will run `git` - directly
/// (`git_line_labels`) or indirectly (`bdiff_line_labels`, since BDiff shells out to
/// `git diff --no-index ... --unified=0 --numstat` for its raw change detection).
///
/// This is not defensive tidiness, it is a fix for an observed silent-wrong-answer (2026-08-23).
/// This project's own README recommends configuring codediff as git's external diff driver, and
/// with `diff.external=codediff` set, git emits codediff's output instead of a unified diff.
/// `--no-ext-diff` suppresses that for our own invocations, but nothing can suppress it inside
/// BDiff's hard-coded command string - so BDiff parsed zero `@@` headers and returned a **0-entry
/// edit script with exit status 0**, which scores as "this tool thinks nothing changed" rather
/// than as a failure. Pointing both config files at /dev/null removes the whole class: no
/// `diff.external`, no `diff.algorithm` overriding the flag we pass, no `core.autocrlf` rewriting
/// line endings under the measurement.
pub(crate) fn git_env(command: &mut Command) -> &mut Command {
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
}

/// `(before_touched, after_touched)` from `git diff --unified=0`, for one of git's four
/// `--diff-algorithm` values.
///
/// `--unified=0` so every hunk header describes exactly the changed lines with no context, and the
/// header alone carries everything needed - no need to read the body. A header is
/// `@@ -N,K +M,L @@`, where a missing `,K` means a count of 1.
///
/// **The `,0` case is the whole reason this parses headers by hand rather than counting `-`/`+`
/// lines.** For a pure insertion git writes `@@ -N,0 +M,L @@`: `N` there is the line *before
/// which* the insertion lands, and it is not itself touched. Treating it as a touched line shifts
/// the entire before-side label vector by one on every insertion-only hunk, which produces
/// completely plausible mismatch rates that are all quietly wrong. A zero count contributes no
/// lines; a count of `K > 0` contributes lines `N ..= N + K - 1` (1-indexed, as git writes them).
///
/// Cross-checked against `unix_diff_line_labels`: GNU diffutils and git's libxdiff are separate
/// implementations of the same Myers family, so `git_myers` and `unix_diff` must agree on
/// essentially every fixture. `git_myers_agrees_with_unix_diff` in this file's tests asserts that,
/// and a divergence there means this parser is broken, not that a difference was discovered.
pub(crate) fn git_line_labels(
    algorithm: &str,
    before: &Code,
    after: &Code,
) -> Result<(Vec<bool>, Vec<bool>)> {
    let (before_file, after_file) = write_temp_pair(before, after, None)?;

    let mut command = Command::new("git");
    git_env(&mut command);
    let output = command
        .args([
            "--no-pager",
            "diff",
            "--no-ext-diff",
            "--no-index",
            "--no-color",
            "--unified=0",
            &format!("--diff-algorithm={algorithm}"),
        ])
        .arg(before_file.path())
        .arg(after_file.path())
        .output()
        .with_context(|| format!("running git diff --diff-algorithm={algorithm}"))?;
    // `git diff --no-index` uses exit status 1 for "the files differ", which is the normal case
    // here, and only >1 is a real failure.
    if output.status.code().is_none_or(|code| code > 1) {
        bail!(
            "git diff --diff-algorithm={algorithm} exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let mut before_touched = vec![false; before.contents.split('\n').count()];
    let mut after_touched = vec![false; after.contents.split('\n').count()];
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Some(header) = line.strip_prefix("@@ ") else {
            continue;
        };
        let Some((ranges, _)) = header.split_once(" @@") else {
            continue;
        };
        for range in ranges.split_whitespace() {
            let (sign, spec) = range.split_at(1);
            let (start, count) = match spec.split_once(',') {
                Some((start, count)) => (start.parse::<usize>()?, count.parse::<usize>()?),
                None => (spec.parse::<usize>()?, 1),
            };
            if count == 0 {
                continue;
            }
            let touched = match sign {
                "-" => &mut before_touched,
                "+" => &mut after_touched,
                _ => continue,
            };
            for line_number in start..start + count {
                // Headers are 1-indexed; a malformed or out-of-range number is ignored rather
                // than panicking the whole corpus run on one fixture.
                if let Some(slot) = line_number.checked_sub(1).and_then(|i| touched.get_mut(i)) {
                    *slot = true;
                }
            }
        }
    }
    Ok((before_touched, after_touched))
}
