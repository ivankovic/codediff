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
    // First measurement, 2026-09-12. The edit replaces a `local_variable_declaration` with an
    // `enhanced_for_statement` over the same call, so the human carries the call's own leaves -
    // its identifier, its `.`, its parentheses - across the two containers, while codediff's
    // `qualified_name` pass reads them as deleted along with the declaration that held them.
    // One disagreement about which container survives, counted once per carried leaf.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-defects4j-mockito-21-constructorinstantiator",
        7,
        5,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-12: minimal 3.777%, full 4.871% (measured, unexamined)
    assert_matches_human_painting_within_limit(
        "java-defects4j-mockito-21-constructorinstantiator",
        4.89,
    )
}

#[test]
fn invariants() -> Result<()> {
    // Invariant 16, first measured 2026-09-15 when the rule was added: the Minimal/Full split
    // for a renamed identifier is not painted this way yet (`c`/`constructor` and `getDeclaredConstructors`/`getDeclaredConstructor`). Recorded as found; the
    // rule is new, the paintings predate it.
    assert_ground_truth_invariants_with_known_violations(
        "java-defects4j-mockito-21-constructorinstantiator",
        3,
    )
}
