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

//! MoveDetectionRecovery: phase 7 of the pipeline, running after the terminal residual
//! resolution has decided every match (only the re-tagging phases 8-10 come after it). Scans what got wholly deleted and wholly inserted and pairs
//! up byte-identical subtrees between the two sets - code that didn't change but *moved*, which
//! plain ordered tree edit distance structurally cannot express as anything but delete+insert
//! whenever the move crosses a matched boundary (the classic case: a subtree relocating into a
//! newly-inserted wrapper, or into a container whose own identity changed - see the
//! rust-turbopack-module-rule analysis in TODO.md).
//!
//! This is GumTree's "recovery mappings" phase adapted to this pipeline. It deliberately runs
//! after every matching pass: each of them (including the DP) gets first claim on all content, so this can
//! only ever convert delete+insert leftovers into matches - it can never take a node away from a
//! better mapping.
//!
//! Guardrails, each protecting against a previously-observed failure mode:
//!
//! - Only *fully*-deleted subtrees may pair with *fully*-inserted ones. A subtree with even one
//!   matched descendant already has a footprint in the other tree; re-mapping it here could
//!   contradict that footprint's ancestry.
//! - Only subtrees of at least `MIN_MOVE_SUBTREE_SIZE` nodes participate. Hash-matching arbitrary
//!   small nodes re-creates the "stray `;` matches a random other `;`" disease this project spent
//!   the generic-token gate curing; small identical statements (`return None`, `i += 1`) are so
//!   common that pairing them across unrelated regions is noise, not signal.
//! - Largest-first with claiming: a moved function claims its whole subtree in one piece, rather
//!   than its statements each finding their own (possibly different) partners.
//! - Ambiguity refusal below `AMBIGUOUS_MOVE_MIN_SIZE`: when several available targets spell
//!   exactly the same thing, a small subtree's "move" is a coin flip between commodity tokens, so
//!   no pairing is made at all. Above that size the repetition is distinctive enough to trust.
//!   Before refusing, `disambiguate_by_context` gets a chance to decide the question honestly by
//!   comparing the candidates' *surroundings* - identical subtrees can still sit in tellingly
//!   different containers.
//! - Container-identity agreement: the outermost deleted reference-node ancestor of the source
//!   and the outermost inserted reference-node ancestor of the target must have the same kind.
//!   Humans read a move relative to the construct it left and the construct it entered: content
//!   relocating from a deleted `impl` into an inserted (renamed) `impl` is "the same impl,
//!   reshaped, contents moved" (rust-turbopack-module-rule's ground truth maps it), but an
//!   expression re-appearing inside a brand-new construct of a *different* kind - a free
//!   function's `width * height` re-surfacing inside a new `data class`'s method - reads as new
//!   code that happens to spell the same (kotlin-refactor-function's ground truth deletes it).

use std::collections::HashSet;

use crate::code::{ASTMetadata, Code, Language};
use crate::diff::nodes::is_reference;
use crate::diff::{ASTDiff, ASTMapping, ASTMappingReason, NodeCache};

/// Minimum subtree size (node count, incl. the root) for a delete/insert pair to be considered a
/// move. Below this, identical subtrees are commodity code (single tokens, trivial statements)
/// whose pairing is coincidence more often than intent.
const MIN_MOVE_SUBTREE_SIZE: usize = 4;

/// Size at or above which an *ambiguous* move (several available targets spelling exactly the same
/// thing) is trusted anyway - see the guard's own comment at the use site.
///
/// Swept 6/8/unbounded against the full corpus: 8 is the best of the three (-38 mismatches, vs.
/// -32 at 6 and +5 with no size gate at all - an unbounded ambiguity guard also loses
/// `java-genymobile-scrcpy-refactor-for-loop-in-a-function` and doubles `tsx-excalidraw`'s
/// regression). The shape it separates is real rather than fitted: below ~8 nodes an identical
/// subtree is a `self.foo`, a `(self)`, a bare string; at 8 and up it is a distinct enough
/// construct that several copies of it moving is more likely a genuine reorder than a coincidence.
const AMBIGUOUS_MOVE_MIN_SIZE: usize = 8;

