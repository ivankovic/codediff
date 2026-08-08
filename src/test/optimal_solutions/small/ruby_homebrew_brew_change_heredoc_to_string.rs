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
    // A heredoc (`<<~EOS ... EOS`) becomes a plain string literal - tree-sitter-ruby represents
    // the two very differently, so the enclosing `call`/`argument_list` can't be matched across
    // the representation change; codediff deletes rather than transforms them.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "ruby-homebrew-brew-change-heredoc-to-string",
        2,
    )
}
