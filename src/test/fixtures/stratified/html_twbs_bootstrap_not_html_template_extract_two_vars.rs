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
    // Not pure HTML, actually a template, and needs N:M even if it parsed. Both mismatches are an
    // `attribute_value` the human calls inserted, which `StructurallyIdenticalAncestor` pairs with
    // the old one as an `Update`, positionally under an identical `element` chain.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "html-twbs-bootstrap-not-html-template-extract-two-vars",
        2,
        2,
    )
}

#[test]
fn painting() -> Result<()> {
    // `FULL` sets the limit.
    assert_matches_human_painting_within_limit(
        "html-twbs-bootstrap-not-html-template-extract-two-vars",
        24.14,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("html-twbs-bootstrap-not-html-template-extract-two-vars")
}
