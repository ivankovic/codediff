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
//!
//! `--stratified` switches the sampling unit from "language" to "(language, LOC bucket)" - the
//! same [`codediff::stats::sampling::LOC_BUCKETS`] `sample_code_pairs` uses - so large/small files
//! are guaranteed representation in the resulting `diffs/stratified` corpus rather than large
//! files (rare in practice) being drowned out by small ones (common). `--count` under
//! `--stratified` means "per (language, bucket)", *not* "per language total" - unlike
//! `sample_code_pairs --count`, which is a per-language total split evenly across buckets.
use anyhow::{Result, bail};
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
use codediff::stats::git::{text_loc_if_in_range, walk_single_parent_commit_diffs};
use codediff::stats::sampling::{Reservoir, loc_bucket};

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

    /// Which research dataset (tiny/small/full/stratified) this run's newly-sampled rows are
    /// provenance-tagged with - recorded per row so a later promotion (`human_solver`) knows
    /// which of `codediff::test::helper::DIFF_DATASETS` to place the fixture under. Auto-inferred
    /// from `--repos-dir`'s parent directory name when omitted (`.../research/small/repositories`
    /// -> "small", matching this flag's own default and `materialize_test_diffs`'s
    /// `DEFAULT_REPO_ROOTS`) - except under `--stratified`, which defaults this to "stratified"
    /// instead (a stratified row's dataset records its *sampling method*, not which checkout
    /// happened to supply it, so inferring the checkout name here would be misleading - a
    /// `diffs/small/` fixture and a `diffs/stratified/` one sampled from the exact same checkout
    /// mean different things about how they were selected). Pass explicitly for a non-conventional
    /// checkout root, or to override the `--stratified` default; explicitly passing a *different*
    /// dataset together with `--stratified` is rejected outright rather than silently writing
    /// bucket-stratified rows into a non-stratified corpus. Rows already on disk keep whatever
    /// dataset they were originally sampled with, even if it differs from this run's.
    #[arg(long)]
    dataset: Option<String>,

    /// Stratify sampling by [`codediff::stats::sampling::LOC_BUCKETS`] (the larger of a pair's
    /// before/after line count) in addition to language, so `--count` becomes a target *per
    /// (language, bucket)* rather than per language - see this file's module doc comment.
    #[arg(long, default_value_t = false)]
    stratified: bool,
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
    /// Which research dataset (tiny/small/full/stratified) this row was sampled from - see
    /// `Args::dataset`. Carried through unchanged on every re-run, same as `promoted_to`: a row's
    /// provenance doesn't change just because a later run happens to target a different
    /// `--repos-dir`.
    dataset: String,
    /// One of `SAMPLED`/`PROMOTED`/`REJECTED` - `human_solver` moves a row from `SAMPLED` to
    /// whichever of the other two applies when the sample is triaged (`s` to promote, `R` to
    /// reject); this tool never sets anything but `SAMPLED` on a freshly-sampled row.
    status: String,
    /// Free-form note about this sample, set (and editable) via `human_solver`'s `e`/`R` prompts -
    /// independent of `status`, though `R` (reject) always sets it to the rejection reason. Empty
    /// if never set; this tool never writes anything but an empty value on a freshly-sampled row.
    comment: String,
    /// `stats::sampling::loc_bucket` of `max(before_loc, after_loc)`, recorded only for a row
    /// sampled under `--stratified` (`None` for every other row, including legacy rows and rows
    /// from an ordinary, non-stratified run - there's no cheap way to backfill a bucket for those
    /// without re-fetching their blobs, and no need to: `capacity_key` below only reads this field
    /// when stratifying, so an unbucketed row simply never counts towards a stratified target,
    /// which is the correct behavior, not a gap - see `Args::stratified`'s doc comment). Still
    /// written to (and read from) a `size_bucket` CSV column, not renamed even though the unit
    /// changed from bytes to lines - see `stats::sampling::loc_bucket`'s own doc comment for why.
    size_bucket: Option<String>,
}

type SampleKey = (String, String, String);
/// What a target count is tracked *per*: language alone normally, or (language, size bucket)
/// under `--stratified` - see `capacity_key`.
type CapacityKey = (String, Option<String>);

/// Historical default for any row read from a sample.csv written before provenance tracking
/// existed - every one of those was in fact sampled from the small research checkout (the only
/// one available when they were added), so this is a real fallback value, not a placeholder.
const LEGACY_DATASET: &str = "small";

/// Backfills `status` for a row read from a sample.csv written before that column existed: a
/// non-empty `promoted_to` means it was already promoted, otherwise it's just sitting there
/// unsampled -- there's no way a pre-existing row could be `REJECTED`, since rejection didn't
/// exist yet either.
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

/// `Args::dataset`/`Args::stratified`'s resolution rule - see `Args::dataset`'s doc comment for
/// why `--stratified` gets its own default rather than falling through to `infer_dataset`, and
/// why an explicit, *conflicting* `--dataset` is rejected rather than silently overridden or
/// silently honored (either of which would let bucket-stratified rows land in a non-stratified
/// corpus, or vice versa, with nothing on disk to say so).
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

/// What `row` counts towards for top-up purposes: `(language, None)` normally, aggregating every
/// existing row for that language regardless of its own `size_bucket` (unchanged from this tool's
/// pre-`--stratified` behavior); `(language, size_bucket)` under `--stratified`, so a legacy or
/// non-stratified row (whose `size_bucket` is `None`) correctly counts towards nothing, per
/// `Row::size_bucket`'s doc comment - it's not a real sample of that bucket, just a row that
/// predates bucket tracking or was sampled a different way.
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

        // The larger of before/after line count decides the bucket - same convention as
        // `sample_code_pairs`. Both counts are fetched (not just range-checked) even when
        // `!stratified`, since `text_loc_if_in_range` is already the cheapest read available here
        // and the boolean-only `in_range` this replaced did the identical two lookups anyway.
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

        // A pre-existing, non-stratified row for the same language: per `Row::size_bucket`'s doc
        // comment, this must not count towards any stratified per-bucket target - it isn't a
        // sample of a known bucket, just a row that predates (or opted out of) stratification.
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

        // The target is 10 *per bucket*; the handmade repository doesn't have 10 distinct Rust
        // candidates in every bucket, but it must not have been capped at "10 total, minus the
        // one pre-existing unstratified row" either - that would mean the unstratified row was
        // wrongly counted against a stratified bucket's budget.
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

        // First pass: deliberately under-sample so there is room to top up.
        let first_pass = sample(&repo_path, 1, &[], 1, false)?;
        let rust_count_after_first: usize =
            first_pass.iter().filter(|r| r.language == "Rust").count();
        assert_eq!(rust_count_after_first, 1);

        // Second pass, with the first pass's rows treated as already on disk: should top up to
        // exactly `target_count` (bounded by how many distinct candidates actually exist) and
        // must not reintroduce any row already present.
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
}
