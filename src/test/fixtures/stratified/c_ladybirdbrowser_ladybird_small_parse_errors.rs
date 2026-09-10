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
    // Impossible to map because of parse errors. Painting is correct.
    //
    // measured 2026-09-10: 1 total, 0 visible - and the one mismatch is exactly that parse error.
    // Both sides carry a tree-sitter `ERROR` node (its parse-failure placeholder); the human
    // mapping pairs them, codediff deletes before's instead, because after's sits one level
    // deeper - under an added `expression_statement` - and APTED("qualified_name") does not
    // follow it down. Nothing a reader can see is affected: `ERROR` is scaffolding, which is why
    // the visible count is 0 while the total is 1.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "c-ladybirdbrowser-ladybird-small-parse-errors",
        1,
        0,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-10: minimal 0.013%, full 0.025%
    assert_matches_human_painting_within_limit(
        "c-ladybirdbrowser-ladybird-small-parse-errors",
        0.04,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("c-ladybirdbrowser-ladybird-small-parse-errors")
}
