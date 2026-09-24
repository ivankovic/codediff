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
    // The brace-attribution gap in the `if let`-chain collapse: `solve_nested_condition_collapse`
    // leaves each wrapper level's `{`/`}` where hash descent put them (innermost) rather than on
    // the outermost wrapper; see that module's doc comment.
    test::helper::human_mapping::assert_matches_human_mapping("rust-next-font-imports-generator")
}

#[test]
fn painting() -> Result<()> {
    // `FULL` depends on `reconcile_moves` not calling two overlapping accounts of one relocation
    // (the de-indented `if let` chain, four columns apart) a conflict, which would blank the after
    // side over sixty-one rows.
    assert_matches_human_painting_within_limit("rust-next-font-imports-generator", 6.91)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("rust-next-font-imports-generator")
}
