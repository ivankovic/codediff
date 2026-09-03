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
    // This test contains an interesting ambigous situation:
    //
    // The added if clause can be mapped in two equally good ways. Either the inner or the outer
    // after if can map to the if in the before code. Expressed as two multi-map groups (inner and
    // outer `if`) since either pairing is valid - but codediff actually matches the inner `if` as
    // Identical rather than the group's declared MatchButNotIdentical, one mismatch beyond the
    // pre-multi-map 4.
    // 2026-09-03: tightened 5,0 -> 4,0. The limit was stale rather than a deliberate allowance: it
    // had outlived the change that closed the gap, and `quality_baseline.csv` was the only thing
    // still holding this fixture to its real number. Any counts above describe the older, larger
    // residual.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "c-linux-small-bugfix",
        4,
        0,
    )
}
