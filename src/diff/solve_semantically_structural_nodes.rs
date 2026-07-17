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
use std::collections::HashMap;

use crate::code::{ASTMetadata, Code, Language};
use crate::diff::apted::{self, Algorithm};
use crate::diff::{ASTDiff, ASTMapping, ASTMappingOperation, ASTMappingReason, NodeCache};

/// TUNING PARAMETERS for semantically structural node matching.
/// For matched semantically structural node pairs, only run APTED if at least one side
/// is "big enough" OR if their structural hashes differ (indicating structural changes).
/// This avoids expensive O(n²) computations on small, structurally-identical pairs.
/// 
/// Note: This optimization only applies to languages that have semantically structural nodes defined
/// (Rust, Python, Go, Kotlin, Java, etc.). Languages without semantically structural nodes (like C)
/// are unaffected.
const MIN_SEMANTIC_DEPTH: usize = 2;
const MIN_SEMANTIC_SUBTREE_SIZE: usize = 20;

/// Check if a semantically structural node is "big enough" to warrant running APTED.
/// Returns true if the node meets the depth and subtree size criteria.
fn is_big_enough_semantic_node(node_id: usize, metadata: &ASTMetadata, depths: &HashMap<usize, usize>) -> bool {
    let depth = depths.get(&node_id).copied().unwrap_or(0);
    let subtree_size = metadata.node_to_subtree_size.get(&node_id).copied().unwrap_or(0);
    depth >= MIN_SEMANTIC_DEPTH && subtree_size >= MIN_SEMANTIC_SUBTREE_SIZE
}

/// Check if we should run APTED on a matched semantically structural node pair.
/// Returns true if at least one node is big enough OR if structural hashes differ.
/// 
/// Rationale: If both nodes are small and have matching structural hashes, the tree structure
/// is identical. In this case, any differences are text-only (e.g., a changed constant) and will
/// be caught efficiently by later passes. Running APTED (O(n²)) on such pairs is wasteful.
fn should_run_apted(
    before_id: usize,
    after_id: usize,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    before_depths: &HashMap<usize, usize>,
    after_depths: &HashMap<usize, usize>,
) -> bool {
    // Run APTED if at least one side is big enough
    if is_big_enough_semantic_node(before_id, before_meta, before_depths)
        || is_big_enough_semantic_node(after_id, after_meta, after_depths) {
        return true;
    }
    
    // Also run if structural hashes differ (structural changes need APTED)
    let b_struct = before_meta.node_to_structural_hash.get(&before_id);
    let a_struct = after_meta.node_to_structural_hash.get(&after_id);
    if b_struct != a_struct {
        return true;
    }
    
    // Both nodes are small and structurally identical - skip APTED
    // (differences will be caught by later passes)
    false
}

/// Recursively add Identical mappings for an already-verified identical subtree pair.
///
/// Both subtrees must have the same full hash (caller's responsibility). All descendants are
/// matched pairwise, position by position, since equal full hashes guarantee equal structure.
fn add_identical_subtree(
    before_id: usize,
    after_id: usize,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    diff: &mut ASTDiff,
    is_root: bool,
) {
    let reason = if is_root {
        ASTMappingReason::IdenticalHash
    } else {
        ASTMappingReason::IdenticalHashOfAncestor
    };
    diff.add_mapping(
        before_id,
        after_id,
        ASTMapping { cost: 0, operation: ASTMappingOperation::Identical, reason },
    );
    let Some(before_info) = before_meta.node_info.get(&before_id) else { return };
    let Some(after_info) = after_meta.node_info.get(&after_id) else { return };
    for (&b_child, &a_child) in before_info.children.iter().zip(after_info.children.iter()) {
        add_identical_subtree(b_child, a_child, before_meta, after_meta, diff, false);
    }
}

