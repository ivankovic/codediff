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

/// Configuration for selecting which nodes to consider for hash-based matching.
///
/// Nodes are included if they are either:
/// - Reference nodes (language-specific structural elements like functions, classes)
/// - OR meet the minimum depth and subtree size thresholds
///
/// This allows the hash matching passes to also consider large, deep subtrees
/// that aren't formally "reference nodes" but are still worth matching.
#[derive(Debug, Clone)]
pub struct NodeSelectionConfig {
    /// Minimum tree depth for a non-reference node to be included
    pub min_depth: usize,
    /// Minimum number of nodes in the subtree for a non-reference node to be included
    pub min_subtree_size: usize,
}

impl Default for NodeSelectionConfig {
    fn default() -> Self {
        Self {
            // Tuned against the benchmark suite: 0/45 was found to provide optimal results.
            // With solve_hash_descent's KindAndValueHash pass using extended selection and its
            // KindOnlyHash pass using only reference nodes, this gives: 1437 -> 771 (666 fewer
            // mismatches). This is the best configuration found after testing various thresholds.
            min_depth: 0,
            min_subtree_size: 45,
        }
    }
}

impl NodeSelectionConfig {
    /// Create a node list selector function that can be passed to `solve_with_hash_map`.
    ///
    /// The selector includes reference nodes plus any nodes that meet the depth/size thresholds,
    /// sorted by subtree size (largest first) and then by start byte for deterministic ordering.
    pub fn to_node_list_selector(&self) -> impl Fn(&ASTMetadata) -> Vec<usize> + '_ {
        move |metadata: &ASTMetadata| build_extended_node_list(metadata, self)
    }
}

/// Build an extended node list that includes reference nodes plus nodes meeting size thresholds.
///
/// Returns nodes sorted by subtree size (largest first), with ties broken by start_byte
/// for deterministic ordering across runs.
pub fn build_extended_node_list(
    metadata: &ASTMetadata,
    config: &NodeSelectionConfig,
) -> Vec<usize> {
    let language = metadata.language;
    let mut nodes_with_info: Vec<(usize, usize, usize)> = Vec::new(); // (node_id, subtree_size, start_byte)

    for (&node_id, info) in &metadata.node_info {
        let subtree_size = metadata
            .node_to_subtree_size
            .get(&node_id)
            .copied()
            .unwrap_or(0);
        let depth = metadata.node_to_depth.get(&node_id).copied().unwrap_or(0);
        let start_byte = info.start_byte;

        let is_reference_node = is_reference(&info.kind, &language);
        let is_big_enough = depth >= config.min_depth && subtree_size >= config.min_subtree_size;

        if is_reference_node || is_big_enough {
            nodes_with_info.push((node_id, subtree_size, start_byte));
        }
    }

    // Sort by subtree size descending, then by start_byte ascending for deterministic tiebreaking.
    // Using start_byte (document position) rather than node_id ensures stability across
    // separate parses of identical source, since node_ids are tree-sitter arena slots that
    // may differ between parses even for identical code.
    nodes_with_info.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)));
    nodes_with_info
        .into_iter()
        .map(|(node_id, _, _)| node_id)
        .collect()
}

