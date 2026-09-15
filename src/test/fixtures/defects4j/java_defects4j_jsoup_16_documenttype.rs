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
    // First measurement, 2026-09-12. The edit wraps an existing call in another one, so the
    // after side has a `method_invocation` inside a `method_invocation` where the before side
    // has one. The human reads the inner pair as the surviving call and the outer one as new;
    // codediff's `qualified_name` pass pairs the receiver's `.` and `identifier` with the outer
    // call instead, which makes the same four leaves disagree twice - once as a wrong pairing,
    // once as an insert that was matched. One reading of one wrap, not four faults.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-defects4j-jsoup-16-documenttype",
        4,
        4,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-12: minimal 1.117%, full 4.276% (measured, unexamined)
    assert_matches_human_painting_within_limit("java-defects4j-jsoup-16-documenttype", 4.29)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("java-defects4j-jsoup-16-documenttype")
}
