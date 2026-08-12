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
    // Lowered from 6 to 2 (the 2026-08 `hash_tree_matching::pair_children_for_descent` fix -
    // parent-relative rather than absolute-file child-ordinal tiebreak - fixed 4 of the original 6
    // as a side effect). The remaining 2 are a callback-to-async/await rewrite: the top-level
    // `fetchData(...)` call's identifier/arguments would need to match their counterparts now
    // nested inside an async IIFE's `await` expression - a "bridge across added nesting" gap in
    // the same family as `rust-algorithm-change`'s documented case 2 (see that fixture's doc
    // comment), not attempted here.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "typescript-async-await",
        2,
    )
}
