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
use anyhow::{Result, anyhow};
use clap::Parser;
use git2::Repository;
use serde::Deserialize;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::collections::HashMap;
use std::io::Write;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use codediff::code::Code;
use codediff::code::language::language_for_path;
use codediff::diff::diff_code;
use codediff::stats::count_nodes;

/// Counts heap allocation per thread, not per process, so a timed-out thread left running cannot
/// pollute the next pair's numbers.
struct CountingAllocator;

thread_local! {
    static CURRENT_BYTES: Cell<usize> = const { Cell::new(0) };
    static PEAK_BYTES: Cell<usize> = const { Cell::new(0) };
    static TOTAL_BYTES: Cell<usize> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            let size = layout.size();
            TOTAL_BYTES.with(|c| c.set(c.get() + size));
            let current = CURRENT_BYTES.with(|c| {
                let v = c.get() + size;
                c.set(v);
                v
            });
            PEAK_BYTES.with(|c| c.set(c.get().max(current)));
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        CURRENT_BYTES.with(|c| c.set(c.get().saturating_sub(layout.size())));
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// Returns this thread's (peak, total) allocated bytes and resets the counters.
fn take_thread_allocation_stats() -> (usize, usize) {
    let peak = PEAK_BYTES.with(|c| c.replace(0));
    let total = TOTAL_BYTES.with(|c| c.replace(0));
    CURRENT_BYTES.with(|c| c.set(0));
    (peak, total)
}

#[derive(Parser)]
struct Args {
    /// CSV produced by `sample_code_pairs` (language, size_bucket, repository, commit, path, old_path).
    /// Required unless `--fixtures` is set.
    #[arg(long, required_unless_present = "fixtures")]
    csv: Option<PathBuf>,

    /// Root directory containing the checked-out repositories named in the CSV. Required unless
    /// `--fixtures` is set.
    #[arg(long, required_unless_present = "fixtures")]
    repo_root: Option<PathBuf>,

    /// Measure every fixture directory of the paper's datasets under src/test/data/diffs/
    /// instead of sampled pairs. Rows carry the dataset in `repository`, the fixture name in
    /// `path`, and leave `size_bucket` and `commit` empty.
    #[arg(long, conflicts_with_all = ["csv", "repo_root"])]
    fixtures: bool,

    /// Where to write per-pair measurements.
    #[arg(long)]
    output: PathBuf,

    /// How many times to re-run diff_code per pair to get a stable median wall-clock time.
    #[arg(long, default_value_t = 5)]
    iterations: usize,

    /// Skip diff_code above this combined (before + after) AST node count. Skipped pairs still
    /// record their AST size.
    #[arg(long, default_value_t = 16_000)]
    max_combined_nodes: usize,

    /// Only repeat the timing measurement (up to `iterations` times) when the first call took
    /// less than this many milliseconds.
    #[arg(long, default_value_t = 1000.0)]
    fast_threshold_ms: f64,

    /// Abandon a single pair's diff_code call after this many seconds and record it as
    /// "timed_out".
    #[arg(long, default_value_t = 120.0)]
    timeout_secs: f64,
}

#[derive(Deserialize)]
struct SampledPair {
    language: String,
    size_bucket: String,
    repository: String,
    commit: String,
    path: String,
    old_path: String,
}

struct Row {
    language: String,
    size_bucket: String,
    repository: String,
    commit: String,
    path: String,
    bytes_before: usize,
    bytes_after: usize,
    ast_nodes_before: usize,
    ast_nodes_after: usize,
    status: &'static str,
    elapsed_ms: Option<f64>,
    peak_memory_bytes: Option<usize>,
    total_allocated_bytes: Option<usize>,
    mapping_operations: Option<usize>,
}

fn read_pairs_csv(path: &Path) -> Result<Vec<SampledPair>> {
    let mut reader = csv::Reader::from_path(path)?;
    let mut pairs = Vec::new();
    for record in reader.deserialize() {
        pairs.push(record?);
    }
    Ok(pairs)
}

/// At most this many repositories open at once. An open `Repository` holds a descriptor per
/// packfile touched, so an unbounded cache runs a whole-corpus run out of descriptors; the pair
/// lists are grouped by repository, so a handful is all the reuse there is.
const OPEN_REPOS_MAX: usize = 8;

fn open_repo<'a>(
    repos: &'a mut HashMap<String, Repository>,
    repo_root: &Path,
    name: &str,
) -> Result<&'a Repository> {
    if !repos.contains_key(name) {
        if repos.len() >= OPEN_REPOS_MAX {
            repos.clear();
        }
        repos.insert(name.to_string(), Repository::open(repo_root.join(name))?);
    }
    Ok(repos.get(name).unwrap())
}

