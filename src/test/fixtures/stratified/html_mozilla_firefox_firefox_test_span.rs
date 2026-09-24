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
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants_with_known_violations;

#[test]
fn mapping() -> Result<()> {
    // Requires N:M mapping. Both mismatches are members of an all-to-all group, which a one-to-one
    // output cannot reach - the N:M floor (see `MultiMapGroup::pairing`).
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "html-mozilla-firefox-firefox-test-span",
        2,
        2,
    )
}

#[test]
fn painting() -> Result<()> {
    assert_matches_human_painting_within_limit("html-mozilla-firefox-firefox-test-span", 2.72)
}

#[test]
fn invariants() -> Result<()> {
    // RECORDED, NOT ACCEPTED: the tree mapping pairs the identifier `office` with `ice`, which both
    // paintings leave unpainted. Either the mapping should not pair them, or both paintings need
    // the rename marked (Minimal `off`, Full the whole identifier). A human has to decide.
    assert_ground_truth_invariants_with_known_violations(
        "html-mozilla-firefox-firefox-test-span",
        4,
    )
}
