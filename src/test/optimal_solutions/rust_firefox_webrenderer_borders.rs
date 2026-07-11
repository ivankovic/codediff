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
    // This test case has several corner cases:
    //
    // 1. Usual case of multiple possible valid mappings for chains of same node kind.
    //
    // In particular, the code "common.prim_rect.size()" changing to "common.prim_size" changes the
    // AST from:
    //
    // call_expression
    //   field_expression
    //     field_expression
    //       identifier "common"
    //       .
    //       field_identifier "prim_rect"
    //   .
    //   field_identifier "size"
    //
    //  to:
    //
    //  field_expression
    //    identifier "common"
    //    .
    //    field_identifier "prim_size"
    //
    // There is no optimal cost difference between mapping the field_expression on the after side to
    // either of the field_expression nodes on the before side. The cost is the same either way.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit("rust-firefox-webrenderer-borders", 18)
}
