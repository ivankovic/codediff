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

#[test]
fn mapping() -> Result<()> {
    // Whitespace only. Minimal painting is empty.
    test::helper::human_mapping::assert_matches_human_mapping(
        "c-openssl-openssl-format-only-change",
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-06: minimal 4.800%, full 11.189%
    // The largest disagreement among the fixtures added on 2026-09-06, and not a mapping defect:
    // this commit only reflows whitespace, and interior whitespace lives in the gaps between AST
    // nodes, where no painting can reach it. codediff paints the reflowed statements themselves;
    // the human painted nothing under Minimal. Recorded as the distance it is, not as a target.
    assert_matches_human_painting_within_limit("c-openssl-openssl-format-only-change", 11.20)
}
