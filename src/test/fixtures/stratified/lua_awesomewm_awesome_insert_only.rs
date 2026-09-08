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
    // A comment inserted into a run of neighbouring `--` comments. Every member of the run has
    // the same shape, so the pipeline slides the whole tail by one and pairs `comment:6` with
    // `comment:7`, `comment:7` with `comment:9`, and so on, while the human keeps each comment
    // with its own text and calls only the new one inserted. Two passes reach the same wrong
    // pairing independently (`IdenticalHashOfAncestor` first, `APTED("fast_fallback")` for the
    // rest), which is the tell that nothing here distinguishes the members of the run - the same
    // family as `ruby-mastodon-mastodon-move` and
    // `ruby-mastodon-mastodon-rare-example-of-true-move`, where a *rotation* of such a run
    // mis-pairs every member. Not attempted.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "lua-awesomewm-awesome-insert-only",
        18,
        12,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-09: minimal 3.170%, full 3.170% (measured, unexamined)
    assert_matches_human_painting_within_limit("lua-awesomewm-awesome-insert-only", 3.18)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("lua-awesomewm-awesome-insert-only")
}
