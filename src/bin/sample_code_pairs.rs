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
use anyhow::Result;
use clap::Parser;
use git2::{Delta, DiffDelta, DiffFindOptions, Oid, Repository, Sort};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use codediff::code::language::{language_for_path, to_treesitter};
use codediff::metadata;

// Files outside this range are excluded: near-empty files make trivial benchmark cases, and
// anything above the upper bound is past the size `expand_from_code` itself treats as
// "too large to parse" (see `stats::expand_from_code`), so diff_code couldn't use it anyway.
const MIN_BYTES: usize = 1;
const MAX_BYTES: usize = 1024 * 1024;

// Size buckets, keyed by the larger of a pair's before/after byte size. Sampling is stratified
// across these per language, rather than purely uniform, so that large files (where tree-edit-
// distance cost grows super-linearly) aren't drowned out by the much more common small ones.
const SIZE_BUCKETS: &[(usize, &str)] = &[
    (1_000, "small"),
    (10_000, "medium"),
    (100_000, "large"),
    (usize::MAX, "xlarge"),
];

fn size_bucket(bytes: usize) -> &'static str {
    SIZE_BUCKETS
        .iter()
        .find(|(upper, _)| bytes < *upper)
        .map(|(_, name)| *name)
        .unwrap_or("xlarge")
}

#[derive(Parser)]
struct Args {
    /// Root directory containing checked-out git repositories (or a single repository).
    #[arg(long)]
    path: PathBuf,

    /// Where to write the sampled (language, repository, commit, path, old_path) rows.
    #[arg(long)]
    output: PathBuf,

    /// Target number of pairs per language, split evenly across size buckets.
    #[arg(long, default_value_t = 1000)]
    count: usize,

    /// RNG seed. Fixed by default so re-running against the same checkouts is reproducible.
    #[arg(long, default_value_t = 42)]
    seed: u64,
}

/// A pointer to a (before, after) code pair: the actual content lives in the repository
/// checkout, not in this tool's output, so only enough is recorded to look it up again later.
///
/// Reconstruction contract: before = blob at `old_path` in `commit`'s (single) parent tree,
/// after = blob at `path` in `commit`'s tree. `old_path` equals `path` except for renames.
struct Candidate {
    repository: String,
    commit: String,
    path: String,
    old_path: String,
}

/// Reservoir sampling (Algorithm R): picks a uniform random sample of `capacity` items from a
/// stream of unknown length in a single pass, without holding the whole stream in memory.
#[derive(Default)]
struct Reservoir {
    items: Vec<Candidate>,
    seen: u64,
}

impl Reservoir {
    fn offer(&mut self, candidate: Candidate, capacity: usize, rng: &mut impl Rng) {
        self.seen += 1;
        if self.items.len() < capacity {
            self.items.push(candidate);
        } else {
            let j = rng.gen_range(0..self.seen) as usize;
            if j < capacity {
                self.items[j] = candidate;
            }
        }
    }
}

/// Find all git repositories in top-level subdirectories of the given path, or the path itself
/// if it is already a repository. Sorted for reproducible traversal order across runs.
fn find_git_repositories(base_path: &Path) -> Result<Vec<PathBuf>> {
    let mut repo_paths = Vec::new();

    if base_path.join(".git").exists() {
        repo_paths.push(base_path.to_path_buf());
    } else if base_path.is_dir() {
        for entry in fs::read_dir(base_path)? {
            let path = entry?.path();
            if path.is_dir() && path.join(".git").exists() {
                repo_paths.push(path);
            }
        }
    }

    repo_paths.sort();
    Ok(repo_paths)
}

