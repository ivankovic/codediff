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
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants_with_known_violations;
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
    // Re-measured 2026-09-08 and tightened 4,4 -> 2,2: the delimiter fix in 974cc062
    // (`reclaim_slot_level_twins`, "Give a delimiter back to the construct it closes") removed 2
    // of them. A limit above the measured number is a test that cannot fail, which is what
    // `the_quality_baseline_accuracy_columns_are_a_projection_of_the_stub_limits` exists to catch
    // - the baseline records the measurement, so the stub has to record it too.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "rust-next-font-imports-generator",
        2,
        2,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-01: minimal 6.537%, full 22.212% (measured, unexamined) - minimal dropped
    // from 37.788% after paint_reindent_only_moves shipped (see solve_nested_condition_collapse)
    assert_matches_human_painting_within_limit("rust-next-font-imports-generator", 22.24)
}

#[test]
fn invariants() -> Result<()> {
    // measured 2026-09-07, down from 4 on 2026-09-06: 2 mapped brace pairs still disagree, on
    // rows 92/144 and 97/143 - the `if let Some(decl)` / `if let Some(expr)` nesting, with the
    // human's pairing crossed between them. The 22/86 and 24/84 pair was re-paired by hand on
    // 2026-09-06. Part of the same unreviewed region as this fixture's painting residual.
    assert_ground_truth_invariants_with_known_violations("rust-next-font-imports-generator", 2)
}
