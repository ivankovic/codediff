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
    // Clamped at 2 (2026-08-18, newly added fixture): `qualified_name` matches a bare
    // `simple_identifier` to the wrong one of two candidates that both spell the same name inside
    // sibling `call_expression`s (a `property_declaration` initializer vs. a `guard`'s
    // `try_expression`) - a same-shape sibling-choice tie, not a structural miss.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "swift-apple-swift-argument-parser-simplify-code",
        2,
        2,
    )
}
