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
use crate::test::helper::human_mapping::assert_matches_human_painting_within_limit;
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants;
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    // A callback-to-async/await rewrite: the top-level `fetchData(...)` call's identifier/arguments
    // would need to match their counterparts now nested inside an async IIFE's `await` expression -
    // a "bridge across added nesting" gap in the same family as `rust-algorithm-change`'s documented
    // case 2 (see that fixture's doc comment), not attempted here. Dropped 9 -> 6
    // (`TRIVIAL_ENTRY_MAX_SIZE` wrap/reparent fix, 2026-08-17) - some of this gap turned out to be
    // the same trivial-leaf-alongside-a-real-wrap shape as `cpp-add-templates`, not solely the
    // bridge-across-nesting gap described above.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "typescript-async-await",
        3,
        1,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-08-26: minimal 28.668%, full 33.634%
    // remeasured 2026-08-31 after ranges_for_options gained extend_leading_whitespace (`Full`
    // now paints a whole inserted line's own leading indentation, per RenderOptions::FULL's own
    // doc comment - see that function): minimal unchanged at 28.668%, full 34.312%. The extra
    // bytes are `return new Promise((resolve) => {`'s own indentation on line 2, which this
    // fixture's ground truth happens to leave unpainted even though the line is a whole new
    // insert - a defensible but not the only reading; not a regression in the rule this option
    // now actually honors.
    // re-measured 2026-09-08 after `RenderOptions::paint_displaced_moves` stopped `MINIMAL` painting a
    // span that kept its own text and its own place and shifted only because of an edit before it:
    // minimal 27.540% -> 2.709%. The option is off under `FULL`, which this fix leaves byte-identical
    // at 10.835%, so `FULL` sets the limit now. Any earlier number in this comment that disagrees with
    // these two predates unrelated rendering and ground-truth fixes and was never re-measured.
    assert_matches_human_painting_within_limit("typescript-async-await", 10.85)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("typescript-async-await")
}
