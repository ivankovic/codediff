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
    test::helper::human_mapping::assert_matches_human_mapping("java-defects4j-mockito-22-equality")
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-12: minimal 0.039%, full 0.000% (measured, unexamined)
    assert_matches_human_painting_within_limit("java-defects4j-mockito-22-equality", 0.05)
}

#[test]
fn invariants() -> Result<()> {
    // One invariant-6 violation, recorded as found on 2026-09-12. The `Minimal` painting claims
    // the single leading tab of after row 15 (`} else if (o1 == null || o2 == null) {`), and
    // `Minimal` never claims a line's indentation. A painting-side slip of one byte: the row's
    // code is painted correctly either way. `human_solver` now keeps this rule at the keystroke
    // for a multi-row full-line sweep, but this range was painted before that or by a vertical
    // selection, which names its own columns and is left as drawn.
    assert_ground_truth_invariants_with_known_violations("java-defects4j-mockito-22-equality", 1)
}
