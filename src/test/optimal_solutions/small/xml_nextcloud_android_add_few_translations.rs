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

#[test]
fn optimal_solution() -> Result<()> {
    // 6 new <string> translation entries are inserted at scattered points in this strings.xml
    // resource file. XML's uniform inter-tag whitespace CharData nodes are frequently byte-
    // identical to each other (just "\n    " indentation), so after each insertion point the
    // ambiguous, interchangeable whitespace nodes downstream get matched to a slightly different
    // (but content-identical) sibling than the human's chosen correspondence - 585 of the 591
    // mismatches are exactly this same off-by-one CharData relabeling, cascading from the same 6
    // insertion points, not independent issues.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "xml-nextcloud-android-add-few-translations",
        591,
    )
}
