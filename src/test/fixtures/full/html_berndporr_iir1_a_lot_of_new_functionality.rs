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
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants;
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
    // The worst rate in the corpus: the licence block between @license tags parses as one
    // `raw_text` node, so the tree says much less than the text. Mostly `APTED("fast_fallback")`
    // on large subtrees, and tag scaffolding hash-matched across the human's insert/delete
    // boundary.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "html-berndporr-iir1-a-lot-of-new-functionality",
        734,
        510,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("html-berndporr-iir1-a-lot-of-new-functionality")
}
