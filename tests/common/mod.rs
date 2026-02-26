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
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */
use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use std::vec::Vec;

use codediff::code::{Code, metadata};
use git2::{Repository, Signature};
use tempfile::tempdir;

pub fn handmade_test_code() -> Result<Vec<Code>> {
    let mut result = Vec::new();

    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("code");

    println!("Reading hand-made inputs from {:?}", root.as_path());

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            let contents = fs::read_to_string(&path)?;

            let mut code = Code {
                contents,
                ..Default::default()
            };
            code.metadata.path = Some(path.with_extension(""));

            metadata::hermetic_expand(&mut code.metadata);

            result.push(code);
        }
    }

    Ok(result)
}

pub fn handmade_git_repository() -> Result<PathBuf> {
    // Create a temporary directory for our git repository
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let repo_path = temp_dir.path().to_path_buf();

    // Initialize a git repository
    let repo = Repository::init(repo_path.clone()).expect("Failed to initialize git repository");

    // Create a signature for commits
    let signature =
        Signature::now("Test Author", "test@example.com").expect("Failed to create signature");

    // Get the path to the test data
    let test_data_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("fake-git-repo");

    // Read directories in sequence (1, 2, 3, etc.)
    let mut dirs: Vec<_> = fs::read_dir(&test_data_root)
        .expect("Failed to read test data directory")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_dir() {
                // Extract directory name and try to parse as number
                if let Some(dir_name) = path.file_name() {
                    if let Ok(num) = dir_name.to_string_lossy().parse::<u32>() {
                        return Some((num, path));
                    }
                }
            }
            None
        })
        .collect();

    // Sort directories by their numeric names
    dirs.sort_by_key(|&(num, _)| num);

    // Process each directory as a separate commit
    for (commit_num, dir_path) in dirs {
        // Read all .test files in the directory
        let files: Vec<_> = fs::read_dir(dir_path)
            .expect("Failed to read directory")
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("test") {
                    Some(path)
                } else {
                    None
                }
            })
            .collect();

        // Copy files to the repository, removing .test extension
        for file_path in files {
            let content = fs::read_to_string(&file_path).expect("Failed to read file");

            // Create the target path without .test extension
            let relative_path = file_path
                .strip_prefix(&test_data_root)
                .expect("Failed to strip prefix");
            let target_path = repo_path.join(relative_path);

            // Remove the directory number from path and .test extension
            let mut final_path = target_path.clone();
            if let Some(file_name_os) = target_path.file_name() {
                if let Some(file_name) = file_name_os.to_str() {
                    if let Some(new_file_name) = file_name.strip_suffix(".test") {
                        final_path.set_file_name(new_file_name);
                    }
                }
            }

            // Remove the first directory component (the number)
            if let Some(parent) = final_path.parent() {
                if let Some(grandparent) = parent.parent() {
                    let file_name = final_path.file_name().unwrap();
                    final_path = grandparent.join(file_name);
                }
            }

            // Create parent directories if they don't exist
            if let Some(parent) = final_path.parent() {
                fs::create_dir_all(parent).expect("Failed to create parent directories");
            }

            // Write the file content
            fs::write(&final_path, content).expect("Failed to write file");
        }

        // Create a commit for this state
        let commit_message = format!("Commit {}", commit_num);

        // Stage all files
        let mut index = repo.index().expect("Failed to open index");
        index
            .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
            .expect("Failed to add files to index");
        index.write().expect("Failed to write index");

        // Get the tree from the index
        let tree_id = index.write_tree().expect("Failed to write tree");
        let tree = repo.find_tree(tree_id).expect("Failed to find tree");

        // Get the current HEAD commit if it exists
        let parent_commit = if commit_num > 1 {
            let obj = repo
                .head()
                .expect("Failed to get HEAD")
                .resolve()
                .expect("Failed to resolve HEAD");
            Some(obj.peel_to_commit().expect("Failed to peel to commit"))
        } else {
            None
        };

        // Create the commit
        if let Some(parent) = parent_commit {
            repo.commit(
                Some("HEAD"),
                &signature,
                &signature,
                &commit_message,
                &tree,
                &[&parent],
            )
            .expect("Failed to create commit");
        } else {
            // First commit
            repo.commit(
                Some("HEAD"),
                &signature,
                &signature,
                &commit_message,
                &tree,
                &[],
            )
            .expect("Failed to create initial commit");
        }
    }

    // Keep the tempdir so it persists until the end of the test
    let path = temp_dir.path().to_path_buf();
    let _ = temp_dir.keep();
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handmade_test_code_loads() -> Result<()> {
        let test_codes = handmade_test_code()?;

        assert!(!test_codes.is_empty());

        Ok(())
    }

    #[test]
    fn handmade_git_repository_loads() -> Result<()> {
        let test_git_repo_path = handmade_git_repository()?;

        assert!(test_git_repo_path.is_dir());

        let git_dir = test_git_repo_path.join(".git");
        assert!(git_dir.is_dir());

        let repo = Repository::open(&test_git_repo_path)?;
        let head = repo.head()?;
        let commit = head.peel_to_commit()?;

        let mut revwalk = repo.revwalk()?;
        revwalk.push(commit.id())?;
        let commit_count = revwalk.count();
        assert!(
            commit_count >= 2,
            "Expected at least 2 commits, found {}",
            commit_count
        );

        let main_rs_path = test_git_repo_path.join("main.rs");
        assert!(main_rs_path.is_file());
        let content = fs::read_to_string(main_rs_path)?;
        assert!(content.contains("Hello World"));

        Ok(())
    }
}
