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

/**
* Human-authored ground-truth AST mappings, used to check codediff's output against what a human
* considers the optimal diff.
*
* These are produced by the `human_solver` binary (src/bin/human_solver.rs), which lets a human
* walk the before/after ASTs of a test case side by side and mark nodes as matching, deleted or
* inserted. The result is stored as JSON in `src/test/data/diffs/<name>/human_mapping.json`.
*
* Nodes are identified by *path* (see [`super::path_for_node`] / [`super::node_for_path`]) rather
* than by TreeSitter node ID, because node IDs are arena slots that are not stable across separate
* parses of the same source: the human_solver process parses the code once to build the mapping,
* and the test that later verifies it parses the code again to compute the diff. Paths, being
* derived purely from node kind and sibling position, are stable across both parses.
*/
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tree_sitter::Node;

use crate::code::ASTMetadata;
use crate::diff::cost::operation_cost;
use crate::diff::{ASTDiff, ASTMapping, ASTMappingOperation, ASTMappingReason, NodeCache};
use crate::test::helper::{PathCache, node_for_path};

/// What a human decided should happen to a node (or pair of nodes) between before and after.
///
/// `Identical`, `Update` and `MatchButNotIdentical` all pair a before node with an after node
/// (like the old single `Match` variant did), but also pin down *which* [`ASTMappingOperation`]
/// codediff is expected to have chosen for that pair, not just that the pair is mapped together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanOperation {
    /// The before and after nodes are the same node, with no difference at all: same kind, no
    /// children, and identical text (or, if either has children, the human confirmed the whole
    /// subtree is unchanged). Expects codediff to have chosen [`ASTMappingOperation::Identical`].
    Identical,
    /// The before and after nodes have the same kind and no children, but different text (e.g. a
    /// changed string literal). Expects [`ASTMappingOperation::Update`].
    Update,
    /// The before and after nodes are matched, but not identical: either they have children and
    /// the human confirmed the subtree differs somewhere, or they have different kinds and the
    /// human confirmed the mapping anyway. Expects [`ASTMappingOperation::MatchButNotIdentical`].
    MatchButNotIdentical,
    /// The before node was removed; its children, if any, are handled by other entries.
    Delete,
    /// The before node and its entire subtree were removed.
    DeleteWithChildren,
    /// The after node is new; its children, if any, are handled by other entries.
    Insert,
    /// The after node and its entire subtree are new.
    InsertWithChildren,
}

/// One human-authored decision about a node (`Delete`/`Insert`) or a pair of nodes
/// (`Identical`/`Update`/`MatchButNotIdentical`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanMappingEntry {
    pub operation: HumanOperation,
    /// Path to the node in the before tree. Present for `Identical`, `Update`,
    /// `MatchButNotIdentical`, `Delete` and `DeleteWithChildren`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub before_path: Option<Vec<String>>,
    /// Path to the node in the after tree. Present for `Identical`, `Update`,
    /// `MatchButNotIdentical`, `Insert` and `InsertWithChildren`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub after_path: Option<Vec<String>>,
}

/// The full set of human decisions for one before/after test case.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HumanMapping {
    pub entries: Vec<HumanMappingEntry>,
}

/// Path to the `human_mapping.json` file for a given test case name (e.g. "rust-add-if").
pub fn mapping_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("diffs")
        .join(name)
        .join("human_mapping.json")
}

/// Loads the human mapping for a given test case name.
pub fn load(name: &str) -> Result<HumanMapping> {
    let path = mapping_path(name);
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("reading human mapping at {:?}", path))?;
    let mapping: HumanMapping = serde_json::from_str(&contents)
        .with_context(|| format!("parsing human mapping at {:?}", path))?;
    Ok(mapping)
}

/// Saves the human mapping for a given test case name, overwriting any existing file.
pub fn save(name: &str, mapping: &HumanMapping) -> Result<()> {
    let path = mapping_path(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(mapping)?;
    fs::write(&path, json).with_context(|| format!("writing human mapping to {:?}", path))?;
    Ok(())
}

fn path_refs(path: &[String]) -> Vec<&str> {
    path.iter().map(String::as_str).collect()
}

/// What kind of human-authored removal a node is marked with: `Deleted` from the before tree, or
/// `Inserted` in the after tree.
///
/// Shared between `human_solver` (which lets a human create/edit a `HumanMapping`) and any
/// read-only consumer that just needs to interpret one (e.g. a static site generator) - moved
/// here, rather than kept private to `human_solver.rs`, specifically so a second consumer doesn't
/// have to carry its own copy of what matched/deleted/inserted/inherited means and risk it
/// silently drifting from the TUI's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkKind {
    Deleted,
    Inserted,
}

/// A node's status under a [`HumanMapping`]: unmarked (no entry says anything about it, directly
/// or via an ancestor), matched to a specific counterpart, or marked deleted/inserted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeStatus {
    Unmarked,
    Matched,
    /// `inherited` is true when this node isn't marked directly but an ancestor is marked
    /// with `with_children`, implying this node too.
    Marked {
        kind: MarkKind,
        with_children: bool,
        inherited: bool,
    },
}

