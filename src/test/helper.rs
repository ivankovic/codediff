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

use anyhow::{Context, Result, bail};
#[cfg(feature = "stats")]
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
* Depth-first, includes-self search for the first node of a given `kind` at or below `node`
* (self first, then children in order, recursively). Was independently copy-pasted as
* `find_first`/`first_child_of_kind` in six different `solve_*.rs` test modules - consolidated
* here since one of those six copies (`solve_bottom_up_expansion`'s old `first_child_of_kind`)
* used *strict*-descendant semantics instead (skipping `node` itself), silently disagreeing with
* the other five whenever called on a node that already was the target kind.
*/
pub fn find_first_of_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    if node.kind() == kind {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_first_of_kind(child, kind) {
            return Some(found);
        }
    }
    None
}

/// Parses one path segment into (node kind, 0-indexed same-kind occurrence) - shared by
/// `node_for_path` and `PathCache::resolve` so the "type" / "type:index" mini-language is defined
/// in exactly one place. Split on the *last* colon rather than the first: node kinds can
/// themselves contain colons (e.g. TypeScript's ":" token, Rust's "::" token), but the index
/// suffix we append is always pure digits, so the rightmost colon is always the one we inserted.
fn parse_path_segment<'a>(path_segment: &'a str, path: &[&str]) -> Result<(&'a str, usize)> {
    match path_segment.rsplit_once(':') {
        Some((node_type, index_str)) => {
            let index = index_str.parse::<usize>().map_err(|_| {
                anyhow::anyhow!(
                    "Invalid index in path segment: {} for path {:?}",
                    path_segment,
                    path
                )
            })? - 1; // Convert to 0-indexed
            Ok((node_type, index))
        }
        None => Ok((path_segment, 0)), // No index given: use first matching child
    }
}

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
*
* Each segment does a fresh linear scan of the current node's children - fine for the occasional
* one-off lookup this is designed for (e.g. `human_solver`'s interactive jump-to-node), but not for
* resolving many paths against the same root, where the same high-fanout parent can get rescanned
* over and over - see [`PathCache`] for that case.
*/
pub fn node_for_path<'a>(root: Node<'a>, path: &[&str]) -> Result<Node<'a>> {
    let mut current_node = root;

    for path_segment in path {
        let (node_type, child_index) = parse_path_segment(path_segment, path)?;

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

/// One parent's children, indexed once for both directions [`PathCache`] needs: `by_kind`
/// answers `node_for_path`'s "the Nth child of kind K" (forward, path -> node), `occurrence_of`
/// answers `path_for_node`'s "which same-kind occurrence is this child" (reverse, node -> path).
/// Building both from the same single scan means a parent visited via both directions (as
/// `human_mapping`'s determinism check does - see `diff_paths_with_config`) still only gets
/// scanned once total, not once per direction.
struct ParentIndex<'a> {
    by_kind: HashMap<(String, usize), Node<'a>>,
    occurrence_of: HashMap<usize, usize>,
}

impl<'a> ParentIndex<'a> {
    fn build(parent: Node<'a>) -> Self {
        let mut by_kind = HashMap::new();
        let mut occurrence_of = HashMap::new();
        let mut counts: HashMap<String, usize> = HashMap::new();
        let mut cursor = parent.walk();
        for child in parent.children(&mut cursor) {
            let kind = child.kind().to_string();
            let occurrence = counts.entry(kind.clone()).or_insert(0);
            occurrence_of.insert(child.id(), *occurrence);
            by_kind.insert((kind, *occurrence), child);
            *occurrence += 1;
        }
        Self {
            by_kind,
            occurrence_of,
        }
    }
}

