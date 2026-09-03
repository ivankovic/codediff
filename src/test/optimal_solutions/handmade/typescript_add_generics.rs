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
    // The 4 predefined_type<->type_identifier mismatches (e.g. "number" becoming a generic "T")
    // are fixed - see TS_TYPE_KEYWORD_KINDS in nodes.rs. The remaining 10/7 is an unrelated
    // fast_fallback issue: the renamed "const container = new NumberContainer(42)" line fails to
    // match its "const numberContainer = new Container<number>(42)" counterpart at all.
    // 2026-09-03: tightened 10,7 -> 1,1. The limit was stale rather than a deliberate allowance: it
    // had outlived the change that closed the gap, and `quality_baseline.csv` was the only thing
    // still holding this fixture to its real number. Any counts above describe the older, larger
    // residual.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "typescript-add-generics",
        1,
        1,
    )
}
