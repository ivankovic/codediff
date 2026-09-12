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
    // First measurement, 2026-09-12. The statement carrying the call moved from position 1 to
    // position 3 of its block; the human keeps its receiver `.` paired across the move, while
    // codediff's `qualified_name` pass reads that leaf as deleted. The statement itself is
    // matched either way, so the residual is the one leaf inside it.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-defects4j-jsoup-52-xmldeclaration",
        1,
        1,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-12: minimal 5.008%, full 7.753% (measured, unexamined)
    assert_matches_human_painting_within_limit("java-defects4j-jsoup-52-xmldeclaration", 7.77)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("java-defects4j-jsoup-52-xmldeclaration")
}
