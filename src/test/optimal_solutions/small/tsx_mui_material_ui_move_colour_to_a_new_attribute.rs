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
    // The `color` jsx_attribute is deleted and its value reappears nested inside a new `sx={{...}}`
    // object property, not as a sibling attribute - codediff matches the surrounding attributes by
    // position instead, so the 4th attribute shifts into the old 3rd slot and the value's
    // identifier/string subtree gets flagged as changed rather than moved. 16 mismatches.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "tsx-mui-material-ui-move-colour-to-a-new-attribute",
        16,
        12,
    )
}
