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

use crate::code::{ASTMetadata, Code};
use crate::diff::apted::{self, Algorithm};
use crate::diff::{ASTDiff, nodes};

/// Minimum direct-child count for a node to be treated as a "flat" sequence worth Myers-diffing
/// on its own - matches `apted::common`'s own `FLAT_MIN_CHILDREN` threshold (the fast path this
/// pass exists to trigger proactively), so a candidate found here is guaranteed to actually
/// qualify once handed to `for_nodes`.
const FLAT_CONTAINER_MIN_CHILDREN: usize = 50;

/**
* Pre-match top-level items with large flat descendants (e.g. a Rust macro's `token_tree` body,
* a big flat argument/element list, ...) via Myers O(ND) sequence diff, before anything else in
* the pipeline gets a chance to bury them inside a much larger, non-flat comparison.
*
* Originally this only looked for Rust `macro_invocation` nodes by macro name (see git history:
* `solve_flat_macro_bodies` in `solve_semantically_structural_nodes.rs`). Generalized
* (2026-07-17) to any top-level item, in any supported language, whose identity can be
* established (either `nodes::is_semantically_structural`'s cross-language name extraction, or -
* preserving the original Rust macro case, which `is_semantically_structural` does not cover -
* the macro's own callee name) and which contains a large flat descendant anywhere inside it.
*
* Mechanism: for each matched top-level (before, after) pair, BFS both subtrees for the single
* largest node with >= `FLAT_CONTAINER_MIN_CHILDREN` direct children (any kind - not just Rust's
* `token_tree`). If both sides have one, diff that flat pair directly first (`resolve_forest`
* inside `for_nodes` detects the flat shape and routes to Myers instead of Zhang-Shasha/APTED),
* then diff the top-level pair itself (the flat descendant is already in `diff` -> pruned by the
* postorder indexer, so this second call is cheap regardless of how big the item is).
*
* Deliberately scoped to *top-level* items only (direct children of the file root), not every
* `semantically_structural_nodes` entry at any depth: that map is populated by a full-tree walk
* (methods, nested items, ...), and BFS-ing inside every one of those as well would rescan a lot
* of already-covered ground for no expected benefit - nested large-flat cases inside an otherwise
* deeply-structured item are comparatively rare, and this can be widened later if the benchmark
* shows it's worth it.
*/
pub fn solve(before: &Code, after: &Code, diff: &mut ASTDiff) {
    let before_metadata = crate::code::metadata::metadata_of(before);
    let after_metadata = crate::code::metadata::metadata_of(after);

    let Some(before_ast) = before.ast.as_ref() else { return };
    let Some(after_ast) = after.ast.as_ref() else { return };
    let language = before_metadata.language;

    let before_items = top_level_identities(before_ast.root_node(), &before_metadata, &language, before);
    let after_items = top_level_identities(after_ast.root_node(), &after_metadata, &language, after);

    for (key, &before_id) in &before_items {
        let Some(&after_id) = after_items.get(key) else { continue };

        let Some(before_flat) = largest_flat_container_in(before_id, &before_metadata) else { continue };
        let Some(after_flat) = largest_flat_container_in(after_id, &after_metadata) else { continue };

        // Diff the flat descendant directly - the flat-tree fast path in `resolve_forest` picks
        // it up and routes to Myers.
        apted::for_nodes(
            &before_metadata,
            &after_metadata,
            vec![before_flat],
            vec![after_flat],
            Algorithm::Apted,
            "large_flat_subtree",
            diff,
        );
        // Diff the top-level item itself (flat descendant already in `diff` -> pruned).
        apted::for_nodes(
            &before_metadata,
            &after_metadata,
            vec![before_id],
            vec![after_id],
            Algorithm::Apted,
            "large_flat_subtree_container",
            diff,
        );
    }
}