/**
* Memoized alternative to [`node_for_path`]/[`path_for_node`], for a caller that resolves *many*
* paths against the *same* root - e.g. `human_mapping::check_entry`'s per-mapping-entry lookups, or
* `human_mapping::diff_paths_with_config`'s per-mapped-node path computation. Both plain functions
* rescan a parent's children from scratch on every call; for a diff with thousands of entries that
* all resolve through one shared high-fanout parent (a large flat JSON object is the motivating
* case - one added key produces one mapping entry, but every *other* entry's path still passes
* through that same object node), that rescan happening once per entry makes the whole loop
* effectively quadratic in the entry count.
*
* `PathCache` fixes this by indexing each parent's children ([`ParentIndex`]) exactly once, the
* first time that parent is visited in either direction, and reusing the index for every
* subsequent lookup through it - the same object node then costs one scan total, not one per entry
* that passes through it.
*
* Not a drop-in replacement for `node_for_path`/`path_for_node`: the memory and setup cost of the
* index only pays for itself when the same root is queried many times, so this is a separate,
* opt-in type rather than either plain function growing an implicit cache.
*/
#[derive(Default)]
pub struct PathCache<'a> {
    by_parent: HashMap<usize, ParentIndex<'a>>,
}

impl<'a> PathCache<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    fn index_of(&mut self, parent: Node<'a>) -> &ParentIndex<'a> {
        self.by_parent
            .entry(parent.id())
            .or_insert_with(|| ParentIndex::build(parent))
    }

    /// Same contract as [`node_for_path`], but resolves each segment via this cache instead of a
    /// fresh scan.
    pub fn resolve(&mut self, root: Node<'a>, path: &[&str]) -> Result<Node<'a>> {
        let mut current_node = root;

        for path_segment in path {
            let (node_type, child_index) = parse_path_segment(path_segment, path)?;

            match self
                .index_of(current_node)
                .by_kind
                .get(&(node_type.to_string(), child_index))
            {
                Some(&node) => current_node = node,
                None => bail!(
                    "Path segment '{}' not found at current position for path {:?}",
                    path_segment,
                    path
                ),
            }
        }

        Ok(current_node)
    }

    /// Same contract as [`path_for_node`], but resolves each ancestor's same-kind occurrence via
    /// this cache instead of a fresh scan of its siblings.
    pub fn path_of(&mut self, node: Node<'a>) -> Vec<String> {
        let mut path = Vec::new();
        let mut current = node;

        while let Some(parent) = current.parent() {
            let kind = current.kind();
            // `occurrence_of` is populated for every child of `parent` by `ParentIndex::build`,
            // including `current` itself, so this can never miss.
            let occurrence = self.index_of(parent).occurrence_of[&current.id()];
            path.push(format!("{}:{}", kind, occurrence + 1));
            current = parent;
        }

        path.reverse();
        path
    }
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

/**
* Every node's path, in the same `"{kind}:{occurrence}"`-per-level format [`path_for_node`]
* produces, computed in a single top-down O(n) pass over `root` instead of `path_for_node`'s
* per-node O(sibling count) backward walk. That per-node cost is invisible for a single lookup,
* but a node with many same-kind siblings (a big JSON array's elements, a large enum's variants)
* makes it O(width) *per node at that level*, which a caller that looks up every node's path in a
* tight loop (`human_solver`'s `action_match_to_end`, or a site generator embedding every node's
* path) would pay again and again - this instead assigns each child its 1-indexed occurrence while
* visiting its parent's children exactly once.
*/
pub fn precompute_paths(root: Node) -> HashMap<usize, Vec<String>> {
    let mut paths = HashMap::new();
    paths.insert(root.id(), Vec::new());

    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        let node_path = paths.get(&node.id()).cloned().unwrap_or_default();
        let mut occurrence: HashMap<&str, usize> = HashMap::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let count = occurrence.entry(child.kind()).or_insert(0);
            *count += 1;
            let mut child_path = node_path.clone();
            child_path.push(format!("{}:{}", child.kind(), count));
            paths.insert(child.id(), child_path);
            stack.push(child);
        }
    }

    paths
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
* Returns true if every node along `path` (including intermediate nodes, not just the final one),
* resolved in both `before_root` and `after_root`, has a mapping in `diff` with `expected_operation`.
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

    // `ensure_parsed`, not the bare `parse`+manual-hasher-setup this used to do: `parse` alone
    // only sets `code.ast`, leaving `code.metadata.ast_metadata` at `None` - every downstream
    // `metadata_of` call on a `Code` from this function then has nothing to borrow and silently
    // recomputes the whole thing from scratch, every single time it's called anywhere in the
    // pipeline (confirmed 2026-07-26: 20 separate `compute_ast_metadata` calls for one `diff_code`
    // call on a single fixture pair, ~10 per side, one per pipeline phase that touches metadata -
    // see TODO.md). `ensure_parsed` parses *and* caches metadata in one idempotent call, matching
    // what every real caller (`Code::from_string`/`from_file`) already does. Safe now that
    // `Code`'s hand-written `Clone` drops `ast_metadata` back to `None` on every clone (see its
    // doc comment) - a caller of this function that clones a returned `Code` before diffing gets a
    // correct, if uncached, copy rather than one with stale root-id-keyed metadata.
    for code in codes.values_mut() {
        if code.metadata.language.is_some() {
            code.ensure_parsed()?;
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
* Returns every (before, after) pair in the corpus, as Code objects, swept from all of
* `src/test/data/diffs/{handmade,small,full}/<dir>/` despite the name - see [`DIFF_DATASETS`]'s
* doc comment for why the split doesn't matter to this function's callers.
*
* Note that the actual files are stored with ".test" extension in each `<dir>/`. This is so
* that the build system doesn't treat data as code. To make sure the files are correctly treated
* during testing, ".test" extension is removed in this function.
*
* Returns a HashMap where the key is the directory name and the value is the (before, after) Code
* object pair.
*/
pub fn handmade_test_code_pairs() -> Result<HashMap<String, (Code, Code)>> {
    // Every fixture directory is re-read and re-parsed with tree-sitter on every call. Fine for
    // the handful of direct callers that run once, but `compute_mismatches` (human_mapping.rs)
    // calls this once *per fixture* it checks - across a whole suite run that turns one full
    // O(fixture count) parse pass into an O(fixture count squared) one. The fixtures are
    // immutable for the life of the process (nothing in this codebase mutates the on-disk test
    // data at runtime), so memoize the whole map after the first successful build and hand out
    // clones of the cached `Code` pairs from then on.
    static CACHE: std::sync::OnceLock<HashMap<String, (Code, Code)>> = std::sync::OnceLock::new();
    if let Some(cached) = CACHE.get() {
        return Ok(cached.clone());
    }
    let result = handmade_test_code_pairs_uncached()?;
    Ok(CACHE.get_or_init(|| result).clone())
}

fn handmade_test_code_pairs_uncached() -> Result<HashMap<String, (Code, Code)>> {
    let mut result = HashMap::new();

    for dataset in DIFF_DATASETS {
        let dataset_root = diffs_root().join(dataset);
        if !dataset_root.exists() {
            continue;
        }
        for entry in fs::read_dir(&dataset_root)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                let dir_name = path.file_name().unwrap().to_string_lossy().into_owned();

                if let Some(pair) = code_pair_from_dir(&path)? {
                    result.insert(dir_name, pair);
                }
            }
        }
    }

    Ok(result)
}

