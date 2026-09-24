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
//! Pre-matching passes that pin obvious pairs before the general search runs: identical statement
//! siblings, and locals that are unique by name.

use super::*;

/// Minimum direct-child count worth pre-matching via [`prematch_identical_statement_siblings`].
/// Far below `FLAT_MIN_CHILDREN` because that pass never commits a non-match.
pub(crate) const STATEMENT_PREMATCH_MIN_CHILDREN: usize = 4;

/// `(child count, id)` of the widest `nodes::is_statement_sequence_body` node at the shallowest
/// depth that has one, `root_id` included. Not `node_to_widest_subtree_node`, which is
/// kind-agnostic and can pick a wider but irrelevant node.
pub(crate) fn widest_statement_sequence_body(
    root_id: usize,
    meta: &ASTMetadata,
) -> Option<(usize, usize)> {
    // Shallowest, not widest overall: a nested loop body is the same kind as the function body,
    // and may be wider on one side only, pairing two unrelated sequences.
    let mut level = vec![root_id];
    while !level.is_empty() {
        let mut best: Option<(usize, usize)> = None;
        let mut next_level = Vec::new();
        for id in level {
            let Some(info) = meta.node_info.get(&id) else {
                continue;
            };
            if nodes::is_statement_sequence_body(&info.kind) {
                let count = info.children.len();
                if best.is_none_or(|(best_count, _)| count > best_count) {
                    best = Some((count, id));
                }
            } else {
                next_level.extend(info.children.iter().copied());
            }
        }
        if best.is_some() {
            return best;
        }
        level = next_level;
    }
    None
}

/// Pre-matches the byte-identical direct children of the two sides' statement-sequence bodies
/// (Myers over full hashes), so the scoped APTED call that follows indexes only the rest.
///
/// Unlike [`resolve_flat_tree_pair`] it emits only identical matches and leaves every other
/// child untouched: in a body of 10-40 statements the few that differ deserve a real APTED
/// resolution, not a committed delete/insert.
pub(crate) fn prematch_identical_statement_siblings(
    before_id: usize,
    after_id: usize,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    source: &'static str,
    diff: &mut ASTDiff,
) {
    let Some((before_count, before_flat)) = widest_statement_sequence_body(before_id, before_meta)
    else {
        return;
    };
    let Some((after_count, after_flat)) = widest_statement_sequence_body(after_id, after_meta)
    else {
        return;
    };
    if before_count < STATEMENT_PREMATCH_MIN_CHILDREN
        || after_count < STATEMENT_PREMATCH_MIN_CHILDREN
    {
        return;
    }

    let Some(before_info) = before_meta.node_info.get(&before_flat) else {
        return;
    };
    let Some(after_info) = after_meta.node_info.get(&after_flat) else {
        return;
    };
    let before_children: Vec<usize> = before_info
        .children
        .iter()
        .copied()
        .filter(|id| !diff.before_node_map.contains_key(id))
        .collect();
    let after_children: Vec<usize> = after_info
        .children
        .iter()
        .copied()
        .filter(|id| !diff.after_node_map.contains_key(id))
        .collect();
    if before_children.is_empty() || after_children.is_empty() {
        return;
    }

    let before_hashes: Vec<u64> = before_children
        .iter()
        .map(|&id| before_meta.node_to_full_hash.get(&id).copied().unwrap_or(0))
        .collect();
    let after_hashes: Vec<u64> = after_children
        .iter()
        .map(|&id| after_meta.node_to_full_hash.get(&id).copied().unwrap_or(0))
        .collect();

    let Some(pairs) = myers_lcs(&before_hashes, &after_hashes, FLAT_MAX_EDIT) else {
        return;
    };
    for (bi, ai) in pairs {
        emit_identical_subtree(
            before_children[bi],
            after_children[ai],
            before_meta,
            after_meta,
            source,
            diff,
        );
    }
}

/// Buckets every not-yet-mapped node under `node_id` (inclusive) by
/// `nodes::local_identity_name`. It descends into nested functions on purpose: two same-named
/// locals in different scopes then disqualify the name, which keeps uniqueness strict.
pub(crate) fn collect_local_identities(
    node_id: usize,
    meta: &ASTMetadata,
    node_map: &rustc_hash::FxHashMap<usize, usize>,
    language: &Language,
    groups: &mut rustc_hash::FxHashMap<(&'static str, String), Vec<usize>>,
) {
    if node_map.contains_key(&node_id) {
        return;
    }
    let Some(info) = meta.node_info.get(&node_id) else {
        return;
    };
    if let Some(key) = nodes::local_identity_name(node_id, meta, language) {
        groups.entry(key).or_default().push(node_id);
    }
    for &child_id in &info.children {
        collect_local_identities(child_id, meta, node_map, language, groups);
    }
}

/// Pre-matches locals (parameters, local variables, shell assignments; see
/// `nodes::local_identity_name`) whose name is unique on both sides, each through its own scoped
/// `for_nodes` call, so a changed body still gets `MatchButNotIdentical`.
///
/// Unit-cost APTED has no sense that "same name, shifted position" beats "same position,
/// different name", so an insertion mid-sequence often pairs locals by position. Ambiguous names
/// are left to APTED.
pub(crate) fn prematch_unique_named_locals(
    before_id: usize,
    after_id: usize,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    source: &'static str,
    diff: &mut ASTDiff,
) {
    let language = before_meta.language;
    if !nodes::has_local_identity_coverage(&language) {
        return;
    }

    let mut before_groups: rustc_hash::FxHashMap<(&'static str, String), Vec<usize>> =
        rustc_hash::FxHashMap::default();
    collect_local_identities(
        before_id,
        before_meta,
        &diff.before_node_map,
        &language,
        &mut before_groups,
    );
    if before_groups.is_empty() {
        return;
    }
    let mut after_groups: rustc_hash::FxHashMap<(&'static str, String), Vec<usize>> =
        rustc_hash::FxHashMap::default();
    collect_local_identities(
        after_id,
        after_meta,
        &diff.after_node_map,
        &language,
        &mut after_groups,
    );
    if after_groups.is_empty() {
        return;
    }

    let mut pairs: Vec<(usize, usize)> = before_groups
        .iter()
        .filter(|(_, ids)| ids.len() == 1)
        .filter_map(|(key, before_ids)| {
            let after_ids = after_groups.get(key)?;
            (after_ids.len() == 1).then(|| (before_ids[0], after_ids[0]))
        })
        .collect();
    // `HashMap` iteration order is not deterministic.
    pairs.sort_unstable_by_key(|&(b, _)| {
        before_meta
            .node_info
            .get(&b)
            .map(|i| i.preorder_index)
            .unwrap_or(usize::MAX)
    });

    for (before_child, after_child) in pairs {
        for_nodes(
            before_meta,
            after_meta,
            vec![before_child],
            vec![after_child],
            Algorithm::Apted,
            source,
            diff,
        );
    }
}
