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
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tree_sitter::Parser as TSParser;

use codediff::stats::CodeStats;
use codediff::stats::filesystem;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    path: PathBuf,

    #[arg(long)]
    db: PathBuf,

    /// Number of parsing worker threads. Defaults to (CPU cores - 1).
    #[arg(long)]
    threads: Option<usize>,

    #[arg(long, default_value_t = 1000)]
    queue_capacity: usize,

    /// How many file records to buffer in memory before each SQLite commit.
    #[arg(long, default_value_t = 500)]
    batch_size: usize,

    /// Hard cap on the database file size, in GB. The run stops gracefully (without losing
    /// already-committed data) once this is exceeded.
    #[arg(long, default_value_t = 100)]
    max_db_size_gb: u64,

    /// Only (re)process files of at least this many bytes. Rows are upserted by path, so this
    /// re-measures one size class in place without re-parsing the rest of the corpus.
    #[arg(long, default_value_t = 0)]
    min_bytes: u64,
}

fn main() {
    let args = Args::parse();

    lower_priority();

    let project_path = &args.path;
    let db_path = &args.db;
    let n_threads = args
        .threads
        .unwrap_or_else(|| num_cpus::get().saturating_sub(1).max(1));
    let queue_capacity = args.queue_capacity;
    let max_db_bytes = args.max_db_size_gb * 1024 * 1024 * 1024;

    if let Err(e) = file_stats(
        project_path,
        db_path,
        n_threads,
        queue_capacity,
        args.batch_size,
        max_db_bytes,
        args.min_bytes,
    ) {
        eprintln!("Failed to compute file stats: {:?}", e)
    }
}

/// Best-effort niceness bump. Must run before any worker thread is spawned, so they inherit it.
#[cfg(unix)]
fn lower_priority() {
    unsafe {
        libc::nice(10);
    }
}

#[cfg(not(unix))]
fn lower_priority() {}

fn file_stats(
    project_path: &Path,
    db_path: &Path,
    n_threads: usize,
    queue_capacity: usize,
    batch_size: usize,
    max_db_bytes: u64,
    min_bytes: u64,
) -> Result<()> {
    let (path_tx, path_rx) = bounded::<PathBuf>(queue_capacity);
    let (stats_tx, stats_rx) = bounded::<(PathBuf, CodeStats)>(queue_capacity);

    // Real corpora hold trees deep enough to overflow the default stack in the recursive AST
    // walks, and a stack overflow aborts the process where a panic could be caught.
    const WORKER_STACK_SIZE: usize = 256 * 1024 * 1024;

    let mut workers = Vec::with_capacity(n_threads);
    for _ in 0..n_threads {
        let path_rx = path_rx.clone();
        let stats_tx = stats_tx.clone();
        workers.push(
            thread::Builder::new()
                .stack_size(WORKER_STACK_SIZE)
                .spawn(move || worker_loop(path_rx, stats_tx))
                .expect("failed to spawn worker thread"),
        );
    }
    drop(stats_tx);

    let project_path_owned = project_path.to_owned();

    let path_producer = if min_bytes == 0 {
        thread::spawn(move || filesystem::all_files_from_path(&project_path_owned, path_tx))
    } else {
        let (raw_tx, raw_rx) = bounded::<PathBuf>(queue_capacity);
        let walker =
            thread::spawn(move || filesystem::all_files_from_path(&project_path_owned, raw_tx));
        thread::spawn(move || {
            for path in raw_rx {
                let large = std::fs::metadata(&path)
                    .map(|m| m.len() >= min_bytes)
                    .unwrap_or(false);
                if large && path_tx.send(path).is_err() {
                    break;
                }
            }
            drop(path_tx);
            walker.join().unwrap_or(Ok(()))
        })
    };

    // Streaming batched writes bound peak memory to O(batch_size); holding every file's stats
    // until the end exhausts memory on a multi-million-file corpus.
    let db_path_owned = db_path.to_owned();
    let writer =
        thread::spawn(move || writer_loop(&db_path_owned, stats_rx, batch_size, max_db_bytes));

    let _ = path_producer.join();
    for w in workers {
        let _ = w.join();
    }

    writer.join().expect("writer thread panicked!")?;

    Ok(())
}

fn worker_loop(path_rx: Receiver<PathBuf>, stats_tx: Sender<(PathBuf, CodeStats)>) {
    let mut parser = TSParser::new();

    while let Ok(path) = path_rx.recv() {
        let mut s = codediff::stats::for_path(&path, &mut parser);
        // Drop the raw contents before the item crosses the channel.
        s.code.contents = String::new();
        if stats_tx.send((path, s)).is_err() {
            break;
        }
    }
}

const PROGRESS_INTERVAL_FILES: u64 = 5000;

