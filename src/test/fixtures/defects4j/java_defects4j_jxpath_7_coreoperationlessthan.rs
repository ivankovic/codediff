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
    // First measurement, 2026-09-12, of a mapping added in the 2026-09-11 Defects4J batch.
    // All four `CoreOperation*` fixtures in this batch carry the same disagreement and the
    // same 36/22: the human deletes the first `method_declaration` whole and inserts its
    // replacement, while codediff keeps that method's scaffolding - its `}`, its `;`, its
    // `<` operator leaf - and re-uses it inside the surviving method. One choice about
    // which of two near-identical methods survives, counted once per re-used leaf, rather
    // than 36 independent errors.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-defects4j-jxpath-7-coreoperationlessthan",
        36,
        22,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-12: minimal 3.411%, full 4.502% (measured, unexamined)
    assert_matches_human_painting_within_limit(
        "java-defects4j-jxpath-7-coreoperationlessthan",
        4.52,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("java-defects4j-jxpath-7-coreoperationlessthan")
}
