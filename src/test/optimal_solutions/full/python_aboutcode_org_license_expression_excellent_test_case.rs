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
    // True solution requires a N:M multi-map because two strings should map to one. And it has
    // a interesting case of modifying the list with version numbers being removed and added,
    // which requires logical analysis not just syntax.
    // The largest residual in the corpus, and far fewer independent decisions than 251 suggests.
    // The human marks whole string literals as deleted with their subtrees; codediff keeps them
    // matched, since those bytes really are identical and appear in the same order on both sides.
    // Counted by node kind, the 251 are dominated by four kinds moving together - `string`,
    // `string_start`, `string_content` and `string_end` at 28 each, 112 in all - i.e. 28 literals
    // disagreed about, each contributing its whole subtree. Another 12 are
    // `APTED("qualified_name")` deletions, and 20 more are `pair`/`:`/`list` descendants of the
    // same shape.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "python-aboutcode-org-license-expression-excellent-test-case",
        251,
        186,
    )
}