fn blob_content(repo: &Repository, treeish: &str, path: &str) -> Result<Vec<u8>> {
    let tree = repo.revparse_single(treeish)?.peel_to_tree()?;
    codediff::stats::git::blob_bytes(repo, &tree, Path::new(path))
}

/// The settings every measurement runs under, in both modes.
#[derive(Clone, Copy)]
struct Budget {
    iterations: usize,
    max_combined_nodes: usize,
    fast_threshold_ms: f64,
    timeout_secs: f64,
}

impl Budget {
    fn from_args(args: &Args) -> Self {
        Budget {
            iterations: args.iterations,
            max_combined_nodes: args.max_combined_nodes,
            fast_threshold_ms: args.fast_threshold_ms,
            timeout_secs: args.timeout_secs,
        }
    }
}

fn measure_pair(
    pair: &SampledPair,
    repo_root: &Path,
    repos: &mut HashMap<String, Repository>,
    budget: Budget,
) -> Result<Row> {
    let repo = open_repo(repos, repo_root, &pair.repository)?;

    let before_text = String::from_utf8(blob_content(
        repo,
        &format!("{}^", pair.commit),
        &pair.old_path,
    )?)?;
    let after_text = String::from_utf8(blob_content(repo, &pair.commit, &pair.path)?)?;

    let language = language_for_path(Path::new(&pair.path))
        .ok_or_else(|| anyhow!("no language detected for {}", pair.path))?;

    let before_code = Code::from_string(&before_text, &language);
    let after_code = Code::from_string(&after_text, &language);

    let identity = Row {
        language: pair.language.clone(),
        size_bucket: pair.size_bucket.clone(),
        repository: pair.repository.clone(),
        commit: pair.commit.clone(),
        path: pair.path.clone(),
        bytes_before: 0,
        bytes_after: 0,
        ast_nodes_before: 0,
        ast_nodes_after: 0,
        status: "ok",
        elapsed_ms: None,
        peak_memory_bytes: None,
        total_allocated_bytes: None,
        mapping_operations: None,
    };
    Ok(measure_codes(identity, before_code, after_code, budget))
}

/// One fixture directory under src/test/data/diffs/ measured exactly like a sampled pair, with
/// the dataset directory standing in for the repository and the fixture name for the path.
fn measure_fixture(
    dataset: &str,
    name: &str,
    before_code: Code,
    after_code: Code,
    budget: Budget,
) -> Row {
    let language = before_code
        .metadata
        .language
        .map(|l| l.to_string())
        .unwrap_or_default();
    let identity = Row {
        language,
        size_bucket: String::new(),
        repository: dataset.to_string(),
        commit: String::new(),
        path: name.to_string(),
        bytes_before: 0,
        bytes_after: 0,
        ast_nodes_before: 0,
        ast_nodes_after: 0,
        status: "ok",
        elapsed_ms: None,
        peak_memory_bytes: None,
        total_allocated_bytes: None,
        mapping_operations: None,
    };
    measure_codes(identity, before_code, after_code, budget)
}

