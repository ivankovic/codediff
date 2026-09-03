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
use crate::test;
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    // 2026-08-05: pre-existing (not introduced by this session's work - confirmed via `git
    // stash`), an accepted heuristic tradeoff, not a bug. A new `block_table` parameter is
    // threaded through a large call's `argument_list`; one existing slot changes from
    // `None,  # page_table,` to `block_table,  # block_table,`. This falls inside
    // `solve_large_flat_subtrees`'s Myers-based matching, which only pairs *byte-identical*
    // hashes; since 2026-08-08 the residual it can't pair recurses through real APTED instead of
    // committing straight to delete/insert (see TODO.md's 2026-08-08 entry), which is why this
    // dropped from 2 mismatches to 1 - the trailing comment now resolves correctly. The remaining
    // one is genuinely unavoidable: `None` (before, kind `none`) and `block_table` (after, kind
    // `identifier`) aren't on `kinds_update_allowed`'s list, so even real APTED's forced-update
    // cost (`COST_DELETE + COST_INSERT + 1` = 3) loses to plain delete+insert (2) - there's no
    // cheaper mapping to find.
    // 2026-09-03: the clamp at 1,1 is gone - this fixture now maps exactly. The limit was stale
    // rather than a deliberate allowance: it had outlived the change that closed the gap, and
    // `quality_baseline.csv` was the only thing still holding this fixture to its real number. Any
    // counts above describe a residual that no longer exists.
    test::helper::human_mapping::assert_matches_human_mapping(
        "python-pytorch-pytorch-add-param-to-many-places-and-update-one",
    )
}
