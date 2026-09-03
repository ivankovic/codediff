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
//!
//! Split out of `main.rs` along the section banner that already marked this boundary.

#[allow(unused_imports)]
use crate::*;

// ---------------------------------------------------------------------------------------------
// Marking actions
// ---------------------------------------------------------------------------------------------

/// Removes any existing entry whose before_path resolves to `before_id`, or whose after_path
/// resolves to `after_id`, so that re-marking a node cleanly replaces its previous decision
/// instead of leaving stale/contradictory entries behind.
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

/// Like [`remove_direct_entries_for`], but removes every entry touching *any* id in `before_ids`/
/// `after_ids` in one pass, instead of one id at a time. Used by `apply_modal_choice` to batch-clear
/// a whole subtree's worth of potential conflicts before `auto_match_pair` recurses into it and
/// appends entries directly -- doing this per-node instead (i.e. calling `remove_direct_entries_for`
/// once per node like `apply_match_entry` does) is what made `M` quadratic over a big subtree.
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

/// Finds a node anywhere in `root`'s subtree by id, unlike [`find_node_by_id`], which only looks
/// among the (possibly collapsed/hidden) visible rows a `flat` slice covers. Used only when
/// resolving a multi-map selection at commit time: a node toggled into `App::before_multi_select`/
/// `after_multi_select` with `x` can end up hidden by a later `Left`/`H` press on an ancestor
/// before `m`/`M` commits the group, and it must still resolve correctly then.
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

/// Removes any existing [`MultiMapGroup`] that shares a node (on either side) with `before_ids`/
/// `after_ids` - the group-level counterpart of [`remove_entries_touching`], used so committing a
/// new multi-map selection can't leave a stale group half-referencing a node that just got
/// reassigned to a different group or a plain match.
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

/// Drops any entry whose before_path resolves to a strict descendant of `ancestor`. Used when
/// marking `ancestor` deleted-with-children, so a previous direct mark on one of its descendants
/// (e.g. a Match) can't survive alongside it and export a self-contradictory mapping.
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

/// Same as [`clear_before_descendants`], but for the After tree (used by insert-with-children).
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

/// What an `m`/`M` press should do next: either it's fully resolved (a mapping entry was added,
/// or nothing needed to change), or it needs a direct human answer before anything is written.
pub(crate) enum ActionOutcome {
    Done(String),
    /// Boxed: `Modal`'s largest variant is much bigger than `Done`'s `String`, so an unboxed
    /// enum would pay that size on every `ActionOutcome` returned. One indirection on a
    /// keystroke-rate path is free; the size difference is what clippy flags.
    NeedsModal(Box<Modal>),
}

/// True if `b` and `a` have the exact same text.
pub(crate) fn node_values_equal(b: Node, a: Node, before_src: &[u8], after_src: &[u8]) -> bool {
    b.utf8_text(before_src).unwrap_or("") == a.utf8_text(after_src).unwrap_or("")
}

/// Replaces any existing direct entry touching `b` or `a` with a single new entry pairing them
/// under `operation`.
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

/// Classifies a same-kind pair with children as `Identical` or `MatchButNotIdentical` without
/// asking: `before_hash`/`after_hash` are each node's precomputed full-content hash (kind + text +
/// children's hashes, folded bottom-up -- see `code::hash::hash_code`), so two subtrees hash equal
/// iff they're byte-identical. Missing hashes (shouldn't happen once `load_case` has run
/// `ensure_parsed`) are treated conservatively as not identical.
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

/// Classifies a multi-map selection's operation the same way [`subtree_match_operation`]
/// classifies a single pair - by full-content hash - generalized to a set: `Identical` only if
/// *every* selected node, on both sides, shares the exact same hash (the whole selection really is
/// N interchangeable copies of one subtree), `MatchButNotIdentical` otherwise. Never `Update` -
/// see `MultiMapGroup::operation`'s own doc comment for why a group has no single fixed pair to
/// call a text edit against.
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