/**
* Seven-phase pipeline rework (`TODO.md`, 2026-07-17): the generalized, reusable version of
* [`solve_with_hash_map`] phase 1 is built around. The old `HashMatchSpec` reads a *named* field
* off `ASTMetadata` via a function pointer (`|m| &m.node_to_full_hash`) - baking in "there is one
* fixed hash per purpose" and requiring a new `HashMatchSpec` variant (plus `ASTMappingReason`
* wiring) for every new hash algorithm. This version takes the before-side hash map and the
* after-side reverse map directly as parameters instead: the caller computes whichever hash
* algorithm it wants (`KindAndValueHash`, `KindOnlyHash`, a normalized-import-path hash, ...)
* before calling in, so this function never needs to know how many hash algorithms exist.
*
* Classification (`Identical` vs `Update`) no longer needs a `classify` function pointer either:
* regardless of which hash matched the pair, whether the match is byte-identical is answered by
* comparing `node_to_kind_and_value_hash` directly (always available - it's the finest-grained
* hash in the new pipeline) rather than threading a second, matcher-specific hash through the
* caller.
*/
// Each parameter is genuinely distinct context (both sides' `Code`, the node cache, the two hash
// maps, the diff, the node-list selector closure) - a params struct here would just relocate the
// same fields, not reduce them.
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
        // Same leaf-vs-interior distinction as the canonical rule in
        // `apted::common::classify_match`: `Update` means a *leaf's own value* changed; an
        // interior node whose `kind_and_value_hash` differs only because some descendant differs
        // (not necessarily this node's own text) is `MatchButNotIdentical` instead - cost 0 here,
        // matching `classify_match`'s own convention, since the children responsible for the
        // actual difference get their own separate classify() call (and their own cost) via this
        // same recursive descent, so charging COST_UPDATE again at every ancestor would double-
        // count it.
        //
        // This closure previously always returned `Update` regardless of leaf-vs-interior -
        // confirmed against a live case (`go-caddy-rename-type`, all 69 mismatches) that a single
        // renamed identifier deep inside several declarations bubbles its hash mismatch up to
        // every ancestor (`const_declaration`, `var_declaration`, `method_declaration`, ...), and
        // every one of them was mislabeled `Update` instead of `MatchButNotIdentical` - every
        // mismatch in that fixture has an identical before/after path, so this was purely an
        // operation-label bug, not a matching/pairing one.
        if before_metadata.is_leaf(before_id) && after_metadata.is_leaf(after_id) {
            (ASTMappingOperation::Update, crate::diff::COST_UPDATE)
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

        // Same tiebreak rationale as `solve_with_hash_map`: proximity in the file, not discovery
        // order, is what tells true duplicates apart from unrelated hash collisions.
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

        // Descend both subtrees in lockstep, pairing children by position and kind - except
        // under a commutative container, where children must be paired by hash instead (see
        // `pair_children_for_descent`'s doc comment for why a positional `zip` is wrong there).
        // `reordered_ids` collects every before-side node whose *own* children were found
        // reordered, so their ancestors (up to this match's own root) can be downgraded from
        // `Identical` afterward - a reorder several levels down (e.g. a `use_list` nested inside
        // `scoped_use_list`/`use_declaration`) means none of its ancestors are a true no-op match
        // either, even though none of *them* are commutative containers themselves.
        let mut reordered_ids: Vec<usize> = Vec::new();
        let mut stack = vec![(before_node, after_node)];
        while let Some((before_parent, after_parent)) = stack.pop() {
            let (pairs, reordered) = pair_children_for_descent(
                before_parent,
                after_parent,
                &before_metadata,
                &after_metadata,
            );

            // `before_parent`/`after_parent`'s own mapping was already added (either as the root
            // match above, or as a `descendant_reason`-tagged child pair in an earlier iteration
            // of this same loop) before we could know whether *its* children turned out to be
            // reordered - patch it now that we know, rather than looking ahead. A pure reorder is
            // not a no-op: children's content is unchanged, but their positions are, so it's
            // recorded as `MatchButNotIdentical` at `COST_UPDATE`, not `Identical` at cost 0 -
            // matching the human-authored ground truth's own convention for these pairs (see
            // `TODO.md`'s "Distinguishing reordered from truly identical" section).
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

        // Propagate: every ancestor between a reordered node and this match's own root
        // (inclusive) is downgraded from `Identical` to `MatchButNotIdentical` too - a container
        // is never a true no-op match if anything inside it, at any depth, wasn't. Reason is left
        // alone for these ancestors (only the node that's actually a commutative container with
        // reordered children gets `FullyMappingSubtrees` - see above); they didn't reorder
        // anything themselves, they just aren't a pure `Identical` match anymore either.
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

/**
* Pairs `before_parent`'s and `after_parent`'s children for the hash-descent engine's lockstep
* walk. Ordinary containers pair positionally (`zip`, filtered to matching kinds) - safe because
* the parent's own hash match (`KindAndValueHash`/`KindOnlyHash`) was computed in document order,
* so equal hashes already imply position-for-position correspondence.
*
* Under a `nodes::is_commutative_container` parent, that assumption breaks: both new hashes hash a
* commutative container's children *unordered* (sorted), so two containers can hash equal while
* their children sit in completely different positions (a same-name reorder is exactly what
* `is_commutative_container` exists to match). A positional `zip` there would silently mis-pair
* reordered children - the exact bug `code::hash::compute_commutative_structural_hash`'s own doc
* comment warned about ("reordered children get re-mangled by the shared engine's positional
* zip").
*
* Fix: pair by hash instead of position, in two tiers - **not** `node_to_kind_only_hash` alone
* (an earlier version of this function did that unconditionally, which is wrong whenever the
* *outer* match came from `KindAndValueHash`: `kind_only_hash` ignores leaf text, so e.g. three
* plain `identifier` children with different names all hash equal, and the nearest-by-position
* tiebreak can silently "recover" a pairing that looks unreordered even though the identifiers
* actually did move - which also breaks reorder detection below, since it works from whichever
* pairing this function returns).
* 1. `node_to_kind_and_value_hash` first (exact - correctly distinguishes same-kind, different-
*    value children like `a`/`b`/`c` above). Multiset equality here is guaranteed whenever the
*    outer match was itself `KindAndValueHash`-driven (the sorted-hash combination that produced
*    the parent's own hash could only be equal if the multiset of child hashes is equal); may
*    leave some children unpaired when the outer match was `KindOnlyHash`-driven instead (content
*    values may legitimately differ there), which tier 2 picks up.
* 2. `node_to_kind_only_hash` as a fallback, for whatever tier 1 left unpaired - the coarser
*    guarantee that *does* hold unconditionally (a `KindOnlyHash` match only guarantees kind-level
*    multiset equality), same tiebreak methodology.
*
* Either tier breaks ties among same-hash candidates by child-ordinal proximity (`before_index` vs
* `after_index` within their own parent's children list) - **not** absolute document byte position,
* which the top-level match itself uses but which is unsound here: `before_parent`/`after_parent`
* were already matched as a pair, but everything *outside* their span can still have grown or
* shrunk from unrelated earlier edits, shifting both parents' (and every descendant's) absolute
* byte offset by some constant delta. Nearest-by-absolute-byte-distance then silently prefers a
* neighboring same-hash sibling over the true positional counterpart whenever that delta exceeds
* half the local gap between siblings - confirmed against a real case
* (`rust-firefox-webrenderer-borders`: two untouched struct definitions later in the file, byte-
* shifted by 14 from edits earlier in the file, whose 25-27-byte comma-to-comma gaps made a
* same-hash *neighboring* comma look closer than each comma's own correct counterpart, rotating
* three of five `,` pairings). Child-ordinal position is relative to the already-matched parent and
* so is immune to any shift outside that parent's own span.
*
* Returns the pairs plus a `reordered` flag: true if `before_parent` is a commutative container
* and at least one pair's after-side document-order index differs from its before-side index -
* i.e. content-wise nothing changed, but the children's actual order did. The caller uses this to
* distinguish `FullyMappingSubtrees` (matched via order-independence, order genuinely changed)
* from a plain `IdenticalHash`/`StructurallyIdenticalSubtrees` match (order-independence didn't
* need to do anything, because nothing moved) - see the user request this responds to ("we do need
* a way to distinguish between truly identical and reordered") and `ASTMappingReason::
* FullyMappingSubtrees`'s doc comment.
*/
fn pair_children_for_descent<'a>(
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

    // Tier 2: kind-only fallback for whatever tier 1 (exact kind+value) couldn't pair - covers a
    // `KindOnlyHash`-driven outer match, where children may legitimately differ in value.
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
        // `{`, `a`, `,`, `b`, `,`, `c`, `}` - every child (identifiers and punctuation alike)
        // must still find its same-kind, same-text counterpart despite the reorder.
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
}
