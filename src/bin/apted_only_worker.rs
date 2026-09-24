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

//! Single-pair worker for `apted_only_benchmark`: one whole-tree tree-edit-distance between two
//! files via `Algorithm::AptedWholeTree`, with no pipeline phases and none of the engine's own
//! shortcuts (`Algorithm::Apted`'s Myers root split and `APTED_MAX_CELLS` decomposition). That is
//! what "just run a generic tree-diff algorithm" means.
//!
//! A separate process so the driver can enforce the 1-second budget with `kill()`: most inputs
//! exceed it by design, and an abandoned thread keeps its CPU and memory while a killed process
//! returns them.

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

    /// Path (or bare filename) used only to detect the language; never read.
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
        Algorithm::AptedWholeTree,
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
