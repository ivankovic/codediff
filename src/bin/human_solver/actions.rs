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
//! Applying and undoing the human's mapping decisions on the open case.

use crate::*;

// ---------------------------------------------------------------------------------------------
// Marking actions
// ---------------------------------------------------------------------------------------------

/// Removes the entries whose paths resolve to `before_id` or `after_id`, so re-marking a node
/// replaces its previous decision.
pub(crate) fn remove_direct_entries_for(
    entries: &mut Vec<HumanMappingEntry>,
    before_id: Option<usize>,
    after_id: Option<usize>,
    before_root: Node,
    after_root: Node,
) {
    entries.retain(|entry| {
        let touches_before = before_id.is_some()
            && entry
                .before_path
                .as_ref()
                .and_then(|p| node_for_path(before_root, &path_refs(p)).ok())
                .map(|n| Some(n.id()) == before_id)
                .unwrap_or(false);
        let touches_after = after_id.is_some()
            && entry
                .after_path
                .as_ref()
                .and_then(|p| node_for_path(after_root, &path_refs(p)).ok())
                .map(|n| Some(n.id()) == after_id)
                .unwrap_or(false);
        !(touches_before || touches_after)
    });
}

/// [`remove_direct_entries_for`] for whole id sets in one pass. `M` over a big subtree needs this:
/// one scan per node is quadratic.
pub(crate) fn remove_entries_touching(
    entries: &mut Vec<HumanMappingEntry>,
    before_ids: &std::collections::HashSet<usize>,
    after_ids: &std::collections::HashSet<usize>,
    before_root: Node,
    after_root: Node,
) {
    entries.retain(|entry| {
        let touches_before = entry
            .before_path
            .as_ref()
            .and_then(|p| node_for_path(before_root, &path_refs(p)).ok())
            .is_some_and(|n| before_ids.contains(&n.id()));
        let touches_after = entry
            .after_path
            .as_ref()
            .and_then(|p| node_for_path(after_root, &path_refs(p)).ok())
            .is_some_and(|n| after_ids.contains(&n.id()));
        !(touches_before || touches_after)
    });
}

/// Finds `id` anywhere under `root`, unlike `flatten_visible` (the same search as
/// [`find_node_by_id_anywhere`]). A multi-map selection member can be collapsed or hidden by the
/// time `m`/`M` commits it.
pub(crate) fn find_node_anywhere(root: Node, id: usize) -> Option<Node> {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.id() == id {
            return Some(n);
        }
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    None
}

/// Removes any [`MultiMapGroup`] sharing a node with `before_ids`/`after_ids`, so no stale group
/// half-references a reassigned node.
pub(crate) fn remove_groups_touching(
    groups: &mut Vec<MultiMapGroup>,
    before_ids: &std::collections::HashSet<usize>,
    after_ids: &std::collections::HashSet<usize>,
    before_root: Node,
    after_root: Node,
) {
    groups.retain(|group| {
        let touches_before = group.before_paths.iter().any(|p| {
            node_for_path(before_root, &path_refs(p))
                .ok()
                .is_some_and(|n| before_ids.contains(&n.id()))
        });
        let touches_after = group.after_paths.iter().any(|p| {
            node_for_path(after_root, &path_refs(p))
                .ok()
                .is_some_and(|n| after_ids.contains(&n.id()))
        });
        !(touches_before || touches_after)
    });
}

pub(crate) fn is_strict_descendant_of(node: Node, ancestor: Node) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.id() == ancestor.id() {
            return true;
        }
        current = parent;
    }
    false
}

/// Drops entries on strict descendants of `ancestor`, which a with-children mark on it overrides;
/// keeping them would export a self-contradictory mapping.
pub(crate) fn clear_before_descendants(
    entries: &mut Vec<HumanMappingEntry>,
    ancestor: Node,
    before_root: Node,
) {
    entries.retain(|entry| {
        !entry
            .before_path
            .as_ref()
            .and_then(|p| node_for_path(before_root, &path_refs(p)).ok())
            .map(|n| is_strict_descendant_of(n, ancestor))
            .unwrap_or(false)
    });
}

pub(crate) fn clear_after_descendants(
    entries: &mut Vec<HumanMappingEntry>,
    ancestor: Node,
    after_root: Node,
) {
    entries.retain(|entry| {
        !entry
            .after_path
            .as_ref()
            .and_then(|p| node_for_path(after_root, &path_refs(p)).ok())
            .map(|n| is_strict_descendant_of(n, ancestor))
            .unwrap_or(false)
    });
}

/// What an `m`/`M` press does next: done, or a question for the human before anything is written.
pub(crate) enum ActionOutcome {
    Done(String),
    /// Boxed because `Modal` is much larger than `Done`'s `String` (clippy `large_enum_variant`).
    NeedsModal(Box<Modal>),
}

pub(crate) fn node_values_equal(b: Node, a: Node, before_src: &[u8], after_src: &[u8]) -> bool {
    b.utf8_text(before_src).unwrap_or("") == a.utf8_text(after_src).unwrap_or("")
}

