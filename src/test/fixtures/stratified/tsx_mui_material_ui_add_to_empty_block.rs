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
    // First baseline (2026-09-08), a measured gap and not a regression: the added
    // `it('To do', () => {});` carries an arrow function byte-identical to the `() => {}` that
    // was already there, so IdenticalHashOfAncestor anchors the outer arrow's punctuation onto
    // the newly inserted inner copy. The known phase-1 hash-matching preference for the
    // byte-identical inner copy on a wrap; 14 of the 23 are visible.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "tsx-mui-material-ui-add-to-empty-block",
        23,
        14,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-08: minimal 29.808%, full 27.885% (measured, unexamined)
    // re-measured 2026-09-08 after `RenderOptions::paint_displaced_moves` stopped `MINIMAL` painting a
    // span that kept its own text and its own place and shifted only because of an edit before it:
    // minimal 29.808% -> 20.192%. The option is off under `FULL`, which this fix leaves byte-identical
    // at 27.885%, so `FULL` sets the limit now. Any earlier number in this comment that disagrees with
    // these two predates unrelated rendering and ground-truth fixes and was never re-measured.
    assert_matches_human_painting_within_limit("tsx-mui-material-ui-add-to-empty-block", 27.90)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("tsx-mui-material-ui-add-to-empty-block")
}
