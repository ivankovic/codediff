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
        "go-gin-gonic-gin-one-space-removed-in-a-comment",
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-08: minimal 0.214%, full 0.214%
    assert_matches_human_painting_within_limit(
        "go-gin-gonic-gin-one-space-removed-in-a-comment",
        0.23,
    )
}

#[test]
fn invariants() -> Result<()> {
    // Was pinned at 1 from 2026-09-08 to 2026-09-11, and for the same reason as its twin
    // go-gin-gonic-gin-whitespace-in-comment: the deleted space is mid-comment, not trailing, so
    // the no-trailing-whitespace invariant should never have fired on it. Back to 0 now that the
    // invariant checks what its name says.
    assert_ground_truth_invariants("go-gin-gonic-gin-one-space-removed-in-a-comment")
}
