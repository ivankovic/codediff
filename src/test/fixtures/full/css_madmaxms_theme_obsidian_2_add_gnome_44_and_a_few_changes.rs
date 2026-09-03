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
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    // Recorded distance from the human mapping, not a target: 125 mismatches (75 visible) of
    // 70234 nodes, 0.18%, measured 2026-08-27 when this fixture's mapping was authored. Lower it
    // when a change earns it; a rise is a regression.
    //
    // Two mechanisms, in roughly equal parts. 54 of the 69 attributed mismatches are
    // `StructurallyIdenticalAncestor` - a container matched on shape, whose descendants then
    // inherit an operation the human classified differently. The other 56 carry no reason at all
    // and are all one shape: the human marked a subtree `Insert`/`Delete (with children)`, so
    // every descendant should be unmapped, but codediff paired those descendants with
    // byte-identical copies elsewhere in the file. A stylesheet is mostly punctuation and repeated
    // property names, so almost every leaf has an identical twin to be captured by.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "css-madmaxms-theme-obsidian-2-add-gnome-44-and-a-few-changes",
        125,
        75,
    )
}
