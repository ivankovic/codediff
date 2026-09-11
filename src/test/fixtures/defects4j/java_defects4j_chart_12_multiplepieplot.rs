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
    // First measurement, 2026-09-12, of a mapping added in the 2026-09-11 Defects4J batch. The two
    // residuals are one pairing: inside the second constructor, the human reads the `dataset`
    // field assignment as gone and the `setDataset(...)` call that replaced it as new, while
    // codediff's APTED pass reads the two `identifier` leaves as one `Update` because they sit in
    // the same large flat subtree and share their text. A container-choice disagreement rather
    // than a wrong pairing - the enclosing statements are already matched - so the number is
    // recorded as the bar, not as a defect with a fix behind it.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-defects4j-chart-12-multiplepieplot",
        2,
        2,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-12: minimal 0.077%, full 0.044% (measured, unexamined)
    assert_matches_human_painting_within_limit("java-defects4j-chart-12-multiplepieplot", 0.09)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("java-defects4j-chart-12-multiplepieplot")
}
