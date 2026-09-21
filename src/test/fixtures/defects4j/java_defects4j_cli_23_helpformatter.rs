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
    // A `throw` in one `if` arm becomes an expression statement in another. The mapping removes
    // and inserts both statements whole; codediff keeps what the two have in common and pairs it
    // across - the `;`, and the name that is a `type_identifier` on one side and an `identifier`
    // on the other. Four of the five are that reuse, the fifth an `if_statement` codediff matches
    // where the mapping does not (reason `APTED("large_flat_subtree")`).
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-defects4j-cli-23-helpformatter",
        5,
        4,
    )
}

#[test]
fn painting() -> Result<()> {
    assert_matches_human_painting_within_limit("java-defects4j-cli-23-helpformatter", 0.12)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("java-defects4j-cli-23-helpformatter")
}
