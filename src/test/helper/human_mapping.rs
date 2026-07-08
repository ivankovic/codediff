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

use crate::diff::{ASTDiff, ASTMappingOperation, NodeCache};
use crate::test::helper::{node_for_path, path_for_node};

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
    node_map: &HashMap<usize, usize>,
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

fn check_entry(
    entry: &HumanMappingEntry,
    before_root: Node,
    after_root: Node,
    diff_ast: &ASTDiff,
    mismatches: &mut Vec<String>,
) -> Result<()> {
    match entry.operation {
        HumanOperation::Identical | HumanOperation::Update | HumanOperation::MatchButNotIdentical => {
            let before_path = entry
                .before_path
                .as_ref()
                .with_context(|| format!("{:?} entry is missing before_path", entry.operation))?;
            let after_path = entry
                .after_path
                .as_ref()
                .with_context(|| format!("{:?} entry is missing after_path", entry.operation))?;

            let before_node = node_for_path(before_root, &path_refs(before_path))
                .with_context(|| format!("resolving before_path {:?}", before_path))?;
            let after_node = node_for_path(after_root, &path_refs(after_path))
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

            let expected_op = expected_ast_operation(entry.operation)
                .expect("Identical/Update/MatchButNotIdentical always have an expected ASTMappingOperation");
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
            let before_node = node_for_path(before_root, &path_refs(before_path))
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
            let after_node = node_for_path(after_root, &path_refs(after_path))
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

/// Runs `diff_code` on a *fresh* parse of `before_source`/`after_source` and returns its mapping
/// keyed by path. Parsing fresh (rather than reusing an already-parsed `Code`) is the point: it's
/// what actually reproduces the arena-layout variation a separate process launch would see.
fn diff_paths(before_source: &str, after_source: &str, language: &crate::code::Language) -> PathKeyedMapping {
    let before = crate::code::Code::from_string(before_source, language);
    let after = crate::code::Code::from_string(after_source, language);
    let diff = crate::diff::diff_code(&before, &after);
    let node_cache = NodeCache::build(&before, &after);
    let diff_ast = diff.ast.expect("Diff has no AST");

    diff_ast
        .mapping
        .iter()
        .filter_map(|(&(b, a), m)| {
            let before_path = path_for_node(*node_cache.before.get(&b)?);
            let after_path = path_for_node(*node_cache.after.get(&a)?);
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
fn describe_nondeterminism(
    before_source: &str,
    after_source: &str,
    language: &crate::code::Language,
) -> Vec<String> {
    let baseline = diff_paths(before_source, after_source, language);
    let mut mismatches = Vec::new();
    for run_number in 2..=3 {
        let repeat = diff_paths(before_source, after_source, language);
        mismatches.extend(describe_path_map_differences(run_number, &baseline, &repeat));
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
    let mapping = load(name)?;

    let test_diffs = crate::test::helper::handmade_test_code_pairs()?;
    let (before, after) = test_diffs
        .get(name)
        .with_context(|| format!("No before/after test code pair found for '{}'", name))?
        .clone();

    let diff = crate::diff::diff_code(&before, &after);
    let diff_ast = diff.ast.context("Diff has no AST")?;

    let node_cache = NodeCache::build(&before, &after);
    let language = before.metadata.language.unwrap_or_default();
    let mut mismatches = describe_nondeterminism(&before.contents, &after.contents, &language);

    // Check that the produced diff is valid
    if !diff_ast.is_valid(&before, &after, &node_cache) {
        mismatches.push("The produced diff is not valid according to ASTDiff::is_valid".to_string());
    }

    let before_ast = before.ast.context("Before code has no AST")?;
    let after_ast = after.ast.context("After code has no AST")?;
    let before_root = before_ast.root_node();
    let after_root = after_ast.root_node();

    for entry in &mapping.entries {
        check_entry(entry, before_root, after_root, &diff_ast, &mut mismatches)?;
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
    let mismatches = compute_mismatches(name)?;

    if !mismatches.is_empty() {
        bail!(
            "{} mismatch(es) between the human mapping and codediff's diff for '{}':\n{}",
            mismatches.len(),
            name,
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
        assert_eq!(round_tripped.entries[0].operation, HumanOperation::Identical);
        assert!(round_tripped.entries[0].after_path.is_some());
        assert_eq!(
            round_tripped.entries[1].operation,
            HumanOperation::DeleteWithChildren
        );
        assert!(round_tripped.entries[1].after_path.is_none());

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
        for entry in &mapping.entries {
            check_entry(
                entry,
                root,
                after_ast.root_node(),
                &diff_ast,
                &mut mismatches,
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
        for entry in &mapping.entries {
            check_entry(
                entry,
                root,
                after_ast.root_node(),
                &diff_ast,
                &mut mismatches,
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
        let report = describe_nondeterminism("fn f() { 1 + 1; }", "fn f() { 1 + 1; }", &Language::Rust);
        assert!(report.is_empty(), "{report:?}");
    }
}
