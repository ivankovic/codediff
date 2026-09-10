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
    test::helper::human_mapping::assert_matches_human_mapping("python-odoo-odoo-version")
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-10: minimal 0.112%, full 0.112%
    // Was 36.018% until the `own_content_span` guard in `classify_node` landed the same day. The
    // whole change is one word inside a `\`-continued triple-quoted string, and those
    // continuations split the string node's own content across several gaps - which is the shape
    // the guard now sends down the descent path instead of painting whole. See that guard's doc
    // comment in `diff::text`.
    assert_matches_human_painting_within_limit("python-odoo-odoo-version", 0.13)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("python-odoo-odoo-version")
}
