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
    // Recorded as found, not examined.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-defects4j-chart-22-keyedobjects2d",
        23,
        16,
    )
}

#[test]
fn painting() -> Result<()> {
    assert_matches_human_painting_within_limit("java-defects4j-chart-22-keyedobjects2d", 1.05)
}

#[test]
fn invariants() -> Result<()> {
    // Invariant 1: both paintings end a run on the trailing space of after rows 321, 332 and 368,
    // once per painting. The file is CRLF and those lines carry a real space before the `\r`, so
    // this is painted trailing whitespace rather than a line-ending artifact - three spans to
    // shorten by one character, in each painting.
    assert_ground_truth_invariants_with_known_violations(
        "java-defects4j-chart-22-keyedobjects2d",
        6,
    )
}
