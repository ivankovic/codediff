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
    // Recorded distance from the human mapping, not a target: 458 mismatches (320 visible) of
    // 20684 nodes, 2.2%, measured 2026-08-27 when this fixture's mapping was authored.
    //
    // Unlike its two siblings in this bucket, every mismatch here is attributed, and to just two
    // reasons: `StructurallyIdenticalAncestor` (352) and `IdenticalHashOfAncestor` (96). Both are
    // inheritance - a container is matched, on shape or on hash, and its descendants take the
    // operation that follows from that rather than the one the human gave them. So this is a
    // single mechanism disagreeing at scale, not a scattering of separate defects, which makes it
    // a better candidate to root-cause than the raw count suggests.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "html-chennes-med-extreme-test",
        458,
        320,
    )
}
