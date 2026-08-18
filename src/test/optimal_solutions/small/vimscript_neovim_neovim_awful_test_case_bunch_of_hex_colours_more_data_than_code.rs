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
    // 22 -> 24 (2026-08-17) -> 19 (2026-08-18), and the round trip is the point. The ambiguity
    // guard `solve_moved_subtrees` gained in 626045c refuses to pick between several identical
    // move targets below `AMBIGUOUS_MOVE_MIN_SIZE` nodes, which cost this file (a hex colour table
    // - "more data than code", so its content is short, repetitive and genuinely relocated) 11 ->
    // 23 mismatches; the guard was still worth -38 corpus-wide, so the ceiling was raised
    // deliberately rather than silently. The guard now consults the similarity sketch before
    // refusing (`disambiguate_by_context`), which recovers most of that here and takes the file
    // below where it started. See `TODO.md`'s 2026-08-18 similarity-sketch section.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "vimscript-neovim-neovim-awful-test-case-bunch-of-hex-colours-more-data-than-code",
        19,
    )
}
