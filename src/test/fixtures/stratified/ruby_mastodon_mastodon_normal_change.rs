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
    // One `simple_symbol` in an `argument_list` is replaced by a different one.
    // `APTED("qualified_name")` renames it in place - same kind, same slot - and the list's
    // commas follow that pairing, while the human deletes the old symbol and inserts the new one
    // and keeps the commas with their surviving neighbours. A rename of a leaf costs less than a
    // delete plus an insert, so this is the cost model choosing, not a search gap; the comma
    // mismatches are bookkeeping downstream of that one choice. Not attempted.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "ruby-mastodon-mastodon-normal-change",
        4,
        4,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-09: minimal 3.880%, full 9.524% (measured, unexamined)
    assert_matches_human_painting_within_limit("ruby-mastodon-mastodon-normal-change", 9.54)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("ruby-mastodon-mastodon-normal-change")
}
