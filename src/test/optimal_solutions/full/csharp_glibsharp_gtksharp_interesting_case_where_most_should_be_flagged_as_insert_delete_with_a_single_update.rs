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
    // One `identifier` the human pairs across a reshape (a method body's call becomes a
    // property's `return`) is deleted instead, by `APTED("qualified_name")`. The same
    // search-quality gap that bucket has carried since 2026-08-17: the name matches, but the
    // enclosing path differs enough that the pass does not reach for it.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "csharp-glibsharp-gtksharp-interesting-case-where-most-should-be-flagged-as-insert-delete-with-a-single-update",
        1,
        1,
    )
}
