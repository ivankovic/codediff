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

use crate::test::helper::human_mapping::assert_matches_human_painting_within_limit;

#[test]
fn painting_agreement() -> Result<()> {
    // measured 2026-08-27: minimal 0.901% (32 of 3550 bytes), full 0.000% (0 bytes)
    //
    // The opposite asymmetry to the two hello-world fixtures: here Full is exact and Minimal is
    // not, so Minimal's filtering is dropping something the human kept painted. Worth a look if
    // Minimal's rules are revisited - it is a small, cheap counterexample to "Minimal is always
    // the easier target".
    assert_matches_human_painting_within_limit("rust-tauri-api-build-2", 0.91)
}