/// Resolved node IDs for every entry in a [`HumanMapping`], used to look up a node's
/// [`NodeStatus`] in O(1) (plus a bounded ancestor walk for inheritance) - see [`rebuild_caches`].
#[derive(Default)]
pub struct Caches {
    pub before_match: HashMap<usize, usize>,
    pub after_match: HashMap<usize, usize>,
    pub before_removed: HashMap<usize, bool>,
    pub after_removed: HashMap<usize, bool>,
    /// Whether a matched node's pair was recorded as [`HumanOperation::Identical`] (`true`) or
    /// [`HumanOperation::Update`]/[`HumanOperation::MatchButNotIdentical`] (`false`). `before_match`/
    /// `after_match` alone can't tell these apart - both write the same node-id pair into those maps
    /// regardless of which of the three operations produced them - so a second map is needed for any
    /// consumer (e.g. `generate_mapping_site`'s "hide identical matches" toggle) that cares whether a
    /// match is a real edit or genuinely unchanged. Absent key means "not recorded" (e.g. a `Caches`
    /// built by hand rather than via `rebuild_caches`), which callers should treat as identical, to
    /// match matched nodes' pre-existing default rendering/quietness.
    pub before_identical: HashMap<usize, bool>,
    pub after_identical: HashMap<usize, bool>,
    /// Number of entries that couldn't be resolved against the current trees (e.g. a
    /// hand-edited or stale mapping file). Surfaced by callers (e.g. `human_solver`'s footer)
    /// rather than treated as fatal, so a bad mapping file doesn't block the caller outright.
    pub unresolved: usize,
}

/// Builds lookup caches from `entries`, skipping (and counting) any entry that doesn't resolve
/// against the current trees rather than failing outright.
pub fn rebuild_caches(
    entries: &[HumanMappingEntry],
    before_root: Node,
    after_root: Node,
) -> Caches {
    let mut caches = Caches::default();

    for entry in entries {
        let resolved = match entry.operation {
            HumanOperation::Identical
            | HumanOperation::Update
            | HumanOperation::MatchButNotIdentical => (|| {
                let before_path = entry.before_path.as_ref()?;
                let after_path = entry.after_path.as_ref()?;
                let b = node_for_path(before_root, &path_refs(before_path)).ok()?;
                let a = node_for_path(after_root, &path_refs(after_path)).ok()?;
                caches.before_match.insert(b.id(), a.id());
                caches.after_match.insert(a.id(), b.id());
                let identical = entry.operation == HumanOperation::Identical;
                caches.before_identical.insert(b.id(), identical);
                caches.after_identical.insert(a.id(), identical);
                Some(())
            })(),
            HumanOperation::Delete | HumanOperation::DeleteWithChildren => (|| {
                let before_path = entry.before_path.as_ref()?;
                let b = node_for_path(before_root, &path_refs(before_path)).ok()?;
                caches.before_removed.insert(
                    b.id(),
                    entry.operation == HumanOperation::DeleteWithChildren,
                );
                Some(())
            })(),
            HumanOperation::Insert | HumanOperation::InsertWithChildren => (|| {
                let after_path = entry.after_path.as_ref()?;
                let a = node_for_path(after_root, &path_refs(after_path)).ok()?;
                caches.after_removed.insert(
                    a.id(),
                    entry.operation == HumanOperation::InsertWithChildren,
                );
                Some(())
            })(),
        };

        if resolved.is_none() {
            caches.unresolved += 1;
        }
    }

    caches
}

/// True if some strict ancestor of `node` is marked with `with_children = true` in `removed`.
pub fn is_inherited_removed(node: Node, removed: &HashMap<usize, bool>) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if removed.get(&parent.id()) == Some(&true) {
            return true;
        }
        current = parent;
    }
    false
}

/// Whether a `Matched` before-node's pair was recorded as `Identical` rather than
/// `Update`/`MatchButNotIdentical`. Meaningless (and unconsulted) for any other [`NodeStatus`].
/// Defaults to `true` when `node` isn't in `before_identical` at all - either because it isn't
/// matched, or because `caches` was built by hand rather than via [`rebuild_caches`] (as several
/// tests do), in which case treating it as identical preserves those matched nodes' existing
/// "quiet"/undecorated rendering.
pub fn is_identical_before(node: Node, caches: &Caches) -> bool {
    caches
        .before_identical
        .get(&node.id())
        .copied()
        .unwrap_or(true)
}

/// After-side counterpart of [`is_identical_before`].
pub fn is_identical_after(node: Node, caches: &Caches) -> bool {
    caches
        .after_identical
        .get(&node.id())
        .copied()
        .unwrap_or(true)
}

pub fn status_before(node: Node, caches: &Caches) -> NodeStatus {
    if caches.before_match.contains_key(&node.id()) {
        return NodeStatus::Matched;
    }
    if let Some(&with_children) = caches.before_removed.get(&node.id()) {
        return NodeStatus::Marked {
            kind: MarkKind::Deleted,
            with_children,
            inherited: false,
        };
    }
    if is_inherited_removed(node, &caches.before_removed) {
        return NodeStatus::Marked {
            kind: MarkKind::Deleted,
            with_children: true,
            inherited: true,
        };
    }
    NodeStatus::Unmarked
}

pub fn status_after(node: Node, caches: &Caches) -> NodeStatus {
    if caches.after_match.contains_key(&node.id()) {
        return NodeStatus::Matched;
    }
    if let Some(&with_children) = caches.after_removed.get(&node.id()) {
        return NodeStatus::Marked {
            kind: MarkKind::Inserted,
            with_children,
            inherited: false,
        };
    }
    if is_inherited_removed(node, &caches.after_removed) {
        return NodeStatus::Marked {
            kind: MarkKind::Inserted,
            with_children: true,
            inherited: true,
        };
    }
    NodeStatus::Unmarked
}

/// Helper function to find a node by ID in a tree and return its kind.
/// Returns "None" if the node is not found, "0" if the ID is 0,
/// or the node kind if found.
fn node_kind_for_id(root: Node, node_id: usize) -> String {
    if node_id == 0 {
        return "0".to_string();
    }

    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.id() == node_id {
            return n.kind().to_string();
        }

        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }

    "None".to_string()
}

