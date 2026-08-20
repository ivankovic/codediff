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
    // Deliberately pathological: the source isn't valid CSS, so tree-sitter's error recovery
    // produces thousands of ERROR nodes on both sides. Node correspondence through an ERROR
    // subtree is essentially undefined - codediff's APTED pass maps most of them to 0 rather than
    // following the human's chosen (necessarily somewhat arbitrary) correspondence.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "css-shadcn-ui-ui-completely-broken-treesitter-parsing",
        124,
        124,
    )
}
