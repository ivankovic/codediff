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
use crate::test::helper::human_mapping::assert_matches_human_painting_within_limit;
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants;

#[test]
fn mapping() -> Result<()> {
    // One `return` becomes two: all-to-all 1:2 groups, whose second after members a one-to-one
    // output cannot reach (the N:M floor). The rest is the `qualified_name` nesting-bridging gap
    // of `rust-algorithm-change`: the call chain is deleted and the nested `if`/`return` inserted.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-defects4j-cli-3-typehandler",
        14,
        11,
    )
}

#[test]
fn painting() -> Result<()> {
    assert_matches_human_painting_within_limit("java-defects4j-cli-3-typehandler", 0.71)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("java-defects4j-cli-3-typehandler")
}
