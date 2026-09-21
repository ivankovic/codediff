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
    // Repairing the invariant-18 violation put this mismatch here: the mapping now pairs
    // `type: undefined` with `type: GREENBONE_SENSOR_SCANNER_TYPE` across `pair.value`, and
    // codediff deletes the `undefined` instead (reason `APTED("greedy_anchor_block")`). One
    // mismatch, and it is the whole of the change.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "tsx-greenbone-gsa-add-import-and-use-it",
        1,
        1,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("tsx-greenbone-gsa-add-import-and-use-it")
}

#[test]
fn painting() -> Result<()> {
    assert_matches_human_painting_within_limit("tsx-greenbone-gsa-add-import-and-use-it", 0.76)
}
