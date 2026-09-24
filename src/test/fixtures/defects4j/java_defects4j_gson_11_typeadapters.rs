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
use crate::test::helper::human_mapping::assert_matches_human_painting_within_limit;
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants;

#[test]
fn mapping() -> Result<()> {
    // A new `case` in a `switch`: the human calls group 3 new, `APTED("large_flat_subtree")` slides
    // the match by one group. The equal-looking-siblings family of `closure-31-compiler`.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-defects4j-gson-11-typeadapters",
        10,
        6,
    )
}

#[test]
fn painting() -> Result<()> {
    assert_matches_human_painting_within_limit("java-defects4j-gson-11-typeadapters", 0.07)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("java-defects4j-gson-11-typeadapters")
}