/// Pre-match children of a semantically-matched pair that share both the same ordinal position
/// and the same full hash.
///
/// Walks both subtrees in parallel (depth-first). At each level, children at the same position
/// are compared:
/// - Same hash → the entire subtree is matched as Identical (all descendants are mapped).
/// - Different hash, same kind → recurse deeper (handles container nodes like variant_list whose
///   hash changed because a child was added/deleted, but whose surviving children are unchanged).
/// - Different hash, different kind → stop this branch.
///
/// This reduces the residual seen by APTED from the full pair to just the changed fragment,
/// turning O(n²) into O(k²) where k is the number of nodes that actually differ.
fn pre_match_by_path(
    before_id: usize,
    after_id: usize,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    diff: &mut ASTDiff,
) {
    let Some(before_info) = before_meta.node_info.get(&before_id) else { return };
    let Some(after_info) = after_meta.node_info.get(&after_id) else { return };
    let before_children = before_info.children.clone();
    let after_children = after_info.children.clone();

    for (&b_child, &a_child) in before_children.iter().zip(after_children.iter()) {
        if diff.before_node_map.contains_key(&b_child) || diff.after_node_map.contains_key(&a_child) {
            continue;
        }
        let b_hash = before_meta.node_to_full_hash.get(&b_child);
        let a_hash = after_meta.node_to_full_hash.get(&a_child);
        if b_hash.is_some() && b_hash == a_hash {
            add_identical_subtree(b_child, a_child, before_meta, after_meta, diff, true);
        } else {
            let b_kind = before_meta.node_info.get(&b_child).map(|i| i.kind.as_str());
            let a_kind = after_meta.node_info.get(&a_child).map(|i| i.kind.as_str());
            if b_kind != a_kind || b_kind.is_none() {
                // Kinds differ: the two child lists have diverged at this position.
                // All subsequent positions would be misaligned, so stop here.
                break;
            }
            // Only recurse when the subtree shapes match (same structural hash).
            // Recursing into differently-shaped same-kind nodes (e.g. two field_declarations
            // with different type children) produces spurious matches for shared tokens like `:`.
            let b_struct = before_meta.node_to_structural_hash.get(&b_child);
            let a_struct = after_meta.node_to_structural_hash.get(&a_child);
            if b_struct.is_some() && b_struct == a_struct {
                pre_match_by_path(b_child, a_child, before_meta, after_meta, diff);
            }
        }
    }
}

