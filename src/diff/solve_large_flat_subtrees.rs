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
use crate::diff::solve_syntax_aware_matching::solve_named_reference_groups_within;
use crate::diff::{ASTDiff, NodeCache, nodes};

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
* `solve_flat_macro_bodies`, since removed - folded into this file). Generalized
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
*
* A file whose grammar wraps a single anonymous root value (JSON, YAML, ...) has no named
* top-level item to key off at all, so the name-based matching above finds nothing and this pass
* used to do nothing for such files (2026-07-23 gap, since fixed): `only_named_child` gives the
* file's one real top-level value an implicit identity when there's exactly one on each side and
* nothing already matched by name, so a large flat object/mapping at the very top of the file
* still gets the same Myers fast path instead of the whole file falling to `final_apted` on every
* edit (see this pass's own git history / `nodes::is_commutative_container`'s doc comment for the
* concrete case this fixes).
*/
pub fn solve(before: &Code, after: &Code, node_cache: &NodeCache, diff: &mut ASTDiff) {
    let before_metadata = crate::code::metadata::metadata_of(before);
    let after_metadata = crate::code::metadata::metadata_of(after);

    let Some(before_ast) = before.ast.as_ref() else {
        return;
    };
    let Some(after_ast) = after.ast.as_ref() else {
        return;
    };
    let language = before_metadata.language;

    let mut before_items =
        top_level_identities(before_ast.root_node(), &before_metadata, &language, before);
    let mut after_items =
        top_level_identities(after_ast.root_node(), &after_metadata, &language, after);

    // A data-shaped file (JSON, YAML, ...) has no named top-level declarations to key off at
    // all - its whole content is one anonymous value (an `object`/`array`/mapping/...), so
    // `top_level_identities` above always comes back empty for it and this pass previously did
    // nothing for such files whatsoever. That's a real gap, not just a missed optimization:
    // confirmed against a live case (jellyfin-jellyfin's `cs.json`, a single deleted key out of
    // ~140 in a flat object) that the *entire* file then falls through every other pass onto
    // `final_apted`'s unconstrained tree-edit-distance - 1.2s and 100% `APTED`-attributed mappings
    // for a 3,075-combined-node file, versus 0.6ms when the same top-level object is fed through
    // this pass's existing Myers machinery directly.
    //
    // Unlike the general "match top-level items by name" case above, this doesn't need a name to
    // disambiguate *which* top-level item corresponds to which: when there's exactly one named
    // top-level node on each side (true for any file whose grammar wraps a single root value,
    // and only ever attempted when nothing already matched by name), it's necessarily the same
    // logical thing on both sides - there is nothing else it could correspond to.
    if before_items.is_empty() && after_items.is_empty() {
        if let (Some(b), Some(a)) = (
            only_named_child(before_ast.root_node()),
            only_named_child(after_ast.root_node()),
        ) {
            let key = ("<whole-file value>".to_string(), String::new());
            before_items.insert(key.clone(), b.id());
            after_items.insert(key, a.id());
        }
    }

    for (key, &before_id) in &before_items {
        let Some(&after_id) = after_items.get(key) else {
            continue;
        };

        let Some(before_flat) = largest_flat_container_in(before_id, &before_metadata, &language)
        else {
            continue;
        };
        let Some(after_flat) = largest_flat_container_in(after_id, &after_metadata, &language)
        else {
            continue;
        };

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

        // Also pre-match any *named* content nested inside this item (e.g. Go's literal-named
        // `t.Run("...", ...)` subtest calls) before the container-wide call below - otherwise it
        // pays full, unconstrained tree-edit-distance for content that `solve_named_reference_
        // groups` would otherwise have matched cheaply by name, since that pass runs *after* this
        // one (deliberately - see this function's doc comment) and so never gets the chance. See
        // `solve_named_reference_groups_within`'s doc comment for the confirmed live cases this
        // fixes.
        if let (Some(&before_node), Some(&after_node)) = (
            node_cache.before.get(&before_id),
            node_cache.after.get(&after_id),
        ) {
            solve_named_reference_groups_within(
                before_node,
                before_id,
                after_node,
                after_id,
                &before_metadata,
                &after_metadata,
                before,
                after,
                diff,
            );
        }

        // Also pre-match any *other* mostly-unchanged statement sequence still left inside this
        // item (e.g. the item's own body, if the flat descendant found above was something else
        // entirely - a nested data literal, not the item's top-level statements) - see that
        // function's own doc comment. Purely additive, same as the pre-match above.
        apted::prematch_identical_statement_siblings(
            before_id,
            after_id,
            &before_metadata,
            &after_metadata,
            "large_flat_subtree_container",
            diff,
        );

        // Diff the top-level item itself (flat descendant, and anything just pre-matched above,
        // already in `diff` -> pruned).
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
            result
                .entry(("macro_invocation".to_string(), name))
                .or_insert(child.id());
        }
    }
    result
}