/// Pushes a mismatch for every node in `node`'s subtree (inclusive) that isn't mapped to zero
/// (i.e. deleted, if `node` is in the before tree, or inserted, if in the after tree) in `node_map`.
fn check_subtree_maps_to_zero(
    node: Node,
    node_map: &rustc_hash::FxHashMap<usize, usize>,
    context: &str,
    mismatches: &mut Vec<String>,
    lookup_root: Node,
) {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        match node_map.get(&n.id()) {
            Some(0) => {}
            other => {
                let mapped_kind = match other {
                    Some(&mapped_id) => node_kind_for_id(lookup_root, mapped_id),
                    None => "None".to_string(),
                };
                mismatches.push(format!(
                    "{}: descendant node '{}' was expected to be removed (mapped to 0), but was mapped to {}",
                    context,
                    n.kind(),
                    mapped_kind
                ))
            }
        }

        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
}

/// Formats " (op X, reason Y)" for the mapping the before node actually landed in, so a mismatch
/// message identifies which pass produced the wrong mapping (via `ASTMappingReason`), not just
/// what it mapped to. Empty string when there's no mapping to describe.
fn actual_mapping_info(
    diff_ast: &ASTDiff,
    before_id: usize,
    actual_partner: Option<usize>,
) -> String {
    let Some(partner) = actual_partner else {
        return String::new();
    };
    match diff_ast.mapping.get(&(before_id, partner)) {
        Some(m) => format!(" (op {:?}, reason {:?})", m.operation, m.reason),
        None => String::new(),
    }
}

/// After-side counterpart of `actual_mapping_info` (mapping keys are `(before, after)`).
fn actual_mapping_info_after(
    diff_ast: &ASTDiff,
    after_id: usize,
    actual_partner: Option<usize>,
) -> String {
    let Some(partner) = actual_partner else {
        return String::new();
    };
    match diff_ast.mapping.get(&(partner, after_id)) {
        Some(m) => format!(" (op {:?}, reason {:?})", m.operation, m.reason),
        None => String::new(),
    }
}

/// The [`ASTMappingOperation`] codediff is expected to have chosen for a matched pair, given the
/// human's [`HumanOperation`] for that pair.
fn expected_ast_operation(operation: HumanOperation) -> Option<ASTMappingOperation> {
    match operation {
        HumanOperation::Identical => Some(ASTMappingOperation::Identical),
        HumanOperation::Update => Some(ASTMappingOperation::Update),
        HumanOperation::MatchButNotIdentical => Some(ASTMappingOperation::MatchButNotIdentical),
        HumanOperation::Delete
        | HumanOperation::DeleteWithChildren
        | HumanOperation::Insert
        | HumanOperation::InsertWithChildren => None,
    }
}

/**
* Total edit cost of a human-authored `HumanMapping` under the same unit-cost model as
* `crate::diff::cost::diff_cost`, so the two numbers are directly comparable -
* `benchmark_optimal_solutions` prints both per fixture.
*
* Reuses `operation_cost` for the per-entry cost table rather than reimplementing it, so codediff's
* cost and the human's cost can never silently drift apart from having two separate copies of the
* same table. `before_metadata`/`after_metadata` must come from the same parsed `Code` as
* `before_root`/`after_root` (tree-sitter node ids are only stable within one parse - see
* `ASTNodeMetadata::start_byte`'s doc comment) so `node_for_path`'s resolved ids can look up subtree
* sizes in them for `DeleteWithChildren`/`InsertWithChildren` entries.
*
* Only sums entries actually present in `mapping` - an unannotated node contributes nothing. That's
* fine as long as the mapping is *complete over every actual change* (every unannotated node is
* genuinely unchanged, and therefore costs 0 whether or not it's written down): most fixtures'
* `human_mapping.json` only has a few hundred entries against many thousands of nodes precisely
* because the rest is untouched code, not because the human skipped grading real edits. If a fixture
* ever *does* leave a real change unannotated, this will silently undercount the human side and
* inflate `diff_cost - human_mapping_cost` for a reason that has nothing to do with the algorithm -
* worth checking with `--details` before trusting a surprising gap on an unfamiliar fixture.
*/
pub fn human_mapping_cost(
    mapping: &HumanMapping,
    before_root: Node,
    after_root: Node,
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
) -> Result<u64> {
    // Same one-cache-per-root-per-loop pattern as `check_entry`'s caller - see `PathCache`'s own
    // doc comment for why a fixture with many DeleteWithChildren/InsertWithChildren entries under
    // one large flat parent (e.g. a big JSON object) needs this to stay linear.
    let mut before_cache = PathCache::new();
    let mut after_cache = PathCache::new();

    let mut total = 0u64;
    for entry in &mapping.entries {
        let (operation, subtree_size) = match entry.operation {
            HumanOperation::Identical => (ASTMappingOperation::Identical, 1),
            HumanOperation::Update => (ASTMappingOperation::Update, 1),
            HumanOperation::MatchButNotIdentical => (ASTMappingOperation::MatchButNotIdentical, 1),
            HumanOperation::Delete => (ASTMappingOperation::Delete, 1),
            HumanOperation::Insert => (ASTMappingOperation::Insert, 1),
            HumanOperation::DeleteWithChildren => {
                let path = entry
                    .before_path
                    .as_ref()
                    .context("DeleteWithChildren entry is missing before_path")?;
                let node = before_cache
                    .resolve(before_root, &path_refs(path))
                    .with_context(|| format!("resolving before_path {:?}", path))?;
                let size = before_metadata
                    .node_to_subtree_size
                    .get(&node.id())
                    .copied()
                    .unwrap_or(1);
                (ASTMappingOperation::DeleteWithChildren, size)
            }
            HumanOperation::InsertWithChildren => {
                let path = entry
                    .after_path
                    .as_ref()
                    .context("InsertWithChildren entry is missing after_path")?;
                let node = after_cache
                    .resolve(after_root, &path_refs(path))
                    .with_context(|| format!("resolving after_path {:?}", path))?;
                let size = after_metadata
                    .node_to_subtree_size
                    .get(&node.id())
                    .copied()
                    .unwrap_or(1);
                (ASTMappingOperation::InsertWithChildren, size)
            }
        };
        total += operation_cost(&operation, subtree_size);
    }
    Ok(total)
}

