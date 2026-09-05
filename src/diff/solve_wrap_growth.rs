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
use tree_sitter::Node;

use crate::diff::PassCtx;
use crate::diff::{ASTDiff, ASTMappingOperation, ASTMappingReason, NodeCache};

/// Fixes up the same kind of phase-1 attribution gap as `solve_heritage_clause_growth`, for a
/// different structural shape: wrapping existing code in a brand-new construct (Java/TypeScript's
/// `try { EXISTING } catch (...) { NEW }`, Rust's `if COND { NEW } else if EXISTING_COND {
/// EXISTING }`, Python's `if COND: NEW` around... - anywhere a node (or several sibling nodes)
/// that already exist stay byte-identical but gain a brand-new parent chain around them, with
/// nothing else about their own position among the *original* siblings changing).
///
/// Phase 1 already matches the reused content correctly (`Identical`, by hash) - this pass only
/// re-tags that match's `reason` so `ranges()` can recognize it as a verified pure repositioning,
/// gated by [`crate::diff::text::RenderOptions::paint_reindent_only_moves`] the same way
/// `solve_nested_condition_collapse` is (not unconditionally, like `solve_heritage_clause_growth` -
/// `rust-add-if`'s own hand-painted ground truth wants this shape painted `Move` under `Full` and
/// unpainted under `Minimal`, so both readings have to stay reachable). It never creates a new
/// mapping and never moves anything.
///
/// **The verification, in one sentence**: climbing up from the after-side node through only
/// brand-new ancestor levels (nothing else at any of those levels has an identity anywhere else in
/// `before` except other content that *also* came from the node's own original parent) must land
/// on a node already matched to that node's own real before-side parent.
///
/// **Why this is safe where two prior, more general attempts at the same idea were not** (see
/// `solve_heritage_clause_growth`'s own doc comment for their history): both of those keyed on
/// parent-match or sibling-adjacency alone, for *any* node, which is exactly what let `rust-add-if`
/// (a case that should still sometimes paint `Move`) and a JS destructuring rewrite's coincidental
/// duplicate literal slip through as false positives. This pass doesn't exclude `rust-add-if` by
/// node kind (unlike `solve_heritage_clause_growth`'s kind whitelist) - it includes it, correctly,
/// by gating the *rendering* consequence instead of the *tagging* decision: `rust-add-if` gets
/// tagged, but `ranges()` only acts on the tag under `Full`, matching what that fixture's own
/// ground truth wants either way. The sibling-purity check (`is_safe_wrapper_sibling`) is what
/// guards against the JS-destructuring-style false positive: a sibling with an identity elsewhere
/// in `before` that *isn't* the reused node's own original sibling disqualifies the whole climb.
pub fn solve(ctx: &PassCtx, diff: &mut ASTDiff) {
    let node_cache = ctx.node_cache;
    // `node_to_parent`, never `Node::parent()`: tree-sitter's parent lookup walks down from the
    // root (O(depth) per call), and this pass asks for a parent once per shifted node in the
    // file - on a 75k-node file that was half a million such walks, 10% of the whole diff
    // (callgrind, 2026-09-06). The metadata already holds every parent id.
    let before_parents = &ctx.before_metadata().node_to_parent;
    let after_parents = &ctx.after_metadata().node_to_parent;
    let candidates: Vec<(usize, usize)> = diff
        .mapping
        .iter()
        .filter(|(_, mapping)| mapping.operation == ASTMappingOperation::Identical)
        .map(|(&ids, _)| ids)
        .collect();

    for (before_id, after_id) in candidates {
        let Some(&before_node) = node_cache.before.get(&before_id) else {
            continue;
        };
        let Some(&after_node) = node_cache.after.get(&after_id) else {
            continue;
        };

        // Only a real re-tag if the node's position actually moved - see
        // `solve_heritage_clause_growth`'s identical guard for why.
        if before_node.start_position() == after_node.start_position() {
            continue;
        }
        // Leaves (bare keywords, punctuation, single tokens) are deliberately out of scope: a
        // leaf's own reindent verdict rarely drives `ranges()`'s Move/Identical choice for
        // anything a reader would notice on its own, and including them measurably regressed
        // `python-bugfix-loop` (its rewritten `for` header's `in` keyword got tagged, which
        // shifted an unrelated match's rendering boundary) for no corresponding gain anywhere in
        // the corpus - restricting to container nodes recovered it with no other change.
        if before_node.child_count() == 0 {
            continue;
        }

        let Some(&before_parent_id) = before_parents.get(&before_id) else {
            continue;
        };

        if !verify_pure_wrap(
            after_node,
            before_parent_id,
            diff,
            node_cache,
            before_parents,
            after_parents,
        ) {
            continue;
        }

        if let Some(mapping) = diff.mapping.get_mut(&(before_id, after_id)) {
            mapping.reason = ASTMappingReason::WrapGrowth;
        }
    }
}

