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
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants;
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    // Not an objective wall: codediff's cost exceeds the human's, so the search misses a better
    // mapping. Mostly `APTED("large_flat_subtree")` and `MovedSubtree` deleting and reinserting the
    // `class_body`'s constructors with every descendant.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-pdftk-java-pdftk-real-change-all-across-the-file",
        354,
        243,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("java-pdftk-java-pdftk-real-change-all-across-the-file")
}
