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
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    // 4 mismatches, all the same documented brace-attribution gap in the `if let`-chain collapse:
    // `solve_nested_condition_collapse` deliberately leaves each wrapper level's own `{`/`}`
    // tokens matched wherever phase 1's hash descent already put them (innermost), rather than
    // re-attributing them to the outermost wrapper - a prior attempt at that re-attribution was
    // reverted after measuring it disagreed with this fixture's own hand-painted ground truth in
    // a way that wasn't simply "backwards" (see that module's own doc comment for the measurement
    // and why a real fix needs a clearer picture of what the ground truth wants, not a second
    // guess at the same theory).
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "rust-next-font-imports-generator",
        4,
        4,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-01: minimal 6.537%, full 22.212% (measured, unexamined) - minimal dropped
    // from 37.788% after paint_reindent_only_moves shipped (see solve_nested_condition_collapse)
    assert_matches_human_painting_within_limit("rust-next-font-imports-generator", 22.24)
}
