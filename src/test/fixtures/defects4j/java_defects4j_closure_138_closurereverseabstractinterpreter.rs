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
    // Three closing braces, and they are the mapping's own brace confusion seen from codediff's
    // side - see this fixture's `invariants` below. codediff pairs two `}`s the mapping does not
    // and drops one it does (reason `APTED("qualified_name")`).
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
    // 2026-09-21, invariant 3, two: a crossed brace pairing. The `if` on before row 203 closes on
    // row 222 and the `if` on row 208 closes on row 220, nested inside it. The mapping keeps row
    // 203's `{` and row 220's `}` while deleting row 208's `{` and row 222's `}` - so what
    // survives is the *outer* opener paired with the *inner* closer, which is neither "outer with
    // outer" nor "inner with inner". Invariant 3 sees it as two separate disagreements, one per
    // pair, because it compares each bracket against its own partner's status.
    //
    // Recorded as found. The repair is to pick one reading and keep both halves of it; which one
    // is a free choice, and the corpus has no rule preferring inner or outer - see
    // `research/data/quality/kind_mismatch_census_2026_09_21.md` on why that choice is left to the
    // author and `delimiter_pairs_agree`'s own doc on the multi-map group that usually expresses
    // it. The fixture's three `mapping()` mismatches are the same braces seen from codediff's
    // side.
    assert_ground_truth_invariants_with_known_violations(
        "java-defects4j-closure-138-closurereverseabstractinterpreter",
        2,
    )
}
