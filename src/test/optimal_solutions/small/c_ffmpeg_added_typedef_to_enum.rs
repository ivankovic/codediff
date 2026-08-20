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
    // A small residual of the 2026-08-08 `solve_large_flat_subtrees` recursion fix (see TODO.md):
    // one new `typedef` insertion sits right next to a `;` that should've mapped identically, and
    // the APTED sub-resolution over the Myers-unmatched residual picks a slightly different, still
    // globally-optimal-cost mapping for that one semicolon. Small, understood, and dominated by the
    // fix's corpus-wide net improvement (-9 mismatches; this fixture alone went 16 -> 4 after the
    // fix, from a pre-fix baseline of 0 before `solve_large_flat_subtrees` could even reach it).
    test::helper::human_mapping::assert_matches_human_mapping("c-ffmpeg-added-typedef-to-enum")
}