/**
* Loads the human mapping for `name` and computes its total edit cost (see
* [`human_mapping_cost`]), resolving paths against a fresh parse of `before`/`after`.
*
* Convenience wrapper for callers (like `benchmark_optimal_solutions`) that only have a fixture
* name and a `Code` pair, not an already-loaded `HumanMapping`/already-built `ASTMetadata`.
*/
pub fn human_mapping_cost_for(
    name: &str,
    before: &crate::code::Code,
    after: &crate::code::Code,
) -> Result<u64> {
    let mapping = load(name)?;
    let before_ast = before.ast.as_ref().context("Before code has no AST")?;
    let after_ast = after.ast.as_ref().context("After code has no AST")?;
    let before_metadata = crate::code::metadata::metadata_of(before);
    let after_metadata = crate::code::metadata::metadata_of(after);
    human_mapping_cost(
        &mapping,
        before_ast.root_node(),
        after_ast.root_node(),
        &before_metadata,
        &after_metadata,
    )
}

/// Builds a synthetic `ASTDiff` from `name`'s human mapping, resolving every entry's path(s)
/// against a fresh parse of `before`/`after` and feeding them through `ASTDiff::add_mapping` -
/// same shape as `human_mapping_cost`, but producing the full `ASTDiff` rather than just a total
/// cost, so any machinery that only knows how to consume a real `ASTDiff` (e.g.
/// `diff::text::TextDiff`) can treat the human-authored mapping exactly like codediff's own
/// output. Used by `benchmark_other` to project the human mapping down to per-line labels via the
/// same `TextDiff`/`line_operations` path codediff's own diff goes through, so the two are
/// comparable on equal footing.
///
/// `cost`/`reason` on the resulting `ASTMapping`s are placeholders - nothing that consumes a
/// synthetic diff built this way needs them, only `operation` and the node-id maps
/// (`ASTDiff::mapping_for_node`, which `diff::text::ranges` walks, only reads `operation`).
pub fn as_ast_diff(
    name: &str,
    before: &crate::code::Code,
    after: &crate::code::Code,
) -> Result<ASTDiff> {
    let mapping = load(name)?;
    let before_ast = before.ast.as_ref().context("Before code has no AST")?;
    let after_ast = after.ast.as_ref().context("After code has no AST")?;
    let before_root = before_ast.root_node();
    let after_root = after_ast.root_node();

    let mut before_cache = PathCache::new();
    let mut after_cache = PathCache::new();

    let mut diff = ASTDiff::default();
    for entry in &mapping.entries {
        let before_id = match &entry.before_path {
            Some(path) => before_cache
                .resolve(before_root, &path_refs(path))
                .with_context(|| format!("resolving before_path {:?}", path))?
                .id(),
            None => 0,
        };
        let after_id = match &entry.after_path {
            Some(path) => after_cache
                .resolve(after_root, &path_refs(path))
                .with_context(|| format!("resolving after_path {:?}", path))?
                .id(),
            None => 0,
        };
        let operation = match entry.operation {
            HumanOperation::Identical => ASTMappingOperation::Identical,
            HumanOperation::Update => ASTMappingOperation::Update,
            HumanOperation::MatchButNotIdentical => ASTMappingOperation::MatchButNotIdentical,
            HumanOperation::Delete => ASTMappingOperation::Delete,
            HumanOperation::DeleteWithChildren => ASTMappingOperation::DeleteWithChildren,
            HumanOperation::Insert => ASTMappingOperation::Insert,
            HumanOperation::InsertWithChildren => ASTMappingOperation::InsertWithChildren,
        };
        diff.add_mapping(
            before_id,
            after_id,
            ASTMapping {
                cost: 0,
                operation,
                reason: ASTMappingReason::default(),
            },
        );
    }
    Ok(diff)
}

