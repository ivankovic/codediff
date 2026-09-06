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
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants;
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    test::helper::human_mapping::assert_matches_human_mapping("python-api-change")
}

#[test]
fn painting() -> Result<()> {
    // remeasured 2026-09-06 after `Full`'s Match over the untouched indentation of
    // `        return response.json()` was dropped - it painted eight spaces on a row whose own
    // text neither changed nor shifted, which is the one Full indentation entry here that was not
    // an insert or a re-indent: minimal 1.047%, full 4.188% -> 3.141%. The 25.0 it replaces was
    // recorded against the 2026-08-26 renderer and its own comment (26.505%/27.945%) had been
    // impossible for some time.
    assert_matches_human_painting_within_limit("python-api-change", 3.16)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("python-api-change")
}
