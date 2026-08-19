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
    // A new `elseif` branch (genuinely new logic, per the fixture name) contains a string-
    // concatenation chain (`..` operator plus its identifier operands) whose individual tokens
    // coincidentally match earlier occurrences elsewhere in the file. codediff correctly treats
    // the branch as new (Delete) since its surrounding structure has no match; the human mapping
    // instead correlates a few of those coincidentally-identical leaf tokens.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "lua-neovim-neovim-add-new-logic",
        10,
        5,
    )
}