/// How many ancestor levels above `after_node` this will climb before giving up - a real wrapper
/// (`try`/`catch`, an `if`/`else if`) is a handful of syntax levels at most; a bound here is pure
/// defensiveness against an unexpectedly deep or malformed tree, not a real limit any fixture in
/// the corpus needs raised.
const MAX_WRAP_DEPTH: usize = 6;

/// Climbs from `after_node` through unmatched ancestor levels, verifying at each one that every
/// *other* child is either genuinely new content or another piece of the same original container's
/// content that also relocated here - see the module doc comment. Stops as soon as it reaches an
/// ancestor that's already matched to something: success only if that something is exactly
/// `before_parent_id` (the wrapper was inserted *exactly* between the node and its real original
/// parent, nothing else changed structurally along the way).
fn verify_pure_wrap(
    after_node: Node,
    before_parent_id: usize,
    diff: &ASTDiff,
    node_cache: &NodeCache,
    before_parents: &rustc_hash::FxHashMap<usize, usize>,
    after_parents: &rustc_hash::FxHashMap<usize, usize>,
) -> bool {
    let mut current = after_node;
    for climbed in 0..MAX_WRAP_DEPTH {
        let Some(&parent) = after_parents
            .get(&current.id())
            .and_then(|id| node_cache.after.get(id))
        else {
            return false;
        };

        let mut cursor = parent.walk();
        for sibling in parent.children(&mut cursor) {
            if sibling.id() == current.id() {
                continue;
            }
            if !is_safe_wrapper_sibling(sibling, diff, before_parents, before_parent_id) {
                return false;
            }
        }

        if let Some(&matched_before_id) = diff.after_node_map.get(&parent.id())
            && matched_before_id != 0
        {
            // `climbed == 0` means `after_node`'s own immediate parent is already matched, with no
            // new wrapper level climbed through at all - an ordinary sibling shift (something
            // inserted *before* this node at the *same* level it already had), not a wrap. That
            // shape is the existing, deliberately-calibrated single-row column-shift territory
            // `ranges()` already owns (see its own doc comment on `column_shift_is_meaningful`) -
            // firing here too double-tagged it and, on `typescript-refactor-interface`, suppressed
            // a shift that fixture's own `Full` ground truth wants painted `Move`, regressing it
            // from ~0% to 75%. A wrap, by definition, needs at least one genuinely new level.
            return climbed > 0 && matched_before_id == before_parent_id;
        }
        // `parent` has no mapping at all because it's the after-tree's own root - no root is ever
        // individually recorded in `ASTDiff` (see `ASTDiff::is_complete`'s own root carve-out), so
        // there is no `(before_id, after_id)` entry to find no matter how far this climbs. A
        // top-level statement wrapped in a new `try`/`if` (e.g. `typescript-add-error-handling`'s
        // module-level statements, which have no enclosing `block` at all - their real parent
        // already *is* the file's root) needs this as its own success path: both files' roots
        // correspond to each other by construction, the same trivial correspondence a real mapping
        // entry would otherwise represent.
        if !after_parents.contains_key(&parent.id()) {
            return climbed > 0 && !before_parents.contains_key(&before_parent_id);
        }
        current = parent;
    }
    false
}

