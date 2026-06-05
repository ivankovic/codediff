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
use crate::code::Code;
use crate::diff::optimal_iud;
use crate::diff::{ASTDiff, NodeCache};

/**
* Match semantically structural nodes and solve their subtrees.
*/
pub fn solve(before: &Code, after: &Code, node_cache: &NodeCache, diff: &mut ASTDiff) {
    let before_metadata = match &before.metadata.ast_metadata {
        Some(m) => m,
        None => return,
    };
    let after_metadata = match &after.metadata.ast_metadata {
        Some(m) => m,
        None => return,
    };

    for ((kind, identifier), &before_node_id) in &before_metadata.semantically_structural_nodes {
        if let Some(&after_node_id) = after_metadata
            .semantically_structural_nodes
            .get(&(kind.clone(), identifier.clone()))
        {
            // TODO: Consider checking the cost and reject the solution if it is too
            // expensive?
            let _ = optimal_iud::for_nodes(
                before,
                after,
                before_metadata,
                after_metadata,
                vec![before_node_id],
                vec![after_node_id],
                node_cache,
                diff,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use anyhow::Result;

    use crate::diff::ASTDiff;
    use crate::test;

    #[test]
    fn rust_hash_optimization() -> Result<()> {
        let test_diffs = test::helper::handmade_test_code_pairs()?;
        let (before, after) = test_diffs.get("rust-hash-optimization").unwrap().clone();

        // Null mappings should be considered valid
        let node_cache = NodeCache::build(&before, &after);

        let mut diff = ASTDiff {
            ..Default::default()
        };

        solve(&before, &after, &node_cache, &mut diff);

        Ok(())
    }
}
