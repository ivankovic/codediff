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
use crossbeam_channel::{Receiver, Sender, bounded};
use rusqlite::{Connection, params};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Parser)]
struct Args {
    #[arg(long = "path")]
    path: PathBuf,

    #[arg(long)]
    db: PathBuf,

    #[arg(long)]
    threads: Option<usize>,

    #[arg(long, default_value_t = 10000)]
    queue_capacity: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
struct Stats {
    commit_id: String,
    relative_file_path: String,

    git_reported_status: String,

    // Absolute values
    bytes_before: u64,
    bytes_after: u64,

    lines_before: u64,
    lines_after: u64,

    nodes_before: u64,
    nodes_after: u64,

    // Measures of the difference
    unix_diff_script_bytes: u64,

    lines_added: u64,
    lines_removed: u64,
    lines_changed: u64,

    nodes_added: u64,
    nodes_removed: u64,
    nodes_changed: u64,
}

/// Find all git repositories in subfolders of the given path
fn find_git_repositories(base_path: &Path) -> Result<Vec<PathBuf>> {
    let mut repo_paths = Vec::new();

    println!("Looking for repositories...");

    if base_path.is_dir() {
        for entry in walkdir::WalkDir::new(base_path)
            .follow_links(true)
            .sort_by_file_name()
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_dir() {
                let git_dir = entry.path().join(".git");
                if git_dir.exists() {
                    repo_paths.push(entry.path().to_path_buf());
                }
            }
        }
    } else if base_path.join(".git").exists() {
        // Single repository path
        repo_paths.push(base_path.to_path_buf());
    }

    println!("Found {} repositories.", repo_paths.len());

    Ok(repo_paths)
}

