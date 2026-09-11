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
        "csharp-sonarr-sonarr-fix-comment-typo",
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-11: minimal 0.000%, full 0.004%
    assert_matches_human_painting_within_limit("csharp-sonarr-sonarr-fix-comment-typo", 0.02)
}

#[test]
fn invariants() -> Result<()> {
    // measured 2026-09-11: 1 violation, and one the left-anchor rule would repair. The
    // painted run ends on a space because the deletion was anchored right; the same
    // deletion anchored LEFT covers the same bytes, ends on a visible character, and so
    // satisfies this invariant as well as the rule. Re-painting that one run in
    // human_solver should take this to 0 - it needs the ground truth edited, not codediff.
    // Painting 'Full', before row 234: a doubled `// ` in a comment, either copy deletable.
    assert_ground_truth_invariants_with_known_violations("csharp-sonarr-sonarr-fix-comment-typo", 1)
}
