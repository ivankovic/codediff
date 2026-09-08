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
    // The edit rewrites a `typeof x === "undefined"` guard into a `isUndefined(x)` call. Both
    // sides still hold a `binary_expression` in the same slot, and `APTED("fast_fallback")` pairs
    // them on that alone - the human reads the old test as removed and the new call as new, since
    // nothing inside survives (`parenthesized_expression`/`unary_expression` on one side,
    // `call_expression`/`arguments` on the other). A same-kind container whose entire content was
    // replaced costs less to rename than to delete-and-insert under a unit cost model, so this is
    // the cost function speaking, not a search failure. Not attempted.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "javascript-axios-axios-real-small-change",
        3,
        1,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-09: minimal 2.374%, full 11.732% (measured, unexamined)
    assert_matches_human_painting_within_limit("javascript-axios-axios-real-small-change", 11.75)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("javascript-axios-axios-real-small-change")
}
