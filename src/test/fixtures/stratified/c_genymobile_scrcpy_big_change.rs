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
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants_with_known_violations;

#[test]
fn mapping() -> Result<()> {
    // First baseline, not a regression: this fixture was promoted with its human mapping already
    // written, so the stub's generated 0/0 never reflected a measurement.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "c-genymobile-scrcpy-big-change",
        102,
        69,
    )
}

#[test]
fn painting() -> Result<()> {
    assert_matches_human_painting_within_limit("c-genymobile-scrcpy-big-change", 40.44)
}

#[test]
fn invariants() -> Result<()> {
    // 2026-09-21, invariant 18, one: `devices->count = 0;` becomes
    // `devices->keyboard = keyboard;` and the mapping deletes the `0` while inserting the
    // `keyboard` (`assignment_expression.right`, row 7 -> 12), although it matches everything
    // else in that statement - the assignment, the `devices->...` field expression, the `=`, and
    // `count` -> `keyboard` as an `Update`. That last one is the tell: a same-kind rename to an
    // unrelated name was recorded as a match, while the cross-kind `0` -> `keyboard` in the same
    // statement was not, which is the schema's doing rather than the author's judgement.
    assert_ground_truth_invariants_with_known_violations("c-genymobile-scrcpy-big-change", 1)
}
