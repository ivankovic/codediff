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

//! MoveDetectionRecovery: the final pass of the pipeline, running after the tree-edit-distance
//! step has fully decided the diff. Scans what got wholly deleted and wholly inserted and pairs
//! up byte-identical subtrees between the two sets - code that didn't change but *moved*, which
//! plain ordered tree edit distance structurally cannot express as anything but delete+insert
//! whenever the move crosses a matched boundary (the classic case: a subtree relocating into a
//! newly-inserted wrapper, or into a container whose own identity changed - see the
//! rust-turbopack-module-rule analysis in TODO.md).
//!
//! This is GumTree's "recovery mappings" phase adapted to this pipeline. It deliberately runs
//! dead last: every earlier pass (including the DP) gets first claim on all content, so this can
//! only ever convert delete+insert leftovers into matches - it can never take a node away from a
//! better mapping.
//!
//! Guardrails, each protecting against a previously-observed failure mode:
//!
//! - Only *fully*-deleted subtrees may pair with *fully*-inserted ones. A subtree with even one
//!   matched descendant already has a footprint in the other tree; re-mapping it here could
//!   contradict that footprint's ancestry.
//! - Only subtrees of at least `MIN_MOVE_SUBTREE_SIZE` nodes participate. Hash-matching arbitrary
//!   small nodes re-creates the "stray `;` matches a random other `;`" disease this project spent
//!   the generic-token gate curing; small identical statements (`return None`, `i += 1`) are so
//!   common that pairing them across unrelated regions is noise, not signal.
//! - Largest-first with claiming: a moved function claims its whole subtree in one piece, rather
//!   than its statements each finding their own (possibly different) partners.
//! - Container-identity agreement: the outermost deleted reference-node ancestor of the source
//!   and the outermost inserted reference-node ancestor of the target must have the same kind.
//!   Humans read a move relative to the construct it left and the construct it entered: content
//!   relocating from a deleted `impl` into an inserted (renamed) `impl` is "the same impl,
//!   reshaped, contents moved" (rust-turbopack-module-rule's ground truth maps it), but an
//!   expression re-appearing inside a brand-new construct of a *different* kind - a free
//!   function's `width * height` re-surfacing inside a new `data class`'s method - reads as new
//!   code that happens to spell the same (kotlin-refactor-function's ground truth deletes it).

use std::collections::HashSet;

use crate::code::{ASTMetadata, Code, Language};
use crate::diff::nodes::is_reference;
use crate::diff::{ASTDiff, ASTMapping, ASTMappingOperation, ASTMappingReason, NodeCache};

/// Minimum subtree size (node count, incl. the root) for a delete/insert pair to be considered a
/// move. Below this, identical subtrees are commodity code (single tokens, trivial statements)
/// whose pairing is coincidence more often than intent.
const MIN_MOVE_SUBTREE_SIZE: usize = 4;

