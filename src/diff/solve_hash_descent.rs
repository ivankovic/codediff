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
use crate::code::metadata::metadata_of;
use crate::diff::hash_tree_matching::{self, NodeSelectionConfig};
use crate::diff::{ASTDiff, ASTMappingReason, NodeCache};

/**
* Phase 1 of the seven-phase pipeline (`TODO.md`, 2026-07-17/18): "hash-based, largest-subtree-
* first descent". Runs the generalized `hash_tree_matching::solve_with_hash_map` engine twice,
* once per hash algorithm - each call finds and matches whatever it can, then the next call only
* sees whatever's left unmatched (`PostorderIndexer`/the engine's own `before_node_map` check skip
* anything already claimed):
*
* 1. `KindAndValueHash` - byte-identical subtrees. Extended node selection (reference nodes +
*    big-enough nodes).
* 2. `KindOnlyHash` - same shape, any leaf value. Reference nodes only. Deliberately one coarse
*    tier rather than several intermediate granularities (ignore punctuation only, literals only,
*    identifiers only, ...) - see `TODO.md`'s accepted precision-loss tradeoff.
*
* A third hash variant (normalized import path - import statements matched by normalized path
* rather than syntax) existed here through 2026-08-16; removed outright (not just left disabled)
* once the 2026-07-15 ablation study's finding (net-negative, `-89` when disabled individually)
* had stood permanently off by default for a month with no re-measurement changing that.
*
* Both remaining hashes are order-independent per `nodes::is_commutative_container` at *every*
* recursion level (see `code::hash::compute_kind_and_value_hash`'s doc comment) - order-
* independence is inherent to both hashes, not a bolted-on third one.
*/
pub fn solve(before: &Code, after: &Code, node_cache: &NodeCache, diff: &mut ASTDiff) {
    let selector_config = NodeSelectionConfig::default();
    let before_metadata = metadata_of(before);
    let after_metadata = metadata_of(after);
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
        |metadata| metadata.reference_nodes_ordered.clone(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::Language;
    use crate::diff::{ASTMappingOperation, ASTMappingReason, COST_UPDATE};
    use crate::test::helper::find_first_of_kind;

    /// Reordering a Rust `use_list` (a `nodes::is_commutative_container` kind) with no other
    /// change must be distinguishable from a genuinely untouched one - see the user request this
    /// responds to ("we do need a way to distinguish between truly identical and reordered", then
    /// "make reordering cost more than 0 and change operation to MatchButNotIdentical") and
    /// `ASTMappingReason::FullyMappingSubtrees`'s doc comment.
    #[test]
    fn reordered_commutative_container_is_distinguished_from_truly_identical() {
        let before = Code::from_string("use std::{a, b, c};\nfn f() {}\n", &Language::Rust);
        let after = Code::from_string("use std::{c, a, b};\nfn f() {}\n", &Language::Rust);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve(&before, &after, &node_cache, &mut diff);

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

        // The unrelated, untouched `fn f() {}` must NOT get relabeled - only the actually-
        // reordered node should carry the distinguishing reason.
        let before_fn = find_first_of_kind(before_root, "function_item").unwrap();
        let after_fn = find_first_of_kind(after_root, "function_item").unwrap();
        let fn_mapping = diff.mapping.get(&(before_fn.id(), after_fn.id())).unwrap();
        assert_ne!(
            fn_mapping.reason,
            ASTMappingReason::FullyMappingSubtrees,
            "an untouched, non-reordered node must not be tagged FullyMappingSubtrees"
        );

        // `use_declaration` and `scoped_use_list` wrap `use_list` but aren't commutative
        // containers themselves - a reorder several levels down must still downgrade them from
        // Identical, since neither is a true no-op match either.
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
        // The ancestor itself didn't reorder anything - only the actual commutative container
        // gets tagged FullyMappingSubtrees.
        assert_ne!(
            use_decl_mapping.reason,
            ASTMappingReason::FullyMappingSubtrees
        );
    }
}
