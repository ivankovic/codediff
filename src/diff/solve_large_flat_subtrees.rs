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
use crate::diff::PassCtx;
use crate::diff::apted::{self, Algorithm};
use crate::diff::solve_syntax_aware_matching::solve_qualified_name_groups_within;
use crate::diff::{ASTDiff, nodes};

/// `apted`'s own `FLAT_MIN_CHILDREN`, so a container found here is guaranteed to take the Myers fast
/// path once handed to `for_nodes`.
const FLAT_CONTAINER_MIN_CHILDREN: usize = apted::FLAT_MIN_CHILDREN;

/// Pre-matches identity-matched top-level items that hold a large flat descendant: the flat pair
/// is diffed on its own first (Myers, via `resolve_forest`'s fast path), then the item itself, with
/// the flat part already pruned. Otherwise the flat part is buried inside a much larger non-flat
/// comparison where the fast path never fires.
///
/// Top-level items only: nested large-flat cases inside deeply structured items are rare, and
/// scanning every structural node rescans covered ground.
pub fn solve(ctx: &PassCtx, diff: &mut ASTDiff) {
    let (before, after, node_cache) = (ctx.before, ctx.after, ctx.node_cache);
    let before_metadata = ctx.before_metadata();
    let after_metadata = ctx.after_metadata();

    let Some(before_ast) = before.ast.as_ref() else {
        return;
    };
    let Some(after_ast) = after.ast.as_ref() else {
        return;
    };
    let language = before_metadata.language;

    let mut before_items =
        top_level_identities(before_ast.root_node(), before_metadata, &language, before);
    let mut after_items =
        top_level_identities(after_ast.root_node(), after_metadata, &language, after);

    // A data file (JSON, YAML) is one anonymous root value with no name to key on; with exactly
    // one named child on each side, they can only correspond to each other.
    if before_items.is_empty()
        && after_items.is_empty()
        && let (Some(b), Some(a)) = (
            only_named_child(before_ast.root_node()),
            only_named_child(after_ast.root_node()),
        )
    {
        let key = ("<whole-file value>".to_string(), String::new());
        before_items.insert(key.clone(), b.id());
        after_items.insert(key, a.id());
    }

    for (key, &before_id) in &before_items {
        let Some(&after_id) = after_items.get(key) else {
            continue;
        };

        let Some(before_flat) = largest_flat_container_in(before_id, before_metadata, &language)
        else {
            continue;
        };
        let Some(after_flat) = largest_flat_container_in(after_id, after_metadata, &language)
        else {
            continue;
        };

        apted::for_nodes(
            before_metadata,
            after_metadata,
            vec![before_flat],
            vec![after_flat],
            Algorithm::Apted,
            "large_flat_subtree",
            diff,
        );

        // The whole-file name pass runs after this one, so named content nested here (Go `t.Run`
        // subtests) would otherwise pay for unconstrained APTED in the container call below.
        if let (Some(&before_node), Some(&after_node)) = (
            node_cache.before.get(&before_id),
            node_cache.after.get(&after_id),
        ) {
            solve_qualified_name_groups_within(
                before_node,
                before_id,
                after_node,
                after_id,
                before_metadata,
                after_metadata,
                before,
                after,
                diff,
            );
        }

        apted::prematch_identical_statement_siblings(
            before_id,
            after_id,
            before_metadata,
            after_metadata,
            "large_flat_subtree_container",
            diff,
        );

        apted::for_nodes(
            before_metadata,
            after_metadata,
            vec![before_id],
            vec![after_id],
            Algorithm::Apted,
            "large_flat_subtree_container",
            diff,
        );
    }
}

