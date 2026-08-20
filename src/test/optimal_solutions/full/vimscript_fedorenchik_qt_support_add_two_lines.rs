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
    // 19 mismatches: the whole script_file root plus its 18 leading (license-header) comment
    // lines all come back `reason=APTED("fast_fallback")` - DiffMode::Fast's cheap Myers-LCS
    // substitute for full APTED (see EXPENSIVE_RESIDUAL_THRESHOLD/PendingDiff::looks_expensive in
    // diff.rs), triggered because this two-line addition still leaves a large enough unmatched
    // residual. The fallback deletes+reinserts every one of the 18 identical comment lines instead
    // of matching them, rather than paying for exact tree-edit-distance. Not yet root-caused
    // further; lower (or drop back to assert_matches_human_mapping) if that's revisited.
    test::helper::human_mapping::assert_matches_human_mapping(
        "vimscript-fedorenchik-qt-support-add-two-lines",
    )
}
