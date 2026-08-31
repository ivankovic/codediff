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
    // measured 2026-08-26: minimal 28.668%, full 33.634%
    // remeasured 2026-08-31 after ranges_for_options gained extend_leading_whitespace (`Full`
    // now paints a whole inserted line's own leading indentation, per RenderOptions::FULL's own
    // doc comment - see that function): minimal unchanged at 28.668%, full 34.312%. The extra
    // bytes are `return new Promise((resolve) => {`'s own indentation on line 2, which this
    // fixture's ground truth happens to leave unpainted even though the line is a whole new
    // insert - a defensible but not the only reading; not a regression in the rule this option
    // now actually honors.
    assert_matches_human_painting_within_limit("typescript-async-await", 34.32)
}
