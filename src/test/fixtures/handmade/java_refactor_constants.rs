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
    test::helper::human_mapping::assert_matches_human_mapping("java-refactor-constants")
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-08-26: minimal 12.963%, full 13.805%
    // re-measured 2026-09-08 after `RenderOptions::paint_displaced_moves` stopped `MINIMAL` painting a
    // span that kept its own text and its own place and shifted only because of an edit before it:
    // minimal 12.963% -> 3.872%. The option is off under `FULL`, which this fix leaves byte-identical
    // at 12.795%, so `FULL` sets the limit now. Any earlier number in this comment that disagrees with
    // these two predates unrelated rendering and ground-truth fixes and was never re-measured.
    assert_matches_human_painting_within_limit("java-refactor-constants", 12.81)
}

#[test]
fn invariants() -> Result<()> {
    // Invariant 9, found when it was added on 2026-09-13: the painting pairs text that the
    // fixture's own tree mapping leaves unmatched, so one record says the code survived and the
    // other says it did not. Recorded rather than repaired - pairing the nodes in the mapping
    // and dropping the painted Move are both one edit, and which of the two records is the
    // author's real reading is the author's to say.
    // Here the literal `3.14159` leaves `return 3.14159 * radius * radius;` on before row 9
    // and arrives in `private static final double PI = 3.14159;` on after row 2. Both
    // paintings read that as one relocated literal; the mapping leaves the old occurrence
    // deleted and the new one inserted. Two sites, counted once per painting.
    assert_ground_truth_invariants_with_known_violations("java-refactor-constants", 4)
}
