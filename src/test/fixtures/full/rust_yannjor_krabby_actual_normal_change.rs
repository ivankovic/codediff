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
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants_with_known_violations;
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    // Mostly `APTED("qualified_name")` (a name-keyed search that does not reach across a changed
    // path) and `APTED("fast_fallback")` (a Myers LCS that cannot align a moved node).
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "rust-yannjor-krabby-actual-normal-change",
        442,
        323,
    )
}

#[test]
fn invariants() -> Result<()> {
    // Invariant 16, nine times, around before rows 223-224 and after rows 109-110: the renames
    // `pokemon_db` <-> `pokemon_db_file`, `pokemon` <-> `pokemon_db` and `load_pokemon` <-> `load` are
    // not painted entire under Full, and `load_pokemon` <-> `load` is not painted as its differing
    // word `_pokemon` under Minimal.
    assert_ground_truth_invariants_with_known_violations(
        "rust-yannjor-krabby-actual-normal-change",
        9,
    )
}

#[test]
fn painting() -> Result<()> {
    assert_matches_human_painting_within_limit("rust-yannjor-krabby-actual-normal-change", 18.55)
}