pub fn solve(before: &Code, after: &Code, node_cache: &NodeCache, diff: &mut ASTDiff) {
    let language = before.metadata.language.unwrap_or_default();
    let before_metadata = crate::code::metadata::metadata_of(before);
    let after_metadata = crate::code::metadata::metadata_of(after);
    let before_parents = &before_metadata.node_to_parent;
    let after_parents = &after_metadata.node_to_parent;

    // Every candidate on the deleted side, largest subtree first, so a moved container is
    // re-mapped as one piece before its own children are considered separately. Ties broken by
    // `start_byte` (document position), not `node_id`: `diff.before_node_map` is a `HashMap`, so
    // the source order here is already hash-seeded, and node ids are arena slots that aren't
    // stable across separate parses of identical source - only a source-position tiebreak keeps
    // the result reproducible across process runs, not just within one.
    let mut deleted: Vec<(usize, usize, usize)> = diff
        .before_node_map
        .iter()
        .filter(|&(_, &target)| target == 0)
        .filter_map(|(&b, _)| {
            let size = before_metadata.node_to_subtree_size.get(&b).copied()?;
            let start_byte = before_metadata.node_info.get(&b)?.start_byte;
            (size >= MIN_MOVE_SUBTREE_SIZE).then_some((size, start_byte, b))
        })
        .collect();
    deleted.sort_unstable_by(|x, y| y.0.cmp(&x.0).then(x.1.cmp(&y.1)));

    let mut claimed_before: HashSet<usize> = HashSet::new();
    let mut claimed_after: HashSet<usize> = HashSet::new();

    for (_, _, b) in deleted {
        if claimed_before.contains(&b) {
            continue;
        }
        if !subtree_fully_unmapped(b, &before_metadata, &diff.before_node_map) {
            continue;
        }
        let Some(hash) = before_metadata.node_to_full_hash.get(&b) else {
            continue;
        };
        let Some(candidates) = after_metadata.full_hash_to_node.get(hash) else {
            continue;
        };

        // `full_hash_to_node`'s candidates already arrive in a deterministic order (see its doc
        // comment), but not a document-position one - re-order by document position so that, among
        // several equally-valid move targets, the earliest one in the file wins, which is what a
        // human skimming the diff would expect.
        let mut candidates: Vec<usize> = candidates
            .iter()
            .copied()
            .filter(|a| {
                !claimed_after.contains(a)
                    && diff.after_node_map.get(a) == Some(&0)
                    && subtree_fully_unmapped(*a, &after_metadata, &diff.after_node_map)
            })
            .collect();
        candidates.sort_unstable_by_key(|a| {
            node_cache
                .after
                .get(a)
                .map(|n| n.start_byte())
                .unwrap_or(usize::MAX)
        });
        let source_container = outermost_unmapped_reference_kind(
            b,
            &before_metadata,
            before_parents,
            &diff.before_node_map,
            &language,
        );
        let Some(&a) = candidates.iter().find(|&&a| {
            let target_container = outermost_unmapped_reference_kind(
                a,
                &after_metadata,
                after_parents,
                &diff.after_node_map,
                &language,
            );
            source_container == target_container
        }) else {
            continue;
        };

        remap_moved_subtree(b, a, &before_metadata, &after_metadata, diff);
        claim_subtree(b, &before_metadata, &mut claimed_before);
        claim_subtree(a, &after_metadata, &mut claimed_after);
    }
}

/// The kind of the outermost reference node (see `is_reference`) on `node`'s still-unmapped
/// ancestor chain, including `node` itself - the construct a human perceives the deleted/inserted
/// content as belonging to. The climb stops at the first mapped ancestor (walking into matched
/// territory would attribute the content to a container that visibly survived). `None` when no
/// reference node is on the unmapped chain (content moving within matched surroundings).
fn outermost_unmapped_reference_kind<'m>(
    node: usize,
    meta: &'m ASTMetadata,
    parents: &rustc_hash::FxHashMap<usize, usize>,
    node_map: &rustc_hash::FxHashMap<usize, usize>,
    language: &Language,
) -> Option<&'m str> {
    let mut outermost = None;
    let mut cur = node;
    loop {
        if node_map.get(&cur).copied().unwrap_or(1) != 0 && cur != node {
            break;
        }
        if let Some(info) = meta.node_info.get(&cur)
            && is_reference(&info.kind, language)
        {
            outermost = Some(info.kind.as_str());
        }
        match parents.get(&cur) {
            Some(&p) => cur = p,
            None => break,
        }
    }
    outermost
}

/// True if every node in `root`'s subtree is mapped to 0 - i.e. nothing inside was claimed by any
/// earlier pass, so the whole subtree is free to be re-mapped as one moved unit.
fn subtree_fully_unmapped(
    root: usize,
    meta: &ASTMetadata,
    node_map: &rustc_hash::FxHashMap<usize, usize>,
) -> bool {
    if node_map.get(&root) != Some(&0) {
        return false;
    }
    let Some(info) = meta.node_info.get(&root) else {
        return true;
    };
    info.children
        .iter()
        .all(|&child| subtree_fully_unmapped(child, meta, node_map))
}

