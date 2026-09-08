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
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "go-lazygit-switch-to-strings",
        17,
        7,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-05: minimal 2.564%, full 3.812%
    // re-measured 2026-09-08 after the Full painting was repaired against the two new whitespace
    // invariants: minimal unchanged at 2.564%, full 3.812% -> 4.089%. The repair added an Insert over
    // the leading tab of the inserted `"strings"` import and a Match pairing the `\t\t\t` of the
    // deleted `indentation += "  "` row with the inserted `count++` row - the shared-indentation case
    // invariant 4 explicitly allows. Ground truth moved, not the algorithm.
    assert_matches_human_painting_within_limit("go-lazygit-switch-to-strings", 4.10)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("go-lazygit-switch-to-strings")
}
