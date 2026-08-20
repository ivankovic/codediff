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
    // Reformatting minified CSS into one-declaration-per-line swaps the order of two
    // structurally-identical-shaped declaration pairs within the same rule_set (e.g.
    // `margin-bottom` then `margin-top` before, `margin-top` then `margin-bottom` after, in both
    // `:where(.wp-block-post-excerpt)` and `.wp-block-post-excerpt__excerpt`). APTED's final pass
    // finds an equal-cost mapping that pairs each declaration with its positional counterpart
    // (Updating `margin-bottom`'s node into `margin-top`'s text) rather than following the
    // property name across the reorder - a locality-optimal solution the human ground truth
    // doesn't share, same class of ambiguous-mapping gap as `c_postgres_real_logic_change`.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "css-wordpress-reformat",
        30,
        22,
    )
}
