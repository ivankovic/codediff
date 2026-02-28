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
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use tree_sitter::Parser as TSParser;

use codediff::stats::CodeStats;
use codediff::stats::filesystem;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    path: PathBuf,

    #[arg(long)]
    db: PathBuf,

    #[arg(long)]
    threads: Option<usize>,

    #[arg(long, default_value_t = 1000)]
    queue_capacity: usize,
}

fn main() {
    let args = Args::parse();

    let project_path = &args.path;
    let db_path = &args.db;
    let n_threads = args.threads.unwrap_or_else(num_cpus::get);
    let queue_capacity = args.queue_capacity;

    if let Err(e) = file_stats(project_path, db_path, n_threads, queue_capacity) {
        eprintln!("Failed to compute file stats: {:?}", e)
    }
}

fn file_stats(
    project_path: &Path,
    db_path: &Path,
    n_threads: usize,
    queue_capacity: usize,
) -> Result<()> {
    let (path_tx, path_rx) = bounded::<PathBuf>(queue_capacity);
    let (stats_tx, stats_rx) = bounded::<(PathBuf, CodeStats)>(queue_capacity);

    let mut workers = Vec::with_capacity(n_threads);
    for _ in 0..n_threads {
        let path_rx = path_rx.clone();
        let stats_tx = stats_tx.clone();
        workers.push(thread::spawn(move || worker_loop(path_rx, stats_tx)));
    }
    drop(stats_tx);

    let project_path_owned = project_path.to_owned();

    let path_producer =
        thread::spawn(move || filesystem::all_files_from_path(&project_path_owned, path_tx));

    let stats_collector = thread::spawn(move || {
        let mut map: HashMap<PathBuf, CodeStats> = HashMap::new();
        while let Ok((path, stats)) = stats_rx.recv() {
            map.insert(path, stats);
        }
        map
    });

    let _ = path_producer.join();
    for w in workers {
        let _ = w.join();
    }

    let stats = stats_collector.join().expect("Stats collector panicked!");

    export_stats_sqlite(db_path, stats)?;

    Ok(())
}

fn worker_loop(path_rx: Receiver<PathBuf>, stats_tx: Sender<(PathBuf, CodeStats)>) {
    let mut parser = TSParser::new();

    while let Ok(path) = path_rx.recv() {
        let s = codediff::stats::for_path(&path, &mut parser);
        if stats_tx.send((path, s)).is_err() {
            break;
        }
    }
}

fn export_stats_sqlite(path: &Path, stats: HashMap<PathBuf, CodeStats>) -> Result<()> {
    let mut conn = Connection::open(path)?;

    conn.execute(
        r#"
        CREATE TABLE IF NOT EXISTS files (
            last_updated INTEGER NOT NULL,

            path TEXT PRIMARY KEY,

            language TEXT,
            tip TEXT,

            automatically_generated BOOLEAN,

            ast_nodes INTEGER,
            bytes INTEGER,

            failed_to_convert_to_utf8 INTEGER,
            failed_to_parse INTEGER,
            too_large_to_parse INTEGER
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
        // Language and tip don't implement ToSql and I don't want to add a dependency to rusqlite
        // from code.rs.
        //
        // Likewise, PathBuf also doesn't.
        let path = s.code.metadata.path.map(|p| p.to_str().map(String::from));
        let language = s.code.metadata.language.map(|l| l.to_string());
        let tip = s.code.metadata.tip.map(|t| t.to_string());

        tx.execute(
            r#"
            INSERT OR REPLACE INTO files (
                last_updated,
                path,
                language,
                tip,
                automatically_generated,
                ast_nodes,
                bytes,
                failed_to_convert_to_utf8,
                failed_to_parse,
                too_large_to_parse
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10);
            "#,
            params![
                now,
                path,
                language,
                tip,
                s.bytes as i64,
                s.automatically_generated as i32,
                s.ast_nodes as i64,
                s.failed_to_convert_to_utf8 as i32,
                s.failed_to_parse as i32,
                s.too_large_to_parse as i32,
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

    #[test]
    fn end_to_end() -> Result<()> {
        // Create a temporary git repository for testing
        let repo_path = helper::handmade_git_repository()?;

        Ok(())
    }
}

