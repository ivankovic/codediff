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
use git2::Delta;
use rand::SeedableRng;
use rand::rngs::StdRng;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use codediff::anomalous_paths;
use codediff::code::Language;
use codediff::code::language::{language_for_path, language_for_path_and_content, to_treesitter};
use codediff::stats::filesystem::{find_git_repositories, for_each_repository};
use codediff::stats::git::{text_loc_if_in_range, walk_single_parent_commit_diffs};
use codediff::stats::sampling::{LOC_BUCKETS, Reservoir, loc_bucket};

// The upper bound is the size `stats::expand_from_code` refuses to parse.
const MIN_BYTES: usize = 1;
const MAX_BYTES: usize = 1024 * 1024;

// Sampling is stratified across `LOC_BUCKETS` per language so the rare large files, where
// tree-edit-distance cost grows super-linearly, are not drowned out by small ones.

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

    /// Restrict sampling to this language (e.g. "Rust"), matching `Language`'s Debug name.
    /// Default samples every tree-sitter-supported language found.
    #[arg(long)]
    language: Option<String>,

    /// Stop after walking this many commits per repository (most-recent-first). Repeated
    /// shallow fetches can deepen a clone far past its original depth.
    #[arg(long, default_value_t = 1000)]
    max_commits_per_repo: usize,
}

/// A pointer to a (before, after) code pair in a repository checkout.
///
/// Reconstruction contract: before = blob at `old_path` in `commit`'s (single) parent tree,
/// after = blob at `path` in `commit`'s tree. `old_path` equals `path` except for renames.
struct Candidate {
    repository: String,
    commit: String,
    path: String,
    old_path: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let repo_paths = find_git_repositories(&args.path)?;
    if repo_paths.is_empty() {
        eprintln!("No git repositories found in {:?}", args.path);
        return Ok(());
    }
    println!("Found {} repositories", repo_paths.len());

    let bucket_capacity = (args.count / LOC_BUCKETS.len()).max(1);

    let mut rng = StdRng::seed_from_u64(args.seed);
    let mut reservoirs: HashMap<(String, &'static str), Reservoir<Candidate>> = HashMap::new();

    for_each_repository(&repo_paths, |repo_path, repository_name| {
        sample_repository(
            repo_path,
            repository_name,
            bucket_capacity,
            args.language.as_deref(),
            args.max_commits_per_repo,
            &mut reservoirs,
            &mut rng,
        )
    });

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
    language_filter: Option<&str>,
    max_commits: usize,
    reservoirs: &mut HashMap<(String, &'static str), Reservoir<Candidate>>,
    rng: &mut StdRng,
) -> Result<()> {
    walk_single_parent_commit_diffs(repo_path, max_commits, true, |repo, id, delta| {
        if !matches!(delta.status(), Delta::Modified | Delta::Renamed) {
            return Ok(());
        }
        // A pure rename is a trivial diff pair.
        if delta.old_file().id() == delta.new_file().id() {
            return Ok(());
        }

        let Some(path) = delta.new_file().path() else {
            return Ok(());
        };
        let old_path = delta.old_file().path().unwrap_or(path);
        if anomalous_paths::is_anomalous(path) || anomalous_paths::is_anomalous(old_path) {
            return Ok(());
        }

        let Some(mut language) = language_for_path(path) else {
            return Ok(());
        };
        // Only `.ts` needs content to disambiguate (Qt Linguist vs. TypeScript); gating on it
        // avoids a blob read for every other file.
        if language == Language::TypeScript
            && let Ok(blob) = repo.find_blob(delta.new_file().id())
            && let Ok(text) = std::str::from_utf8(blob.content())
            && let Some(refined) = language_for_path_and_content(path, text)
        {
            language = refined;
        }
        if to_treesitter(&language).is_none() {
            return Ok(());
        }
        if let Some(filter) = language_filter
            && language.to_string() != filter
        {
            return Ok(());
        }

        // The larger side decides the bucket; a binary or out-of-range side skips the pair.
        let loc = text_loc_if_in_range(repo, delta.old_file().id(), MIN_BYTES, MAX_BYTES)
            .zip(text_loc_if_in_range(
                repo,
                delta.new_file().id(),
                MIN_BYTES,
                MAX_BYTES,
            ))
            .map(|(before_loc, after_loc)| before_loc.max(after_loc));
        let Some(loc) = loc else {
            return Ok(());
        };

        let candidate = Candidate {
            repository: repository_name.to_string(),
            commit: id.to_string(),
            path: path.to_string_lossy().into_owned(),
            old_path: old_path.to_string_lossy().into_owned(),
        };

        reservoirs
            .entry((language.to_string(), loc_bucket(loc)))
            .or_default()
            .offer(candidate, bucket_capacity, rng);

        Ok(())
    })
}

fn write_csv(
    path: &Path,
    reservoirs: &HashMap<(String, &'static str), Reservoir<Candidate>>,
) -> Result<()> {
    let mut writer = csv::Writer::from_path(path)?;
    writer.write_record([
        "language",
        "size_bucket",
        "repository",
        "commit",
        "path",
        "old_path",
    ])?;

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
        let mut reservoirs: HashMap<(String, &'static str), Reservoir<Candidate>> = HashMap::new();

        sample_repository(
            &repo_path,
            "handmade",
            10,
            None,
            1000,
            &mut reservoirs,
            &mut rng,
        )?;

        // Any bucket: which one is a property of the fixture's line count, not of sampling.
        let rust_items: Vec<_> = reservoirs
            .iter()
            .filter(|((language, _), _)| language == "Rust")
            .flat_map(|(_, reservoir)| &reservoir.items)
            .collect();
        assert!(
            !rust_items.is_empty(),
            "expected at least one sampled Rust pair"
        );
        assert!(rust_items.iter().any(|c| c.path.ends_with("main.rs")));
        for item in &rust_items {
            assert_eq!(item.repository, "handmade");
            assert!(!item.commit.is_empty());
            assert_eq!(item.old_path, item.path);
        }

        Ok(())
    }
}
