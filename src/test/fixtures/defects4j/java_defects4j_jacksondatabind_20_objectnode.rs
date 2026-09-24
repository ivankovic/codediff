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
    // Recorded as found, not examined.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-defects4j-jacksondatabind-20-objectnode",
        32,
        22,
    )
}

#[test]
fn painting() -> Result<()> {
    assert_matches_human_painting_within_limit("java-defects4j-jacksondatabind-20-objectnode", 0.0)
}

#[test]
fn invariants() -> Result<()> {
    // The checkpoint's human solution is not yet clean:
    //  - [11] x2: the after painting (Minimal and Full) leaves five leaves of the removed
    //    `com....JsonAutoDetect` import unpainted on row 3, while the mapping says they are gone.
    //  - [16] x3: the `JsonAutoDetect` <-> `JsonIgnore` rename is painted whole under Minimal on
    //    both sides, where only the differing words should be, and not whole under Full before.
    assert_ground_truth_invariants_with_known_violations(
        "java-defects4j-jacksondatabind-20-objectnode",
        5,
    )
}