/// Replaces any direct entry touching `b` or `a` with one entry pairing them.
pub(crate) fn apply_match_entry(
    mapping: &mut HumanMapping,
    before_root: Node,
    after_root: Node,
    b: Node,
    a: Node,
    operation: HumanOperation,
) {
    remove_direct_entries_for(
        &mut mapping.entries,
        Some(b.id()),
        Some(a.id()),
        before_root,
        after_root,
    );
    mapping.entries.push(HumanMappingEntry {
        operation,
        before_path: Some(path_for_node(b)),
        after_path: Some(path_for_node(a)),
    });
}

/// `Identical` iff the two content hashes (kind, text and children, see `code::hash::hash_code`)
/// match, else `MatchButNotIdentical`. A missing hash counts as not identical.
pub(crate) fn subtree_match_operation(
    before_id: usize,
    after_id: usize,
    before_hash: &rustc_hash::FxHashMap<usize, u64>,
    after_hash: &rustc_hash::FxHashMap<usize, u64>,
) -> HumanOperation {
    let identical = matches!(
        (before_hash.get(&before_id), after_hash.get(&after_id)),
        (Some(b), Some(a)) if b == a
    );
    if identical {
        HumanOperation::Identical
    } else {
        HumanOperation::MatchButNotIdentical
    }
}

/// `Identical` only if every selected node on both sides shares one content hash. Never `Update`:
/// see `MultiMapGroup::operation`.
pub(crate) fn multi_map_group_operation(
    before_ids: &std::collections::BTreeSet<usize>,
    after_ids: &std::collections::BTreeSet<usize>,
    before_hash: &rustc_hash::FxHashMap<usize, u64>,
    after_hash: &rustc_hash::FxHashMap<usize, u64>,
) -> HumanOperation {
    let mut hashes = before_ids
        .iter()
        .filter_map(|id| before_hash.get(id).copied())
        .chain(
            after_ids
                .iter()
                .filter_map(|id| after_hash.get(id).copied()),
        );
    let first = hashes.next();
    if first.is_some() && hashes.all(|h| Some(h) == first) {
        HumanOperation::Identical
    } else {
        HumanOperation::MatchButNotIdentical
    }
}

/// Commits a confirmed multi-map selection as a new [`MultiMapGroup`], first removing any entry or
/// group that touches one of its nodes.
#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_multi_map_group(
    mapping: &mut HumanMapping,
    before_root: Node,
    after_root: Node,
    before_ids: &std::collections::BTreeSet<usize>,
    after_ids: &std::collections::BTreeSet<usize>,
    operation: HumanOperation,
    with_children: bool,
    pairing: GroupPairing,
) -> Result<String> {
    let mut before_nodes = before_ids
        .iter()
        .map(|&id| {
            find_node_anywhere(before_root, id)
                .context("A selected Before node could no longer be found in the tree")
        })
        .collect::<Result<Vec<Node>>>()?;
    let mut after_nodes = after_ids
        .iter()
        .map(|&id| {
            find_node_anywhere(after_root, id)
                .context("A selected After node could no longer be found in the tree")
        })
        .collect::<Result<Vec<Node>>>()?;
    // Source-position order: arena ids are not stable across parses. `representative_entries`
    // sorts the same way.
    before_nodes.sort_by_key(|n| n.start_byte());
    after_nodes.sort_by_key(|n| n.start_byte());

    let before_id_set: std::collections::HashSet<usize> = before_ids.iter().copied().collect();
    let after_id_set: std::collections::HashSet<usize> = after_ids.iter().copied().collect();
    remove_entries_touching(
        &mut mapping.entries,
        &before_id_set,
        &after_id_set,
        before_root,
        after_root,
    );
    remove_groups_touching(
        &mut mapping.groups,
        &before_id_set,
        &after_id_set,
        before_root,
        after_root,
    );
    if with_children {
        // A with-children member's descendants must stay free for codediff to pair however it pairs
        // them (see `check_subtree_maps_within`), so leftover entries on them go, as for `d`/`i`.
        for &node in &before_nodes {
            clear_before_descendants(&mut mapping.entries, node, before_root);
        }
        for &node in &after_nodes {
            clear_after_descendants(&mut mapping.entries, node, after_root);
        }
    }

    let (before_count, after_count) = (before_nodes.len(), after_nodes.len());
    mapping.groups.push(MultiMapGroup {
        before_paths: before_nodes.into_iter().map(path_for_node).collect(),
        after_paths: after_nodes.into_iter().map(path_for_node).collect(),
        operation,
        with_children,
        pairing,
    });

    Ok(format!(
        "Committed {} group: {} before, {} after node(s), {:?}{}",
        group_pairing_name(pairing),
        before_count,
        after_count,
        operation,
        if with_children { " with children" } else { "" }
    ))
}

