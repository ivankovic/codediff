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

#[test]
fn optimal_solution() -> Result<()> {
    // A new import replaces a large block of code. The human keeps the surviving imports paired;
    // `APTED("fast_fallback")` deletes them - the terminal Myers-LCS resolver, which cannot align
    // a node whose position in the residual forest moved. Same owner and same shape as the
    // reparenting gap recorded in `project_quality_goal_cost_anomaly_census`.
    // 2026-09-03: tightened 63,39 -> 53,30. The limit was stale rather than a deliberate allowance:
    // it had outlived the change that closed the gap, and `quality_baseline.csv` was the only thing
    // still holding this fixture to its real number. Any counts above describe the older, larger
    // residual.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "scala-com-lihaoyi-mill-new-import-used-to-remove-a-lot-of-code",
        53,
        30,
    )
}
