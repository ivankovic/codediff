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

/// Depth-first, pre-order search for the first node of `kind` at or below `node`. Includes `node`
/// itself.
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

/// Parses one path segment into (node kind, 0-indexed same-kind occurrence). Splits on the *last*
/// colon: node kinds can contain colons (Rust's "::"), the index suffix never does.
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

/// Follows path from the root node and returns the resulting node, if the path is valid.
///
/// The path is a vector of strings. Each string is one of the following:
///
/// 1) The type of the node, e.g. "expression_statement". If only the type is given, the first child
///    node matching the type is used for traversal.
/// 2) The type of the node, followed by a number, e.g. "block:3". The n-th child matching the type
///    is used for traversal. In the case of "block:3", the third block node. Note the 1-indexing
///    used to make the string easier to read for humans.
///
/// If the path is invalid, an error is returned. Each segment rescans the parent's children; use
/// [`PathCache`] to resolve many paths against one root.
pub fn node_for_path<'a>(root: Node<'a>, path: &[&str]) -> Result<Node<'a>> {
    let mut current_node = root;

    for path_segment in path {
        let (node_type, child_index) = parse_path_segment(path_segment, path)?;

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

/// One parent's children, indexed once for both directions: `by_kind` for path -> node,
/// `occurrence_of` (child id -> 0-indexed same-kind occurrence) for node -> path.
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

/// Memoized [`node_for_path`]/[`path_for_node`] for resolving many paths against one root. Without
/// it, entries that all pass through one high-fanout parent (a large flat JSON object) rescan it
/// once each, which is quadratic in the entry count. Opt-in, because the index only pays for itself
/// when the same root is queried many times.
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
            // `ParentIndex::build` indexes every child of `parent`, so this cannot miss.
            let occurrence = self.index_of(parent).occurrence_of[&current.id()];
            path.push(format!("{}:{}", kind, occurrence + 1));
            current = parent;
        }

        path.reverse();
        path
    }
}

/// The inverse of [`node_for_path`]: computes the path from the root of the tree down to `node`,
/// using the same "type" / "type:index" mini-language.
///
/// Always emits the "type:index" form, so `node_for_path(root, &path_for_node(node))` is `node`.
/// Unlike node ids, paths are stable across re-parses, which is why ground-truth mappings are keyed
/// by them.
pub fn path_for_node(node: Node) -> Vec<String> {
    let mut path = Vec::new();
    let mut current = node;

    while let Some(parent) = current.parent() {
        let kind = current.kind();

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

/// Every node's [`path_for_node`], keyed by node id, in one O(n) pass. Calling `path_for_node` per
/// node is quadratic in the width of wide parents.
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

/// Returns true if every node along `path` (including intermediate nodes, not just the final one),
/// resolved in both `before_root` and `after_root`, has a mapping in `diff` with `expected_operation`.
pub fn entire_path_has_mapping<'a>(
    path: &[&str],
    before_root: Node<'a>,
    after_root: Node<'a>,
    diff: &ASTDiff,
    expected_operation: ASTMappingOperation,
) -> Result<bool> {
    let mut current_before_path = Vec::new();
    let mut current_after_path = Vec::new();

    for &path_segment in path {
        current_before_path.push(path_segment);
        current_after_path.push(path_segment);

        match node_for_path(before_root, &current_before_path) {
            Ok(node_before) => match node_for_path(after_root, &current_after_path) {
                Ok(node_after) => {
                    let mapping = diff.mapping.get(&(node_before.id(), node_after.id()));

                    if let Some(mapping) = mapping {
                        if mapping.operation != expected_operation {
                            return Ok(false);
                        }
                    } else {
                        return Ok(false);
                    }
                }
                Err(_) => {
                    return Ok(false);
                }
            },
            Err(_) => {
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

/// The files in `src/test/data/code/`, parsed, keyed by file name without the ".test" extension
/// (stored as ".test" so the build does not treat them as code).
pub fn handmade_test_code() -> Result<HashMap<String, Code>> {
    let mut codes = handmade_unparsed_test_code()?;

    // `ensure_parsed`, not `parse`: without cached `ast_metadata` every downstream `metadata_of`
    // silently recomputes it, once per pipeline phase.
    for code in codes.values_mut() {
        if code.metadata.language.is_some() {
            code.ensure_parsed()?;
        }
    }

    Ok(codes)
}

/// [`handmade_test_code`] without parsing, for code that consumes files that are never parsed.
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

            let new_path = path.with_extension("");
            let file_name = new_path.file_name().unwrap();
            result.insert(file_name.to_string_lossy().into_owned(), code);
        }
    }

    Ok(result)
}

/// [`handmade_test_code`] copied to a temporary directory with the ".test" extension dropped, so
/// metadata detection sees the real extension: `"hello_world.rs"` -> `<tmp>/hello_world.rs`.
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

            let new_path = path.with_extension("");
            let file_name_os_str = new_path.file_name().unwrap();

            let dest_path = temp_path.join(file_name_os_str);
            fs::write(&dest_path, contents).expect("Failed to write file");

            result.insert(file_name_os_str.to_string_lossy().into_owned(), dest_path);
        }
    }

    Ok(result)
}

