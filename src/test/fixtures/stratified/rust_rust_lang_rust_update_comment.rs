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
    // measured 2026-09-11: minimal 0.193%, full 4.499%
    // The right-anchored alternative painting was removed on 2026-09-11: inside a node
    // whose value we read character by character, ground truth anchors an ambiguous
    // add/delete LEFT. `intra_node_update_ranges` takes the common prefix first and the
    // suffix of the remainder, so it is right-anchored by construction and still emits the
    // dropped spelling. This residual is that disagreement, and it is expected - it is the
    // price of the rule, not a regression. See TODO.md for why the renderer was not
    // flipped to match (it would fix 4 fixtures and break 12).
    // Superseded the 2026-09-06 remeasurement, which tracked a `Minimal (right)` Delete
    // that no longer exists.
    assert_matches_human_painting_within_limit("rust-rust-lang-rust-update-comment", 4.51)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("rust-rust-lang-rust-update-comment")
}
