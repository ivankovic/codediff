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
pub mod human_mapping;
pub mod optimal_iud;

use anyhow::{Result, bail};
use git2::{Repository, Signature};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::vec::Vec;
use tempfile::tempdir;
use tree_sitter::Node;

use crate::code::{Code, metadata};
use crate::diff::{ASTDiff, ASTMapping, ASTMappingOperation};

/**
* Follows path from the root node and returns the resulting node, if the path is valid.
*
* The path is a vector of strings. Each string is one of the following:
*
* 1) The type of the node, e.g. "expression_statement". If only the type is given, the first child
*    node matching the type is used for traversal.
* 2) The type of the node, followed by a number, e.g. "block:3". The n-th child matching the type
*    is used for traversal. In the case of "block:3", the third block node. Note the 1-indexing
*    used to make the string easier to read for humans.
*
* If the path is invalid, an error is returned.
*/
pub fn node_for_path<'a>(root: Node<'a>, path: &[&str]) -> Result<Node<'a>> {
    let mut current_node = root;

    for path_segment in path {
        // Parse the path segment to extract node type and optional index. Split on the *last*
        // colon rather than the first: node kinds can themselves contain colons (e.g. TypeScript's
        // ":" token, Rust's "::" token), but the index suffix we append is always pure digits, so
        // the rightmost colon is always the one we inserted.
        let (node_type, child_index) = match path_segment.rsplit_once(':') {
            Some((node_type, index_str)) => {
                let index = index_str.parse::<usize>().map_err(|_| {
                    anyhow::anyhow!(
                        "Invalid index in path segment: {} for path {:?}",
                        path_segment,
                        path
                    )
                })? - 1; // Convert to 0-indexed
                (node_type, index)
            }
            None => (*path_segment, 0), // No index given: use first matching child
        };

        // Find the matching child node
        let mut found_node = None;
        let mut current_count = 0;

        let mut cursor = current_node.walk();
        for child in current_node.children(&mut cursor) {
            if child.kind() == node_type {
                if current_count == child_index {
                    found_node = Some(child);
                    break;
                }
                current_count += 1;
            }
        }

        match found_node {
            Some(node) => current_node = node,
            None => bail!(
                "Path segment '{}' not found at current position for path {:?}",
                path_segment,
                path
            ),
        }
    }

    Ok(current_node)
}

/**
* The inverse of [`node_for_path`]: computes the path from the root of the tree down to `node`,
* using the same "type" / "type:index" mini-language.
*
* This always emits the fully-qualified "type:index" form (never the bare-type shorthand), which
* `node_for_path` also accepts, so the two functions round-trip: for any node in a tree,
* `node_for_path(root, &path_for_node(node))` returns that same node.
*
* Paths are stable across re-parses of the same source text (unlike TreeSitter node IDs, which are
* arena slot indices and can differ between parses), which is why this is the basis for comparing
* human-authored ground-truth mappings against freshly computed diffs.
*/
pub fn path_for_node(node: Node) -> Vec<String> {
    let mut path = Vec::new();
    let mut current = node;

    while let Some(parent) = current.parent() {
        let kind = current.kind();

        // Count how many earlier siblings share this node's kind, to reproduce the same
        // 1-indexed "occurrence of this kind" numbering that node_for_path consumes.
        let mut occurrence = 0usize;
        let mut cursor = parent.walk();
        for sibling in parent.children(&mut cursor) {
            if sibling.id() == current.id() {
                break;
            }
            if sibling.kind() == kind {
                occurrence += 1;
            }
        }

        path.push(format!("{}:{}", kind, occurrence + 1));
        current = parent;
    }

    path.reverse();
    path
}

pub fn mapping_for_path<'a>(
    path_before: &[&str],
    path_after: &[&str],
    before_root: Node<'a>,
    after_root: Node<'a>,
    diff: &ASTDiff,
) -> Result<ASTMapping> {
    let node_before = node_for_path(before_root, path_before)?;
    let node_after = node_for_path(after_root, path_after)?;
    let mapping = diff.mapping.get(&(node_before.id(), node_after.id()));

    if mapping.is_none() {
        bail!(
            "Mapping not found for paths {:?} and {:?}",
            path_before,
            path_after
        );
    }

    let mapping = mapping.unwrap();

    Ok(mapping.clone())
}