fn claim_subtree(root: usize, meta: &ASTMetadata, claimed: &mut HashSet<usize>) {
    claimed.insert(root);
    if let Some(info) = meta.node_info.get(&root) {
        for &child in &info.children {
            claim_subtree(child, meta, claimed);
        }
    }
}

/// Replaces the delete/insert mappings of two identical subtrees with pairwise matches, walking
/// both in lockstep (identical full hashes guarantee identical shape, so children line up 1:1).
/// Every pair is `Identical` - the content is byte-for-byte the same; only its location changed -
/// with the `MovedSubtree` reason marking how the pair was found.
fn remap_moved_subtree(
    b: usize,
    a: usize,
    before_meta: &ASTMetadata,
    after_meta: &ASTMetadata,
    diff: &mut ASTDiff,
) {
    diff.remove_delete_mapping(b);
    diff.remove_insert_mapping(a);
    diff.add_mapping(
        b,
        a,
        ASTMapping {
            cost: 0,
            operation: ASTMappingOperation::Identical,
            reason: ASTMappingReason::MovedSubtree,
        },
    );

    let b_children = before_meta
        .node_info
        .get(&b)
        .map(|i| i.children.clone())
        .unwrap_or_default();
    let a_children = after_meta
        .node_info
        .get(&a)
        .map(|i| i.children.clone())
        .unwrap_or_default();
    for (cb, ca) in b_children.into_iter().zip(a_children) {
        remap_moved_subtree(cb, ca, before_meta, after_meta, diff);
    }
}

#[cfg(test)]
mod tests {
    use crate::code::{Code, Language};
    use crate::diff::diff_code;

    /// A whole function moving across another (unchanged) function must come out as matched
    /// content, not a delete+insert pair.
    #[test]
    fn moved_function_is_matched_not_deleted() {
        let before = Code::from_string(
            "fn moved_one(x: i64, y: i64) -> i64 { let q = x * y; q + x }\nfn stay() {}\n",
            &Language::Rust,
        );
        let after = Code::from_string(
            "fn stay() {}\nfn moved_one(x: i64, y: i64) -> i64 { let q = x * y; q + x }\n",
            &Language::Rust,
        );

        let diff = diff_code(&before, &after);
        let ast = diff.ast.unwrap();

        // No node on either side may remain deleted/inserted: both functions exist on both sides.
        let deleted: Vec<_> = ast
            .before_node_map
            .iter()
            .filter(|&(_, &t)| t == 0)
            .collect();
        let inserted: Vec<_> = ast
            .after_node_map
            .iter()
            .filter(|&(_, &t)| t == 0)
            .collect();
        assert!(
            deleted.is_empty() && inserted.is_empty(),
            "moved function should be fully matched, found {} deletes / {} inserts",
            deleted.len(),
            inserted.len()
        );
    }

    /// Two tiny identical statements in unrelated functions must NOT be "moved" onto each other -
    /// the size floor keeps commodity code out of move detection.
    #[test]
    fn tiny_identical_statements_do_not_move() {
        let before = Code::from_string(
            "fn a() { let x = 1; }\nfn c() {}\n",
            &Language::Rust,
        );
        let after = Code::from_string(
            "fn c() {}\nfn d() { let x = 1; }\n",
            &Language::Rust,
        );

        let diff = diff_code(&before, &after);
        let ast = diff.ast.unwrap();

        // `fn a` was deleted and `fn d` inserted; their bodies share `let x = 1;` but the
        // functions are different. The statement is small commodity code: it must round-trip as
        // delete+insert with its function, not "move" between two unrelated functions...
        // unless a legitimate larger container above it was matched (which it isn't here: the
        // names differ, so the name-keyed pass orphans both).
        let has_move = ast
            .mapping
            .values()
            .any(|m| m.reason == crate::diff::ASTMappingReason::MovedSubtree);
        assert!(
            !has_move,
            "tiny identical statements must not be paired as moves"
        );
    }
}
