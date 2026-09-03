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
    // A nested `if ... else if ... end end` becomes `if ... elseif ... end`, inserting an extra
    // `elseif_statement` tree level around content that otherwise stays the same. Every
    // descendant under that reindented subtree gets classified MatchButNotIdentical/Update
    // instead of clean Identical, since its ancestor path (and therefore its own "own_content"
    // gap text around it) genuinely differs by one nesting level - not scattered independent
    // issues, all downstream of this one structural change.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "lua-neovim-neovim-if-flips-two-branches",
        68,
        46,
    )
}
