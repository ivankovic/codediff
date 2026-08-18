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

//! Driver for the introductory paper's RQ1 ("what percentage of real-world source-code changes
//! can a single, whole-tree tree-edit-distance computation complete within a one-second budget?").
//!
//! Reads one or more `sample_code_pairs`-format CSVs, extracts each pair's before/after blob
//! content directly (like `benchmark_diff_pairs.rs`), then times out `apted_only_worker` - a
//! separate process that runs *only* `apted::for_roots(..., Algorithm::Apted, ...)`, none of
//! CodeDiff's 7-phase pipeline - at exactly `--timeout-secs` (default 1s) per pair, via poll +
//! `kill()` rather than a thread-abandon timeout. See `apted_only_worker.rs`'s own doc comment for
//! why a killable subprocess, not a thread, is the correct primitive for this specific experiment:
//! most inputs are expected to exceed the budget by design, so a thread-abandon approach would
//! accumulate unboundedly running background computations instead of freeing their memory.
//!
//! Every pair is attempted, regardless of size - no `max_combined_nodes` skip filter. Skipping
//! large pairs would bias the headline RQ1 number exactly where the answer is most interesting
//! (the large tail), and the subprocess `kill()` already bounds resource use, which is the whole
//! reason this binary uses a subprocess instead of an in-process call.
//!
//! LOC/byte/AST-node sizes are computed by this driver, not by the worker, so they are available
//! for every pair (including ones the worker times out on) - the same design `benchmark_diff_pairs`
//! uses for its own `ast_nodes_before`/`after` columns.

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use git2::Repository;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use codediff::code::Code;
use codediff::code::language::language_for_path;
use codediff::stats::count_nodes;

#[derive(Parser)]
struct Args {
    /// One or more CSVs produced by `sample_code_pairs` (language, size_bucket, repository,
    /// commit, path, old_path). Multiple values, not a merged file, so per-language CSVs (this
    /// corpus's actual on-disk shape - see research/data/samples/sampled_code_pairs_*.csv) can be passed
    /// directly without a separate concatenation step.
    #[arg(long, required = true, num_args = 1..)]
    csv: Vec<PathBuf>,

    /// Root directory containing the checked-out repositories named in the CSVs.
    #[arg(long)]
    repo_root: PathBuf,

    /// Where to write per-pair measurements.
    #[arg(long)]
    output: PathBuf,

    /// Path to the apted_only_worker binary. Defaults to the sibling binary next to this one
    /// (same target/release or target/debug directory), so the common case needs no flag.
    #[arg(long)]
    worker_bin: Option<PathBuf>,

    /// Hard per-pair budget in seconds. The worker process is killed if it has not exited by the
    /// time this elapses; the reported `elapsed_ms` for a successful pair comes from the worker's
    /// own `Instant` measurement (parse/spawn overhead excluded), not this driver's poll loop.
    #[arg(long, default_value_t = 1.0)]
    timeout_secs: f64,

    /// How often to poll the worker process for completion.
    #[arg(long, default_value_t = 10)]
    poll_interval_ms: u64,
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
    loc_before: usize,
    loc_after: usize,
    loc_combined: usize,
    bytes_before: usize,
    bytes_after: usize,
    ast_nodes_before: usize,
    ast_nodes_after: usize,
    status: &'static str,
    elapsed_ms: Option<f64>,
}

fn read_pairs_csv(path: &Path) -> Result<Vec<SampledPair>> {
    let mut reader = csv::Reader::from_path(path)?;
    let mut pairs = Vec::new();
    for record in reader.deserialize() {
        pairs.push(record?);
    }
    Ok(pairs)
}

fn open_repo<'a>(
    repos: &'a mut HashMap<String, Repository>,
    repo_root: &Path,
    name: &str,
) -> Result<&'a Repository> {
    if !repos.contains_key(name) {
        repos.insert(name.to_string(), Repository::open(repo_root.join(name))?);
    }
    Ok(repos.get(name).unwrap())
}

fn blob_content(repo: &Repository, treeish: &str, path: &str) -> Result<Vec<u8>> {
    let tree = repo.revparse_single(treeish)?.peel_to_tree()?;
    codediff::stats::git::blob_bytes(repo, &tree, Path::new(path))
}

/// Runs `apted_only_worker` on `before_text`/`after_text`, killing it if it has not exited after
/// `timeout`. Returns `(status, elapsed_ms)` - `elapsed_ms` is the worker's own self-reported
/// timing, `None` unless status is "ok".
fn run_worker(
    worker_bin: &Path,
    before_text: &str,
    after_text: &str,
    lang_path: &str,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<(&'static str, Option<f64>)> {
    let mut before_file = tempfile::NamedTempFile::new().context("creating before temp file")?;
    before_file.write_all(before_text.as_bytes())?;
    let mut after_file = tempfile::NamedTempFile::new().context("creating after temp file")?;
    after_file.write_all(after_text.as_bytes())?;

    let mut child = Command::new(worker_bin)
        .arg("--before")
        .arg(before_file.path())
        .arg("--after")
        .arg(after_file.path())
        .arg("--lang-path")
        .arg(lang_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("spawning worker {worker_bin:?}"))?;

    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(poll_interval);
    };

    let Some(status) = status else {
        return Ok(("timed_out", None));
    };

    if !status.success() {
        return Ok(("worker_error", None));
    }

    let mut stdout = String::new();
    if let Some(mut out) = child.stdout.take() {
        out.read_to_string(&mut stdout)?;
    }

    let elapsed_ms = stdout
        .split_whitespace()
        .find_map(|tok| tok.strip_prefix("elapsed_ms="))
        .and_then(|v| v.parse::<f64>().ok());

    match elapsed_ms {
        Some(ms) => Ok(("ok", Some(ms))),
        None => Ok(("worker_error", None)),
    }
}

