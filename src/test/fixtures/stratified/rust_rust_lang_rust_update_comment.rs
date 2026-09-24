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
use crate::test::helper::human_mapping::assert_matches_human_painting_within_limit;
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants;

#[test]
fn mapping() -> Result<()> {
    test::helper::human_mapping::assert_matches_human_mapping("rust-rust-lang-rust-update-comment")
}

#[test]
fn painting() -> Result<()> {
    // Ground truth anchors an ambiguous add/delete inside a value LEFT; `intra_node_update_ranges`
    // is right-anchored by construction. An expected residual, the price of that rule.
    assert_matches_human_painting_within_limit("rust-rust-lang-rust-update-comment", 4.51)
}

#[test]
fn invariants() -> Result<()> {
    // No node is unmatched, so invariant 9 must not read the renderer's `Delete` of an edited
    // character (a colon the painter and `TextDiff` put on opposite sides) as a missing partner.
    assert_ground_truth_invariants("rust-rust-lang-rust-update-comment")
}
