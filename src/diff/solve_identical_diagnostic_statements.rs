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
use std::collections::{HashMap, VecDeque};

use crate::diff::PassCtx;
use crate::diff::nodes::{collect_unmatched, is_diagnostic_statement, map_identical_descendants};
use crate::diff::{ASTDiff, ASTMapping, ASTMappingReason};

/**
* MatchIdenticalDiagnosticStatements: pairs up still-unmatched calls/macros that look like they're
* meant for the programmer rather than the end user - logging (`log::error!`, `logger.warn(...)`),
* error bailouts (`bail!`, `panic!`), assertions, debug `printf`s - whenever the *entire statement*
* is byte-for-byte identical on both sides. See [`crate::diff::nodes::is_diagnostic_statement`] for
* exactly what counts.
*
* Runs as part of phase 2 of the matching pipeline (`Diff::pending_with_config` lists all ten
* phases), right after phase 1's hash descent and before phase 4's syntax-aware matching, the
* terminal residual resolution and the move-detection fallback. Running it after phase 1 means it only ever picks up
* statements phase 1's byte-identical/structural hash matching left behind, so it can't fragment a
* match a bigger/more-reliable pass would otherwise have made in one piece (e.g. matching a whole
* function wholesale) into a smaller one anchored on some incidental identical `log::debug!(...)`
* call inside it. Running it before the later phases means it can still find a diagnostic
* statement buried inside a function/impl that has no same-named counterpart on the other side -
* the same reasoning the since-deleted (2026-08-14) `solve_similar_flow_control` used to document
* for its own placement inside phase 4.
*
* Note this pass has no effect on the `rust-turbopack-module-rule` fixture that motivated
* `solve_similar_flow_control` (deleted 2026-08-14): its one `bail!(...)` isn't byte-identical
* between before/after (the message text changed to list an added "json" module type), so it
* wouldn't match here regardless. And even if it were identical, the enclosing `match_expression` -
* `bail!` arm included - gets fully resolved via a real `apted::for_nodes` call on the container
* regardless (through phase 4's named-group matching on the enclosing declaration, or, when
* `solve_similar_flow_control` still existed and was enabled, through its own container-wide call),
* since `for_nodes` computes a complete edit script over the whole subtree, not just the root pair.
* So this pass is currently validated only by its own unit tests (including non-Rust ones covering
* C/Python/Go callee extraction) and has no real-fixture coverage yet - worth keeping in mind if it
* ever needs revisiting.
*
* Matching requires an *exact* full-content hash match (see `ASTMetadata::node_to_full_hash`) - no
* similarity threshold. A `log::error!("failed: {e}")` that
* only *changed* on one side is deliberately left for the final APTED pass to size up as an Update,
* since this pass has no basis for deciding two non-identical diagnostic statements are "the same"
* one.
*/
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

    let before_source = before.contents.as_bytes();
    let after_source = after.contents.as_bytes();

    let before_candidates =
        collect_unmatched(before_ast.root_node(), &diff.before_node_map, |node| {
            is_diagnostic_statement(node, &language, before_source)
        });
    let after_candidates = collect_unmatched(after_ast.root_node(), &diff.after_node_map, |node| {
        is_diagnostic_statement(node, &language, after_source)
    });
    if before_candidates.is_empty() || after_candidates.is_empty() {
        return;
    }

    // Index the after-side candidates by full hash, so each is only ever offered to one
    // before-node (first-come, first-served, largest subtree first below).
    let mut after_by_hash: HashMap<u64, VecDeque<usize>> = HashMap::new();
    for node in &after_candidates {
        if let Some(&hash) = after_metadata.node_to_full_hash.get(&node.id()) {
            after_by_hash.entry(hash).or_default().push_back(node.id());
        }
    }

    // Largest subtree first, mirroring `solve_hash_descent` - not load-bearing here (these are
    // small leaf-ish statements, rarely nested in one another), but keeps behavior deterministic
    // and consistent with the rest of the pipeline.
    let mut before_candidates = before_candidates;
    before_candidates.sort_by(|a, b| {
        let size_a = before_metadata
            .node_to_subtree_size
            .get(&a.id())
            .copied()
            .unwrap_or(0);
        let size_b = before_metadata
            .node_to_subtree_size
            .get(&b.id())
            .copied()
            .unwrap_or(0);
        size_b.cmp(&size_a)
    });

    for before_node in before_candidates {
        if diff.before_node_map.contains_key(&before_node.id()) {
            continue;
        }
        let Some(hash) = before_metadata.node_to_full_hash.get(&before_node.id()) else {
            continue;
        };
        let Some(queue) = after_by_hash.get_mut(hash) else {
            continue;
        };
        let Some(after_node_id) = queue.pop_front() else {
            continue;
        };
        let Some(after_node) = node_cache.after.get(&after_node_id).copied() else {
            continue;
        };

        diff.add_mapping(
            before_node.id(),
            after_node_id,
            ASTMapping::identical(ASTMappingReason::IdenticalHash),
        );

        // Recursively add all descendants with IdenticalHashOfAncestor reason - same idiom as
        // `solve_hash_descent`, since an identical full hash guarantees the entire subtree
        // (structure and values) is byte-for-byte identical too.
        map_identical_descendants(before_node, after_node, diff);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::Code;
    use crate::code::Language;
    use crate::diff::ASTMappingOperation;
    use crate::diff::NodeCache;
    use crate::test::helper::find_first_of_kind;
    use tree_sitter::Node;

    fn find_all<'a>(node: Node<'a>, kind: &str, out: &mut Vec<Node<'a>>) {
        if node.kind() == kind {
            out.push(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            find_all(child, kind, out);
        }
    }

    #[test]
    fn identical_bail_macro_is_matched_across_renamed_functions() {
        // `from_str` has no counterpart named function in `after` (renamed to `parse`), so
        // without this pass the whole function - including the `bail!` inside it - would be
        // swallowed into a from-scratch delete+insert by the final orphan pass.
        let before_src = r#"
fn from_str(s: &str) -> Result<i32> {
    if s.is_empty() {
        bail!("input must not be empty");
    }
    Ok(1)
}
"#;
        let after_src = r#"
fn parse(s: &str) -> Result<i32> {
    if s.is_empty() {
        bail!("input must not be empty");
    }
    Ok(2)
}
"#;
        let before = Code::from_string(before_src, &Language::Rust);
        let after = Code::from_string(after_src, &Language::Rust);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );

        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();
        let before_bail = find_first_of_kind(before_ast.root_node(), "macro_invocation").unwrap();
        let after_bail = find_first_of_kind(after_ast.root_node(), "macro_invocation").unwrap();

        let mapping = diff
            .mapping
            .get(&(before_bail.id(), after_bail.id()))
            .expect("the two identical `bail!` calls should be mapped to each other");
        assert_eq!(mapping.operation, ASTMappingOperation::Identical);
    }

    #[test]
    fn changed_diagnostic_statement_is_not_matched() {
        // Same macro, different message: not byte-identical, so this pass - which requires an
        // exact hash match, no similarity threshold - must leave it alone for the final APTED
        // pass to size up.
        let before_src = r#"
fn a() {
    log::error!("first message");
}
"#;
        let after_src = r#"
fn b() {
    log::error!("second message");
}
"#;
        let before = Code::from_string(before_src, &Language::Rust);
        let after = Code::from_string(after_src, &Language::Rust);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );

        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();
        let before_log = find_first_of_kind(before_ast.root_node(), "macro_invocation").unwrap();
        let after_log = find_first_of_kind(after_ast.root_node(), "macro_invocation").unwrap();

        assert!(
            !diff
                .mapping
                .contains_key(&(before_log.id(), after_log.id())),
            "non-identical diagnostic statements should not be matched by this pass"
        );
    }

    #[test]
    fn non_diagnostic_identical_call_is_not_matched() {
        // Same call, byte-identical, but `compute(...)` isn't a recognized diagnostic callee -
        // this pass should leave ordinary calls to the bigger/later passes.
        let before_src = r#"
fn a() {
    compute(1, 2);
}
"#;
        let after_src = r#"
fn b() {
    compute(1, 2);
}
"#;
        let before = Code::from_string(before_src, &Language::Rust);
        let after = Code::from_string(after_src, &Language::Rust);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );

        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();
        let before_call = find_first_of_kind(before_ast.root_node(), "call_expression").unwrap();
        let after_call = find_first_of_kind(after_ast.root_node(), "call_expression").unwrap();

        assert!(
            !diff
                .mapping
                .contains_key(&(before_call.id(), after_call.id())),
            "non-diagnostic calls should not be matched by this pass, even if identical"
        );
    }

    #[test]
    fn duplicate_identical_diagnostic_calls_are_matched_one_to_one() {
        // Two identical `bail!` calls on each side: each before-node should claim a distinct
        // after-node, not both collapse onto the same one (`solve_hash_descent` used to have
        // exactly that collapse quirk, since fixed - this pass indexes after-candidates in a
        // `VecDeque` per hash to guarantee one-to-one pairing).
        let before_src = r#"
fn a() {
    if true { bail!("dup"); }
    if false { bail!("dup"); }
}
"#;
        let after_src = r#"
fn b() {
    if true { bail!("dup"); }
    if false { bail!("dup"); }
}
"#;
        let before = Code::from_string(before_src, &Language::Rust);
        let after = Code::from_string(after_src, &Language::Rust);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );

        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();
        let mut before_bails = Vec::new();
        let mut after_bails = Vec::new();
        find_all(
            before_ast.root_node(),
            "macro_invocation",
            &mut before_bails,
        );
        find_all(after_ast.root_node(), "macro_invocation", &mut after_bails);
        assert_eq!(before_bails.len(), 2);
        assert_eq!(after_bails.len(), 2);

        let targets: std::collections::HashSet<usize> = before_bails
            .iter()
            .map(|b| {
                *diff
                    .before_node_map
                    .get(&b.id())
                    .unwrap_or_else(|| panic!("before bail! {} was never matched", b.id()))
            })
            .collect();
        assert_eq!(
            targets.len(),
            2,
            "each duplicate bail! should match a distinct after-node"
        );
    }
}
