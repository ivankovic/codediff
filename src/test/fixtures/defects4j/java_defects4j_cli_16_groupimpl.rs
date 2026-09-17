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
    test::helper::human_mapping::assert_matches_human_mapping("java-defects4j-cli-16-groupimpl")
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-17: minimal 0.000%, full 0.037% (measured, unexamined)
    assert_matches_human_painting_within_limit("java-defects4j-cli-16-groupimpl", 0.05)
}

#[test]
fn invariants() -> Result<()> {
    // First measurement, 2026-09-17: invariant 4, after row 92 paints every visible character
    // Insert but leaves the line's own 12-byte indent unpainted
    // ("            option.setParent(this);"). Recorded as found; one painted range needs
    // extending to the start of the line's content.
    assert_ground_truth_invariants_with_known_violations("java-defects4j-cli-16-groupimpl", 1)
}
