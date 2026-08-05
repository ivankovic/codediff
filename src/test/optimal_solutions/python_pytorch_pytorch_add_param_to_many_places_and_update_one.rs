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
    // stash`), an accepted heuristic tradeoff, not a bug. A new `block_table` parameter is
    // threaded through a large call's `argument_list`; one existing slot changes from
    // `None,  # page_table,` to `block_table,  # block_table,`. This falls inside
    // `solve_large_flat_subtrees`'s Myers-based matching, which only pairs *byte-identical*
    // hashes and commits every non-match straight to delete/insert (a deliberate speed tradeoff -
    // see `resolve_flat_tree_pair`'s own doc comment) rather than running real tree-edit-distance
    // on the residual. `None` (before, kind `none`) and `block_table` (after, kind `identifier`)
    // were never going to be Myers-paired - and even a real APTED pass wouldn't merge them
    // anyway: `none`/`identifier` aren't on `kinds_update_allowed`'s list, so a forced update
    // would cost `COST_DELETE + COST_INSERT + 1` (3), strictly more than the delete+insert (2)
    // codediff actually produced. Both the `None`/`identifier` and the trailing comment end up
    // deleted-and-reinserted rather than updated in place.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "python-pytorch-pytorch-add-param-to-many-places-and-update-one",
        2,
    )
}
