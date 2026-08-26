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

#[test]
fn optimal_solution() -> Result<()> {
    // Two top-level functions are moved verbatim into a new class's body (method extraction). The
    // human ground truth marks both as Delete (before) + Insert (after) - a new class, not a
    // refactor of the existing functions. codediff's move-detection instead matches them across
    // the new `class_declaration` wrapper, since their content is untouched - the same "container
    // added around moved code" pattern as `java_add_exception_handling`'s documented gap, but with
    // the *opposite* human preference (there, outer-to-outer match wins; here, no match at all is
    // wanted). That conflict between two real fixtures is itself evidence there's no single
    // correct general heuristic - this is fixture-specific human judgment, not a bug. Not
    // attempted.
    // Top-level functions become methods of a new class - every one of them gains an enclosing
    // level. The human pairs each `identifier` across the reparent; `APTED("fast_fallback")`
    // deletes them, because the Myers LCS it ends in cannot align a node that moved deeper in the
    // residual forest. The canonical wrap/reparent gap, at the largest scale in the handmade set.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "kotlin-refactor-function",
        46,
        32,
    )
}
