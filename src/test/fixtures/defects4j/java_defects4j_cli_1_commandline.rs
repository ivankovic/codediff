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
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants_with_known_violations;

#[test]
fn mapping() -> Result<()> {
    // First measurement, 2026-09-17, of a mapping from the 2026-09-16 Defects4J batch. The human
    // deletes whole import declarations and a method and inserts their replacements, while
    // codediff's `qualified_name` pass re-uses the deleted leaves - the identifiers, the dots,
    // the semicolons - inside the inserted ones. One disagreement about which container
    // survives, counted once per re-used leaf; 32 of the residuals name that pass. The largest
    // clamp in this batch, recorded as found, not examined line by line.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-defects4j-cli-1-commandline",
        106,
        72,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-17: minimal 3.079%, full 3.816% (measured, unexamined)
    assert_matches_human_painting_within_limit("java-defects4j-cli-1-commandline", 3.83)
}

#[test]
fn invariants() -> Result<()> {
    // Measured at ten on 2026-09-17, eight once invariants 3 and 9 learned to read a multi-map
    // group: two of invariant 3 and one byte of each `Full` painting's invariant 9 were
    // `representative_entries`' arbitrary flattening of the 1:2 `(` and `)` groups on after row
    // 67, not anything the mapping claims.
    //
    // The eight left are real and uniform - every one of the four paintings paints 3 bytes Move
    // on before row 93 and 3 on after row 91 that the tree mapping reads Delete/Insert. Those two
    // ground truths have to be reconciled by hand.
    assert_ground_truth_invariants_with_known_violations("java-defects4j-cli-1-commandline", 8)
}
