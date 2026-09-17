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
    // First measurement, 2026-09-17, of a mapping from the 2026-09-16 Defects4J batch. An
    // assignment gains a cast around a method call, and the human treats the new expression as
    // inserted whole; codediff carries the old right-hand side's leaves into it, partly through
    // `qualified_name` and partly as a `MovedSubtree` of the argument list. Recorded as found,
    // not examined.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-defects4j-cli-15-writeablecommandlineimpl",
        37,
        27,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-17: minimal 2.189%, full 3.575% (measured, unexamined)
    assert_matches_human_painting_within_limit(
        "java-defects4j-cli-15-writeablecommandlineimpl",
        3.59,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("java-defects4j-cli-15-writeablecommandlineimpl")
}
