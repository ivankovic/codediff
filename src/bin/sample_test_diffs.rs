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
//! Samples real (repository, commit, path) pointers to single-file edits, for use as candidates
//! for hand-curated unit tests under `src/test/data/diffs/`. Unlike `sample_code_pairs`, this
//! tool is meant to be run repeatedly against different checkout roots (the tiny/small/full
//! research datasets): it reads whatever is already in `src/test/data/sample.csv`, and tops up
//! each language to exactly `--count` samples rather than starting over every time.
use anyhow::Result;
use clap::Parser;
use git2::Delta;
use rand::SeedableRng;
use rand::rngs::StdRng;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use codediff::code::language::{language_for_path, to_treesitter};
use codediff::metadata;
use codediff::stats::filesystem::{find_git_repositories, for_each_repository};
use codediff::stats::git::{text_len_if_in_range, walk_single_parent_commit_diffs};
use codediff::stats::sampling::Reservoir;

// Files outside this range are excluded: near-empty files make trivial test fixtures, and
// anything above the upper bound is past the size `expand_from_code` itself treats as
// "too large to parse" (see `stats::expand_from_code`), so diff_code couldn't use it anyway.
const MIN_BYTES: usize = 1;
const MAX_BYTES: usize = 1024 * 1024;

#[derive(Parser)]
struct Args {
    /// Root directory containing checked-out git repositories (or a single repository). Run
    /// this tool again with a different root (e.g. the tiny/small/full research checkouts) to
    /// keep topping up the same sample.csv from a broader pool.
    #[arg(long, default_value = "/var/tmp/research/small/repositories")]
    repos_dir: PathBuf,

    /// Target number of samples per language. Existing rows in the output file already count
    /// towards this; only the shortfall is sampled.
    #[arg(long, default_value_t = 20)]
    count: usize,

    /// Where the (language, repository, commit, path) rows are read from and written to.
    #[arg(long)]
    output: Option<PathBuf>,

    /// RNG seed. Omitted by default so every run draws a genuinely fresh sample; set this only
    /// to get reproducible output (e.g. in tests).
    #[arg(long)]
    seed: Option<u64>,

    /// Restrict sampling to this language (e.g. "Rust"), matching `Language`'s Debug name.
    /// Default tops up every tree-sitter-supported language found short of `--count`.
    #[arg(long)]
    language: Option<String>,

    /// Stop after walking this many commits per repository (most-recent-first). Repos cloned
    /// with `git fetch --depth=N` repeatedly can accumulate far more local history than N as
    /// fetches deepen them over time, so an unbounded walk can take effectively forever on a
    /// long-lived project; this keeps each repo's contribution bounded.
    #[arg(long, default_value_t = 1000)]
    max_commits_per_repo: usize,

    /// Which research dataset (tiny/small/full) this run's newly-sampled rows are provenance-
    /// tagged with - recorded per row so a later promotion (`human_solver`) knows which of
    /// `codediff::test::helper::DIFF_DATASETS` to place the fixture under. Auto-inferred from
    /// `--repos-dir`'s parent directory name when omitted (`.../research/small/repositories` ->
    /// "small", matching this flag's own default and `materialize_test_diffs`'s
    /// `DEFAULT_REPO_ROOTS`); pass explicitly for a non-conventional checkout root. Rows already
    /// on disk keep whatever dataset they were originally sampled with, even if it differs from
    /// this run's.
    #[arg(long)]
    dataset: Option<String>,
}

/// A pointer to a (before, after) code pair: the actual content lives in the repository
/// checkout, not in this tool's output, so only enough is recorded to look it up again later.
///
/// Reconstruction contract: before = blob at `path` in `commit`'s (single) parent tree,
/// after = blob at `path` in `commit`'s tree. Renames are deliberately excluded (see
/// `sample_repository`), so `path` always names both sides.
#[derive(Clone, Eq, PartialEq)]
struct Row {
    language: String,
    repository: String,
    commit: String,
    path: String,
    /// Name of the `src/test/data/diffs/` test case this row was promoted to, if any (set by
    /// `human_solver`, not by this tool). Carried through unchanged on every re-run so topping up
    /// `sample.csv` never clobbers a promotion that already happened.
    promoted_to: String,
    /// Which research dataset (tiny/small/full) this row was sampled from - see `Args::dataset`.
    /// Carried through unchanged on every re-run, same as `promoted_to`: a row's provenance
    /// doesn't change just because a later run happens to target a different `--repos-dir`.
    dataset: String,
}

type SampleKey = (String, String, String);

/// Historical default for any row read from a sample.csv written before provenance tracking
/// existed - every one of those was in fact sampled from the small research checkout (the only
/// one available when they were added), so this is a real fallback value, not a placeholder.
const LEGACY_DATASET: &str = "small";

