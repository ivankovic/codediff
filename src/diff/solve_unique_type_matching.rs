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
use crate::diff::PassCtx;
use crate::diff::nodes::anchor_pair_via_apted;
use crate::diff::{ASTDiff, ASTMappingReason};
use std::collections::HashMap;

/// "Unique type matching", the third and last-resort sub-phase of GumTree Simple's recovery phase
/// (Falleri & Martinez, ICSE 2024, "Fine-grained, accurate and scalable source differencing" -
/// itself inspired by XYDiff's type-matching step) - see `TODO.md`'s 2026-08-17 literature survey
/// for the corpus-wide cross-check that motivated adding this. Not present anywhere in codediff
/// before this: exact-subtree isomorphism is `solve_hash_descent`, and structural isomorphism
/// ignoring leaf values is codediff's own `KindOnlyHash` sub-anchoring - this pass is the one piece
/// of GumTree Simple's recovery phase that had no equivalent here.
///
/// For every currently-matched `(before_id, after_id)` pair, look at that pair's own direct
/// children still unmatched on both sides. If exactly one before-child and exactly one after-child
/// share the same node *kind*, pair them - real bounded APTED (`anchor_pair_via_apted`) then scores
/// and resolves everything inside that pair, so the cost/operation is never invented, only the
/// pairing decision is a heuristic.
///
/// Deliberately conservative: a kind with zero or two-or-more unmatched candidates on either side
/// is left alone (no plurality vote, no "closest" guess) - the whole value of this heuristic is
/// that "exactly one of this kind on each side, under a parent we already know corresponds" is
/// unambiguous by construction, not merely likely.
///
/// Single pass over a snapshot of currently-matched pairs (not recursive to a fixed point, unlike
/// the literature's own placement inside a recursive bottom-up walk) - simpler, and consistent with
/// this codebase's other single-shot passes; newly-resolved pairs from a first call could in
/// principle unlock more unique-type matches among *their own* still-unmatched children, but that
/// is left for a later iteration if the full corpus benchmark shows it matters, rather than assumed
/// upfront.
///
/// Runs after `solve_bottom_up_propagation` (so it benefits from every parent pairing that pass
/// resolves) and before the terminal Myers-LCS fallback (`apted::for_roots_fallback`), so this
/// pass's precise, cheap pairs are locked in before that lossy, whole-subtree-only catch-all ever
/// sees them.
pub fn solve(ctx: &PassCtx, diff: &mut ASTDiff) {
    let before_metadata = ctx.before_metadata();
    let after_metadata = ctx.after_metadata();

    // Snapshot of currently-matched pairs, sorted by the before node's preorder index for
    // deterministic iteration order regardless of `FxHashMap`'s own iteration order - see
    // `ASTNodeMetadata::start_byte`'s doc comment on why node ids themselves aren't a safe sort key.
    // Only pairs that actually have an unmatched child on the before side can produce anything
    // here, and on a large, mostly-unchanged file almost none do - filtering before the sort keeps
    // this proportional to the residual rather than to the whole file (measured 2026-08-17: ~60ms
    // of a 907ms diff on a 258k-node fixture, for a pass that fires zero times corpus-wide, see
    // this module's `TODO.md` entry). The filter is a necessary condition for the per-kind loop
    // below, which re-checks everything properly; matching only ever adds map entries, so a child
    // unmatched now can only become matched later, never the reverse.
    let mut matched_pairs: Vec<(usize, usize)> = diff
        .before_node_map
        .iter()
        .filter_map(|(&before_id, &after_id)| (after_id != 0).then_some((before_id, after_id)))
        .filter(|(before_id, _)| {
            before_metadata
                .node_info
                .get(before_id)
                .is_some_and(|info| {
                    info.children
                        .iter()
                        .any(|child| !diff.before_node_map.contains_key(child))
                })
        })
        .collect();
    matched_pairs.sort_unstable_by_key(|&(before_id, _)| {
        before_metadata
            .node_info
            .get(&before_id)
            .map(|info| info.preorder_index)
            .unwrap_or(usize::MAX)
    });

    for (before_id, after_id) in matched_pairs {
        let Some(before_info) = before_metadata.node_info.get(&before_id) else {
            continue;
        };
        let Some(after_info) = after_metadata.node_info.get(&after_id) else {
            continue;
        };
        if before_info.children.is_empty() || after_info.children.is_empty() {
            continue;
        }

        let mut before_by_kind: HashMap<&str, Vec<usize>> = HashMap::new();
        for &child_id in &before_info.children {
            if diff.before_node_map.contains_key(&child_id) {
                continue;
            }
            if let Some(child_info) = before_metadata.node_info.get(&child_id) {
                before_by_kind
                    .entry(child_info.kind.as_str())
                    .or_default()
                    .push(child_id);
            }
        }
        if before_by_kind.is_empty() {
            continue;
        }

        let mut after_by_kind: HashMap<&str, Vec<usize>> = HashMap::new();
        for &child_id in &after_info.children {
            if diff.after_node_map.contains_key(&child_id) {
                continue;
            }
            if let Some(child_info) = after_metadata.node_info.get(&child_id) {
                after_by_kind
                    .entry(child_info.kind.as_str())
                    .or_default()
                    .push(child_id);
            }
        }

        // Sorted by kind name for deterministic pairing order within this parent, independent of
        // `HashMap` iteration order.
        let mut kinds: Vec<&str> = before_by_kind.keys().copied().collect();
        kinds.sort_unstable();

        for kind in kinds {
            let Some(before_candidates) = before_by_kind.get(kind) else {
                continue;
            };
            let Some(after_candidates) = after_by_kind.get(kind) else {
                continue;
            };
            let ([before_child_id], [after_child_id]) =
                (before_candidates.as_slice(), after_candidates.as_slice())
            else {
                continue;
            };
            // A pass earlier in this same loop (a sibling under the same parent, different kind)
            // cannot have touched this child - kinds partition a parent's children - but a nested
            // call could in principle be reached twice if this function is ever made recursive;
            // re-check both sides are still unmatched immediately before committing, cheap and safe.
            if diff.before_node_map.contains_key(before_child_id)
                || diff.after_node_map.contains_key(after_child_id)
            {
                continue;
            }
            anchor_pair_via_apted(
                *before_child_id,
                *after_child_id,
                before_metadata,
                after_metadata,
                "unique_type_matching",
                ASTMappingReason::UniqueTypeMatching,
                diff,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::Code;
    use crate::code::Language;
    use crate::diff::ASTMapping;
    use crate::diff::NodeCache;

    /// Manually marks `before_id`/`after_id` as matched (an arbitrary placeholder mapping, not
    /// produced by any real pass) and runs *only* `solve` - no `solve_hash_descent` or any other
    /// pass in the setup path, so a passing assertion can only be this pass's own doing, not an
    /// accident of an earlier, more powerful mechanism (`solve_hash_descent`'s `KindOnlyHash`
    /// sub-anchoring already covers same-*shape* subtrees differing only in leaf values - a test
    /// fixture whose children differ only by an identifier, as an earlier draft of this test used,
    /// would pass via that mechanism with this pass never actually running, silently proving
    /// nothing).
    fn solve_with_container_pre_matched(
        before: &Code,
        after: &Code,
        container_before_id: usize,
        container_after_id: usize,
    ) -> ASTDiff {
        let node_cache = NodeCache::build(before, after);
        let mut diff = ASTDiff::default();
        diff.add_mapping(
            container_before_id,
            container_after_id,
            ASTMapping::identical(ASTMappingReason::IdenticalHash),
        );
        solve(
            &crate::diff::PassCtx::new(before, after, &node_cache),
            &mut diff,
        );
        diff
    }

    /// The if-node's *internal shape* differs between before/after (an extra statement in the
    /// after-side body), not just a leaf value - `KindOnlyHash` would not match these two subtrees,
    /// so only this pass's coarser "same kind, unique on both sides" rule can pair them. Pre-matches
    /// the *block* (the if-node's direct parent), not the outer function: pre-matching the function
    /// instead would let this pass first match the block itself (also a unique-count-1 child of the
    /// function), and the real APTED call that match triggers would then recursively resolve the
    /// if-node too, as an internal side effect - correct, but not what this test means to isolate
    /// (`ASTMappingReason::APTED("unique_type_matching")`, not `UniqueTypeMatching` directly).
    #[test]
    fn unique_leftover_child_kind_matches_under_an_already_matched_parent() {
        let before = Code::from_string(
            "fn container() {\n    if flag { x(); }\n}\n",
            &Language::Rust,
        );
        let after = Code::from_string(
            "fn container() {\n    if flag { x(); y(); }\n}\n",
            &Language::Rust,
        );

        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();
        let before_block = before_ast
            .root_node()
            .child(0)
            .unwrap()
            .child_by_field_name("body")
            .unwrap();
        let after_block = after_ast
            .root_node()
            .child(0)
            .unwrap()
            .child_by_field_name("body")
            .unwrap();
        let before_if = before_block.named_child(0).unwrap();
        let after_if = after_block.named_child(0).unwrap();
        assert_eq!(before_if.kind(), after_if.kind(), "test setup sanity check");

        let diff =
            solve_with_container_pre_matched(&before, &after, before_block.id(), after_block.id());

        assert_eq!(
            diff.before_node_map.get(&before_if.id()).copied(),
            Some(after_if.id()),
            "the sole leftover if-node on each side under the already-matched block should \
             pair via unique-type matching, not fall through unmatched"
        );
        assert_eq!(
            diff.mapping
                .get(&(before_if.id(), after_if.id()))
                .map(|m| m.reason),
            Some(ASTMappingReason::UniqueTypeMatching),
            "the pairing decision itself should be attributed to this pass"
        );
    }

    /// Two candidates of the same kind on one side must block the match - no arbitrary pick.
    /// Pre-matches the *block* directly (see the previous test's doc comment for why the outer
    /// function must not be used here): a block pre-match makes the two/one `let_declaration`
    /// candidates this pass's own direct decision, with nothing else able to resolve them first.
    #[test]
    fn ambiguous_multiple_candidates_of_the_same_kind_do_not_match() {
        let before = Code::from_string(
            "fn container() {\n    let a = 1;\n    let b = 2;\n}\n",
            &Language::Rust,
        );
        let after = Code::from_string("fn container() {\n    let x = 9;\n}\n", &Language::Rust);

        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();
        let before_block = before_ast
            .root_node()
            .child(0)
            .unwrap()
            .child_by_field_name("body")
            .unwrap();
        let after_block = after_ast
            .root_node()
            .child(0)
            .unwrap()
            .child_by_field_name("body")
            .unwrap();
        let first_let = before_block.named_child(0).unwrap();
        let second_let = before_block.named_child(1).unwrap();

        let diff =
            solve_with_container_pre_matched(&before, &after, before_block.id(), after_block.id());

        assert!(
            !diff.before_node_map.contains_key(&first_let.id())
                && !diff.before_node_map.contains_key(&second_let.id()),
            "two unmatched `let` statements on the before side is ambiguous against one on the \
             after side - neither should be force-matched"
        );
    }
}
