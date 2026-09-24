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

use anyhow::{Context, Result, bail};
use codediff::code::Code;
use std::process::Command;

use super::write_temp_pair;

/// Neutralizes the user's git configuration for a child that runs `git`, directly or through
/// BDiff. BDiff's hard-coded `git diff` cannot take `--no-ext-diff`, so a user's
/// `diff.external=codediff` makes it return an empty edit script that scores as "nothing changed"
/// rather than as a failure. Ignoring the config also stops `diff.algorithm` and `core.autocrlf`
/// from altering the measurement.
pub(crate) fn git_env(command: &mut Command) -> &mut Command {
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
}

/// `(before_touched, after_touched)` from `git diff --unified=0`, for one of git's four
/// `--diff-algorithm` values.
///
/// Only the `--unified=0` hunk headers `@@ -N,K +M,L @@` are read (a missing `,K` means 1). In the
/// `,0` case `N` is an anchor, not a touched line; counting it shifts every insertion-only hunk by
/// one and still yields plausible-looking rates.
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
    // Exit status 1 means "the files differ"; only >1 is a failure.
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
                if let Some(slot) = line_number.checked_sub(1).and_then(|i| touched.get_mut(i)) {
                    *slot = true;
                }
            }
        }
    }
    Ok((before_touched, after_touched))
}
