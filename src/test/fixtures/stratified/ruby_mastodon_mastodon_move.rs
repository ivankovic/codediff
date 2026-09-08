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
    // Three neighbouring comments are rotated. `StructurallyIdenticalAncestor` pairs them by
    // position - `comment:6` with `comment:8`, `comment:7` with `comment:6`, `comment:8` with
    // `comment:7` - and calls all three `Update`, where the human follows each comment's own text
    // to where it moved. Position is the only signal any pass uses inside a run of same-kind
    // siblings, so a rotation mis-pairs every member of the run; see
    // `ruby-mastodon-mastodon-rare-example-of-true-move` for the same three-comment shape and
    // `lua-awesomewm-awesome-insert-only` for the insertion version of it. Not attempted.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "ruby-mastodon-mastodon-move",
        3,
        3,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-09: minimal 12.996%, full 13.380% (measured, unexamined)
    assert_matches_human_painting_within_limit("ruby-mastodon-mastodon-move", 13.39)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("ruby-mastodon-mastodon-move")
}
