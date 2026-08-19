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

#[test]
fn optimal_solution() -> Result<()> {
    // `.` becoming `?.` (adding a safe-call) isn't recognized as an Update: the two tokens are
    // different named-node kinds under Kotlin's grammar, so APTED deletes the old `.` rather than
    // mapping it to the new `?.`. Affects 3 navigation chains (7 mismatches, since each chain's
    // ancestor `navigation_expression` levels are checked too).
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "kotlin-nextcloud-android-dot-to-question-mark-dot",
        7,
        7,
    )
}