/// Process multiple repositories in parallel using async
async fn process_repositories_parallel(
    repo_paths: &[PathBuf],
    db_path: &Path,
    n_threads: usize,
    queue_capacity: usize,
) -> Result<()> {
    let mut tasks = Vec::with_capacity(repo_paths.len());

    for repo_path in repo_paths {
        let db_path = db_path.to_path_buf();
        let repo_path = repo_path.to_path_buf();

        println!("Processing {}", repo_path.display());

        tasks.push(tokio::spawn(async move {
            repository_stats_async(repo_path, db_path, n_threads, queue_capacity).await
        }));
    }

    // Wait for all tasks to complete
    for task in tasks {
        task.await??;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let base_path = &args.path;
    let db_path = &args.db;
    let n_threads = args.threads.unwrap_or_else(num_cpus::get);
    let queue_capacity = args.queue_capacity;

    // Find all git repositories in subfolders
    let repo_paths = find_git_repositories(base_path)?;

    if repo_paths.is_empty() {
        eprintln!("No git repositories found in: {:?}", base_path);
        return Ok(());
    }

    println!("Found {} git repositories to process", repo_paths.len());

    // Process all repositories in parallel
    process_repositories_parallel(&repo_paths, db_path, n_threads, queue_capacity).await?;

    Ok(())
}

fn repository_stats(
    repository_path: &Path,
    db_path: &Path,
    n_threads: usize,
    queue_capacity: usize,
) -> Result<()> {
    let repo = git2::Repository::open(repository_path)?;

    let (delta_tx, delta_rx) = bounded::<(Stats, String, String)>(queue_capacity);
    let (stats_tx, stats_rx) = bounded::<Stats>(queue_capacity);

    let mut delta_workers = Vec::with_capacity(n_threads);
    for _ in 0..n_threads {
        let delta_rx = delta_rx.clone();
        let stats_tx = stats_tx.clone();
        delta_workers.push(thread::spawn(move || {
            process_delta_loop(delta_rx, stats_tx)
        }));
    }
    drop(stats_tx);

    let delta_producer = thread::spawn(move || process_repository(&repo, delta_tx));

    let stats_collector = thread::spawn(move || {
        let mut map: HashMap<(String, String), Stats> = HashMap::new();
        while let Ok(stats) = stats_rx.recv() {
            map.insert(
                (stats.commit_id.clone(), stats.relative_file_path.clone()),
                stats,
            );
        }
        map
    });

    let _ = delta_producer.join();
    for w in delta_workers {
        let _ = w.join();
    }
    let stats = stats_collector.join().expect("Stats collector panicked!");

    export_stats_sqlite(db_path, stats)?;

    Ok(())
}

/// Async version of repository_stats
async fn repository_stats_async(
    repository_path: PathBuf,
    db_path: PathBuf,
    n_threads: usize,
    queue_capacity: usize,
) -> Result<()> {
    // Run the synchronous version in a blocking task
    tokio::task::spawn_blocking(move || {
        repository_stats(&repository_path, &db_path, n_threads, queue_capacity)
    })
    .await??;

    Ok(())
}

fn path_from_delta(delta: &git2::DiffDelta) -> String {
    match delta.old_file().path() {
        Some(path) => String::from(path.to_string_lossy()),
        None => match delta.new_file().path() {
            Some(path) => String::from(path.to_string_lossy()),
            None => String::from(""),
        },
    }
}

fn process_repository(
    repo: &git2::Repository,
    delta_tx: Sender<(Stats, String, String)>,
) -> Result<()> {
    let mut walk = repo.revwalk()?;
    // We don't need to set sorting here, because we don't really care.
    walk.push_head()?;

    for id in walk {
        let id = id?;
        let commit = repo.find_commit(id)?;

        let before_tree = if commit.parents().len() > 0 {
            let parent = commit.parent(0)?;
            Some(parent.tree()?)
        } else {
            None
        };
        let after_tree = commit.tree()?;

        let mut diff_options = git2::DiffOptions::new();

        let diff = repo.diff_tree_to_tree(
            before_tree.as_ref(),
            Some(&after_tree),
            Some(&mut diff_options),
        )?;

        for delta in diff.deltas() {
            let mut result = Stats {
                commit_id: id.to_string(),
                relative_file_path: path_from_delta(&delta),
                ..Default::default()
            };

            let before: String;
            let after: String;

            match delta.status() {
                git2::Delta::Added => {
                    result.git_reported_status = String::from("Added");

                    before = String::from("");

                    let after_blob = repo.find_blob(delta.new_file().id())?;
                    after = String::from_utf8(after_blob.content().to_vec())?;
                }
                git2::Delta::Deleted => {
                    result.git_reported_status = String::from("Deleted");

                    let before_blob = repo.find_blob(delta.old_file().id())?;
                    before = String::from_utf8(before_blob.content().to_vec())?;

                    after = String::from("");
                }
                git2::Delta::Modified => {
                    result.git_reported_status = String::from("Modified");

                    let before_blob = repo.find_blob(delta.old_file().id())?;
                    before = String::from_utf8(before_blob.content().to_vec())?;

                    let after_blob = repo.find_blob(delta.new_file().id())?;
                    after = String::from_utf8(after_blob.content().to_vec())?;
                }
                _ => {
                    result.git_reported_status = String::from("Other");

                    before = String::from("");
                    after = String::from("");
                }
            }

            result.bytes_before = before.len() as u64;
            result.bytes_after = after.len() as u64;
            if delta_tx.send((result, before, after)).is_err() {
                break;
            }
        }
    }

    drop(delta_tx);

    Ok(())
}

fn process_delta_loop(
    delta_rx: Receiver<(Stats, String, String)>,
    stats_tx: Sender<Stats>,
) -> Result<()> {
    while let Ok((stats, before, after)) = delta_rx.recv() {
        match process_delta(&stats, &before, &after) {
            Ok(stats) => {
                if stats_tx.send(stats).is_err() {
                    break;
                }
            }
            Err(e) => {
                eprintln!("Failed to process: {:?}", e);
            }
        }
    }
    Ok(())
}

fn process_delta(stats: &Stats, before: &str, after: &str) -> Result<Stats> {
    let mut result = stats.clone();

    // Count lines
    result.lines_before = before.lines().count() as u64;
    result.lines_after = after.lines().count() as u64;

    // For now, we'll just set some basic values
    // The actual diff processing will be implemented later
    result.nodes_before = 0;
    result.nodes_after = 0;
    result.lines_added = 0;
    result.lines_removed = 0;
    result.lines_changed = 0;
    result.nodes_added = 0;
    result.nodes_removed = 0;
    result.nodes_changed = 0;

    Ok(result)
}

fn export_stats_sqlite(path: &Path, stats: HashMap<(String, String), Stats>) -> Result<()> {
    let mut conn = Connection::open(path)?;

    conn.execute(
        r#"
        CREATE TABLE IF NOT EXISTS commits (
            commit_id TEXT PRIMARY KEY,
            relative_file_path TEXT NOT NULL,

            last_updated INTEGER NOT NULL,

            git_reported_status TEXT NOT NULL,
            bytes_before INTEGER,
            bytes_after INTEGER,
            lines_before INTEGER,
            lines_after INTEGER,
            nodes_before INTEGER,
            nodes_after INTEGER,
            unix_diff_script_bytes INTEGER,
            lines_added INTEGER,
            lines_removed INTEGER,
            lines_changed INTEGER,
            nodes_added INTEGER,
            nodes_removed INTEGER,
            nodes_changed INTEGER
        );
        "#,
        [],
    )?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let tx = conn.transaction()?;

    for (_, s) in stats {
        tx.execute(
            r#"
            INSERT OR REPLACE INTO commits (
                commit_id,
                relative_file_path,
                last_updated,
                git_reported_status,
                bytes_before,
                bytes_after,
                lines_before,
                lines_after,
                nodes_before,
                nodes_after,
                unix_diff_script_bytes,
                lines_added,
                lines_removed,
                lines_changed,
                nodes_added,
                nodes_removed,
                nodes_changed
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17);
            "#,
            params![
                s.commit_id,
                s.relative_file_path,
                now,
                s.git_reported_status,
                s.bytes_before,
                s.bytes_after,
                s.lines_before,
                s.lines_after,
                s.nodes_before,
                s.nodes_after,
                s.unix_diff_script_bytes,
                s.lines_added,
                s.lines_removed,
                s.lines_changed,
                s.nodes_added,
                s.nodes_removed,
                s.nodes_changed
            ],
        )?;
    }

    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use codediff::test::helper;
    use rusqlite::Connection;
    use std::path::Path;
    use tempfile::NamedTempFile;

    #[test]
    fn end_to_end() -> Result<()> {
        let repo_path = helper::handmade_git_repository()?;

        let db_file = NamedTempFile::new()?;
        let db_path = db_file.path();

        repository_stats(&repo_path, db_path, 2, 1000)?;

        verify_database_contents(db_path)?;

        Ok(())
    }

    fn verify_database_contents(db_path: &Path) -> Result<()> {
        let conn = Connection::open(db_path)?;

        let mut stmt =
            conn.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='commits'")?;
        let table_exists = stmt.exists([])?;
        assert!(table_exists, "Commits table should exist in the database");

        let mut count_stmt = conn.prepare("SELECT COUNT(*) as count FROM commits")?;
        let count: i64 = count_stmt.query_row([], |row| row.get(0))?;
        assert!(
            count > 0,
            "Database should contain at least one commit record"
        );

        let mut columns_stmt = conn.prepare(
            "SELECT commit_id, relative_file_path, git_reported_status, bytes_before, bytes_after, lines_before, lines_after
            FROM commits WHERE git_reported_status != 'Added' LIMIT 1",
        )?;

        let mut main_rs_found = false;

        let mut rows = columns_stmt.query([])?;
        if let Some(row) = rows.next()? {
            // Verify we can read all the expected columns
            let commit_id: String = row.get(0)?;
            let relative_file_path: String = row.get(1)?;
            let git_reported_status: String = row.get(2)?;
            let bytes_before: i64 = row.get(3)?;
            let bytes_after: i64 = row.get(4)?;
            let lines_before: i64 = row.get(5)?;
            let lines_after: i64 = row.get(6)?;

            assert!(!commit_id.is_empty(), "Commit ID should be present");
            assert!(
                !relative_file_path.is_empty(),
                "Relative file path should be present"
            );
            assert!(
                !git_reported_status.is_empty(),
                "Git reported status should be present"
            );

            if relative_file_path.ends_with("main.rs") {
                main_rs_found = true;

                assert!(
                    bytes_before > 0,
                    "Bytes before should be strictly greater than 0"
                );
                assert!(
                    bytes_after > 0,
                    "Bytes after should be strictly greater than 0"
                );
                assert!(
                    lines_before > 0,
                    "Lines before should be strictly greater than 0"
                );
                assert!(
                    lines_after > 0,
                    "Lines after should be strictly greater than 0"
                );
            }
        }

        assert!(main_rs_found, "main.rs was not found! It MUST exist.");

        Ok(())
    }
}