/**
* Returns true if all nodes along the given path in both trees have the same mapping operation.
*
* This function is useful for checking that a sequence of nodes that all have the same mapping
* and path prefixes. For each node along the path (including intermediate nodes), it checks
* that the corresponding nodes in the before and after trees have a mapping with the expected
* operation.
*
* @param path The path to check in both trees (same path for before and after)
* @param before_root The root node of the before tree
* @param after_root The root node of the after tree
* @param diff The ASTDiff containing the mappings
* @param expected_operation The expected ASTMappingOperation that all nodes should have
* @return true if all nodes along the path have the expected mapping operation
*/
pub fn entire_path_has_mapping<'a>(
    path: &[&str],
    before_root: Node<'a>,
    after_root: Node<'a>,
    diff: &ASTDiff,
    expected_operation: ASTMappingOperation,
) -> Result<bool> {
    // Build up the path incrementally, checking each intermediate node
    let mut current_before_path = Vec::new();
    let mut current_after_path = Vec::new();

    for &path_segment in path {
        current_before_path.push(path_segment);
        current_after_path.push(path_segment);

        // Get the nodes at this level
        match node_for_path(before_root, &current_before_path) {
            Ok(node_before) => {
                // Try to get the after node
                match node_for_path(after_root, &current_after_path) {
                    Ok(node_after) => {
                        // Get the mapping for these nodes
                        let mapping = diff.mapping.get(&(node_before.id(), node_after.id()));

                        if let Some(mapping) = mapping {
                            // Check if this mapping has the expected operation
                            if mapping.operation != expected_operation {
                                return Ok(false);
                            }
                        } else {
                            // If there's no mapping, the path doesn't have the expected operation
                            return Ok(false);
                        }
                    }
                    Err(_) => {
                        // If we can't find the path in the after tree, return false
                        return Ok(false);
                    }
                }
            }
            Err(_) => {
                // If we can't find the path in the before tree, return false
                return Ok(false);
            }
        }
    }

    Ok(true)
}

pub fn was_node_added<'a>(path: &[&str], root: Node<'a>, diff: &ASTDiff) -> Result<bool> {
    let node = node_for_path(root, path)?;
    Ok(diff.mapping.contains_key(&(0, node.id())))
}

pub fn was_node_deleted<'a>(path: &[&str], root: Node<'a>, diff: &ASTDiff) -> Result<bool> {
    let node = node_for_path(root, path)?;
    Ok(diff.mapping.contains_key(&(node.id(), 0)))
}

