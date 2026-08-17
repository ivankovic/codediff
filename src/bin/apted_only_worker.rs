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

//! Single-pair worker for `apted_only_benchmark`: computes exactly one whole-tree tree-edit-
//! distance between two files via `apted::for_roots(..., Algorithm::Apted, ...)` - CodeDiff's own
//! bounded APTED implementation, with none of the 7-phase pipeline's pre-matching heuristics run
//! first (no hash descent, no bottom-up expansion, nothing). This is deliberately the most
//! favorable case for whole-tree tree-edit-distance: real commits touch a small fraction of a
//! file (see `research/papers/introductory-paper/main.tex`'s Phase 1 discussion), so running APTED
//! directly on the full trees, unaided, is what "just run a generic tree-diff algorithm" means in
//! practice.
//!
//! Exists as its own process, not an in-process call from `apted_only_benchmark`, so the driver
//! can enforce the RQ1 experiment's 1-second budget with an OS-level `kill()` instead of merely
//! abandoning a thread: APTED's cost on a large or pathological tree can run for minutes and
//! consume gigabytes, and most inputs in this experiment are expected to exceed the budget by
//! design (that is the point of the measurement) - a thread-abandon timeout, as used by
//! `benchmark_diff_pairs.rs` for the full pipeline (which rarely blows its own much larger 120s
//! budget), would accumulate unboundedly here. A killed process returns all its memory to the OS
//! immediately; an abandoned thread does not.

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use std::path::PathBuf;
use std::time::Instant;

use codediff::code::Code;
use codediff::code::language::language_for_path;
use codediff::diff::apted::{Algorithm, for_roots};
use codediff::diff::{ASTDiff, NodeCache};

#[derive(Parser)]
struct Args {
    /// Path to a file containing the "before" source text.
    #[arg(long)]
    before: PathBuf,

    /// Path to a file containing the "after" source text.
    #[arg(long)]
    after: PathBuf,

    /// Original repository-relative path (or just a filename with the right extension) - used
    /// only to detect the language via `language_for_path`. Its content is never read.
    #[arg(long)]
    lang_path: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let before_text = std::fs::read_to_string(&args.before)
        .with_context(|| format!("reading {:?}", args.before))?;
    let after_text = std::fs::read_to_string(&args.after)
        .with_context(|| format!("reading {:?}", args.after))?;

    let language = language_for_path(std::path::Path::new(&args.lang_path))
        .ok_or_else(|| anyhow!("no language detected for {}", args.lang_path))?;

    let before = Code::from_string(&before_text, &language);
    let after = Code::from_string(&after_text, &language);

    if before.ast.is_none() || after.ast.is_none() {
        return Err(anyhow!("failed to parse before/after as {language}"));
    }

    let node_cache = NodeCache::build(&before, &after);
    let mut diff = ASTDiff::default();

    let start = Instant::now();
    for_roots(
        &before,
        &after,
        &node_cache,
        Algorithm::Apted,
        "apted_only",
        &mut diff,
    );
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

    println!(
        "elapsed_ms={elapsed_ms} mapping_operations={}",
        diff.mapping.len()
    );
    Ok(())
}