/// Commits `before_ids`/`after_ids` (a confirmed multi-map selection - see `App::before_multi_select`/
/// `after_multi_select`) as a new [`MultiMapGroup`], clearing out any prior plain entry or group
/// that touched one of these nodes first (the group-level equivalent of [`apply_match_entry`]'s own
/// "replace whatever was there" behavior for a single pair).
pub(crate) fn commit_multi_map_group(
    mapping: &mut HumanMapping,
    before_root: Node,
    after_root: Node,
    before_ids: &std::collections::BTreeSet<usize>,
    after_ids: &std::collections::BTreeSet<usize>,
    operation: HumanOperation,
    with_children: bool,
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
    // Deterministic, parse-stable order - not the arena-id order iterating a `BTreeSet<usize>`
    // would otherwise produce (see the project's benchmark-determinism-fix lesson on node ids as
    // ordering keys), and the same order `representative_entries` sorts its own pairing by.
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
        // A member's descendants must stay free to close over whichever specific pair codediff's
        // own diff actually realizes (see `check_subtree_maps_within`) - a leftover pre-existing
        // entry on one would otherwise pin a pairing the group deliberately leaves open, or flatly
        // contradict a leftover member's "this whole subtree must be removed" requirement. Same
        // "with-children can't coexist with a descendant mark" invariant `d`/`i`'s own
        // `clear_before_descendants`/`clear_after_descendants` calls already enforce.
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
    });

    Ok(format!(
        "Committed multi-map group: {} before, {} after node(s), {:?}{}",
        before_count,
        after_count,
        operation,
        if with_children { " with children" } else { "" }
    ))
}

/// What `m`/`M` does when the multi-map selection (`App::before_multi_select`/`after_multi_select`)
/// is non-empty: infers the group's operation (see [`multi_map_group_operation`]), then either
/// commits it directly (every selected node shares one AST kind) or raises
/// `Modal::ConfirmMultiMapGroup` first - the group-level counterpart of [`kind_mismatch_modal`].
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
) -> Result<ActionOutcome> {
    if before_ids.is_empty() || after_ids.is_empty() {
        bail!(
            "Multi-map group needs at least one selected node on both sides (x to select, c to clear)"
        );
    }

    // Same precondition `action_match`/`action_match_subtree` enforce for a single pair: a member
    // sitting under an ancestor already marked deleted/inserted-with-children can't also be
    // committed into a group without producing a self-contradictory mapping.
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
    )?;
    Ok(ActionOutcome::Done(msg))
}

/// Builds the `NeedsModal` outcome for a before/after cursor pair whose kinds don't match -
/// shared by every action that requires same-kind nodes before it can proceed. `recursive`
/// distinguishes a single-node match attempt (`m`, `false`) from a whole-subtree one (`M`, `true`)
/// - see [`Modal::ConfirmKindMismatch`].
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

/// Classifies a same-kind cursor pair as it would be auto-classified by a single `m` press:
/// `Identical`/`Update` by raw text for a leaf pair, or via [`subtree_match_operation`] (content
/// hash) for a pair with children. Shared by [`action_match`] and [`action_match_to_end`], which
/// both just need the resulting operation before continuing their own, differing follow-up logic
/// - unlike [`action_match_subtree`]'s leaf case, which resolves and returns immediately instead
///   of continuing, so it classifies its own leaf pairs inline rather than sharing this helper.
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

