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
        "vimscript-neovim-neovim-small-change-2",
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-10: minimal 0.590%, full 5.605%
    // codediff splits into Insert+Delete what the human painting calls one Update. Note this
    // fixture also has a known ground-truth invariant violation (see invariants() below), so part
    // of this rate may be the data rather than the algorithm.
    assert_matches_human_painting_within_limit("vimscript-neovim-neovim-small-change-2", 5.62)
}

#[test]
fn invariants() -> Result<()> {
    // Was pinned at 1: `Minimal` inserts `| ` on row 18 of the after side and the run ends on
    // that space. It is mid-row - `setl com< cms<'` follows it - so this was the invariant
    // over-firing, not a painting to repair. Back to 0 since `rows_end_on_visible_characters`
    // was narrowed to genuinely trailing whitespace on 2026-09-11.
    assert_ground_truth_invariants("vimscript-neovim-neovim-small-change-2")
}
