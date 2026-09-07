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
    // Re-baselined 2026-09-07 from 11/5, and NOT an algorithm regression: this fixture's human
    // mapping was re-paired by hand on 2026-09-06 (the crossed inner/outer braces its
    // `invariants()` test used to record), so the limits now score codediff against a different,
    // corrected ground truth. The residual is the nested `if` this commit did not touch - the
    // container choice, not the delimiters.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "cpp-godot-small-bugfix",
        13,
        7,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("cpp-godot-small-bugfix")
}
