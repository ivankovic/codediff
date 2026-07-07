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
    //    Delete + Insert.
    // 2. The "return Some(nums[i])" / "return Some(num)" pair however, is something we want to
    //    match. While similar logic to 1. applies, showing to the human that the loop in both cases
    //    contains the logically same return is valuable, so these nodes should match.
    test::helper::human_mapping::assert_matches_human_mapping("rust-algorithm-change")
}
