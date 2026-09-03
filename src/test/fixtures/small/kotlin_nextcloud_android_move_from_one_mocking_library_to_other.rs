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
    // `val captor = argumentCaptor<FilesServiceCallback<OCFile>>()` is inserted twice, once per
    // test method, replacing a single class-level `@Captor` field each old method referenced -
    // two structurally-identical new local declarations with no earlier occurrence to anchor to,
    // an inherently ambiguous near-duplicate-insert case.
    // 2026-09-03: tightened 46,30 -> 24,12. The limit was stale rather than a deliberate allowance:
    // it had outlived the change that closed the gap, and `quality_baseline.csv` was the only thing
    // still holding this fixture to its real number. Any counts above describe the older, larger
    // residual.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "kotlin-nextcloud-android-move-from-one-mocking-library-to-other",
        24,
        12,
    )
}