fn default_output_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("sample.csv")
}

/// `--dataset`'s auto-inference: `repos_dir`'s parent directory name, matching the
/// `.../research/<dataset>/repositories` convention `--repos-dir`'s own default and
/// `materialize_test_diffs`'s `DEFAULT_REPO_ROOTS` already use. `None` for a `--repos-dir` that
/// doesn't follow that convention (e.g. a bare repository path, or a custom checkout layout) -
/// callers should require `--dataset` explicitly in that case rather than guessing.
fn infer_dataset(repos_dir: &Path) -> Option<String> {
    repos_dir
        .parent()?
        .file_name()?
        .to_str()
        .map(|s| s.to_string())
}

fn read_existing_rows(path: &Path) -> Result<Vec<Row>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let mut reader = csv::Reader::from_path(path)?;
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record?;
        rows.push(Row {
            language: record[0].to_string(),
            repository: record[1].to_string(),
            commit: record[2].to_string(),
            path: record[3].to_string(),
            promoted_to: record.get(4).unwrap_or("").to_string(),
            dataset: record.get(5).unwrap_or(LEGACY_DATASET).to_string(),
        });
    }
    Ok(rows)
}

fn main() -> Result<()> {
    let args = Args::parse();
    let output = args.output.clone().unwrap_or_else(default_output_path);
    let dataset = match args.dataset.clone() {
        Some(dataset) => dataset,
        None => infer_dataset(&args.repos_dir).ok_or_else(|| {
            anyhow::anyhow!(
                "could not infer a dataset name from --repos-dir {:?} (expected \
                 .../<dataset>/repositories); pass --dataset explicitly",
                args.repos_dir
            )
        })?,
    };

    let existing_rows = read_existing_rows(&output)?;
    let mut existing_counts: HashMap<String, usize> = HashMap::new();
    let mut existing_keys: HashSet<SampleKey> = HashSet::new();
    for row in &existing_rows {
        *existing_counts.entry(row.language.clone()).or_default() += 1;
        existing_keys.insert((row.repository.clone(), row.commit.clone(), row.path.clone()));
    }

    let repo_paths = find_git_repositories(&args.repos_dir)?;
    if repo_paths.is_empty() {
        eprintln!("No git repositories found in {:?}", args.repos_dir);
        return Ok(());
    }
    println!("Found {} repositories", repo_paths.len());

    let mut rng = match args.seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => StdRng::from_entropy(),
    };

    let mut reservoirs: HashMap<String, Reservoir<Row>> = HashMap::new();
    let mut capacities: HashMap<String, usize> = HashMap::new();

    for_each_repository(&repo_paths, |repo_path, repository_name| {
        sample_repository(
            repo_path,
            repository_name,
            args.language.as_deref(),
            args.max_commits_per_repo,
            args.count,
            &dataset,
            &existing_counts,
            &existing_keys,
            &mut reservoirs,
            &mut capacities,
            &mut rng,
        )
    });

    let added: usize = reservoirs.values().map(|r| r.items.len()).sum();
    write_csv(&output, existing_rows, reservoirs)?;
    println!("Added {} new samples to {:?}", added, output);

    Ok(())
}

/// Walks every non-merge commit in the repository and offers each purely-modified file's
/// (commit, path) to the reservoir for its language, topping up towards `target_count`.
#[allow(clippy::too_many_arguments)]
fn sample_repository(
    repo_path: &Path,
    repository_name: &str,
    language_filter: Option<&str>,
    max_commits: usize,
    target_count: usize,
    dataset: &str,
    existing_counts: &HashMap<String, usize>,
    existing_keys: &HashSet<SampleKey>,
    reservoirs: &mut HashMap<String, Reservoir<Row>>,
    capacities: &mut HashMap<String, usize>,
    rng: &mut StdRng,
) -> Result<()> {
    walk_single_parent_commit_diffs(repo_path, max_commits, false, |repo, id, delta| {
        // Only in-place edits keep before and after at the same `path`, which is what the
        // (repository, commit, path) schema here relies on to locate both blobs later.
        // Rename detection is off (see the `false` above), so this also naturally excludes renames.
        if delta.status() != Delta::Modified {
            return Ok(());
        }
        // A no-op delta (e.g. a mode-only change) is not a useful diff pair.
        if delta.old_file().id() == delta.new_file().id() {
            return Ok(());
        }

        let Some(path) = delta.new_file().path() else {
            return Ok(());
        };
        if metadata::is_anomalous(path) {
            return Ok(());
        }

        let Some(language) = language_for_path(path) else {
            return Ok(());
        };
        // Only sample languages diff_code can actually parse.
        if to_treesitter(&language).is_none() {
            return Ok(());
        }
        let language = language.to_string();
        if let Some(filter) = language_filter
            && language != filter
        {
            return Ok(());
        }

        let path = path.to_string_lossy().into_owned();
        let key = (repository_name.to_string(), id.to_string(), path.clone());
        if existing_keys.contains(&key) {
            return Ok(());
        }

        let in_range =
            |oid: git2::Oid| text_len_if_in_range(repo, oid, MIN_BYTES, MAX_BYTES).is_some();
        if !in_range(delta.old_file().id()) || !in_range(delta.new_file().id()) {
            return Ok(());
        }

        let capacity = *capacities.entry(language.clone()).or_insert_with(|| {
            target_count.saturating_sub(existing_counts.get(&language).copied().unwrap_or(0))
        });

        let row = Row {
            language: language.clone(),
            repository: repository_name.to_string(),
            commit: id.to_string(),
            path,
            promoted_to: String::new(),
            dataset: dataset.to_string(),
        };
        reservoirs
            .entry(language)
            .or_default()
            .offer(row, capacity, rng);

        Ok(())
    })
}