/// "multi-map" is the original kind of group and keeps its name in messages.
pub(crate) fn group_pairing_name(pairing: GroupPairing) -> &'static str {
    match pairing {
        GroupPairing::AnyOneToOne => "multi-map",
        GroupPairing::AllToAll => "all-to-all",
    }
}

/// A member under a deleted/inserted-with-children ancestor would make the mapping contradict
/// itself.
fn ensure_members_not_under_removed_ancestor(
    before_root: Node,
    after_root: Node,
    before_ids: &std::collections::BTreeSet<usize>,
    after_ids: &std::collections::BTreeSet<usize>,
    caches: &Caches,
) -> Result<()> {
    for &id in before_ids {
        if let Some(node) = find_node_anywhere(before_root, id)
            && is_inherited_removed(node, &caches.before_removed)
        {
            bail!(
                "Before node '{}' is covered by an ancestor's delete-with-children mark; clear that first (u on the ancestor)",
                node.kind()
            );
        }
    }
    for &id in after_ids {
        if let Some(node) = find_node_anywhere(after_root, id)
            && is_inherited_removed(node, &caches.after_removed)
        {
            bail!(
                "After node '{}' is covered by an ancestor's insert-with-children mark; clear that first (u on the ancestor)",
                node.kind()
            );
        }
    }
    Ok(())
}

/// The member sets `M` commits for an all-to-all selection: the roots, then one set per child
/// position at every depth, so every node lands in exactly one group with its counterparts.
/// Every member of a set must agree on kind and child count; the first divergence is an error and
/// nothing is returned, so the caller commits all or nothing (`m` commits the roots alone).
pub(crate) fn all_to_all_subtree_groups<'t>(
    before: Vec<Node<'t>>,
    after: Vec<Node<'t>>,
    before_src: &[u8],
    after_src: &[u8],
) -> Result<Vec<(Vec<Node<'t>>, Vec<Node<'t>>)>> {
    let mut groups = Vec::new();
    collect_all_to_all_subtree_groups(before, after, before_src, after_src, &mut groups)?;
    Ok(groups)
}

fn collect_all_to_all_subtree_groups<'t>(
    before: Vec<Node<'t>>,
    after: Vec<Node<'t>>,
    before_src: &[u8],
    after_src: &[u8],
    groups: &mut Vec<(Vec<Node<'t>>, Vec<Node<'t>>)>,
) -> Result<()> {
    // The first before node is the reference shape; it only decides which node a message calls
    // the expected one.
    let reference = before[0];
    let members = before
        .iter()
        .map(|node| (Side::Before, *node))
        .chain(after.iter().map(|node| (Side::After, *node)));
    for (side, node) in members {
        let (src, side_name) = match side {
            Side::Before => (before_src, "Before"),
            Side::After => (after_src, "After"),
        };
        if node.kind() != reference.kind() {
            bail!(
                "Subtrees diverge: Before {} vs {side_name} {} - kinds differ; nothing committed (m commits the roots alone)",
                node_label(reference, before_src),
                node_label(node, src)
            );
        }
        if node.child_count() != reference.child_count() {
            bail!(
                "Subtrees diverge: Before {} has {} child(ren) but {side_name} {} has {}; nothing committed (m commits the roots alone)",
                node_label(reference, before_src),
                reference.child_count(),
                node_label(node, src),
                node.child_count()
            );
        }
    }

    let child_count = reference.child_count();
    groups.push((before.clone(), after.clone()));
    for index in 0..child_count {
        let child = |node: &Node<'t>| {
            node.child(index)
                .expect("every member has child_count children")
        };
        collect_all_to_all_subtree_groups(
            before.iter().map(child).collect(),
            after.iter().map(child).collect(),
            before_src,
            after_src,
            groups,
        )?;
    }
    Ok(())
}

/// `ids` as nodes, in `commit_multi_map_group`'s source-position order.
fn selected_nodes<'t>(
    root: Node<'t>,
    ids: &std::collections::BTreeSet<usize>,
) -> Result<Vec<Node<'t>>> {
    let mut nodes = ids
        .iter()
        .map(|&id| {
            find_node_anywhere(root, id)
                .context("A selected node could no longer be found in the tree")
        })
        .collect::<Result<Vec<Node>>>()?;
    nodes.sort_by_key(|node| node.start_byte());
    Ok(nodes)
}