fn check_entry<'b, 'a>(
    entry: &HumanMappingEntry,
    before_root: Node<'b>,
    after_root: Node<'a>,
    diff_ast: &ASTDiff,
    mismatches: &mut Vec<String>,
    before_cache: &mut PathCache<'b>,
    after_cache: &mut PathCache<'a>,
) -> Result<()> {
    match entry.operation {
        HumanOperation::Identical
        | HumanOperation::Update
        | HumanOperation::MatchButNotIdentical => {
            let before_path = entry
                .before_path
                .as_ref()
                .with_context(|| format!("{:?} entry is missing before_path", entry.operation))?;
            let after_path = entry
                .after_path
                .as_ref()
                .with_context(|| format!("{:?} entry is missing after_path", entry.operation))?;

            let before_node = before_cache
                .resolve(before_root, &path_refs(before_path))
                .with_context(|| format!("resolving before_path {:?}", before_path))?;
            let after_node = after_cache
                .resolve(after_root, &path_refs(after_path))
                .with_context(|| format!("resolving after_path {:?}", after_path))?;

            let actual_partner = diff_ast.before_node_map.get(&before_node.id()).copied();
            if actual_partner != Some(after_node.id()) {
                let mapped_kind = match actual_partner {
                    Some(mapped_id) => node_kind_for_id(after_root, mapped_id),
                    None => "None".to_string(),
                };
                mismatches.push(format!(
                    "{:?} {:?} <-> {:?}: expected before node '{}' to map to after node '{}', but it mapped to {}{}",
                    entry.operation,
                    before_path,
                    after_path,
                    before_node.kind(),
                    after_node.kind(),
                    mapped_kind,
                    actual_mapping_info(diff_ast, before_node.id(), actual_partner)
                ));
                return Ok(());
            }

            let expected_op = expected_ast_operation(entry.operation).expect(
                "Identical/Update/MatchButNotIdentical always have an expected ASTMappingOperation",
            );
            match diff_ast.mapping.get(&(before_node.id(), after_node.id())) {
                Some(actual_mapping) if actual_mapping.operation == expected_op => {}
                Some(actual_mapping) => mismatches.push(format!(
                    "{:?} {:?} <-> {:?}: expected codediff operation {:?}, but it chose {:?}",
                    entry.operation, before_path, after_path, expected_op, actual_mapping.operation
                )),
                None => mismatches.push(format!(
                    "{:?} {:?} <-> {:?}: nodes are mapped to each other but have no ASTMapping entry (unexpected)",
                    entry.operation, before_path, after_path
                )),
            }
        }
        HumanOperation::Delete | HumanOperation::DeleteWithChildren => {
            let before_path = entry
                .before_path
                .as_ref()
                .context("Delete entry is missing before_path")?;
            let before_node = before_cache
                .resolve(before_root, &path_refs(before_path))
                .with_context(|| format!("resolving before_path {:?}", before_path))?;

            if entry.operation == HumanOperation::DeleteWithChildren {
                check_subtree_maps_to_zero(
                    before_node,
                    &diff_ast.before_node_map,
                    &format!("Delete (with children) {:?}", before_path),
                    mismatches,
                    after_root,
                );
            } else {
                let actual = diff_ast.before_node_map.get(&before_node.id()).copied();
                if actual != Some(0) {
                    let mapped_kind = match actual {
                        Some(mapped_id) => node_kind_for_id(after_root, mapped_id),
                        None => "None".to_string(),
                    };
                    mismatches.push(format!(
                        "Delete {:?}: expected before node '{}' to be removed (mapped to 0), but it mapped to {}{}",
                        before_path,
                        before_node.kind(),
                        mapped_kind,
                        actual_mapping_info(diff_ast, before_node.id(), actual)
                    ));
                }
            }
        }
        HumanOperation::Insert | HumanOperation::InsertWithChildren => {
            let after_path = entry
                .after_path
                .as_ref()
                .context("Insert entry is missing after_path")?;
            let after_node = after_cache
                .resolve(after_root, &path_refs(after_path))
                .with_context(|| format!("resolving after_path {:?}", after_path))?;

            if entry.operation == HumanOperation::InsertWithChildren {
                check_subtree_maps_to_zero(
                    after_node,
                    &diff_ast.after_node_map,
                    &format!("Insert (with children) {:?}", after_path),
                    mismatches,
                    before_root,
                );
            } else {
                let actual = diff_ast.after_node_map.get(&after_node.id()).copied();
                if actual != Some(0) {
                    let mapped_kind = match actual {
                        Some(mapped_id) => node_kind_for_id(before_root, mapped_id),
                        None => "None".to_string(),
                    };
                    mismatches.push(format!(
                        "Insert {:?}: expected after node '{}' to be new (mapped to 0), but it mapped to {}{}",
                        after_path,
                        after_node.kind(),
                        mapped_kind,
                        actual_mapping_info_after(diff_ast, after_node.id(), actual)
                    ));
                }
            }
        }
    }

    Ok(())
}

/// One `diff_code` run's mapping, keyed by node *path* rather than node ID.
///
/// Node IDs are tree-sitter arena slots: stable within one parse, but not across separate parses
/// of identical source (allocator/arena layout can differ run to run, even within the same
/// process). A determinism check that reuses a single parse for every run can't see that class of
/// bug at all - both runs would agree on IDs trivially. Keying by path (derived purely from node
/// kind and sibling position, see [`super::path_for_node`]) makes two independently-parsed runs
/// directly comparable.
type PathKeyedMapping = HashMap<(Vec<String>, Vec<String>), ASTMappingOperation>;

/// Runs `diff_code_with_config` on a *fresh* parse of `before_source`/`after_source` and returns
/// its mapping keyed by path. Parsing fresh (rather than reusing an already-parsed `Code`) is the
/// point: it's what actually reproduces the arena-layout variation a separate process launch
/// would see. See [`crate::diff::HeuristicConfig`] for what `config` is for.
fn diff_paths_with_config(
    before_source: &str,
    after_source: &str,
    language: &crate::code::Language,
    config: &crate::diff::HeuristicConfig,
) -> PathKeyedMapping {
    let before = crate::code::Code::from_string(before_source, language);
    let after = crate::code::Code::from_string(after_source, language);
    let diff = crate::diff::diff_code_with_config(&before, &after, config);
    let node_cache = NodeCache::build(&before, &after);
    let diff_ast = diff.ast.expect("Diff has no AST");

    // One cache per side, reused across every mapping entry below - see `PathCache`'s own doc
    // comment for why a fresh `path_for_node` per entry would be quadratic here: this walks
    // *every* mapped node in the whole diff (not just human-annotated ones), and for a large flat
    // container (e.g. a big JSON object) many thousands of them share the same huge-fanout parent.
    let mut before_cache = PathCache::new();
    let mut after_cache = PathCache::new();

    diff_ast
        .mapping
        .iter()
        .filter_map(|(&(b, a), m)| {
            let before_path = before_cache.path_of(*node_cache.before.get(&b)?);
            let after_path = after_cache.path_of(*node_cache.after.get(&a)?);
            Some(((before_path, after_path), m.operation.clone()))
        })
        .collect()
}

