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
use crate::test::helper::human_mapping::assert_matches_human_painting_within_limit;
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants_with_known_violations;
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    // Wrap/reparent: `class_specifier` gains a `template_declaration` parent. Pins
    // `resolve_residual_forest_via_myers_lcs`'s trivial-entry filtering (`TRIVIAL_ENTRY_MAX_SIZE`)
    // and `rescue_wrapped_trivial_entries`, which re-points the wrapped `;`'s insert to its
    // before-side twin. Keep this exact.
    test::helper::human_mapping::assert_matches_human_mapping("cpp-add-templates")
}

#[test]
fn painting() -> Result<()> {
    // `MINIMAL` sets the limit.
    assert_matches_human_painting_within_limit("cpp-add-templates", 10.56)
}

#[test]
fn invariants() -> Result<()> {
    // Invariant 16: the rename split is not painted (`IntBox` against `Box`, four times).
    // Invariant 18, three more: `int` becomes `T` in `field_declaration.type`,
    // `parameter_declaration.type` and `function_definition.type`. Each parent is matched and each
    // field holds one child, yet the mapping deletes `int` and inserts `T`.
    assert_ground_truth_invariants_with_known_violations("cpp-add-templates", 9)
}