/// `(kind, identity) -> node_id` for `root_node`'s direct children, identified by
/// `nodes::is_semantically_structural`, else a Rust macro's callee name, else kind-uniqueness.
fn top_level_identities(
    root_node: tree_sitter::Node,
    metadata: &ASTMetadata,
    language: &crate::code::Language,
    code: &Code,
) -> HashMap<(String, String), usize> {
    let mut result = HashMap::new();
    let mut identified_ids = std::collections::HashSet::new();
    let mut cursor = root_node.walk();
    for child in root_node.children(&mut cursor) {
        if let Some(key) = nodes::is_semantically_structural(&child, language, code) {
            result.entry(key).or_insert(child.id());
            identified_ids.insert(child.id());
            continue;
        }
        if child.kind() == "macro_invocation"
            && let Some(name) = macro_callee_name(child.id(), metadata)
        {
            result
                .entry(("macro_invocation".to_string(), name))
                .or_insert(child.id());
            identified_ids.insert(child.id());
        }
    }

    // A child whose kind is unique among the unidentified children can only correspond to its
    // same-kind counterpart (a top-level `if` wrapping a large literal). Skipped with a single
    // named child: `only_named_child` handles that case kind-agnostically, which also covers a root
    // value whose kind changed (a JSON object rewritten as an array).
    if root_node.named_child_count() > 1 {
        let mut kind_counts: HashMap<&str, usize> = HashMap::new();
        let mut cursor = root_node.walk();
        for child in root_node.children(&mut cursor) {
            if !identified_ids.contains(&child.id()) {
                *kind_counts.entry(child.kind()).or_default() += 1;
            }
        }
        let mut cursor = root_node.walk();
        for child in root_node.children(&mut cursor) {
            if !identified_ids.contains(&child.id()) && kind_counts.get(child.kind()) == Some(&1) {
                result
                    .entry((child.kind().to_string(), String::new()))
                    .or_insert(child.id());
            }
        }
    }

    result
}

/// `root_node`'s single named child, if it has exactly one; anonymous tokens do not count.
fn only_named_child(root_node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    (root_node.named_child_count() == 1)
        .then(|| root_node.named_child(0))
        .flatten()
}

/// Text of a `macro_invocation`'s first `identifier`/`scoped_identifier` child (`foo::bar`).
fn macro_callee_name(macro_id: usize, meta: &ASTMetadata) -> Option<String> {
    let info = meta.node_info.get(&macro_id)?;
    info.children.iter().find_map(|&id| {
        meta.node_info
            .get(&id)
            .filter(|ci| ci.kind == "identifier" || ci.kind == "scoped_identifier")
            .map(|ci| ci.text.clone())
    })
}

/// The widest node in `root_id`'s subtree (inclusive) if it has at least
/// `FLAT_CONTAINER_MIN_CHILDREN` children, else [`widest_data_literal_container`]. The precomputed
/// widest node cannot answer the data-literal question: a small test table is routinely narrower
/// than its function's own body.
fn largest_flat_container_in(
    root_id: usize,
    meta: &ASTMetadata,
    language: &Language,
) -> Option<usize> {
    let &(count, id) = meta.node_to_widest_subtree_node.get(&root_id)?;
    if count >= FLAT_CONTAINER_MIN_CHILDREN {
        return Some(id);
    }
    widest_data_literal_container(root_id, meta, language)
}

/// The widest `is_data_literal_container` node with at least `DATA_LITERAL_MIN_CHILDREN` children
/// in `root_id`'s subtree (inclusive). A real walk, bounded by one top-level item.
fn widest_data_literal_container(
    root_id: usize,
    meta: &ASTMetadata,
    language: &Language,
) -> Option<usize> {
    // Only Go has data-literal kinds; skip the walk elsewhere.
    if !matches!(language, Language::Go) {
        return None;
    }
    let mut best: Option<(usize, usize)> = None;
    let mut stack = vec![root_id];
    while let Some(id) = stack.pop() {
        let Some(info) = meta.node_info.get(&id) else {
            continue;
        };
        if is_data_literal_container(&info.kind, language) {
            let count = info.children.len();
            if count >= DATA_LITERAL_MIN_CHILDREN
                && best.is_none_or(|(best_count, _)| count > best_count)
            {
                best = Some((count, id));
            }
        }
        stack.extend(info.children.iter().copied());
    }
    best.map(|(_, id)| id)
}