/// Every (before, after) pair in the whole corpus (every [`DIFF_DATASETS`] entry, despite the
/// name), parsed, keyed by fixture name. Cached for the process: it holds every fixture in memory,
/// so prefer [`handmade_test_code_pair`] or [`handmade_test_case_dirs`].
pub fn handmade_test_code_pairs() -> Result<std::sync::Arc<HashMap<String, (Code, Code)>>> {
    // `Arc` because `Code::clone` deep-copies the tree; cloning the map would copy the corpus.
    static CACHE: std::sync::OnceLock<std::sync::Arc<HashMap<String, (Code, Code)>>> =
        std::sync::OnceLock::new();
    if let Some(cached) = CACHE.get() {
        return Ok(std::sync::Arc::clone(cached));
    }
    let result = handmade_test_code_pairs_uncached()?;
    Ok(std::sync::Arc::clone(
        CACHE.get_or_init(|| std::sync::Arc::new(result)),
    ))
}

/// Every fixture as a `(name, directory)` pair, sorted by name, without reading or parsing
/// anything. For a single pass over the corpus that loads and drops one fixture at a time: the
/// cached [`handmade_test_code_pairs`] holds every parsed tree and its metadata in memory at once,
/// which does not fit a standard CI runner.
pub fn handmade_test_case_dirs() -> Result<Vec<(String, std::path::PathBuf)>> {
    let mut cases = Vec::new();

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
                cases.push((dir_name, path));
            }
        }
    }

    // `read_dir` order differs between machines.
    cases.sort();
    Ok(cases)
}

fn handmade_test_code_pairs_uncached() -> Result<HashMap<String, (Code, Code)>> {
    let mut result = HashMap::new();

    for (dir_name, path) in handmade_test_case_dirs()? {
        if let Some(pair) = code_pair_from_dir(&path)? {
            result.insert(dir_name, pair);
        }
    }

    Ok(result)
}

/// The corpus under `src/test/data/diffs/`, split by provenance: `handmade` (hand-authored),
/// `small` and `full` (sampled from the two research datasets), `stratified` (sampled per language
/// *per size bucket*, so large files are represented), and `defects4j` (a third party's list taken
/// whole, so a rate over its solved fixtures is not a rate over the dataset; see
/// `research/external/README.md`). Names are unique across all five, so readers resolve a name to
/// whichever dataset holds it.
pub const DIFF_DATASETS: &[&str] = &["handmade", "small", "full", "stratified", "defects4j"];

fn diffs_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("diffs")
}

/// The directory for fixture `name` in whichever of `DIFF_DATASETS` holds it, or `None`.
pub fn diffs_case_dir(name: &str) -> Option<std::path::PathBuf> {
    DIFF_DATASETS
        .iter()
        .map(|dataset| diffs_root().join(dataset).join(name))
        .find(|path| path.is_dir())
}

/// The human note for one fixture, `<fixture dir>/description.md`, or `None` for a name no dataset
/// holds. It says what the fixture demands, never why codediff falls short (that belongs in the
/// fixture's stub, see `test::fixtures`). A separate file because `human_mapping.json` is too
/// large to parse just to list notes, and `README.md` is generated.
pub fn note_path(name: &str) -> Option<std::path::PathBuf> {
    diffs_case_dir(name).map(|dir| dir.join("description.md"))
}

