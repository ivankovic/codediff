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
use crate::test::helper::human_mapping::assert_matches_human_painting_within_limit;
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants;

#[test]
fn mapping() -> Result<()> {
    test::helper::human_mapping::assert_matches_human_mapping(
        "vimscript-neovim-neovim-add-one-dict-entry",
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-10: minimal 47.820%, full 47.820%
    // Nearly half the file, for a one-line change, and the AST mapping is exact - the gap is
    // entirely in `diff::text`. The enclosing container's children are separated by `\` line
    // continuations, which are *not* whitespace, so `own_content` sees the container's own gap
    // text change and `classify_node` returns `OwnContentChanged` instead of `Descend`. The
    // container has a gap between every pair of children, so `own_content_span` returns `None`
    // (it only localizes a single contiguous gap) and `own_content_update_ranges` falls back to
    // painting the whole container `Update`. Recorded 2026-09-10 as a measured gap, not accepted
    // as correct - see TODO.md.
    assert_matches_human_painting_within_limit("vimscript-neovim-neovim-add-one-dict-entry", 47.83)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("vimscript-neovim-neovim-add-one-dict-entry")
}
