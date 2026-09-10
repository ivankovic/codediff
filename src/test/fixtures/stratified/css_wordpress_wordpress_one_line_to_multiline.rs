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
    test::helper::human_mapping::assert_matches_human_mapping(
        "css-wordpress-wordpress-one-line-to-multiline",
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-10: minimal 71.581%, full 3.419%
    // The corpus's largest painting disagreement. Minified CSS expanded to multi-line: every
    // disagreement is `ours=Move, theirs=None`, i.e. codediff calls each rule a Move because the
    // added comment and newlines displaced it, while the human painting says a rule shifted by an
    // edit beside it has not moved. That is the Move-vs-nothing family (57% of all painting
    // disagreements, 2026-09-08), amplified here because the file is 196 bytes and one insertion
    // displaces all of it. The rate is recorded to stop it growing, NOT endorsed.
    assert_matches_human_painting_within_limit(
        "css-wordpress-wordpress-one-line-to-multiline",
        71.60,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("css-wordpress-wordpress-one-line-to-multiline")
}
