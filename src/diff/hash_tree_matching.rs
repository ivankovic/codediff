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
use std::collections::HashMap;

use crate::code::metadata::metadata_of;
use crate::code::{ASTMetadata, Code};
use crate::diff::nodes::is_reference;
use crate::diff::{ASTDiff, ASTMapping, ASTMappingOperation, ASTMappingReason, NodeCache};

/// Which nodes are hash-matching candidates: reference nodes, plus any node meeting both
/// thresholds.
#[derive(Debug, Clone)]
pub struct NodeSelectionConfig {
    pub min_depth: usize,
    pub min_subtree_size: usize,
}

impl Default for NodeSelectionConfig {
    fn default() -> Self {
        Self {
            // Tuned against the benchmark corpus.
            min_depth: 0,
            min_subtree_size: 45,
        }
    }
}

impl NodeSelectionConfig {
    /// [`build_extended_node_list`] as a selector for `solve_with_hash_map`.
    pub fn to_node_list_selector(&self) -> impl Fn(&ASTMetadata) -> Vec<usize> + '_ {
        move |metadata: &ASTMetadata| build_extended_node_list(metadata, self)
    }
}

/// The candidates `config` selects, largest subtree first, ties by start byte.
pub fn build_extended_node_list(
    metadata: &ASTMetadata,
    config: &NodeSelectionConfig,
) -> Vec<usize> {
    let language = metadata.language;
    let mut nodes_with_info: Vec<(usize, usize, usize)> = Vec::new(); // (id, size, start_byte)

    for (&node_id, info) in &metadata.node_info {
        let subtree_size = metadata
            .node_to_subtree_size
            .get(&node_id)
            .copied()
            .unwrap_or(0);
        let depth = metadata.node_to_depth.get(&node_id).copied().unwrap_or(0);
        let start_byte = info.start_byte;

        // A keyword token can share its statement's kind string (Kotlin `import`); unnamed, it
        // would pair with an arbitrary same keyword elsewhere.
        let is_reference_node = info.is_named && is_reference(&info.kind, &language);
        let is_big_enough = depth >= config.min_depth && subtree_size >= config.min_subtree_size;

        if is_reference_node || is_big_enough {
            nodes_with_info.push((node_id, subtree_size, start_byte));
        }
    }

    // Ties by `start_byte`; see `ASTNodeMetadata::start_byte`.
    nodes_with_info.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)));
    nodes_with_info
        .into_iter()
        .map(|(node_id, _, _)| node_id)
        .collect()
}

