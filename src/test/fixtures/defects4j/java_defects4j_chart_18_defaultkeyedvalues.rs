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
    test::helper::human_mapping::assert_matches_human_mapping(
        "java-defects4j-chart-18-defaultkeyedvalues",
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-12: minimal 0.010%, full 0.017% (measured, unexamined)
    assert_matches_human_painting_within_limit("java-defects4j-chart-18-defaultkeyedvalues", 0.03)
}

#[test]
fn invariants() -> Result<()> {
    // Two invariant-1 violations, one per preset, both on the same row: after row 333, the
    // `throw new UnknownKeyException("The key (" + key ` line, whose painted run ends on the
    // space after `key` rather than on a visible character. The row's own content continues
    // past it on the next line of the wrapped expression, so this is a run that stops one
    // character late rather than a stripe of colour hanging off a line end - a repair of the
    // painting, not of the rule, and the painter's to make.
    assert_ground_truth_invariants_with_known_violations(
        "java-defects4j-chart-18-defaultkeyedvalues",
        2,
    )
}
