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
        "css-wordpress-wordpress-reformat-and-fix-lint-errors",
    )
}

#[test]
fn painting() -> Result<()> {
    // re-measured 2026-09-08: minimal 71.970%, full 50.758%, up from 64.394% the same day and
    // for the same reason as `shellscript-stgpetrovic-stacuist-pure-add` - `reconcile_moves` now
    // mirrors onto the after side `Move`s the before side already claimed, so a pre-existing
    // over-report on a whole-file CSS reformat is stated twice instead of once.
    assert_matches_human_painting_within_limit(
        "css-wordpress-wordpress-reformat-and-fix-lint-errors",
        71.98,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("css-wordpress-wordpress-reformat-and-fix-lint-errors")
}
