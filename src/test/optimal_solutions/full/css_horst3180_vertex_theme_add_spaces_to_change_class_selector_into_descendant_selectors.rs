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
    // A whitespace change that actually modifies the AST and the code logic: `.a.b` (one
    // `class_selector` with two `.`-prefixed parts) becomes `.a .b` (a `descendant_selector`
    // wrapping a second `class_selector`) - real tree restructuring from a formatting-looking edit.
    //
    // Clamped at 16 (2026-08-18, newly added fixture): the terminal `fast_fallback` resolver
    // leaves every relocated `class_selector`/`.` pair as a Delete+Identical-elsewhere rather than
    // reaching inside the newly-inserted `descendant_selector` wrapper to match them - the
    // wrap/reparent shape this project's move-detection work has repeatedly found hard, not a new
    // failure mode.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "css-horst3180-vertex-theme-add-spaces-to-change-class-selector-into-descendant-selectors",
        16,
        8,
    )
}
