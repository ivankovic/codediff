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
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants;
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    // Two deliberate human choices:
    // 1. `pwd = passwords->elts` -> `cb_data.pwd = (*passwords)->elts` should match: a small
    //    textual edit whose AST change is large (the dereference and parentheses), subjectively
    //    optimal rather than edit-distance optimal.
    // 2. `pwd++` -> `cb_data.pwd++` maps `identifier` "pwd" to `field_identifier` "pwd": matching
    //    by kind alone would update `pwd` into `cb_data` and insert a new "pwd", which a human would
    //    not. So cross-kind matches must be allowed.
    // Partly resolved by `prematch_identical_statement_siblings` and `ContainmentCtx`'s
    // sibling-order-consistency check.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "c-nginx-add-typedef",
        50,
        32,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("c-nginx-add-typedef")
}