/// The corpus under `src/test/data/diffs/` is split by provenance into three sibling folders:
/// `handmade` (hand-authored fixtures, never sampled), `small` (promoted from the small research
/// dataset's `sample.csv`), and `full` (reserved for a future sample from the full dataset, not
/// available on every machine - empty until then). Fixture names are unique across all three (a
/// promoted name can't collide with a handmade one - see `human_solver`'s `action_promote`), so
/// every reader below treats the split as an implementation detail: a name resolves to whichever
/// of the three actually holds it, and callers never need to know which.
pub const DIFF_DATASETS: &[&str] = &["handmade", "small", "full"];

fn diffs_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("diffs")
}

/// The directory for fixture `name`, searched across `DIFF_DATASETS` in order - the one place
/// that resolves the on-disk dataset split into a flat name lookup. `None` if `name` isn't a
/// directory under any of the three.
pub fn diffs_case_dir(name: &str) -> Option<std::path::PathBuf> {
    DIFF_DATASETS
        .iter()
        .map(|dataset| diffs_root().join(dataset).join(name))
        .find(|path| path.is_dir())
}

/**
* Loads exactly one named fixture from `src/test/data/diffs/{handmade,small,full}/<name>/`,
* without touching the rest of the corpus. Unlike [`handmade_test_code_pairs`], which parses and
* caches all ~220+ fixtures on first call, this parses only `name` - the first call for a given
* `name` pays one fixture's parse cost, not the whole corpus's. Subsequent calls for the same
* `name` (from other tests) hit a per-name cache and are free.
*
* Most tests only ever look up one or two specific fixtures by name (see
* [`compute_mismatches_with_config`] and most callers in `apted/common/tests.rs`), so this is the
* right default for new test code. A test that wants coverage across every language without a
* specific fixture in mind should use [`UNIT_TEST_FIXTURES`] via [`handmade_test_code_pairs_for`]
* instead - reach for [`handmade_test_code_pairs`] (the full corpus) only when a test genuinely
* can't be satisfied by that sample (e.g. `test_handmade_test_code_pairs_returns_all_diffs`, or a
* `#[ignore = "slow"]` full-corpus check).
*/
pub fn handmade_test_code_pair(name: &str) -> Result<(Code, Code)> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<HashMap<String, (Code, Code)>>> =
        std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()));

    if let Some(pair) = cache.lock().unwrap().get(name) {
        return Ok(pair.clone());
    }

    let dir = diffs_case_dir(name)
        .with_context(|| format!("No test case directory found for '{}'", name))?;
    let pair = code_pair_from_dir(&dir)?
        .with_context(|| format!("No before/after test code pair found for '{}'", name))?;
    cache.lock().unwrap().insert(name.to_string(), pair.clone());
    Ok(pair)
}

