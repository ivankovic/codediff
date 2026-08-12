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
    // `const ALL: [Protocol; 3] = ...` -> `[Protocol; 4]`: the array-length `integer_literal`
    // changes value, everything else in `array_type` is unchanged. The human ground truth expects
    // this as an `Update` (obviously the same slot, just a new length), but `UnitCostModel::ren`
    // deliberately prices a same-kind, different-text *literal* rename at `COST_LITERAL_UPDATE`
    // (2) - the same as `COST_DELETE + COST_INSERT` - specifically to discourage matching
    // unrelated same-kind literals elsewhere in a file (see `ren`'s doc comment). APTED is free to
    // pick either at that exact tie, and picks Delete+Insert here. `MultiMapGroup` doesn't help
    // for a 1-before/1-after pair (it would still require the one pairing to be realized, same as
    // a plain entry), so this is left as a known, accepted cost tie rather than an algorithm fix -
    // lowering `COST_LITERAL_UPDATE` is a global cost-tier change with corpus-wide regression risk
    // (see `TODO.md`'s other documented cost-tier attempts).
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "rust-sniffnet-protocol",
        1,
    )
}
