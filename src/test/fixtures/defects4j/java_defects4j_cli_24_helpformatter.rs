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
    // A `throw new ...` becomes an assignment. The mapping removes and inserts both statements
    // whole; codediff pairs the class name across the replacement, matching the
    // `type_identifier` of the old `object_creation_expression` to the `identifier` of the new
    // assignment (reason `APTED("large_flat_subtree")`). Two mismatches, one per side of that one
    // reused name.
    //
    // 2026-09-21, 4,4 -> 2,2: the ground-truth repair that cleared this fixture's invariant-1
    // violation also halved the disagreement, so the old limit sat above the measurement. Caught
    // by `the_quality_baseline_accuracy_columns_are_a_projection_of_the_stub_limits`, which is
    // what keeps a clamp from outliving what it measured.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-defects4j-cli-24-helpformatter",
        2,
        2,
    )
}

#[test]
fn painting() -> Result<()> {
    assert_matches_human_painting_within_limit("java-defects4j-cli-24-helpformatter", 0.12)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("java-defects4j-cli-24-helpformatter")
}
