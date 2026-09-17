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
    // Clamped at 1/1 on 2026-09-15; exact since 2026-09-17, when Java gained
    // `BOOLEAN_LITERAL_KINDS`. The one residual was a `false` -> `true` flip - the mirror of
    // `java-defects4j-math-22-fdistribution`, and fixed by the same kind family.
    test::helper::human_mapping::assert_matches_human_mapping(
        "java-defects4j-math-22-uniformrealdistribution",
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-15: minimal 0.070%, full 0.070% (measured, unexamined)
    assert_matches_human_painting_within_limit(
        "java-defects4j-math-22-uniformrealdistribution",
        0.09,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("java-defects4j-math-22-uniformrealdistribution")
}
