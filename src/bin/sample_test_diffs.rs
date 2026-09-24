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
//! Samples (repository, commit, path) pointers to single-file edits as candidate fixtures for
//! `src/test/data/diffs/`. Unlike `sample_code_pairs`, it tops up the existing
//! `src/test/data/sample.csv` to `--count` per language instead of starting over, so it can be
//! re-run against different checkout roots.
//!
//! `--stratified` samples per (language, [`codediff::stats::sampling::LOC_BUCKETS`] bucket), and
//! `--count` then means per bucket - unlike `sample_code_pairs --count`, a per-language total.
use anyhow::{Result, bail};
use clap::Parser;
use git2::Delta;
use rand::SeedableRng;
use rand::rngs::StdRng;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use codediff::anomalous_paths;
use codediff::code::Language;
use codediff::code::language::{language_for_path, language_for_path_and_content, to_treesitter};
use codediff::stats::filesystem::{find_git_repositories, for_each_repository};
use codediff::stats::git::{text_loc_if_in_range, walk_single_parent_commit_diffs};
use codediff::stats::sampling::{Reservoir, loc_bucket};

// The upper bound is the size `stats::expand_from_code` refuses to parse.
const MIN_BYTES: usize = 1;
const MAX_BYTES: usize = 1024 * 1024;

#[derive(Parser)]
struct Args {
    /// Root directory containing checked-out git repositories (or a single repository).
    #[arg(long, default_value = "/var/tmp/research/small/repositories")]
    repos_dir: PathBuf,

    /// Target number of samples per language. Existing rows in the output file already count
    /// towards this; only the shortfall is sampled.
    #[arg(long, default_value_t = 20)]
    count: usize,

    /// Where the (language, repository, commit, path) rows are read from and written to.
    #[arg(long)]
    output: Option<PathBuf>,

    /// RNG seed. Omitted by default so every run draws a fresh sample.
    #[arg(long)]
    seed: Option<u64>,

    /// Restrict sampling to this language (e.g. "Rust"), matching `Language`'s Debug name.
    /// Default tops up every tree-sitter-supported language found short of `--count`.
    #[arg(long)]
    language: Option<String>,

    /// Stop after walking this many commits per repository (most-recent-first). Repeated
    /// shallow fetches can deepen a clone far past its original depth.
    #[arg(long, default_value_t = 1000)]
    max_commits_per_repo: usize,

    /// Which research dataset (tiny/small/full/stratified) new rows are tagged with; decides
    /// where `human_solver` promotes them. Defaults to `--repos-dir`'s parent directory name
    /// (`.../research/small/repositories` -> "small"), or to "stratified" under `--stratified`,
    /// since there it records the sampling method rather than the checkout. A different value
    /// together with `--stratified` is rejected.
    #[arg(long)]
    dataset: Option<String>,

    /// Stratify sampling by [`codediff::stats::sampling::LOC_BUCKETS`] (of the larger side's line
    /// count) as well as language; `--count` becomes a target per (language, bucket).
    #[arg(long, default_value_t = false)]
    stratified: bool,
}

/// A pointer to a (before, after) code pair in a repository checkout.
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
    /// The `src/test/data/diffs/` case this row was promoted to, set by `human_solver`.
    promoted_to: String,
    /// Which research dataset this row was sampled from (see `Args::dataset`).
    dataset: String,
    /// One of `SAMPLED`/`PROMOTED`/`REJECTED`; only `human_solver` moves a row off `SAMPLED`.
    status: String,
    /// Free-form note, set via `human_solver` (a rejection's reason lands here).
    comment: String,
    /// `stats::sampling::loc_bucket` of `max(before_loc, after_loc)`, only for a row sampled
    /// under `--stratified`; an unbucketed row never counts towards a stratified target.
    size_bucket: Option<String>,
}

type SampleKey = (String, String, String);
/// What a target count is tracked per (see `capacity_key`).
type CapacityKey = (String, Option<String>);

/// The dataset of a row without one: every such row was sampled from the small checkout.
const LEGACY_DATASET: &str = "small";