/// Compares two path-keyed mappings and describes every pair whose presence or
/// `ASTMappingOperation` differs between them - i.e. every sign that `diff_code` is not a pure
/// function of its inputs. Empty when the runs fully agree.
fn describe_path_map_differences(
    run_number: usize,
    baseline: &PathKeyedMapping,
    repeat: &PathKeyedMapping,
) -> Vec<String> {
    let mut keys: Vec<&(Vec<String>, Vec<String>)> = baseline.keys().chain(repeat.keys()).collect();
    keys.sort_unstable();
    keys.dedup();

    keys.into_iter()
        .filter_map(|key| {
            let base_entry = baseline.get(key);
            let repeat_entry = repeat.get(key);
            if base_entry == repeat_entry {
                return None;
            }
            let describe = |entry: Option<&ASTMappingOperation>| match entry {
                Some(op) => format!("{op:?}"),
                None => "unmapped".to_string(),
            };
            Some(format!(
                "Non-deterministic diff across independent parses: run 1 and run {run_number} disagree on {:?} <-> {:?}: {} vs {}",
                key.0,
                key.1,
                describe(base_entry),
                describe(repeat_entry),
            ))
        })
        .collect()
}

/// Compares three independently-parsed `diff_code` runs of the same before/after source and
/// describes every point of disagreement (empty if all three fully agree).
#[cfg(test)]
fn describe_nondeterminism(
    before_source: &str,
    after_source: &str,
    language: &crate::code::Language,
) -> Vec<String> {
    describe_nondeterminism_with_config(
        before_source,
        after_source,
        language,
        &crate::diff::HeuristicConfig::default(),
    )
}

/// Same as [`describe_nondeterminism`], but computes each of the three independent runs via
/// [`diff_paths_with_config`] - see [`crate::diff::HeuristicConfig`] for what `config` is for.
fn describe_nondeterminism_with_config(
    before_source: &str,
    after_source: &str,
    language: &crate::code::Language,
    config: &crate::diff::HeuristicConfig,
) -> Vec<String> {
    let baseline = diff_paths_with_config(before_source, after_source, language, config);
    let mut mismatches = Vec::new();
    for run_number in 2..=3 {
        let repeat = diff_paths_with_config(before_source, after_source, language, config);
        mismatches.extend(describe_path_map_differences(
            run_number, &baseline, &repeat,
        ));
    }
    mismatches
}

/**
* Loads the human mapping for `name`, computes codediff's own diff for the same test case, and
* returns every point of disagreement between the two (empty if they fully agree).
*
* Also re-parses the before/after source two more times from scratch and re-diffs, comparing all
* three results by node *path* (not ID - see [`describe_nondeterminism`]) against each other:
* `diff_code` is supposed to be a pure function of its source text, so any difference between
* independently-parsed runs means some pass is relying on something other than the source text
* (e.g. an unordered `HashMap`/`HashSet` iteration, or a tree-sitter arena node ID used as a sort
* key) to pick a winner - which would otherwise silently make every mismatch count in this suite,
* and in `benchmark_optimal_solutions` (which shares this function), unreliable from run to run.
*
* Shared by `assert_matches_human_mapping` (which just turns a non-empty result into a test
* failure) and the `benchmark_optimal_solutions` binary (which wants the raw count across every
* fixture, not a single pass/fail).
*/
pub fn compute_mismatches(name: &str) -> Result<Vec<String>> {
    compute_mismatches_with_config(name, &crate::diff::HeuristicConfig::default())
}

/// Same as [`compute_mismatches`], but forwards `config` to [`compute_mismatches_for_with_config`]
/// - see [`crate::diff::HeuristicConfig`] for what it's for. Used by
///   `benchmark_optimal_solutions --details --no-solver-X`.
pub fn compute_mismatches_with_config(
    name: &str,
    config: &crate::diff::HeuristicConfig,
) -> Result<Vec<String>> {
    let test_diffs = crate::test::helper::handmade_test_code_pairs()?;
    let (before, after) = test_diffs
        .get(name)
        .with_context(|| format!("No before/after test code pair found for '{}'", name))?;
    compute_mismatches_for_with_config(name, before, after, config)
}

/**
* Total number of AST nodes across both `before` and `after` - the denominator
* `benchmark_optimal_solutions` uses to turn a fixture's absolute mismatch count into a relative
* percentage, so a 3-mismatch fixture with 20 nodes and a 3-mismatch fixture with 2000 nodes don't
* read as equally bad.
*/
pub fn total_node_count_for(before: &crate::code::Code, after: &crate::code::Code) -> usize {
    let node_cache = NodeCache::build(before, after);
    node_cache.before.len() + node_cache.after.len()
}

/**
* Same as [`compute_mismatches`], but takes an already-loaded before/after pair instead of looking
* it up in a freshly-fetched `handmade_test_code_pairs()` map.
*
* Callers that check many fixtures in a loop (e.g. `benchmark_optimal_solutions`) should load the
* map once and call this directly with a borrowed pair, rather than going through
* `compute_mismatches` once per fixture - `handmade_test_code_pairs()` is memoized, but every call
* still clones the *entire* map to hand back an owned one, which is O(fixture count) work just to
* reach a single entry.
*/
pub fn compute_mismatches_for(
    name: &str,
    before: &crate::code::Code,
    after: &crate::code::Code,
) -> Result<Vec<String>> {
    compute_mismatches_for_with_config(
        name,
        before,
        after,
        &crate::diff::HeuristicConfig::default(),
    )
}

