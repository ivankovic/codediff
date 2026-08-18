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

//! A *similarity*-preserving sketch, as opposed to the four *equality* hashes in
//! [`crate::code::hash`].
//!
//! `node_to_full_hash` and friends answer "are these two subtrees exactly the same?" and say
//! nothing at all once the answer is no. A great many of the open matching problems in
//! `diff/TODO.md` need the other question - "how nearly the same are they?" - in a place where
//! actually comparing the two subtrees is unaffordable: choosing between several equally
//! hash-identical move targets, deciding whether two crossed siblings are the same entity
//! relocated or two different entities, ranking candidates in a large rewrite. One changed token
//! flips a Merkle hash completely, so "95% the same" and "entirely unrelated" are indistinguishable
//! to every hash we have.
//!
//! This module adds a bottom-k MinHash sketch of the set of *leaf* hashes in a node's subtree,
//! computed bottom-up in the same walk that computes the equality hashes (so O(n) at metadata
//! time), and comparable in O(k) - independent of subtree size - afterwards.
//!
//! # Why leaves, and not every descendant node
//!
//! Sketching every descendant's full hash sounds more discriminative and is in fact strictly
//! worse for the cases this exists to serve. A single changed token flips the full hash of every
//! *ancestor* of that token inside the subtree, so a one-token edit deep in a nested chain would
//! remove O(depth) elements from the set rather than 1. Near-identical subtrees differing in one
//! token are precisely the target, so the sketch is taken over leaves only, where one changed
//! token costs exactly one element.
//!
//! # It is an estimate - rank and gate with it, never conclude equality
//!
//! [`SimilaritySketch::jaccard`] is exact whenever both subtrees have at most [`SKETCH_WIDTH`]
//! distinct leaf hashes (the sketch *is* the set at that size, which happens to cover every small
//! subtree - exactly where the estimator's variance would have hurt most) and an estimate above
//! it. Use it to rank candidates and to gate decisions; the existing exact hashes already answer
//! "are these identical" definitively and should keep doing so.

/// Number of retained bottom-k values. 16 keeps a sketch to 136 bytes per node and makes the
/// sketch exact (not estimated) for any subtree with <= 16 distinct leaf hashes, which is most
/// nodes in a real file.
pub const SKETCH_WIDTH: usize = 16;

/// A fixed, hard-coded seed. It must never come from a per-process `RandomState`/`DefaultHasher`:
/// the retained values decide which elements two sketches appear to share, so a per-run seed would
/// make similarity - and therefore matching decisions downstream - differ between two runs on
/// identical input. That failure mode is documented at length on `ASTMetadata::node_to_full_hash`
/// and was a real, diagnosed bug in this codebase.
const SKETCH_SEED: u64 = 0x9E37_79B9_7F4A_7C15;

/// SplitMix64. Used as the MinHash permutation: it spreads the Merkle hashes (which are already
/// well distributed, but correlated in low bits for structurally similar nodes) so that "the k
/// smallest images" is an unbiased random sample of the set.
fn mix(value: u64) -> u64 {
    let mut z = value.wrapping_add(SKETCH_SEED);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The k smallest *distinct* `mix`ed leaf hashes in a node's subtree, ascending.
///
/// Distinct, not merely smallest-k-with-duplicates: keeping duplicates would make the retained
/// values depend on the order children were merged in, i.e. on traversal details rather than on
/// the subtree's content. Deduplicating makes a sketch a pure function of the *set* of leaf
/// hashes, which is what [`jaccard`](Self::jaccard) is defined over.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SimilaritySketch {
    values: [u64; SKETCH_WIDTH],
    /// How many of `values` are populated. `< SKETCH_WIDTH` means the sketch is the complete set.
    len: u8,
}

impl SimilaritySketch {
    /// The sketch of a leaf: the single element `{mix(full_hash)}`.
    pub fn leaf(full_hash: u64) -> Self {
        let mut values = [0u64; SKETCH_WIDTH];
        values[0] = mix(full_hash);
        Self { values, len: 1 }
    }

    /// The sketch of an internal node: the bottom-k of the union of its children's sketches.
    ///
    /// This is exactly the bottom-k of the union of the children's underlying *sets* whenever
    /// every child sketch is itself a correct bottom-k, which is the standard bottom-k merge
    /// property and is why the whole tree can be sketched in one bottom-up pass: an element small
    /// enough to survive in the parent was small enough to survive in its child.
    pub fn merge(children: impl IntoIterator<Item = Self>) -> Self {
        let mut pool: Vec<u64> = Vec::new();
        for child in children {
            pool.extend_from_slice(child.as_slice());
        }
        pool.sort_unstable();
        pool.dedup();
        pool.truncate(SKETCH_WIDTH);

        let mut values = [0u64; SKETCH_WIDTH];
        values[..pool.len()].copy_from_slice(&pool);
        Self {
            values,
            len: pool.len() as u8,
        }
    }

    fn as_slice(&self) -> &[u64] {
        &self.values[..self.len as usize]
    }

    /// True when this sketch holds the subtree's complete set of distinct leaf hashes, so
    /// [`jaccard`](Self::jaccard) against another complete sketch is exact rather than estimated.
    pub fn is_exact(&self) -> bool {
        (self.len as usize) < SKETCH_WIDTH
    }

