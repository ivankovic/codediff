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
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants;

#[test]
fn mapping() -> Result<()> {
    // measured 2026-09-10: 20 total, 14 visible. Every one of them is the same disagreement,
    // reported as `MatchButNotIdentical, reason APTED("qualified_name")`: the human mapping pairs
    // the first `call_expression` on each side, codediff pairs before's first with after's
    // *second*, and the whole subtree under it (member_expression, arguments, ...) follows the
    // parent into the wrong slot. That is the qualified_name family investigated on 2026-08-17
    // and left unfixed, not a new bug - but note it is NOT one of the gaps TODO.md writes up, so
    // this comment is the whole record of it. Lower both numbers when a fix lands.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "javascript-d3-d3-nice-small-change",
        20,
        14,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-10: minimal 9.293%, full 15.249%
    // The inverse Move asymmetry: 11 of the disagreements are `ours=None, theirs=Move`. Same
    // fixture whose mapping() is clamped just above for the qualified_name family - the mapping
    // pairs the wrong call_expression, so the painting inherits it. Fixing the mapping should
    // move this number too.
    assert_matches_human_painting_within_limit("javascript-d3-d3-nice-small-change", 15.26)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("javascript-d3-d3-nice-small-change")
}
