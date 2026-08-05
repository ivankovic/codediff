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
    // stash`), cost-model-optimal, not a bug. `group="${args[4]}"` shifts to
    // `group="${args[5]}"` when a new `powershell="${args[4]}"` line is inserted right before it.
    // A human expects `group` (before) to match `group` (after) with just the array index
    // updated (4 -> 5). Codediff instead matches old `group` to new `powershell` (both index 4)
    // and inserts the shifted `group=args[5]` wholesale. Hand-verified this is strictly cheaper
    // under the unit cost model, not a Myers/heuristic artifact: matching by name costs 2 (an
    // `Update` on the array-index `number` literal, cost 2 - see `UnitCostModel::ren`, literals
    // cost more to update than identifiers) plus a full 14-node insert of the new `powershell`
    // statement = 16 total. Matching by (coincidentally shared) array index costs only 1 (an
    // `Update` on the `variable_name` identifier, group -> powershell, cost 1) plus the same
    // 14-node insert of the shifted `group=args[5]` = 15 total. The literal-costs-more-than-
    // identifier design choice (deliberate, calibrated elsewhere in the corpus) is exactly what
    // tips this one pairing the "wrong" way; the mapping itself is genuinely optimal.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "shellscript-ansible-ansible-add-variable-and-string-expansion",
        28,
    )
}
