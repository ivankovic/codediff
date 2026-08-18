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
use rand::Rng;

/// LOC buckets for stratified sampling, keyed by the larger of a code pair's before/after line
/// count. Shared here rather than duplicated so every caller that stratifies by size
/// (`sample_code_pairs`, `sample_test_diffs --stratified`) provably uses the same buckets.
///
/// Human-readable ranges by deliberate choice (2026-08-19, on request): "10-30 lines" means
/// something to a person skimming a sample; the byte-size scheme this replaced (small/medium/
/// large/xlarge, <1,000/<10,000/<100,000/>=100,000 bytes) is what originally built the RQ1 corpus
/// underlying the introductory paper's empirical study (see `research/analysis/
/// apted_only_report.py`'s docstring, which cites it directly) - kept only as history here, not
/// reproduced: the already-committed `research/data/samples/sampled_code_pairs_*.csv` files still carry those
/// old byte-based labels in their own `size_bucket` column (that column name is unchanged - see
/// this module's `pub fn loc_bucket`'s doc comment - only what gets written into it going
/// forward), and aren't retroactively relabeled by this change; only a fresh `sample_code_pairs`
/// run produces LOC-based labels.
///
/// LOC, not AST node count, even though node count is the truer cost driver for tree-edit-distance
/// work (LOC-per-node varies widely by language): every caller here is picking candidates during a
/// commit walk, where a blob's line count is a cheap read (`stats::git::text_loc_if_in_range`) and
/// a node count would mean tree-sitter-parsing every candidate seen, not just the ones kept.
pub const LOC_BUCKETS: &[(usize, &str)] = &[
    (10, "0-10"),
    (30, "10-30"),
    (100, "30-100"),
    (300, "100-300"),
    (1_000, "300-1000"),
    (3_000, "1000-3000"),
    (usize::MAX, "3000+"),
];

/// Which of [`LOC_BUCKETS`] `loc` falls into - the first bucket whose upper bound it's strictly
/// less than, or `"3000+"` if it exceeds every bound (only reachable if `LOC_BUCKETS` is ever
/// changed to not end in `usize::MAX`).
///
/// Written into a `size_bucket` CSV column by both callers - not renamed to `loc_bucket` even
/// though the unit changed, because that column name is already baked into already-committed
/// research CSVs and downstream readers' struct field names (`benchmark_diff_pairs.rs`,
/// `apted_only_benchmark.rs`); renaming it would break deserializing every pre-existing file for
/// no benefit besides internal naming purity.
pub fn loc_bucket(loc: usize) -> &'static str {
    LOC_BUCKETS
        .iter()
        .find(|(upper, _)| loc < *upper)
        .map(|(_, name)| *name)
        .unwrap_or("3000+")
}

/// Reservoir sampling (Algorithm R): picks a uniform random sample of `capacity` items from a
/// stream of unknown length in a single pass, without holding the whole stream in memory.
pub struct Reservoir<T> {
    pub items: Vec<T>,
    seen: u64,
}

// Hand-written instead of derived so `T` itself doesn't need to be `Default`.
impl<T> Default for Reservoir<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            seen: 0,
        }
    }
}

impl<T> Reservoir<T> {
    pub fn offer(&mut self, item: T, capacity: usize, rng: &mut impl Rng) {
        self.seen += 1;
        if self.items.len() < capacity {
            self.items.push(item);
        } else {
            let j = rng.gen_range(0..self.seen) as usize;
            if j < capacity {
                self.items[j] = item;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn reservoir_never_exceeds_capacity() {
        let mut rng = StdRng::seed_from_u64(7);
        let mut reservoir = Reservoir::default();
        for i in 0..1000u32 {
            reservoir.offer(i, 5, &mut rng);
        }
        assert_eq!(reservoir.items.len(), 5);
        assert_eq!(reservoir.seen, 1000);
    }

    #[test]
    fn reservoir_keeps_everything_below_capacity() {
        let mut rng = StdRng::seed_from_u64(7);
        let mut reservoir = Reservoir::default();
        for i in 0..3u32 {
            reservoir.offer(i, 5, &mut rng);
        }
        assert_eq!(reservoir.items, vec![0, 1, 2]);
    }

    #[test]
    fn loc_bucket_boundaries() {
        assert_eq!(loc_bucket(0), "0-10");
        assert_eq!(loc_bucket(9), "0-10");
        assert_eq!(loc_bucket(10), "10-30");
        assert_eq!(loc_bucket(29), "10-30");
        assert_eq!(loc_bucket(30), "30-100");
        assert_eq!(loc_bucket(99), "30-100");
        assert_eq!(loc_bucket(100), "100-300");
        assert_eq!(loc_bucket(299), "100-300");
        assert_eq!(loc_bucket(300), "300-1000");
        assert_eq!(loc_bucket(999), "300-1000");
        assert_eq!(loc_bucket(1_000), "1000-3000");
        assert_eq!(loc_bucket(2_999), "1000-3000");
        assert_eq!(loc_bucket(3_000), "3000+");
        assert_eq!(loc_bucket(usize::MAX), "3000+");
    }
}
