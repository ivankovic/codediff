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
        "php-wordpress-wordpress-not-sure-if-this-parses-correctly",
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-10: minimal 0.008%, full 0.009%
    assert_matches_human_painting_within_limit(
        "php-wordpress-wordpress-not-sure-if-this-parses-correctly",
        0.02,
    )
}

#[test]
fn invariants() -> Result<()> {
    // Invariant 11, found when it was added on 2026-09-14: the mapping inserts two `text` leaves
    // at after row 7801 (`</script>` and the line break after it, in the region this fixture's
    // name already doubts the parse of) and neither painting has a byte of them. Counted once
    // per painting.
    assert_ground_truth_invariants_with_known_violations(
        "php-wordpress-wordpress-not-sure-if-this-parses-correctly",
        2,
    )
}