/// The note for `name`, trimmed, or `None` if there is no note file, it can't be read, or it holds
/// only whitespace.
pub fn read_note(name: &str) -> Option<String> {
    let text = std::fs::read_to_string(note_path(name)?).ok()?;
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Writes `name`'s note, or **deletes** the file when `text` is blank, so "no note" is one state on
/// disk.
pub fn write_note(name: &str, text: &str) -> Result<()> {
    let path = note_path(name).with_context(|| format!("no fixture directory for '{name}'"))?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err).with_context(|| format!("removing {path:?}")),
        }
    } else {
        std::fs::write(&path, format!("{trimmed}\n"))
            .with_context(|| format!("writing note to {path:?}"))
    }
}

/// `note` with every whitespace run collapsed to one space, for `diffs.csv`'s one-line `comment`
/// cell (so rewrapping a note does not churn the CSV).
pub fn note_as_csv_cell(note: &str) -> String {
    note.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ─── Upstream provenance ────────────────────────────────────────────────────────────────────

/// The upstream columns of one promoted `sample.csv` row.
#[derive(Debug, Clone, Default)]
pub struct SampleProvenance {
    /// The clone-directory slug, `owner-repo`. The dash is ambiguous (owners contain dashes), so
    /// [`repository_urls`] resolves it against the clone list rather than splitting it.
    pub repository: String,
    /// The commit the *after* side was taken from; the before side is its single parent, so this
    /// commit is the change the fixture captures.
    pub commit: String,
    /// Path within the repository, at `commit`.
    pub path: String,
    /// The sample's recorded comment. A promoted fixture's `description.md` supersedes it.
    pub comment: String,
}

/// Provenance read from the fixture's own generated `README.md`, so a fixture describes itself
/// without a join against `sample.csv`. `None` when there is no README (handmade fixtures).
///
/// Parses `render_readme`'s output, not free-form Markdown: each fact is the last backticked span
/// on its labelled line (on the repository line, the slug rather than the URL).
#[cfg(feature = "test-fixtures")]
pub fn readme_provenance(name: &str) -> Option<SampleProvenance> {
    let dir = diffs_case_dir(name)?;
    let readme = std::fs::read_to_string(dir.join("README.md")).ok()?;

    let backticked = |label: &str| -> String {
        readme
            .lines()
            .find(|line| line.starts_with(&format!("- **{label}:**")))
            .and_then(|line| line.rsplit_once('`').map(|(head, _)| head))
            .and_then(|head| head.rsplit_once('`').map(|(_, value)| value.to_string()))
            .unwrap_or_default()
    };

    Some(SampleProvenance {
        repository: backticked("Repository"),
        commit: backticked("Commit"),
        path: backticked("File"),
        comment: String::new(),
    })
}

/// `sample.csv` keyed by the fixture name each row was promoted to; unpromoted rows are skipped
/// and a missing file is an empty map. Gated because `csv` is not linked in a featureless
/// `cfg(test)` build.
#[cfg(feature = "test-fixtures")]
pub fn sample_provenance() -> Result<HashMap<String, SampleProvenance>> {
    let path = data_root().join("sample.csv");
    let mut out = HashMap::new();
    if !path.exists() {
        return Ok(out);
    }
    let mut reader = csv::Reader::from_path(&path)
        .with_context(|| format!("reading sample provenance from {path:?}"))?;
    for record in reader.deserialize::<HashMap<String, String>>() {
        let record = record.context("parsing a sample.csv row")?;
        let promoted_to = record.get("promoted_to").cloned().unwrap_or_default();
        if promoted_to.is_empty() {
            continue;
        }
        let field = |key: &str| record.get(key).cloned().unwrap_or_default();
        out.insert(
            promoted_to,
            SampleProvenance {
                repository: field("repository"),
                commit: field("commit"),
                path: field("path"),
                comment: field("comment"),
            },
        );
    }
    Ok(out)
}

/// Clone URL for each [`SampleProvenance::repository`] slug, from `list_of_repositories.csv`,
/// found by deriving the slug from every clone URL (the slug cannot be split back). A slug that
/// does not resolve is absent. Every listed host serves `<clone url>/commit/<sha>`.
#[cfg(feature = "test-fixtures")]
pub fn repository_urls() -> Result<HashMap<String, String>> {
    let path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("list_of_repositories.csv");
    let mut out = HashMap::new();
    if !path.exists() {
        return Ok(out);
    }
    let mut reader = csv::Reader::from_path(&path)
        .with_context(|| format!("reading repository list from {path:?}"))?;
    for record in reader.deserialize::<HashMap<String, String>>() {
        let record = record.context("parsing a list_of_repositories.csv row")?;
        let Some(url) = record.get("repository") else {
            continue;
        };
        let url = url.trim().trim_end_matches('/');
        if let Some(slug) = repository_slug(url) {
            out.entry(slug).or_insert_with(|| url.to_string());
        }
    }
    Ok(out)
}

/// The clone-directory slug for a clone URL: everything after the host, `.git` dropped, `/`
/// replaced by `-`. `None` for a URL with nothing after the host.
fn repository_slug(url: &str) -> Option<String> {
    let after_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let (_host, path) = after_scheme.split_once('/')?;
    let path = path.trim_matches('/');
    if path.is_empty() {
        return None;
    }
    Some(path.trim_end_matches(".git").replace('/', "-"))
}

/// The upstream commit URL for one fixture, or `None` when it has no sample row, no commit, or a
/// repository the clone list doesn't resolve.
pub fn upstream_commit_url(
    provenance: &SampleProvenance,
    repository_urls: &HashMap<String, String>,
) -> Option<String> {
    if provenance.commit.is_empty() {
        return None;
    }
    // `sample.csv` records some slugs with `.git`; the derived list never does.
    let slug = provenance.repository.trim_end_matches(".git");
    let url = repository_urls.get(slug)?;
    Some(format!("{url}/commit/{}", provenance.commit))
}

#[cfg(feature = "test-fixtures")]
fn data_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
}

