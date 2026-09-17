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
use crate::test;
use crate::test::helper::human_mapping::assert_matches_human_painting_within_limit;
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants;
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    test::helper::human_mapping::assert_matches_human_mapping("kotlin-add-data-class")
}

#[test]
fn painting() -> Result<()> {
    // A genuine matching gap, not a rendering-option question - human marks shifted name/age
    // parameter names Move, codediff leaves them Identical - not attempted. minimal 3.310%, full
    // 8.983% - four bytes of the gap above were a relocation the after side had not been told
    // about, see `reconcile_moves`. The matching gap described above is the rest and is still not
    // attempted.
    assert_matches_human_painting_within_limit("kotlin-add-data-class", 9.0)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("kotlin-add-data-class")
}
