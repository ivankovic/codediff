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
    // 2026-08-05: dropped 1141 -> 856 by teaching `nodes::is_reference` that XML's `element` is a
    // reference node (see that function's own doc comment) - this ~1200-entry Android
    // `strings.xml` file was tripping `EXPENSIVE_RESIDUAL_THRESHOLD` (94% of the file unmatched
    // despite being 99.9% byte-identical to `after`) purely because every `<string name="...">
    // ...</string>` entry is far smaller than `min_subtree_size` (45), so exact-hash matching
    // never got the chance to find them. The remaining mismatches were all `CharData` whitespace
    // separators between entries. Went 856 -> 857 as an incidental side effect of the 2026-08-08
    // `solve_large_flat_subtrees` recursion fix.
    //
    // Fixed to 0: `resolve_flat_tree_pair`'s Myers pass (`apted/common.rs`) used to pool every
    // still-unmatched flat child into one sequence diff, which silently dropped the ~1137
    // already-matched `element` siblings surrounding each whitespace run - with no anchors left in
    // the sequence, a run of hash-identical whitespace gives Myers many tied-optimal alignments,
    // and its own tie-break (not ground truth) picked which one "moved" whenever an element was
    // deleted, drifting every whitespace node after the deletion point by one. Fixed by splitting
    // the flat child list into segments at already-matched boundaries first
    // (`split_into_anchored_segments`) and running Myers per segment, so a shift on one side of an
    // anchor can no longer misalign anything on the other side of it. Purely a mismatch-count fix:
    // `algorithm_cost == human_cost` was already true here, so rendered diff output is unchanged.
    test::helper::human_mapping::assert_matches_human_mapping(
        "xml-nextcloud-android-delete-element",
    )
}
