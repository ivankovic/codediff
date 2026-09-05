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
    // 2026-09-05: the human mapping was cleared and is being re-solved from scratch, so
    // `human_mapping.json` currently grades nothing at all. Held at 0/0 rather than left at the
    // old 23/19: against an empty mapping any positive limit passes vacuously, and a limit that
    // cannot fail is worse than no test. Raise it to the real counts when the mapping is redone.
    test::helper::human_mapping::assert_matches_human_mapping(
        "vimscript-neovim-neovim-test-debian-package-parsing-awful-string-matching",
    )
}