/// The measurement shared by both modes. `row` arrives with only its naming columns set.
fn measure_codes(mut row: Row, before_code: Code, after_code: Code, budget: Budget) -> Row {
    row.bytes_before = before_code.contents.len();
    row.bytes_after = after_code.contents.len();
    row.ast_nodes_before = before_code
        .ast
        .as_ref()
        .map_or(0, |a| count_nodes(a.root_node()));
    row.ast_nodes_after = after_code
        .ast
        .as_ref()
        .map_or(0, |a| count_nodes(a.root_node()));

    if row.ast_nodes_before + row.ast_nodes_after > budget.max_combined_nodes {
        row.status = "skipped_too_large";
        return row;
    }

    // The first call runs on its own thread so a hang can be abandoned after `timeout`; its
    // fresh allocator counters make it the memory measurement as well as the first sample.
    let before_arc = Arc::new(before_code);
    let after_arc = Arc::new(after_code);
    let (tx, rx) = mpsc::channel();
    {
        let before_arc = Arc::clone(&before_arc);
        let after_arc = Arc::clone(&after_arc);
        // The product's stack size, not the 2MB default: an overflow aborts the process past
        // `catch_unwind`, and would blame the diff for a harness limit.
        std::thread::Builder::new()
            .stack_size(codediff::tui::app::DIFF_COMPUTE_STACK_SIZE)
            .spawn(move || {
                let start = Instant::now();
                let result =
                    panic::catch_unwind(AssertUnwindSafe(|| diff_code(&before_arc, &after_arc)));
                let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
                let (peak_memory_bytes, total_allocated_bytes) = take_thread_allocation_stats();
                let _ = tx.send((result, elapsed_ms, peak_memory_bytes, total_allocated_bytes));
            })
            .expect("spawning the diff thread");
    }

    let (result, first_elapsed_ms, peak_memory_bytes, total_allocated_bytes) =
        match rx.recv_timeout(Duration::from_secs_f64(budget.timeout_secs)) {
            Ok(received) => received,
            Err(_) => {
                row.status = "timed_out";
                return row;
            }
        };

    row.peak_memory_bytes = Some(peak_memory_bytes);
    row.total_allocated_bytes = Some(total_allocated_bytes);

    let diff = match result {
        Ok(diff) => diff,
        Err(_) => {
            row.status = "panicked";
            return row;
        }
    };
    row.mapping_operations = diff.ast.as_ref().map(|a| a.mapping.len());

    // Timings include the counting allocator's overhead. Slow calls are stable and expensive, so
    // only fast ones are repeated, inline: the first call proved this input completes.
    let mut samples_ms = vec![first_elapsed_ms];
    if first_elapsed_ms < budget.fast_threshold_ms {
        for _ in 1..budget.iterations {
            let start = Instant::now();
            diff_code(&before_arc, &after_arc);
            samples_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        }
    }
    samples_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    row.elapsed_ms = Some(samples_ms[samples_ms.len() / 2]);

    row
}

fn write_row(writer: &mut csv::Writer<std::fs::File>, row: &Row) -> Result<()> {
    let opt_to_string = |v: Option<f64>| v.map(|x| x.to_string()).unwrap_or_default();
    let opt_usize_to_string = |v: Option<usize>| v.map(|x| x.to_string()).unwrap_or_default();

    writer.write_record([
        row.language.as_str(),
        row.size_bucket.as_str(),
        row.repository.as_str(),
        row.commit.as_str(),
        row.path.as_str(),
        &row.bytes_before.to_string(),
        &row.bytes_after.to_string(),
        &row.ast_nodes_before.to_string(),
        &row.ast_nodes_after.to_string(),
        row.status,
        &opt_to_string(row.elapsed_ms),
        &opt_usize_to_string(row.peak_memory_bytes),
        &opt_usize_to_string(row.total_allocated_bytes),
        &opt_usize_to_string(row.mapping_operations),
    ])?;
    Ok(())
}

/// The datasets the paper reports on. Mirrors `research/analysis/_common.py`'s PAPER_DATASETS;
/// keep the two in step.
///
/// Unlike every other producer, this walks fixture *directories*, not solved fixtures:
/// robustness scores nothing against ground truth, so an unsolved unit is a valid test case.
/// `paper_variables.robustness_fixtures` scopes the CSV back to the solved corpus for the paper.
const FIXTURE_DATASETS: &[&str] = &["small", "full", "stratified", "defects4j"];

/// Every fixture directory in the paper's datasets as `(dataset, name, dir)`, in
/// `handmade_test_case_dirs`' deterministic order.
fn paper_fixture_dirs() -> Result<Vec<(String, String, PathBuf)>> {
    let mut out = Vec::new();
    for (name, dir) in codediff::test::helper::handmade_test_case_dirs()? {
        let dataset = dir
            .parent()
            .and_then(|p| p.file_name())
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default();
        if FIXTURE_DATASETS.contains(&dataset.as_str()) {
            out.push((dataset, name, dir));
        }
    }
    Ok(out)
}

