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
//! Turns the pointers sampled by `sample_test_diffs` into `before.<ext>.test` / `after.<ext>.test`
//! fixtures under `src/test/data/samples/`, not `src/test/data/diffs/`: every test sweeps
//! `diffs/` unfiltered and real files are expensive to diff, so a fixture enters `diffs/` only when
//! a human promotes it from `human_solver`.
//!
//! Each fixture also gets a `source.json` (its `sample.csv` row, which `human_solver` reads back
//! on promotion) and a `README.md` (see `codediff::stats::license`) recording provenance and the
//! license the content is actually under: it is someone else's code, not covered by codediff's
//! AGPL-3.0.
//!
//! Safe to re-run: byte-identical content is left alone and a missing README.md is backfilled.
use anyhow::{Result, bail};
use clap::Parser;
use git2::{Oid, Repository, Tree};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

use codediff::stats::license;

const DEFAULT_REPO_ROOTS: &[&str] = &[
    "/var/tmp/research/tiny/repositories",
    "/var/tmp/research/small/repositories",
    "/var/tmp/research/full/repositories",
];

#[derive(Parser)]
struct Args {
    /// CSV produced by `sample_test_diffs` (language, repository, commit, path).
    #[arg(long)]
    sample_csv: Option<PathBuf>,

    /// Root directory to search for a `<repository>` checkout. May be repeated; searched in
    /// order, first match wins. Defaults to the tiny/small/full research checkouts.
    #[arg(long)]
    repos_dir: Vec<PathBuf>,

    /// Directory to write `<name>/{before,after}.<ext>.test` fixtures into. Defaults to
    /// `src/test/data/samples/`.
    #[arg(long)]
    output_dir: Option<PathBuf>,

    /// Only materialize rows for this language (matching the CSV's `language` column).
    #[arg(long)]
    language: Option<String>,

    /// Also backfill a missing `README.md` (provenance/license attribution) into already
    /// promoted rows' `src/test/data/diffs/<dataset>/<promoted_to>/` directories. `REJECTED` rows
    /// have no directory and are never backfilled.
    #[arg(long)]
    include_triaged: bool,
}

#[derive(Serialize)]
struct Row {
    language: String,
    repository: String,
    commit: String,
    path: String,
    /// Which research dataset this row was sampled from (`sample_test_diffs`'s `--dataset`).
    /// Provenance only: it does not pick which `repos_dir` root is searched.
    dataset: String,
    /// `human_solver`'s triage state (`SAMPLED`/`PROMOTED`/`REJECTED`).
    status: String,
    /// The `diffs/<dataset>/` case name this row was promoted to; empty unless `PROMOTED`.
    #[serde(skip)]
    promoted_to: String,
}

enum Resolution {
    Create(PathBuf),
    AlreadyPresent(PathBuf),
}

fn default_sample_csv() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("sample.csv")
}

fn default_output_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("samples")
}

fn diffs_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("diffs")
}

fn read_rows(path: &Path) -> Result<Vec<Row>> {
    let mut reader = csv::Reader::from_path(path)?;
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record?;
        rows.push(Row {
            language: record[0].to_string(),
            repository: record[1].to_string(),
            commit: record[2].to_string(),
            path: record[3].to_string(),
            promoted_to: record[4].to_string(),
            // Rows without the column were all sampled from the small checkout.
            dataset: record.get(5).unwrap_or("small").to_string(),
            // As `sample_test_diffs::default_status`: never `REJECTED` without the column.
            status: record.get(6).map(str::to_string).unwrap_or_else(|| {
                if record[4].is_empty() {
                    "SAMPLED".to_string()
                } else {
                    "PROMOTED".to_string()
                }
            }),
        });
    }
    Ok(rows)
}

