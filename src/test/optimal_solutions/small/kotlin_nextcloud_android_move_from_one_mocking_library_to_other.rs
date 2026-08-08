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
    // `val captor = argumentCaptor<FilesServiceCallback<OCFile>>()` is inserted twice, once per
    // test method, replacing a single class-level `@Captor` field each old method referenced -
    // two structurally-identical new local declarations with no earlier occurrence to anchor to,
    // an inherently ambiguous near-duplicate-insert case.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "kotlin-nextcloud-android-move-from-one-mocking-library-to-other",
        50,
    )
}