fn print_progress(
    i: usize,
    total: usize,
    label: &str,
    status_counts: &HashMap<&'static str, usize>,
    failed: usize,
) {
    println!(
        "[{}/{}] {} (ok={} skipped_too_large={} timed_out={} panicked={} failed_to_read={})",
        i + 1,
        total,
        label,
        status_counts.get("ok").unwrap_or(&0),
        status_counts.get("skipped_too_large").unwrap_or(&0),
        status_counts.get("timed_out").unwrap_or(&0),
        status_counts.get("panicked").unwrap_or(&0),
        failed,
    );
    let _ = std::io::stdout().flush();
}

fn main() -> Result<()> {
    let args = Args::parse();
    let budget = Budget::from_args(&args);

    let mut writer = csv::Writer::from_path(&args.output)?;
    writer.write_record([
        "language",
        "size_bucket",
        "repository",
        "commit",
        "path",
        "bytes_before",
        "bytes_after",
        "ast_nodes_before",
        "ast_nodes_after",
        "status",
        "elapsed_ms",
        "peak_memory_bytes",
        "total_allocated_bytes",
        "mapping_operations",
    ])?;

    // Silence panic output for the duration of the run: a panic on one pair is expected to
    // happen occasionally and is already captured in the "status" column.
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));

    let mut status_counts: HashMap<&'static str, usize> = HashMap::new();
    let mut failed = 0;
    let total;

    if args.fixtures {
        let fixtures = paper_fixture_dirs()?;
        total = fixtures.len();
        println!("Loaded {} fixture directories", total);

        for (i, (dataset, name, dir)) in fixtures.iter().enumerate() {
            match codediff::test::helper::code_pair_from_dir(dir) {
                Ok(Some((before, after))) => {
                    let row = measure_fixture(dataset, name, before, after, budget);
                    *status_counts.entry(row.status).or_insert(0) += 1;
                    write_row(&mut writer, &row)?;
                    writer.flush()?;
                }
                Ok(None) => {
                    failed += 1;
                    eprintln!(
                        "Failed to measure {}/{}: no before/after pair",
                        dataset, name
                    );
                }
                Err(e) => {
                    failed += 1;
                    eprintln!("Failed to measure {}/{}: {:?}", dataset, name, e);
                }
            }
            print_progress(
                i,
                total,
                &format!("{}/{}", dataset, name),
                &status_counts,
                failed,
            );
        }
    } else {
        let csv_path = args
            .csv
            .as_ref()
            .expect("--csv is required without --fixtures");
        let repo_root = args
            .repo_root
            .as_ref()
            .expect("--repo-root is required without --fixtures");
        let pairs = read_pairs_csv(csv_path)?;
        total = pairs.len();
        println!("Loaded {} sampled pairs", total);

        let mut repos: HashMap<String, Repository> = HashMap::new();
        for (i, pair) in pairs.iter().enumerate() {
            match measure_pair(pair, repo_root, &mut repos, budget) {
                Ok(row) => {
                    *status_counts.entry(row.status).or_insert(0) += 1;
                    write_row(&mut writer, &row)?;
                    writer.flush()?;
                }
                Err(e) => {
                    failed += 1;
                    eprintln!(
                        "Failed to measure {} {}: {:?}",
                        pair.repository, pair.path, e
                    );
                    // Every pair gets a row: research/measure/overnight_benchmarks.sh resumes
                    // from, and blames a killed attempt on, the first pair without one.
                    write_row(
                        &mut writer,
                        &Row {
                            language: pair.language.clone(),
                            size_bucket: pair.size_bucket.clone(),
                            repository: pair.repository.clone(),
                            commit: pair.commit.clone(),
                            path: pair.path.clone(),
                            bytes_before: 0,
                            bytes_after: 0,
                            ast_nodes_before: 0,
                            ast_nodes_after: 0,
                            status: "failed_to_read",
                            elapsed_ms: None,
                            peak_memory_bytes: None,
                            total_allocated_bytes: None,
                            mapping_operations: None,
                        },
                    )?;
                    writer.flush()?;
                }
            }
            print_progress(
                i,
                total,
                &format!(
                    "{}@{} {}",
                    pair.repository,
                    &pair.commit[..pair.commit.len().min(8)],
                    pair.path
                ),
                &status_counts,
                failed,
            );
        }
    }

    panic::set_hook(default_hook);

    println!(
        "Measured {} pairs into {:?}: ok={} skipped_too_large={} timed_out={} panicked={} failed_to_read={}",
        total,
        args.output,
        status_counts.get("ok").unwrap_or(&0),
        status_counts.get("skipped_too_large").unwrap_or(&0),
        status_counts.get("timed_out").unwrap_or(&0),
        status_counts.get("panicked").unwrap_or(&0),
        failed,
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use codediff::test::helper;
    use std::collections::HashMap as StdHashMap;

    fn budget(iterations: usize, max_combined_nodes: usize) -> Budget {
        Budget {
            iterations,
            max_combined_nodes,
            fast_threshold_ms: 1000.0,
            timeout_secs: 30.0,
        }
    }

    #[test]
    fn measures_a_real_pair_from_handmade_repository() -> Result<()> {
        let repo_path = helper::handmade_git_repository()?;
        let repo_root = repo_path.parent().unwrap().to_path_buf();
        let repository_name = repo_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();

        let repo = Repository::open(&repo_path)?;
        let mut walk = repo.revwalk()?;
        walk.push_head()?;

        let mut commit_with_parent = None;
        for id in walk {
            let id = id?;
            let commit = repo.find_commit(id)?;
            if commit.parents().len() == 1 {
                commit_with_parent = Some(id.to_string());
                break;
            }
        }
        let commit = commit_with_parent.expect("expected a non-root commit in the fixture");

        let pair = SampledPair {
            language: "Rust".to_string(),
            size_bucket: "small".to_string(),
            repository: repository_name,
            commit,
            path: "main.rs".to_string(),
            old_path: "main.rs".to_string(),
        };

        let mut repos: StdHashMap<String, Repository> = StdHashMap::new();
        let row = measure_pair(&pair, &repo_root, &mut repos, budget(2, 20_000))?;

        assert_eq!(row.status, "ok");
        assert!(row.ast_nodes_before > 0);
        assert!(row.ast_nodes_after > 0);
        assert!(row.elapsed_ms.is_some());
        assert!(row.peak_memory_bytes.is_some());
        assert!(row.mapping_operations.is_some());

        Ok(())
    }

    #[test]
    fn skips_pairs_above_the_node_ceiling() -> Result<()> {
        let repo_path = helper::handmade_git_repository()?;
        let repo_root = repo_path.parent().unwrap().to_path_buf();
        let repository_name = repo_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();

        let repo = Repository::open(&repo_path)?;
        let mut walk = repo.revwalk()?;
        walk.push_head()?;
        let commit = walk
            .filter_map(Result::ok)
            .find(|id| {
                repo.find_commit(*id)
                    .map(|c| c.parents().len() == 1)
                    .unwrap_or(false)
            })
            .unwrap()
            .to_string();

        let pair = SampledPair {
            language: "Rust".to_string(),
            size_bucket: "small".to_string(),
            repository: repository_name,
            commit,
            path: "main.rs".to_string(),
            old_path: "main.rs".to_string(),
        };

        let mut repos: StdHashMap<String, Repository> = StdHashMap::new();
        let row = measure_pair(&pair, &repo_root, &mut repos, budget(1, 0))?;

        assert_eq!(row.status, "skipped_too_large");
        assert!(row.elapsed_ms.is_none());
        assert!(row.mapping_operations.is_none());

        Ok(())
    }

    #[test]
    fn measures_a_fixture_directory() -> Result<()> {
        let fixtures = paper_fixture_dirs()?;
        assert!(!fixtures.is_empty());
        assert!(
            fixtures
                .iter()
                .all(|(dataset, _, _)| FIXTURE_DATASETS.contains(&dataset.as_str()))
        );

        let (dataset, name, dir) = &fixtures[0];
        let (before, after) = helper::code_pair_from_dir(dir)?.expect("fixture has both sides");
        let row = measure_fixture(dataset, name, before, after, budget(1, usize::MAX));

        assert_eq!(row.status, "ok");
        assert_eq!(&row.repository, dataset);
        assert_eq!(&row.path, name);
        assert!(row.commit.is_empty());
        assert!(!row.language.is_empty());
        assert!(row.ast_nodes_before > 0);
        assert!(row.elapsed_ms.is_some());
        assert!(row.mapping_operations.is_some());

        Ok(())
    }

    #[test]
    fn paper_fixture_dirs_include_fixtures_without_ground_truth() -> Result<()> {
        let fixtures = paper_fixture_dirs()?;
        assert!(
            fixtures
                .iter()
                .any(|(_, _, dir)| !dir.join("human_mapping.json").exists())
        );
        Ok(())
    }
}
