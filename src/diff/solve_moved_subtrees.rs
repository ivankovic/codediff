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

//! Phase 7: pairs byte-identical subtrees between the wholly-deleted and wholly-inserted sets -
//! code that *moved* across a matched boundary, which ordered tree edit distance can only express
//! as delete+insert. GumTree's "recovery mappings", run after every matching pass so it only ever
//! converts leftovers and never takes a node from a better mapping.
//!
//! Guardrails:
//!
//! - Only a *fully*-deleted subtree pairs with a *fully*-inserted one; a matched descendant already
//!   has a footprint in the other tree whose ancestry a remap could contradict.
//! - Largest first, claiming whole subtrees, so a moved function moves as one piece.
//! - Several identical targets for a small subtree is a coin flip, so it is refused unless
//!   `disambiguate_by_context` finds one clearly better surrounding.
//! - The outermost unmapped reference-node ancestors on both sides must have the same kind: a
//!   move into a renamed `impl` is the same construct reshaped (rust-turbopack-module-rule), but
//!   an expression resurfacing inside a new construct of another kind reads as new code
//!   (kotlin-refactor-function).

use std::collections::HashSet;

use crate::code::{ASTMetadata, Language};
use crate::diff::PassCtx;
use crate::diff::nodes::is_reference;
use crate::diff::{ASTDiff, ASTMapping, ASTMappingReason};

/// Minimum subtree size (node count, incl. the root) for a move. Below this, identical subtrees
/// are commodity code (`return None`, `i += 1`) whose pairing is coincidence more often than intent.
const MIN_MOVE_SUBTREE_SIZE: usize = 4;

/// Size at or above which an *ambiguous* move (several identical targets) is trusted anyway.
/// Below it an identical subtree is a `self.foo` or a bare string; above it, several copies moving
/// is more likely a genuine reorder (a data file's rows) than a coincidence.
const AMBIGUOUS_MOVE_MIN_SIZE: usize = 8;

pub fn solve(ctx: &PassCtx, diff: &mut ASTDiff) {
    let (before, node_cache) = (ctx.before, ctx.node_cache);
    let language = before.metadata.language.unwrap_or_default();
    let before_metadata = ctx.before_metadata();
    let after_metadata = ctx.after_metadata();
    let before_parents = &before_metadata.node_to_parent;
    let after_parents = &after_metadata.node_to_parent;

    // Ties broken by `start_byte`, not node id: ids are arena slots that are not stable across
    // parses, so only a source-position tiebreak is reproducible across process runs.
    let mut deleted: Vec<(usize, usize, usize)> = diff
        .before_node_map
        .iter()
        .filter(|&(_, &target)| target == 0)
        .filter_map(|(&b, _)| {
            let size = before_metadata.node_to_subtree_size.get(&b).copied()?;
            let start_byte = before_metadata.node_info.get(&b)?.start_byte;
            (size >= MIN_MOVE_SUBTREE_SIZE).then_some((size, start_byte, b))
        })
        .collect();
    deleted.sort_unstable_by(|x, y| y.0.cmp(&x.0).then(x.1.cmp(&y.1)));

    let mut claimed_before: HashSet<usize> = HashSet::new();
    let mut claimed_after: HashSet<usize> = HashSet::new();

    for (_, _, b) in deleted {
        if claimed_before.contains(&b) {
            continue;
        }
        if !subtree_fully_unmapped(b, before_metadata, &diff.before_node_map) {
            continue;
        }
        let Some(hash) = before_metadata.node_to_full_hash.get(&b) else {
            continue;
        };
        let Some(candidates) = after_metadata.full_hash_to_node.get(hash) else {
            continue;
        };

        // Document order, so the earliest of several equally-valid targets wins.
        let mut candidates: Vec<usize> = candidates
            .iter()
            .copied()
            .filter(|a| {
                !claimed_after.contains(a)
                    && diff.after_node_map.get(a) == Some(&0)
                    && subtree_fully_unmapped(*a, after_metadata, &diff.after_node_map)
            })
            .collect();
        candidates.sort_unstable_by_key(|a| {
            node_cache
                .after
                .get(a)
                .map(|n| n.start_byte())
                .unwrap_or(usize::MAX)
        });
        // Ambiguity guard: among several identical targets the document-order pick is a guess
        // (a Python `self.foo` occurs dozens of times per file). Not the same question as
        // `MIN_MOVE_SUBTREE_SIZE`: raising that instead would also discard unambiguous small moves.
        // Pinned by python-django-django-update-unit-tests-actual-logic-change.
        if candidates.len() > 1
            && before_metadata
                .node_to_subtree_size
                .get(&b)
                .copied()
                .unwrap_or(0)
                < AMBIGUOUS_MOVE_MIN_SIZE
        {
            match disambiguate_by_context(b, &candidates, before_metadata, after_metadata) {
                Some(best) => candidates = vec![best],
                None => continue,
            }
        }
        let source_container = outermost_unmapped_reference_kind(
            b,
            before_metadata,
            before_parents,
            &diff.before_node_map,
            &language,
        );
        let Some(&a) = candidates.iter().find(|&&a| {
            let target_container = outermost_unmapped_reference_kind(
                a,
                after_metadata,
                after_parents,
                &diff.after_node_map,
                &language,
            );
            source_container == target_container
        }) else {
            continue;
        };

        remap_moved_subtree(b, a, before_metadata, after_metadata, diff);
        claim_subtree(b, before_metadata, &mut claimed_before);
        claim_subtree(a, after_metadata, &mut claimed_after);
    }
}

