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
    // 2026-08-06: fixed exactly (10 -> 0), a side effect of `apted::prematch_unique_named_locals`
    // (added for `shellscript-ansible-...`'s shift-due-to-insertion gap - see that fixture's own
    // comment and `TODO.md`) - not independently investigated, but the same mechanism.
    test::helper::human_mapping::assert_matches_human_mapping(
        "shellscript-genymobile-scrcpy-add-two-flags",
    )
}