/// Same as [`compute_mismatches_for`], but computes codediff's diff (and its determinism check)
/// via [`crate::diff::diff_code_with_config`] - see [`crate::diff::HeuristicConfig`] for what
/// `config` is for. Used by `benchmark_optimal_solutions --no-solver-X`'s ablation study.
pub fn compute_mismatches_for_with_config(
    name: &str,
    before: &crate::code::Code,
    after: &crate::code::Code,
    config: &crate::diff::HeuristicConfig,
) -> Result<Vec<String>> {
    let mapping = load(name)?;

    let diff = crate::diff::diff_code_with_config(before, after, config);
    let diff_ast = diff.ast.context("Diff has no AST")?;

    let node_cache = NodeCache::build(before, after);
    let language = before.metadata.language.unwrap_or_default();
    let mut mismatches =
        describe_nondeterminism_with_config(&before.contents, &after.contents, &language, config);

    // Check that the produced diff is valid
    if !diff_ast.is_valid(before, after, &node_cache) {
        mismatches
            .push("The produced diff is not valid according to ASTDiff::is_valid".to_string());
    }

    let before_ast = before.ast.as_ref().context("Before code has no AST")?;
    let after_ast = after.ast.as_ref().context("After code has no AST")?;
    let before_root = before_ast.root_node();
    let after_root = after_ast.root_node();

    let mut before_cache = PathCache::new();
    let mut after_cache = PathCache::new();
    for entry in &mapping.entries {
        check_entry(
            entry,
            before_root,
            after_root,
            &diff_ast,
            &mut mismatches,
            &mut before_cache,
            &mut after_cache,
        )?;
    }

    Ok(mismatches)
}

/**
* Loads the human mapping for `name`, computes codediff's own diff for the same test case, and
* checks that every human-authored decision holds in codediff's output.
*
* This is the whole body of the generated `optimal_solutions/<name>.rs` tests: `human_solver`
* writes a human_mapping.json file, and each of those tests just calls this function. Reports every
* mismatch at once (rather than failing on the first one), since the point of these tests is to see
* the full extent of any disagreement between codediff and the human-authored optimum.
*/
pub fn assert_matches_human_mapping(name: &str) -> Result<()> {
    assert_matches_human_mapping_within_limit(name, 0)
}

