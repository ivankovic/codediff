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
    // Deliberately pathological, like the CSS "completely broken treesitter parsing" fixture:
    // this is TypeScript compiler test fixture data, not real code, and doesn't parse cleanly -
    // ERROR nodes dominate the tree, and node correspondence through them is essentially
    // undefined.
    // Re-measured 2026-09-08: the residual is gone. The delimiter fix in 974cc062
    // (`reclaim_slot_level_twins`, "Give a delimiter back to the construct it closes") settled the
    // sibling-choice tie above, so this is exact now and asserts so with the exact call shape
    // rather than a limit of zero.
    test::helper::human_mapping::assert_matches_human_mapping(
        "javascript-microsoft-typescript-test-data-pretending-to-be-code-maybe-broken-parsing",
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants(
        "javascript-microsoft-typescript-test-data-pretending-to-be-code-maybe-broken-parsing",
    )
}