/// How much more similar the winner's surroundings must be than the runner-up's. A margin, not a
/// threshold: the question is whether one container is clearly the right one.
const CONTEXT_TIEBREAK_MARGIN: f32 = 0.15;

/// Candidate count above which the tie-break refuses without scoring. A pure cost bound on
/// commodity hashes (`,`, `self`), set where it matches an uncapped search in quality.
const MAX_AMBIGUOUS_CANDIDATES: usize = 32;

/// Picks the one candidate whose *parent* is clearly the most similar to `source`'s parent, or
/// `None` when no candidate stands out. The candidates themselves are identical (shared full
/// hash), so only their surroundings carry evidence; sketches compare them without a subtree walk.
fn disambiguate_by_context(
    source: usize,
    candidates: &[usize],
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
) -> Option<usize> {
    if candidates.len() > MAX_AMBIGUOUS_CANDIDATES {
        return None;
    }
    let source_parent = before_metadata.node_to_parent.get(&source)?;
    let source_sketch = before_metadata
        .node_to_similarity_sketch
        .get(source_parent)?;

    let mut scored: Vec<(f32, usize)> = candidates
        .iter()
        .filter_map(|&candidate| {
            let parent = after_metadata.node_to_parent.get(&candidate)?;
            let sketch = after_metadata.node_to_similarity_sketch.get(parent)?;
            Some((source_sketch.jaccard(sketch), candidate))
        })
        .collect();
    if scored.len() < 2 {
        return None;
    }

    scored.sort_by(|x, y| y.0.total_cmp(&x.0));
    (scored[0].0 - scored[1].0 >= CONTEXT_TIEBREAK_MARGIN).then_some(scored[0].1)
}

/// The kind of the outermost reference node (see `is_reference`) on `node`'s unmapped ancestor
/// chain, `node` included. Stops at the first mapped ancestor, since a container that visibly
/// survived is not the one the content left. `None` when the chain holds no reference node.
fn outermost_unmapped_reference_kind<'m>(
    node: usize,
    meta: &'m ASTMetadata,
    parents: &rustc_hash::FxHashMap<usize, usize>,
    node_map: &rustc_hash::FxHashMap<usize, usize>,
    language: &Language,
) -> Option<&'m str> {
    let mut outermost = None;
    let mut cur = node;
    loop {
        if node_map.get(&cur).copied().unwrap_or(1) != 0 && cur != node {
            break;
        }
        if let Some(info) = meta.node_info.get(&cur)
            && is_reference(&info.kind, language)
        {
            outermost = Some(info.kind.as_str());
        }
        match parents.get(&cur) {
            Some(&p) => cur = p,
            None => break,
        }
    }
    outermost
}