fn main() -> Result<()> {
    let args = Args::parse();

    let repo_paths = find_git_repositories(&args.path)?;
    if repo_paths.is_empty() {
        eprintln!("No git repositories found in {:?}", args.path);
        return Ok(());
    }
    println!("Found {} repositories", repo_paths.len());

    // Capacity is per (language, size bucket), so each language's overall budget stays close to
    // `count` while guaranteeing every size class gets a fair share regardless of how rare it is.
    let bucket_capacity = (args.count / SIZE_BUCKETS.len()).max(1);

    let mut rng = StdRng::seed_from_u64(args.seed);
    let mut reservoirs: HashMap<(String, &'static str), Reservoir> = HashMap::new();

    for repo_path in &repo_paths {
        let repository_name = repo_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        println!("Scanning {}", repository_name);
        if let Err(e) = sample_repository(
            repo_path,
            &repository_name,
            bucket_capacity,
            &mut reservoirs,
            &mut rng,
        ) {
            eprintln!("Failed to process {:?}: {:?}", repo_path, e);
        }
    }

    write_csv(&args.output, &reservoirs)?;

    let total: usize = reservoirs.values().map(|r| r.items.len()).sum();
    let languages: std::collections::HashSet<&String> =
        reservoirs.keys().map(|(language, _)| language).collect();
    println!(
        "Sampled {} pairs across {} languages into {:?}",
        total,
        languages.len(),
        args.output
    );

    Ok(())
}

/// Walks every non-merge commit in the repository and offers each modified or renamed file's
/// (commit, path) to the reservoir for its (language, size bucket).
fn sample_repository(
    repo_path: &Path,
    repository_name: &str,
    bucket_capacity: usize,
    reservoirs: &mut HashMap<(String, &'static str), Reservoir>,
    rng: &mut StdRng,
) -> Result<()> {
    let repo = Repository::open(repo_path)?;
    let mut walk = repo.revwalk()?;
    walk.set_sorting(Sort::TOPOLOGICAL)?;
    walk.push_head()?;

    for id in walk {
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
        // Rename detection is off by default; without it, a renamed+edited file shows up as an
        // unrelated Added/Deleted pair instead of the single meaningful edit pair it actually is.
        let mut find_opts = DiffFindOptions::new();
        find_opts.renames(true);
        diff.find_similar(Some(&mut find_opts))?;

        for delta in diff.deltas() {
            if !matches!(delta.status(), Delta::Modified | Delta::Renamed) {
                continue;
            }
            // A pure rename with no content change is a trivial, not a useful, diff pair.
            if delta.old_file().id() == delta.new_file().id() {
                continue;
            }

            let Some(path) = delta.new_file().path() else {
                continue;
            };
            let old_path = delta.old_file().path().unwrap_or(path);
            if metadata::is_anomalous(path) || metadata::is_anomalous(old_path) {
                continue;
            }

            let Some(language) = language_for_path(path) else {
                continue;
            };
            // Only sample languages diff_code can actually parse.
            if to_treesitter(&language).is_none() {
                continue;
            }

            let Some(size) = text_pair_size(&repo, &delta) else {
                continue;
            };

            let candidate = Candidate {
                repository: repository_name.to_string(),
                commit: id.to_string(),
                path: path.to_string_lossy().into_owned(),
                old_path: old_path.to_string_lossy().into_owned(),
            };

            reservoirs
                .entry((language.to_string(), size_bucket(size)))
                .or_default()
                .offer(candidate, bucket_capacity, rng);
        }
    }

    Ok(())
}

/// Returns the larger of the before/after byte sizes, or `None` if either side is binary or
/// outside the configured size bounds. Content is not kept around any longer than the check.
fn text_pair_size(repo: &Repository, delta: &DiffDelta) -> Option<usize> {
    let text_len_in_range = |oid: Oid| -> Option<usize> {
        let blob = repo.find_blob(oid).ok()?;
        let content = blob.content();
        if content.len() >= MIN_BYTES
            && content.len() <= MAX_BYTES
            && std::str::from_utf8(content).is_ok()
        {
            Some(content.len())
        } else {
            None
        }
    };

    let before_len = text_len_in_range(delta.old_file().id())?;
    let after_len = text_len_in_range(delta.new_file().id())?;
    Some(before_len.max(after_len))
}

fn write_csv(path: &Path, reservoirs: &HashMap<(String, &'static str), Reservoir>) -> Result<()> {
    let mut writer = csv::Writer::from_path(path)?;
    writer.write_record(["language", "size_bucket", "repository", "commit", "path", "old_path"])?;

    let mut keys: Vec<&(String, &'static str)> = reservoirs.keys().collect();
    keys.sort();

    for key in keys {
        let mut items: Vec<&Candidate> = reservoirs[key].items.iter().collect();
        items.sort_by(|a, b| {
            (&a.repository, &a.commit, &a.path).cmp(&(&b.repository, &b.commit, &b.path))
        });

        let (language, bucket) = key;
        for item in items {
            writer.write_record([
                language.as_str(),
                bucket,
                item.repository.as_str(),
                item.commit.as_str(),
                item.path.as_str(),
                item.old_path.as_str(),
            ])?;
        }
    }

    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use codediff::test::helper;

    #[test]
    fn samples_real_pairs_from_handmade_repository() -> Result<()> {
        let repo_path = helper::handmade_git_repository()?;
        let mut rng = StdRng::seed_from_u64(1);
        let mut reservoirs: HashMap<(String, &'static str), Reservoir> = HashMap::new();

        sample_repository(&repo_path, "handmade", 10, &mut reservoirs, &mut rng)?;

        let rust = reservoirs
            .get(&("Rust".to_string(), "small"))
            .expect("expected at least one sampled small Rust pair");
        assert!(!rust.items.is_empty());
        assert!(rust.items.iter().any(|c| c.path.ends_with("main.rs")));
        for item in &rust.items {
            assert_eq!(item.repository, "handmade");
            assert!(!item.commit.is_empty());
            assert_eq!(item.old_path, item.path);
        }

        Ok(())
    }

    #[test]
    fn reservoir_never_exceeds_capacity() {
        let mut rng = StdRng::seed_from_u64(7);
        let mut reservoir = Reservoir::default();

        for i in 0..100 {
            reservoir.offer(
                Candidate {
                    repository: "r".to_string(),
                    commit: i.to_string(),
                    path: "p".to_string(),
                    old_path: "p".to_string(),
                },
                10,
                &mut rng,
            );
        }

        assert_eq!(reservoir.items.len(), 10);
        assert_eq!(reservoir.seen, 100);
    }

    #[test]
    fn size_bucket_boundaries() {
        assert_eq!(size_bucket(1), "small");
        assert_eq!(size_bucket(999), "small");
        assert_eq!(size_bucket(1_000), "medium");
        assert_eq!(size_bucket(10_000), "large");
        assert_eq!(size_bucket(100_000), "xlarge");
        assert_eq!(size_bucket(MAX_BYTES), "xlarge");
    }
}
