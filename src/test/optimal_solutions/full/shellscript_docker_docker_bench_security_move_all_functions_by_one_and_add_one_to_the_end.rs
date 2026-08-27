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
    // Recorded distance from the human mapping, not a target: 334 mismatches (227 visible) of
    // 19036 nodes, 1.75%, measured 2026-08-27 when this fixture's mapping was authored. Lower it
    // when a change earns it; a rise is a regression.
    //
    // The fixture's name is the mechanism: every `function_definition` shifts by one, so the human
    // paired each before function with its neighbour on the after side, while codediff paired them
    // positionally and called the leftover an insert. 303 of the mismatches are
    // `StructurallyIdenticalAncestor` and 31 `StructurallyIdenticalSubtrees` - descendants
    // inheriting the container decision, not 334 independent ones.
    //
    // Codediff's answer costs 210 against the human's 175. A gap of 35 rather than a tie, so this
    // is not the indifferent-cost case `research/data/quality/move_attribution.md` describes: the
    // positional pairing really is the more expensive answer here, and something is picking it
    // anyway.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "shellscript-docker-docker-bench-security-move-all-functions-by-one-and-add-one-to-the-end",
        334,
        227,
    )
}
