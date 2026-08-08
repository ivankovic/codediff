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
    // Same cascading pattern as xml-nextcloud-android-add-few-translations (see its own doc
    // comment): new attributes on two existing elements plus 7 new <string> elements (with their
    // own preceding comments) inserted at scattered points. XML's uniform inter-tag whitespace
    // CharData nodes are frequently byte-identical to each other, so downstream of each
    // insertion the ambiguous whitespace nodes get matched to a slightly different (but
    // content-identical) sibling than the human's chosen correspondence.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "xml-mozilla-firefox-firefox-add-a-few-translations-and-a-few-attributes",
        70,
    )
}
