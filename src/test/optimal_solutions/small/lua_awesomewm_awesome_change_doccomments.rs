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
    // Two near-identical doc comment blocks (vertical/horizontal alignment) are rewritten in
    // parallel, each swapping prose and reformatting a `* **word**` bullet list to `` * `"word"` ``
    // - comment nodes carry no syntactic substructure to disambiguate by, so matching individual
    // rewritten comment lines to their old counterparts across two near-duplicate blocks is
    // inherently ambiguous, not a matcher bug. `solve_leading_siblings` (formerly
    // `solve_comment_nodes`) walks a whole chain of leading comments rather than just one hop,
    // which correctly anchors some of the unchanged comment lines in these blocks; the rest are
    // the genuinely ambiguous rewritten ones.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "lua-awesomewm-awesome-change-doccomments",
        21,
    )
}
