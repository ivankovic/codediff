/*  This file is part of the OmniDiff code diffing tool.
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
    test::helper::human_mapping::assert_matches_human_mapping(
        "c-openssl-openssl-very-interesting-test-case",
    )
}

#[test]
fn painting() -> Result<()> {
    assert_matches_human_painting_within_limit("c-openssl-openssl-very-interesting-test-case", 3.3)
}

#[test]
fn invariants() -> Result<()> {
    // Invariant 16: `okeylen` -> `md_len` on after row 321 is not painted as its differing word `md_`
    // under Minimal. Invariant 5, twice: Full leaves columns 31..32 unpainted between two painted
    // regions on before row 298 and after row 301.
    assert_ground_truth_invariants_with_known_violations(
        "c-openssl-openssl-very-interesting-test-case",
        3,
    )
}
