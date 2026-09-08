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
    // The edit unwraps an IIFE: every statement moves from `call_expression > arguments >
    // function_expression > statement_block` up to the file's top level. The human pairs the
    // statements across those four removed levels; the pipeline's structural matchers do not
    // bridge removed nesting, so `APTED("fast_fallback")` deletes and re-inserts the string and
    // its punctuation. The same "bridge across added/removed nesting" gap `rust-algorithm-change`
    // documents as its case 2 and `typescript-async-await` carries in the other direction. Not
    // attempted here.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "javascript-microsoft-typescript-refactor",
        10,
        6,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-09: minimal 2.965%, full 3.877% (measured, unexamined)
    assert_matches_human_painting_within_limit("javascript-microsoft-typescript-refactor", 3.89)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("javascript-microsoft-typescript-refactor")
}
