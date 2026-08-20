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
    // 2026-08-05: pre-existing (not introduced by this session's work - confirmed via `git
    // stash`), cost-model-optimal, not a bug. The change wraps a single statement,
    // `assert(session):stop()`, in a new `if loop_running then ... end`. Codediff matches the
    // *outer* function-body `block` (before) to the *inner* if-block (after) - both containing
    // just that byte-identical assert call - and inserts only the wrapper (`if_statement`,
    // `if`/`then`/`end`, the `loop_running` condition). That's cheaper under unit cost than the
    // "obvious" outer-to-outer match a human expects: `block` is an internal node (rename cost 0
    // either way), but the outer-to-outer match's *children* differ in kind
    // (`expression_statement` vs `if_statement`, not on `kinds_update_allowed`'s list), forcing a
    // full delete-and-reinsert of the assert statement's whole subtree - far more expensive than
    // inserting just the wrapper. Single-node-granularity tree edit distance is explicitly
    // designed to find exactly this kind of cross-depth reuse (see `forest_dist`'s own doc
    // comment) - same category as `c-nginx-add-typedef`'s documented pwd/field_expression case:
    // genuinely optimal, not what a human would write.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "lua-neovim-neovim-add-if-around-one-line",
        2,
        0,
    )
}
