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
use crate::test::helper::human_mapping::assert_matches_human_painting_within_limit;
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants_with_known_violations;
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    // Unavoidable under the cost model: one slot changes from `None, # page_table,` to
    // `block_table, # block_table,`, and `none` vs `identifier` is not on `kinds_update_allowed`,
    // so a forced update (3) loses to delete plus insert (2).
    test::helper::human_mapping::assert_matches_human_mapping(
        "python-pytorch-pytorch-add-param-to-many-places-and-update-one",
    )
}

#[test]
fn invariants() -> Result<()> {
    // Invariant 10, both presets: the `,` on before row 299 and after row 301 is painted gone and
    // new, where the tree mapping calls it the same text.
    assert_ground_truth_invariants_with_known_violations(
        "python-pytorch-pytorch-add-param-to-many-places-and-update-one",
        2,
    )
}

#[test]
fn painting() -> Result<()> {
    assert_matches_human_painting_within_limit(
        "python-pytorch-pytorch-add-param-to-many-places-and-update-one",
        0.17,
    )
}
