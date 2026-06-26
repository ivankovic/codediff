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
use crate::diff::{ASTDiff, NodeCache};

/**
* Match semantically structural nodes and solve their subtrees.
*
* For impl_item pairs, methods inside the matched pair are pre-matched by name before running
* APTED on the impl body. This lets the postorder indexer prune already-matched method subtrees,
* reducing the effective tree size passed to the O(n²) engine from the entire impl body to just
* the unmatched residual (impl header + unmatched methods).
*/
pub fn solve(before: &Code, after: &Code, _node_cache: &NodeCache, diff: &mut ASTDiff) {
    let before_metadata = match &before.metadata.ast_metadata {
        Some(m) => m,
        None => return,
    };
    let after_metadata = match &after.metadata.ast_metadata {
        Some(m) => m,
        None => return,
    };

    let language = before.metadata.language.as_ref();

    // Pass 0a: Rust — top-level macro_invocations with large flat token_tree bodies.
    if matches!(language, Some(Language::Rust)) {
        solve_flat_macro_bodies(before, after, before_metadata, after_metadata, diff);
    }

    // Pass 0b: Python — class pairs with method pre-matching (mirrors Rust impl recursion).
    if matches!(language, Some(Language::Python)) {
        for ((kind, identifier), &before_class_id) in
            &before_metadata.semantically_structural_nodes
        {
            if kind != "class_definition" {
                continue;
            }
            let Some(&after_class_id) = after_metadata
                .semantically_structural_nodes
                .get(&(kind.clone(), identifier.clone()))
            else {
                continue;
            };
            let before_methods = methods_in_class(before_class_id, before_metadata);
            let after_methods = methods_in_class(after_class_id, after_metadata);
            for (method_name, before_method_id) in &before_methods {
                if let Some(&after_method_id) = after_methods.get(method_name) {
                    let _ = apted::for_nodes(
                        before_metadata,
                        after_metadata,
                        vec![*before_method_id],
                        vec![after_method_id],
                        Algorithm::ZhangShasha,
                        diff,
                    );
                }
            }
            let _ = apted::for_nodes(
                before_metadata,
                after_metadata,
                vec![before_class_id],
                vec![after_class_id],
                Algorithm::ZhangShasha,
                diff,
            );
        }
    }

    // Pass 1: impl_item pairs — match methods within each matched impl first, then diff the impl.
    // This is done before the global pass so that method node_ids are in `diff` before any
    // global function_item entries (which may include these same methods from the DFS traversal)
    // are processed.
    for ((kind, identifier), &before_impl_id) in &before_metadata.semantically_structural_nodes {
        if kind != "impl_item" {
            continue;
        }
        let Some(&after_impl_id) = after_metadata
            .semantically_structural_nodes
            .get(&(kind.clone(), identifier.clone()))
        else {
            continue;
        };

        let before_methods = methods_in_impl(before_impl_id, before_metadata);
        let after_methods = methods_in_impl(after_impl_id, after_metadata);

        for (method_name, before_method_id) in &before_methods {
            if let Some(&after_method_id) = after_methods.get(method_name) {
                let _ = apted::for_nodes(
                    before_metadata,
                    after_metadata,
                    vec![*before_method_id],
                    vec![after_method_id],
                    Algorithm::ZhangShasha,
                    diff,
                );
            }
        }

        // The method subtrees are now in `diff` and will be pruned by PostorderIndexer.
        let _ = apted::for_nodes(
            before_metadata,
            after_metadata,
            vec![before_impl_id],
            vec![after_impl_id],
            Algorithm::ZhangShasha,
            diff,
        );
    }

    // Pass 2: all other matched pairs (fn, struct, enum, …).
    // Methods already matched in Pass 0b/1 are skipped via filter_before/after_nodes inside
    // for_nodes → resolve_forest.
    for ((kind, identifier), &before_node_id) in &before_metadata.semantically_structural_nodes {
        if kind == "impl_item" || kind == "class_definition" {
            continue;
        }
        if let Some(&after_node_id) = after_metadata
            .semantically_structural_nodes
            .get(&(kind.clone(), identifier.clone()))
        {
            let _ = apted::for_nodes(
                before_metadata,
                after_metadata,
                vec![before_node_id],
                vec![after_node_id],
                Algorithm::ZhangShasha,
                diff,
            );
        }
    }
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

/// Minimum leaf-child count for a token_tree to be treated as a flat sequence.
const FLAT_MACRO_MIN_TOKENS: usize = 50;

/// Pre-match top-level macro_invocation nodes that contain large flat token_tree bodies.
///
/// For each matched pair (same macro name on both sides) where the token_tree body has
/// >= FLAT_MACRO_MIN_TOKENS leaf children, we call `for_nodes` directly on the token_tree pair.
/// `resolve_forest` inside `for_nodes` detects the flat shape and routes to Myers O(ND) diff
/// instead of Zhang-Shasha O(N²), then we call `for_nodes` on the macro pair (token_tree
/// already in `diff` → pruned by the postorder indexer).
fn solve_flat_macro_bodies(
    before: &Code,
    after: &Code,
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
    diff: &mut ASTDiff,
) {
    let Some(before_ast) = before.ast.as_ref() else { return };
    let Some(after_ast) = after.ast.as_ref() else { return };
    let before_root = before_ast.root_node().id();
    let after_root = after_ast.root_node().id();

    let before_macros = collect_flat_macros(before_root, before_metadata);
    let after_macros = collect_flat_macros(after_root, after_metadata);

    for (name, (before_macro_id, before_tt_id)) in &before_macros {
        let Some(&(after_macro_id, after_tt_id)) = after_macros.get(name) else { continue };

        // Diff the flat token_tree bodies — the flat-tree fast path in resolve_forest picks it up.
        let _ = apted::for_nodes(
            before_metadata,
            after_metadata,
            vec![*before_tt_id],
            vec![after_tt_id],
            Algorithm::ZhangShasha,
            diff,
        );
        // Diff the macro_invocation wrapper (token_tree body already in diff → pruned).
        let _ = apted::for_nodes(
            before_metadata,
            after_metadata,
            vec![*before_macro_id],
            vec![after_macro_id],
            Algorithm::ZhangShasha,
            diff,
        );
    }
}

/// Returns a map of `macro_name → (macro_invocation_id, token_tree_id)` for each top-level
/// macro_invocation in `root_id`'s children that has a large nested token_tree (searched
/// recursively inside the macro body, picking the token_tree with the most direct children).
fn collect_flat_macros(
    root_id: usize,
    meta: &ASTMetadata,
) -> HashMap<String, (usize, usize)> {
    let mut result = HashMap::new();
    let Some(root_info) = meta.node_info.get(&root_id) else { return result };

    for &child_id in &root_info.children {
        let Some(child_info) = meta.node_info.get(&child_id) else { continue };
        if child_info.kind != "macro_invocation" {
            continue;
        }
        // Macro name = text of the first identifier/scoped_identifier child.
        let Some(macro_name) = child_info.children.iter().find_map(|&id| {
            meta.node_info
                .get(&id)
                .filter(|ci| ci.kind == "identifier" || ci.kind == "scoped_identifier")
                .map(|ci| ci.text.clone())
        }) else {
            continue;
        };
        // Find the largest token_tree anywhere in the macro body (BFS).
        let Some(tt_id) = largest_token_tree_in(child_id, meta) else { continue };

        result.entry(macro_name).or_insert((child_id, tt_id));
    }
    result
}

/// BFS over all descendants of `root_id` to find the `token_tree` node with the most direct
/// children, provided that count is >= FLAT_MACRO_MIN_TOKENS.
fn largest_token_tree_in(root_id: usize, meta: &ASTMetadata) -> Option<usize> {
    let mut best: Option<(usize, usize)> = None; // (child_count, node_id)
    let mut queue = vec![root_id];
    while let Some(id) = queue.pop() {
        let Some(info) = meta.node_info.get(&id) else { continue };
        if info.kind == "token_tree" && info.children.len() >= FLAT_MACRO_MIN_TOKENS {
            let n = info.children.len();
            if best.map_or(true, |(best_n, _)| n > best_n) {
                best = Some((n, id));
            }
        }
        for &cid in &info.children {
            queue.push(cid);
        }
    }
    best.map(|(_, id)| id)
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
