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
        "tsx-shadcn-ui-ui-order-of-class-names",
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-10: minimal 3.226%, full 3.226%
    // Two `ours=Update, theirs=Move` - a class-attribute reorder that codediff reads as a
    // rewrite. Move-vs-Update, the second-largest painting family.
    assert_matches_human_painting_within_limit("tsx-shadcn-ui-ui-order-of-class-names", 3.24)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("tsx-shadcn-ui-ui-order-of-class-names")
}
