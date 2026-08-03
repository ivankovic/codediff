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
use crossbeam_channel::Sender;
use std::io::Write;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::metadata;

/// Find all git repositories in top-level subdirectories of the given path, or the path itself
/// if it is already a repository. Sorted for reproducible traversal order across runs.
pub fn find_git_repositories(base_path: &Path) -> Result<Vec<PathBuf>> {
    let mut repo_paths = Vec::new();

    if base_path.join(".git").exists() {
        repo_paths.push(base_path.to_path_buf());
    } else if base_path.is_dir() {
        for entry in std::fs::read_dir(base_path)? {
            let path = entry?.path();
            if path.is_dir() && path.join(".git").exists() {
                repo_paths.push(path);
            }
        }
    }

    repo_paths.sort();
    Ok(repo_paths)
}

/// Runs `process` once per repository in `repo_paths`, printing "Scanning {name}... " before and
/// "done" (or the error) after each - the identical progress-reporting wrapper
/// `sample_code_pairs.rs` and `sample_test_diffs.rs` each hand-rolled around their own
/// differently-shaped per-repository sampling call. `process` gets the repository's path and its
/// derived directory-name (empty string if the path has none); a returned `Err` is reported to
/// stderr and does not stop the remaining repositories from being processed.
pub fn for_each_repository(
    repo_paths: &[PathBuf],
    mut process: impl FnMut(&Path, &str) -> Result<()>,
) {
    for repo_path in repo_paths {
        let repository_name = repo_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        print!("Scanning {repository_name}... ");
        let _ = std::io::stdout().flush();
        if let Err(e) = process(repo_path, &repository_name) {
            eprintln!("Failed to process {repo_path:?}: {e:?}");
        } else {
            println!("done");
        }
        let _ = std::io::stdout().flush();
    }
}

pub fn all_files_from_path(root: &Path, path_tx: Sender<PathBuf>) -> Result<()> {
    if root.is_file() {
        // Ignore error if no receivers (program shutting down)
        if !metadata::is_anomalous(root) {
            let _ = path_tx.send(PathBuf::from(root));
        }
    } else if root.is_dir() {
        for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
            if entry.file_type().is_file() {
                if metadata::is_anomalous(entry.path()) {
                    continue;
                }

                // This will block when the queue is full.
                if path_tx.send(entry.into_path()).is_err() {
                    // All workers are gone, stop producing.
                    break;
                }
            }
        }
    }
    drop(path_tx);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::fs;
    use tempfile::tempdir;

    fn collect_files(root: &Path) -> HashSet<PathBuf> {
        let (tx, rx) = crossbeam_channel::unbounded();
        all_files_from_path(root, tx).expect("walk root");
        rx.into_iter().collect()
    }

    #[test]
    fn walks_directory_and_skips_anomalous_paths() {
        let dir = tempdir().expect("create temp dir");

        let normal_file = dir.path().join("main.rs");
        fs::write(&normal_file, "fn main() {}").expect("write normal file");

        let nested_dir = dir.path().join("src").join("nested");
        fs::create_dir_all(&nested_dir).expect("create nested dir");
        let nested_file = nested_dir.join("lib.rs");
        fs::write(&nested_file, "// nested").expect("write nested file");

        let anomalous_dir = dir.path().join("third_party");
        fs::create_dir_all(&anomalous_dir).expect("create anomalous dir");
        let anomalous_file = anomalous_dir.join("vendored.rs");
        fs::write(&anomalous_file, "// vendored").expect("write anomalous file");

        let found = collect_files(dir.path());

        assert!(found.contains(&normal_file));
        assert!(found.contains(&nested_file));
        assert!(!found.contains(&anomalous_file));
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn for_each_repository_visits_every_path_with_its_derived_name() {
        let repo_paths = vec![PathBuf::from("/repos/alpha"), PathBuf::from("/repos/beta")];

        let mut visited = Vec::new();
        for_each_repository(&repo_paths, |path, name| {
            visited.push((path.to_path_buf(), name.to_string()));
            Ok(())
        });

        assert_eq!(
            visited,
            vec![
                (PathBuf::from("/repos/alpha"), "alpha".to_string()),
                (PathBuf::from("/repos/beta"), "beta".to_string()),
            ]
        );
    }

    #[test]
    fn for_each_repository_keeps_going_after_one_repository_errors() {
        let repo_paths = vec![
            PathBuf::from("/repos/fails"),
            PathBuf::from("/repos/succeeds"),
        ];

        let mut visited = Vec::new();
        for_each_repository(&repo_paths, |path, _name| {
            if path.ends_with("fails") {
                anyhow::bail!("simulated failure");
            }
            visited.push(path.to_path_buf());
            Ok(())
        });

        assert_eq!(
            visited,
            vec![PathBuf::from("/repos/succeeds")],
            "the second repository should still be processed after the first one errors"
        );
    }

    #[test]
    fn single_file_path_is_returned_directly() {
        let dir = tempdir().expect("create temp dir");
        let file = dir.path().join("only.rs");
        fs::write(&file, "fn main() {}").expect("write file");

        let found = collect_files(&file);

        assert_eq!(found, HashSet::from([file]));
    }

    #[test]
    fn single_anomalous_file_path_is_skipped() {
        let dir = tempdir().expect("create temp dir");
        let nested = dir.path().join("third_party");
        fs::create_dir_all(&nested).expect("create dir");
        let file = nested.join("only.rs");
        fs::write(&file, "fn main() {}").expect("write file");

        let found = collect_files(&file);

        assert!(found.is_empty());
    }
}
