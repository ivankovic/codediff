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
use crate::test;
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants;
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    // Two tests merged into one, resolved by the human as an ordinary one-to-one mapping with no
    // multi-map group: a reachable target. Mostly `fast_fallback`.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "lua-luakit-luakit-actual-test-change-merging-two-tests-into-one",
        107,
        70,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants(
        "lua-luakit-luakit-actual-test-change-merging-two-tests-into-one",
    )
}
