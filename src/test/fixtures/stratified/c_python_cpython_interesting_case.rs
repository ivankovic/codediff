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
        "c-python-cpython-interesting-case",
        219,
        143,
    )
}

#[test]
fn painting() -> Result<()> {
    assert_matches_human_painting_within_limit("c-python-cpython-interesting-case", 11.28)
}

#[test]
fn invariants() -> Result<()> {
    // Invariant 2: Minimal paints 10 bytes that Full leaves unpainted, on before rows 67-70 and after
    // rows 74-78. Invariant 16, twice: the `_PyOpcode_RecordFunction_CALLER_CODE` <->
    // `_PyOpcode_RecordFunction_CODE` rename (before row 55, after row 68) is not painted entire under
    // Full.
    assert_ground_truth_invariants_with_known_violations("c-python-cpython-interesting-case", 3)
}
