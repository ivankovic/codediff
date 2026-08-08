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
    // Single-quoted `raw_string`s become double-quoted `string`s (different node kinds), so the 2
    // conversions aren't recognized as Updates; one `list` also gets wrapped in a new `pipeline`,
    // shifting its `&&` token's path. 3 mismatches total.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "shellscript-langchain-ai-langchain-some-interesting-raw-string-to-string-content",
        3,
    )
}
