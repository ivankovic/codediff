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
use std::path::Path;

use anyhow::Result;
use git2::{DiffDelta, DiffFindOptions, Oid, Repository, Sort, Tree};

/// The raw content of the blob at `path` inside `tree`.
pub fn blob_bytes(repo: &Repository, tree: &Tree, path: &Path) -> Result<Vec<u8>> {
    let entry = tree.get_path(path)?;
    let blob = repo.find_blob(entry.id())?;
    Ok(blob.content().to_vec())
}

/// The line count of the blob at `oid`, or `None` if it's binary or its *byte* length falls
/// outside `[min_bytes, max_bytes]`. Shared size/UTF-8-validity check behind `sample_test_diffs`'
/// and `sample_code_pairs`' blob filtering - both tools exclude near-empty files (trivial
/// fixtures) and anything above the size `stats::expand_from_code` itself treats as "too large to
/// parse", since `diff_code` couldn't use it anyway.
///
/// The gate is byte-based but the return value is a line count, not a byte count: both callers
/// only ever used the byte length to feed `stats::sampling::loc_bucket`, and that function buckets
/// by LOC (chosen for human-readable bucket labels - "10-30 lines" reads far better than a byte
/// count), not bytes. The size *gate* stays byte-based regardless, since it's a cheap sanity/perf
/// filter unrelated to which unit a caller happens to bucket by.
pub fn text_loc_if_in_range(
    repo: &Repository,
    oid: Oid,
    min_bytes: usize,
    max_bytes: usize,
) -> Option<usize> {
    let blob = repo.find_blob(oid).ok()?;
    let content = blob.content();
    if content.len() < min_bytes || content.len() > max_bytes {
        return None;
    }
    let text = std::str::from_utf8(content).ok()?;
    Some(text.lines().count())
}

/// Walks the most-recent `max_commits` non-merge, non-root commits reachable from `repo_path`'s
/// HEAD (most-recent-first: repeated shallow fetches can leave a checkout with far more local
/// history than its nominal depth, so walking "all reachable commits" can mean walking the entire
/// project history - time order lets `max_commits` reliably mean "the N most recent"), and calls
/// `on_delta` once per changed-file delta in each commit's diff against its single parent.
///
/// Only the walk/diff machinery is shared: what counts as a *useful* delta (which `Delta::*`
/// statuses, path/language filtering, content validation, ...) differs between callers, so that
/// stays their responsibility inside `on_delta`.
pub fn walk_single_parent_commit_diffs(
    repo_path: &Path,
    max_commits: usize,
    detect_renames: bool,
    mut on_delta: impl FnMut(&Repository, Oid, &DiffDelta) -> Result<()>,
) -> Result<()> {
    let repo = Repository::open(repo_path)?;
    let mut walk = repo.revwalk()?;
    walk.set_sorting(Sort::TIME)?;
    walk.push_head()?;

    for (visited, id) in walk.enumerate() {
        if visited >= max_commits {
            break;
        }
        let Ok(id) = id else { continue };
        let Ok(commit) = repo.find_commit(id) else {
            continue;
        };

        // Merges mix unrelated changes and root commits have no "before" version; neither
        // produces a clean (before, after) edit pair, so only plain single-parent commits count.
        if commit.parents().len() != 1 {
            continue;
        }
        let parent = commit.parent(0)?;

        let before_tree = parent.tree()?;
        let after_tree = commit.tree()?;
        let mut diff = repo.diff_tree_to_tree(Some(&before_tree), Some(&after_tree), None)?;
        if detect_renames {
            // Rename detection is off by default; without it, a renamed+edited file shows up as
            // an unrelated Added/Deleted pair instead of the single meaningful edit pair it
            // actually is.
            let mut find_opts = DiffFindOptions::new();
            find_opts.renames(true);
            diff.find_similar(Some(&mut find_opts))?;
        }

        for delta in diff.deltas() {
            on_delta(&repo, id, &delta)?;
        }
    }

    Ok(())
}
