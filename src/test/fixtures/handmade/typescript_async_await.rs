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
use crate::test::helper::human_mapping::assert_matches_human_painting_within_limit;
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants;
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    // A callback becomes async/await: the top-level `fetchData(...)` call's parts now sit inside an
    // async IIFE's `await`. The bridge-across-added-nesting gap of `rust-algorithm-change`'s
    // case 2, partly the trivial-leaf-beside-a-wrap shape of `cpp-add-templates`.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "typescript-async-await",
        2,
        0,
    )
}

#[test]
fn painting() -> Result<()> {
    // `FULL` sets the limit. It paints the whole inserted `return new Promise((resolve) => {` line
    // including indentation, which the ground truth leaves unpainted: defensible either way.
    assert_matches_human_painting_within_limit("typescript-async-await", 10.85)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("typescript-async-await")
}
