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
    // A `return` becomes a `throw` in the deepest arm of a nine-deep `if` chain. The human
    // mapping removes the whole `return_statement` and inserts the whole `throw_statement`;
    // codediff keeps the `;` the two have in common and pairs it across. Two mismatches, one per
    // statement, and both are that same semicolon seen from each side. Whether a shared delimiter
    // survives a statement being replaced is the kind of question invariant 18 deliberately does
    // not answer for a construct substitution - see the 2026-09-21 kind-mismatch census.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-defects4j-cli-40-typehandler",
        2,
        2,
    )
}

#[test]
fn painting() -> Result<()> {
    assert_matches_human_painting_within_limit("java-defects4j-cli-40-typehandler", 0.15)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("java-defects4j-cli-40-typehandler")
}