/**
* Same as [`handmade_test_code_pair`], but for a batch of names at once - for tests that sample a
* handful of fixtures across languages (see `UNIT_TEST_FIXTURES`) rather than needing just one.
* Each name still goes through the same per-name cache, so this is just a convenience wrapper, not
* a separate loading path.
*/
pub fn handmade_test_code_pairs_for(names: &[&str]) -> Result<HashMap<String, (Code, Code)>> {
    names
        .iter()
        .map(|&name| Ok((name.to_string(), handmade_test_code_pair(name)?)))
        .collect()
}

/// 1-3 fixtures per language (smallest/fastest available, preferring real-world diffs over
/// synthetic ones, each well under the corpus's largest fixtures) - the "unit test set" for tests
/// that need coverage across every supported language but not the full ~220-fixture corpus. Chosen
/// for small file size / node count, which is a reliable proxy for parse cost - but *not* for the
/// cost of running the real diff algorithm (`for_roots`/APTED) on a fixture, which is driven by
/// tree shape, not size (see the 2026-08-07 TODO.md entry: two of these fixtures are 100+ seconds
/// through `for_roots` despite being small by node count). A caller that only needs to parse this
/// set (a path<->node round trip, a size check) gets a fast, representative sample; a caller that
/// runs the actual diff algorithm over it (e.g. `is_always_valid`) should stay `#[ignore = "slow"]`.
/// See [`test_path_for_node_round_trips_through_node_for_path`] for the original motivating case (a
/// handful of multi-hundred-KB fixtures were dominating that test's runtime while adding no
/// coverage the smaller fixtures in the same language don't already provide).
pub const UNIT_TEST_FIXTURES: &[&str] = &[
    // c
    "c-freeciv-add-parameter-to-function",
    "c-htop-remove-function-declaration",
    "c-ffmpeg-added-typedef-to-enum",
    // cpp
    "cpp-add-templates",
    "cpp-fix-segfault",
    "cpp-tensorflow-switch-to-primitive-types",
    // csharp
    "csharp-sonarr-change-type",
    "csharp-lidarr-new-feature",
    "csharp-jellyfin-sql-query-fix",
    // css
    "css-add-property",
    "css-wordpress-reformat",
    "css-playwright-add-class-selector",
    // go
    "go-lazygit-switch-to-strings",
    "go-gin-add-function",
    "go-prometheus-single-comment-change",
    // html
    "html-fatedier-add-attribute",
    "html-hugo-tag-to-selfclosing-tag",
    "html-ladybird-delete-attribute",
    // java
    "java-fix-array-index",
    "java-genymobile-scrcpy-change-some-android-version-constant",
    "java-scrcpy-remove-or-expression",
    // javascript
    "javascript-add-destructuring",
    "javascript-fix-promises",
    "javascript-twbs-bootstrap-comment-version-update",
    // json (no synthetic fixture exists for this language)
    "json-shadcn-ui-ui-string-value-update-string-is-code",
    "json-nextcloud-server-deleted-pair",
    "json-shadcn-ui-ui-react-code-in-string-constant",
    // kotlin
    "kotlin-add-null-check",
    "kotlin-nextcloud-whitespace-only-change",
    "kotlin-remove-function",
    // lua (no synthetic fixture exists for this language)
    "lua-awesomewm-awesome-align-to-halign",
    "lua-neovim-one-added-line",
    "lua-awesomewm-awesome-comment-changes-and-additions",
    // php (no synthetic fixture exists for this language)
    "php-nextcloud-server-whitespace-and-added-declaration",
    "php-wordpress-wordpress-version-update",
    "php-nextcloud-change-doccomment",
    // python
    "python-added-if-block-small",
    "python-openhands-openhands-change-string-constant",
    "python-thefuck-multiline-string-change",
    // ruby: only one fixture is small - the other two are ~250KB+ and defeat the point
    "ruby-homebrew-add-or-expression",
    // rust: the literal "hello world" fixture, plus 2 more
    "rust-hello-world-added-message",
    "rust-add-if",
    "rust-sniffnet-protocol",
    // shellscript
    "shellscript-ansible-ansible-simple-deletion",
    "shellscript-langchain-ai-langchain-some-interesting-raw-string-to-string-content",
    "shellscript-genymobile-scrcpy-add-two-flags",
    // swift: all 3 existing fixtures are small and real
    "swift-swiftlang-swift-comment-change-2",
    "swift-swiftlang-swift-comment-change",
    "swift-nextcloud-ios-call-different-function",
    // tsx (no synthetic fixture exists for this language)
    "tsx-shadcn-ui-ui-add-attribute",
    "tsx-excalidraw-excalidraw-import-path-change",
    "tsx-material-remove-import",
    // typescript
    "typescript-microsoft-typescript-comment-change",
    "typescript-microsoft-typescript-add-target-comment",
    "typescript-microsoft-typescript-add-dot-js-to-import-paths",
    // vimscript: only 2 small fixtures exist, the rest are 65KB+
    "vimscript-neovim-neovim-add-a-few-lines",
    "vimscript-neovim-neovim-add-a-few-lines-one-after-the-other",
    // xml: only 2 small fixtures exist, the rest are 200KB+
    "xml-mozilla-firefox-firefox-add-a-few-attributes",
    "xml-odoo-odoo-change-value",
    // yaml
    "yaml-junegunn-fzf-version-upgrade",
    "yaml-axios-axios-update-string-value",
    "yaml-twbs-bootstrap-version-pin-with-comment",
];

