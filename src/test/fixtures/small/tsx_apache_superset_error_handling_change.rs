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
    // A whole new ternary expression is inserted into JSX, but a couple of its string-literal/
    // quote leaf tokens coincidentally match identical tokens elsewhere in the file, so codediff
    // partially matches those leaves instead of treating the whole subtree as new.
    //
    // 2026-09-02: 3/2 -> 6/4 when this fixture's human mapping was re-verified by hand. Not an
    // algorithm regression - nothing under `src/diff/` changed. The re-verified mapping records
    // the real relationship as 1:2 (see this fixture's own `description.md`), which codediff
    // cannot express at all: one before-side string becomes two after-side ones, so every leaf
    // codediff pairs one-to-one is scored wrong however it pairs them. This limit will only come
    // down with N:M support, not with a better matcher.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "tsx-apache-superset-error-handling-change",
        6,
        4,
    )
}
