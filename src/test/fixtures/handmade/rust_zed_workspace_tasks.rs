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
    // An `if let Some(x) = self.foo() { ... }` is rewritten to a plain `if` with an internal
    // `let_declaration` (the `let_condition` pattern is flattened out of the condition and into
    // the block). The human ground truth wants the shared inner content matched across that
    // restructure, but codediff (reason `APTED("qualified_name")`, formerly `"syntax_named"` -
    // renamed 2026-08-14 - throughout) treats it as a wholesale delete-and-reinsert of the changed
    // region instead. Same "container/structure
    // changed, content mostly persists" wall family as `java_add_exception_handling` and
    // `kotlin_refactor_function` - not attempted here.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "rust-zed-workspace-tasks",
        117,
        81,
    )
}