    /// Estimated Jaccard similarity of the two subtrees' leaf-hash sets, in `[0.0, 1.0]`.
    ///
    /// Standard bottom-k estimator: take `U`, the k smallest values of the union of the two
    /// sketches, and report the fraction of `U` present in *both*. Because both sketches are
    /// bottom-k of their own sets, `U` is the bottom-k of the union of the sets, and membership of
    /// any element of `U` in either set is decidable from the sketches alone.
    ///
    /// The divisor is `|U|`, not `k`. When both subtrees have fewer than `k` distinct leaf hashes
    /// `|U| < k`, and dividing by `k` would systematically under-report similarity - in the small-
    /// subtree regime, which is where several intended callers (the move-detection ambiguity guard
    /// above all) live exclusively.
    pub fn jaccard(&self, other: &Self) -> f32 {
        let (a, b) = (self.as_slice(), other.as_slice());
        if a.is_empty() || b.is_empty() {
            return 0.0;
        }

        // Merge-walk the two ascending slices, taking the k smallest distinct values of the union
        // and counting how many of them appear in both.
        let (mut i, mut j) = (0usize, 0usize);
        let (mut union_size, mut shared) = (0usize, 0usize);
        while union_size < SKETCH_WIDTH && (i < a.len() || j < b.len()) {
            match (a.get(i), b.get(j)) {
                (Some(&x), Some(&y)) if x == y => {
                    shared += 1;
                    i += 1;
                    j += 1;
                }
                (Some(&x), Some(&y)) if x < y => i += 1,
                (Some(_), Some(_)) => j += 1,
                (Some(_), None) => i += 1,
                (None, Some(_)) => j += 1,
                (None, None) => unreachable!("loop condition guarantees one side has values left"),
            }
            union_size += 1;
        }

        shared as f32 / union_size as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sketch_of(leaf_hashes: &[u64]) -> SimilaritySketch {
        SimilaritySketch::merge(leaf_hashes.iter().map(|&h| SimilaritySketch::leaf(h)))
    }

    #[test]
    fn identical_sets_score_one_and_disjoint_sets_score_zero() {
        let a = sketch_of(&[1, 2, 3, 4]);
        assert_eq!(a.jaccard(&sketch_of(&[1, 2, 3, 4])), 1.0);
        assert_eq!(a.jaccard(&sketch_of(&[90, 91, 92, 93])), 0.0);
    }

    #[test]
    fn small_sets_are_exact_not_estimated() {
        // 4 shared out of a 5-element union: exactly 0.8, no sampling error, because neither
        // sketch is saturated. This is the regime the `|U|` divisor exists for - dividing by
        // SKETCH_WIDTH would have reported 4/16 = 0.25 for two nearly identical subtrees.
        let a = sketch_of(&[1, 2, 3, 4]);
        let b = sketch_of(&[1, 2, 3, 4, 5]);
        assert!(a.is_exact() && b.is_exact());
        assert_eq!(a.jaccard(&b), 0.8);
    }

    #[test]
    fn one_changed_leaf_out_of_many_stays_near_one() {
        // The motivating case: two subtrees differing in a single token must not read as
        // "different", which is all any of the equality hashes can say about them.
        let shared: Vec<u64> = (0..40).collect();
        let mut changed = shared.clone();
        changed[7] = 1_000;
        let similarity = sketch_of(&shared).jaccard(&sketch_of(&changed));
        assert!(
            similarity > 0.8,
            "one differing leaf in 40 scored {similarity}"
        );
    }

    #[test]
    fn merge_is_order_independent() {
        // Sketches must be a function of the content, not of the order children were visited in -
        // otherwise the same file could sketch differently depending on traversal details.
        let forward = sketch_of(&[5, 9, 1, 7, 3]);
        let backward = sketch_of(&[3, 7, 1, 9, 5]);
        assert_eq!(forward, backward);
    }

    #[test]
    fn merging_is_associative_over_intermediate_nodes() {
        // A parent's sketch must not depend on how its leaves were grouped into children: the
        // bottom-k merge property is what makes one bottom-up pass correct.
        let flat = sketch_of(&(0..60).collect::<Vec<_>>());
        let nested = SimilaritySketch::merge([
            sketch_of(&(0..20).collect::<Vec<_>>()),
            SimilaritySketch::merge([
                sketch_of(&(20..40).collect::<Vec<_>>()),
                sketch_of(&(40..60).collect::<Vec<_>>()),
            ]),
        ]);
        assert_eq!(flat, nested);
    }

    #[test]
    fn saturated_sketches_estimate_large_set_similarity() {
        // Above SKETCH_WIDTH distinct leaves the answer is sampled, so assert the estimate lands
        // near the true Jaccard rather than on an exact value.
        let a: Vec<u64> = (0..1_000).collect();
        let b: Vec<u64> = (500..1_500).collect(); // true Jaccard = 500/1500 = 0.333...
        let estimate = sketch_of(&a).jaccard(&sketch_of(&b));
        assert!(
            (0.1..=0.6).contains(&estimate),
            "estimate {estimate} is nowhere near the true 0.333"
        );
    }
}
