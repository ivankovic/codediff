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
    // Recorded distance from the human mapping, not a target: 993 mismatches (662 visible) of
    // 8987 nodes, 11.05%, re-measured 2026-08-28 after the mapping was extended by 77k lines
    // (commit 6b15a71). The previous 214/144 was against a mapping that annotated far less of the
    // file - a more complete ground truth has more to disagree with, so the rise is the fixture
    // getting stricter rather than codediff getting worse: `algorithm_cost` is unchanged at 2585
    // across both measurements. Lower it when a change earns it; a rise from here is a regression.
    //
    // Still not an objective wall, and still the same mechanism the smaller mapping showed: codediff
    // costs 2585 against the human's 929, a gap of 1656. 888 of the 993 mismatches carry
    // `APTED("large_flat_subtree")` and 889 are nodes mapped to nothing at all - the class_body's
    // constructor_declaration children are deleted outright and reinserted rather than matched, and
    // every descendant of those subtrees goes with them. The remaining 105 are 49 `MovedSubtree`,
    // one `fast_fallback`, and the multi-map-group operation mismatches.
    // 2026-09-03: tightened 993,662 -> 362,251. The limit was stale rather than a deliberate
    // allowance: it had outlived the change that closed the gap, and `quality_baseline.csv` was the
    // only thing still holding this fixture to its real number. Any counts above describe the
    // older, larger residual.
    // Re-measured 2026-09-08 and tightened 362,251 -> 360,249: the delimiter fix in 974cc062
    // (`reclaim_slot_level_twins`, "Give a delimiter back to the construct it closes") removed 2
    // of them. A limit above the measured number is a test that cannot fail, which is what
    // `the_quality_baseline_accuracy_columns_are_a_projection_of_the_stub_limits` exists to catch
    // - the baseline records the measurement, so the stub has to record it too.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-pdftk-java-pdftk-real-change-all-across-the-file",
        360,
        249,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("java-pdftk-java-pdftk-real-change-all-across-the-file")
}
