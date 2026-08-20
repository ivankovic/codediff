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
    // The following surprising mappings exist in this diff:
    //
    // 1. When the if (passwords) block around lines 530/540 changes to if (*passwords), the change
    //    from pwd = passwords->elts to cb_data.pwd = (*passwords)->elts is particularly
    //    interesting. The textual change is somewhat small, and visually the code is very similar.
    //    Logically, the human would want these expressions to match, since they both represent
    //    "storing elts to a variable" and you can imagine a human editing one to the other.
    //    However, the AST diff is quite big for such a small textual change. In particular the
    //    addition of pointer dereference and parenthesis around "passwords" causes a change few
    //    levels high. However, the human solution has been double checked and it is subjectively
    //    optimal, even if not necessarily edit-distance optimal.
    //
    //  2. When "pwd++" changes to "cb_data.pwd++" the AST changes from:
    //
    //  update_expression
    //    identifier "pwd"
    //    ++
    //
    //  to
    //
    //  update_expression
    //    field_expression
    //      identifier "cb_data"
    //      .
    //      field_identifier "pwd"
    //    ++
    //
    //  What is interesting here is that if we didn't allow nodes of different kinds to match, the
    //  optimal solution matches identifier to identifier and has to pay the value-update cost to
    //  update pwd to cb_data, and then pay the insert cost to insert field_identifier "pwd".
    //  However, for humans, it is much better to match "pwd" to "pwd", even if the kinds differ. So
    //  the optimal solution has to allow for identifier and field_identifier node kinds to match.
    // 2026-08-05: dropped 62 -> 50 as a side effect of `prematch_identical_statement_siblings`
    // (`apted::common`) pre-matching more of this function's unchanged statements before its own
    // real APTED call, plus `ContainmentCtx`'s sibling-order-consistency check (added the same day
    // to fix a real regression that same pre-matching pass caused elsewhere, see
    // `python_refactoring.rs`) - see `TODO.md`.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "c-nginx-add-typedef",
        50,
        32,
    )
}
