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
    // First baseline (2026-09-10), not a regression: this fixture was promoted with its human
    // mapping already written, so the stub's generated 0/0 never reflected a measurement.
    //
    // One `<test/>` element appended to a run of siblings, so the whitespace `CharData` that used
    // to close the list ("\r\n", with `</tests>` at column 0) now separates two elements and
    // carries the new line's two-space indent instead. codediff and the human agree on both the
    // pairing and on after `CharData:17` being the insert - the whole disagreement is the
    // operation on that one pair, which the human reads as `Update` and codediff reports as
    // `Identical`. One node, visible. Lower both numbers when a fix lands.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "xml-microsoft-terminal-add-one-element",
        1,
        1,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-10: minimal 1.320%, full 1.390%
    assert_matches_human_painting_within_limit("xml-microsoft-terminal-add-one-element", 1.40)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("xml-microsoft-terminal-add-one-element")
}
