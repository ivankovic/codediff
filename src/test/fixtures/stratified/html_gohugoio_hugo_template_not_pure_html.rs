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
    // Not pure HTML. Contains templating characters that break TreeSitter HTML parsing. Because
    // of this, requires N:M mapping for the AST, but wouldn't if it parsed correctly.
    test::helper::human_mapping::assert_matches_human_mapping(
        "html-gohugoio-hugo-template-not-pure-html",
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-05: minimal 16.798%, full 22.572%
    assert_matches_human_painting_within_limit("html-gohugoio-hugo-template-not-pure-html", 22.59)
}
