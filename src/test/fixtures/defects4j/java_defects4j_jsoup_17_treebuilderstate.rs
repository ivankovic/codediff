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
    // The same shape as `java-defects4j-cli-8-helpformatter` next door: an argument changes
    // lexical class in place - `hex_integer_literal` against `character_literal` - and the human
    // mapping pairs it while codediff deletes and inserts (reason `APTED("qualified_name")`).
    // Also outside invariant 18's reach, and for the same reason: `argument_list` has no fields,
    // so the slot that makes the pairing obvious to a reader is not one the grammar names.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-defects4j-jsoup-17-treebuilderstate",
        1,
        1,
    )
}

#[test]
fn painting() -> Result<()> {
    assert_matches_human_painting_within_limit("java-defects4j-jsoup-17-treebuilderstate", 0.0)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("java-defects4j-jsoup-17-treebuilderstate")
}
