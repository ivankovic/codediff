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
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants_with_known_violations;

#[test]
fn mapping() -> Result<()> {
    // A `throw_statement` is replaced by an `expression_statement`: the human deletes one and
    // inserts the other, while codediff re-uses the `;` and reads the old `type_identifier` against
    // the new `identifier`. Four re-used leaves, two on each side - the scaffolding-reuse family
    // again.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-defects4j-cli-24-helpformatter",
        4,
        4,
    )
}

#[test]
fn painting() -> Result<()> {
    assert_matches_human_painting_within_limit("java-defects4j-cli-24-helpformatter", 0.12)
}

#[test]
fn invariants() -> Result<()> {
    // Invariant 1 under both presets, before row 825's last painted run ends on a space rather than
    // on a visible character (the `+` continuation of the `IllegalStateException` message).
    // Recorded as found; one painted range needs its trailing space trimmed.
    assert_ground_truth_invariants_with_known_violations("java-defects4j-cli-24-helpformatter", 2)
}
