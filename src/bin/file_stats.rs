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

    /// Number of parsing worker threads. Defaults to (CPU cores - 1) so at least one core is
    /// always left free for the rest of the system, even on a fully loaded run.
    #[arg(long)]
    threads: Option<usize>,

    #[arg(long, default_value_t = 1000)]
    queue_capacity: usize,

    /// How many file records to buffer in memory before each SQLite commit. Keeps peak memory
    /// bounded regardless of corpus size, instead of accumulating every file's stats until the
    /// very end of the run.
    #[arg(long, default_value_t = 500)]
    batch_size: usize,

    /// Hard cap on the database file size, in GB. The run stops gracefully (without losing
    /// already-committed data) once this is exceeded.
    #[arg(long, default_value_t = 100)]
    max_db_size_gb: u64,
}

fn main() {
    let args = Args::parse();

    lower_priority();

    let project_path = &args.path;
    let db_path = &args.db;
    // Reserve one core for the rest of the system: a corpus scan should never be able to make
    // the machine unresponsive to other work.
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
    ) {
        eprintln!("Failed to compute file stats: {:?}", e)
    }
}

/// Best-effort niceness bump so CPU-bound worker threads yield to the rest of the system under
/// contention. Applied once in `main`, before any worker threads are spawned, so every spawned
/// thread inherits the lowered priority. Failure is not fatal - e.g. some sandboxes disallow it.
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
) -> Result<()> {
    let (path_tx, path_rx) = bounded::<PathBuf>(queue_capacity);
    let (stats_tx, stats_rx) = bounded::<(PathBuf, CodeStats)>(queue_capacity);

    // Real-world corpora contain pathologically deep trees (huge generated/minified files,
    // deeply nested JSON, long chained expressions) that overflow the default ~8MB thread stack
    // in the recursive AST walks (count_nodes, compute_kind_stats). A stack overflow aborts the
    // whole process unconditionally (unlike a panic, it can't be caught), losing every file
    // processed so far since results are only persisted at the very end. A generous stack size
    // raises the ceiling far enough that this is no longer the practical limit.
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

    let path_producer =
        thread::spawn(move || filesystem::all_files_from_path(&project_path_owned, path_tx));

    // The writer commits in small batches and never retains more than `batch_size` files' worth
    // of stats at once. This is the key difference from the earlier design, which accumulated a
    // `HashMap` of every file's stats in memory and only wrote to SQLite at the very end: on a
    // multi-million-file corpus that unbounded accumulation, combined with each entry still
    // holding the file's full raw source text, exhausted system memory and hung the whole
    // machine. Streaming writes bound peak memory to O(batch_size) regardless of corpus size.
    let db_path_owned = db_path.to_owned();
    let writer = thread::spawn(move || writer_loop(&db_path_owned, stats_rx, batch_size, max_db_bytes));

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
        // The raw file contents are only needed to compute the derived stats above; keeping them
        // around after that just inflates the size of every in-flight item on the channel and,
        // previously, the size of the final in-memory accumulation. Drop them as soon as we're
        // done with them.
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

    // `recv()` returning `Err` means all workers are done and the channel is closed.
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

/// Size of the database on disk, in bytes. Deliberately reads only the main DB file (not any
/// `-wal`/`-journal` sidecar) - we intentionally use the default rollback-journal mode (see
/// `create_tables`) rather than WAL, so committed data is reflected in this file's size
/// immediately and the `max_db_size_gb` cap check stays accurate.
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

    // Per-(file, node kind) occurrence count, keyed by `files.id` rather than the file's path -
    // the path string would otherwise be repeated across dozens of kind rows per file, and at
    // multi-million-file scale that repetition is a meaningful chunk of database size.
    // `language` isn't duplicated here either - join against `files` to slice by language, tip,
    // etc: `SELECT f.language, k.kind, SUM(k.count) FROM node_kind_counts k JOIN files f ON
    // f.id = k.file_id GROUP BY f.language, k.kind ORDER BY f.language, 3 DESC`.
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

    // Per-(file, node kind, size bucket) histogram of subtree sizes, where bucket B covers
    // subtree sizes in [2^B, 2^(B+1)) (see `stats::KindStats`). Powers the "distribution of
    // subtree sizes per node kind" analysis: e.g. `SELECT f.language, h.kind, h.size_bucket,
    // SUM(h.count) FROM node_kind_subtree_size_histogram h JOIN files f ON f.id = h.file_id
    // GROUP BY f.language, h.kind, h.size_bucket ORDER BY 1, 2, 3`.
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
/// written. Re-running against paths already present in `db_path` updates the existing row (and
/// its `id`) in place rather than duplicating it, so repeated runs don't leave orphaned
/// node_kind_counts/node_kind_subtree_size_histogram rows behind.
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
            // Language and tip don't implement ToSql and I don't want to add a dependency to
            // rusqlite from code.rs. Likewise, PathBuf also doesn't.
            let path = s
                .code
                .metadata
                .path
                .as_ref()
                .and_then(|p| p.to_str())
                .map(String::from);
            let language = s.code.metadata.language.map(|l| l.to_string());
            let tip = s.code.metadata.tip.map(|t| t.to_string());

            // Without a path we have nothing to key rows on, and no way to join
            // node_kind_counts/node_kind_subtree_size_histogram back to `files` - skip entirely.
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
                    upsert_histogram_bucket
                        .execute(params![file_id, kind, size_bucket, count as i64])?;
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

        file_stats(&repo_path, db_path, 2, 1000, 500, 100 * 1024 * 1024 * 1024)?;

        verify_database_contents(db_path)?;

        Ok(())
    }

    #[test]
    fn small_batch_size_still_writes_everything() -> Result<()> {
        let repo_path = helper::handmade_git_repository()?;

        let db_file = NamedTempFile::new()?;
        let db_path = db_file.path();

        // Force multiple small batches (and a final partial batch) instead of one batch covering
        // the whole run, to make sure batching boundaries don't drop or duplicate files.
        file_stats(&repo_path, db_path, 2, 1000, 1, 100 * 1024 * 1024 * 1024)?;

        verify_database_contents(db_path)?;

        Ok(())
    }

    #[test]
    fn stops_early_once_db_size_cap_is_reached() -> Result<()> {
        let repo_path = helper::handmade_git_repository()?;

        let db_file = NamedTempFile::new()?;
        let db_path = db_file.path();

        // A 1-byte cap is exceeded as soon as the first batch is committed, so the run must stop
        // after writing at least one file rather than erroring out or hanging.
        file_stats(&repo_path, db_path, 2, 1000, 1, 1)?;

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

        file_stats(&repo_path, db_path, 2, 1000, 500, 100 * 1024 * 1024 * 1024)?;
        file_stats(&repo_path, db_path, 2, 1000, 500, 100 * 1024 * 1024 * 1024)?;

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
            // Verify we can read all the expected columns
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
            assert!(stmt.exists([])?, "{table} table should exist in the database");
        }

        // Join both new tables back to `files` by file_id, the way downstream analysis is
        // expected to: e.g. per-language node-kind distributions, or per-kind subtree-size
        // histograms.
        let mut counts_stmt = conn.prepare(
            "SELECT f.language, k.kind, k.count
             FROM node_kind_counts k JOIN files f ON f.id = k.file_id
             WHERE f.path LIKE '%main.rs' AND k.kind = 'function_item'",
        )?;
        let (language, kind, count): (Option<String>, String, i64) =
            counts_stmt.query_row([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        assert_eq!(language.as_deref(), Some("Rust"));
        assert_eq!(kind, "function_item");
        assert!(count > 0, "main.rs should contain at least one function_item");

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
