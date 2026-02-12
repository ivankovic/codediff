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
pub mod metadata;
pub mod stats;

#[derive(Debug, Default, Clone)]
pub struct TwoDiff {
    pub unix_diff_format: String,
    pub gumtree_diff_format: String,
}

pub fn diff(_before: &str, _after: &str) -> TwoDiff {
    TwoDiff::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_diff() {
        let d = diff("", "");

        assert_eq!(d.unix_diff_format, "");
    }
}
