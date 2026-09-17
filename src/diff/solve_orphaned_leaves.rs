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
use crate::diff::{ASTDiff, ASTMapping, ASTMappingOperation, ASTMappingReason};
use std::collections::HashMap;

/**
* **A leaf deleted on one side and inserted on the other, under a parent pair everything else
* already agrees on, and reading the same text.**
*
* The corpus-wide mismatch census (2026-09-17,
* `research/data/quality/mismatch_census.csv`) put "the human pairs these two nodes and we drop
* them both" at 39% of all visible mismatches, and found **47 fixtures whose *every* visible
* mismatch is that shape** - a `,` here, a `.` there, a comment, an identifier. They come out of
* four different residual paths (`fast_fallback`, `qualified_name`, `large_flat_subtree` and its
* container variant), so the fix does not belong in any one of them: by the time they are wrong,
* the mapping already says everything needed to see it.
*
* `TODO.md` has carried this as an open item under `fast_fallback` since 2026-08-20 - "pair
* leftover trivial entries among themselves ... after the substantial ones are paired" - and names
* the reason it was never just done: matching a `;` to "random other `;` in the code" is exactly
* how this kind of pass goes wrong.
*
* So the rule keeps the two guards every neighbouring mechanism uses, and adds nothing else:
*
* * **The parents are already a matched pair.** Not "somewhere in the file" - the two leaves are
*   direct children of nodes the pipeline already decided correspond. That is the same anchor
*   [`crate::diff::solve_unique_type_matching`] requires, and this pass is that one's sibling:
*   it keys on kind *and text* where that keys on kind alone, and runs after the terminal fallback
*   rather than before it, so it sees the parent pairs the fallback itself produced.
* * **No choice is being made.** A (kind, text) key is paired only when the two sides have the
*   *same number* of leftovers under that pair - then any bijection between them is the same
*   mapping, because the text is identical, and pairing in document order just picks the readable
*   one. An unequal count means somebody would have to guess which leaf survived, and this pass
*   does not guess: it leaves every one of them alone.
* * **The neighbours correspond too.** Same parent pair and same text is still not enough, and
*   `csharp-lidarr-call-different-function` is why: an `argument_list` whose arguments changed has
*   a `,` on each side that this rule would happily pair, while the human deletes the old one and
*   inserts the new one, because the argument it punctuates is gone. So every existing immediate
*   sibling of the leaf must itself be matched to the corresponding sibling of the candidate. A
*   comma between the same two things is the same comma; a comma between different things is not,
*   whatever it reads.
*
* Pairing identical text is also strictly cheaper under `cost::operation_cost` (an `Identical` pair
* costs 0 where a delete plus an insert costs `COST_DELETE + COST_INSERT`), so this only ever moves
* codediff's own objective in the direction the human mapping already sits.
*/
/// Whether `before_id` and `after_id` sit between the same things: every immediate sibling either
/// side of them that exists on both sides must already be matched to the other's. A leaf with no
/// siblings at all trivially passes - there is nothing that could disagree.
fn neighbours_correspond(
    before_id: usize,
    after_id: usize,
    before_siblings: &[usize],
    after_siblings: &[usize],
    diff: &ASTDiff,
) -> bool {
    let (Some(before_index), Some(after_index)) = (
        before_siblings.iter().position(|&id| id == before_id),
        after_siblings.iter().position(|&id| id == after_id),
    ) else {
        return false;
    };
    // Rejects only on *positive* evidence of disagreement: a neighbour that is matched, to
    // something other than the candidate's neighbour. A neighbour nothing has matched yet says
    // nothing either way - and holding that against the pair is what made the first cut of this
    // guard useless, because `css-shadcn-ui-ui-completely-broken-treesitter-parsing`'s leaves sit
    // in a region whose parse is broken enough that almost nothing around them is matched.
    let agrees = |before: Option<&usize>, after: Option<&usize>| match (before, after) {
        (Some(&before), Some(&after)) => match (
            diff.before_node_map.get(&before),
            diff.after_node_map.get(&after),
        ) {
            (Some(&partner), _) if partner != 0 => partner == after,
            (_, Some(&partner)) if partner != 0 => partner == before,
            _ => true,
        },
        (None, None) => true,
        // One side has a neighbour here and the other does not: the leaf is not between the same
        // things, so this is exactly the case the guard exists for.
        _ => false,
    };
    let previous = agrees(
        before_index
            .checked_sub(1)
            .and_then(|i| before_siblings.get(i)),
        after_index
            .checked_sub(1)
            .and_then(|i| after_siblings.get(i)),
    );
    let next = agrees(
        before_siblings.get(before_index + 1),
        after_siblings.get(after_index + 1),
    );
    previous && next
}

