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
    // Recorded distance from the human mapping, not a target: 2 mismatches (both visible) of
    // 2282 nodes, 0.09%, measured 2026-08-27 when this fixture's mapping was authored. Lower it
    // when a change earns it; a rise is a regression.
    //
    // Both are the same decision seen from two sides, and both carry `APTED("qualified_name")`:
    // two `comment` nodes in one `declaration_list`, where the human said the second before-side
    // comment corresponds to the *fourth* after-side one and the second after-side one is new,
    // while codediff paired them positionally. Costs tie exactly at 95 - which makes this a cost
    // function that cannot tell the two answers apart, not an answer codediff was unable to reach.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "php-theseer-directoryscanner-add-two-test-cases-and-reformat-file",
        2,
        2,
    )
}
