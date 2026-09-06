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
    test::helper::human_mapping::assert_matches_human_mapping("rust-rust-lang-rust-update-comment")
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-05: minimal 0.064%, full 4.499%
    assert_matches_human_painting_within_limit("rust-rust-lang-rust-update-comment", 4.51)
}

#[test]
fn invariants() -> Result<()> {
    // measured 2026-09-06: 1 painted row ends on a space, in the `Minimal (right)` alternative.
    // The comment's trailing space is inside the repainted run.
    assert_ground_truth_invariants_with_known_violations("rust-rust-lang-rust-update-comment", 1)
}