/// `(kind, identity) -> node_id` for every direct child of `root_node` whose identity can be
/// established - `nodes::is_semantically_structural`'s cross-language name extraction first,
/// falling back to a macro's own callee name (Rust `macro_invocation`, which
/// `is_semantically_structural` does not cover - see that function's doc comment for why: it's
/// about compiler-enforced-unique declarations, and a macro invocation is neither).
fn top_level_identities(
    root_node: tree_sitter::Node,
    metadata: &ASTMetadata,
    language: &crate::code::Language,
    code: &Code,
) -> HashMap<(String, String), usize> {
    let mut result = HashMap::new();
    let mut cursor = root_node.walk();
    for child in root_node.children(&mut cursor) {
        if let Some(key) = nodes::is_semantically_structural(&child, language, code) {
            result.entry(key).or_insert(child.id());
            continue;
        }
        if child.kind() == "macro_invocation"
            && let Some(name) = macro_callee_name(child.id(), metadata)
        {
            result.entry(("macro_invocation".to_string(), name)).or_insert(child.id());
        }
    }
    result
}

/// Macro name = text of the first `identifier`/`scoped_identifier` child of a `macro_invocation`
/// node (e.g. `println` in `println!(...)`, `foo::bar` in `foo::bar!(...)`).
fn macro_callee_name(macro_id: usize, meta: &ASTMetadata) -> Option<String> {
    let info = meta.node_info.get(&macro_id)?;
    info.children.iter().find_map(|&id| {
        meta.node_info
            .get(&id)
            .filter(|ci| ci.kind == "identifier" || ci.kind == "scoped_identifier")
            .map(|ci| ci.text.clone())
    })
}

/// The node with the most direct children found anywhere in `root_id`'s own subtree (inclusive),
/// provided that count is >= `FLAT_CONTAINER_MIN_CHILDREN`. Any kind qualifies - unlike the
/// Rust-macro-specific predecessor this generalizes, which only ever looked for `token_tree`.
///
/// O(1): `ASTMetadata::node_to_widest_subtree_node` is precomputed once per file (bottom-up,
/// alongside `node_to_subtree_size`), so this pass never needs to walk a candidate's subtree
/// itself just to find out it has nothing flat in it - which is the common case (a qualifying
/// flat subtree is rare; most top-level items never have one).
fn largest_flat_container_in(root_id: usize, meta: &ASTMetadata) -> Option<usize> {
    let &(count, id) = meta.node_to_widest_subtree_node.get(&root_id)?;
    (count >= FLAT_CONTAINER_MIN_CHILDREN).then_some(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::Language;
    use crate::diff::NodeCache;

    #[test]
    fn large_flat_macro_body_is_myers_diffed() {
        let mut args_before = (0..80).map(|i| format!("a{i}")).collect::<Vec<_>>().join(", ");
        args_before.insert_str(0, "vec![");
        args_before.push(']');
        let mut args_after = (0..80).map(|i| format!("a{i}")).collect::<Vec<_>>();
        args_after.insert(40, "NEW".to_string());
        let args_after = format!("vec![{}]", args_after.join(", "));

        let before_src = format!("fn f() {{ let v = {args_before}; }}");
        let after_src = format!("fn f() {{ let v = {args_after}; }}");

        let before = Code::from_string(&before_src, &Language::Rust);
        let after = Code::from_string(&after_src, &Language::Rust);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve(&before, &after, &mut diff);

        // The macro_invocation (vec!) itself should end up mapped, with the flat body
        // pre-matched via the "large_flat_subtree" reason before it was diffed.
        let has_flat_reason = diff
            .mapping
            .values()
            .any(|m| matches!(&m.reason, crate::diff::ASTMappingReason::APTED("large_flat_subtree")));
        assert!(has_flat_reason, "expected at least one large_flat_subtree-reasoned mapping");
        let _ = node_cache;
    }

    #[test]
    fn small_macro_body_is_left_alone() {
        let before_src = "fn f() { let v = vec![1, 2, 3]; }";
        let after_src = "fn f() { let v = vec![1, 2, 3, 4]; }";
        let before = Code::from_string(before_src, &Language::Rust);
        let after = Code::from_string(after_src, &Language::Rust);
        let mut diff = ASTDiff::default();

        solve(&before, &after, &mut diff);

        assert!(
            diff.mapping.is_empty(),
            "a 3-4 element vec! shouldn't clear FLAT_CONTAINER_MIN_CHILDREN"
        );
    }
}
