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
    // One `-i "..." \` line removed from a long run. The file does not parse as shell, so every
    // `-i` is a byte-identical `word` under one `ERROR`, and only position says which went. The
    // human deletes `word:13` and keeps each `-i` with its string; `APTED("large_flat_subtree")`
    // deletes `word:14`. The same off-by-one as `xml-libreoffice-add-one-menu-item`.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "shellscript-pandas-dev-pandas-remove-one-line",
        2,
        2,
    )
}

#[test]
fn painting() -> Result<()> {
    // A `\`-continued container holds a non-whitespace character in every gap; the
    // `own_content_span` guard in `classify_node` keeps it from being painted whole.
    assert_matches_human_painting_within_limit(
        "shellscript-pandas-dev-pandas-remove-one-line",
        0.16,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("shellscript-pandas-dev-pandas-remove-one-line")
}
