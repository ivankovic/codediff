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
    // 6 new <string> translation entries are inserted at scattered points in this strings.xml
    // resource file. XML's uniform inter-tag whitespace CharData nodes are frequently byte-
    // identical to each other (just "\n    " indentation); 585 of the 591 mismatches were exactly
    // this off-by-one CharData relabeling, cascading downstream from each insertion point because
    // `resolve_flat_tree_pair`'s Myers pass pooled every unmatched whitespace node into one flat
    // sequence with no already-matched `element` anchors left to resync against. Fixed to 0 by
    // splitting that flat child list into segments at already-matched boundaries first
    // (`split_into_anchored_segments`, `apted/common.rs`) so a shift after one insertion point
    // can't drift into the next - see `xml_nextcloud_android_delete_element.rs`'s comment for the
    // full mechanism.
    test::helper::human_mapping::assert_matches_human_mapping(
        "xml-nextcloud-android-add-few-translations",
    )
}