fn write_csv(
    path: &Path,
    existing_rows: Vec<Row>,
    reservoirs: HashMap<String, Reservoir<Row>>,
) -> Result<()> {
    let mut rows = existing_rows;
    for (_, reservoir) in reservoirs {
        rows.extend(reservoir.items);
    }
    rows.sort_by(|a, b| {
        (&a.language, &a.repository, &a.commit, &a.path).cmp(&(
            &b.language,
            &b.repository,
            &b.commit,
            &b.path,
        ))
    });

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut writer = csv::Writer::from_path(path)?;
    writer.write_record([
        "language",
        "repository",
        "commit",
        "path",
        "promoted_to",
        "dataset",
    ])?;
    for row in &rows {
        writer.write_record([
            &row.language,
            &row.repository,
            &row.commit,
            &row.path,
            &row.promoted_to,
            &row.dataset,
        ])?;
    }
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use codediff::test::helper;

    fn sample(
        repo_path: &Path,
        target_count: usize,
        existing: &[Row],
        seed: u64,
    ) -> Result<Vec<Row>> {
        let mut existing_counts: HashMap<String, usize> = HashMap::new();
        let mut existing_keys: HashSet<SampleKey> = HashSet::new();
        for row in existing {
            *existing_counts.entry(row.language.clone()).or_default() += 1;
            existing_keys.insert((row.repository.clone(), row.commit.clone(), row.path.clone()));
        }

        let mut reservoirs: HashMap<String, Reservoir<Row>> = HashMap::new();
        let mut capacities: HashMap<String, usize> = HashMap::new();
        let mut rng = StdRng::seed_from_u64(seed);

        sample_repository(
            repo_path,
            "handmade",
            None,
            1000,
            target_count,
            "small",
            &existing_counts,
            &existing_keys,
            &mut reservoirs,
            &mut capacities,
            &mut rng,
        )?;

        let mut rows = existing.to_vec();
        for (_, reservoir) in reservoirs {
            rows.extend(reservoir.items);
        }
        Ok(rows)
    }

    #[test]
    fn samples_real_pairs_from_handmade_repository() -> Result<()> {
        let repo_path = helper::handmade_git_repository()?;
        let rows = sample(&repo_path, 10, &[], 1)?;

        let rust: Vec<&Row> = rows.iter().filter(|r| r.language == "Rust").collect();
        assert!(!rust.is_empty());
        assert!(rust.iter().any(|r| r.path.ends_with("main.rs")));
        for row in &rust {
            assert_eq!(row.repository, "handmade");
            assert!(!row.commit.is_empty());
        }

        Ok(())
    }

    #[test]
    fn tops_up_existing_samples_without_duplicates() -> Result<()> {
        let repo_path = helper::handmade_git_repository()?;

        // First pass: deliberately under-sample so there is room to top up.
        let first_pass = sample(&repo_path, 1, &[], 1)?;
        let rust_count_after_first: usize =
            first_pass.iter().filter(|r| r.language == "Rust").count();
        assert_eq!(rust_count_after_first, 1);

        // Second pass, with the first pass's rows treated as already on disk: should top up to
        // exactly `target_count` (bounded by how many distinct candidates actually exist) and
        // must not reintroduce any row already present.
        let second_pass = sample(&repo_path, 5, &first_pass, 2)?;
        let rust_rows: Vec<&Row> = second_pass
            .iter()
            .filter(|r| r.language == "Rust")
            .collect();

        assert!(rust_rows.len() > rust_count_after_first);
        assert!(rust_rows.len() <= 5);

        let mut seen: HashSet<SampleKey> = HashSet::new();
        for row in &rust_rows {
            let key = (row.repository.clone(), row.commit.clone(), row.path.clone());
            assert!(seen.insert(key), "duplicate row sampled: {}", row.path);
        }

        Ok(())
    }
}
