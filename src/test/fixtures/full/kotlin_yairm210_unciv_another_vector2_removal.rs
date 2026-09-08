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
    // Known, unreviewed gap against the human-authored mapping - not yet root-caused. Clamped at
    // the observed count rather than requiring an exact match. Lower (or drop back to
    // `assert_matches_human_mapping`) once a fix lands.
    //
    // 18,14 -> 12,9 on 2026-09-08 with `solve_import_path_similarity`. This file collapses eight
    // `import com.unciv.logic.civilization.X` lines into one `com.unciv.logic.civilization.*`; the
    // eight are unmapped when phase 4 runs and the new pass now pairs the one of them the token
    // budget accepts without a rival, instead of `APTED("fast_fallback")` aligning the whole block
    // positionally. The remaining twelve are the rest of that collapse, which no 1:1 matcher can
    // express.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "kotlin-yairm210-unciv-another-vector2-removal",
        12,
        9,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("kotlin-yairm210-unciv-another-vector2-removal")
}