/// `root_node`'s single named child, if it has exactly one - i.e. `root_node` wraps exactly one
/// substantive value (as a JSON/YAML file's document root does) rather than a sequence of several
/// top-level items. Anonymous children (punctuation, etc.) don't count, so a trailing newline
/// token (if the grammar even emits one at this level) can't spuriously disqualify a file that
/// otherwise has just one real top-level value.
fn only_named_child(root_node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    (root_node.named_child_count() == 1)
        .then(|| root_node.named_child(0))
        .flatten()
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
///
/// Below `FLAT_CONTAINER_MIN_CHILDREN`, falls back to `widest_data_literal_container` - a
/// dedicated walk for a recognized data-literal body (`is_data_literal_container`) with at least
/// `DATA_LITERAL_MIN_CHILDREN` children. This can't reuse the precomputed widest-subtree-of-any-
/// kind the way the general case above does: the *overall* widest subtree in a function
/// containing, say, a 15-element `testCases` table is routinely something else entirely (the
/// function's own body, a nested closure, ...) with more direct children than the table itself,
/// so a plain kind check on it misses the table completely - confirmed against a live case
/// (cockroachdb's `api_v2_grants_test.go`: the precomputed widest subtree wasn't the table at all,
/// so the table never got a chance).
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

/// The widest `is_data_literal_container` node (>= `DATA_LITERAL_MIN_CHILDREN` direct children)
/// found anywhere in `root_id`'s own subtree (inclusive), or `None`.
///
/// Not O(1) like `largest_flat_container_in`'s general case - a real walk of `root_id`'s subtree,
/// since (unlike the widest-subtree-of-*any*-kind precomputation) nothing tracks "widest subtree
/// of *this specific* kind" up front. Bounded by `root_id`'s own subtree size, though (one
/// top-level item, e.g. one test function - not the whole file), so this stays cheap: exactly the
/// walk `solve_large_flat_subtrees` did everywhere before the O(1) precomputation existed, just
/// scoped down to the kinds that actually need it.
fn widest_data_literal_container(
    root_id: usize,
    meta: &ASTMetadata,
    language: &Language,
) -> Option<usize> {
    // `is_data_literal_container` is only ever true for `Language::Go`, so for every other
    // language this walk is guaranteed to return `None` - skip paying for it on the common case
    // (every non-Go top-level item that didn't already qualify via the O(1) widest-subtree check
    // above).
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

/// Minimum direct-child count for a *data-literal* body (see `is_data_literal_container`) to
/// qualify for the Myers fast path - much lower than `FLAT_CONTAINER_MIN_CHILDREN`, since a real
/// table-driven test table commonly has far fewer than 50 entries (the live case this was tuned
/// against, cockroachdb's `api_v2_grants_test.go`, has 15).
const DATA_LITERAL_MIN_CHILDREN: usize = 8;

/// Node kinds that hold a data literal's *elements* - each one an independent, self-contained data
/// item, unlike a `block`/`statement_list`'s sequentially-related statements. Safe to Myers-diff
/// at a much smaller size than an arbitrary container (`DATA_LITERAL_MIN_CHILDREN` vs.
/// `FLAT_CONTAINER_MIN_CHILDREN`) precisely because Go's `testCases := []struct{...}{ {...}, {...},
/// ... }` idiom - the whole reason this exists - is exactly this shape and commonly has well under
/// 50 entries.
///
/// Deliberately does *not* include ordinary code containers (`block`, `statement_list`, ...): a
/// uniformly-lowered `FLAT_CONTAINER_MIN_CHILDREN` was tried and reverted (2026-07-23) after it
/// regressed two previously-exact fixtures (`python-refactoring`, `kotlin-nextcloud-a-few-small-
/// removals`) - Myers-by-exact-hash can only recognize byte-identical elements, and unlike a data
/// table's independent entries, ordinary statements are routinely related-but-different, which
/// real tree-edit-distance can still partially match (`Update`) and hash-only Myers cannot (it can
/// only call each one an outright delete+insert). Restricting to kinds that are genuinely
/// data-literal bodies keeps the low threshold from ever applying to that case at all.
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

        solve(&before, &after, &node_cache, &mut diff);

        // The macro_invocation (vec!) itself should end up mapped, with the flat body
        // pre-matched via the "large_flat_subtree" reason before it was diffed.
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

        solve(&before, &after, &node_cache, &mut diff);

        assert!(
            diff.mapping.is_empty(),
            "a 3-4 element vec! shouldn't clear FLAT_CONTAINER_MIN_CHILDREN"
        );
    }

    /// Regression guard for the 2026-07-23 fix: a JSON (or YAML, ...) file has no named
    /// top-level declaration to key off - its whole content is one anonymous `object` - so this
    /// pass used to never fire for such files at all, no matter how large the top-level object
    /// was. Confirmed against a real case (a single deleted key out of ~140 in a jellyfin
    /// localization file) that this used to send the *entire* file through `final_apted`'s
    /// unconstrained tree-edit-distance: 1.2s and 100% `APTED`-attributed mappings for a
    /// 3,075-combined-node file, vs. ~2ms once this pass can see it.
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

        solve(&before, &after, &node_cache, &mut diff);

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

    /// A JSON file with more than one top-level value doesn't parse (JSON only ever has one root
    /// value) - `only_named_child`'s fallback should quietly do nothing rather than guess when a
    /// (hypothetical, for another language) file's root has several unnamed children, since which
    /// one corresponds to which would be ambiguous. Exercised here via a small object (too small
    /// to have a qualifying flat descendant either way) to confirm the fallback path doesn't
    /// misfire or panic on an otherwise-ordinary file.
    #[test]
    fn small_json_object_is_left_alone() {
        let before = Code::from_string(r#"{"a": 1, "b": 2}"#, &Language::JSON);
        let after = Code::from_string(r#"{"a": 1, "b": 3}"#, &Language::JSON);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve(&before, &after, &node_cache, &mut diff);

        assert!(
            diff.mapping.is_empty(),
            "a 2-key object shouldn't clear FLAT_CONTAINER_MIN_CHILDREN"
        );
    }

    /// Regression guard for the 2026-07-23 fix (`widest_data_literal_container`): a Go
    /// table-driven test's `testCases := []struct{...}{...}` (well under
    /// `FLAT_CONTAINER_MIN_CHILDREN`, but a recognized `literal_value` data-literal body with
    /// enough entries to clear `DATA_LITERAL_MIN_CHILDREN`) still gets the Myers fast path, even
    /// though it is *not* the single widest subtree in its enclosing function - a large amount of
    /// surrounding code (here, a big `switch` acting as deliberate padding) outweighs it in direct
    /// child count, confirmed against a live case (cockroachdb's `api_v2_grants_test.go`) to
    /// otherwise make the data table invisible to the plain widest-subtree-of-any-kind check.
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

        // A run of 20 byte-identical statements (not a `literal_value`, so it can't itself be
        // picked up by `widest_data_literal_container`'s kind-filtered walk) makes the enclosing
        // `block`/`statement_list` wider than the 15-element testCases table, so the *overall*
        // widest-subtree-of-any-kind precomputation points here instead - all identical content,
        // so it's still instant to diff (an `IdenticalHash` match), unlike the switch-statement
        // padding this test originally used, which took 8+ seconds in an unoptimized debug build.
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

        solve(&before, &after, &node_cache, &mut diff);

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

    /// Regression guard for the 2026-07-23 fix (`solve_named_reference_groups_within`): a Go test
    /// function containing *both* a large data-literal table and an independent, literal-named
    /// `t.Run(...)` subtest call should get the subtest call pre-matched by name (`syntax_named`)
    /// before the container-wide `large_flat_subtree_container` call, rather than paying full
    /// tree-edit-distance for it - confirmed against live cases (cockroachdb's
    /// `api_v2_grants_test.go`, jesseduffield/lazygit's `graph_test.go`) that this was previously
    /// the residual cost keeping those files multi-second even after the data table itself got the
    /// Myers fast path.
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

        solve(&before, &after, &node_cache, &mut diff);

        let has_syntax_named_reason = diff.mapping.values().any(|m| {
            matches!(
                &m.reason,
                crate::diff::ASTMappingReason::APTED("syntax_named")
            )
        });
        assert!(
            has_syntax_named_reason,
            "expected the independent t.Run(\"independent case\", ...) call to be pre-matched by name"
        );
    }
}
