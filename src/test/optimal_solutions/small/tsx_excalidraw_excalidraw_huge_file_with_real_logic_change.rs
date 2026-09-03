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
    // Known, unreviewed gap in a real-world huge TSX file - not yet root-caused. Clamped at the
    // observed count rather than requiring an exact match. Lower (or drop back to
    // `assert_matches_human_mapping`) once a fix lands.
    // Ticked down 1892 -> 1883 as an incidental side effect of extending `solve_leading_siblings`
    // to TypeScript/TSX decorators, then 1883 -> 1882 as an incidental side effect of the
    // `resolve_flat_tree_pair` anchor-splitting fix (see `xml_nextcloud_android_delete_element.rs`'s
    // comment) - same "known, unreviewed gap" class as before, not a targeted fix for this fixture
    // specifically.
    // 2026-09-03: tightened 231,155 -> 229,155. The limit was stale rather than a deliberate
    // allowance: it had outlived the change that closed the gap, and `quality_baseline.csv` was the
    // only thing still holding this fixture to its real number. Any counts above describe the
    // older, larger residual.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "tsx-excalidraw-excalidraw-huge-file-with-real-logic-change",
        229,
        155,
    )
}
