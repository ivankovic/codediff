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
use git2::{Repository, Signature};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::vec::Vec;
use tempfile::tempdir;

use crate::code::{Code, metadata};

/**
* Returns handmade test code as Code objects.
*
* Useful for testing any function that takes Code as input.
*
* Note that the actual files are stored with ".test" extension in "src/test/data/code". This is so
* that the build system doesn't treat data as code. To make sure the files are correctly treated
* during testing, ".test" extension is removed in this function.
*
* Returns a HashMap where the key is the file name without the ".test" extension.
*/
pub fn handmade_test_code() -> Result<HashMap<String, Code>> {
    let mut codes = handmade_unparsed_test_code()?;

    let mut parser = tree_sitter::Parser::new();

    for (_, code) in codes.iter_mut() {
        if let Some(language) = &code.metadata.language {
            let ts_language = crate::code::language::to_treesitter(language)
                .expect("Handmade test code for unknown language?");

            parser.set_language(&ts_language)?;

            code.parse(&mut parser);
        }
    }

    Ok(codes)
}

/**
* Returns handmade test code as Code objects.
*
* This is a special version of handmade_test_code that doesn't parse the code. This is useful for
* testing functions that consume Data and similar files that don't get parsed.
*/
pub fn handmade_unparsed_test_code() -> Result<HashMap<String, Code>> {
    let mut result = HashMap::new();

    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("code");

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

            // Extract file name without .test extension for the key
            let new_path = path.with_extension("");
            let file_name = new_path.file_name().unwrap();
            result.insert(file_name.to_string_lossy().into_owned(), code);
        }
    }

    Ok(result)
}

/**
* Returns handmade test code, but as paths to a temporary file system.
*
* The files are returned as a hash map, where the key of the map is the name of the file in the
* "src/test/data/code" directory, with the ".test" extension removed. E.g.
* "src/test/data/code/hello_world.rs.test" will become the following key-value pair:
*
* ("hello_world.rs", PathBuf("<temporary directory>/hello_world.rs"))
*
* This is useful for testing code that expects paths. This function will correctly remove the
* ".test" extension when copying the code over to the temporary filesystem, so that all metadata
* recognition works correctly.
*/
pub fn handmade_test_code_as_paths() -> Result<HashMap<String, PathBuf>> {
    let mut result = HashMap::new();

    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("code");

    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let temp_path = temp_dir.path().to_path_buf();
    let _ = temp_dir.keep();

    println!(
        "Copying hand-made inputs from {:?} to {:?}",
        root.as_path(),
        temp_path
    );

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            let contents = fs::read_to_string(&path)?;

            // Create the destination path with .test extension removed
            let new_path = path.with_extension("");
            let file_name_os_str = new_path.file_name().unwrap();

            let dest_path = temp_path.join(file_name_os_str);
            fs::write(&dest_path, contents).expect("Failed to write file");

            result.insert(file_name_os_str.to_string_lossy().into_owned(), dest_path);
        }
    }

    Ok(result)
}

