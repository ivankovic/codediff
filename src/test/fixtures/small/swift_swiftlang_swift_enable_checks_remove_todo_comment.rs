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
    // Nine `// TODO:` lines collapse into two `// RUN:` lines. The human updates the last two and
    // deletes the first; codediff the reverse. Nothing syntactic prefers either.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "swift-swiftlang-swift-enable-checks-remove-todo-comment",
        2,
        2,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("swift-swiftlang-swift-enable-checks-remove-todo-comment")
}