/// The `status` of a row without one, from its `promoted_to`; never `REJECTED`, which postdates
/// the column.
fn default_status(promoted_to: &str) -> &'static str {
    if promoted_to.is_empty() {
        "SAMPLED"
    } else {
        "PROMOTED"
    }
}

fn default_output_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("sample.csv")
}

/// `--dataset`'s default: `repos_dir`'s parent directory name, per the
/// `.../research/<dataset>/repositories` convention.
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
        let promoted_to = record.get(4).unwrap_or("").to_string();
        let status = match record.get(6) {
            Some(status) if !status.is_empty() => status.to_string(),
            _ => default_status(&promoted_to).to_string(),
        };
        rows.push(Row {
            language: record[0].to_string(),
            repository: record[1].to_string(),
            commit: record[2].to_string(),
            path: record[3].to_string(),
            promoted_to,
            dataset: record.get(5).unwrap_or(LEGACY_DATASET).to_string(),
            status,
            comment: record.get(7).unwrap_or("").to_string(),
            size_bucket: record.get(8).filter(|s| !s.is_empty()).map(str::to_string),
        });
    }
    Ok(rows)
}

/// Resolves `--dataset` against `--stratified` (see `Args::dataset`). A conflict is an error, not
/// an override: either way round, stratified rows would land in a non-stratified corpus silently.
fn resolve_dataset(args: &Args) -> Result<String> {
    match (args.dataset.as_deref(), args.stratified) {
        (Some(dataset), true) if dataset != "stratified" => bail!(
            "--stratified samples are provenance-tagged \"stratified\" (the sampling method, not \
             a checkout) - pass --dataset stratified or omit --dataset, not --dataset {dataset}"
        ),
        (Some(dataset), _) => Ok(dataset.to_string()),
        (None, true) => Ok("stratified".to_string()),
        (None, false) => infer_dataset(&args.repos_dir).ok_or_else(|| {
            anyhow::anyhow!(
                "could not infer a dataset name from --repos-dir {:?} (expected \
                 .../<dataset>/repositories); pass --dataset explicitly",
                args.repos_dir
            )
        }),
    }
}

/// What a row counts towards for top-up: `(language, None)` normally, `(language, size_bucket)`
/// under `--stratified`.
fn capacity_key(language: &str, bucket: Option<&str>, stratified: bool) -> CapacityKey {
    (
        language.to_string(),
        if stratified {
            bucket.map(str::to_string)
        } else {
            None
        },
    )
}

