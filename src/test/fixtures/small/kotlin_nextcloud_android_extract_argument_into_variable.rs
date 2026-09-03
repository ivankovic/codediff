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
    // 3 mismatches around the newly-extracted `val dim = ...` line's identifiers
    // (dimensions/i/dim) - `qualified_name`'s (formerly `syntax_named`, renamed 2026-08-14) same-
    // name matching picks different, equally plausible identifier pairings than the human's
    // semantically-intended ones. See TODO.md's "1 new optimal-solution fixture added, clamped"
    // entry.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "kotlin-nextcloud-android-extract-argument-into-variable",
        3,
        3,
    )
}
