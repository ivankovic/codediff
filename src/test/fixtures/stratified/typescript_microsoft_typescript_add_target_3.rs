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
        "typescript-microsoft-typescript-add-target-3",
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-10: minimal 0.000%, full 8.730%
    // Two instances of `ours=Move, theirs=None` - the same Move-vs-nothing family as
    // css-wordpress-wordpress-one-line-to-multiline, at a far smaller scale. Minimal is exact;
    // only Full disagrees, which is the axis those two presets are known to differ on.
    assert_matches_human_painting_within_limit("typescript-microsoft-typescript-add-target-3", 8.74)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("typescript-microsoft-typescript-add-target-3")
}
