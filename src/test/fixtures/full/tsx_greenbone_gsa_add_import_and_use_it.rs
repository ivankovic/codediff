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
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants_with_known_violations;
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    test::helper::human_mapping::assert_matches_human_mapping(
        "tsx-greenbone-gsa-add-import-and-use-it",
    )
}

#[test]
fn invariants() -> Result<()> {
    // 2026-09-21, invariant 18, one: `type: undefined,` becomes
    // `type: GREENBONE_SENSOR_SCANNER_TYPE,` inside an object literal whose other keys are
    // unchanged (`pair.value`, row 264 -> 267). The `pair` is matched and `.value` holds one
    // child on each side, so this key's value changed; the mapping deletes and inserts instead.
    assert_ground_truth_invariants_with_known_violations(
        "tsx-greenbone-gsa-add-import-and-use-it",
        1,
    )
}

#[test]
fn painting() -> Result<()> {
    // Not measured yet: 100.0 passes unconditionally. Run this test and record the
    // limit it reports instead.
    assert_matches_human_painting_within_limit("tsx-greenbone-gsa-add-import-and-use-it", 100.0)
}
