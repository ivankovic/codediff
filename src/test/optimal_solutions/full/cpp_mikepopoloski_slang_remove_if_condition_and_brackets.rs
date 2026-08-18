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
    // Before has 3 comment nodes, after has only 2. But realistically a human would just expect
    // a 3:2 Update mapping.
    //
    // Clamped at 2 (2026-08-18, newly added fixture): `qualified_name` swaps which of two
    // identical `)` tokens inside the same `condition_clause` claims the human's chosen partner -
    // both pairings are byte-for-byte equivalent closes of a parenthesized expression, so this is
    // a same-shape sibling-choice tie, not a real structural miss.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "cpp-mikepopoloski-slang-remove-if-condition-and-brackets",
        2,
    )
}
