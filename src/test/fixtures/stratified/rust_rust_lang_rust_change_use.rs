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
    // The 6 residual mismatches (all visible) are one rotation of a `use_list`, counted once per
    // node it touches. The human reads the edit as the names moving - `identifier:1` to
    // `identifier:3`, `:2` to `:1`, `:3` to `:2`, with each separating comma following its name -
    // while `APTED("import_list_overlap")` pairs each list member with the one at its own index.
    // That pass is built to match import lists by overlap rather than by position, so a rotation
    // that preserves the set is exactly the shape it reads as "unchanged, in place".
    //
    // The known move-detection gap in an import list. Recorded 2026-09-04 as a measured gap, not
    // accepted as correct.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "rust-rust-lang-rust-change-use",
        6,
        6,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-05: minimal 7.092%, full 7.008%
    // re-measured 2026-09-08 after `RenderOptions::paint_displaced_moves` stopped `MINIMAL` painting a
    // span that kept its own text and its own place and shifted only because of an edit before it:
    // minimal 7.092% -> 6.420%. The option is off under `FULL`, which this fix leaves byte-identical
    // at 7.008%, so `FULL` sets the limit now. Any earlier number in this comment that disagrees with
    // these two predates unrelated rendering and ground-truth fixes and was never re-measured.
    assert_matches_human_painting_within_limit("rust-rust-lang-rust-change-use", 7.02)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("rust-rust-lang-rust-change-use")
}
