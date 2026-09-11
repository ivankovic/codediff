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
    // measured 2026-09-11: 6 mismatch(es), 4 visible. `new Foo(a, b)` became
    // `absl::make_unique<Foo>(a, b)`. The human maps the old `argument_list` and its parens to
    // nothing - the call is a different call - while APTED pairs them with the new call's
    // `argument_list` on qualified_name, because the arguments inside really are identical. 4 of
    // the 6 are the two paren pairs; this is the flat delimiter-pairing family, not something
    // specific to this fixture.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "cpp-tensorflow-tensorflow-new-to-make-unique",
        6,
        4,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-11: minimal 0.220%, full 0.220%
    assert_matches_human_painting_within_limit("cpp-tensorflow-tensorflow-new-to-make-unique", 0.23)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("cpp-tensorflow-tensorflow-new-to-make-unique")
}
