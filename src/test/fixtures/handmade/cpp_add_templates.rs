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
    // Wrap/reparent: `class_specifier` gets a new `template_declaration` parent. Fixed from 25 to 2
    // mismatches by `resolve_residual_forest_via_myers_lcs`'s trivial-entry filtering
    // (`TRIVIAL_ENTRY_MAX_SIZE`, apted/common.rs) - excluding an unrelated size-1 `;` in the same
    // gap from the count comparison let the real `class_specifier`/`template_declaration` pair
    // recurse through real APTED instead of falling to atomic delete/insert. Exact since:
    // `rescue_wrapped_trivial_entries` finishes the job. That filtered `;` was not unrelated after
    // all - it was wrapped along with the declaration and ends up *inside* the new
    // `template_declaration`, where the substantial pair's own recursion had already emitted it as
    // an `Insert`. Re-pointing that insert to the before-side `;` closes the last mismatch. This is
    // now an exact-match fixture; keep it that way.
    test::helper::human_mapping::assert_matches_human_mapping("cpp-add-templates")
}

#[test]
fn painting() -> Result<()> {
    // After `RenderOptions::paint_displaced_moves` stopped `MINIMAL` painting a span that kept its
    // own text and its own place and shifted only because of an edit before it: minimal 29.615% ->
    // 10.548%. The option is off under `FULL`, which this fix leaves byte-identical at 5.882%, so
    // `MINIMAL` sets the limit now.
    assert_matches_human_painting_within_limit("cpp-add-templates", 10.56)
}

#[test]
fn invariants() -> Result<()> {
    // Invariant 16: the Minimal/Full split for a renamed identifier is not painted this way yet
    // (`IntBox` against `Box`, four times over). Recorded as found.
    assert_ground_truth_invariants_with_known_violations("cpp-add-templates", 6)
}