/**
* Match semantically structural nodes and solve their subtrees.
*
* For impl_item pairs, methods inside the matched pair are pre-matched by name before running
* APTED on the impl body. This lets the postorder indexer prune already-matched method subtrees,
* reducing the effective tree size passed to the O(n²) engine from the entire impl body to just
* the unmatched residual (impl header + unmatched methods).
*/
pub fn solve(before: &Code, after: &Code, _node_cache: &NodeCache, diff: &mut ASTDiff) {
    let before_metadata = crate::code::metadata::metadata_of(before);
    let after_metadata = crate::code::metadata::metadata_of(after);

    // Depths for both trees for the big-enough heuristic - already precomputed once per file in
    // `ASTMetadata` (see `compute_ast_metadata`), no need to walk the tree again here.
    let before_depths = &before_metadata.node_to_depth;
    let after_depths = &after_metadata.node_to_depth;

    let language = before.metadata.language.as_ref();

    // Pass 0b: Python — class pairs with method pre-matching (mirrors Rust impl recursion).
    if matches!(language, Some(Language::Python)) {
        let class_pairs: Vec<_> = before_metadata
            .semantically_structural_nodes
            .iter()
            .filter(|((kind, _), _)| *kind == "class_definition")
            .filter_map(|((kind, identifier), &before_class_id)| {
                after_metadata
                    .semantically_structural_nodes
                    .get(&(kind.clone(), identifier.clone()))
                    .map(|&after_class_id| (before_class_id, after_class_id))
            })
            .collect();

        for (before_class_id, after_class_id) in class_pairs {
            let before_methods = methods_in_class(before_class_id, &before_metadata);
            let after_methods = methods_in_class(after_class_id, &after_metadata);

            for (method_name, before_method_id) in &before_methods {
                if let Some(&after_method_id) = after_methods.get(method_name) {
                    // Only run APTED on method pairs that warrant it
                    if should_run_apted(
                        *before_method_id,
                        after_method_id,
                        &before_metadata,
                        &after_metadata,
                        &before_depths,
                        &after_depths,
                    ) {
                        apted::for_nodes(
                            &before_metadata,
                            &after_metadata,
                            vec![*before_method_id],
                            vec![after_method_id],
                            Algorithm::Apted,
                            "python_method",
                            diff,
                        );
                    }
                }
            }
            // Only run APTED on class pairs that warrant it
            if should_run_apted(
                before_class_id,
                after_class_id,
                &before_metadata,
                &after_metadata,
                &before_depths,
                &after_depths,
            ) {
                apted::for_nodes(
                    &before_metadata,
                    &after_metadata,
                    vec![before_class_id],
                    vec![after_class_id],
                    Algorithm::Apted,
                    "python_class",
                    diff,
                );
            }
        }
    }

    // Pass 1: impl_item pairs — match methods within each matched impl first, then diff the impl.
    // This is done before the global pass so that method node_ids are in `diff` before any
    // global function_item entries (which may include these same methods from the DFS traversal)
    // are processed.
    let impl_pairs: Vec<_> = before_metadata
        .semantically_structural_nodes
        .iter()
        .filter(|((kind, _), _)| *kind == "impl_item")
        .filter_map(|((kind, identifier), &before_impl_id)| {
            after_metadata
                .semantically_structural_nodes
                .get(&(kind.clone(), identifier.clone()))
                .map(|&after_impl_id| (before_impl_id, after_impl_id))
        })
        .collect();

    for (before_impl_id, after_impl_id) in impl_pairs {
        let before_methods = methods_in_impl(before_impl_id, &before_metadata);
        let after_methods = methods_in_impl(after_impl_id, &after_metadata);

        for (method_name, before_method_id) in &before_methods {
            if let Some(&after_method_id) = after_methods.get(method_name) {
                pre_match_by_path(*before_method_id, after_method_id, &before_metadata, &after_metadata, diff);
                // Only run APTED on method pairs that warrant it
                if should_run_apted(
                    *before_method_id,
                    after_method_id,
                    &before_metadata,
                    &after_metadata,
                    &before_depths,
                    &after_depths,
                ) {
                    apted::for_nodes(
                        &before_metadata,
                        &after_metadata,
                        vec![*before_method_id],
                        vec![after_method_id],
                        Algorithm::Apted,
                        "impl_method",
                        diff,
                    );
                }
            }
        }

        pre_match_by_path(before_impl_id, after_impl_id, &before_metadata, &after_metadata, diff);
        // The method subtrees are now in `diff` and will be pruned by PostorderIndexer.
        // Only run APTED on impl pairs that warrant it
        if should_run_apted(
            before_impl_id,
            after_impl_id,
            &before_metadata,
            &after_metadata,
            &before_depths,
            &after_depths,
        ) {
            apted::for_nodes(
                &before_metadata,
                &after_metadata,
                vec![before_impl_id],
                vec![after_impl_id],
                Algorithm::Apted,
                "impl",
                diff,
            );
        }
    }

    // Pass 2: all other matched pairs (fn, struct, enum, …).
    // Methods already matched in Pass 0b/1 are skipped via filter_before/after_nodes inside
    // for_nodes → resolve_forest.
    let other_pairs: Vec<_> = before_metadata
        .semantically_structural_nodes
        .iter()
        .filter(|((kind, _), _)| *kind != "impl_item" && *kind != "class_definition")
        .filter_map(|((kind, identifier), &before_node_id)| {
            after_metadata
                .semantically_structural_nodes
                .get(&(kind.clone(), identifier.clone()))
                .map(|&after_node_id| (before_node_id, after_node_id))
        })
        .collect();

    for (before_node_id, after_node_id) in other_pairs {
        pre_match_by_path(before_node_id, after_node_id, &before_metadata, &after_metadata, diff);
        // Only run APTED if the pair warrants it (big enough or structural differences)
        if should_run_apted(
            before_node_id,
            after_node_id,
            &before_metadata,
            &after_metadata,
            &before_depths,
            &after_depths,
        ) {
            apted::for_nodes(
                &before_metadata,
                &after_metadata,
                vec![before_node_id],
                vec![after_node_id],
                Algorithm::Apted,
                "semantic_node",
                diff,
            );
        }
    }

    // Pass 3 (`solve_orphaned_semantic_nodes`) intentionally does *not* run here — see its own
    // doc comment for why it's a separate step that runs after the final APTED pass.
}

