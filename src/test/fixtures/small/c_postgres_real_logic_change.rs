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
use crate::test;
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants;
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    // Many "human quality" choices:
    // 1. Two `if(remote_slot->confirmed_lsn > latestFlushPtr)` sites merge into one, previously
    //    `if(found_consistent_snapshot)` and `if(remote_slot_preceedes)`; either mapping is
    //    defensible (the second matches the logic better, the first avoids a leading Delete).
    // 2. In the same `if`, two identical `ereport`s become one: 2-to-1.
    // 3. The third modified `ereport` simply moved, which a reviewer should see.
    // 4. Its surrounding `if` matches both the before `if` around the `ereport` and the one at the
    //    same location: another 2-to-1.
    // The unmarked nodes left are all at that N:M site, which no `MultiMapGroup` can encode; the
    // limit is high because the mapping is specific, not because codediff is wrong there.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "c-postgres-real-logic-change",
        124,
        82,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("c-postgres-real-logic-change")
}
