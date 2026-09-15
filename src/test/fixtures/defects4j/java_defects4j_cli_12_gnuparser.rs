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
    // First measurement, 2026-09-15, of a mapping from the 2026-09-15 Defects4J batch.
    // Recorded as found, not examined.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-defects4j-cli-12-gnuparser",
        7,
        5,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-15: minimal 1.270%, full 3.272% (measured, unexamined)
    assert_matches_human_painting_within_limit("java-defects4j-cli-12-gnuparser", 3.29)
}

#[test]
fn invariants() -> Result<()> {
    // Invariant 9, first measured 2026-09-15: the `Full` painting calls 20 bytes on after row 87
    // a `Move` while the tree mapping has them as `Insert`. One of the two ground truths is
    // wrong about whether that code survives; which one is not yet decided.
    assert_ground_truth_invariants_with_known_violations("java-defects4j-cli-12-gnuparser", 1)
}
