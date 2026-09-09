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
//! This fixture's language has no tree-sitter grammar, so there is no tree to map
//! and no `mapping()` test here. codediff renders the pair with its plain-text
//! fallback diff (`plain_text_line_diff`), and that is what the `painting()` test
//! below is graded against - see `PaintingDiff::PlainText`.

use anyhow::Result;
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants;
use crate::test::helper::human_mapping::assert_matches_human_painting_within_limit;

#[test]
fn painting() -> Result<()> {
    // Not measured yet: 100.0 passes unconditionally. Run this test, read the rate it
    // reports for both modes, and record that instead.
    assert_matches_human_painting_within_limit("bazel-not-actually-supported-by-treesitter", 100.0)
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("bazel-not-actually-supported-by-treesitter")
}
