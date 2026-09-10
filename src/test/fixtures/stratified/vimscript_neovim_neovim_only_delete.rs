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
    test::helper::human_mapping::assert_matches_human_mapping("vimscript-neovim-neovim-only-delete")
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-10: minimal 0.054%, full 0.054%
    assert_matches_human_painting_within_limit("vimscript-neovim-neovim-only-delete", 0.07)
}

#[test]
fn invariants() -> Result<()> {
    // 2 known violations in the ground truth itself, not in codediff - both the same defect, and
    // both spelled out here because this assertion is exact and a bare count is unreadable:
    //
    //   painting 'Minimal' before row 24 ends its last painted run on ' ', not on a visible
    //     character: "  let b:undo_ftplugin .= '| setlocal keywordprg< iskeyword< | sil! delc
    //     -buffer SudoersKeywordPrg'"
    //   painting 'Full' before row 24: the same run, same trailing space.
    //
    // Repairing it means re-painting row 24 in human_solver so the run stops at the last visible
    // byte; until then this pins the count. See the no-trailing-whitespace painting work of
    // 2026-08-31 for why a painted run may not end on whitespace.
    assert_ground_truth_invariants_with_known_violations("vimscript-neovim-neovim-only-delete", 2)
}
