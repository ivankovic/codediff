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
use anyhow::Result;

use crate::test;

#[test]
fn matches_human_solution() -> Result<()> {
    // In this test, we have several subjective quality decisions:
    //
    // 1. The theorethical lowest-cost solution uses some of the call_exprsion, identifier,
    //    arguments and '(' and ')' from the before side, e.g. from "..nums.len()" and maps them as
    //    Update/MatchButNotIdentical/Identical with the "HashSet::new()" call. However, this is
    //    very low quality matching from a human perspective. It is extremely unlikely a human would
    //    actually transform "..nums.len()" into HashSet::new(). It is far more likely it would be a
    //    Delete + Insert. Since the cost is tied either way, this is modeled as a `MultiMapGroup`
    //    (see `human_mapping.json`) rather than asserted as one specific answer - codediff's actual
    //    tied-cost choice is accepted.
    // 2. The "return Some(nums[i])" / "return Some(num)" pair however, is something we want to
    //    match. While similar logic to 1. applies, showing to the human that the loop in both cases
    //    contains the logically same return is valuable, so these nodes should match. Unlike 1.,
    //    this is a genuine algorithm gap, not a cost tie: matching it would require bridging a
    //    removed loop-nesting level (the before side has the if/return two `for_expression` levels
    //    deep, the after side one), which the pipeline's structural matchers don't currently do.
    //    The remaining 12 mismatches below are exactly this one chain (the if/return and everything
    //    under it) failing to bridge that nesting-depth change - left as a known, accepted gap
    //    rather than a broad "bridge removed nesting" heuristic, which risks regressing the rest of
    //    the corpus the same way past attempts at similar generalizations have (see `TODO.md`).
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "rust-algorithm-change",
        12,
        7,
    )
}
