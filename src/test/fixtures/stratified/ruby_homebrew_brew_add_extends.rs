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
    test::helper::human_mapping::assert_matches_human_mapping("ruby-homebrew-brew-add-extends")
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-10: minimal 0.061%, full 0.000%
    assert_matches_human_painting_within_limit("ruby-homebrew-brew-add-extends", 0.08)
}

#[test]
fn invariants() -> Result<()> {
    // measured 2026-09-10: 1 violation, and a repairable annotation defect rather than a fact
    // about the fixture. The whole change is one added line, `        extend AutoCorrector`, and
    // the `Minimal` painting claims its eight columns of leading indentation - which is exactly
    // what `Minimal` is defined not to do (see RULES_AND_PREFERENCES.md's "Indentation": option 2
    // paints the visible characters and the whitespace *between* them, option 4 paints the
    // leading whitespace too, and `Minimal` is the former). Re-painting that one run in
    // human_solver to start at `extend` should take this back to 0; it needs the ground truth
    // edited, not codediff.
    assert_ground_truth_invariants_with_known_violations("ruby-homebrew-brew-add-extends", 1)
}
