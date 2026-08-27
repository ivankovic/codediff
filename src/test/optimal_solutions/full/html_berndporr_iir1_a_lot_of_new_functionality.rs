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
    // For some reason, code between @license tags is jsut raw_text
    // Recorded distance from the human mapping, not a target: 749 mismatches (519 visible) of
    // 8800 nodes, 8.5%, measured 2026-08-27 when this fixture's mapping was authored. The worst
    // rate in the corpus by a wide margin, and the reason is above: with the licence block landing
    // as one raw_text node, the parser hands the differ a tree whose shape says much less about
    // the edit than the text does.
    //
    // 281 of the 367 attributed mismatches are `APTED("fast_fallback")` - the size-guarded cheap
    // path, taken here because the subtrees are large. The remaining 382 carry no reason and are
    // the descendants of subtrees the human marked `Insert`/`Delete (with children)`: HTML tag
    // scaffolding and quote characters are byte-identical throughout the file, so phase-1 hash
    // matching pairs them straight across the insert/delete boundary the human drew.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "html-berndporr-iir1-a-lot-of-new-functionality",
        749,
        519,
    )
}