/// Whether `sibling` (some other child at a level this pass is climbing through) is consistent
/// with a *pure* wrap: either it carries no identity anywhere in `before` at all (a brand-new part
/// of the wrapper's own shell - a `catch` clause, a new `if`'s own condition and first branch), or
/// it's matched to a node whose own before-side parent is `before_parent_id` - i.e. it's *another*
/// piece of the same original container's content that got relocated into this same wrapper right
/// alongside the node this pass is actually verifying (the shape a multi-statement wrap, like
/// Java's `try` swallowing a whole run of a method's original statements, needs - each statement
/// is its own independently-matched `Identical` pair, and every one of them is the others'
/// `sibling` at the wrapper's body level).
///
/// A leaf (no children) is always safe without either check - punctuation and keywords carry no
/// identity to verify.
fn is_safe_wrapper_sibling(
    sibling: Node,
    diff: &ASTDiff,
    before_parents: &rustc_hash::FxHashMap<usize, usize>,
    before_parent_id: usize,
) -> bool {
    if sibling.child_count() == 0 {
        return true;
    }
    if let Some(&before_id) = diff.after_node_map.get(&sibling.id())
        && before_id != 0
    {
        return before_parents.get(&before_id) == Some(&before_parent_id);
    }
    // The sibling's own root carries no reused identity - every descendant must be equally free of
    // one, or something with a real identity elsewhere in `before` would be smuggled through as
    // "part of the wrapper's shell" when it's actually evidence of a more complex restructuring.
    let mut stack = vec![sibling];
    while let Some(node) = stack.pop() {
        if let Some(&before_id) = diff.after_node_map.get(&node.id())
            && before_id != 0
        {
            return false;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::Code;
    use crate::code::Language;
    use crate::diff::diff_code;
    use crate::test::helper::find_first_of_kind;

    /// `rust-add-if`'s own shape, minimized: an existing `if`/`else` becomes the `else if` branch
    /// of a brand-new outer `if`. The reused inner `if_expression` must be tagged `WrapGrowth`.
    #[test]
    fn an_existing_if_else_becoming_an_else_if_branch_is_tagged() {
        let before = Code::from_string(
            "fn f() {\n    if number % 2 == 0 {\n        even();\n    } else {\n        odd();\n    }\n}\n",
            &Language::Rust,
        );
        let after = Code::from_string(
            "fn f() {\n    if number == 0 {\n        zero();\n    } else if number % 2 == 0 {\n        even();\n    } else {\n        odd();\n    }\n}\n",
            &Language::Rust,
        );

        let diff = diff_code(&before, &after).ast.expect("ast diff");

        let before_root = before.ast.as_ref().unwrap().root_node();
        let before_if = find_first_of_kind(before_root, "if_expression").unwrap();
        let after_id = diff.before_node_map.get(&before_if.id()).copied();
        let mapping = after_id.and_then(|id| diff.mapping.get(&(before_if.id(), id)));

        assert_eq!(
            mapping.map(|m| &m.reason),
            Some(&ASTMappingReason::WrapGrowth),
            "the reused if/else must be tagged WrapGrowth, not left to a bare column-shift guess"
        );
    }

    /// Java's `try { EXISTING } catch (...) { NEW }` wrap: several sibling statements, not one
    /// node, all relocate together. Every one of the three top-level statements must be tagged -
    /// each is independently `Identical`-matched by phase 1, and each climbs to the same verified
    /// wrapper. Checked by counting `expression_statement` nodes specifically, not the mapping's
    /// total tagged count - that count also includes non-leaf descendants inside each statement
    /// (e.g. the `method_invocation` a leaf-only exclusion still leaves standing), which is fine
    /// but not what this test is about.
    #[test]
    fn a_run_of_statements_wrapped_in_a_new_try_block_are_all_tagged() {
        let before = Code::from_string(
            "class C {\n    void m() {\n        a();\n        b();\n        c();\n    }\n}\n",
            &Language::Java,
        );
        let after = Code::from_string(
            "class C {\n    void m() {\n        try {\n            a();\n            b();\n            c();\n        } catch (Exception e) {\n            handle(e);\n        }\n    }\n}\n",
            &Language::Java,
        );

        let diff = diff_code(&before, &after).ast.expect("ast diff");
        let after_root = after.ast.as_ref().unwrap().root_node();

        let tagged_statements = collect_of_kind(after_root, "expression_statement")
            .into_iter()
            .filter(|node| {
                diff.after_node_map
                    .get(&node.id())
                    .and_then(|&before_id| diff.mapping.get(&(before_id, node.id())))
                    .is_some_and(|m| m.reason == ASTMappingReason::WrapGrowth)
            })
            .count();
        assert_eq!(
            tagged_statements, 3,
            "all three reused statements (a(); b(); c();) should be tagged, not just one"
        );
    }

    fn collect_of_kind<'a>(root: Node<'a>, kind: &str) -> Vec<Node<'a>> {
        let mut result = Vec::new();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if node.kind() == kind {
                result.push(node);
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
        }
        result
    }

    /// A node whose content also genuinely changed (the condition, `% 2` -> `% 3`) is itself never
    /// an `Identical` candidate at all - its bytes differ, so this pass never even considers it.
    /// Whatever *does* get tagged here - if anything, depending on how far phase 1's matching
    /// descended given the changed condition - must still be byte-identical: the one property this
    /// test actually enforces, the same belt-and-suspenders guarantee
    /// `solve_heritage_clause_growth` checks for its own shape.
    #[test]
    fn only_byte_identical_content_is_ever_tagged_even_when_a_sibling_condition_changed() {
        let before_src = "fn f() {\n    if number % 2 == 0 {\n        even();\n    } else {\n        odd();\n    }\n}\n";
        let after_src = "fn f() {\n    if number == 0 {\n        zero();\n    } else if number % 3 == 0 {\n        even();\n    } else {\n        odd();\n    }\n}\n";
        let before = Code::from_string(before_src, &Language::Rust);
        let after = Code::from_string(after_src, &Language::Rust);

        let diff = diff_code(&before, &after).ast.expect("ast diff");
        let node_cache = NodeCache::build(&before, &after);

        let tagged: Vec<_> = diff
            .mapping
            .iter()
            .filter(|(_, m)| m.reason == ASTMappingReason::WrapGrowth)
            .collect();
        for (&(before_id, after_id), _) in tagged {
            let before_node = node_cache.before[&before_id];
            let after_node = node_cache.after[&after_id];
            assert_eq!(
                before_node.utf8_text(before_src.as_bytes()),
                after_node.utf8_text(after_src.as_bytes()),
                "a WrapGrowth-tagged pair must always be byte-identical"
            );
        }
    }
}
