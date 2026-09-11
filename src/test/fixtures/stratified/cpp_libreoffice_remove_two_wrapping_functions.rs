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
    // measured 2026-09-11: 2 mismatch(es), 2 visible. Two wrapping calls are removed and an
    // argument list loses one of two identical `,` tokens. As with
    // rust-rust-lang-rust-remove-path-from-using, either comma is a defensible choice and
    // disagreeing about which costs exactly two.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "cpp-libreoffice-remove-two-wrapping-functions",
        2,
        2,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-11: minimal 0.091%, full 0.006%
    assert_matches_human_painting_within_limit(
        "cpp-libreoffice-remove-two-wrapping-functions",
        0.11,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("cpp-libreoffice-remove-two-wrapping-functions")
}
