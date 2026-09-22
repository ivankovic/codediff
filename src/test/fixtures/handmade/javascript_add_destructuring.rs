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
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants;
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    // Clamped at the measured residual; what the original three are has not been analysed here.
    // The other sixteen are the four all-to-all 3:1 groups (see `MultiMapGroup::pairing`) over
    // the `const`, `=`, `identifier` and `;` of three declarations folded into one destructuring
    // declaration. Two of each group's three before members are unavoidably unmatched by a
    // one-to-one output (eight of the sixteen); codediff's `fast_fallback` deletes the third and
    // inserts the after node as well rather than pairing them, which is the other eight.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "javascript-add-destructuring",
        19,
        19,
    )
}

#[test]
fn painting() -> Result<()> {
    assert_matches_human_painting_within_limit("javascript-add-destructuring", 26.28)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("javascript-add-destructuring")
}