/// Implements `f`: repeats exactly what a single `m` press does -- match the Before and After
/// cursor nodes (auto-classified Identical/Update for leaves, or by content hash for nodes with
/// children, precisely like [`action_match`]), then advance both cursors to their own next
/// `Unmarked` node -- over and over, as if `m` were being pressed by hand again and again.
///
/// Stops in exactly the two places a human doing that would have to stop too: once neither cursor
/// has an `Unmarked` node left to advance to (there's nothing left to pair up -- end of file), or
/// the moment the next pair has different kinds, which is precisely when a real `m` press would
/// raise [`Modal::ConfirmKindMismatch`] instead of matching outright. Every match applied before
/// that point is kept; pressing `f` again after resolving the mismatch (or manually) resumes the
/// sweep from the new cursor position.
///
/// Deliberately doesn't go through [`apply_match_entry`]/[`rebuild_caches`]/[`path_for_node`] on
/// every single node the way a literal "call `action_match` in a loop" implementation would:
/// those are built for a human's pace (one call per keypress, cost spread over real time), and
/// each costs O(current entry count) or O(sibling count) -- fine for a single `m` press, but this
/// loop can run once per AST node, so paying that on every iteration turns an O(n) sweep into
/// O(n^2). Two real fixtures exposed this: a ~5,500-node real-world file took 26s to reach 740
/// matches and climbing (`rebuild_caches`/`apply_match_entry`'s O(entries)-per-call cost), and a
/// large flat JSON-array-shaped tree took 52s even after that fix (`path_for_node`'s O(siblings)
/// occurrence-counting, paid per node, on a level with thousands of same-kind children). Both are
/// worked around here: `caches` is built once and updated incrementally in place; entries are
/// appended directly, skipping `apply_match_entry`'s dedup scan (provably a no-op here, since
/// `status_before`/`status_after` having just reported `Unmarked` means neither node has an
/// existing entry to remove); cursors are tracked as plain indices into `before_flat`/`after_flat`
/// so advancing never re-scans from the start; and every node's path is looked up in a table
/// built by one O(n) pass per tree ([`precompute_paths`]) instead of walked fresh from each node.
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

        // Nothing left to pair up on at least one side: reached the end of what `f` can do. (This
        // also covers a node under an inherited delete/insert-with-children mark, since
        // `status_before`/`status_after` never report those as `Unmarked`.)
        if status_before(before_node, &caches) != NodeStatus::Unmarked
            || status_after(after_node, &caches) != NodeStatus::Unmarked
        {
            break;
        }

        if before_node.kind() != after_node.kind() {
            app.before.cursor_id = before_node.id();
            app.after.cursor_id = after_node.id();
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

/// The first index at or after `start` whose node is `Unmarked`, or `None` if there isn't one.
/// Callers that advance `start` monotonically across repeated calls (as `action_match_to_end`
/// does) get amortized O(n) total work rather than O(n) *per call* -- each slot in `flat` is only
/// ever examined once across the whole sweep.
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

    // A leaf top pair has no children to auto-fill, so resolve it immediately like `m` does.
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

/// Auto-matches `b` <-> `a` and all descendants, with no prompting: leaves are classified
/// Identical/Update by comparing text; container nodes are classified Identical only if every
/// descendant came back Identical too, otherwise MatchButNotIdentical. Recursion stops (without
/// matching further) the moment a level's child-kind sequences diverge, or a node is already
/// covered by an unrelated ancestor mark. Returns whether the whole subtree matched Identically.
///
/// Used to bulk-fill the rest of an `M` (recursive match) after the top-level pair's own operation
/// has already been decided (via `subtree_match_operation` or a confirmed `Modal::ConfirmKindMismatch`)
/// -- classifying each descendant individually by hash keeps this fast for a tree of any size.
///
/// Any pair (this one or a descendant) that ends up classified `Identical` and has children is
/// also collapsed in both panels, so a whole-unchanged subtree doesn't clutter the view -- this is
/// the main payoff of `M` over doing the same matches one at a time with `m`.
///
/// Rather than pushing straight into `mapping.entries` (via `apply_match_entry`, which costs
/// O(current entry count) per call through its dedup scan -- recursing over a subtree of size k
/// would turn that into O(k^2), exactly the hang `action_match_to_end` (`f`) had before it was
/// fixed the same way, and `M` hits it too since this function is its recursive workhorse), this
/// buffers new entries into `new_entries` and records every node id it actually decides on into
/// `touched_before`/`touched_after`. The caller ([`apply_modal_choice`]) removes pre-existing
/// entries for exactly those touched ids in one batch pass *after* recursion finishes, then
/// appends `new_entries` -- cheaper than a scan per node, and correct in a way that eagerly
/// collecting a subtree's ids up front isn't: recursion can bail out of a node early (kind
/// mismatch or a shape mismatch) without visiting its descendants at all, and a descendant that
/// was never visited must keep whatever pre-existing entry it had, not have it wiped because it
/// happened to be nested under the node `M` was pressed on.
///
/// Paths are looked up from `before_paths`/`after_paths` (each precomputed once, in
/// `apply_modal_choice`, by [`precompute_paths`]) instead of calling `path_for_node` fresh per
/// node, for the same reason.
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
        // Shouldn't happen for children reached via the same_shape check below, but the very
        // first call into this function (the top pair's children) hasn't been shape-checked yet.
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

/// Applies `operation` to the top pair -- whether decided by `subtree_match_operation` (`M`) or a
/// confirmed `Modal::ConfirmKindMismatch` -- and if `recursive`, auto-fills the rest of the subtree
/// via [`auto_match_pair`].
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

        // Batched equivalent of what `apply_match_entry`'s per-node dedup scan would otherwise do
        // node by node inside `auto_match_pair` (an O(existing entries) scan for every node in the
        // subtree, which goes quadratic over a big one -- see `auto_match_pair`'s doc comment):
        // clear out, in one pass, any pre-existing entry that touches a node the recursion above
        // actually decided on, *then* append what it produced. Using the ids `auto_match_pair`
        // actually touched (rather than every id in the subtree) matters: a node the recursion
        // bailed out of without visiting keeps whatever pre-existing entry it had.
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

// Each parameter is genuinely distinct context (the mapping, focus, both sides' flattened node
// lists, both cursors, both roots, the caches) - a params struct here would just relocate the
// same fields, not reduce them.
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

    // A group member (whichever specific pair `representative_entries` realized, or a leftover)
    // isn't recorded as its own `mapping.entries` item at all - the whole group is one
    // `MultiMapGroup` in `mapping.groups` - so `u` here removes that entire group rather than
    // trying (and failing) to find a single direct entry to drop.
    if let Some(group_idx) = group
        && group_idx < mapping.groups.len()
    {
        let removed_group = mapping.groups.remove(group_idx);
        return Ok(format!(
            "Removed multi-map group ({} before, {} after node(s))",
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
