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

use tree_sitter::ffi::{TSPoint, TSRange};

use crate::diff::{ASTDiff, ASTMappingOperation};

/*
* Holds the API and necessary data to query an AST based diff using TreeSitter Point structures as
* reference points. TreeSitter Points are row-column based points in the textual representation of
* the code. When the diff is displayed to humans in textual form, this structure helps keep the
* translation logic from the tree based diff, which has no inherent visual form, to the two
* dimensions of text.
*
* This is very useful in text editors or TUI apps.
*/
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TSPointDiff {}

impl TSPointDiff {
    /// Construct the TSPointDiff from an ASTDiff.
    ///
    /// An ASTDiff must exist to create the TSPointDiff. There is no algorithm currently
    /// implemented that can construct the TSPointDiff directly from code.
    pub fn from(diff: ASTDiff) -> Self {}

    /// For a given TSPoint, return the diff information.
    ///
    /// Returns the operation, the range this point belongs to and the range of the nearest
    /// ancestor reference node.
    pub fn for_point(point: TSPoint) -> (ASTMappingOperation, TSRange, TSRange) {}
}

#[cfg(test)]
mod tests {
    use crate::{
        code::{Code, Language},
        test,
    };
    use anyhow::Result;

    use super::*;

    #[test]
    fn python_leetcode_1_added_if_block() -> Result<()> {
        let test_diffs = test::helper::handmade_test_diffs()?;
        let (before, after) = test_diffs.get("python-added-if-block").unwrap().clone();

        Ok(())
    }
