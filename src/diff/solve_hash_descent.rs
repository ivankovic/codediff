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
use crate::diff::PassCtx;
use crate::diff::hash_tree_matching::{self, NodeSelectionConfig};
use crate::diff::nodes::is_import_kind;
use crate::diff::{ASTDiff, ASTMappingReason};

/// Phase 1: largest-subtree-first hash descent, run twice; the second call sees only what the
/// first left unmatched.
///
/// 1. `KindAndValueHash`: byte-identical subtrees, over reference nodes plus big-enough nodes.
/// 2. `KindOnlyHash`: same shape, any leaf value, over reference nodes only. One coarse tier on
///    purpose, rather than several intermediate normalisations.
///
/// Both hashes are order-independent for `nodes::is_commutative_container` kinds at every level.
pub fn solve(ctx: &PassCtx, diff: &mut ASTDiff) {
    let (before, after, node_cache) = (ctx.before, ctx.after, ctx.node_cache);
    let selector_config = NodeSelectionConfig::default();
    let before_metadata = ctx.before_metadata();
    let after_metadata = ctx.after_metadata();
    hash_tree_matching::solve_with_hash_map(
        before,
        after,
        node_cache,
        diff,
        &before_metadata.node_to_kind_and_value_hash,
        &after_metadata.kind_and_value_hash_to_node,
        ASTMappingReason::IdenticalHash,
        ASTMappingReason::IdenticalHashOfAncestor,
        selector_config.to_node_list_selector(),
    );

    hash_tree_matching::solve_with_hash_map(
        before,
        after,
        node_cache,
        diff,
        &before_metadata.node_to_kind_only_hash,
        &after_metadata.kind_only_hash_to_node,
        ASTMappingReason::StructurallyIdenticalSubtrees,
        ASTMappingReason::StructurallyIdenticalAncestor,
        // Shape alone cannot tell two imports apart; see `nodes::is_import_kind`.
        |metadata| {
            metadata
                .reference_nodes_ordered
                .iter()
                .copied()
                .filter(|id| {
                    metadata
                        .node_info
                        .get(id)
                        .is_none_or(|info| !is_import_kind(&info.kind, &metadata.language))
                })
                .collect()
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::Code;
    use crate::code::Language;
    use crate::diff::NodeCache;
    use crate::diff::{ASTMappingOperation, ASTMappingReason, COST_UPDATE};
    use crate::test::helper::find_first_of_kind;

    /// A reordered commutative container is matched, but not as a no-op.
    #[test]
    fn reordered_commutative_container_is_distinguished_from_truly_identical() {
        let before = Code::from_string("use std::{a, b, c};\nfn f() {}\n", &Language::Rust);
        let after = Code::from_string("use std::{c, a, b};\nfn f() {}\n", &Language::Rust);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );

        let before_root = before.ast.as_ref().unwrap().root_node();
        let after_root = after.ast.as_ref().unwrap().root_node();
        let before_use_list = find_first_of_kind(before_root, "use_list").unwrap();
        let after_use_list = find_first_of_kind(after_root, "use_list").unwrap();

        let mapping = diff
            .mapping
            .get(&(before_use_list.id(), after_use_list.id()))
            .expect("reordered use_list should still be matched");
        assert_eq!(
            mapping.reason,
            ASTMappingReason::FullyMappingSubtrees,
            "a reordered-but-unchanged use_list must be tagged FullyMappingSubtrees, not plain IdenticalHash"
        );
        assert_eq!(
            mapping.operation,
            ASTMappingOperation::MatchButNotIdentical,
            "a pure reorder is not a no-op - operation must be MatchButNotIdentical, not Identical"
        );
        assert_eq!(
            mapping.cost, COST_UPDATE,
            "a pure reorder must cost more than 0"
        );

        let before_fn = find_first_of_kind(before_root, "function_item").unwrap();
        let after_fn = find_first_of_kind(after_root, "function_item").unwrap();
        let fn_mapping = diff.mapping.get(&(before_fn.id(), after_fn.id())).unwrap();
        assert_ne!(
            fn_mapping.reason,
            ASTMappingReason::FullyMappingSubtrees,
            "an untouched, non-reordered node must not be tagged FullyMappingSubtrees"
        );

        // Ancestors of the reordered container are not no-ops either.
        let before_use_decl = find_first_of_kind(before_root, "use_declaration").unwrap();
        let after_use_decl = find_first_of_kind(after_root, "use_declaration").unwrap();
        let use_decl_mapping = diff
            .mapping
            .get(&(before_use_decl.id(), after_use_decl.id()))
            .unwrap();
        assert_eq!(
            use_decl_mapping.operation,
            ASTMappingOperation::MatchButNotIdentical,
            "an ancestor of a reordered container must also be downgraded from Identical"
        );
        assert_eq!(use_decl_mapping.cost, COST_UPDATE);
        assert_ne!(
            use_decl_mapping.reason,
            ASTMappingReason::FullyMappingSubtrees
        );
    }
}
