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
    // measured 2026-09-11: 8 mismatch(es), 7 visible. This fixture is in the corpus because both
    // sides contain CSS that tree-sitter cannot parse, so much of the tree is ERROR nodes and the
    // mapping is being asked to align rubble. Recorded as-is rather than tuned against.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "css-twbs-bootstrap-parse-errors",
        8,
        7,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-11: minimal 2.138%, full 3.246%
    assert_matches_human_painting_within_limit("css-twbs-bootstrap-parse-errors", 3.26)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("css-twbs-bootstrap-parse-errors")
}
