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
    // Every `function_definition` shifts by one: the human pairs each with its neighbour, codediff
    // positionally (`StructurallyIdenticalAncestor`, inherited by descendants). The positional
    // answer costs more than the human's, so this is not a cost tie; something picks it anyway.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "shellscript-docker-docker-bench-security-move-all-functions-by-one-and-add-one-to-the-end",
        334,
        227,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants(
        "shellscript-docker-docker-bench-security-move-all-functions-by-one-and-add-one-to-the-end",
    )
}
