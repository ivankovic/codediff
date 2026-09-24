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
    // Cost-model optimal, not a bug. `assert(session):stop()` gets wrapped in a new `if`. codediff
    // matches the outer function-body `block` to the inner if-block (both holding just that call)
    // and inserts the wrapper: outer-to-outer would need the call's whole subtree deleted and
    // reinserted, since `expression_statement` vs `if_statement` cannot update. Tree edit
    // distance is built to find such cross-depth reuse; compare `c-nginx-add-typedef`.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "lua-neovim-neovim-add-if-around-one-line",
        2,
        0,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("lua-neovim-neovim-add-if-around-one-line")
}
