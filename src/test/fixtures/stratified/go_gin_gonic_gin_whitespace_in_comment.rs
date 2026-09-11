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
        "go-gin-gonic-gin-whitespace-in-comment",
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-11: minimal 0.069%, full 0.069%
    assert_matches_human_painting_within_limit("go-gin-gonic-gin-whitespace-in-comment", 0.08)
}

#[test]
fn invariants() -> Result<()> {
    // measured 2026-09-11: 1 violation, and one the left-anchor rule would repair. The
    // painted run ends on a space because the deletion was anchored right; the same
    // deletion anchored LEFT covers the same bytes, ends on a visible character, and so
    // satisfies this invariant as well as the rule. Re-painting that one run in
    // human_solver should take this to 0 - it needs the ground truth edited, not codediff.
    // Painting 'Only one solution', before row 1: `// Copyright 2021 Gin Core Team.  All
    // rights reserved.` - the doubled space is the point of the fixture.
    assert_ground_truth_invariants_with_known_violations(
        "go-gin-gonic-gin-whitespace-in-comment",
        1,
    )
}
