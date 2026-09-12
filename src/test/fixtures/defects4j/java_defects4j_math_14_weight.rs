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
    // First measurement, 2026-09-12. A `method_invocation` is replaced by an
    // `object_creation_expression`: the human deletes the call whole and inserts the
    // constructor, while codediff re-uses the deleted call's leaves - its parentheses and its
    // identifier, one of them against a `type_identifier` - inside the new expression. The same
    // scaffolding-reuse family as the jxpath `CoreOperation*` fixtures, counted once per
    // re-used leaf.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-defects4j-math-14-weight",
        10,
        8,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-12: minimal 0.662%, full 0.662% (measured, unexamined)
    assert_matches_human_painting_within_limit("java-defects4j-math-14-weight", 0.68)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("java-defects4j-math-14-weight")
}
