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

#[test]
fn optimal_solution() -> Result<()> {
    // 2026-08-06: dropped 31 -> 11 via `apted::prematch_unique_named_locals` - `TaskView`'s
    // `capability`/`showTaskActions` parameters keep their names but shift position when a new
    // `viewModel` parameter is inserted before them; now pre-matched by parameter name (see
    // `TODO.md`'s "shift-due-to-insertion" entry). The remaining 11 are a different, already-
    // investigated-and-declined-to-fix pattern: `viewModel` (a pure insert) and
    // `showTranslateScreen` (a pure delete, unrelated to it) get cross-matched by real APTED's
    // same-kind-internal-node cost preference, even though they share no name - the "near-
    // duplicate but distinct reuse-vs-replace" gap (see `kotlin-remove-function`/`rust-algorithm-
    // change`), tried and reverted twice already (`TODO.md`, container-dissimilarity-surcharge
    // and leaf-rename-graduation) - not re-attempted here.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "kotlin-nextcloud-change-function-fingerprint",
        11,
    )
}
