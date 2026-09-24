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
    // The mapping's own brace confusion (see `invariants` below) seen from codediff's side
    // (`APTED("qualified_name")`).
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-defects4j-closure-138-closurereverseabstractinterpreter",
        3,
        3,
    )
}

#[test]
fn painting() -> Result<()> {
    assert_matches_human_painting_within_limit(
        "java-defects4j-closure-138-closurereverseabstractinterpreter",
        0.0,
    )
}

#[test]
fn invariants() -> Result<()> {
    // Invariant 3, twice: a crossed brace pairing. The `if` on before row 203 closes on row 222,
    // the nested one on row 208 closes on row 220. The mapping keeps row 203's `{` and row 220's
    // `}` and deletes the others: outer opener with inner closer. The repair is to pick either
    // reading and keep both halves of it.
    assert_ground_truth_invariants_with_known_violations(
        "java-defects4j-closure-138-closurereverseabstractinterpreter",
        2,
    )
}