/**
* Same as [`assert_matches_human_mapping`], but allows up to `upper_limit_of_mismatched_nodes`
* mismatches instead of requiring an exact match.
*
* For fixtures where codediff's mapping has a known, understood gap against the human-authored
* mapping (documented in `TODO.md` - an objective-wall gap, a premature-pruning architecture
* issue, etc. - not a bug that's simply unfixed), pin the limit to today's actual mismatch count.
* The test still catches *regressions* (an increase past the limit) without blocking the suite on
* a fix that doesn't exist yet. When a fix does land for one of these gaps, lower the limit (or
* switch back to [`assert_matches_human_mapping`] if it reaches 0) so the test keeps the new bar.
*/
pub fn assert_matches_human_mapping_within_limit(
    name: &str,
    upper_limit_of_mismatched_nodes: usize,
) -> Result<()> {
    let mismatches = compute_mismatches(name)?;

    if mismatches.len() > upper_limit_of_mismatched_nodes {
        bail!(
            "{} mismatch(es) between the human mapping and codediff's diff for '{}' (allowed up to {}):\n{}",
            mismatches.len(),
            name,
            upper_limit_of_mismatched_nodes,
            mismatches.join("\n")
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::Language;
    use crate::test::helper::path_for_node;

    #[test]
    fn round_trips_through_json() -> Result<()> {
        let mapping = HumanMapping {
            entries: vec![
                HumanMappingEntry {
                    operation: HumanOperation::Identical,
                    before_path: Some(vec!["function_item:1".to_string()]),
                    after_path: Some(vec!["function_item:1".to_string()]),
                },
                HumanMappingEntry {
                    operation: HumanOperation::DeleteWithChildren,
                    before_path: Some(vec!["function_item:1".to_string(), "block:1".to_string()]),
                    after_path: None,
                },
            ],
        };

        let json = serde_json::to_string_pretty(&mapping)?;
        let round_tripped: HumanMapping = serde_json::from_str(&json)?;

        assert_eq!(round_tripped.entries.len(), 2);
        assert_eq!(
            round_tripped.entries[0].operation,
            HumanOperation::Identical
        );
        assert!(round_tripped.entries[0].after_path.is_some());
        assert_eq!(
            round_tripped.entries[1].operation,
            HumanOperation::DeleteWithChildren
        );
        assert!(round_tripped.entries[1].after_path.is_none());

        Ok(())
    }

    #[test]
    fn rebuild_caches_distinguishes_identical_from_update_and_match_but_not_identical() -> Result<()>
    {
        let source = "fn f() {\n    let a = 1;\n    let b = 2;\n    let c = 3;\n}\n";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&crate::code::language::to_treesitter(&Language::Rust).unwrap())
            .unwrap();
        let before_tree = parser.parse(source, None).unwrap();
        let after_tree = parser.parse(source, None).unwrap();
        let before_root = before_tree.root_node();
        let after_root = after_tree.root_node();

        let function_item = before_root.child(0).unwrap();
        let block = {
            let mut c = function_item.walk();
            function_item
                .children(&mut c)
                .find(|n| n.kind() == "block")
                .unwrap()
        };
        let mut stmt_cursor = block.walk();
        let statements: Vec<Node> = block
            .children(&mut stmt_cursor)
            .filter(|n| n.kind() == "let_declaration")
            .collect();
        let identical_stmt = statements[0];
        let update_stmt = statements[1];
        let match_but_not_identical_stmt = statements[2];

        let after_function_item = after_root.child(0).unwrap();
        let after_block = {
            let mut c = after_function_item.walk();
            after_function_item
                .children(&mut c)
                .find(|n| n.kind() == "block")
                .unwrap()
        };
        let mut after_stmt_cursor = after_block.walk();
        let after_statements: Vec<Node> = after_block
            .children(&mut after_stmt_cursor)
            .filter(|n| n.kind() == "let_declaration")
            .collect();

        let entries = vec![
            HumanMappingEntry {
                operation: HumanOperation::Identical,
                before_path: Some(path_for_node(identical_stmt)),
                after_path: Some(path_for_node(after_statements[0])),
            },
            HumanMappingEntry {
                operation: HumanOperation::Update,
                before_path: Some(path_for_node(update_stmt)),
                after_path: Some(path_for_node(after_statements[1])),
            },
            HumanMappingEntry {
                operation: HumanOperation::MatchButNotIdentical,
                before_path: Some(path_for_node(match_but_not_identical_stmt)),
                after_path: Some(path_for_node(after_statements[2])),
            },
        ];

        let caches = rebuild_caches(&entries, before_root, after_root);

        assert!(is_identical_before(identical_stmt, &caches));
        assert!(is_identical_after(after_statements[0], &caches));
        assert!(!is_identical_before(update_stmt, &caches));
        assert!(!is_identical_after(after_statements[1], &caches));
        assert!(!is_identical_before(match_but_not_identical_stmt, &caches));
        assert!(!is_identical_after(after_statements[2], &caches));
        // A node with no entry at all defaults to identical (matches matched nodes' pre-existing
        // quiet/undecorated rendering when a `Caches` is built by hand rather than via
        // `rebuild_caches`).
        assert!(is_identical_before(before_root, &caches));

        Ok(())
    }

    #[test]
    fn detects_a_correct_hand_written_mapping_for_rust_no_change() -> Result<()> {
        // rust-no-change is fully identical before/after, so every node should match itself.
        let test_diffs = crate::test::helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs.get("rust-no-change").unwrap().clone();

        let before_ast = before.ast.as_ref().unwrap();
        let root = before_ast.root_node();

        // Build an Identical entry for the root: since before == after, before_root and
        // after_root are the same path ("source_file:1"), and codediff should have hashed the
        // whole tree as an identical match.
        let entries = vec![HumanMappingEntry {
            operation: HumanOperation::Identical,
            before_path: Some(path_for_node(root)),
            after_path: Some(path_for_node(root)),
        }];

        let mapping = HumanMapping { entries };

        let diff = crate::diff::diff_code(&before, &after);
        let diff_ast = diff.ast.unwrap();
        let after_ast = after.ast.as_ref().unwrap();

        let mut mismatches = Vec::new();
        let mut before_cache = PathCache::new();
        let mut after_cache = PathCache::new();
        for entry in &mapping.entries {
            check_entry(
                entry,
                root,
                after_ast.root_node(),
                &diff_ast,
                &mut mismatches,
                &mut before_cache,
                &mut after_cache,
            )?;
        }

        assert!(mismatches.is_empty(), "{:?}", mismatches);

        Ok(())
    }

    #[test]
    fn detects_an_incorrect_hand_written_mapping() -> Result<()> {
        // Deliberately claim the root is deleted, which is false for rust-no-change.
        let test_diffs = crate::test::helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs.get("rust-no-change").unwrap().clone();

        let before_ast = before.ast.as_ref().unwrap();
        let root = before_ast.root_node();

        let entries = vec![HumanMappingEntry {
            operation: HumanOperation::DeleteWithChildren,
            before_path: Some(path_for_node(root)),
            after_path: None,
        }];

        let mapping = HumanMapping { entries };

        let diff = crate::diff::diff_code(&before, &after);
        let diff_ast = diff.ast.unwrap();
        let after_ast = after.ast.as_ref().unwrap();

        let mut mismatches = Vec::new();
        let mut before_cache = PathCache::new();
        let mut after_cache = PathCache::new();
        for entry in &mapping.entries {
            check_entry(
                entry,
                root,
                after_ast.root_node(),
                &diff_ast,
                &mut mismatches,
                &mut before_cache,
                &mut after_cache,
            )?;
        }

        assert!(!mismatches.is_empty());

        Ok(())
    }

    fn path_map(entries: &[((&str, &str), ASTMappingOperation)]) -> PathKeyedMapping {
        entries
            .iter()
            .map(|((b, a), op)| ((vec![b.to_string()], vec![a.to_string()]), op.clone()))
            .collect()
    }

    #[test]
    fn describe_path_map_differences_is_empty_when_runs_agree() {
        let baseline = path_map(&[(("b", "a"), ASTMappingOperation::Identical)]);
        let repeat = baseline.clone();

        assert!(describe_path_map_differences(2, &baseline, &repeat).is_empty());
    }

    #[test]
    fn describe_path_map_differences_reports_a_differing_operation() {
        let baseline = path_map(&[(("b", "a"), ASTMappingOperation::Identical)]);
        let repeat = path_map(&[(("b", "a"), ASTMappingOperation::Update)]);

        let report = describe_path_map_differences(3, &baseline, &repeat);
        assert_eq!(report.len(), 1, "{report:?}");
        assert!(report[0].contains("run 1 and run 3"), "{}", report[0]);
        assert!(report[0].contains("Identical"), "{}", report[0]);
        assert!(report[0].contains("Update"), "{}", report[0]);
    }

    #[test]
    fn describe_path_map_differences_reports_a_pair_missing_from_one_run() {
        let baseline = path_map(&[(("b", "a"), ASTMappingOperation::Identical)]);
        let repeat = PathKeyedMapping::new();

        let report = describe_path_map_differences(2, &baseline, &repeat);
        assert_eq!(report.len(), 1, "{report:?}");
        assert!(report[0].contains("unmapped"), "{}", report[0]);
    }

    /// End-to-end sanity check for `describe_nondeterminism` itself (not just the pure
    /// comparator): identical source parsed three independent times must fully agree.
    #[test]
    fn describe_nondeterminism_is_empty_for_stable_source() {
        let report =
            describe_nondeterminism("fn f() { 1 + 1; }", "fn f() { 1 + 1; }", &Language::Rust);
        assert!(report.is_empty(), "{report:?}");
    }
}
