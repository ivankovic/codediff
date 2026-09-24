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
    // Pins two things on a ~1200-entry `strings.xml`:
    // - `nodes::is_reference` treats XML's `element` as a reference node; each entry is below
    //   `min_subtree_size`, so exact-hash matching would otherwise never see them.
    // - `resolve_flat_tree_pair` runs Myers per segment between already-matched elements
    //   (`split_into_anchored_segments`). Pooling every unmatched whitespace node into one sequence
    //   leaves Myers many tied alignments, and its tie-break drifts every node after a deletion.
    test::helper::human_mapping::assert_matches_human_mapping(
        "xml-nextcloud-android-delete-element",
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("xml-nextcloud-android-delete-element")
}
