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

//! Re-derives the `tip` (file type) column of a `file_stats` database from each row's path,
//! using the current `code::tip::type_from_path`.
//!
//! `file_stats` classifies every file once, from its path, when it first walks a corpus; the
//! category then sits in `stats.sqlite` and feeds the paper's file-type figure. Widening the
//! classification tables therefore changes nothing on disk until either the whole corpus is
//! re-walked (hours over the Full List, and the checkouts may be gone) or the tips are
//! recomputed from the paths already stored. This does the latter, and it is the only part of a
//! `files` row that *can* be recomputed without the file: everything else (bytes, LOC, AST
//! counts) needs the contents.
//!
//! One consequence to keep in mind when reading the figure afterwards: a file that moves from
//! Unknown to Code or Configuration this way was never read, so it has no size or AST numbers.
//! The category shares are right; per-category size percentiles over such rows are not, until
//! the corpus is re-walked.
//!
//! Dry-run by default. Prints the category counts before and after, the biggest category
//! transitions, and the extensions that remain unclassified, so the tables in `code::tip` can be
//! iterated against a real corpus. `--write` applies the change in one transaction.

use anyhow::Result;
use clap::Parser;
use rusqlite::{Connection, params};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use codediff::code::tip::type_from_path;

#[derive(Parser)]
struct Args {
    /// The `stats.sqlite` written by `file_stats`.
    #[arg(long)]
    db: PathBuf,
    /// Apply the recomputed tips. Without this flag nothing is written.
    #[arg(long)]
    write: bool,
    /// How many still-unclassified extensions and extensionless names to list.
    #[arg(long, default_value_t = 60)]
    top: usize,
}

/// The gross category, i.e. the `Type` variant name that `analysis/file_stats.py` also cuts the
/// stored string at: everything before the first `(`.
fn category(tip: Option<&str>) -> &str {
    match tip {
        None | Some("") => "Unknown",
        Some(t) => t.split('(').next().unwrap_or(t),
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let mut conn = Connection::open(&args.db)?;

    let mut before: HashMap<String, u64> = HashMap::new();
    let mut after: HashMap<String, u64> = HashMap::new();
    let mut transitions: HashMap<(String, String), u64> = HashMap::new();
    let mut unknown_ext: HashMap<String, u64> = HashMap::new();
    let mut unknown_name: HashMap<String, u64> = HashMap::new();
    let mut changes: Vec<(i64, Option<String>)> = Vec::new();
    let mut total: u64 = 0;

    {
        let mut rows = conn.prepare("SELECT rowid, path, tip FROM files")?;
        let mut iter = rows.query([])?;
        while let Some(row) = iter.next()? {
            let id: i64 = row.get(0)?;
            let path: String = row.get(1)?;
            let old: Option<String> = row.get(2)?;
            let new = type_from_path(Path::new(&path)).map(|t| t.to_string());

            total += 1;
            let old_category = category(old.as_deref()).to_string();
            let new_category = category(new.as_deref()).to_string();
            *before.entry(old_category.clone()).or_default() += 1;
            *after.entry(new_category.clone()).or_default() += 1;
            if old_category != new_category {
                *transitions
                    .entry((old_category, new_category.clone()))
                    .or_default() += 1;
            }
            if new_category == "Unknown" {
                let name = path.rsplit('/').next().unwrap_or(&path);
                match name.trim_start_matches('.').rsplit_once('.') {
                    Some((_, ext)) => {
                        *unknown_ext.entry(ext.to_ascii_lowercase()).or_default() += 1
                    }
                    None => *unknown_name.entry(name.to_string()).or_default() += 1,
                }
            }
            if new != old {
                changes.push((id, new));
            }
        }
    }

    let pct = |n: u64| 100.0 * n as f64 / total.max(1) as f64;
    println!("{total} files");
    println!(
        "\n{:<16}{:>12}{:>8}{:>12}{:>8}",
        "category", "before", "%", "after", "%"
    );
    let mut categories: Vec<&String> = before.keys().chain(after.keys()).collect();
    categories.sort();
    categories.dedup();
    for c in categories {
        let b = before.get(c).copied().unwrap_or(0);
        let a = after.get(c).copied().unwrap_or(0);
        println!("{c:<16}{b:>12}{:>8.2}{a:>12}{:>8.2}", pct(b), pct(a));
    }

    let mut moved: Vec<_> = transitions.into_iter().collect();
    moved.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    println!("\ntransitions");
    for ((from, to), n) in moved {
        println!("{from:<14}-> {to:<14}{n:>12}");
    }

    let top = |title: &str, counts: HashMap<String, u64>| {
        let mut v: Vec<_> = counts.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        println!("\nstill unclassified, {title}");
        for (k, n) in v.into_iter().take(args.top) {
            println!("{n:>10}  {k}");
        }
    };
    top("by extension", unknown_ext);
    top("extensionless, by name", unknown_name);

    println!("\n{} rows would change", changes.len());
    if args.write {
        let tx = conn.transaction()?;
        {
            let mut update = tx.prepare_cached("UPDATE files SET tip = ?1 WHERE rowid = ?2")?;
            for (id, tip) in &changes {
                update.execute(params![tip, id])?;
            }
        }
        tx.commit()?;
        println!("{} rows updated", changes.len());
    } else {
        println!("dry run; pass --write to apply");
    }
    Ok(())
}
