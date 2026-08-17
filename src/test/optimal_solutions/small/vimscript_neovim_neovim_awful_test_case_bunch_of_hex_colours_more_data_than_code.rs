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
    // Raised 22 -> 24 (2026-08-17), a deliberate and measured regression on *this* fixture, not a
    // drifting ceiling. `solve_moved_subtrees`' new ambiguity guard refuses to pick between several
    // identical move targets below `AMBIGUOUS_MOVE_MIN_SIZE` nodes, which costs this file (a hex
    // colour table - "more data than code", so its content is short, repetitive and genuinely
    // relocated) 11 -> 23 mismatches. The same guard takes
    // `python-django-...-update-unit-tests-actual-logic-change` from 48 to **zero** and is worth
    // -38 mismatches corpus-wide, so the trade is net positive on every target metric; see
    // `TODO.md`'s 2026-08-17 move-detection section for the full before/after table.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "vimscript-neovim-neovim-awful-test-case-bunch-of-hex-colours-more-data-than-code",
        24,
    )
}
