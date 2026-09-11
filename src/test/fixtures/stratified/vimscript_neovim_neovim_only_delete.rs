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
    test::helper::human_mapping::assert_matches_human_mapping("vimscript-neovim-neovim-only-delete")
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-10: minimal 0.054%, full 0.054%
    assert_matches_human_painting_within_limit("vimscript-neovim-neovim-only-delete", 0.07)
}

#[test]
fn invariants() -> Result<()> {
    // Repaired in the ground truth on 2026-09-11 and back to 0, from the 2 violations this
    // pinned since 2026-08-31. Both presets deleted `iskeyword< ` on row 24 and ended the run on
    // a space; `Full` now takes ` iskeyword<` (the left-anchored spelling) and `Minimal`
    // `iskeyword<`. Both end on `<`, so the no-trailing-whitespace invariant holds.
    assert_ground_truth_invariants("vimscript-neovim-neovim-only-delete")
}