fn measure_pair(
    pair: &SampledPair,
    repo_root: &Path,
    repos: &mut HashMap<String, Repository>,
    worker_bin: &Path,
    timeout: Duration,
    poll_interval: Duration,
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

    let ast_nodes_before = before_code
        .ast
        .as_ref()
        .map_or(0, |a| count_nodes(a.root_node()));
    let ast_nodes_after = after_code
        .ast
        .as_ref()
        .map_or(0, |a| count_nodes(a.root_node()));

    let loc_before = before_text.lines().count();
    let loc_after = after_text.lines().count();

    let mut row = Row {
        language: pair.language.clone(),
        size_bucket: pair.size_bucket.clone(),
        repository: pair.repository.clone(),
        commit: pair.commit.clone(),
        path: pair.path.clone(),
        loc_before,
        loc_after,
        loc_combined: loc_before + loc_after,
        bytes_before: before_text.len(),
        bytes_after: after_text.len(),
        ast_nodes_before,
        ast_nodes_after,
        status: "ok",
        elapsed_ms: None,
    };

    if before_code.ast.is_none() || after_code.ast.is_none() {
        row.status = "parse_failed";
        return Ok(row);
    }

    let (status, elapsed_ms) = run_worker(
        worker_bin,
        &before_text,
        &after_text,
        &pair.path,
        timeout,
        poll_interval,
    )?;
    row.status = status;
    row.elapsed_ms = elapsed_ms;

    Ok(row)
}

fn write_row(writer: &mut csv::Writer<std::fs::File>, row: &Row) -> Result<()> {
    let opt_to_string = |v: Option<f64>| v.map(|x| x.to_string()).unwrap_or_default();

    writer.write_record([
        row.language.as_str(),
        row.size_bucket.as_str(),
        row.repository.as_str(),
        row.commit.as_str(),
        row.path.as_str(),
        &row.loc_before.to_string(),
        &row.loc_after.to_string(),
        &row.loc_combined.to_string(),
        &row.bytes_before.to_string(),
        &row.bytes_after.to_string(),
        &row.ast_nodes_before.to_string(),
        &row.ast_nodes_after.to_string(),
        row.status,
        &opt_to_string(row.elapsed_ms),
    ])?;
    Ok(())
}

fn default_worker_bin() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("resolving current_exe")?;
    let dir = exe
        .parent()
        .ok_or_else(|| anyhow!("current_exe has no parent directory"))?;
    let candidate = dir.join(if cfg!(windows) {
        "apted_only_worker.exe"
    } else {
        "apted_only_worker"
    });
    if !candidate.exists() {
        return Err(anyhow!(
            "no --worker-bin given and {candidate:?} does not exist - build it first \
             (cargo build --release --features test-fixtures --bin apted_only_worker)"
        ));
    }
    Ok(candidate)
}

fn main() -> Result<()> {
    let args = Args::parse();

    let worker_bin = match args.worker_bin {
        Some(p) => p,
        None => default_worker_bin()?,
    };
    let timeout = Duration::from_secs_f64(args.timeout_secs);
    let poll_interval = Duration::from_millis(args.poll_interval_ms);

    let mut pairs = Vec::new();
    for csv_path in &args.csv {
        let loaded = read_pairs_csv(csv_path)?;
        println!("Loaded {} pairs from {csv_path:?}", loaded.len());
        pairs.extend(loaded);
    }
    println!("Total: {} pairs", pairs.len());

    let mut writer = csv::Writer::from_path(&args.output)?;
    writer.write_record([
        "language",
        "size_bucket",
        "repository",
        "commit",
        "path",
        "loc_before",
        "loc_after",
        "loc_combined",
        "bytes_before",
        "bytes_after",
        "ast_nodes_before",
        "ast_nodes_after",
        "status",
        "elapsed_ms",
    ])?;

    let mut repos: HashMap<String, Repository> = HashMap::new();
    let mut status_counts: HashMap<&'static str, usize> = HashMap::new();
    let mut failed = 0;

    for (i, pair) in pairs.iter().enumerate() {
        match measure_pair(
            pair,
            &args.repo_root,
            &mut repos,
            &worker_bin,
            timeout,
            poll_interval,
        ) {
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
            }
        }

        println!(
            "[{}/{}] {}@{} {} (ok={} timed_out={} worker_error={} parse_failed={} failed_to_read={})",
            i + 1,
            pairs.len(),
            pair.repository,
            &pair.commit[..pair.commit.len().min(8)],
            pair.path,
            status_counts.get("ok").unwrap_or(&0),
            status_counts.get("timed_out").unwrap_or(&0),
            status_counts.get("worker_error").unwrap_or(&0),
            status_counts.get("parse_failed").unwrap_or(&0),
            failed,
        );
        let _ = std::io::stdout().flush();
    }

    println!(
        "Measured {} pairs into {:?}: ok={} timed_out={} worker_error={} parse_failed={} failed_to_read={}",
        pairs.len(),
        args.output,
        status_counts.get("ok").unwrap_or(&0),
        status_counts.get("timed_out").unwrap_or(&0),
        status_counts.get("worker_error").unwrap_or(&0),
        status_counts.get("parse_failed").unwrap_or(&0),
        failed,
    );

    Ok(())
}
