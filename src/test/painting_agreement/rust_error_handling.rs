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
    // measured 2026-08-27: minimal 6.160% (43 of 698 bytes), full 5.014% (35 of 698)
    //
    // Byte counts, not just percentages, because the clamp is a 2dp ceiling of the true
    // fraction and 43/698 is 6.16046% - a first pass recorded 6.16 from the displayed 6.160%
    // and failed on the very run that set it.
    assert_matches_human_painting_within_limit("rust-error-handling", 6.17)
}