/// Reads one `before.<ext>.test`/`after.<ext>.test` file into an unparsed `Code`, with its
/// `metadata.path` set - shared by both sides of [`code_pair_from_dir`].
fn load_side(file_path: &Path) -> Result<Code> {
    let contents = fs::read_to_string(file_path)?;
    let mut code = Code {
        contents,
        ..Default::default()
    };
    code.metadata.path = Some(file_path.with_extension(""));
    metadata::hermetic_expand(&mut code.metadata);
    Ok(code)
}

/**
* Reads and parses the `before.<ext>.test` / `after.<ext>.test` pair out of a single directory
* (a test case under `src/test/data/diffs/`, or a sampled candidate under
* `src/test/data/samples/`). Returns `None` if the directory doesn't have both files -- this is
* not an error, since `handmade_test_code_pairs` tolerates directories that aren't (yet) complete
* test cases.
*/
pub fn code_pair_from_dir(path: &Path) -> Result<Option<(Code, Code)>> {
    let mut before_code = None;
    let mut after_code = None;

    for file_entry in fs::read_dir(path)? {
        let file_entry = file_entry?;
        let file_path = file_entry.path();

        if file_path.is_file() {
            let file_name = file_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned();

            if file_name.starts_with("before.") && file_name.ends_with(".test") {
                before_code = Some(load_side(&file_path)?);
            } else if file_name.starts_with("after.") && file_name.ends_with(".test") {
                after_code = Some(load_side(&file_path)?);
            }
        }
    }

    let (Some(mut before), Some(mut after)) = (before_code, after_code) else {
        return Ok(None);
    };

    // `ensure_parsed`, not `parse` - see `handmade_test_code`'s doc comment for why leaving
    // `ast_metadata` uncached here silently turns every downstream `metadata_of` call into a full
    // recompute, and why this is now safe against the stale-root-id hazard that blocked the first
    // attempt at this fix. No `tree_sitter::Parser` needed here at all - `ensure_parsed` builds its
    // own short-lived one internally.
    if before.metadata.language.is_some() {
        before.ensure_parsed()?;
    }
    if after.metadata.language.is_some() {
        after.ensure_parsed()?;
    }

    Ok(Some((before, after)))
}

