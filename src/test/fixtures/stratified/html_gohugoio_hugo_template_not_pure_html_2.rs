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
    // Note pure HTML. Template use causes TreeSitter parsing to return suboptimal results.
    test::helper::human_mapping::assert_matches_human_mapping(
        "html-gohugoio-hugo-template-not-pure-html-2",
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-05: minimal 77.670%, full 77.670%
    assert_matches_human_painting_within_limit("html-gohugoio-hugo-template-not-pure-html-2", 77.68)
}
