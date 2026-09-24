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
    // Top-level functions move verbatim into a new class. The human marks them Delete + Insert (a
    // new class, not a refactor); `APTED("fast_fallback")` cannot align nodes that moved deeper,
    // yet other passes match some across the wrapper. The opposite human preference from
    // `java_add_exception_handling`'s same shape, so no single heuristic serves both.
    // Nine all-to-all 2:1 groups: the two identical `width: Double, height: Double` parameter lists
    // become the one constructor's, each leaving one before member unavoidably unmatched.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "kotlin-refactor-function",
        74,
        54,
    )
}

#[test]
fn painting() -> Result<()> {
    // `FULL` sets the limit.
    assert_matches_human_painting_within_limit("kotlin-refactor-function", 58.61)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("kotlin-refactor-function")
}