fn main() -> Result<()> {
    let args = Args::parse();

    let sample_csv = args.sample_csv.unwrap_or_else(default_sample_csv);
    let output_dir = args.output_dir.unwrap_or_else(default_output_dir);
    let repo_roots: Vec<PathBuf> = if args.repos_dir.is_empty() {
        DEFAULT_REPO_ROOTS.iter().map(PathBuf::from).collect()
    } else {
        args.repos_dir
    };

    let rows = read_rows(&sample_csv)?;

    let mut created = 0;
    let mut already_present = 0;
    let mut skipped = 0;

    for row in &rows {
        if let Some(filter) = args.language.as_deref()
            && row.language != filter
        {
            continue;
        }
        let result = if row.status == "SAMPLED" {
            materialize_row(row, &repo_roots, &output_dir)
        } else if args.include_triaged && row.status == "PROMOTED" {
            backfill_promoted_readme(row, &repo_roots)
        } else {
            continue;
        };

        match result {
            Ok(Resolution::Create(dir)) => {
                println!("created {:?}", dir);
                created += 1;
            }
            Ok(Resolution::AlreadyPresent(dir)) => {
                println!("already present {:?}", dir);
                already_present += 1;
            }
            Err(e) => {
                eprintln!(
                    "skipping {}@{} {}: {:?}",
                    row.repository, row.commit, row.path, e
                );
                skipped += 1;
            }
        }
    }

    println!(
        "Created {}, already present {}, skipped {}",
        created, already_present, skipped
    );

    Ok(())
}

fn find_repo_path(roots: &[PathBuf], repository: &str) -> Option<PathBuf> {
    roots
        .iter()
        .map(|root| root.join(repository))
        .find(|candidate| candidate.join(".git").exists())
}

fn blob_text(repo: &Repository, tree: &Tree, path: &Path) -> Result<String> {
    Ok(String::from_utf8(codediff::stats::git::blob_bytes(
        repo, tree, path,
    )?)?)
}

/// Directory name: `<language>-x-<repository>-<commit>-<filename>`, all lowercase, with the
/// repository's trailing `.git`, the path's directories, and the file's extension stripped (the
/// extension is carried by the fixture filenames instead). The commit is abbreviated to 8 hex
/// characters.
fn base_name(row: &Row) -> String {
    let language = row.language.to_lowercase();
    let repository = row
        .repository
        .strip_suffix(".git")
        .unwrap_or(&row.repository)
        .to_lowercase();
    let commit = &row.commit[..8.min(row.commit.len())];
    let filename = Path::new(&row.path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase().replace('.', "-"))
        .unwrap_or_default();

    format!("{language}-x-{repository}-{commit}-{filename}")
}

/// Finds (or claims) the directory to write this row's fixture into: an existing directory with
/// identical content is returned as already present, and one holding different content is
/// skipped for the next of `-2`, `-3`, ...
fn resolve_target(
    output_dir: &Path,
    base_name: &str,
    ext: &str,
    before: &str,
    after: &str,
) -> Result<Resolution> {
    for attempt in 1..1000 {
        let name = if attempt == 1 {
            base_name.to_string()
        } else {
            format!("{base_name}-{attempt}")
        };
        let dir = output_dir.join(&name);
        if !dir.exists() {
            return Ok(Resolution::Create(dir));
        }

        let before_path = dir.join(format!("before.{ext}.test"));
        let after_path = dir.join(format!("after.{ext}.test"));
        if let (Ok(existing_before), Ok(existing_after)) = (
            fs::read_to_string(&before_path),
            fs::read_to_string(&after_path),
        ) && existing_before == before
            && existing_after == after
        {
            return Ok(Resolution::AlreadyPresent(dir));
        }
    }
    bail!("too many name collisions for base name {base_name}");
}

/// Writes `README.md` into an already-promoted row's `diffs/<dataset>/<promoted_to>/` directory
/// when it has none (see `Args::include_triaged`). Touches nothing else.
fn backfill_promoted_readme(row: &Row, repo_roots: &[PathBuf]) -> Result<Resolution> {
    if row.promoted_to.is_empty() {
        bail!("row has status PROMOTED but no promoted_to name recorded");
    }
    let dir = diffs_root().join(&row.dataset).join(&row.promoted_to);
    if !dir.is_dir() {
        bail!("promoted case directory {:?} does not exist", dir);
    }

    let readme_path = dir.join("README.md");
    if readme_path.exists() {
        return Ok(Resolution::AlreadyPresent(dir));
    }

    // Best-effort: a commit that has aged out of a shallow clone still gets a README.md, with
    // `unverifiable_reason` saying why, rather than one claiming "no license". `repo_url` is
    // resolved outside the fallible closure because the repository usually outlives the commit.
    let repo = find_repo_path(repo_roots, &row.repository).and_then(|p| Repository::open(p).ok());
    let repo_url = repo.as_ref().and_then(license::origin_remote_url);

    let lookup: Result<Vec<license::LicenseFile>> = (|| {
        let repo = repo.as_ref().ok_or_else(|| {
            anyhow::anyhow!("repository {} not found in any root", row.repository)
        })?;
        let commit = repo.find_commit(Oid::from_str(&row.commit)?)?;
        let tree = commit.tree()?;
        Ok(license::find_license_files(repo, &tree))
    })();

    let (license_files, unverifiable_reason) = match lookup {
        Ok(license_files) => (license_files, None),
        Err(err) => {
            eprintln!(
                "  note: {:?} could not be verified against the local checkout ({:#}); writing a \
                 placeholder README.md instead",
                dir, err
            );
            (Vec::new(), Some(format!("{err:#}")))
        }
    };

    let readme = license::render_readme(
        repo_url.as_deref(),
        &row.repository,
        &row.commit,
        &row.path,
        &row.dataset,
        &license_files,
        unverifiable_reason.as_deref(),
    );
    fs::write(&readme_path, readme)?;

    Ok(Resolution::Create(dir))
}

