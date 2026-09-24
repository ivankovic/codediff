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
    // Pins a cross-kind operator match: `=` against the `.=` that replaced it, which needs
    // Vimscript's operator families in `families_for_language`.
    test::helper::human_mapping::assert_matches_human_mapping(
        "vimscript-neovim-neovim-small-change-2",
    )
}

#[test]
fn painting() -> Result<()> {
    // codediff splits into Insert+Delete what the painting calls one Update.
    assert_matches_human_painting_within_limit("vimscript-neovim-neovim-small-change-2", 5.62)
}

#[test]
fn invariants() -> Result<()> {
    // `Minimal` inserts `| ` on row 18 ending on a space, but mid-row: invariant 1 must not fire.
    assert_ground_truth_invariants("vimscript-neovim-neovim-small-change-2")
}
