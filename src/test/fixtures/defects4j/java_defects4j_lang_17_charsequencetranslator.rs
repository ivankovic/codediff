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
        "java-defects4j-lang-17-charsequencetranslator",
        5,
        2,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-15: minimal 0.298%, full 0.318% (measured, unexamined)
    assert_matches_human_painting_within_limit(
        "java-defects4j-lang-17-charsequencetranslator",
        0.33,
    )
}

#[test]
fn invariants() -> Result<()> {
    // Invariant 3, first measured 2026-09-15: the tree mapping splits two brace pairs - before row
    // 90's `{` is deleted while its `}` on row 101 is matched, and row 93's `{` is matched while
    // its `}` on row 99 is deleted. A mapping repair, not a painting one.
    assert_ground_truth_invariants_with_known_violations(
        "java-defects4j-lang-17-charsequencetranslator",
        2,
    )
}
