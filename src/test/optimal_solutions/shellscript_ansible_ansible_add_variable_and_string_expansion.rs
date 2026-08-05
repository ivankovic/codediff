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

#[test]
fn optimal_solution() -> Result<()> {
    // 2026-08-06: fixed exactly (28 -> 0) by `apted::prematch_unique_named_locals` - `group`
    // ("shift-due-to-insertion": `group="${args[4]}"` shifts to `group="${args[5]}"` when a new
    // `powershell="${args[4]}"` line is inserted right before it) is now pre-matched by variable
    // name before the file-root `final_pass` call gets a chance to prefer the cheaper-but-wrong
    // by-position pairing. See that function's doc comment and `TODO.md`'s "shift-due-to-
    // insertion" entry for the full cost-model root cause this closes.
    test::helper::human_mapping::assert_matches_human_mapping(
        "shellscript-ansible-ansible-add-variable-and-string-expansion",
    )
}
