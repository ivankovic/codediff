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
    // Recorded distance from the human mapping, not a target: 76 mismatches (54 visible),
    // measured 2026-08-27 when this fixture was added. Lower it when a change earns it; a rise is
    // a regression.
    //
    // Clamped on arrival rather than after a root-cause pass, which is the honest state: nobody
    // has looked at where these 76 come from yet. That is a different thing from the clamps on
    // css-madmaxms and html-chennes, whose comments name the mechanism `--details` attributes
    // them to - so start there if this one is picked up.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "rust-tiffany352-rink-rs-real-change",
        76,
        54,
    )
}