/// Matches each selected, still-unmatched before node to the nearest unmatched after node with the
/// same value in the caller's hash, then maps both subtrees in lockstep. The caller picks the hash;
/// whether a pair is identical is always decided by `node_to_kind_and_value_hash`.
// Every parameter is distinct context; a params struct would only relocate them.
#[allow(clippy::too_many_arguments)]
pub(crate) fn solve_with_hash_map(
    before: &Code,
    after: &Code,
    node_cache: &NodeCache,
    diff: &mut ASTDiff,
    before_hash: &rustc_hash::FxHashMap<usize, u64>,
    after_hash_to_nodes: &rustc_hash::FxHashMap<u64, Vec<usize>>,
    root_reason: ASTMappingReason,
    descendant_reason: ASTMappingReason,
    node_list_selector: impl Fn(&ASTMetadata) -> Vec<usize>,
) {
    let before_metadata = metadata_of(before);
    let after_metadata = metadata_of(after);

    let classify = |before_id: usize, after_id: usize| -> (ASTMappingOperation, u64) {
        let before_kv = before_metadata.node_to_kind_and_value_hash.get(&before_id);
        let after_kv = after_metadata.node_to_kind_and_value_hash.get(&after_id);
        if before_kv.is_some() && before_kv == after_kv {
            return (ASTMappingOperation::Identical, 0);
        }
        // As in `apted::common::classify_match`, `Update` is a leaf's own value. An interior node
        // differs only through descendants that carry their own cost; labelling it `Update` would
        // turn one renamed leaf into a mismatch at every ancestor.
        if before_metadata.is_leaf(before_id) && after_metadata.is_leaf(after_id) {
            (ASTMappingOperation::Update, crate::diff::COST_UPDATE)
        } else if before_metadata
            .node_info
            .get(&before_id)
            .map(|info| info.owned_text_hash)
            != after_metadata
                .node_info
                .get(&after_id)
                .map(|info| info.owned_text_hash)
        {
            // Its own gap text differs, which no descendant entry carries (see `operation_cost`).
            (
                ASTMappingOperation::MatchButNotIdentical,
                crate::diff::COST_UPDATE,
            )
        } else {
            (ASTMappingOperation::MatchButNotIdentical, 0)
        }
    };

    let before_node_ids = node_list_selector(&before_metadata);

    for &before_node_id in &before_node_ids {
        if diff.before_node_map.contains_key(&before_node_id) {
            continue;
        }
        let Some(&before_node) = node_cache.before.get(&before_node_id) else {
            continue;
        };
        let Some(before_hash_value) = before_hash.get(&before_node_id) else {
            continue;
        };
        let Some(after_candidates) = after_hash_to_nodes.get(before_hash_value) else {
            continue;
        };

        // Proximity in the file, not discovery order, tells true duplicates from coincidences.
        let Some(&after_node_id) = after_candidates
            .iter()
            .filter(|&&id| !diff.after_node_map.contains_key(&id))
            .min_by_key(|&&id| {
                node_cache
                    .after
                    .get(&id)
                    .map(|n| n.start_byte().abs_diff(before_node.start_byte()))
                    .unwrap_or(usize::MAX)
            })
        else {
            continue;
        };
        let Some(&after_node) = node_cache.after.get(&after_node_id) else {
            continue;
        };

        let (operation, cost) = classify(before_node_id, after_node_id);
        diff.add_mapping(
            before_node_id,
            after_node_id,
            ASTMapping {
                cost,
                operation,
                reason: root_reason,
            },
        );

        // Nodes whose own children were reordered; their ancestors are downgraded afterwards.
        let mut reordered_ids: Vec<usize> = Vec::new();
        let mut stack = vec![(before_node, after_node)];
        while let Some((before_parent, after_parent)) = stack.pop() {
            let (pairs, reordered) = pair_children_for_descent(
                before_parent,
                after_parent,
                &before_metadata,
                &after_metadata,
            );

            // The parent's mapping is already recorded, so patch it. A pure reorder is not a no-op:
            // the ground truth records it as `MatchButNotIdentical` at `COST_UPDATE`.
            if reordered {
                if let Some(mapping) = diff
                    .mapping
                    .get_mut(&(before_parent.id(), after_parent.id()))
                {
                    mapping.reason = ASTMappingReason::FullyMappingSubtrees;
                    mapping.operation = ASTMappingOperation::MatchButNotIdentical;
                    mapping.cost = crate::diff::COST_UPDATE;
                }
                reordered_ids.push(before_parent.id());
            }

            for (before_child, after_child) in pairs {
                if diff.before_node_map.contains_key(&before_child.id()) {
                    continue;
                }
                let (operation, cost) = classify(before_child.id(), after_child.id());
                diff.add_mapping(
                    before_child.id(),
                    after_child.id(),
                    ASTMapping {
                        cost,
                        operation,
                        reason: descendant_reason,
                    },
                );
                stack.push((before_child, after_child));
            }
        }

        // A container is not a no-op if anything inside it was reordered, up to this match's root.
        // The ancestors keep their reason: only the reordered container itself reordered.
        for reordered_id in reordered_ids {
            let mut cur = reordered_id;
            while let Some(&parent_id) = before_metadata.node_to_parent.get(&cur) {
                let Some(&after_parent_id) = diff.before_node_map.get(&parent_id) else {
                    break;
                };
                if let Some(mapping) = diff.mapping.get_mut(&(parent_id, after_parent_id))
                    && mapping.operation == ASTMappingOperation::Identical
                {
                    mapping.operation = ASTMappingOperation::MatchButNotIdentical;
                    mapping.cost = crate::diff::COST_UPDATE;
                }
                if parent_id == before_node_id {
                    break;
                }
                cur = parent_id;
            }
        }
    }
}