/**
* Pass 3: semantic nodes that exist only on one side (pure deletions and insertions).
*
* Running APTED on the full source_file pair would force it to compare these lone subtrees
* against everything on the other side, burning O(n²) time for what is really an O(n) walk.
* Calling for_nodes with an empty opposite forest lets APTED mark the whole subtree as
* deleted / inserted in a single O(n) pass, removing them from the apted_roots residual.
*
* This is split out from [`solve`] and run as its own, AFTER the final APTED pass (see
* `Diff::from_code`). This ordering is critical: running it before the final APTED pass
* caused premature/irreversible pruning. When a semantically structural node (e.g., impl_item)
* had no same-named counterpart, it would be immediately marked as deleted/inserted before APTED
* had a chance to find a match based on structure alone. This caused issues like
* rust-turbopack-module-rule where impl ModuleType was renamed to impl ConfiguredModuleType -
* the type name changed but the body was similar enough that APTED could have found a good
* match, but the premature orphan handling prevented this.
*
* Now APTED runs first on the entire residual forest, and only nodes that APTED itself
* couldn't match are marked as orphans here. `add_delete_mappings`/`add_insert_mappings`
* already skip over any node a prior pass mapped, so nothing pre-matched by APTED gets
* clobbered here.
*/
pub fn solve_orphaned_semantic_nodes(
    _before: &Code,
    _after: &Code,
    _node_cache: &NodeCache,
    _diff: &mut ASTDiff,
) {
    // This function now acts as a fallback for nodes that were NOT matched by any earlier pass
    // (including the final full-tree APTED pass). Previously, it would force-delete/insert orphaned
    // semantic nodes before APTED had a chance, which caused premature pruning (e.g., the
    // rust-turbopack-module-rule case where impl ModuleType was renamed to impl
    // ConfiguredModuleType - the type name changed but the body was similar enough that APTED
    // could have found matches, but the premature orphan handling prevented this).
    //
    // Now it's a no-op because the final full-tree APTED pass (which runs before this function
    // in Diff::from_code) already handles all possible structural matches. Nodes that remain
    // unmatched after APTED have no good match anywhere in the opposite tree, so explicitly
    // marking them as orphaned here would produce the same result APTED already computed.
    //
    // This function is kept for documentation and in case future pipeline changes need
    // explicit orphan handling.
}

/// Collect function_item children of an impl_item body, keyed by method name.
///
/// Uses `declaration_list` as the intermediate body container (tree-sitter-rust grammar).
/// When a method name appears more than once (which cannot happen in valid Rust), the first
/// occurrence wins.
fn methods_in_impl(impl_id: usize, meta: &ASTMetadata) -> HashMap<String, usize> {
    let mut methods = HashMap::new();
    let Some(impl_info) = meta.node_info.get(&impl_id) else {
        return methods;
    };
    for &child_id in &impl_info.children {
        let Some(child_info) = meta.node_info.get(&child_id) else {
            continue;
        };
        if child_info.kind != "declaration_list" {
            continue;
        }
        for &item_id in &child_info.children {
            let Some(item_info) = meta.node_info.get(&item_id) else {
                continue;
            };
            if item_info.kind == "function_item" {
                if let Some(name) = fn_name_from_node_info(item_id, meta) {
                    methods.entry(name).or_insert(item_id);
                }
            }
        }
        break;
    }
    methods
}

/// Extract the function name from a function_item node using pre-computed node_info.
///
/// Finds the first direct child with kind "identifier", which is the function name regardless
/// of whether a visibility modifier is present.
fn fn_name_from_node_info(fn_id: usize, meta: &ASTMetadata) -> Option<String> {
    let fn_info = meta.node_info.get(&fn_id)?;
    fn_info.children.iter().find_map(|&child_id| {
        meta.node_info
            .get(&child_id)
            .filter(|info| info.kind == "identifier")
            .map(|info| info.text.clone())
    })
}

