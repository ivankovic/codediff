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
    // The class body references a symbol from one of the changed imports, so that one real
    // change propagates MatchButNotIdentical up through every ancestor level (export_statement,
    // class_declaration, class_body, public_field_definition, ...) even though the class's own
    // content is otherwise unchanged - standard classification-bubbling from a single leaf
    // change, not scattered independent issues.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "typescript-n8n-io-n8n-remove-and-add-imports",
        42,
    )
}
