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
use crate::test::helper::human_mapping::assert_matches_human_painting_within_limit;
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants_with_known_violations;

#[test]
fn mapping() -> Result<()> {
    // One rotation of a `use_list`, counted per node. The human reads the names as moving (with
    // their commas); `APTED("import_list_overlap")` matches by overlap, so a set-preserving
    // rotation reads as unchanged in place. The move-detection gap in an import list.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "rust-rust-lang-rust-change-use",
        6,
        6,
    )
}

#[test]
fn painting() -> Result<()> {
    // `FULL` sets the limit.
    assert_matches_human_painting_within_limit("rust-rust-lang-rust-change-use", 7.02)
}

#[test]
fn invariants() -> Result<()> {
    // Invariant 16: the Minimal/Full split for a renamed identifier is not painted this way yet
    // (`Abi`/`CfgAbi` and `abi`/`cfg_abi`).
    assert_ground_truth_invariants_with_known_violations("rust-rust-lang-rust-change-use", 6)
}