/// Loads and parses one named fixture, cached per name for the process. The default for new test
/// code; for coverage across languages use [`UNIT_TEST_FIXTURES`] via
/// [`handmade_test_code_pairs_for`], and the full corpus only when that sample cannot do.
///
/// Returns an `Arc` because the cache never evicts and `Code::clone` deep-copies the tree: owned
/// copies per caller grow memory without bound under parallel tests.
pub fn handmade_test_code_pair(name: &str) -> Result<std::sync::Arc<(Code, Code)>> {
    type PairCache = std::sync::Mutex<HashMap<String, std::sync::Arc<(Code, Code)>>>;
    static CACHE: std::sync::OnceLock<PairCache> = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()));

    if let Some(pair) = cache.lock().unwrap().get(name) {
        return Ok(std::sync::Arc::clone(pair));
    }

    let dir = diffs_case_dir(name)
        .with_context(|| format!("No test case directory found for '{}'", name))?;
    let pair = code_pair_from_dir(&dir)?
        .with_context(|| format!("No before/after test code pair found for '{}'", name))?;
    let pair = std::sync::Arc::new(pair);
    cache
        .lock()
        .unwrap()
        .insert(name.to_string(), std::sync::Arc::clone(&pair));
    Ok(pair)
}

/// [`handmade_test_code_pair`] for several names, keyed by name.
pub fn handmade_test_code_pairs_for(
    names: &[&str],
) -> Result<HashMap<String, std::sync::Arc<(Code, Code)>>> {
    names
        .iter()
        .map(|&name| Ok((name.to_string(), handmade_test_code_pair(name)?)))
        .collect()
}

/// 1-3 small fixtures per language, for tests that need every language but not the corpus. Chosen
/// for parse cost only: small does not mean fast to diff (cost follows tree shape), so a test that
/// runs the diff over this set belongs behind `#[ignore = "slow"]`.
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
/// `metadata.path` set.
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

/// Reads and parses the `before.<ext>.test` / `after.<ext>.test` pair in `path`, with AST metadata.
/// `None`, not an error, if either file is missing.
pub fn code_pair_from_dir(path: &Path) -> Result<Option<(Code, Code)>> {
    let Some((mut before, mut after)) = code_pair_from_dir_without_metadata(path)? else {
        return Ok(None);
    };

    // The tree is already parsed; this adds the metadata (see `handmade_test_code`).
    if before.metadata.language.is_some() {
        before.ensure_parsed()?;
    }
    if after.metadata.language.is_some() {
        after.ensure_parsed()?;
    }

    Ok(Some((before, after)))
}

