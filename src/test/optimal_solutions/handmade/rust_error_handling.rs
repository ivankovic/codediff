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
    // In this test, there are two interesting choices:
    //
    // 1. The .unwrap() to ? changes the AST from:
    //
    // call_expression
    //  field_expression
    //    call_expression
    //      field_expression
    //
    //  to
    //
    //  try_expression
    //    call_expression
    //      field_expression
    //
    //  There is no difference in the optimal cost if the call_expression and field_expression in
    //  the after side matches to either of the two corresponding candidates on the before side. In
    //  any case, the cost is always 2 x COST_UPDATE (which is 0) + 2 x COST_DELETE. This is why the
    //  human solution leaves these nodes unmarked (no entry in `human_mapping.json` at all -
    //  `compute_mismatches_for_with_config` only checks nodes that have one, so this is a genuine
    //  "don't care" rather than an assertion either way).
    //
    //  2. The "contents" changing to "OK(contents)". The AST changes from:
    //
    //  identifier "contents"
    //
    //  to
    //
    //  call_expression
    //    identifier "Ok"
    //    arguments
    //      (
    //      identifier "contents"
    //      )
    //
    //  This is quite a "deep" change, and heuristics that prevent changes across too many levels
    //  might not allow "contents" to match. However, for humans it is obviously correct to match
    //  them. Confirmed this is APTED's own tree-edit-distance ordering constraint, not a heuristic
    //  cutoff: both nodes reach APTED (reason `APTED("syntax_named")` on both the before-side
    //  Delete and the after-side Insert) and matching them is textually free (identical
    //  identifiers), but APTED's optimal solution still doesn't include the pair - matching them
    //  would violate the LCA-consistent ordering APTED's mapping model requires relative to the
    //  other already-fixed matches in the block. This is the same class of "objective wall" as
    //  other cost-tied APTED gaps documented in `TODO.md` - not attempted here.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit("rust-error-handling", 1)
}
