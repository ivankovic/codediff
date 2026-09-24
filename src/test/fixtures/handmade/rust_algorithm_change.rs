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
    // Several subjective quality decisions:
    //
    // 1. The lowest-cost solution reuses parts of `..nums.len()` for `HashSet::new()`, which no
    //    human would do. The cost is tied, so it is a `MultiMapGroup` and codediff's choice is
    //    accepted.
    // 2. `return Some(nums[i])` / `return Some(num)` should match, which requires bridging a
    //    removed loop-nesting level; the structural matchers do not. Every mismatch on the
    //    if/return chain (`APTED("qualified_name")`) is this gap, left open rather than risk a
    //    broad "bridge removed nesting" heuristic.
    // 3. Two nested `for` loops become one: their `for`, `{` and `}` are all-to-all 2:1 groups,
    //    each leaving one before member unavoidably unmatched.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "rust-algorithm-change",
        47,
        33,
    )
}

#[test]
fn painting() -> Result<()> {
    // `FULL` sets the limit. `displaced_beside_an_edit_on_its_first_row` costs `MINIMAL` two bytes
    // here that the human paints.
    assert_matches_human_painting_within_limit("rust-algorithm-change", 28.0)
}

#[test]
fn invariants() -> Result<()> {
    // Invariant 16: the rename split is not painted (`num` against `nums`).
    assert_ground_truth_invariants_with_known_violations("rust-algorithm-change", 1)
}
