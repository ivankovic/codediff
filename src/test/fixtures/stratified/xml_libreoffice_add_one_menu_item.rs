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
    // First baseline (2026-09-10), not a regression: this fixture was promoted with its human
    // mapping already written, so the stub's generated 0/0 never reflected a measurement.
    //
    // One `<menu:menuitem/>` added to a long run of sibling menu items, each preceded by a
    // byte-identical whitespace `CharData`. The run gains one member, and nothing but position
    // says which member is the new one. The human takes the last: before `CharData:13` becomes
    // after `CharData:13` and after `CharData:14` is the insert.
    // `APTED("large_flat_subtree_container")` takes an earlier one: it pairs before `CharData:13`
    // with after `CharData:14` and calls after `CharData:13` new. Both of the human's entries for
    // those two nodes are therefore wrong, and both nodes are visible. The same family as
    // `shellscript-pandas-dev-pandas-remove-one-line`. Lower both numbers when a fix lands.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "xml-libreoffice-add-one-menu-item",
        2,
        2,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-10: minimal 0.032%, full 0.032%
    assert_matches_human_painting_within_limit("xml-libreoffice-add-one-menu-item", 0.05)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("xml-libreoffice-add-one-menu-item")
}