/**
* Returns handmade (before, after) pairs, as Code objects.
*
* Note that the actual files are stored with ".test" extension in "src/test/data/diffs/<dir>/". This is so
* that the build system doesn't treat data as code. To make sure the files are correctly treated
* during testing, ".test" extension is removed in this function.
*
* Returns a HashMap where the key is the directory name and the value is the (before, after) Code
* object pair.
*/
pub fn handmade_test_diffs() -> Result<HashMap<String, (Code, Code)>> {
    let mut result = HashMap::new();

    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("diffs");

    let mut parser = tree_sitter::Parser::new();

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let dir_name = path.file_name().unwrap().to_string_lossy().into_owned();

            let mut before_code = None;
            let mut after_code = None;

            // Read all files in the directory
            for file_entry in fs::read_dir(&path)? {
                let file_entry = file_entry?;
                let file_path = file_entry.path();

                if file_path.is_file() {
                    let file_name = file_path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned();

                    if file_name.starts_with("before.") && file_name.ends_with(".test") {
                        let contents = fs::read_to_string(&file_path)?;
                        let mut code = Code {
                            contents,
                            ..Default::default()
                        };
                        code.metadata.path = Some(file_path.with_extension(""));
                        metadata::hermetic_expand(&mut code.metadata);
                        before_code = Some(code);
                    } else if file_name.starts_with("after.") && file_name.ends_with(".test") {
                        let contents = fs::read_to_string(&file_path)?;
                        let mut code = Code {
                            contents,
                            ..Default::default()
                        };
                        code.metadata.path = Some(file_path.with_extension(""));
                        metadata::hermetic_expand(&mut code.metadata);
                        after_code = Some(code);
                    }
                }
            }

            // Parse both codes if they exist
            if let (Some(mut before), Some(mut after)) = (before_code, after_code) {
                if let Some(language) = &before.metadata.language {
                    let ts_language = crate::code::language::to_treesitter(language)
                        .expect("Handmade test code for unknown language?");

                    parser.set_language(&ts_language)?;
                    before.parse(&mut parser);
                }

                if let Some(language) = &after.metadata.language {
                    let ts_language = crate::code::language::to_treesitter(language)
                        .expect("Handmade test code for unknown language?");

                    parser.set_language(&ts_language)?;
                    after.parse(&mut parser);
                }

                result.insert(dir_name, (before, after));
            }
        }
    }

    Ok(result)
}

/**
* Returns a path to a fully functional git repository that is on a temporary path.
*
* The repository contains handmade commits to be used in tests.
*/
pub fn handmade_git_repository() -> Result<PathBuf> {
    let (repo_path, repo) = initialize_repository()?;
    let dirs = read_fake_git_repo_testdata()?;
    add_commits(&repo, &repo_path, dirs)?;
    Ok(repo_path)
}

fn initialize_repository() -> Result<(PathBuf, Repository)> {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let repo_path = temp_dir.path().to_path_buf();

    let repo = Repository::init(repo_path.clone()).expect("Failed to initialize git repository");
    let _ = temp_dir.keep();

    Ok((repo_path, repo))
}

fn read_fake_git_repo_testdata() -> Result<Vec<(u32, PathBuf)>> {
    let test_data_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("fake-git-repo");

    let mut dirs: Vec<_> = fs::read_dir(test_data_root)
        .expect("Failed to read test data directory")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_dir() {
                // Extract directory name and try to parse as number
                if let Some(dir_name) = path.file_name()
                    && let Ok(num) = dir_name.to_string_lossy().parse::<u32>()
                {
                    return Some((num, path));
                }
            }
            None
        })
        .collect();

    // Sort directories by their numeric names
    dirs.sort_by_key(|&(num, _)| num);
    Ok(dirs)
}

fn add_commits(repo: &Repository, repo_path: &Path, dirs: Vec<(u32, PathBuf)>) -> Result<()> {
    let signature =
        Signature::now("Test Author", "test@example.com").expect("Failed to create signature");

    for (commit_num, dir_path) in dirs {
        copy_test_files_to_repo(&dir_path, commit_num, repo_path)?;
        create_commit(repo, &signature, commit_num)?;
    }
    Ok(())
}

/// Copy test files from source directory to repository, transforming paths
fn copy_test_files_to_repo(dir_path: &Path, commit_num: u32, repo_path: &Path) -> Result<()> {
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

    for file_path in files {
        let content = fs::read_to_string(&file_path).expect("Failed to read file");
        let final_path = path_in_repo(&file_path, commit_num, repo_path);
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent).expect("Failed to create parent directories");
        }
        fs::write(&final_path, content).expect("Failed to write file");
    }

    Ok(())
}

