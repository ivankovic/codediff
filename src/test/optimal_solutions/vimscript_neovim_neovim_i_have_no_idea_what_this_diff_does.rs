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
    // Was ~30s/run (~120s total with the determinism check) before the 2026-08-08
    // `solve_large_flat_subtrees` fixes (kind-uniqueness top-level identity + recursing the
    // Myers-unmatched residual through APTED instead of atomically deleting/inserting it) let this
    // fixture's giant, deeply-nested dictionary finally get matched properly - now 0.17s at the
    // same 0-mismatch exactness as before. See TODO.md's 2026-08-08 entry for the full writeup.
    test::helper::human_mapping::assert_matches_human_mapping(
        "vimscript-neovim-neovim-i-have-no-idea-what-this-diff-does",
    )
}