fn materialize_row(row: &Row, repo_roots: &[PathBuf], output_dir: &Path) -> Result<Resolution> {
    let repo_path = find_repo_path(repo_roots, &row.repository)
        .ok_or_else(|| anyhow::anyhow!("repository {} not found in any root", row.repository))?;
    let repo = Repository::open(&repo_path)?;

    let commit = repo.find_commit(Oid::from_str(&row.commit)?)?;
    if commit.parents().len() != 1 {
        bail!("commit {} does not have exactly one parent", row.commit);
    }
    let parent_tree = commit.parent(0)?.tree()?;
    let tree = commit.tree()?;

    let path = Path::new(&row.path);
    let before = blob_text(&repo, &parent_tree, path)?;
    let after = blob_text(&repo, &tree, path)?;

    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .ok_or_else(|| anyhow::anyhow!("path {} has no extension", row.path))?;

    let base = base_name(row);
    let resolution = resolve_target(output_dir, &base, &ext, &before, &after)?;

    let dir = match &resolution {
        Resolution::Create(dir) => {
            fs::create_dir_all(dir)?;
            fs::write(dir.join(format!("before.{ext}.test")), &before)?;
            fs::write(dir.join(format!("after.{ext}.test")), &after)?;
            fs::write(dir.join("source.json"), serde_json::to_string_pretty(row)?)?;
            dir
        }
        Resolution::AlreadyPresent(dir) => dir,
    };

    // Never overwritten, so a hand-corrected README.md survives a re-run.
    let readme_path = dir.join("README.md");
    if !readme_path.exists() {
        let license_files = license::find_license_files(&repo, &tree);
        let repo_url = license::origin_remote_url(&repo);
        let readme = license::render_readme(
            repo_url.as_deref(),
            &row.repository,
            &row.commit,
            &row.path,
            &row.dataset,
            &license_files,
            None,
        );
        fs::write(&readme_path, readme)?;
    }

    Ok(resolution)
}

#[cfg(test)]
mod tests {
    use super::*;
    use codediff::test::helper;
    use git2::Sort;
    use tempfile::tempdir;

    /// Finds the first single-parent commit in `repo_path` and the path of a file it modified.
    fn find_a_modified_file(repo_path: &Path) -> (String, String) {
        let repo = Repository::open(repo_path).unwrap();
        let mut walk = repo.revwalk().unwrap();
        walk.set_sorting(Sort::TIME).unwrap();
        walk.push_head().unwrap();

        for id in walk {
            let id = id.unwrap();
            let commit = repo.find_commit(id).unwrap();
            if commit.parents().len() != 1 {
                continue;
            }
            let parent_tree = commit.parent(0).unwrap().tree().unwrap();
            let tree = commit.tree().unwrap();
            let diff = repo
                .diff_tree_to_tree(Some(&parent_tree), Some(&tree), None)
                .unwrap();
            for delta in diff.deltas() {
                if delta.status() == git2::Delta::Modified
                    && let Some(path) = delta.new_file().path()
                {
                    return (id.to_string(), path.to_string_lossy().into_owned());
                }
            }
        }
        panic!("handmade repository has no single-parent modified-file commit");
    }

    fn row_for(repo_path: &Path, commit: String, path: String) -> Row {
        Row {
            language: "Rust".to_string(),
            repository: repo_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            commit,
            path,
            dataset: "small".to_string(),
            status: "SAMPLED".to_string(),
            promoted_to: String::new(),
        }
    }