fn path_in_repo(file_path: &Path, commit_num: u32, repo_path: &Path) -> PathBuf {
    let test_data_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("fake-git-repo")
        .join(commit_num.to_string());

    let relative_path = file_path
        .strip_prefix(test_data_root)
        .expect("Failed to strip prefix")
        .with_extension("");

    repo_path.join(relative_path)
}

/// Create a git commit for the current repository state
fn create_commit(repo: &Repository, signature: &Signature, commit_num: u32) -> Result<()> {
    let commit_message = format!("Commit {}", commit_num);

    let mut index = repo.index().expect("Failed to open index");
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .expect("Failed to add files to index");
    index.write().expect("Failed to write index");

    let tree_id = index.write_tree().expect("Failed to write tree");
    let tree = repo.find_tree(tree_id).expect("Failed to find tree");

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

    if let Some(parent) = parent_commit {
        repo.commit(
            Some("HEAD"),
            signature,
            signature,
            &commit_message,
            &tree,
            &[&parent],
        )
        .expect("Failed to create commit");
    } else {
        // First commit
        repo.commit(
            Some("HEAD"),
            signature,
            signature,
            &commit_message,
            &tree,
            &[],
        )
        .expect("Failed to create initial commit");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::code::Language;

    use super::*;

    #[test]
    fn test_path_to_repo_path() -> Result<()> {
        let test_data_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("test")
            .join("data")
            .join("fake-git-repo");

        let file_path = test_data_root
            .join("1")
            .join("should_not_be_removed")
            .join("file.rs.test");

        let repo_path = PathBuf::from("some/random/path");

        let in_repo_path = path_in_repo(&file_path, 1, &repo_path);

        assert_eq!(
            in_repo_path
                .to_str()
                .expect("Unable to convert path to string"),
            "some/random/path/should_not_be_removed/file.rs"
        );

        Ok(())
    }

    #[test]
    fn handmade_code_contains_hello_world() -> Result<()> {
        let codes = handmade_test_code()?;

        assert!(!codes.is_empty());

        assert!(codes.contains_key("hello-world.rs"));

        let code = codes.get("hello-world.rs").unwrap();

        assert_ne!(code.contents, "");

        assert!(code.metadata.language.is_some());
        if let Some(l) = &code.metadata.language {
            assert_eq!(*l, Language::Rust);
        }

        // Check that it parsed successfully.
        assert!(code.ast.is_some());

        Ok(())
    }

    #[test]
    fn test_handmade_test_code_as_paths() -> Result<()> {
        let paths = handmade_test_code_as_paths()?;

        assert!(!paths.is_empty(), "Should have found test code files");

        for (key, path) in &paths {
            assert!(path.exists(), "Path should exist: {:?}", path);
            assert!(path.is_file(), "Path should be a file: {:?}", path);

            assert!(
                !key.ends_with(".test"),
                "Key should not contain .test extension: {}",
                key
            );
        }

        Ok(())
    }

    #[test]
    fn test_handmade_test_diffs_returns_all_diffs() -> Result<()> {
        let diffs = handmade_test_diffs()?;

        println!("Found {} test diffs:", diffs.len());
        for key in diffs.keys() {
            println!("  - {}", key);
        }

        // We should have all the expected diffs
        assert!(diffs.contains_key("no-change"));
        assert!(diffs.contains_key("hello-world-added-message"));
        assert!(diffs.contains_key("leet-code-1-bugfix"));

        assert_eq!(diffs.len(), 3);

        Ok(())
    }

    #[test]
    fn test_handmade_test_diffs_no_change_diff() -> Result<()> {
        let diffs = handmade_test_diffs()?;

        assert!(!diffs.is_empty(), "Should have found some test diffs");

        assert!(diffs.contains_key("no-change"));

        let (before, after) = diffs.get("no-change").unwrap();

        assert_ne!(before.contents, "");
        assert_ne!(after.contents, "");
        assert_eq!(before.contents, after.contents);

        assert!(before.metadata.language.is_some());
        assert_eq!(before.metadata.language, after.metadata.language);

        Ok(())
    }
}
