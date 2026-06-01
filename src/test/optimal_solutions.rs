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
#[cfg(test)]
mod tests {
    use crate::diff;
    use crate::test;

    use anyhow::Result;

    #[test]
    fn rust_hash_optimization() -> Result<()> {
        let test_diffs = test::helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs.get("rust-hash-optimization").unwrap().clone();

        let diff = diff::diff_code(&before, &after);

        assert!(diff.ast.is_some());
        let diff_ast = diff.ast.unwrap();

        Ok(())
    }
}
