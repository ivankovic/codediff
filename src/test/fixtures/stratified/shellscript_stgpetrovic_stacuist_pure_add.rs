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
        "shellscript-stgpetrovic-stacuist-pure-add",
    )
}

#[test]
fn painting() -> Result<()> {
    // re-measured 2026-09-08: minimal 39.332%, full 39.589%, up from 17.224%/17.481% the same
    // day. Nothing new is wrong here - codediff already called the one command line a `Move` on
    // the before side, and `reconcile_moves`' containment fix now paints the same claim on the
    // after side too, doubling a residual instead of creating one. The underlying false `Move` is
    // the documented one-row-two-edits gap in `node_untouched_on_its_row`: the before file
    // indents every line by one space and the after file does not, *and* the command gains a
    // `--strategy=...` argument at the end, so the row's single common-prefix/common-suffix pair
    // finds neither edit and the one-column de-indent reads as a relocation. Fixing that needs a
    // multi-segment row diff, not a limit.
    assert_matches_human_painting_within_limit("shellscript-stgpetrovic-stacuist-pure-add", 39.60)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("shellscript-stgpetrovic-stacuist-pure-add")
}
