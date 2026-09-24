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
//! [`crate::code::hash`], which say nothing once two subtrees differ in one token.
//!
//! A bottom-k MinHash sketch of the set of *leaf* hashes in a node's subtree, built bottom-up with
//! the equality hashes and compared in O(k) whatever the subtree size. Leaves, not every
//! descendant: one changed token flips the full hash of every ancestor, so a one-token edit would
//! cost O(depth) elements instead of one.
//!
//! [`SimilaritySketch::jaccard`] is exact when both subtrees have fewer than [`SKETCH_WIDTH`]
//! distinct leaves and an estimate above. Rank and gate with it; never conclude equality from it.

/// Number of retained bottom-k values: small per node, and exact for most real subtrees.
pub const SKETCH_WIDTH: usize = 16;

/// Fixed, never per-process: a random seed would make similarity, and so matching, differ between
/// runs on identical input.
const SKETCH_SEED: u64 = 0x9E37_79B9_7F4A_7C15;

/// SplitMix64, the MinHash permutation: Merkle hashes of similar nodes correlate in their low
/// bits, and the k smallest images must be an unbiased sample.
fn mix(value: u64) -> u64 {
    let mut z = value.wrapping_add(SKETCH_SEED);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The k smallest *distinct* `mix`ed leaf hashes in a node's subtree, ascending.
///
/// Distinct, so a sketch is a function of the leaf-hash *set*, not of merge order.
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
    /// Equal to the bottom-k of the union of the children's sets (the bottom-k merge property),
    /// which is why one bottom-up pass sketches the whole tree.
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
    #[cfg(test)]
    pub fn is_exact(&self) -> bool {
        (self.len as usize) < SKETCH_WIDTH
    }

    /// Estimated Jaccard similarity of the two subtrees' leaf-hash sets, in `[0.0, 1.0]`.
    ///
    /// Standard bottom-k estimator: the fraction of `U`, the k smallest values of the union, present
    /// in both. The divisor is `|U|`, not `k`, or small subtrees would under-report similarity.
    pub fn jaccard(&self, other: &Self) -> f32 {
        let (a, b) = (self.as_slice(), other.as_slice());
        if a.is_empty() || b.is_empty() {
            return 0.0;
        }

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
        // Dividing by SKETCH_WIDTH instead of `|U|` would report 4/16.
        let a = sketch_of(&[1, 2, 3, 4]);
        let b = sketch_of(&[1, 2, 3, 4, 5]);
        assert!(a.is_exact() && b.is_exact());
        assert_eq!(a.jaccard(&b), 0.8);
    }

    #[test]
    fn one_changed_leaf_out_of_many_stays_near_one() {
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
        let forward = sketch_of(&[5, 9, 1, 7, 3]);
        let backward = sketch_of(&[3, 7, 1, 9, 5]);
        assert_eq!(forward, backward);
    }

    #[test]
    fn merging_is_associative_over_intermediate_nodes() {
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
        let a: Vec<u64> = (0..1_000).collect();
        let b: Vec<u64> = (500..1_500).collect(); // true Jaccard = 500/1500 = 0.333...
        let estimate = sketch_of(&a).jaccard(&sketch_of(&b));
        assert!(
            (0.1..=0.6).contains(&estimate),
            "estimate {estimate} is nowhere near the true 0.333"
        );
    }
}