/// Pairs two matched parents' children for the lockstep descent, and reports whether a
/// commutative container's children were reordered.
///
/// Ordinary parents pair positionally, dropping kind mismatches: equal hashes computed in document
/// order imply positional correspondence. A `nodes::is_commutative_container` hashes its children
/// unordered, so its children pair by hash: kind-and-value first, then kind-only for what is left
/// (a kind-only hash outer match allows values to differ). Kind-only alone would pair differently
/// named identifiers arbitrarily and hide a real reorder.
///
/// Ties go to the nearest sibling index, not byte offset: an edit before the parents shifts every
/// offset inside them, and a shifted neighbouring comma can be closer than the right one.
pub(crate) fn pair_children_for_descent<'a>(
    before_parent: tree_sitter::Node<'a>,
    after_parent: tree_sitter::Node<'a>,
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
) -> (Vec<(tree_sitter::Node<'a>, tree_sitter::Node<'a>)>, bool) {
    let mut before_cursor = before_parent.walk();
    let mut after_cursor = after_parent.walk();
    let before_children: Vec<_> = before_parent.children(&mut before_cursor).collect();
    let after_children: Vec<_> = after_parent.children(&mut after_cursor).collect();

    let language = before_metadata.language;
    if !crate::diff::nodes::is_commutative_container(before_parent.kind(), &language) {
        let pairs = before_children
            .into_iter()
            .zip(after_children)
            .filter(|(b, a)| b.kind() == a.kind())
            .collect();
        return (pairs, false);
    }

    let index_by_hash = |children: &[tree_sitter::Node<'a>],
                         hash_map: &rustc_hash::FxHashMap<usize, u64>| {
        let mut by_hash: HashMap<u64, Vec<(usize, tree_sitter::Node<'a>)>> = HashMap::new();
        for (index, &child) in children.iter().enumerate() {
            let hash = hash_map.get(&child.id()).copied().unwrap_or(0);
            by_hash.entry(hash).or_default().push((index, child));
        }
        by_hash
    };
    let after_by_kv = index_by_hash(&after_children, &after_metadata.node_to_kind_and_value_hash);
    let after_by_ko = index_by_hash(&after_children, &after_metadata.node_to_kind_only_hash);

    let mut used = std::collections::HashSet::new();
    let mut pairs = Vec::new();
    let mut reordered = false;
    let mut unmatched_before: Vec<(usize, tree_sitter::Node<'a>)> = Vec::new();

    for (before_index, before_child) in before_children.into_iter().enumerate() {
        let kv_hash = before_metadata
            .node_to_kind_and_value_hash
            .get(&before_child.id())
            .copied()
            .unwrap_or(0);
        let found = after_by_kv
            .get(&kv_hash)
            .and_then(|candidates| {
                candidates
                    .iter()
                    .filter(|(_, c)| !used.contains(&c.id()) && c.kind() == before_child.kind())
                    .min_by_key(|(after_index, _)| after_index.abs_diff(before_index))
            })
            .copied();
        match found {
            Some((after_index, best)) => {
                used.insert(best.id());
                if after_index != before_index {
                    reordered = true;
                }
                pairs.push((before_child, best));
            }
            None => unmatched_before.push((before_index, before_child)),
        }
    }

    // Kind-only fallback for what the exact tier left.
    for (before_index, before_child) in unmatched_before {
        let ko_hash = before_metadata
            .node_to_kind_only_hash
            .get(&before_child.id())
            .copied()
            .unwrap_or(0);
        let Some(candidates) = after_by_ko.get(&ko_hash) else {
            continue;
        };
        let Some(&(after_index, best)) = candidates
            .iter()
            .filter(|(_, c)| !used.contains(&c.id()) && c.kind() == before_child.kind())
            .min_by_key(|(after_index, _)| after_index.abs_diff(before_index))
        else {
            continue;
        };
        used.insert(best.id());
        if after_index != before_index {
            reordered = true;
        }
        pairs.push((before_child, best));
    }

    (pairs, reordered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::Language;
    use crate::test::helper::find_first_of_kind;

    #[test]
    fn build_extended_node_list_always_includes_reference_nodes_regardless_of_size() {
        let code = Code::from_string("fn f() {}\n", &Language::Rust);
        let metadata = metadata_of(&code);
        let config = NodeSelectionConfig {
            min_depth: 0,
            min_subtree_size: usize::MAX,
        };
        let list = build_extended_node_list(&metadata, &config);

        let root = code.ast.as_ref().unwrap().root_node();
        let function_item = find_first_of_kind(root, "function_item").unwrap();
        assert!(
            list.contains(&function_item.id()),
            "a reference node must be included even though it's far smaller than min_subtree_size"
        );
    }

    #[test]
    fn build_extended_node_list_excludes_small_non_reference_nodes() {
        let code = Code::from_string("fn f() { let x = 1; }\n", &Language::Rust);
        let metadata = metadata_of(&code);
        let config = NodeSelectionConfig {
            min_depth: 0,
            min_subtree_size: usize::MAX,
        };
        let list = build_extended_node_list(&metadata, &config);

        let root = code.ast.as_ref().unwrap().root_node();
        let let_decl = find_first_of_kind(root, "let_declaration").unwrap();
        assert!(
            !list.contains(&let_decl.id()),
            "a small, non-reference node must be excluded when it can't meet the size threshold"
        );
    }

    #[test]
    fn build_extended_node_list_includes_non_reference_nodes_above_the_size_threshold() {
        let code = Code::from_string("fn f() { let x = 1; }\n", &Language::Rust);
        let metadata = metadata_of(&code);
        let config = NodeSelectionConfig {
            min_depth: 0,
            min_subtree_size: 1,
        };
        let list = build_extended_node_list(&metadata, &config);

        let root = code.ast.as_ref().unwrap().root_node();
        let let_decl = find_first_of_kind(root, "let_declaration").unwrap();
        assert!(
            list.contains(&let_decl.id()),
            "a low enough min_subtree_size must let ordinary, non-reference nodes in too"
        );
    }

    #[test]
    fn build_extended_node_list_sorts_by_subtree_size_descending() {
        let code = Code::from_string("fn f() { let x = 1; }\nfn g() {}\n", &Language::Rust);
        let metadata = metadata_of(&code);
        let config = NodeSelectionConfig {
            min_depth: 0,
            min_subtree_size: usize::MAX,
        };
        let list = build_extended_node_list(&metadata, &config);

        let sizes: Vec<usize> = list
            .iter()
            .map(|id| metadata.node_to_subtree_size.get(id).copied().unwrap_or(0))
            .collect();
        let mut sorted_desc = sizes.clone();
        sorted_desc.sort_by(|a, b| b.cmp(a));
        assert_eq!(
            sizes, sorted_desc,
            "list must be sorted by subtree size descending"
        );
    }

    #[test]
    fn pair_children_for_descent_zips_positionally_and_drops_mismatched_kinds() {
        let before = Code::from_string("fn f() { let x = 1; let y = 2; }\n", &Language::Rust);
        let after = Code::from_string("fn f() { let x = 1; return; }\n", &Language::Rust);
        let before_metadata = metadata_of(&before);
        let after_metadata = metadata_of(&after);

        let before_root = before.ast.as_ref().unwrap().root_node();
        let after_root = after.ast.as_ref().unwrap().root_node();
        let before_block = find_first_of_kind(before_root, "block").unwrap();
        let after_block = find_first_of_kind(after_root, "block").unwrap();

        let (pairs, reordered) =
            pair_children_for_descent(before_block, after_block, &before_metadata, &after_metadata);

        assert!(
            !reordered,
            "a block is not a commutative container - reordered must always be false"
        );
        assert!(
            pairs.iter().all(|(b, a)| b.kind() == a.kind()),
            "every returned pair must share a kind - the positional zip filters out kind mismatches: {:?}",
            pairs
                .iter()
                .map(|(b, a)| (b.kind(), a.kind()))
                .collect::<Vec<_>>()
        );
        assert!(
            pairs
                .iter()
                .any(|(b, a)| b.kind() == "let_declaration" && a.kind() == "let_declaration"),
            "the first, still-matching let_declaration must still be paired"
        );
    }

    #[test]
    fn pair_children_for_descent_detects_reordering_in_a_commutative_container() {
        let before = Code::from_string("use std::{a, b, c};\n", &Language::Rust);
        let after = Code::from_string("use std::{c, a, b};\n", &Language::Rust);
        let before_metadata = metadata_of(&before);
        let after_metadata = metadata_of(&after);

        let before_root = before.ast.as_ref().unwrap().root_node();
        let after_root = after.ast.as_ref().unwrap().root_node();
        let before_use_list = find_first_of_kind(before_root, "use_list").unwrap();
        let after_use_list = find_first_of_kind(after_root, "use_list").unwrap();

        let (pairs, reordered) = pair_children_for_descent(
            before_use_list,
            after_use_list,
            &before_metadata,
            &after_metadata,
        );

        assert!(
            reordered,
            "identical identifiers reshuffled inside a commutative container must be detected as reordered"
        );
        assert_eq!(
            pairs.len(),
            7,
            "every child must still be paired despite the reorder"
        );
    }

    #[test]
    fn pair_children_for_descent_reports_no_reorder_when_nothing_moved() {
        let before = Code::from_string("use std::{a, b, c};\n", &Language::Rust);
        let after = Code::from_string("use std::{a, b, c};\n", &Language::Rust);
        let before_metadata = metadata_of(&before);
        let after_metadata = metadata_of(&after);

        let before_root = before.ast.as_ref().unwrap().root_node();
        let after_root = after.ast.as_ref().unwrap().root_node();
        let before_use_list = find_first_of_kind(before_root, "use_list").unwrap();
        let after_use_list = find_first_of_kind(after_root, "use_list").unwrap();

        let (_, reordered) = pair_children_for_descent(
            before_use_list,
            after_use_list,
            &before_metadata,
            &after_metadata,
        );

        assert!(
            !reordered,
            "an unchanged commutative container must not be flagged as reordered"
        );
    }

    #[test]
    fn pair_children_for_descent_pairs_reordered_children_by_value_not_by_position() {
        let before = Code::from_string("use std::{a, b, c};\n", &Language::Rust);
        let after = Code::from_string("use std::{c, a, b};\n", &Language::Rust);
        let before_metadata = metadata_of(&before);
        let after_metadata = metadata_of(&after);
        let before_use_list =
            find_first_of_kind(before.ast.as_ref().unwrap().root_node(), "use_list").unwrap();
        let after_use_list =
            find_first_of_kind(after.ast.as_ref().unwrap().root_node(), "use_list").unwrap();

        let (pairs, _) = pair_children_for_descent(
            before_use_list,
            after_use_list,
            &before_metadata,
            &after_metadata,
        );

        for (b, a) in &pairs {
            assert_eq!(
                b.utf8_text(before.contents.as_bytes()).unwrap(),
                a.utf8_text(after.contents.as_bytes()).unwrap()
            );
        }
    }

    /// `rust-firefox-webrenderer-borders`: an edit earlier in the file shifts every byte offset
    /// inside the matched parents.
    #[test]
    fn pair_children_for_descent_breaks_ties_by_sibling_index_not_byte_offset() {
        let before = Code::from_string("use std::{a, a, a};\n", &Language::Rust);
        let after = Code::from_string("//\nuse std::{a, a, a};\n", &Language::Rust);
        let before_metadata = metadata_of(&before);
        let after_metadata = metadata_of(&after);
        let before_use_list =
            find_first_of_kind(before.ast.as_ref().unwrap().root_node(), "use_list").unwrap();
        let after_use_list =
            find_first_of_kind(after.ast.as_ref().unwrap().root_node(), "use_list").unwrap();

        let (pairs, reordered) = pair_children_for_descent(
            before_use_list,
            after_use_list,
            &before_metadata,
            &after_metadata,
        );

        assert!(!reordered);
        for (b, a) in &pairs {
            assert_eq!(
                b.start_byte() - before_use_list.start_byte(),
                a.start_byte() - after_use_list.start_byte()
            );
        }
    }
}
