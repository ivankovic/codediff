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
    // The edit hoists an `if` condition into two `let` declarations, so every operator and
    // identifier in it gains an enclosing `let_declaration` where it used to sit under
    // `expression_statement`. The human pairs them across that reparent; `APTED("qualified_name")`
    // deletes them, because the names still match but the enclosing path no longer does - the same
    // search-quality gap that bucket has carried since 2026-08-17.
    //
    // Fittingly, the before/after here is this repository's own `diff/text.rs` across the
    // move-classification fix, so the fixture is a diff of the code that renders it.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "rust-small-addition-with-reuse-of-binary-expressions",
        10,
        5,
    )
}