/// [`code_pair_from_dir`] without the AST metadata: both sides read and parsed with tree-sitter,
/// `ast_metadata` left `None`.
///
/// For corpus scans that only walk the tree, where the metadata is most of the load cost. Anything
/// that diffs must use [`code_pair_from_dir`]: without cached metadata every `metadata_of`
/// recomputes it.
pub fn code_pair_from_dir_without_metadata(path: &Path) -> Result<Option<(Code, Code)>> {
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

    let mut parser = tree_sitter::Parser::new();
    before.parse(&mut parser);
    after.parse(&mut parser);

    Ok(Some((before, after)))
}

/// A temporary git repository holding one commit per numbered directory of
/// `src/test/data/fake-git-repo/`.
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
            if path.is_dir()
                && let Some(dir_name) = path.file_name()
                && let Ok(num) = dir_name.to_string_lossy().parse::<u32>()
            {
                return Some((num, path));
            }
            None
        })
        .collect();

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
    use super::*;

    #[test]
    fn repository_slug_matches_the_clone_directory_name_sample_csv_records() {
        assert_eq!(
            repository_slug("https://github.com/awslabs/aws-c-common").as_deref(),
            Some("awslabs-aws-c-common")
        );
        assert_eq!(
            repository_slug("https://gitlab.com/gitlab-org/gitlab-runner").as_deref(),
            Some("gitlab-org-gitlab-runner")
        );
        assert_eq!(
            repository_slug("https://codeberg.org/dnkl/foot.git").as_deref(),
            Some("dnkl-foot")
        );
        // Why the slug is derived from URLs rather than split: the owner contains a dash.
        assert_eq!(
            repository_slug("https://github.com/Ondsel-Development/OndselSolver").as_deref(),
            Some("Ondsel-Development-OndselSolver")
        );
        assert_eq!(repository_slug("https://git.libreoffice.org"), None);
    }

    #[test]
    fn upstream_commit_url_needs_a_commit_and_a_resolvable_repository() {
        let urls = HashMap::from([(
            "awslabs-aws-c-common".to_string(),
            "https://github.com/awslabs/aws-c-common".to_string(),
        )]);
        let sample = |repository: &str, commit: &str| SampleProvenance {
            repository: repository.to_string(),
            commit: commit.to_string(),
            path: "include/aws/common/file.h".to_string(),
            comment: String::new(),
        };

        assert_eq!(
            upstream_commit_url(&sample("awslabs-aws-c-common", "fbb2123"), &urls).as_deref(),
            Some("https://github.com/awslabs/aws-c-common/commit/fbb2123")
        );
        // `sample.csv` records some slugs with the `.git` suffix; the clone list never does.
        assert_eq!(
            upstream_commit_url(&sample("awslabs-aws-c-common.git", "fbb2123"), &urls).as_deref(),
            Some("https://github.com/awslabs/aws-c-common/commit/fbb2123")
        );
        assert!(upstream_commit_url(&sample("awslabs-aws-c-common", ""), &urls).is_none());
        assert!(upstream_commit_url(&sample("nobody-nothing", "fbb2123"), &urls).is_none());
    }

    /// Against the real corpus, so a renamed repository or a new host fails here rather than
    /// silently dropping links.
    #[cfg(feature = "test-fixtures")]
    #[test]
    fn the_corpus_provenance_resolves_to_upstream_urls() {
        let provenance = sample_provenance().expect("sample.csv should parse");
        let urls = repository_urls().expect("list_of_repositories.csv should parse");
        assert!(
            provenance.len() > 400,
            "only {} promoted samples - sample.csv is not being read",
            provenance.len()
        );

        let resolved = provenance
            .values()
            .filter(|sample| upstream_commit_url(sample, &urls).is_some())
            .count();
        let rate = 100.0 * resolved as f64 / provenance.len() as f64;
        assert!(
            rate > 99.0,
            "only {resolved} of {} promoted samples resolve to an upstream commit ({rate:.1}%)",
            provenance.len()
        );
    }

    use crate::code::Language;

    /// Removes `name`'s description.md on drop, so a panicking test leaves no note in the real
    /// corpus.
    struct NoteGuard(&'static str);

    impl Drop for NoteGuard {
        fn drop(&mut self) {
            let _ = write_note(self.0, "");
        }
    }

    /// Uses the real fixture directory: `diffs_case_dir` cannot point at a temp dir.
    #[test]
    fn a_note_round_trips_and_a_blank_one_deletes_the_file() -> Result<()> {
        // A fixture with no description.md of its own, so nothing real is overwritten.
        const CASE: &str = "rust-hello-world-added-message";
        assert!(
            read_note(CASE).is_none(),
            "{CASE} was expected to have no note"
        );
        let _guard = NoteGuard(CASE);

        write_note(CASE, "  a note, with surrounding space  ")?;
        assert_eq!(
            read_note(CASE).as_deref(),
            Some("a note, with surrounding space")
        );
        assert!(note_path(CASE).expect("a real case").exists());

        write_note(CASE, "   ")?;
        assert!(read_note(CASE).is_none());
        assert!(!note_path(CASE).expect("a real case").exists());

        // Deleting a note that is already gone is not an error.
        write_note(CASE, "")?;
        Ok(())
    }

    #[test]
    fn a_name_no_dataset_holds_has_no_note_path() {
        assert!(note_path("no-such-fixture-anywhere").is_none());
        assert!(read_note("no-such-fixture-anywhere").is_none());
        assert!(write_note("no-such-fixture-anywhere", "x").is_err());
    }

    #[test]
    fn a_multi_line_note_becomes_one_csv_line() {
        assert_eq!(
            note_as_csv_cell("first line\n\nsecond   line\n"),
            "first line second line"
        );
        assert_eq!(note_as_csv_cell(""), "");
    }

    #[test]
    fn the_descriptions_already_in_the_corpus_are_readable() {
        let note = read_note("rust-no-change").expect("rust-no-change has a description.md");
        assert!(note.contains("identical"), "got {note:?}");
    }

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
    fn find_first_of_kind_includes_the_starting_node_and_searches_depth_first() -> Result<()> {
        let test_codes = handmade_test_code()?;
        let code = test_codes.get("hello-world.rs").unwrap();
        let root = code.ast.as_ref().unwrap().root_node();

        let function = find_first_of_kind(root, "function_item").unwrap();
        assert_eq!(
            find_first_of_kind(function, "function_item").map(|n| n.id()),
            Some(function.id())
        );
        // Pre-order: `main`, not the later `println` macro identifier.
        let identifier = find_first_of_kind(root, "identifier").unwrap();
        assert_eq!(identifier.utf8_text(code.contents.as_bytes())?, "main");
        assert!(find_first_of_kind(root, "no_such_kind").is_none());
        Ok(())
    }

    #[test]
    fn a_path_segment_splits_on_its_last_colon() -> Result<()> {
        assert_eq!(parse_path_segment("block:3", &[])?, ("block", 2));
        assert_eq!(parse_path_segment("block", &[])?, ("block", 0));
        assert_eq!(parse_path_segment(":::2", &[])?, ("::", 1));
        assert!(parse_path_segment("block:x", &[]).is_err());
        Ok(())
    }

    #[test]
    fn precompute_paths_agrees_with_path_for_node() -> Result<()> {
        let test_codes = handmade_test_code()?;
        let code = test_codes.get("hello-world.rs").unwrap();
        let root = code.ast.as_ref().unwrap().root_node();

        let paths = precompute_paths(root);
        let mut stack = vec![root];
        let mut visited = 0;
        while let Some(node) = stack.pop() {
            assert_eq!(paths[&node.id()], path_for_node(node), "{}", node.kind());
            visited += 1;
            let mut cursor = node.walk();
            stack.extend(node.children(&mut cursor));
        }
        assert_eq!(paths.len(), visited);
        Ok(())
    }

    #[test]
    fn test_path_for_node_round_trips_through_node_for_path() -> Result<()> {
        let sampled = handmade_test_code_pairs_for(UNIT_TEST_FIXTURES)?;
        assert_eq!(
            sampled.len(),
            UNIT_TEST_FIXTURES.len(),
            "a name in UNIT_TEST_FIXTURES doesn't match any directory under src/test/data/diffs/ (typo, or fixture renamed/removed)"
        );

        for (name, pair) in &sampled {
            let (before, after) = &**pair;
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
    fn path_cache_resolve_matches_node_for_path_for_every_node() -> Result<()> {
        // Compares node ids, not just success: resolving to a different valid node is the bug.
        let sampled = handmade_test_code_pairs_for(UNIT_TEST_FIXTURES)?;

        for (name, pair) in &sampled {
            let (before, after) = &**pair;
            for (label, code) in [("before", before), ("after", after)] {
                let ast = code
                    .ast
                    .as_ref()
                    .unwrap_or_else(|| panic!("{} {} has no AST", name, label));
                let root = ast.root_node();
                let mut cache = PathCache::new();

                let mut stack = vec![root];
                while let Some(node) = stack.pop() {
                    let path = path_for_node(node);
                    let path_refs: Vec<&str> = path.iter().map(String::as_str).collect();

                    let via_scan = node_for_path(root, &path_refs).unwrap_or_else(|e| {
                        panic!(
                            "{} {}: node_for_path failed to resolve {:?}: {}",
                            name, label, path_refs, e
                        )
                    });
                    let via_cache = cache.resolve(root, &path_refs).unwrap_or_else(|e| {
                        panic!(
                            "{} {}: PathCache::resolve failed to resolve {:?}: {}",
                            name, label, path_refs, e
                        )
                    });
                    assert_eq!(
                        via_scan.id(),
                        via_cache.id(),
                        "{} {}: node_for_path and PathCache::resolve disagreed on {:?}",
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

    /// The walk without the parse: loading the whole corpus is too expensive for a unit test.
    #[test]
    fn test_handmade_test_case_dirs_lists_every_diff() -> Result<()> {
        let names: Vec<String> = handmade_test_case_dirs()?
            .into_iter()
            .map(|(name, _)| name)
            .collect();

        assert!(names.contains(&"rust-no-change".to_string()));
        assert!(names.contains(&"rust-hello-world-added-message".to_string()));
        assert!(names.contains(&"rust-leetcode-1-bugfix".to_string()));
        assert!(names.len() > 3);
        assert!(names.is_sorted(), "sorted so every caller sees one order");

        Ok(())
    }

    #[test]
    fn test_handmade_test_code_pairs_no_change_diff() -> Result<()> {
        let (before, after) = &*handmade_test_code_pair("rust-no-change")?;

        assert_ne!(before.contents, "");
        assert_ne!(after.contents, "");
        assert_eq!(before.contents, after.contents);

        assert!(before.metadata.language.is_some());
        assert_eq!(before.metadata.language, after.metadata.language);

        Ok(())
    }

    #[test]
    fn test_entire_path_has_mapping() -> Result<()> {
        let (before, after) = &*handmade_test_code_pair("rust-no-change")?;

        let diff = crate::diff::diff_code(before, after);
        let diff_ast = diff.ast.unwrap();
        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();

        let before_root = before_ast.root_node();
        let after_root = after_ast.root_node();

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

        let path = vec!["impl_item"];
        assert!(!entire_path_has_mapping(
            &path,
            before_root,
            after_root,
            &diff_ast,
            ASTMappingOperation::MatchButNotIdentical
        )?);

        // A path that does not resolve is false, not an error.
        let path = vec!["impl_item", "nonexistent"];
        assert!(!entire_path_has_mapping(
            &path,
            before_root,
            after_root,
            &diff_ast,
            ASTMappingOperation::Identical
        )?);

        let (before2, after2) = &*handmade_test_code_pair("rust-hello-world-added-message")?;

        let diff2 = crate::diff::diff_code(before2, after2);
        let diff_ast2 = diff2.ast.unwrap();
        let before_ast2 = before2.ast.as_ref().unwrap();
        let after_ast2 = after2.ast.as_ref().unwrap();
        let before_root2 = before_ast2.root_node();
        let after_root2 = after_ast2.root_node();

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
