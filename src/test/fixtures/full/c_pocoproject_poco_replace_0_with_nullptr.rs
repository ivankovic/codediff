/*  This file is part of the OmniDiff code diffing tool.
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
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants_with_known_violations;
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    test::helper::human_mapping::assert_matches_human_mapping(
        "c-pocoproject-poco-replace-0-with-nullptr",
    )
}

#[test]
fn invariants() -> Result<()> {
    // Invariant 18, three times: `binary_expression.right` holds `0` on before rows 151, 164 and 165
    // and `nullptr` on the same after rows. The parents are matched, yet the mapping deletes each `0`
    // and inserts each `nullptr`.
    assert_ground_truth_invariants_with_known_violations(
        "c-pocoproject-poco-replace-0-with-nullptr",
        3,
    )
}
