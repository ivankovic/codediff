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
    // A <table> element gets wrapped in a new enclosing <div> (structural move up one level).
    // codediff's match for the wrapped element's own start_tag/tag_name chain diverges from the
    // human's (MatchButNotIdentical vs. Update on tag_name), and one attribute's subtree isn't
    // fully swept by the delete instead of being followed into the new wrapper.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "html-gohugoio-hugo-enclose-table-with-div-and-add-thead-tbody",
        4,
        3,
    )
}
