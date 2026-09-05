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
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    // This is a great diff that has many "human quality" choices:
    //
    // 1. Two separate "if(remote_slot->confirmed_lsn > latestFlushPtr)" got refactored into a
    //    single site. That site was previously "if(found_consistent_snapshot)" and
    //    "if(remote_slot_preceedes)". Of the two ifs in before, "if(remote_slot_preceedes)" matches
    //    the new logic slightly more, but either would be equaly good mapping to humans. If the
    //    first if is mapped, the logic is slightly off, but the mapping is clearly visible. If the
    //    second if is mapped, the logic maps better, but then the UX is worse because the user is
    //    faced with a Delete first, which looks like a coding error but isn't.
    // 2. In the same if as 1., the ereport has a 2-to-1 mapping. Two separate instances of the same
    //    error got refactored into a single instance.
    // 3. The third modified ereport has simply moved, and it would be good for humans to know
    //    during review that the message existed before.
    // 4. The if that surrounds 3. is interesting. The if condition is the same as the before if
    //    surrounding the ereport, but the slot_persistence_pending being set to true has been
    //    deleted. However, the if condition also matches the before if at the same location, but
    //    with a bit of pointer dereference removed. Logically, it is the same condition however.
    //    This is again a possible 2-to-1 mapping where we would like to show both on the same
    //    location and the original location of the ereport that the code has been Moved/Updated.
    //
    //    TODO: Deal with mutli-to-multi mapps. We can't represent this either in the mapping or
    //    visually at this time!
    //
    // 2026-09-05: the human mapping was re-solved and is now essentially complete - 14327 unmarked
    // nodes down to 302, all of them at the N:M site described above, which no `MultiMapGroup` can
    // encode. Almost all of this file was previously ungraded, so the limit moving 27/16 -> 124/82
    // is the ground truth getting more specific, not codediff regressing: the new mismatches are
    // on nodes the old mapping simply said nothing about.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "c-postgres-real-logic-change",
        124,
        82,
    )
}