    #[test]
    fn base_name_matches_language_x_repository_commit_filename_scheme() {
        let row = Row {
            language: "Rust".to_string(),
            repository: "GyulyVGC-sniffnet.git".to_string(),
            commit: "1a70be36eb3d50a2b7248a76056fe9b3c2f71c82".to_string(),
            path: "src/gui/pages/overview_page.rs".to_string(),
            dataset: "small".to_string(),
            status: "SAMPLED".to_string(),
            promoted_to: String::new(),
        };
        assert_eq!(
            base_name(&row),
            "rust-x-gyulyvgc-sniffnet-1a70be36-overview_page"
        );
    }

    #[test]
    fn materializes_a_real_pair_from_handmade_repository() -> Result<()> {
        let repo_path = helper::handmade_git_repository()?;
        let (commit, path) = find_a_modified_file(&repo_path);
        let row = row_for(&repo_path, commit, path);

        let repo_roots = vec![repo_path.parent().unwrap().to_path_buf()];
        let output_dir = tempdir()?;

        let resolution = materialize_row(&row, &repo_roots, output_dir.path())?;
        let dir = match resolution {
            Resolution::Create(dir) => dir,
            Resolution::AlreadyPresent(_) => panic!("expected a fresh directory"),
        };

        assert!(dir.join("before.rs.test").exists());
        assert!(dir.join("after.rs.test").exists());
        let before = fs::read_to_string(dir.join("before.rs.test"))?;
        let after = fs::read_to_string(dir.join("after.rs.test"))?;
        assert_ne!(before, after);

        let source: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join("source.json"))?)?;
        assert_eq!(source["language"], "Rust");
        assert_eq!(source["repository"], row.repository);
        assert_eq!(source["commit"], row.commit);
        assert_eq!(source["path"], row.path);
        assert_eq!(source["dataset"], row.dataset);

        Ok(())
    }

    #[test]
    fn rerun_is_idempotent_and_distinct_content_gets_a_numbered_suffix() -> Result<()> {
        let repo_path = helper::handmade_git_repository()?;
        let (commit, path) = find_a_modified_file(&repo_path);
        let row = row_for(&repo_path, commit, path);
        let repo_roots = vec![repo_path.parent().unwrap().to_path_buf()];
        let output_dir = tempdir()?;

        let first = materialize_row(&row, &repo_roots, output_dir.path())?;
        let first_dir = match first {
            Resolution::Create(dir) => dir,
            Resolution::AlreadyPresent(_) => panic!("expected a fresh directory on first run"),
        };

        let second = materialize_row(&row, &repo_roots, output_dir.path())?;
        match second {
            Resolution::AlreadyPresent(dir) => assert_eq!(dir, first_dir),
            Resolution::Create(_) => panic!("expected the second run to detect existing content"),
        }

        // Different content in the slot forces a name collision.
        fs::write(first_dir.join("before.rs.test"), "not the real content")?;
        let third = materialize_row(&row, &repo_roots, output_dir.path())?;
        match third {
            Resolution::Create(dir) => {
                assert_eq!(
                    dir,
                    first_dir.with_file_name(format!(
                        "{}-2",
                        first_dir.file_name().unwrap().to_string_lossy()
                    ))
                );
            }
            Resolution::AlreadyPresent(_) => panic!("content was tampered with, should not match"),
        }

        Ok(())
    }

    #[test]
    fn rerun_keeps_a_hand_edited_readme_and_backfills_a_missing_one() -> Result<()> {
        let repo_path = helper::handmade_git_repository()?;
        let (commit, path) = find_a_modified_file(&repo_path);
        let row = row_for(&repo_path, commit, path);
        let repo_roots = vec![repo_path.parent().unwrap().to_path_buf()];
        let output_dir = tempdir()?;

        let dir = match materialize_row(&row, &repo_roots, output_dir.path())? {
            Resolution::Create(dir) => dir,
            Resolution::AlreadyPresent(_) => panic!("expected a fresh directory"),
        };
        let readme = dir.join("README.md");
        assert!(readme.exists());

        fs::write(&readme, "hand-corrected")?;
        materialize_row(&row, &repo_roots, output_dir.path())?;
        assert_eq!(fs::read_to_string(&readme)?, "hand-corrected");

        fs::remove_file(&readme)?;
        materialize_row(&row, &repo_roots, output_dir.path())?;
        assert!(readme.exists());

        Ok(())
    }
}
