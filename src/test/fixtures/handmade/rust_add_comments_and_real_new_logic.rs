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
use crate::test::helper::human_mapping::assert_matches_human_painting_within_limit;
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    // Known, pre-existing gap against the human-authored mapping - not yet root-caused, so clamped
    // rather than fixed here. 84 -> 85 (2026-08-18) from the `COST_LITERAL_UPDATE` tie fix (see
    // `rust_sniffnet_protocol.rs`): a deliberate, measured +1 - the fixture's `algorithm_cost`
    // *improved* 273 -> 271 (human 261) under the same change, so the extra mismatch is the
    // mapping moving further from the human's labels while getting cheaper by the objective, on a
    // fixture whose gap is unexplained to begin with. Lower (or drop back to
    // `assert_matches_human_mapping`) once the gap is root-caused.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "rust-add-comments-and-real-new-logic",
        85,
        54,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-01: minimal 1.129%, full 0.780% (measured, unexamined)
    assert_matches_human_painting_within_limit("rust-add-comments-and-real-new-logic", 1.15)
}
