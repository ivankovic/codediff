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
    test::helper::human_mapping::assert_matches_human_mapping(
        "html-prettier-prettier-not-pure-html-includes-yaml-as-well",
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-05: minimal 39.227%, full 41.436%
    assert_matches_human_painting_within_limit(
        "html-prettier-prettier-not-pure-html-includes-yaml-as-well",
        41.45,
    )
}

#[test]
fn invariants() -> Result<()> {
    // measured 2026-09-06: 1 painted row ends on a space.
    // The row is `because: ` - the source line itself ends in a space, and the painted run covers
    // the whole line.
    assert_ground_truth_invariants_with_known_violations(
        "html-prettier-prettier-not-pure-html-includes-yaml-as-well",
        1,
    )
}