/// `M` on an all-to-all selection: one `AllToAll` group per [`all_to_all_subtree_groups`] set, none
/// `with_children` (each descendant has its own group). Operations are inferred per group, so
/// identical tokens under differing parents are still `Identical`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn action_commit_all_to_all_subtrees(
    mapping: &mut HumanMapping,
    before_root: Node,
    after_root: Node,
    before_ids: &std::collections::BTreeSet<usize>,
    after_ids: &std::collections::BTreeSet<usize>,
    before_hash: &rustc_hash::FxHashMap<usize, u64>,
    after_hash: &rustc_hash::FxHashMap<usize, u64>,
    caches: &Caches,
    before_src: &[u8],
    after_src: &[u8],
) -> Result<String> {
    if before_ids.is_empty() || after_ids.is_empty() {
        bail!(
            "Multi-map group needs at least one selected node on both sides (x to select, c to clear)"
        );
    }
    ensure_members_not_under_removed_ancestor(
        before_root,
        after_root,
        before_ids,
        after_ids,
        caches,
    )?;

    let before_roots = selected_nodes(before_root, before_ids)?;
    let after_roots = selected_nodes(after_root, after_ids)?;
    let root_kind = before_roots[0].kind().to_string();

    let groups = all_to_all_subtree_groups(before_roots, after_roots, before_src, after_src)?;
    let count = groups.len();
    for (before, after) in groups {
        let before_set: std::collections::BTreeSet<usize> = before.iter().map(Node::id).collect();
        let after_set: std::collections::BTreeSet<usize> = after.iter().map(Node::id).collect();
        let operation = multi_map_group_operation(&before_set, &after_set, before_hash, after_hash);
        commit_multi_map_group(
            mapping,
            before_root,
            after_root,
            &before_set,
            &after_set,
            operation,
            false,
            GroupPairing::AllToAll,
        )?;
    }
    Ok(format!(
        "Committed {count} all-to-all groups: every node of {} before and {} after '{root_kind}' subtree(s), position by position",
        before_ids.len(),
        after_ids.len()
    ))
}

/// `m`/`M` with a non-empty multi-map selection: commits it directly when every node shares one
/// kind, otherwise raises `Modal::ConfirmMultiMapGroup` first.
#[allow(clippy::too_many_arguments)]
pub(crate) fn action_commit_multi_map_group(
    mapping: &mut HumanMapping,
    before_root: Node,
    after_root: Node,
    before_ids: &std::collections::BTreeSet<usize>,
    after_ids: &std::collections::BTreeSet<usize>,
    before_hash: &rustc_hash::FxHashMap<usize, u64>,
    after_hash: &rustc_hash::FxHashMap<usize, u64>,
    caches: &Caches,
    with_children: bool,
    pairing: GroupPairing,
) -> Result<ActionOutcome> {
    if before_ids.is_empty() || after_ids.is_empty() {
        bail!(
            "Multi-map group needs at least one selected node on both sides (x to select, c to clear)"
        );
    }

    ensure_members_not_under_removed_ancestor(
        before_root,
        after_root,
        before_ids,
        after_ids,
        caches,
    )?;

    let operation = multi_map_group_operation(before_ids, after_ids, before_hash, after_hash);

    let kinds: Vec<String> = before_ids
        .iter()
        .filter_map(|&id| find_node_anywhere(before_root, id))
        .chain(
            after_ids
                .iter()
                .filter_map(|&id| find_node_anywhere(after_root, id)),
        )
        .map(|n| n.kind().to_string())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    if kinds.len() > 1 {
        return Ok(ActionOutcome::NeedsModal(Box::new(
            Modal::ConfirmMultiMapGroup {
                before_ids: before_ids.iter().copied().collect(),
                after_ids: after_ids.iter().copied().collect(),
                operation,
                with_children,
                pairing,
                kinds,
            },
        )));
    }

    let msg = commit_multi_map_group(
        mapping,
        before_root,
        after_root,
        before_ids,
        after_ids,
        operation,
        with_children,
        pairing,
    )?;
    Ok(ActionOutcome::Done(msg))
}

/// The `NeedsModal` outcome for a cursor pair of different kinds; `recursive` is `M` rather than
/// `m` (see [`Modal::ConfirmKindMismatch`]).
pub(crate) fn kind_mismatch_modal(
    before_node: Node,
    after_node: Node,
    recursive: bool,
) -> ActionOutcome {
    ActionOutcome::NeedsModal(Box::new(Modal::ConfirmKindMismatch {
        before_id: before_node.id(),
        after_id: after_node.id(),
        before_kind: before_node.kind().to_string(),
        after_kind: after_node.kind().to_string(),
        recursive,
    }))
}

