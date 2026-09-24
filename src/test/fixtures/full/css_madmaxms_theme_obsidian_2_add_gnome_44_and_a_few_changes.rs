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
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants;
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    // Two mechanisms in equal parts: `StructurallyIdenticalAncestor` containers whose descendants
    // inherit an operation, and descendants of human-deleted/inserted subtrees paired with
    // byte-identical twins elsewhere (a stylesheet is mostly repeated punctuation and names).
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "css-madmaxms-theme-obsidian-2-add-gnome-44-and-a-few-changes",
        125,
        75,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("css-madmaxms-theme-obsidian-2-add-gnome-44-and-a-few-changes")
}
