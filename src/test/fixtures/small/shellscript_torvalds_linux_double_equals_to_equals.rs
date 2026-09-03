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
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    // 2026-09-03: the clamp at 2,2 is gone - this fixture now maps exactly. The limit was stale
    // rather than a deliberate allowance: it had outlived the change that closed the gap, and
    // `quality_baseline.csv` was the only thing still holding this fixture to its real number.
    test::helper::human_mapping::assert_matches_human_mapping(
        "shellscript-torvalds-linux-double-equals-to-equals",
    )
}
