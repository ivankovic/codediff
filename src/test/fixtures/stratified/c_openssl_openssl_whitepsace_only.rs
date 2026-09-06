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
    // Whitespace only
    test::helper::human_mapping::assert_matches_human_mapping("c-openssl-openssl-whitepsace-only")
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-05: minimal 8.173%, full 8.173%
    assert_matches_human_painting_within_limit("c-openssl-openssl-whitepsace-only", 8.19)
}

#[test]
fn invariants() -> Result<()> {
    // measured 2026-09-06: 5 painted rows end on a space.
    // This commit re-aligns the trailing `/* opener */`-style comments in a table of `NULL,`
    // entries, so on each of five rows the only thing that changed is the run of spaces between
    // the comma and the comment. The painter covered that run, which is the whole edit there and
    // the one place the "end on something visible" rule has nothing to offer: narrowing the span
    // to a visible character would paint the unchanged `NULL,` instead of the change.
    assert_ground_truth_invariants_with_known_violations("c-openssl-openssl-whitepsace-only", 5)
}
