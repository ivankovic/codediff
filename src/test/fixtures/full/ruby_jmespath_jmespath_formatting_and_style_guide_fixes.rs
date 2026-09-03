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
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    // A whole-file restyle: `while_modifier` rewritten as `while`, `%i[...]` symbol arrays
    // rewritten as `[...]` arrays. The human pairs the constructs across those rewrites;
    // `APTED("qualified_name")` deletes them, since the node kinds themselves changed on both
    // sides. Also the fixture with the most residual move asymmetry after `reconcile_moves` -
    // see `research/data/quality/move_attribution.md`, which names it for the same reason: many
    // small independent reorders in one file.
    // 2026-09-03: tightened 18,16 -> 17,15. The limit was stale rather than a deliberate allowance:
    // it had outlived the change that closed the gap, and `quality_baseline.csv` was the only thing
    // still holding this fixture to its real number. Any counts above describe the older, larger
    // residual.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "ruby-jmespath-jmespath-formatting-and-style-guide-fixes",
        17,
        15,
    )
}
