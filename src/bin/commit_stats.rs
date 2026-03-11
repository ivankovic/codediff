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
use codediff::code::Language;
use crossbeam_channel::{Receiver, Sender, bounded};
use rusqlite::{Connection, params};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    repository_path: PathBuf,

    #[arg(long)]
    db: PathBuf,
}

#[derive(Debug, Clone, Default, Serialize)]
struct Stats {
    commit_id: String,
    relative_file_path: String,

    git_reported_status: String,

    // Absolute values
    bytes_before: u64,
    bytes_after: u64,
    nodes_before: u64,
    nodes_after: u64,

    // Measures of the difference
    nodes_added: u64,
    nodes_removed: u64,
    nodes_changed: u64,
    lines_added: u64,
    lines_removed: u64,
    lines_changed: u64,
    unix_diff_script_bytes: u64,
}

fn main() {
    let args = Args::parse();

    let repo = match git2::Repository::open(args.repository_path.as_path()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to open repository: {:?}", e);
            return;
        }
    };

    let n_threads = num_cpus::get();

    let (delta_tx, delta_rx) = bounded::<(Stats, String, String)>(10000);
    let (stats_tx, stats_rx) = bounded::<Stats>(10000);

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

    println!("Collected stats for {} files", stats.len());
    if let Err(e) = export_stats_sqlite(args.db.as_path(), stats) {
        eprintln!("Failed to save results: {:?}", e)
    }
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

    let _ = codediff::diff_strings(before, after, &Language::Unknown);

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
            bytes_after INTEGER
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
                bytes_after
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6);
            "#,
            params![
                s.commit_id,
                s.relative_file_path,
                now,
                s.git_reported_status,
                s.bytes_before,
                s.bytes_after
            ],
        )?;
    }

    tx.commit()?;
    Ok(())
}
