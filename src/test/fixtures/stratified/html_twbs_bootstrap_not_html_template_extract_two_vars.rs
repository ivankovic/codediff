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

#[test]
fn mapping() -> Result<()> {
    // Not pure HTML, actually a template. This doesn't parse fully. And even if, it requires
    // N:M mapping.
    //
    // The 2 residual mismatches are both the same shape: an `attribute_value` the human calls
    // newly inserted, which codediff pairs with the old one as an `Update` via
    // `StructurallyIdenticalAncestor` - the enclosing `attribute`/`start_tag`/`element` chain is
    // identical on both sides, so that pass matches the values positionally rather than reading
    // them as one value replaced by another. Recorded 2026-09-03 as a measured gap, not accepted
    // as correct.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "html-twbs-bootstrap-not-html-template-extract-two-vars",
        2,
        2,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-05: minimal 24.734%, full 24.127%
    assert_matches_human_painting_within_limit(
        "html-twbs-bootstrap-not-html-template-extract-two-vars",
        24.75,
    )
}
