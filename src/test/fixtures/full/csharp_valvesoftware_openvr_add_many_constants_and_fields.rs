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
    // Many constants and fields added to a large declaration list. Roughly half the residual is
    // `StructurallyIdenticalAncestor` (52) - codediff inherits a match from an ancestor whose
    // shape survived, where the human paired the members individually - with
    // `APTED("qualified_name")` (32) and `MovedSubtree` (13) accounting for most of the rest.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "csharp-valvesoftware-openvr-add-many-constants-and-fields",
        106,
        58,
    )
}
