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
    // Re-baselined 2026-09-07 from 6/4, and the one fixture `reclaim_slot_level_twins` costs. Its
    // nested `token_tree`s are matched one level off - the multimap gap this fixture's name
    // records - and its delimiters used to contradict that decision in a way that happened to land
    // on the human's answer. They now follow their own container, so they are wrong for the same
    // reason it is, rather than by a second bug cancelling the first. See that function's doc
    // comment for why that trade is the right way round.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "rust-vercel-nextjs-refactoring-would-require-mulitmap-mapping",
        10,
        8,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("rust-vercel-nextjs-refactoring-would-require-mulitmap-mapping")
}
