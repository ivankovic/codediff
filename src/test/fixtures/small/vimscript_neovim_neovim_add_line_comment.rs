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
    // Was 15 mismatches (same-kind-sibling ambiguity in a 1732-line file with hundreds of
    // near-duplicate test functions - see TODO.md's "1 new optimal-solution fixture added,
    // clamped" entry) until the 2026-08-08 `solve_large_flat_subtrees` fixes incidentally
    // resolved it too - see TODO.md's 2026-08-08 entry.
    test::helper::human_mapping::assert_matches_human_mapping(
        "vimscript-neovim-neovim-add-line-comment",
    )
}
