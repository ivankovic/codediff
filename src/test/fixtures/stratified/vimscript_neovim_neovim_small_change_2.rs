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
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants_with_known_violations;

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
    // 1 known violation in the ground truth itself, not in codediff:
    //
    //   painting 'Minimal' after row 18 ends its last painted run on ' ', not on a visible
    //     character: "let b:undo_ftplugin .= '| setl com< cms<'"
    //
    // Same defect and same repair as vimscript-neovim-neovim-only-delete next door: re-paint the
    // row in human_solver so the run stops at the last visible byte, then drop this to 0.
    assert_ground_truth_invariants_with_known_violations(
        "vimscript-neovim-neovim-small-change-2",
        1,
    )
}