/**
* Returns a path to a fully functional git repository that is on a temporary path.
*
* The repository contains handmade commits to be used in tests.
*/
#[cfg(feature = "stats")]
pub fn handmade_git_repository() -> Result<PathBuf> {
    let (repo_path, repo) = initialize_repository()?;
    let dirs = read_fake_git_repo_testdata()?;
    add_commits(&repo, &repo_path, dirs)?;
    Ok(repo_path)
}

#[cfg(feature = "stats")]
fn initialize_repository() -> Result<(PathBuf, Repository)> {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let repo_path = temp_dir.path().to_path_buf();

    let repo = Repository::init(repo_path.clone()).expect("Failed to initialize git repository");
    let _ = temp_dir.keep();

    Ok((repo_path, repo))
}

#[cfg(feature = "stats")]
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

#[cfg(feature = "stats")]
fn add_commits(repo: &Repository, repo_path: &Path, dirs: Vec<(u32, PathBuf)>) -> Result<()> {
    let signature =
        Signature::now("Test Author", "test@example.com").expect("Failed to create signature");

    for (commit_num, dir_path) in dirs {
        copy_test_files_to_repo(&dir_path, commit_num, repo_path)?;
        create_commit(repo, &signature, commit_num)?;
    }
    Ok(())
}

#[cfg(feature = "stats")]
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

#[cfg(feature = "stats")]
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

#[cfg(feature = "stats")]
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
        // since human-authored mappings are compared against fresh diffs purely by path. Runs
        // against a per-language sample (see `UNIT_TEST_FIXTURES`) rather than the whole corpus -
        // this property doesn't depend on tree size, and the full corpus takes minutes.
        let sampled = handmade_test_code_pairs_for(UNIT_TEST_FIXTURES)?;
        assert_eq!(
            sampled.len(),
            UNIT_TEST_FIXTURES.len(),
            "a name in UNIT_TEST_FIXTURES doesn't match any directory under src/test/data/diffs/ (typo, or fixture renamed/removed)"
        );

        for (name, (before, after)) in &sampled {
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
    #[cfg(feature = "stats")]
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
        let (before, after) = handmade_test_code_pair("rust-no-change")?;

        assert_ne!(before.contents, "");
        assert_ne!(after.contents, "");
        assert_eq!(before.contents, after.contents);

        assert!(before.metadata.language.is_some());
        assert_eq!(before.metadata.language, after.metadata.language);

        Ok(())
    }

    #[test]
    fn test_entire_path_has_mapping() -> Result<()> {
        // Use rust-no-change since all nodes should have Identical mapping
        let (before, after) = handmade_test_code_pair("rust-no-change")?;

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
        let (before2, after2) = handmade_test_code_pair("rust-hello-world-added-message")?;

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
