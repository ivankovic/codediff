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
    // `const ALL: [Protocol; 3] = ...` -> `[Protocol; 4]`: the array-length `integer_literal`
    // changes value, everything else in `array_type` is unchanged. Zero as of 2026-08-18: this
    // fixture used to be the documented casualty of `COST_LITERAL_UPDATE` = 2 being *exactly*
    // `COST_DELETE + COST_INSERT` - a tie APTED resolved as Delete+Insert against the human's
    // obvious `Update`. The 2026-08-18 tie scan measured both escapes: raising to 3 changed
    // nothing corpus-wide (the tie was already always resolving toward delete+insert, so "2 to
    // discourage" was functionally a forbid), lowering to 1 fixed this fixture and was net -4
    // mismatches / +1 zero-mismatch fixture. See `ren`'s doc comment.
    test::helper::human_mapping::assert_matches_human_mapping("rust-sniffnet-protocol")
}
