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
use crate::test;
use crate::test::helper::human_mapping::assert_matches_human_painting_within_limit;
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants_with_known_violations;
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    // This test contains an interesting ambigous situation:
    //
    // The added if clause can be mapped in two equally good ways. Either the inner or the outer
    // after if can map to the if in the before code. Expressed as two multi-map groups (inner and
    // outer `if`) since either pairing is valid - but codediff actually matches the inner `if` as
    // Identical rather than the group's declared MatchButNotIdentical, one mismatch beyond the
    // pre-multi-map 4.
    // 2026-09-03: tightened 5,0 -> 4,0. The limit was stale rather than a deliberate allowance: it
    // had outlived the change that closed the gap, and `quality_baseline.csv` was the only thing
    // still holding this fixture to its real number. Any counts above describe the older, larger
    // residual.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "c-linux-small-bugfix",
        4,
        0,
    )
}

#[test]
fn invariants() -> Result<()> {
    // Invariant 15, found when it was added on 2026-09-14: four `MatchButNotIdentical` entries
    // whose two subtrees read byte-identically, with every descendant paired inside - the
    // `val |= omr.omr_hitm ? P(SNOOP, HITM) : P(SNOOP, HIT);` chain (assignment, conditional,
    // both calls) that moves from before row 348 into the new `if` on after row 362. The grader
    // is strict about the operation, so each is a claim codediff can only meet by calling an
    // identical subtree not identical; the repair is to relabel them `Identical`, which is the
    // mapping author's to make.
    assert_ground_truth_invariants_with_known_violations("c-linux-small-bugfix", 4)
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-11: minimal 0.053%, full 0.004%
    assert_matches_human_painting_within_limit("c-linux-small-bugfix", 0.07)
}
