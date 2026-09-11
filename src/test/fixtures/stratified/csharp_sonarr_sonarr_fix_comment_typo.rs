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
    // Repaired in the ground truth on 2026-09-11 and back to 0. `Full` used to delete the
    // second of the two `// ` copies on row 234, ending its run on a space; it now deletes the
    // first, which covers the same bytes, ends on `/`, and is the left-anchored spelling the
    // rule in RULES_AND_PREFERENCES.md asks for. The invariant and the rule agreed here.
    assert_ground_truth_invariants("csharp-sonarr-sonarr-fix-comment-typo")
}