fn writer_loop(
    db_path: &Path,
    stats_rx: Receiver<(PathBuf, CodeStats)>,
    batch_size: usize,
    max_db_bytes: u64,
) -> Result<()> {
    let mut conn = Connection::open(db_path)?;
    create_tables(&conn)?;

    let start = Instant::now();
    let mut processed: u64 = 0;
    let mut last_progress_at: u64 = 0;
    let mut batch = Vec::with_capacity(batch_size);
    let mut stopped_early = false;

    while let Ok(item) = stats_rx.recv() {
        batch.push(item);
        if batch.len() >= batch_size {
            processed += write_batch(&mut conn, &mut batch)? as u64;
            batch.clear();

            if processed - last_progress_at >= PROGRESS_INTERVAL_FILES {
                last_progress_at = processed;
                report_progress(db_path, processed, start.elapsed());
            }

            if db_size_bytes(db_path)? >= max_db_bytes {
                eprintln!(
                    "file_stats: database reached the {} GB cap after {} files - \
                     stopping early (already-written data is preserved)",
                    max_db_bytes / (1024 * 1024 * 1024),
                    processed
                );
                stopped_early = true;
                break;
            }
        }
    }

    if !batch.is_empty() {
        processed += write_batch(&mut conn, &mut batch)? as u64;
    }

    if !stopped_early {
        report_progress(db_path, processed, start.elapsed());
    }

    Ok(())
}

fn report_progress(db_path: &Path, processed: u64, elapsed: Duration) {
    let rate = processed as f64 / elapsed.as_secs_f64().max(0.001);
    let db_mb = db_size_bytes(db_path).unwrap_or(0) / (1024 * 1024);
    eprintln!(
        "file_stats: {processed} files written, db size {db_mb} MB, elapsed {:.0}s ({rate:.1} files/sec)",
        elapsed.as_secs_f64()
    );
}

/// Size of the main database file, in bytes. Accurate for the size cap only because the database
/// uses the rollback journal, not WAL, so committed data lands in this file immediately.
fn db_size_bytes(db_path: &Path) -> Result<u64> {
    Ok(std::fs::metadata(db_path)?.len())
}

fn create_tables(conn: &Connection) -> Result<()> {
    conn.execute(
        r#"
        CREATE TABLE IF NOT EXISTS files (
            id INTEGER PRIMARY KEY,

            last_updated INTEGER NOT NULL,

            path TEXT NOT NULL UNIQUE,

            language TEXT,
            tip TEXT,

            automatically_generated BOOLEAN,

            ast_nodes INTEGER,
            bytes INTEGER,
            lines_of_code INTEGER,

            failed_to_convert_to_utf8 INTEGER,
            failed_to_parse INTEGER,
            too_large_to_parse INTEGER
        );
        "#,
        [],
    )?;

    // Keyed by `files.id`, not path, to keep the database small; join `files` to slice by
    // language or tip.
    conn.execute(
        r#"
        CREATE TABLE IF NOT EXISTS node_kind_counts (
            file_id INTEGER NOT NULL REFERENCES files(id),
            kind TEXT NOT NULL,
            count INTEGER NOT NULL,
            PRIMARY KEY (file_id, kind)
        );
        "#,
        [],
    )?;

    // Bucket B covers subtree sizes in [2^B, 2^(B+1)) (see `stats::KindStats`).
    conn.execute(
        r#"
        CREATE TABLE IF NOT EXISTS node_kind_subtree_size_histogram (
            file_id INTEGER NOT NULL REFERENCES files(id),
            kind TEXT NOT NULL,
            size_bucket INTEGER NOT NULL,
            count INTEGER NOT NULL,
            PRIMARY KEY (file_id, kind, size_bucket)
        );
        "#,
        [],
    )?;

    Ok(())
}

