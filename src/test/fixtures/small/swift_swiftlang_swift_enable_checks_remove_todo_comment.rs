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
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    // A 9-line block of `// TODO:` comments collapses to 2 `// RUN:` comments. The human mapping
    // deletes the block's first two comment lines and updates its last two into the new RUN
    // lines; codediff picks the other pairing (update the first two, delete the last two) -
    // both are equally valid many-old-comments-into-few-new-ones correspondences with no
    // syntactic signal to prefer one over the other.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "swift-swiftlang-swift-enable-checks-remove-todo-comment",
        2,
        2,
    )
}