pub fn was_tree_added<'a>(path: &[&str], root: Node<'a>, diff: &ASTDiff) -> Result<bool> {
    let node = node_for_path(root, path)?;
    let mut stack = vec![node];

    while let Some(node) = stack.pop() {
        if !diff.mapping.contains_key(&(0, node.id())) {
            return Ok(false);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }

    Ok(true)
}

pub fn was_tree_deleted<'a>(path: &[&str], root: Node<'a>, diff: &ASTDiff) -> Result<bool> {
    let node = node_for_path(root, path)?;
    let mut stack = vec![node];

    while let Some(node) = stack.pop() {
        if !diff.mapping.contains_key(&(node.id(), 0)) {
            return Ok(false);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }

    Ok(true)
}

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
pub fn handmade_test_code_pairs() -> Result<HashMap<String, (Code, Code)>> {
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
* Returns handmade Diff objects from code pairs, along with the before and after Code objects.
*
* This function creates actual Diff objects from the handmade test code pairs.
* It accepts boolean parameters to control which parts of the diff are computed.
*
* Note that the actual files are stored with ".test" extension in "src/test/data/diffs/<dir>/".
*
* @param compute_ast If true, compute the AST-based diff
* @param compute_ts_points If true, compute the TreeSitter points diff
* @return A HashMap where the key is the directory name and the value is a tuple of
*         (before Code, after Code, Diff object)
*/
pub fn handmade_test_diffs(
    compute_ast: bool,
    compute_ts_points: bool,
    filename_filter: &str,
) -> Result<HashMap<String, (Code, Code, crate::diff::Diff)>> {
    let code_pairs = handmade_test_code_pairs()?;
    let mut result = HashMap::new();

    for (name, (before, after)) in code_pairs {
        if !filename_filter.is_empty() && !name.contains(filename_filter) {
            continue;
        }
        let mut diff = crate::diff::Diff {
            language: before
                .metadata
                .language
                .clone()
                .unwrap_or(crate::code::Language::Unknown),
            ..Default::default()
        };

        if compute_ast {
            // Use the new from_code method to compute AST diff
            let ast_diff = crate::diff::Diff::from_code(&before, &after);
            diff.ast = ast_diff.ast;
        }

        if compute_ts_points {
            // Compute ts_points if requested
            // For now, we'll leave this as a placeholder since the implementation
            // would depend on the existing ts_points module
            // diff.ts_points = Some(crate::diff::ts_points::compute(&antes, &after)?);
        }

        result.insert(name, (before, after, diff));
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
    fn test_node_for_path() -> Result<()> {
        let test_codes = handmade_test_code()?;
        let code = test_codes.get("hello-world.rs").unwrap().clone();

        // The hello-world.rs TreeSitter AST has 22 nodes.
        // It looks like this:
        //
        // source_file
        //   function_item
        //     fn
        //     identifier
        //     parameters
        //       (
        //       )
        //     block
        //       {
        //       expression_statement
        //         macro_invocation
        //           identifier
        //           !
        //           token_tree
        //             (
        //             string_literal
        //               "
        //               string_content
        //               "
        //             )
        //         ;
        //       }
        //
        //  The code is identical, so the minimal diff script is just empty.

        let ast = code.ast.unwrap();

        // Correct paths

        let t = node_for_path(
            ast.root_node(),
            &["function_item", "block", "expression_statement"],
        )?;
        assert_eq!(t.kind(), "expression_statement");

        let t = node_for_path(
            ast.root_node(),
            &[
                "function_item:1",
                "block:1",
                "expression_statement",
                "macro_invocation:1",
            ],
        )?;
        assert_eq!(t.kind(), "macro_invocation");

        // Invalid paths
        assert!(node_for_path(ast.root_node(), &["no such node"]).is_err());

        Ok(())
    }

    #[test]
    fn test_path_for_node_round_trips_through_node_for_path() -> Result<()> {
        // path_for_node must be the exact inverse of node_for_path for every node in a tree,
        // since human-authored mappings are compared against fresh diffs purely by path.
        let test_diffs = handmade_test_code_pairs()?;

        for (name, (before, after)) in &test_diffs {
            for (label, code) in [("before", before), ("after", after)] {
                let ast = code
                    .ast
                    .as_ref()
                    .unwrap_or_else(|| panic!("{} {} has no AST", name, label));
                let root = ast.root_node();

                let mut stack = vec![root];
                while let Some(node) = stack.pop() {
                    let path = path_for_node(node);
                    let path_refs: Vec<&str> = path.iter().map(String::as_str).collect();
                    let found = node_for_path(root, &path_refs).unwrap_or_else(|e| {
                        panic!(
                            "{} {}: path {:?} for node {} ({}) did not resolve: {}",
                            name,
                            label,
                            path_refs,
                            node.kind(),
                            node.id(),
                            e
                        )
                    });
                    assert_eq!(
                        found.id(),
                        node.id(),
                        "{} {}: path {:?} resolved to a different node than it was derived from",
                        name,
                        label,
                        path_refs
                    );

                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        stack.push(child);
                    }
                }
            }
        }

        Ok(())
    }

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
    fn test_handmade_test_code_pairs_returns_all_diffs() -> Result<()> {
        let diffs = handmade_test_code_pairs()?;

        println!("Found {} test code pairs:", diffs.len());
        for key in diffs.keys() {
            println!("  - {}", key);
        }

        // We should have all the expected diffs
        assert!(diffs.contains_key("rust-no-change"));
        assert!(diffs.contains_key("rust-hello-world-added-message"));
        assert!(diffs.contains_key("rust-leetcode-1-bugfix"));

        assert!(diffs.len() > 3);

        Ok(())
    }

    #[test]
    fn test_handmade_test_code_pairs_no_change_diff() -> Result<()> {
        let diffs = handmade_test_code_pairs()?;

        assert!(!diffs.is_empty(), "Should have found some test code pairs");

        assert!(diffs.contains_key("rust-no-change"));

        let (before, after) = diffs.get("rust-no-change").unwrap();

        assert_ne!(before.contents, "");
        assert_ne!(after.contents, "");
        assert_eq!(before.contents, after.contents);

        assert!(before.metadata.language.is_some());
        assert_eq!(before.metadata.language, after.metadata.language);

        Ok(())
    }

    #[test]
    fn test_handmade_test_diffs_creates_diff_objects() -> Result<()> {
        let diffs = handmade_test_diffs(true, false, "")?;

        assert!(!diffs.is_empty(), "Should have found some test diffs");

        assert!(diffs.contains_key("rust-no-change"));

        let (before, after, diff) = diffs.get("rust-no-change").unwrap();

        // The diff should have the AST computed since we passed true for compute_ast
        assert!(diff.ast.is_some());

        // The language should be set
        assert_ne!(diff.language, crate::code::Language::Unknown);

        // Check that code objects are returned
        assert_ne!(before.contents, "");
        assert_ne!(after.contents, "");

        Ok(())
    }

    #[test]
    fn test_entire_path_has_mapping() -> Result<()> {
        // Use rust-no-change since all nodes should have Identical mapping
        let test_diffs = handmade_test_code_pairs()?;
        let (before, after) = test_diffs
            .get("rust-no-change")
            .unwrap()
            .clone();

        let diff = crate::diff::diff_code(&before, &after);
        let diff_ast = diff.ast.unwrap();
        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let before_root = before_ast.root_node();
        let after_root = after_ast.root_node();

        // rust-no-change has impl_item as a child of source_file (with comments before it)
        // Test a path where all nodes should have Identical mapping (no changes)
        let path = vec!["impl_item"];
        assert!(entire_path_has_mapping(
            &path,
            before_root,
            after_root,
            &diff_ast,
            ASTMappingOperation::Identical
        )?);

        let path = vec!["impl_item", "declaration_list"];
        assert!(entire_path_has_mapping(
            &path,
            before_root,
            after_root,
            &diff_ast,
            ASTMappingOperation::Identical
        )?);

        let path = vec!["impl_item", "declaration_list", "function_item"];
        assert!(entire_path_has_mapping(
            &path,
            before_root,
            after_root,
            &diff_ast,
            ASTMappingOperation::Identical
        )?);

        // Test with wrong expected operation - should return false
        let path = vec!["impl_item"];
        assert!(!entire_path_has_mapping(
            &path,
            before_root,
            after_root,
            &diff_ast,
            ASTMappingOperation::MatchButNotIdentical
        )?);

        // Test with a non-existent path - should return false (no mapping)
        let path = vec!["impl_item", "nonexistent"];
        assert!(!entire_path_has_mapping(
            &path,
            before_root,
            after_root,
            &diff_ast,
            ASTMappingOperation::Identical
        )?);

        // Test with rust-hello-world-added-message where we know function_item has MatchButNotIdentical
        let test_diffs2 = handmade_test_code_pairs()?;
        let (before2, after2) = test_diffs2
            .get("rust-hello-world-added-message")
            .unwrap()
            .clone();

        let diff2 = crate::diff::diff_code(&before2, &after2);
        let diff_ast2 = diff2.ast.unwrap();
        let before_ast2 = before2.ast.unwrap();
        let after_ast2 = after2.ast.unwrap();
        let before_root2 = before_ast2.root_node();
        let after_root2 = after_ast2.root_node();

        // Just check the function_item node - it has MatchButNotIdentical
        let path = vec!["function_item"];
        assert!(entire_path_has_mapping(
            &path,
            before_root2,
            after_root2,
            &diff_ast2,
            ASTMappingOperation::MatchButNotIdentical
        )?);

        Ok(())
    }
}
