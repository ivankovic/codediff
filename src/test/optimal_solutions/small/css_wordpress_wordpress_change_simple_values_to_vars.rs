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
    // A plain `integer_value` (e.g. `35px`) becomes a `float_value` wrapped in a new
    // `call_expression`/`arguments` (e.g. `var(--x, 35px)`) - too structurally different (kind,
    // depth, and container all change at once) for APTED to bridge, so it deletes the old value
    // instead of matching it into the new wrapped position.
    // 2026-09-03: the clamp at 1,1 is gone - this fixture now maps exactly. The limit was stale
    // rather than a deliberate allowance: it had outlived the change that closed the gap, and
    // `quality_baseline.csv` was the only thing still holding this fixture to its real number. Any
    // counts above describe a residual that no longer exists.
    test::helper::human_mapping::assert_matches_human_mapping(
        "css-wordpress-wordpress-change-simple-values-to-vars",
    )
}
