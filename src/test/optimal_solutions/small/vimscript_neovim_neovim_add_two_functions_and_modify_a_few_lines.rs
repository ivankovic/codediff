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
    // Dropped 1542 -> 22 when `solve_leading_siblings` (formerly `solve_comment_nodes`) was
    // extended to walk a whole chain of leading comments/modifiers instead of just one hop - this
    // fixture apparently has runs of several consecutive leading comments that only the multi-hop
    // walk can fully anchor. Remaining 22 are a different, not yet root-caused gap.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "vimscript-neovim-neovim-add-two-functions-and-modify-a-few-lines",
        22,
    )
}
