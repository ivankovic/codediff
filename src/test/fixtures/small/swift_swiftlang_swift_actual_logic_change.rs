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
    // A call moves from inside a closure literal into a new if_statement branch - the same
    // structural cross-boundary move gap as the scrcpy for-loop fixture - plus two multi-map
    // group pairings codediff doesn't realize.
    //
    // 2026-09-02: 36/19 -> 37/20 when this fixture's human mapping was re-verified by hand. Not
    // an algorithm regression - nothing under `src/diff/` changed, only the ground truth being
    // scored against. One extra node, in the same already-documented cross-boundary move.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "swift-swiftlang-swift-actual-logic-change",
        37,
        20,
    )
}
