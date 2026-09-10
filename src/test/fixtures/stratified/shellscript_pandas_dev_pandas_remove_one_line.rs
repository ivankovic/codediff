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
    // One `-i "..." \` line removed from a long run of them. The file does not parse as shell, so
    // every `-i` hangs flat off one `ERROR`/`command` as a byte-identical `word`, and only
    // position says which one went. The human deletes `word:13`, the `-i` on the removed line,
    // and maps `word:14` onto after `word:13` - keeping each surviving `-i` with the string it
    // introduces. `APTED("large_flat_subtree")` deletes `word:14` instead and leaves `word:13`
    // paired at its own index, so the deletion and one identical pair swap places. Both nodes are
    // visible. The same off-by-one within a run of same-kind siblings as
    // `xml-libreoffice-add-one-menu-item`. Lower both numbers when a fix lands.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "shellscript-pandas-dev-pandas-remove-one-line",
        2,
        2,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-10: minimal 0.066%, full 0.142%
    // Was 35.043% until the `own_content_span` guard in `classify_node` landed the same
    // day: this fixture's container separates its children with `\` line continuations, so
    // every gap held a non-whitespace character and the whole container was painted
    // `Update` for a one-line change. See that guard's doc comment in `diff::text`.
    assert_matches_human_painting_within_limit(
        "shellscript-pandas-dev-pandas-remove-one-line",
        0.16,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("shellscript-pandas-dev-pandas-remove-one-line")
}