/// Lower than `FLAT_CONTAINER_MIN_CHILDREN`: a table-driven test table often has far fewer than 50
/// entries.
const DATA_LITERAL_MIN_CHILDREN: usize = 8;

/// Kinds whose children are independent data items (Go's `testCases := []struct{...}{...}`), safe
/// to Myers-diff at a small size. Code containers are excluded on purpose: their statements are
/// often related-but-different, which APTED can match as `Update` and hash-only Myers cannot.
fn is_data_literal_container(kind: &str, language: &Language) -> bool {
    match language {
        Language::Go => kind == "literal_value",
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::NodeCache;

    #[test]
    fn large_flat_macro_body_is_myers_diffed() {
        let mut args_before = (0..80)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join(", ");
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

        solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );

        let has_flat_reason = diff.mapping.values().any(|m| {
            matches!(
                &m.reason,
                crate::diff::ASTMappingReason::APTED("large_flat_subtree")
            )
        });
        assert!(
            has_flat_reason,
            "expected at least one large_flat_subtree-reasoned mapping"
        );
    }

    #[test]
    fn small_macro_body_is_left_alone() {
        let before_src = "fn f() { let v = vec![1, 2, 3]; }";
        let after_src = "fn f() { let v = vec![1, 2, 3, 4]; }";
        let before = Code::from_string(before_src, &Language::Rust);
        let after = Code::from_string(after_src, &Language::Rust);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );

        assert!(
            diff.mapping.is_empty(),
            "a 3-4 element vec! shouldn't clear FLAT_CONTAINER_MIN_CHILDREN"
        );
    }

    /// A JSON file's one anonymous root value gets an implicit identity.
    #[test]
    fn large_flat_top_level_json_object_is_myers_diffed() {
        let mut pairs_before: Vec<String> = (0..80)
            .map(|i| format!("\"key{i}\": \"value {i}\""))
            .collect();
        let mut pairs_after = pairs_before.clone();
        pairs_before.remove(40);
        let before_src = format!("{{{}}}", pairs_before.join(", "));
        pairs_after[41] = "\"key41\": \"changed value\"".to_string();
        let after_src = format!("{{{}}}", pairs_after.join(", "));

        let before = Code::from_string(&before_src, &Language::JSON);
        let after = Code::from_string(&after_src, &Language::JSON);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );

        let has_flat_reason = diff.mapping.values().any(|m| {
            matches!(
                &m.reason,
                crate::diff::ASTMappingReason::APTED("large_flat_subtree")
            )
        });
        assert!(
            has_flat_reason,
            "expected the top-level JSON object's implicit identity to trigger the flat-subtree fast path"
        );
    }

    #[test]
    fn small_json_object_is_left_alone() {
        let before = Code::from_string(r#"{"a": 1, "b": 2}"#, &Language::JSON);
        let after = Code::from_string(r#"{"a": 1, "b": 3}"#, &Language::JSON);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );

        assert!(
            diff.mapping.is_empty(),
            "a 2-key object shouldn't clear FLAT_CONTAINER_MIN_CHILDREN"
        );
    }

    /// A Go test table below `FLAT_CONTAINER_MIN_CHILDREN` is found even when padding makes
    /// another node the widest subtree.
    #[test]
    fn data_literal_table_is_myers_diffed_even_when_not_the_widest_subtree() {
        let cases_before: String = (0..15)
            .map(|i| format!("{{name: \"case{i}\"}},"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut cases_after: Vec<String> =
            (0..15).map(|i| format!("{{name: \"case{i}\"}},")).collect();
        cases_after[7] = "{name: \"changed\"},".to_string();
        let cases_after = cases_after.join("\n");

        // Identical statements that make the function body wider than the table.
        let padding: String = "_ = 0\n".repeat(16);

        let before_src = format!(
            "package main\nfunc TestThings(t *testing.T) {{\n\
             testCases := []struct{{ name string }}{{\n{cases_before}\n}}\n\
             {padding}}}\n"
        );
        let after_src = format!(
            "package main\nfunc TestThings(t *testing.T) {{\n\
             testCases := []struct{{ name string }}{{\n{cases_after}\n}}\n\
             {padding}}}\n"
        );

        let before = Code::from_string(&before_src, &Language::Go);
        let after = Code::from_string(&after_src, &Language::Go);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );

        let has_flat_reason = diff.mapping.values().any(|m| {
            matches!(
                &m.reason,
                crate::diff::ASTMappingReason::APTED("large_flat_subtree")
            )
        });
        assert!(
            has_flat_reason,
            "expected the testCases table to be found and Myers-diffed despite not being the widest subtree"
        );
    }

    /// A `t.Run` subtest next to a data table is matched by name before the container call.
    #[test]
    fn named_subtest_inside_a_data_literal_function_is_prematched_by_name() {
        let cases_before: String = (0..15)
            .map(|i| format!("{{name: \"case{i}\"}},"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut cases_after: Vec<String> =
            (0..15).map(|i| format!("{{name: \"case{i}\"}},")).collect();
        cases_after[7] = "{name: \"changed\"},".to_string();
        let cases_after = cases_after.join("\n");

        let before_src = format!(
            "package main\nfunc TestThings(t *testing.T) {{\n\
             testCases := []struct{{ name string }}{{\n{cases_before}\n}}\n\
             t.Run(\"independent case\", func(t *testing.T) {{ old() }})\n}}\n"
        );
        let after_src = format!(
            "package main\nfunc TestThings(t *testing.T) {{\n\
             testCases := []struct{{ name string }}{{\n{cases_after}\n}}\n\
             t.Run(\"independent case\", func(t *testing.T) {{ newImpl() }})\n}}\n"
        );

        let before = Code::from_string(&before_src, &Language::Go);
        let after = Code::from_string(&after_src, &Language::Go);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );

        let has_qualified_name_reason = diff.mapping.values().any(|m| {
            matches!(
                &m.reason,
                crate::diff::ASTMappingReason::APTED("qualified_name")
            )
        });
        assert!(
            has_qualified_name_reason,
            "expected the independent t.Run(\"independent case\", ...) call to be pre-matched by name"
        );
    }

    fn python_script_with_guarded_dict(changed: usize, second_if: bool) -> String {
        let entries: Vec<String> = (0..80)
            .map(|i| {
                let value = if i == changed { 999 } else { i };
                format!("\"k{i}\": {value}")
            })
            .collect();
        let extra = if second_if {
            "if False:\n    pass\n"
        } else {
            ""
        };
        format!(
            "x = 1\nif True:\n    d = {{{}}}\n{extra}",
            entries.join(", ")
        )
    }

    #[test]
    fn kind_unique_top_level_wrapper_reaches_a_nested_flat_literal() {
        let before = Code::from_string(
            &python_script_with_guarded_dict(99, false),
            &Language::Python,
        );
        let after = Code::from_string(
            &python_script_with_guarded_dict(40, false),
            &Language::Python,
        );
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();
        solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );
        assert!(diff.mapping.values().any(|m| matches!(
            &m.reason,
            crate::diff::ASTMappingReason::APTED("large_flat_subtree")
        )));
    }

    #[test]
    fn repeated_top_level_wrapper_kind_has_no_identity() {
        let before = Code::from_string(
            &python_script_with_guarded_dict(99, true),
            &Language::Python,
        );
        let after = Code::from_string(
            &python_script_with_guarded_dict(40, true),
            &Language::Python,
        );
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();
        solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );
        assert!(!diff.mapping.values().any(|m| matches!(
            &m.reason,
            crate::diff::ASTMappingReason::APTED("large_flat_subtree")
        )));
    }
}