fn subtree_fully_unmapped(
    root: usize,
    meta: &ASTMetadata,
    node_map: &rustc_hash::FxHashMap<usize, usize>,
) -> bool {
    if node_map.get(&root) != Some(&0) {
        return false;
    }
    let Some(info) = meta.node_info.get(&root) else {
        return true;
    };
    info.children
        .iter()
        .all(|&child| subtree_fully_unmapped(child, meta, node_map))
}

fn claim_subtree(root: usize, meta: &ASTMetadata, claimed: &mut HashSet<usize>) {
    claimed.insert(root);
    if let Some(info) = meta.node_info.get(&root) {
        for &child in &info.children {
            claim_subtree(child, meta, claimed);
        }
    }
}

/// Maps two identical subtrees pairwise in lockstep; identical full hashes guarantee identical
/// shape, so children line up 1:1.
fn remap_moved_subtree(
    b: usize,
    a: usize,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    diff: &mut ASTDiff,
) {
    diff.remove_delete_mapping(b);
    diff.remove_insert_mapping(a);
    diff.add_mapping(b, a, ASTMapping::identical(ASTMappingReason::MovedSubtree));

    let b_children = before_meta
        .node_info
        .get(&b)
        .map(|i| i.children.clone())
        .unwrap_or_default();
    let a_children = after_meta
        .node_info
        .get(&a)
        .map(|i| i.children.clone())
        .unwrap_or_default();
    for (cb, ca) in b_children.into_iter().zip(a_children) {
        remap_moved_subtree(cb, ca, before_meta, after_meta, diff);
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_AMBIGUOUS_CANDIDATES, disambiguate_by_context};
    use crate::code::similarity::SimilaritySketch;
    use crate::code::{ASTMetadata, Code, Language};
    use crate::diff::diff_code;

    /// A whole function moving across another (unchanged) function must come out as matched
    /// content, not a delete+insert pair.
    #[test]
    fn moved_function_is_matched_not_deleted() {
        let before = Code::from_string(
            "fn moved_one(x: i64, y: i64) -> i64 { let q = x * y; q + x }\nfn stay() {}\n",
            &Language::Rust,
        );
        let after = Code::from_string(
            "fn stay() {}\nfn moved_one(x: i64, y: i64) -> i64 { let q = x * y; q + x }\n",
            &Language::Rust,
        );

        let diff = diff_code(&before, &after);
        let ast = diff.ast.unwrap();

        // No node on either side may remain deleted/inserted: both functions exist on both sides.
        let deleted: Vec<_> = ast
            .before_node_map
            .iter()
            .filter(|&(_, &t)| t == 0)
            .collect();
        let inserted: Vec<_> = ast
            .after_node_map
            .iter()
            .filter(|&(_, &t)| t == 0)
            .collect();
        assert!(
            deleted.is_empty() && inserted.is_empty(),
            "moved function should be fully matched, found {} deletes / {} inserts",
            deleted.len(),
            inserted.len()
        );
    }

    /// Two tiny identical statements in unrelated functions must NOT be "moved" onto each other -
    /// the size floor keeps commodity code out of move detection.
    #[test]
    fn tiny_identical_statements_do_not_move() {
        let before = Code::from_string("fn a() { let x = 1; }\nfn c() {}\n", &Language::Rust);
        let after = Code::from_string("fn c() {}\nfn d() { let x = 1; }\n", &Language::Rust);

        let diff = diff_code(&before, &after);
        let ast = diff.ast.unwrap();

        // The differing names keep `fn a` and `fn d` unmatched, so only this pass could pair
        // their shared `let x = 1;`.
        let has_move = ast
            .mapping
            .values()
            .any(|m| m.reason == crate::diff::ASTMappingReason::MovedSubtree);
        assert!(
            !has_move,
            "tiny identical statements must not be paired as moves"
        );
    }

    /// A small subtree with *several* identical candidates on the other side has no honest answer
    /// to "which one did it move to", so no move may be recorded - see the ambiguity guard in
    /// `solve`. Here the deleted function's `self.log(x)` call could equally have "moved" into
    /// either of the two inserted functions that contain the very same call.
    #[test]
    fn ambiguous_small_moves_are_refused_rather_than_guessed() {
        let before = Code::from_string(
            "class A:\n    def gone(self, x):\n        self.log(x)\n",
            &Language::Python,
        );
        let after = Code::from_string(
            "class A:\n    def one(self, x):\n        self.log(x)\n\n    def two(self, x):\n        self.log(x)\n",
            &Language::Python,
        );

        let diff = diff_code(&before, &after);
        let ast = diff.ast.unwrap();

        let has_move = ast
            .mapping
            .values()
            .any(|m| m.reason == crate::diff::ASTMappingReason::MovedSubtree);
        assert!(
            !has_move,
            "with two equally good targets, no move should be invented"
        );
    }

    /// Hand-built metadata for `disambiguate_by_context`: an end-to-end test could not prove which
    /// pass produced a pairing. Only the two maps it reads are populated.
    fn metadata_with(parents: &[(usize, usize)], sketches: &[(usize, &[u64])]) -> ASTMetadata {
        let mut metadata = ASTMetadata::default();
        for &(child, parent) in parents {
            metadata.node_to_parent.insert(child, parent);
        }
        for &(node, leaves) in sketches {
            metadata.node_to_similarity_sketch.insert(
                node,
                SimilaritySketch::merge(leaves.iter().map(|&h| SimilaritySketch::leaf(h))),
            );
        }
        metadata
    }

    #[test]
    fn context_tiebreak_picks_the_candidate_in_the_more_familiar_surroundings() {
        let before = metadata_with(&[(1, 10)], &[(10, &[1, 2, 3])]);
        let after = metadata_with(&[(2, 20), (3, 30)], &[(20, &[1, 2, 3]), (30, &[7, 8, 9])]);
        assert_eq!(
            disambiguate_by_context(1, &[2, 3], &before, &after),
            Some(2)
        );
    }

    #[test]
    fn context_tiebreak_refuses_when_the_surroundings_are_equally_alike() {
        let before = metadata_with(&[(1, 10)], &[(10, &[1, 2, 3])]);
        let after = metadata_with(&[(2, 20), (3, 30)], &[(20, &[1, 2, 3]), (30, &[1, 2, 3])]);
        assert_eq!(disambiguate_by_context(1, &[2, 3], &before, &after), None);
    }

    #[test]
    fn context_tiebreak_refuses_a_near_tie() {
        // Jaccard 1.00 against 0.91: a bare argmax would pair, the margin refuses.
        let ten: Vec<u64> = (1..=10).collect();
        let eleven: Vec<u64> = (1..=11).collect();
        let before = metadata_with(&[(1, 10)], &[(10, &ten)]);
        let after = metadata_with(&[(2, 20), (3, 30)], &[(20, &ten), (30, &eleven)]);
        assert_eq!(disambiguate_by_context(1, &[2, 3], &before, &after), None);
    }

    #[test]
    fn context_tiebreak_declines_to_rank_a_crowd_of_commodity_tokens() {
        // The first candidate would win clearly, but the crowd is over the cap.
        let before = metadata_with(&[(1, 10)], &[(10, &[1, 2, 3])]);
        let candidates: Vec<usize> = (100..100 + MAX_AMBIGUOUS_CANDIDATES + 1).collect();
        let parents: Vec<(usize, usize)> = candidates.iter().map(|&c| (c, c + 1_000)).collect();
        let mut sketches: Vec<(usize, &[u64])> = candidates
            .iter()
            .map(|&c| (c + 1_000, &[7, 8, 9][..]))
            .collect();
        sketches[0] = (candidates[0] + 1_000, &[1, 2, 3][..]);
        let after = metadata_with(&parents, &sketches);
        assert_eq!(
            disambiguate_by_context(1, &candidates, &before, &after),
            None
        );
    }
}
