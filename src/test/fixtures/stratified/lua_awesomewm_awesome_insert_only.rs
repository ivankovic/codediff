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
    // A comment inserted into a run of same-shaped `--` comments: the pipeline slides the tail by
    // one (`IdenticalHashOfAncestor`, then `APTED("fast_fallback")`), while the human keeps each
    // comment with its own text. Nothing distinguishes run members but position; see
    // `ruby-mastodon-mastodon-move` for the rotation version.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "lua-awesomewm-awesome-insert-only",
        18,
        12,
    )
}

#[test]
fn painting() -> Result<()> {
    assert_matches_human_painting_within_limit("lua-awesomewm-awesome-insert-only", 3.18)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("lua-awesomewm-awesome-insert-only")
}
