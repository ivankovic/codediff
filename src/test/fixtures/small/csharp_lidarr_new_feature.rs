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
    // 2026-08-06: fixed exactly (16 -> 0) by `apted::prematch_unique_named_locals` - the local
    // variable `resources` (`var resources = MapToResource(...)`) shifts from the 4th to the 8th
    // declaration in `GetCalendar`'s body when 4 new declarations are inserted before it; now
    // pre-matched by variable name before real APTED resolves the rest of the method. See that
    // function's doc comment and `TODO.md`'s "shift-due-to-insertion" entry.
    test::helper::human_mapping::assert_matches_human_mapping("csharp-lidarr-new-feature")
}