/// Collect methods from a Python `class_definition` node, keyed by function name.
///
/// Handles both plain `function_definition` children and `decorated_definition` wrappers
/// (e.g. `@staticmethod def foo()`). Only the innermost `function_definition` name is used
/// as the key, so decorators don't affect matching.
fn methods_in_class(class_id: usize, meta: &ASTMetadata) -> HashMap<String, usize> {
    let mut methods = HashMap::new();
    let Some(class_info) = meta.node_info.get(&class_id) else { return methods };
    for &child_id in &class_info.children {
        let Some(child_info) = meta.node_info.get(&child_id) else { continue };
        if child_info.kind != "block" {
            continue;
        }
        for &item_id in &child_info.children {
            let Some(item_info) = meta.node_info.get(&item_id) else { continue };
            if item_info.kind == "function_definition" {
                if let Some(name) = fn_name_from_node_info(item_id, meta) {
                    methods.entry(name).or_insert(item_id);
                }
            } else if item_info.kind == "decorated_definition" {
                // The `function_definition` is a child of decorated_definition.
                if let Some(&fn_id) = item_info.children.iter().find(|&&id| {
                    meta.node_info
                        .get(&id)
                        .map_or(false, |ci| ci.kind == "function_definition")
                }) {
                    if let Some(name) = fn_name_from_node_info(fn_id, meta) {
                        methods.entry(name).or_insert(fn_id);
                    }
                }
            }
        }
        break;
    }
    methods
}

#[cfg(test)]
mod tests {
    use super::*;

    use anyhow::Result;

    use crate::code::{Code, Language};
    use crate::diff::{ASTDiff, ASTMappingOperation, NodeCache};
    use crate::test;

    #[test]
    fn rust_hash_optimization() -> Result<()> {
        let test_diffs = test::helper::handmade_test_code_pairs()?;
        let (mut before, mut after) = test_diffs.get("rust-hash-optimization").unwrap().clone();

        // Ensure both codes have their metadata computed
        before.ensure_parsed()?;
        after.ensure_parsed()?;

        // Null mappings should be considered valid
        let node_cache = NodeCache::build(&before, &after);

        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();
        let before_root = before_ast.root_node();
        let after_root = after_ast.root_node();

        let mut diff = ASTDiff {
            ..Default::default()
        };

        solve(&before, &after, &node_cache, &mut diff);

        // The root nodes must NOT be mapped
        assert!(
            !diff
                .mapping
                .contains_key(&(before_ast.root_node().id(), after_ast.root_node().id()))
        );

        // fn main() should be mapped now.
        let path = vec!["function_item"];
        let mapping = test::helper::mapping_for_path(&path, &path, before_root, after_root, &diff)?;
        assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);

        Ok(())
    }

    #[test]
    fn methods_in_different_impls_are_matched_within_their_own_impl() -> Result<()> {
        // Two impls with same method name `new`. Only Bar::new changes body.
        let before_src = "
struct Foo;
struct Bar;
impl Foo { fn new() -> Foo { Foo } }
impl Bar { fn new() -> Bar { Bar } }
";
        let after_src = "
struct Foo;
struct Bar;
impl Foo { fn new() -> Foo { Foo } }
impl Bar { fn new() -> Bar { Bar::default() } }
";
        let mut before = Code::from_string(before_src, &Language::Rust);
        let mut after = Code::from_string(after_src, &Language::Rust);
        before.ensure_parsed()?;
        after.ensure_parsed()?;

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();
        solve(&before, &after, &node_cache, &mut diff);

        let before_root = before.ast.as_ref().unwrap().root_node();
        let after_root = after.ast.as_ref().unwrap().root_node();

        // impl Foo::new is identical on both sides.
        let foo_new_mapping = test::helper::mapping_for_path(
            &["impl_item:1", "declaration_list", "function_item"],
            &["impl_item:1", "declaration_list", "function_item"],
            before_root,
            after_root,
            &diff,
        )?;
        assert_eq!(
            foo_new_mapping.operation,
            ASTMappingOperation::Identical,
            "Foo::new should be identical"
        );

        // impl Bar::new changed its return expression.
        let bar_new_mapping = test::helper::mapping_for_path(
            &["impl_item:2", "declaration_list", "function_item"],
            &["impl_item:2", "declaration_list", "function_item"],
            before_root,
            after_root,
            &diff,
        )?;
        assert_eq!(
            bar_new_mapping.operation,
            ASTMappingOperation::MatchButNotIdentical,
            "Bar::new should be changed"
        );

        Ok(())
    }


}
