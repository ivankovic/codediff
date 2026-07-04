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
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */
pub mod code;
pub mod diff;
pub mod metadata;
pub mod stats;
pub mod tui;

pub mod test;

use crate::{
    code::{Code, Language},
    diff::Diff,
    diff::diff_code,
};

/**
* Compute the difference between two programms, given as strings in a given language.
*
* TODO: If the language is Unknown, try to auto-detect it. That should be done in from_string.
* Also, when doing that, check that both strings are in the same language.
*/
pub fn diff_strings(before: &str, after: &str, language: &Language) -> Diff {
    let code_before = Code::from_string(before, language);
    let code_after = Code::from_string(after, language);

    let mut diff = diff_code(&code_before, &code_after);
    diff.language = *language;
    diff
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_empty_strings() {
        let d = diff_strings("", "", &Language::Rust);

        assert!(d.ast.is_some());
    }
}