/// Writes one batch of file stats in a single transaction and returns how many files were
/// written. A path already present is updated in place, leaving no orphaned per-kind rows.
fn write_batch(conn: &mut Connection, batch: &mut Vec<(PathBuf, CodeStats)>) -> Result<usize> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let tx = conn.transaction()?;
    let mut written = 0;

    {
        let mut upsert_file = tx.prepare_cached(
            r#"
            INSERT INTO files (
                last_updated, path, language, tip, automatically_generated, ast_nodes, bytes,
                lines_of_code, failed_to_convert_to_utf8, failed_to_parse, too_large_to_parse
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(path) DO UPDATE SET
                last_updated = excluded.last_updated,
                language = excluded.language,
                tip = excluded.tip,
                automatically_generated = excluded.automatically_generated,
                ast_nodes = excluded.ast_nodes,
                bytes = excluded.bytes,
                lines_of_code = excluded.lines_of_code,
                failed_to_convert_to_utf8 = excluded.failed_to_convert_to_utf8,
                failed_to_parse = excluded.failed_to_parse,
                too_large_to_parse = excluded.too_large_to_parse
            RETURNING id;
            "#,
        )?;
        let mut upsert_kind_count = tx.prepare_cached(
            r#"
            INSERT OR REPLACE INTO node_kind_counts (file_id, kind, count)
            VALUES (?1, ?2, ?3);
            "#,
        )?;
        let mut upsert_histogram_bucket = tx.prepare_cached(
            r#"
            INSERT OR REPLACE INTO node_kind_subtree_size_histogram
                (file_id, kind, size_bucket, count)
            VALUES (?1, ?2, ?3, ?4);
            "#,
        )?;

        for (_, s) in batch.drain(..) {
            // Stringified here to keep rusqlite out of code.rs.
            let path = s
                .code
                .metadata
                .path
                .as_ref()
                .and_then(|p| p.to_str())
                .map(String::from);
            let language = s.code.metadata.language.map(|l| l.to_string());
            let tip = s.code.metadata.tip.map(|t| t.to_string());

            let Some(path) = path else { continue };

            let file_id: i64 = upsert_file.query_row(
                params![
                    now,
                    path,
                    language,
                    tip,
                    s.automatically_generated as i32,
                    s.ast_nodes as i64,
                    s.bytes as i64,
                    s.lines_of_code as i64,
                    s.failed_to_convert_to_utf8 as i32,
                    s.failed_to_parse as i32,
                    s.too_large_to_parse as i32,
                ],
                |row| row.get(0),
            )?;
            written += 1;

            for (kind, kind_stats) in &s.kind_stats {
                upsert_kind_count.execute(params![file_id, kind, kind_stats.count as i64])?;

                for (&size_bucket, &count) in &kind_stats.subtree_size_histogram {
                    upsert_histogram_bucket.execute(params![
                        file_id,
                        kind,
                        size_bucket,
                        count as i64
                    ])?;
                }
            }
        }
    }

    tx.commit()?;
    Ok(written)
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

        file_stats(
            &repo_path,
            db_path,
            2,
            1000,
            500,
            100 * 1024 * 1024 * 1024,
            0,
        )?;

        verify_database_contents(db_path)?;

        Ok(())
    }

    #[test]
    fn min_bytes_filters_before_anything_is_read() -> Result<()> {
        let repo_path = helper::handmade_git_repository()?;

        let db_file = NamedTempFile::new()?;
        let db_path = db_file.path();

        // A threshold above every file in the repository: the walk runs, nothing is measured.
        file_stats(
            &repo_path,
            db_path,
            2,
            1000,
            500,
            100 * 1024 * 1024 * 1024,
            u64::MAX,
        )?;
        let conn = Connection::open(db_path)?;
        let none: i64 = conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))?;
        assert_eq!(none, 0);

        file_stats(
            &repo_path,
            db_path,
            2,
            1000,
            500,
            100 * 1024 * 1024 * 1024,
            1,
        )?;
        let some: i64 = conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))?;
        let empty: i64 = conn.query_row("SELECT COUNT(*) FROM files WHERE bytes = 0", [], |r| {
            r.get(0)
        })?;
        assert!(some > 0);
        assert_eq!(empty, 0, "an empty file is below a one-byte threshold");

        Ok(())
    }

    #[test]
    fn small_batch_size_still_writes_everything() -> Result<()> {
        let repo_path = helper::handmade_git_repository()?;

        let db_file = NamedTempFile::new()?;
        let db_path = db_file.path();

        // Several batches plus a partial one, so batch boundaries are exercised.
        file_stats(&repo_path, db_path, 2, 1000, 1, 100 * 1024 * 1024 * 1024, 0)?;

        verify_database_contents(db_path)?;

        Ok(())
    }

    #[test]
    fn stops_early_once_db_size_cap_is_reached() -> Result<()> {
        let repo_path = helper::handmade_git_repository()?;

        let db_file = NamedTempFile::new()?;
        let db_path = db_file.path();

        file_stats(&repo_path, db_path, 2, 1000, 1, 1, 0)?;

        let conn = Connection::open(db_path)?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))?;
        assert!(
            count > 0,
            "run should have written at least one file before stopping early"
        );

        Ok(())
    }

    #[test]
    fn rerunning_against_the_same_db_updates_rather_than_duplicates() -> Result<()> {
        let repo_path = helper::handmade_git_repository()?;

        let db_file = NamedTempFile::new()?;
        let db_path = db_file.path();

        file_stats(
            &repo_path,
            db_path,
            2,
            1000,
            500,
            100 * 1024 * 1024 * 1024,
            0,
        )?;
        file_stats(
            &repo_path,
            db_path,
            2,
            1000,
            500,
            100 * 1024 * 1024 * 1024,
            0,
        )?;

        let conn = Connection::open(db_path)?;
        let files_count: i64 = conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))?;
        let distinct_paths: i64 =
            conn.query_row("SELECT COUNT(DISTINCT path) FROM files", [], |r| r.get(0))?;
        assert_eq!(
            files_count, distinct_paths,
            "re-running should update existing rows, not duplicate them"
        );

        let orphaned_kind_counts: i64 = conn.query_row(
            "SELECT COUNT(*) FROM node_kind_counts k \
             LEFT JOIN files f ON f.id = k.file_id WHERE f.id IS NULL",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(
            orphaned_kind_counts, 0,
            "node_kind_counts should not reference a stale file_id after a re-run"
        );

        Ok(())
    }

    fn verify_database_contents(db_path: &Path) -> Result<()> {
        let conn = Connection::open(db_path)?;

        let mut stmt =
            conn.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='files'")?;
        let table_exists = stmt.exists([])?;
        assert!(table_exists, "Files table should exist in the database");

        let mut count_stmt = conn.prepare("SELECT COUNT(*) as count FROM files")?;
        let count: i64 = count_stmt.query_row([], |row| row.get(0))?;
        assert!(
            count > 0,
            "Database should contain at least one file record"
        );

        let mut columns_stmt = conn.prepare(
            "SELECT path, language, tip, automatically_generated, ast_nodes, bytes, lines_of_code,
            failed_to_convert_to_utf8, failed_to_parse, too_large_to_parse
            FROM files LIMIT 1",
        )?;

        let mut main_rs_found = false;

        let mut rows = columns_stmt.query([])?;
        if let Some(row) = rows.next()? {
            let path: Option<String> = row.get(0)?;
            let _language: Option<String> = row.get(1)?;
            let tip: Option<String> = row.get(2)?;
            let automatically_generated: i32 = row.get(3)?;
            let ast_nodes: i64 = row.get(4)?;
            let bytes: i64 = row.get(5)?;
            let lines_of_code: i64 = row.get(6)?;
            let failed_to_convert_to_utf8: i32 = row.get(7)?;
            let failed_to_parse: i32 = row.get(8)?;
            let too_large_to_parse: i32 = row.get(9)?;

            assert!(path.is_some(), "Path should be present");

            if let Some(path_content) = &path
                && path_content.ends_with("main.rs")
            {
                main_rs_found = true;

                assert!(tip.is_some(), "Tip should be present");
                assert!(bytes > 0, "File should have some bytes");
                assert!(lines_of_code > 0, "Main should have some lines");

                assert_eq!(
                    automatically_generated, 0,
                    "Test files should not be marked as automatically generated"
                );
                assert!(ast_nodes > 0, "AST nodes should be non-zero");

                assert_eq!(
                    failed_to_convert_to_utf8, 0,
                    "Test files should not fail UTF-8 conversion"
                );
                assert_eq!(failed_to_parse, 0, "Test files should parse successfully");
                assert_eq!(
                    too_large_to_parse, 0,
                    "Test files should not be too large to parse"
                );

                if let Some(tip_content) = &tip {
                    assert!(
                        tip_content.contains("Code"),
                        "Tip should indicate this is code"
                    );
                }
            }
        }

        assert!(main_rs_found, "main.rs was not found! It MUST exist.");

        verify_node_kind_stats(&conn)?;

        Ok(())
    }

    fn verify_node_kind_stats(conn: &Connection) -> Result<()> {
        for table in ["node_kind_counts", "node_kind_subtree_size_histogram"] {
            let mut stmt = conn.prepare(&format!(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='{table}'"
            ))?;
            assert!(
                stmt.exists([])?,
                "{table} table should exist in the database"
            );
        }

        let mut counts_stmt = conn.prepare(
            "SELECT f.language, k.kind, k.count
             FROM node_kind_counts k JOIN files f ON f.id = k.file_id
             WHERE f.path LIKE '%main.rs' AND k.kind = 'function_item'",
        )?;
        let (language, kind, count): (Option<String>, String, i64) =
            counts_stmt.query_row([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        assert_eq!(language.as_deref(), Some("Rust"));
        assert_eq!(kind, "function_item");
        assert!(
            count > 0,
            "main.rs should contain at least one function_item"
        );

        let mut histogram_stmt = conn.prepare(
            "SELECT SUM(h.count)
             FROM node_kind_subtree_size_histogram h JOIN files f ON f.id = h.file_id
             WHERE f.path LIKE '%main.rs' AND h.kind = 'function_item'",
        )?;
        let bucketed_count: i64 = histogram_stmt.query_row([], |row| row.get(0))?;
        assert_eq!(
            bucketed_count, count,
            "subtree-size histogram buckets for function_item should sum to the same total as \
             node_kind_counts"
        );

        Ok(())
    }
}