/// The operation a single `m` gives a same-kind pair: by text for leaves, by content hash
/// ([`subtree_match_operation`]) otherwise.
pub(crate) fn classify_match_operation(
    before_node: Node,
    after_node: Node,
    before_src: &[u8],
    after_src: &[u8],
    before_hash: &rustc_hash::FxHashMap<usize, u64>,
    after_hash: &rustc_hash::FxHashMap<usize, u64>,
) -> HumanOperation {
    if before_node.child_count() == 0 && after_node.child_count() == 0 {
        if node_values_equal(before_node, after_node, before_src, after_src) {
            HumanOperation::Identical
        } else {
            HumanOperation::Update
        }
    } else {
        subtree_match_operation(before_node.id(), after_node.id(), before_hash, after_hash)
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn action_match(
    mapping: &mut HumanMapping,
    before_flat: &FlatIndex,
    after_flat: &FlatIndex,
    before_cursor: usize,
    after_cursor: usize,
    before_root: Node,
    after_root: Node,
    caches: &Caches,
    before_src: &[u8],
    after_src: &[u8],
    before_hash: &rustc_hash::FxHashMap<usize, u64>,
    after_hash: &rustc_hash::FxHashMap<usize, u64>,
) -> Result<ActionOutcome> {
    let before_node = before_flat
        .node_for_id(before_cursor)
        .context("Before cursor node not found")?;
    let after_node = after_flat
        .node_for_id(after_cursor)
        .context("After cursor node not found")?;

    if is_inherited_removed(before_node, &caches.before_removed) {
        bail!(
            "Before node is covered by an ancestor's delete-with-children mark; clear that first (u on the ancestor)"
        );
    }
    if is_inherited_removed(after_node, &caches.after_removed) {
        bail!(
            "After node is covered by an ancestor's insert-with-children mark; clear that first (u on the ancestor)"
        );
    }

    if before_node.kind() != after_node.kind() {
        return Ok(kind_mismatch_modal(before_node, after_node, false));
    }

    let operation = classify_match_operation(
        before_node,
        after_node,
        before_src,
        after_src,
        before_hash,
        after_hash,
    );
    apply_match_entry(
        mapping,
        before_root,
        after_root,
        before_node,
        after_node,
        operation,
    );
    Ok(ActionOutcome::Done(format!(
        "Matched '{}' <-> '{}' as {:?}",
        before_node.kind(),
        after_node.kind(),
        operation
    )))
}

/// `f`: repeats `m` and advance-both-cursors until one side has no `Unmarked` node left or the
/// next pair's kinds differ (where `m` would ask). Earlier matches are kept, and `f` resumes.
///
/// Must stay linear: it can run once per node, so it updates `caches` in place, appends entries
/// without [`apply_match_entry`]'s dedup scan (both nodes are `Unmarked`, so there is nothing to
/// remove), tracks cursors as indices, and reads paths from [`precompute_paths`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn action_match_to_end(
    app: &mut App,
    before_flat: &FlatIndex,
    after_flat: &FlatIndex,
    before_root: Node,
    after_root: Node,
    before_src: &[u8],
    after_src: &[u8],
    before_hash: &rustc_hash::FxHashMap<usize, u64>,
    after_hash: &rustc_hash::FxHashMap<usize, u64>,
) -> Result<ActionOutcome> {
    let mut matched = 0usize;
    let mut caches = rebuild_caches_for_mapping(&app.mapping, before_root, after_root);
    let before_paths = precompute_paths(before_root);
    let after_paths = precompute_paths(after_root);

    let mut before_idx = before_flat
        .index_of(app.before.cursor_id)
        .context("Before cursor node not found")?;
    let mut after_idx = after_flat
        .index_of(app.after.cursor_id)
        .context("After cursor node not found")?;

    loop {
        let before_node = before_flat[before_idx].0;
        let after_node = after_flat[after_idx].0;

        // Also covers nodes under an inherited with-children mark: those are never `Unmarked`.
        if status_before(before_node, &caches) != NodeStatus::Unmarked
            || status_after(after_node, &caches) != NodeStatus::Unmarked
        {
            break;
        }

        if before_node.kind() != after_node.kind() {
            app.before.cursor_id = before_node.id();
            app.after.cursor_id = after_node.id();
            // A one-sided diff already answers the modal's question (see [`one_sided_diff`]), so focus
            // the side that needs marking and stop. It does not mark for the human: what to record is
            // the ground truth author's call.
            if let Some(side) = run_unix_diff(before_src, after_src)
                .ok()
                .as_deref()
                .and_then(one_sided_diff)
            {
                app.focus = match side {
                    Side::Before => Focus::Before,
                    Side::After => Focus::After,
                };
                let panel = match side {
                    Side::Before => "Before",
                    Side::After => "After",
                };
                let reason = match side {
                    Side::Before => "the diff only removes",
                    Side::After => "the diff only adds",
                };
                return Ok(ActionOutcome::Done(format!(
                    "{} - kinds differ ({} vs {}); {reason}, so the {panel} panel has the focus",
                    if matched == 0 {
                        "Nothing matched".to_string()
                    } else {
                        format!("Matched {matched} pair(s)")
                    },
                    before_node.kind(),
                    after_node.kind(),
                )));
            }
            return Ok(kind_mismatch_modal(before_node, after_node, false));
        }

        let operation = classify_match_operation(
            before_node,
            after_node,
            before_src,
            after_src,
            before_hash,
            after_hash,
        );
        app.mapping.entries.push(HumanMappingEntry {
            operation,
            before_path: before_paths.get(&before_node.id()).cloned(),
            after_path: after_paths.get(&after_node.id()).cloned(),
        });
        caches
            .before_match
            .insert(before_node.id(), after_node.id());
        caches.after_match.insert(after_node.id(), before_node.id());
        app.dirty = true;
        matched += 1;

        let next_before = next_unmarked_index(before_idx + 1, before_flat, &caches, status_before);
        let next_after = next_unmarked_index(after_idx + 1, after_flat, &caches, status_after);
        if let Some(idx) = next_before {
            before_idx = idx;
        }
        if let Some(idx) = next_after {
            after_idx = idx;
        }
        if next_before.is_none() || next_after.is_none() {
            break;
        }
    }

    app.before.cursor_id = before_flat[before_idx].0.id();
    app.after.cursor_id = after_flat[after_idx].0.id();

    Ok(ActionOutcome::Done(if matched == 0 {
        "Nothing left to match".to_string()
    } else {
        format!("Matched {matched} pair(s) up to end of file")
    }))
}

/// The first `Unmarked` index at or after `start`. Monotonic callers do O(n) work in total.
pub(crate) fn next_unmarked_index(
    start: usize,
    flat: &[(Node, usize)],
    caches: &Caches,
    status_fn: fn(Node, &Caches) -> NodeStatus,
) -> Option<usize> {
    (start..flat.len()).find(|&i| status_fn(flat[i].0, caches) == NodeStatus::Unmarked)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn action_match_subtree(
    mapping: &mut HumanMapping,
    before_flat: &FlatIndex,
    after_flat: &FlatIndex,
    before_cursor: usize,
    after_cursor: usize,
    before_root: Node,
    after_root: Node,
    caches: &Caches,
    before_src: &[u8],
    after_src: &[u8],
    before_hash: &rustc_hash::FxHashMap<usize, u64>,
    after_hash: &rustc_hash::FxHashMap<usize, u64>,
    before_collapsed: &mut std::collections::HashSet<usize>,
    after_collapsed: &mut std::collections::HashSet<usize>,
) -> Result<ActionOutcome> {
    let before_node = before_flat
        .node_for_id(before_cursor)
        .context("Before cursor node not found")?;
    let after_node = after_flat
        .node_for_id(after_cursor)
        .context("After cursor node not found")?;

    if is_inherited_removed(before_node, &caches.before_removed) {
        bail!(
            "Before node is covered by an ancestor's delete-with-children mark; clear that first (u on the ancestor)"
        );
    }
    if is_inherited_removed(after_node, &caches.after_removed) {
        bail!(
            "After node is covered by an ancestor's insert-with-children mark; clear that first (u on the ancestor)"
        );
    }

    if before_node.kind() != after_node.kind() {
        return Ok(kind_mismatch_modal(before_node, after_node, true));
    }

    if before_node.child_count() == 0 && after_node.child_count() == 0 {
        let identical = node_values_equal(before_node, after_node, before_src, after_src);
        let operation = if identical {
            HumanOperation::Identical
        } else {
            HumanOperation::Update
        };
        apply_match_entry(
            mapping,
            before_root,
            after_root,
            before_node,
            after_node,
            operation,
        );
        return Ok(ActionOutcome::Done(format!(
            "Matched '{}' <-> '{}' as {}",
            before_node.kind(),
            after_node.kind(),
            if identical { "Identical" } else { "Update" }
        )));
    }

    let operation =
        subtree_match_operation(before_node.id(), after_node.id(), before_hash, after_hash);
    let msg = apply_modal_choice(
        mapping,
        before_flat,
        after_flat,
        before_root,
        after_root,
        caches,
        before_src,
        after_src,
        before_node.id(),
        after_node.id(),
        operation,
        true,
        before_collapsed,
        after_collapsed,
    );
    Ok(ActionOutcome::Done(msg))
}

/// Auto-matches `b` <-> `a` and every descendant pair, without prompting: leaves by text,
/// containers `Identical` only if every descendant was. Stops descending where child kinds
/// diverge or a node is under an unrelated ancestor mark. `Identical` pairs with children are
/// collapsed in both panels. Returns whether the whole subtree matched `Identical`.
///
/// Buffers entries into `new_entries` and records decided ids in `touched_before`/`touched_after`
/// so [`apply_modal_choice`] can clear old entries in one pass (a scan per node is quadratic). Only
/// touched ids are cleared: a descendant the recursion never visited keeps its entry.
#[allow(clippy::too_many_arguments)]
pub(crate) fn auto_match_pair(
    new_entries: &mut Vec<HumanMappingEntry>,
    touched_before: &mut std::collections::HashSet<usize>,
    touched_after: &mut std::collections::HashSet<usize>,
    caches: &Caches,
    b: Node,
    a: Node,
    before_src: &[u8],
    after_src: &[u8],
    before_paths: &HashMap<usize, Vec<String>>,
    after_paths: &HashMap<usize, Vec<String>>,
    matched: &mut usize,
    skipped: &mut usize,
    before_collapsed: &mut std::collections::HashSet<usize>,
    after_collapsed: &mut std::collections::HashSet<usize>,
) -> bool {
    if is_inherited_removed(b, &caches.before_removed)
        || is_inherited_removed(a, &caches.after_removed)
    {
        *skipped += 1;
        return false;
    }

    let push = |new_entries: &mut Vec<HumanMappingEntry>,
                touched_before: &mut std::collections::HashSet<usize>,
                touched_after: &mut std::collections::HashSet<usize>,
                operation: HumanOperation| {
        new_entries.push(HumanMappingEntry {
            operation,
            before_path: before_paths.get(&b.id()).cloned(),
            after_path: after_paths.get(&a.id()).cloned(),
        });
        touched_before.insert(b.id());
        touched_after.insert(a.id());
    };

    if b.kind() != a.kind() {
        // The top pair's children reach here without a shape check.
        push(
            new_entries,
            touched_before,
            touched_after,
            HumanOperation::MatchButNotIdentical,
        );
        *matched += 1;
        return false;
    }

    let mut b_cursor = b.walk();
    let b_children: Vec<Node> = b.children(&mut b_cursor).collect();
    let mut a_cursor = a.walk();
    let a_children: Vec<Node> = a.children(&mut a_cursor).collect();

    if b_children.is_empty() && a_children.is_empty() {
        let identical = node_values_equal(b, a, before_src, after_src);
        push(
            new_entries,
            touched_before,
            touched_after,
            if identical {
                HumanOperation::Identical
            } else {
                HumanOperation::Update
            },
        );
        *matched += 1;
        return identical;
    }

    let same_shape = b_children.len() == a_children.len()
        && b_children
            .iter()
            .zip(&a_children)
            .all(|(x, y)| x.kind() == y.kind());

    if !same_shape {
        push(
            new_entries,
            touched_before,
            touched_after,
            HumanOperation::MatchButNotIdentical,
        );
        *matched += 1;
        return false;
    }

    let mut all_identical = true;
    for (b_child, a_child) in b_children.into_iter().zip(a_children) {
        let child_identical = auto_match_pair(
            new_entries,
            touched_before,
            touched_after,
            caches,
            b_child,
            a_child,
            before_src,
            after_src,
            before_paths,
            after_paths,
            matched,
            skipped,
            before_collapsed,
            after_collapsed,
        );
        all_identical &= child_identical;
    }

    push(
        new_entries,
        touched_before,
        touched_after,
        if all_identical {
            HumanOperation::Identical
        } else {
            HumanOperation::MatchButNotIdentical
        },
    );
    *matched += 1;
    if all_identical {
        before_collapsed.insert(b.id());
        after_collapsed.insert(a.id());
    }
    all_identical
}

/// Applies `operation` to the top pair and, if `recursive`, auto-fills the subtree via
/// [`auto_match_pair`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_modal_choice(
    mapping: &mut HumanMapping,
    before_flat: &FlatIndex,
    after_flat: &FlatIndex,
    before_root: Node,
    after_root: Node,
    caches: &Caches,
    before_src: &[u8],
    after_src: &[u8],
    before_id: usize,
    after_id: usize,
    operation: HumanOperation,
    recursive: bool,
    before_collapsed: &mut std::collections::HashSet<usize>,
    after_collapsed: &mut std::collections::HashSet<usize>,
) -> String {
    let (Some(b), Some(a)) = (
        before_flat.node_for_id(before_id),
        after_flat.node_for_id(after_id),
    ) else {
        return "Node no longer available (tree changed?)".to_string();
    };

    apply_match_entry(mapping, before_root, after_root, b, a, operation);

    if !recursive {
        return format!(
            "Matched '{}' <-> '{}' as {:?}",
            b.kind(),
            a.kind(),
            operation
        );
    }

    if operation == HumanOperation::Identical {
        before_collapsed.insert(b.id());
        after_collapsed.insert(a.id());
    }

    let mut matched = 1usize;
    let mut skipped = 0usize;

    let mut b_cursor = b.walk();
    let b_children: Vec<Node> = b.children(&mut b_cursor).collect();
    let mut a_cursor = a.walk();
    let a_children: Vec<Node> = a.children(&mut a_cursor).collect();
    let same_shape = b_children.len() == a_children.len()
        && b_children
            .iter()
            .zip(&a_children)
            .all(|(x, y)| x.kind() == y.kind());

    if same_shape {
        let before_paths = precompute_paths(before_root);
        let after_paths = precompute_paths(after_root);
        let mut new_entries = Vec::new();
        let mut touched_before = std::collections::HashSet::new();
        let mut touched_after = std::collections::HashSet::new();

        for (b_child, a_child) in b_children.into_iter().zip(a_children) {
            auto_match_pair(
                &mut new_entries,
                &mut touched_before,
                &mut touched_after,
                caches,
                b_child,
                a_child,
                before_src,
                after_src,
                &before_paths,
                &after_paths,
                &mut matched,
                &mut skipped,
                before_collapsed,
                after_collapsed,
            );
        }

        // One batched clear of the ids the recursion decided on, then append (see
        // `auto_match_pair`).
        remove_entries_touching(
            &mut mapping.entries,
            &touched_before,
            &touched_after,
            before_root,
            after_root,
        );
        mapping.entries.extend(new_entries);
    }

    if skipped > 0 {
        format!(
            "Matched {} node pair(s) under '{}' <-> '{}'; skipped {} node(s) already covered by an unrelated ancestor mark",
            matched,
            b.kind(),
            a.kind(),
            skipped
        )
    } else {
        format!(
            "Matched {} node pair(s) under '{}' <-> '{}'",
            matched,
            b.kind(),
            a.kind()
        )
    }
}

pub(crate) fn action_delete(
    mapping: &mut HumanMapping,
    before_flat: &FlatIndex,
    before_cursor: usize,
    before_root: Node,
    after_root: Node,
    with_children: bool,
    caches: &Caches,
) -> Result<String> {
    let before_node = before_flat
        .node_for_id(before_cursor)
        .context("Before cursor node not found")?;

    if is_inherited_removed(before_node, &caches.before_removed) {
        bail!(
            "Node is already covered by an ancestor's delete-with-children mark; clear that first (u on the ancestor)"
        );
    }

    remove_direct_entries_for(
        &mut mapping.entries,
        Some(before_node.id()),
        None,
        before_root,
        after_root,
    );
    if with_children {
        clear_before_descendants(&mut mapping.entries, before_node, before_root);
    }
    mapping.entries.push(HumanMappingEntry {
        operation: if with_children {
            HumanOperation::DeleteWithChildren
        } else {
            HumanOperation::Delete
        },
        before_path: Some(path_for_node(before_node)),
        after_path: None,
    });

    Ok(format!(
        "Marked '{}' deleted{}",
        before_node.kind(),
        if with_children {
            " (with children)"
        } else {
            ""
        }
    ))
}

pub(crate) fn action_insert(
    mapping: &mut HumanMapping,
    after_flat: &FlatIndex,
    after_cursor: usize,
    before_root: Node,
    after_root: Node,
    with_children: bool,
    caches: &Caches,
) -> Result<String> {
    let after_node = after_flat
        .node_for_id(after_cursor)
        .context("After cursor node not found")?;

    if is_inherited_removed(after_node, &caches.after_removed) {
        bail!(
            "Node is already covered by an ancestor's insert-with-children mark; clear that first (u on the ancestor)"
        );
    }

    remove_direct_entries_for(
        &mut mapping.entries,
        None,
        Some(after_node.id()),
        before_root,
        after_root,
    );
    if with_children {
        clear_after_descendants(&mut mapping.entries, after_node, after_root);
    }
    mapping.entries.push(HumanMappingEntry {
        operation: if with_children {
            HumanOperation::InsertWithChildren
        } else {
            HumanOperation::Insert
        },
        before_path: None,
        after_path: Some(path_for_node(after_node)),
    });

    Ok(format!(
        "Marked '{}' inserted{}",
        after_node.kind(),
        if with_children {
            " (with children)"
        } else {
            ""
        }
    ))
}

// A params struct would only relocate these fields.
#[allow(clippy::too_many_arguments)]
pub(crate) fn action_unmark(
    mapping: &mut HumanMapping,
    focus: Focus,
    before_flat: &FlatIndex,
    after_flat: &FlatIndex,
    before_cursor: usize,
    after_cursor: usize,
    before_root: Node,
    after_root: Node,
    caches: &Caches,
) -> Result<String> {
    let (id, node, removed, group) = match focus {
        Focus::Before => (
            before_cursor,
            before_flat
                .node_for_id(before_cursor)
                .context("Before cursor node not found")?,
            &caches.before_removed,
            caches.before_group.get(&before_cursor).copied(),
        ),
        Focus::After => (
            after_cursor,
            after_flat
                .node_for_id(after_cursor)
                .context("After cursor node not found")?,
            &caches.after_removed,
            caches.after_group.get(&after_cursor).copied(),
        ),
    };

    // A group member has no entry of its own, so `u` removes its whole group.
    if let Some(group_idx) = group
        && group_idx < mapping.groups.len()
    {
        let removed_group = mapping.groups.remove(group_idx);
        return Ok(format!(
            "Removed {} group ({} before, {} after node(s))",
            group_pairing_name(removed_group.pairing),
            removed_group.before_paths.len(),
            removed_group.after_paths.len()
        ));
    }

    let before_id = if focus == Focus::Before {
        Some(id)
    } else {
        None
    };
    let after_id = if focus == Focus::After {
        Some(id)
    } else {
        None
    };

    let before_len = mapping.entries.len();
    remove_direct_entries_for(
        &mut mapping.entries,
        before_id,
        after_id,
        before_root,
        after_root,
    );

    if mapping.entries.len() < before_len {
        return Ok(format!("Unmarked '{}'", node.kind()));
    }

    if is_inherited_removed(node, removed) {
        bail!(
            "This node is only covered via an ancestor's with-children mark; clear the ancestor instead"
        );
    }

    Ok(format!("'{}' was not marked", node.kind()))
}