fn main() -> Result<()> {
    let args = Args::parse();
    let output = args.output.clone().unwrap_or_else(default_output_path);
    let dataset = resolve_dataset(&args)?;

    let existing_rows = read_existing_rows(&output)?;
    let mut existing_counts: HashMap<CapacityKey, usize> = HashMap::new();
    let mut existing_keys: HashSet<SampleKey> = HashSet::new();
    for row in &existing_rows {
        *existing_counts
            .entry(capacity_key(
                &row.language,
                row.size_bucket.as_deref(),
                args.stratified,
            ))
            .or_default() += 1;
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

    let mut reservoirs: HashMap<CapacityKey, Reservoir<Row>> = HashMap::new();
    let mut capacities: HashMap<CapacityKey, usize> = HashMap::new();

    for_each_repository(&repo_paths, |repo_path, repository_name| {
        sample_repository(
            repo_path,
            repository_name,
            args.language.as_deref(),
            args.max_commits_per_repo,
            args.count,
            &dataset,
            args.stratified,
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
/// (commit, path) to the reservoir for its `capacity_key` (language, or (language, size bucket)
/// under `stratified`), topping up towards `target_count` per key.
#[allow(clippy::too_many_arguments)]
fn sample_repository(
    repo_path: &Path,
    repository_name: &str,
    language_filter: Option<&str>,
    max_commits: usize,
    target_count: usize,
    dataset: &str,
    stratified: bool,
    existing_counts: &HashMap<CapacityKey, usize>,
    existing_keys: &HashSet<SampleKey>,
    reservoirs: &mut HashMap<CapacityKey, Reservoir<Row>>,
    capacities: &mut HashMap<CapacityKey, usize>,
    rng: &mut StdRng,
) -> Result<()> {
    walk_single_parent_commit_diffs(repo_path, max_commits, false, |repo, id, delta| {
        // The schema locates both blobs by one `path`, so only in-place edits qualify (rename
        // detection is off).
        if delta.status() != Delta::Modified {
            return Ok(());
        }
        // A mode-only change.
        if delta.old_file().id() == delta.new_file().id() {
            return Ok(());
        }

        let Some(path) = delta.new_file().path() else {
            return Ok(());
        };
        if anomalous_paths::is_anomalous(path) {
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

        // The larger side decides the bucket, as in `sample_code_pairs`.
        let Some(before_loc) =
            text_loc_if_in_range(repo, delta.old_file().id(), MIN_BYTES, MAX_BYTES)
        else {
            return Ok(());
        };
        let Some(after_loc) =
            text_loc_if_in_range(repo, delta.new_file().id(), MIN_BYTES, MAX_BYTES)
        else {
            return Ok(());
        };
        let bucket = stratified.then(|| loc_bucket(before_loc.max(after_loc)));

        let cap_key = capacity_key(&language, bucket, stratified);
        let capacity = *capacities.entry(cap_key.clone()).or_insert_with(|| {
            target_count.saturating_sub(existing_counts.get(&cap_key).copied().unwrap_or(0))
        });

        let row = Row {
            language: language.clone(),
            repository: repository_name.to_string(),
            commit: id.to_string(),
            path,
            promoted_to: String::new(),
            dataset: dataset.to_string(),
            status: "SAMPLED".to_string(),
            comment: String::new(),
            size_bucket: bucket.map(str::to_string),
        };
        reservoirs
            .entry(cap_key)
            .or_default()
            .offer(row, capacity, rng);

        Ok(())
    })
}

fn write_csv(
    path: &Path,
    existing_rows: Vec<Row>,
    reservoirs: HashMap<CapacityKey, Reservoir<Row>>,
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
        "status",
        "comment",
        "size_bucket",
    ])?;
    for row in &rows {
        writer.write_record([
            &row.language,
            &row.repository,
            &row.commit,
            &row.path,
            &row.promoted_to,
            &row.dataset,
            &row.status,
            &row.comment,
            row.size_bucket.as_deref().unwrap_or(""),
        ])?;
    }
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use codediff::stats::sampling::LOC_BUCKETS;
    use codediff::test::helper;

    fn sample(
        repo_path: &Path,
        target_count: usize,
        existing: &[Row],
        seed: u64,
        stratified: bool,
    ) -> Result<Vec<Row>> {
        let mut existing_counts: HashMap<CapacityKey, usize> = HashMap::new();
        let mut existing_keys: HashSet<SampleKey> = HashSet::new();
        for row in existing {
            *existing_counts
                .entry(capacity_key(
                    &row.language,
                    row.size_bucket.as_deref(),
                    stratified,
                ))
                .or_default() += 1;
            existing_keys.insert((row.repository.clone(), row.commit.clone(), row.path.clone()));
        }

        let mut reservoirs: HashMap<CapacityKey, Reservoir<Row>> = HashMap::new();
        let mut capacities: HashMap<CapacityKey, usize> = HashMap::new();
        let mut rng = StdRng::seed_from_u64(seed);

        sample_repository(
            repo_path,
            "handmade",
            None,
            1000,
            target_count,
            "small",
            stratified,
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
        let rows = sample(&repo_path, 10, &[], 1, false)?;

        let rust: Vec<&Row> = rows.iter().filter(|r| r.language == "Rust").collect();
        assert!(!rust.is_empty());
        assert!(rust.iter().any(|r| r.path.ends_with("main.rs")));
        for row in &rust {
            assert_eq!(row.repository, "handmade");
            assert!(!row.commit.is_empty());
            assert_eq!(
                row.size_bucket, None,
                "not --stratified: no bucket recorded"
            );
        }

        Ok(())
    }

    #[test]
    fn stratified_sampling_records_a_size_bucket_per_row() -> Result<()> {
        let repo_path = helper::handmade_git_repository()?;
        let rows = sample(&repo_path, 10, &[], 1, true)?;

        let rust: Vec<&Row> = rows.iter().filter(|r| r.language == "Rust").collect();
        assert!(!rust.is_empty());
        for row in &rust {
            assert!(
                row.size_bucket.is_some(),
                "--stratified row missing its size bucket: {:?}",
                row.path
            );
        }

        Ok(())
    }

    #[test]
    fn stratified_top_up_ignores_unstratified_rows_and_counts_by_bucket() -> Result<()> {
        let repo_path = helper::handmade_git_repository()?;

        // An unbucketed row must not count towards any stratified per-bucket target.
        let unstratified_existing = Row {
            language: "Rust".to_string(),
            repository: "handmade".to_string(),
            commit: "0".repeat(40),
            path: "not-a-real-path.rs".to_string(),
            promoted_to: String::new(),
            dataset: "small".to_string(),
            status: "SAMPLED".to_string(),
            comment: String::new(),
            size_bucket: None,
        };

        let rows = sample(&repo_path, 10, &[unstratified_existing], 1, true)?;
        let rust_stratified: Vec<&Row> = rows
            .iter()
            .filter(|r| r.language == "Rust" && r.size_bucket.is_some())
            .collect();

        assert!(!rust_stratified.is_empty());
        use std::collections::HashSet as StdHashSet;
        let buckets: StdHashSet<&str> = rust_stratified
            .iter()
            .map(|r| r.size_bucket.as_deref().unwrap())
            .collect();
        assert!(
            buckets
                .iter()
                .all(|b| LOC_BUCKETS.iter().any(|(_, label)| label == b)),
            "unexpected bucket label(s): {:?}",
            buckets
        );

        Ok(())
    }

    #[test]
    fn tops_up_existing_samples_without_duplicates() -> Result<()> {
        let repo_path = helper::handmade_git_repository()?;

        let first_pass = sample(&repo_path, 1, &[], 1, false)?;
        let rust_count_after_first: usize =
            first_pass.iter().filter(|r| r.language == "Rust").count();
        assert_eq!(rust_count_after_first, 1);

        let second_pass = sample(&repo_path, 5, &first_pass, 2, false)?;
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

    fn args(repos_dir: &str, dataset: Option<&str>, stratified: bool) -> Args {
        Args {
            repos_dir: PathBuf::from(repos_dir),
            count: 1,
            output: None,
            seed: None,
            language: None,
            max_commits_per_repo: 1,
            dataset: dataset.map(str::to_string),
            stratified,
        }
    }

    #[test]
    fn dataset_defaults_to_the_repos_dir_parent_name() {
        let resolved = resolve_dataset(&args("/r/research/full/repositories", None, false));
        assert_eq!(resolved.unwrap(), "full");
        assert!(resolve_dataset(&args("/", None, false)).is_err());
    }

    #[test]
    fn stratified_defaults_to_the_stratified_dataset_and_rejects_any_other() {
        let checkout = "/r/research/small/repositories";
        assert_eq!(
            resolve_dataset(&args(checkout, None, true)).unwrap(),
            "stratified"
        );
        assert_eq!(
            resolve_dataset(&args(checkout, Some("stratified"), true)).unwrap(),
            "stratified"
        );
        assert!(resolve_dataset(&args(checkout, Some("small"), true)).is_err());
    }

    #[test]
    fn rows_without_dataset_or_status_columns_get_their_legacy_defaults() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let csv = dir.path().join("sample.csv");
        std::fs::write(
            &csv,
            "language,repository,commit,path,promoted_to\n\
             Rust,r,c,a.rs,\n\
             Rust,r,c,b.rs,rust-case\n",
        )?;
        let rows = read_existing_rows(&csv)?;
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.dataset == "small"));
        assert_eq!(rows[0].status, "SAMPLED");
        assert_eq!(rows[1].status, "PROMOTED");
        Ok(())
    }
}
