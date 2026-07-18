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

/**
* The generic engine behind phase 4's candidate-group matching (`TODO.md`'s "what is the
* generalization of phase 4" analysis, 2026-07-18). Three of phase 4's four mechanisms - named-
* group matching, positional anchoring, and flow-control arm-overlap - turned out to be the same
* algorithm with three different (candidate predicate, grouping key, cost function) tuples plugged
* in:
*
* 1. Partition both sides' candidates into buckets by an exact, hashable compatibility key `K` -
*    a before-candidate and an after-candidate are only ever compared if their keys are equal.
* 2. Score every same-key pair with a caller-supplied, cheap (non-APTED) cost function - lower is
*    better, `0.0` meaning "as good a match as this signal can express."
* 3. Optionally reject pairs whose cost exceeds `max_cost` - `None` means no rejection threshold
*    at all (the shared key alone already justifies the pair; cost only orders ties within a
*    group, e.g. deciding which of several same-named overloads pairs with which).
* 4. Greedily accept the cheapest remaining pair, cheapest first, each side claimable once.
* 5. Hand every accepted pair to `on_accept`, which does the actual mapping (almost always a real
*    `apted::for_nodes` call - the cheap cost function only ever decided *whether/who* to pair,
*    never *how* the pair's internals map).
*
* The fourth mechanism, large flat subtrees, doesn't fit this shape at all - it isn't a matching
* problem (no competing candidates, no scoring), it's a deterministic lookup executed *inside* an
* already-established pair purely to pre-empt part of that pair's own APTED call. It stays a
* separate, directly-called pass.
*
* # Determinism contract
*
* `before_candidates`/`after_candidates` must be supplied in a run-to-run deterministic order (a
* DFS/preorder traversal of the tree - never raw node-id order or anything derived from `HashMap`
* iteration). This function only ever uses a stable sort, and never iterates a `HashMap` to build
* output, so the *only* remaining source of nondeterminism would be nondeterministic input order -
* see `ASTNodeMetadata::start_byte`'s doc comment for why this matters (node ids are parse-unstable
* arena slots, and per-process hash-seed-dependent iteration order has caused real, previously-
* shipped nondeterminism bugs in this codebase).
*/
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

    // Score every before-candidate against same-key, not-yet-mapped after-candidates. Iteration
    // order here is fully determined by `before_candidates`'/`after_candidates`' own (caller-
    // guaranteed deterministic) order - `after_by_key`'s `Vec`s preserve `after_candidates`'
    // insertion order, and looking a key up by reference never iterates the `HashMap` itself.
    let mut scored: Vec<(f64, usize, usize)> = Vec::new();
    for (before_id, key) in before_candidates {
        if diff.before_node_map.contains_key(before_id) {
            continue;
        }
        let Some(after_ids) = after_by_key.get(key) else { continue };
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

    // Stable sort: ties preserve the deterministic pre-sort order established above.
    scored.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut before_claimed: HashSet<usize> = HashSet::new();
    let mut after_claimed: HashSet<usize> = HashSet::new();

    for (_, before_id, after_id) in scored {
        if before_claimed.contains(&before_id) || after_claimed.contains(&after_id) {
            continue;
        }
        // Defensive re-check: an earlier-accepted pair's real APTED resolution may already have
        // claimed one of these nodes as a matched descendant (e.g. an outer container matched
        // first, whose real edit-distance resolution already covered a nested candidate that's
        // also, independently, a candidate here).
        if diff.before_node_map.contains_key(&before_id) || diff.after_node_map.contains_key(&after_id) {
            continue;
        }
        before_claimed.insert(before_id);
        after_claimed.insert(after_id);
        on_accept(before_id, after_id, diff);
    }
}