pub fn solve(ctx: &PassCtx, diff: &mut ASTDiff) {
    let before_metadata = ctx.before_metadata();
    let after_metadata = ctx.after_metadata();

    // Matched pairs, in before-side document order so the pass is deterministic regardless of the
    // hash maps' own iteration order - the same reason `solve_unique_type_matching` sorts.
    let mut pairs: Vec<(usize, usize)> = diff
        .before_node_map
        .iter()
        .filter_map(|(&before_id, &after_id)| (after_id != 0).then_some((before_id, after_id)))
        // Only a pair with a dropped child can produce anything here, and on a large, mostly
        // unchanged file almost none do - the same filter, for the same reason,
        // `solve_unique_type_matching` documents at length.
        .filter(|(before_id, _)| {
            before_metadata
                .node_info
                .get(before_id)
                .is_some_and(|info| {
                    info.children
                        .iter()
                        .any(|child| diff.before_node_map.get(child) == Some(&0))
                })
        })
        .collect();
    pairs.sort_unstable_by_key(|&(before_id, _)| {
        before_metadata
            .node_info
            .get(&before_id)
            .map(|info| info.preorder_index)
            .unwrap_or(usize::MAX)
    });

    for (before_parent, after_parent) in pairs {
        let (Some(before_info), Some(after_info)) = (
            before_metadata.node_info.get(&before_parent),
            after_metadata.node_info.get(&after_parent),
        ) else {
            continue;
        };

        // Children this side dropped: a leaf whose whole mapping is "gone" (`-> 0`).
        let orphans = |ids: &[usize],
                       metadata: &crate::code::ASTMetadata,
                       map: &rustc_hash::FxHashMap<usize, usize>|
         -> HashMap<(String, String), Vec<usize>> {
            let mut out: HashMap<(String, String), Vec<usize>> = HashMap::new();
            for &id in ids {
                if map.get(&id) != Some(&0) {
                    continue;
                }
                let Some(info) = metadata.node_info.get(&id) else {
                    continue;
                };
                if !info.children.is_empty() {
                    continue;
                }
                out.entry((info.kind.clone(), info.text.clone()))
                    .or_default()
                    .push(id);
            }
            for ids in out.values_mut() {
                ids.sort_unstable_by_key(|id| {
                    metadata
                        .node_info
                        .get(id)
                        .map(|info| info.start_byte)
                        .unwrap_or(usize::MAX)
                });
            }
            out
        };

        let before_orphans = orphans(
            &before_info.children,
            before_metadata,
            &diff.before_node_map,
        );
        if before_orphans.is_empty() {
            continue;
        }
        let after_orphans = orphans(&after_info.children, after_metadata, &diff.after_node_map);
        if after_orphans.is_empty() {
            continue;
        }

        let mut keys: Vec<&(String, String)> = before_orphans.keys().collect();
        keys.sort();
        for key in keys {
            let before_ids = &before_orphans[key];
            let Some(after_ids) = after_orphans.get(key) else {
                continue;
            };
            // Unequal counts mean choosing which leaf survived. See this module's doc comment.
            if before_ids.len() != after_ids.len() {
                continue;
            }
            for (&before_id, &after_id) in before_ids.iter().zip(after_ids.iter()) {
                if !neighbours_correspond(
                    before_id,
                    after_id,
                    &before_info.children,
                    &after_info.children,
                    diff,
                ) {
                    continue;
                }
                diff.remove_delete_mapping(before_id);
                diff.remove_insert_mapping(after_id);
                diff.add_mapping(
                    before_id,
                    after_id,
                    ASTMapping {
                        cost: 0,
                        operation: ASTMappingOperation::Identical,
                        reason: ASTMappingReason::OrphanedLeafUnderMatchedParent,
                    },
                );
            }
        }
    }
}
