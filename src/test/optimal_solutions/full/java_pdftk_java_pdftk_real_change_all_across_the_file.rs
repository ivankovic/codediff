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
    // Recorded distance from the human mapping, not a target: 214 mismatches (144 visible) of
    // 8987 nodes, 2.38%, measured 2026-08-27 when this fixture's mapping was authored. Lower it
    // when a change earns it; a rise is a regression.
    //
    // Not an objective wall - codediff's answer costs 2585 against the human's 179, a gap of 2406.
    // The human mapping is optimal ground truth, so a gap that size is a defect with a known
    // owner, not the price of a hard case. 210 of the 214 mismatches carry
    // `APTED("large_flat_subtree")` and all of them sit under one `class_body`: its four
    // `constructor_declaration` children (plus two `line_comment`s) are deleted outright and
    // reinserted rather than matched across the edit, and every descendant of those subtrees is
    // dragged along - which is where the bulk of the 2406 comes from. The remaining 4 are one
    // `fast_fallback` and three multi-map-group pairs where codediff chose `Identical` for a
    // `binary_expression` the human recorded as `MatchButNotIdentical`.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-pdftk-java-pdftk-real-change-all-across-the-file",
        214,
        144,
    )
}
