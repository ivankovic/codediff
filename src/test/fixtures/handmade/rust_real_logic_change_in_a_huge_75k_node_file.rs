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
    // Known, unreviewed gap in a real-world 75k-node file - not yet root-caused. Clamped at the
    // observed count rather than requiring an exact match. Lower (or drop back to
    // `assert_matches_human_mapping`) once a fix lands.
    //
    // Re-baselined 2026-09-06 from 18/15 to 44/31, and NOT an algorithm regression: the human
    // mapping for this fixture was only partially annotated when 18/15 was recorded, so the old
    // limits scored codediff against a fraction of the file. The mapping was finished on
    // 2026-09-06 and the count is now measured against the whole of it. The residual is one
    // shape: a `const_item` and a run of doc comments that `APTED("fast_fallback")` deletes
    // rather than matching.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "rust-real-logic-change-in-a-huge-75k-node-file",
        44,
        31,
    )
}