pub fn solve(before: &Code, after: &Code, node_cache: &NodeCache, diff: &mut ASTDiff) {
    let language = before.metadata.language.unwrap_or_default();
    let before_metadata = crate::code::metadata::metadata_of(before);
    let after_metadata = crate::code::metadata::metadata_of(after);
    let before_parents = &before_metadata.node_to_parent;
    let after_parents = &after_metadata.node_to_parent;

    // Every candidate on the deleted side, largest subtree first, so a moved container is
    // re-mapped as one piece before its own children are considered separately. Ties broken by
    // `start_byte` (document position), not `node_id`: `diff.before_node_map` is a `HashMap`, so
    // the source order here is already hash-seeded, and node ids are arena slots that aren't
    // stable across separate parses of identical source - only a source-position tiebreak keeps
    // the result reproducible across process runs, not just within one.
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
        if !subtree_fully_unmapped(b, &before_metadata, &diff.before_node_map) {
            continue;
        }
        let Some(hash) = before_metadata.node_to_full_hash.get(&b) else {
            continue;
        };
        let Some(candidates) = after_metadata.full_hash_to_node.get(hash) else {
            continue;
        };

        // `full_hash_to_node`'s candidates already arrive in a deterministic order (see its doc
        // comment), but not a document-position one - re-order by document position so that, among
        // several equally-valid move targets, the earliest one in the file wins, which is what a
        // human skimming the diff would expect.
        let mut candidates: Vec<usize> = candidates
            .iter()
            .copied()
            .filter(|a| {
                !claimed_after.contains(a)
                    && diff.after_node_map.get(a) == Some(&0)
                    && subtree_fully_unmapped(*a, &after_metadata, &diff.after_node_map)
            })
            .collect();
        candidates.sort_unstable_by_key(|a| {
            node_cache
                .after
                .get(a)
                .map(|n| n.start_byte())
                .unwrap_or(usize::MAX)
        });
        // Ambiguity guard: with more than one available target spelling exactly the same thing,
        // "which one did this move to" has no answer, and the document-position sort above is a
        // guess dressed as a decision. Commodity code is precisely where this bites - a Python
        // `self.foo` or `(self)` occurs dozens of times per file, so an identical hash carries no
        // information about relocation at all. All 48 of `python-django-django-update-unit-tests-
        // actual-logic-change`'s mismatches were this: `string`, `attribute` and `parameters`
        // subtrees paired across unrelated classes. It is **zero** with this guard.
        //
        // Only below `AMBIGUOUS_MOVE_MIN_SIZE`, because ambiguity stops meaning coincidence once a
        // subtree is big enough to be distinctive - a data file's repeated rows really do move, and
        // refusing to pick among them costs more than guessing (measured; see that constant).
        //
        // Note this is *not* the same question `MIN_MOVE_SUBTREE_SIZE` asks. That one gates whether
        // a subtree is worth considering at all; this one gates whether a *choice between several
        // equals* can be made honestly. Raising `MIN_MOVE_SUBTREE_SIZE` instead was tried and
        // rejected (see its own doc comment): it discards unambiguous small moves too, regressing
        // 11 fixtures to fix one.
        //
        // Before refusing outright, the sketch gets a chance to make the choice honestly: the
        // candidates are identical to each other by construction (they share a full hash), but
        // their *surroundings* need not be, and a small subtree that moved into a container much
        // like the one it left is a better answer than no answer at all.
        if candidates.len() > 1
            && before_metadata
                .node_to_subtree_size
                .get(&b)
                .copied()
                .unwrap_or(0)
                < AMBIGUOUS_MOVE_MIN_SIZE
        {
            match disambiguate_by_context(b, &candidates, &before_metadata, &after_metadata) {
                Some(best) => candidates = vec![best],
                None => continue,
            }
        }
        let source_container = outermost_unmapped_reference_kind(
            b,
            &before_metadata,
            before_parents,
            &diff.before_node_map,
            &language,
        );
        let Some(&a) = candidates.iter().find(|&&a| {
            let target_container = outermost_unmapped_reference_kind(
                a,
                &after_metadata,
                after_parents,
                &diff.after_node_map,
                &language,
            );
            source_container == target_container
        }) else {
            continue;
        };

        remap_moved_subtree(b, a, &before_metadata, &after_metadata, diff);
        claim_subtree(b, &before_metadata, &mut claimed_before);
        claim_subtree(a, &after_metadata, &mut claimed_after);
    }
}

/// How much more similar the winning candidate's surroundings must be than the runner-up's before
/// the ambiguity guard will accept its verdict. A margin, not a threshold: the question is never
/// "is this container similar enough" but "is one of these containers clearly the right one".
const CONTEXT_TIEBREAK_MARGIN: f32 = 0.15;

/// Candidate count above which the tie-break is not even attempted, and the guard refuses as it
/// did before.
///
/// A pure cost bound, not a quality/cost tradeoff. A commodity hash (a `,`, a `;`, `self`) has
/// hundreds of candidates, and scoring all of them for every deleted node made the whole corpus
/// ~8% slower (measured 2026-08-18: `rust-rustdesk-...-io-loop` 2218ms -> 2405ms, and the same
/// ~10-20% on every large fixture). Swept 8/32/uncapped: 8 costs 4 mismatches, **32 matches
/// uncapped exactly** - no fixture in the corpus needs more than 32 candidates - so 32 is simply
/// the smallest cap tried that loses nothing.
const MAX_AMBIGUOUS_CANDIDATES: usize = 32;

