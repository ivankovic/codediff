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
    //  human solution originally left these nodes unmarked - a genuine "don't care", since
    //  `compute_mismatches_for_with_config` only checks nodes that have an entry.
    //
    //  That is no longer true as of the mapping authored 2026-08-27, which resolves the choice
    //  into an explicit multi-map group: the two before-side nodes map to the one after-side node
    //  as `MatchButNotIdentical`. Three of the eight mismatches below are that group, and they
    //  say codediff chose `Identical` for those pairs instead. Cost-tied is not the same as
    //  don't-care, and the human has now said which of the tied readings is the right one.
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
    //  cutoff: both nodes reach APTED (reason `APTED("qualified_name")`, formerly `"syntax_named"`
    //  - renamed 2026-08-14 - on both the before-side Delete and the after-side Insert) and
    //  matching them is textually free (identical
    //  identifiers), but APTED's optimal solution still doesn't include the pair - matching them
    //  would violate the LCA-consistent ordering APTED's mapping model requires relative to the
    //  other already-fixed matches in the block. This is the same class of "objective wall" as
    //  other cost-tied APTED gaps documented in `TODO.md` - not attempted here.
    //
    // Clamp raised 1/1 -> 8/4 on 2026-08-27, by the mapping gaining the group above rather than
    // by anything in the differ changing: more nodes asserted means more of the existing
    // disagreement is visible to the check. Not a regression.
    // 2026-09-03: tightened 8,4 -> 5,4. The limit was stale rather than a deliberate allowance: it
    // had outlived the change that closed the gap, and `quality_baseline.csv` was the only thing
    // still holding this fixture to its real number. Any counts above describe the older, larger
    // residual.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "rust-error-handling",
        5,
        4,
    )
}
