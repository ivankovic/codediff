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
use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use crate::diff::ASTDiff;

/// Greedy matching of candidates grouped by key, shared by phase 4's mechanisms. Only same-key
/// candidates are compared; each same-key pair is scored by `cost` (lower is better), pairs above
/// `max_cost` are dropped (`None`: the key alone justifies a pair, and cost only orders it), and
/// the cheapest remaining pairs are accepted first, each side at most once. `on_accept` does the
/// actual mapping; a pair already mapped by an earlier `on_accept` is skipped.
///
/// # Determinism contract
///
/// The candidate slices must come in a run-to-run deterministic order (a tree traversal, never
/// node-id or `HashMap` order). Ties keep that order; nothing else here can introduce
/// nondeterminism.
pub(crate) fn solve<K: Eq + Hash>(
    diff: &mut ASTDiff,
    before_candidates: &[(usize, K)],
    after_candidates: &[(usize, K)],
    cost: impl Fn(usize, usize) -> f64,
    max_cost: Option<f64>,
    mut on_accept: impl FnMut(usize, usize, &mut ASTDiff),
) {
    let mut after_by_key: HashMap<&K, Vec<usize>> = HashMap::new();
    for (after_id, key) in after_candidates {
        after_by_key.entry(key).or_default().push(*after_id);
    }

    // `after_by_key` is only looked up, never iterated, so order comes from the inputs alone.
    let mut scored: Vec<(f64, usize, usize)> = Vec::new();
    for (before_id, key) in before_candidates {
        if diff.before_node_map.contains_key(before_id) {
            continue;
        }
        let Some(after_ids) = after_by_key.get(key) else {
            continue;
        };
        for &after_id in after_ids {
            if diff.after_node_map.contains_key(&after_id) {
                continue;
            }
            let pair_cost = cost(*before_id, after_id);
            if max_cost.is_some_and(|max| pair_cost > max) {
                continue;
            }
            scored.push((pair_cost, *before_id, after_id));
        }
    }

    // Must stay a stable sort, for the determinism contract.
    scored.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut before_claimed: HashSet<usize> = HashSet::new();
    let mut after_claimed: HashSet<usize> = HashSet::new();

    for (_, before_id, after_id) in scored {
        if before_claimed.contains(&before_id) || after_claimed.contains(&after_id) {
            continue;
        }
        // An earlier `on_accept` may have mapped a nested candidate as a descendant.
        if diff.before_node_map.contains_key(&before_id)
            || diff.after_node_map.contains_key(&after_id)
        {
            continue;
        }
        before_claimed.insert(before_id);
        after_claimed.insert(after_id);
        on_accept(before_id, after_id, diff);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_same_key_candidates_are_ever_paired() {
        let mut diff = ASTDiff::default();
        let mut accepted = Vec::new();

        solve(
            &mut diff,
            &[(1, "a"), (2, "b")],
            &[(10, "a"), (20, "b")],
            |_, _| 0.0,
            None,
            |before_id, after_id, _| accepted.push((before_id, after_id)),
        );

        assert_eq!(accepted, vec![(1, 10), (2, 20)]);
    }

    #[test]
    fn cheapest_pair_in_a_group_wins_and_each_side_is_claimed_once() {
        let mut diff = ASTDiff::default();
        let mut accepted = Vec::new();
        let cost = |before_id: usize, after_id: usize| match (before_id, after_id) {
            (2, 20) => 0.0,
            _ => 5.0,
        };

        solve(
            &mut diff,
            &[(1, "k"), (2, "k")],
            &[(10, "k"), (20, "k")],
            cost,
            None,
            |before_id, after_id, _| accepted.push((before_id, after_id)),
        );

        assert_eq!(
            accepted.len(),
            2,
            "every candidate should still find some pairing"
        );
        assert!(
            accepted.contains(&(2, 20)),
            "the cheapest pair must be accepted"
        );
        assert!(accepted.contains(&(1, 10)));
    }

    #[test]
    fn max_cost_rejects_pairs_above_the_threshold() {
        let mut diff = ASTDiff::default();
        let mut accepted = Vec::new();

        solve(
            &mut diff,
            &[(1, "k")],
            &[(10, "k")],
            |_, _| 5.0,
            Some(1.0),
            |before_id, after_id, _| accepted.push((before_id, after_id)),
        );

        assert!(
            accepted.is_empty(),
            "a pair costing more than max_cost must never be accepted"
        );
    }

    #[test]
    fn none_max_cost_accepts_regardless_of_cost() {
        let mut diff = ASTDiff::default();
        let mut accepted = Vec::new();

        solve(
            &mut diff,
            &[(1, "k")],
            &[(10, "k")],
            |_, _| 1_000_000.0,
            None,
            |before_id, after_id, _| accepted.push((before_id, after_id)),
        );

        assert_eq!(accepted, vec![(1, 10)]);
    }

    #[test]
    fn already_mapped_before_candidates_are_skipped() {
        let mut diff = ASTDiff::default();
        diff.before_node_map.insert(1, 999);
        diff.after_node_map.insert(999, 1);
        let mut accepted = Vec::new();

        solve(
            &mut diff,
            &[(1, "k")],
            &[(10, "k")],
            |_, _| 0.0,
            None,
            |before_id, after_id, _| accepted.push((before_id, after_id)),
        );

        assert!(
            accepted.is_empty(),
            "a before-candidate already mapped elsewhere must not be re-paired"
        );
    }

    #[test]
    fn already_mapped_after_candidates_are_skipped() {
        let mut diff = ASTDiff::default();
        diff.after_node_map.insert(10, 999);
        diff.before_node_map.insert(999, 10);
        let mut accepted = Vec::new();

        solve(
            &mut diff,
            &[(1, "k")],
            &[(10, "k")],
            |_, _| 0.0,
            None,
            |before_id, after_id, _| accepted.push((before_id, after_id)),
        );

        assert!(
            accepted.is_empty(),
            "an after-candidate already mapped elsewhere must not be re-paired"
        );
    }

    #[test]
    fn ties_are_broken_by_input_order_for_determinism() {
        let mut diff = ASTDiff::default();
        let mut accepted = Vec::new();
        solve(
            &mut diff,
            &[(1, "k"), (2, "k")],
            &[(10, "k"), (20, "k")],
            |_, _| 0.0,
            None,
            |before_id, after_id, _| accepted.push((before_id, after_id)),
        );

        assert_eq!(accepted, vec![(1, 10), (2, 20)]);
    }

    #[test]
    fn a_candidate_mapped_by_an_earlier_on_accept_is_not_paired_again() {
        let mut diff = ASTDiff::default();
        let mut accepted = Vec::new();

        solve(
            &mut diff,
            &[(1, "outer"), (2, "inner")],
            &[(10, "outer"), (20, "inner")],
            |_, _| 0.0,
            None,
            |before_id, after_id, diff| {
                accepted.push((before_id, after_id));
                diff.add_mapping(
                    2,
                    20,
                    crate::diff::ASTMapping::identical(crate::diff::ASTMappingReason::OptimalIDU),
                );
            },
        );

        assert_eq!(accepted, vec![(1, 10)]);
    }
}
