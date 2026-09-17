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
    // **The mapping is deliberately incomplete**: its own description.md says "Requires N:M
    // mapping", and diffs.csv records 32 nodes it leaves unmapped because the format cannot express
    // the pairing. Every one of the 39 residuals is inside one `if_statement`, and the edit is why.
    // The before side tests `token.indexOf('=') != -1` and splits on it; the after side hoists that
    // into `int pos` and a ternary, tests something else entirely, and grows a second, nested `if
    // (pos != -1)` in the else branch. One before `if_statement` therefore answers to two after
    // ones plus a `ternary_expression`. The human picks the outer after-`if`; codediff's
    // `qualified_name` pass picks the nested one, which is where the identical `tokens.add(...)`
    // body went. Neither reading is wrong - the ground-truth format just cannot hold both. Expect
    // this limit to move when the mapping can be finished. That will be the ground truth changing,
    // not the algorithm regressing. java-defects4j-cli-19-posixparser is the same shape at a
    // fraction of the size.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-defects4j-cli-20-posixparser",
        39,
        28,
    )
}

#[test]
fn painting() -> Result<()> {
    // High for this corpus, and the same restructured `if` the mapping note describes is why:
    // codediff and the human disagree about which of the two after-`if`s the before one became, so
    // they paint different halves of it. Full costs three times minimal, which is the usual
    // direction - it keeps the standalone brackets and the leading whitespace minimal drops - but
    // the multiple was not examined.
    assert_matches_human_painting_within_limit("java-defects4j-cli-20-posixparser", 1.98)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("java-defects4j-cli-20-posixparser")
}
