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
    // measured 2026-09-10: 10 total, 7 visible. The fixture is a moved import, which is the
    // import/include-list alignment family the 2026-09-08 mismatch census isolated as its own
    // cluster - a rotation in a run of same-kind siblings mis-pairs the members. The import-path
    // similarity matcher shipped in 4099ab9c reduced this family without closing it. Lower both
    // numbers when it closes.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "tsx-mui-material-ui-move-import",
        10,
        7,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-10: minimal 6.951%, full 6.951%
    // Update-vs-nothing in both directions plus one Move: the import/include-list alignment
    // family, the same one this fixture's mapping() clamp above records. Both should move together.
    assert_matches_human_painting_within_limit("tsx-mui-material-ui-move-import", 6.97)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("tsx-mui-material-ui-move-import")
}
