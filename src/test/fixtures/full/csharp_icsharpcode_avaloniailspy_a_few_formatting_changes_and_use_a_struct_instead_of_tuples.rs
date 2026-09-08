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
    // Known, unreviewed gap against the human-authored mapping - not yet root-caused. Clamped at
    // the observed count rather than requiring an exact match. Lower (or drop back to
    // `assert_matches_human_mapping`) once a fix lands.
    // Re-measured 2026-09-08 and tightened 5,5 -> 4,4: the delimiter fix in 974cc062
    // (`reclaim_slot_level_twins`, "Give a delimiter back to the construct it closes") removed 1
    // of them. A limit above the measured number is a test that cannot fail, which is what
    // `the_quality_baseline_accuracy_columns_are_a_projection_of_the_stub_limits` exists to catch
    // - the baseline records the measurement, so the stub has to record it too.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "csharp-icsharpcode-avaloniailspy-a-few-formatting-changes-and-use-a-struct-instead-of-tuples",
        4,
        4,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants(
        "csharp-icsharpcode-avaloniailspy-a-few-formatting-changes-and-use-a-struct-instead-of-tuples",
    )
}
