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
    // `findWrapPos(text, width, nextLineTabStop)` becomes `findWrapPos(text, width, 0)`. The
    // human mapping pairs the third argument across the kind change (`identifier` against
    // `decimal_integer_literal`) because `text` and `width` are untouched and nothing else is
    // left for the `0` to be; codediff deletes the identifier and inserts the literal instead
    // (reason `APTED("large_flat_subtree")`).
    //
    // Invariant 18 does not require this pairing and cannot: an `argument_list` in
    // tree-sitter-java declares no fields at all, so the third argument has no named slot to
    // persist. Position here is fixed by elimination rather than by a field, which is a wider
    // rule than the one that shipped - see the census's "what follows from this".
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-defects4j-cli-8-helpformatter",
        1,
        1,
    )
}

#[test]
fn painting() -> Result<()> {
    assert_matches_human_painting_within_limit("java-defects4j-cli-8-helpformatter", 0.0)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("java-defects4j-cli-8-helpformatter")
}
