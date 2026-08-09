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
    // A new attribute_item is inserted among many textually-identical siblings. Not a multi-map
    // case (any one of the identical siblings being "the new one" isn't equally valid here - the
    // fixture's name documents that codediff should favour the *nearest* one, since position is
    // what disambiguates intent, not text): codediff instead picks an arbitrary one, so its
    // descendant subtree cascades into a big chunk of spurious keep/insert mismatches.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "rust-adding-to-a-list-of-identical-attributes-should-favour-near-matches",
        470,
    )
}
