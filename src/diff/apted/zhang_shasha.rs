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

use crate::code::ASTMetadata;

use super::common::{forest_dist, DeltaTable, ForestDist, PostorderIndexer, UnitCostModel};

/// Populates `delta[(pre_before, pre_after)]` - the fully-resolved tree edit distance between the
/// subtree rooted at `pre_before` and the subtree rooted at `pre_after` - for every keyroot pair.
/// Classic Zhang-Shasha keyroot decomposition (no `spfA`/`spfL`/`spfR` single-path optimization -
/// correct, simpler, and sufficient given APTED only ever runs on the small unmatched residual
/// left by the earlier, cheaper matching passes).
///
/// Keyroots are processed in ascending postorder index on both sides: this is what guarantees
/// that any `delta` lookup `forest_dist` performs for a given keyroot pair was already computed
/// in an earlier iteration (any interior point requiring a lookup is itself a keyroot pair with
/// strictly smaller postorder ids on both sides).
pub(crate) fn compute_delta_zhang_shasha(
    before: &PostorderIndexer,
    after: &PostorderIndexer,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    cost_model: &UnitCostModel,
) -> DeltaTable {
    let mut delta = DeltaTable::new(before.size.max(1), after.size.max(1));
    if before.size == 0 || after.size == 0 {
        return delta;
    }

    let mut before_keyroots = before.keyroots.clone();
    before_keyroots.sort_by_key(|&pre| before.pre_to_post[pre]);
    let mut after_keyroots = after.keyroots.clone();
    after_keyroots.sort_by_key(|&pre| after.pre_to_post[pre]);

    let mut forestdist = ForestDist::new(before.size + 1, after.size + 1);

    for &kr1_pre in &before_keyroots {
        let kr1_boundary = before.pre_to_post[kr1_pre] + 1;
        for &kr2_pre in &after_keyroots {
            let kr2_boundary = after.pre_to_post[kr2_pre] + 1;
            forest_dist(
                before,
                after,
                before_meta,
                after_meta,
                cost_model,
                &mut delta,
                kr1_boundary,
                kr2_boundary,
                &mut forestdist,
                true,
            );
        }
    }

    delta
}

