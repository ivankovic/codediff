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
    // A `!$var` negation moves from being the top-level condition into a new binary_expression
    // (an added `||` condition wraps it) - codediff's APTED pass maps the moved unary_op_expression
    // subtree to Delete/Insert instead of following it into the new binary_expression, the same
    // kind of structural cross-boundary move gap documented elsewhere in this suite.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "php-wordpress-wordpress-add-some-checks",
        10,
    )
}
