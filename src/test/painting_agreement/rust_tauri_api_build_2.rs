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

use crate::test::helper::human_mapping::assert_matches_human_painting_within_limit;

#[test]
fn painting_agreement() -> Result<()> {
    // measured 2026-08-27: minimal 0.901% (32 of 3550 bytes), full 0.000% (0 bytes)
    // measured 2026-09-02: minimal 0.000%, full 0.930% (33 bytes) - the asymmetry flipped.
    //
    // `ranges()`'s `shifted_by_an_edit_beside_it` rule (a single-row node slid sideways by an
    // edit elsewhere on its own row is not a Move) took Minimal to exact, and costs those 33
    // bytes under Full: `|v| v == "true")` on `.map_or(false, ...)` -> `.is_ok_and(...)`, which
    // this fixture's dedicated Full painting calls a Move and its Minimal painting does not.
    // The three fixtures the rule fixed outright (`cpp-add-const-correctness`,
    // `kotlin-fix-loop-bug`, `java-fix-array-index`) carry one painting for both modes that
    // paints no Move for the same shape, so the two readings of "pure same-row repositioning
    // under Full" cannot both be honoured until those fixtures get a Full painting of their
    // own. Accepted deliberately; not a `ranges()` bug to chase.
    assert_matches_human_painting_within_limit("rust-tauri-api-build-2", 0.93)
}