/// Picks the one candidate whose *parent* is clearly the most similar to `source`'s parent, or
/// `None` when no candidate stands out.
///
/// The candidates all share `source`'s full hash, so comparing the candidates themselves is
/// worthless - they are identical. Their parents are not, and
/// [`crate::code::similarity::SimilaritySketch`] can compare two of them in O(k) without walking
/// either subtree, which is what makes this affordable inside the candidate loop.
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

    // Descending by score; `candidates` is already in document order, so the `total_cmp` tie-break
    // below leaves equal scores in that order and the margin check then rejects them anyway.
    scored.sort_by(|x, y| y.0.total_cmp(&x.0));
    (scored[0].0 - scored[1].0 >= CONTEXT_TIEBREAK_MARGIN).then_some(scored[0].1)
}

/// The kind of the outermost reference node (see `is_reference`) on `node`'s still-unmapped
/// ancestor chain, including `node` itself - the construct a human perceives the deleted/inserted
/// content as belonging to. The climb stops at the first mapped ancestor (walking into matched
/// territory would attribute the content to a container that visibly survived). `None` when no
/// reference node is on the unmapped chain (content moving within matched surroundings).
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

/// True if every node in `root`'s subtree is mapped to 0 - i.e. nothing inside was claimed by any
/// earlier pass, so the whole subtree is free to be re-mapped as one moved unit.
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

/// Replaces the delete/insert mappings of two identical subtrees with pairwise matches, walking
/// both in lockstep (identical full hashes guarantee identical shape, so children line up 1:1).
/// Every pair is `Identical` - the content is byte-for-byte the same; only its location changed -
/// with the `MovedSubtree` reason marking how the pair was found.
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
    ///
    /// Was `#[ignore]`d during the `phases-4-7-rearchitecture` branch's Phase 1 (see `TODO.md`):
    /// briefly found 1 delete/1 insert instead of 0/0, from replacing whole-residual full APTED
    /// with the cheaper Myers-LCS fallback (`for_roots_fallback`). Passes again as of the
    /// `maximal_unmatched_roots` traversal fix (`TODO.md`'s "Bug fix" entry) - un-ignored.
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

        // `fn a` was deleted and `fn d` inserted; their bodies share `let x = 1;` but the
        // functions are different. The statement is small commodity code: it must round-trip as
        // delete+insert with its function, not "move" between two unrelated functions...
        // unless a legitimate larger container above it was matched (which it isn't here: the
        // names differ, so the name-keyed pass orphans both).
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

    /// `disambiguate_by_context` is tested directly, on hand-built metadata, rather than through
    /// the pipeline: driving it from source would mean finding real code where the ambiguity guard
    /// fires *and* the surroundings differ, and a passing end-to-end assertion would not prove
    /// which of the pipeline's dozen passes produced the pairing. Only `node_to_parent` and
    /// `node_to_similarity_sketch` are consulted, so only those are populated.
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
        // Node 1 sits in a container spelling {1,2,3}. Candidate 2's container spells the same;
        // candidate 3's has nothing in common. The candidates themselves are indistinguishable -
        // they share a full hash, which is why they are candidates at all - so the surroundings
        // are the only evidence there is.
        let before = metadata_with(&[(1, 10)], &[(10, &[1, 2, 3])]);
        let after = metadata_with(&[(2, 20), (3, 30)], &[(20, &[1, 2, 3]), (30, &[7, 8, 9])]);
        assert_eq!(
            disambiguate_by_context(1, &[2, 3], &before, &after),
            Some(2)
        );
    }

    #[test]
    fn context_tiebreak_refuses_when_the_surroundings_are_equally_alike() {
        // Both containers spell the same thing, so there is no honest answer and the guard must
        // fall back on refusing - the whole point of `AMBIGUOUS_MOVE_MIN_SIZE` is that a guess
        // between equals is worse than no pairing.
        let before = metadata_with(&[(1, 10)], &[(10, &[1, 2, 3])]);
        let after = metadata_with(&[(2, 20), (3, 30)], &[(20, &[1, 2, 3]), (30, &[1, 2, 3])]);
        assert_eq!(disambiguate_by_context(1, &[2, 3], &before, &after), None);
    }

    #[test]
    fn context_tiebreak_refuses_a_near_tie() {
        // 10/10 vs 10/11 shared - 1.00 against 0.91. Candidate 3 is genuinely worse, but not by
        // enough to call. A bare argmax would pair here; `CONTEXT_TIEBREAK_MARGIN` is what stops
        // it.
        let ten: Vec<u64> = (1..=10).collect();
        let eleven: Vec<u64> = (1..=11).collect();
        let before = metadata_with(&[(1, 10)], &[(10, &ten)]);
        let after = metadata_with(&[(2, 20), (3, 30)], &[(20, &ten), (30, &eleven)]);
        assert_eq!(disambiguate_by_context(1, &[2, 3], &before, &after), None);
    }

    #[test]
    fn context_tiebreak_declines_to_rank_a_crowd_of_commodity_tokens() {
        // Above `MAX_AMBIGUOUS_CANDIDATES` the tie-break is not attempted at all, however clear
        // the winner looks: scoring hundreds of candidates per deleted node cost ~8% of total
        // corpus runtime, and "the best of 200 identical `,` tokens" is not a verdict worth
        // having. The winner here would otherwise be unambiguous.
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
