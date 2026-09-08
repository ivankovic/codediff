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
    // A rare example of a true move
    // The same rotated-comment-run shape as `ruby-mastodon-mastodon-move`, and the fixture the
    // corpus keeps for it being a *true* move: `StructurallyIdenticalAncestor` pairs the three
    // comments by position and calls them `Update`, the human follows each comment's text to
    // where it went. Not attempted - see that fixture's comment for why position is the only
    // signal available inside a run of same-kind siblings.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "ruby-mastodon-mastodon-rare-example-of-true-move",
        3,
        3,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-09: minimal 14.928%, full 15.542% (measured, unexamined)
    assert_matches_human_painting_within_limit(
        "ruby-mastodon-mastodon-rare-example-of-true-move",
        15.56,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("ruby-mastodon-mastodon-rare-example-of-true-move")
}
