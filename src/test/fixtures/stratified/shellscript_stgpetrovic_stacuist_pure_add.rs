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
    // A false `Move` painted on both sides under `FULL`: the before file indents every line by one
    // space and the command also gains an argument, so the row's single common-prefix/suffix pair
    // finds neither edit and reads the de-indent as a relocation. The one-row-two-edits gap in
    // `node_untouched_on_its_row`; needs a multi-segment row diff.
    assert_matches_human_painting_within_limit("shellscript-stgpetrovic-stacuist-pure-add", 39.60)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("shellscript-stgpetrovic-stacuist-pure-add")
}
