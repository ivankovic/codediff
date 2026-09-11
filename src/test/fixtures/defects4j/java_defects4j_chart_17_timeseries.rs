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
    test::helper::human_mapping::assert_matches_human_mapping("java-defects4j-chart-17-timeseries")
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-12: minimal 0.019%, full 0.016% (measured, unexamined)
    assert_matches_human_painting_within_limit("java-defects4j-chart-17-timeseries", 0.03)
}

#[test]
fn invariants() -> Result<()> {
    // One invariant-4 violation, recorded as found on 2026-09-12 rather than repaired. On the
    // after side's row 858 - `clone.data = (List) ObjectUtilities.deepClone(this.data);` - the
    // `Full` painting calls every visible character `Insert` but leaves the eight spaces of
    // indentation in front of them unpainted, which reads as a line that arrived in part. What
    // the indent of an inserted line belongs to is the painter's call, so this records the
    // state rather than deciding it.
    assert_ground_truth_invariants_with_known_violations("java-defects4j-chart-17-timeseries", 1)
}
