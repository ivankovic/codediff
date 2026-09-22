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
    // Three identical `foo();` become two, recorded as all-to-all 3:2 groups over the statement
    // and each of its six descendants (see `MultiMapGroup::pairing`). A one-to-one output must
    // leave one before member of every group unmatched, so seven is exactly the floor - codediff
    // matches everything it can express. Closing this is the N:M algorithm work, not a gap here.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "rust-multi-map-duplicate-calls",
        7,
        4,
    )
}

#[test]
fn painting() -> Result<()> {
    assert_matches_human_painting_within_limit("rust-multi-map-duplicate-calls", 58.12)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("rust-multi-map-duplicate-calls")
}
