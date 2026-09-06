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
    // Whitespace only
    test::helper::human_mapping::assert_matches_human_mapping("c-openssl-openssl-whitepsace-only")
}

#[test]
fn painting() -> Result<()> {
    // repainted 2026-09-06, and the limit went UP: 8.19 -> 21.12 (minimal 21.109%, full 21.109%).
    // The whole commit is the alignment run between `NULL,` and `/* opener */` collapsing to one
    // space, and the painting used to be exactly that - five whitespace-only Delete spans, each
    // ending on a space. There is no narrower painting that ends on a visible character, so the
    // five rows are now painted whole, as the Updates a reader sees. The bigger number is the
    // honest one: it measures codediff painting nothing at all here, because interior whitespace
    // lives in the gaps between AST nodes where no painting can reach (same wall as
    // c-openssl-openssl-format-only-change). Recorded as the distance it is, not as a target.
    assert_matches_human_painting_within_limit("c-openssl-openssl-whitepsace-only", 21.12)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("c-openssl-openssl-whitepsace-only")
}
