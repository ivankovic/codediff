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
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants;
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    // Recorded distance from the human mapping, not a target: 354 mismatches (243 visible) of
    // 8987 nodes, 3.94%, against a ground truth that annotates the whole file. Lower it when a
    // change earns it; a rise from here is a regression. Not an objective wall either - codediff
    // costs 1211 against the human's 929, a gap of 282, so the better mapping exists and the
    // search does not find it. 313 of the 354 carry `APTED("large_flat_subtree")` and 40
    // `MovedSubtree`: the class_body's constructor_declaration children are deleted outright and
    // reinserted rather than matched, and every descendant of those subtrees goes with them. A
    // limit above the measured number is a test that cannot fail, which is what
    // `the_quality_baseline_accuracy_columns_are_a_projection_of_the_stub_limits` exists to catch -
    // the baseline records the measurement, so the stub has to record it too.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-pdftk-java-pdftk-real-change-all-across-the-file",
        354,
        243,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("java-pdftk-java-pdftk-real-change-all-across-the-file")
}
