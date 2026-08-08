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
    // Despite the .js extension, the fixture content isn't valid JavaScript (it's a TypeScript
    // compiler test-baseline dump - tab-indented virtual file paths and serialized string
    // contents, per the "broken-js" name). Tree-sitter's error-recovery parsing of malformed
    // input is inherently unstable: removing one short substring from a ~940-line file of mostly
    // parse-error tokens reshuffles error-recovery boundaries throughout, so a tiny textual edit
    // cascades into a large, but not meaningfully wrong, mapping difference.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "javascript-microsoft-typescript-broken-js-remove-string-fragment",
        1105,
    )
}
