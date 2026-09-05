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
use crate::code::ASTMetadata;
use crate::diff::PassCtx;
use crate::diff::nodes::kinds_update_allowed;
use crate::diff::{ASTDiff, ASTMapping, ASTMappingReason};
use rustc_hash::FxHashMap;

/// Match an unmatched container to its counterpart when each side is the lowest common ancestor of
/// the other side's matched content.
///
/// `solve_bottom_up_propagation` deliberately refuses to guess when a node's children matched into
/// *different* after-parents - "no plurality vote". That is the right call for a genuine
/// ambiguity, but it also declines the least ambiguous case there is: an ancestor sitting above an
/// edit, whose descendants all still live inside one corresponding ancestor on the other side. A
/// whole chain of such ancestors goes unclaimed, and (since 365ea5b) the terminal sweep then
/// reports each one deleted and its counterpart inserted. `html-mozilla-firefox-firefox-remove-li-
/// around-button` shows the shape plainly: removing a `<li>` wrapper from deep inside the document
/// left every `element` ancestor above it - with *identical* paths on both sides - marked deleted.
///
/// The rule here needs no threshold and no vote because it is symmetric. Define, for a before-node
/// `B`, `lca_after(B)` as the lowest common ancestor of every after-node that `B`'s matched
/// descendants map to, and symmetrically `lca_before(A)`. Then `B` and `A` are paired only when
/// **each is the other's LCA**: `lca_after(B) == A` *and* `lca_before(A) == B`. One direction alone
/// would happily match a small container to a huge unrelated one that merely happens to contain the
/// same matched content somewhere; requiring both directions means the two containers hold *the
/// same* content and nothing else that is spoken for. Mutuality also makes the pairing unique by
/// construction - two different `B`s can share an `lca_after`, but only one of them can be that
/// node's own `lca_before` - so there is no claiming order to get right.
///
/// Nodes strictly between a paired ancestor and the matched content below it (the removed `<li>`,
/// or a newly added wrapper) are deliberately left alone: they are the actual edit, and the
/// terminal sweep records them as the delete/insert they are.
///
/// Runs after the fallback and both propagation passes - it is a recovery rule for what they leave
/// behind, and the more matched descendants exist when it runs, the better its LCAs are - but
/// before `solve_unresolved_nodes`, which would otherwise have already spent these nodes on
/// delete/insert decisions.
pub fn solve(ctx: &PassCtx, diff: &mut ASTDiff) {
    let (before, after) = (ctx.before, ctx.after);
    let before_metadata = ctx.before_metadata();
    let after_metadata = ctx.after_metadata();
    let (Some(before_ast), Some(after_ast)) = (before.ast.as_ref(), after.ast.as_ref()) else {
        return;
    };

    let lca_after = aggregate_lca(
        before_ast.root_node().id(),
        before_metadata,
        after_metadata,
        &diff.before_node_map,
    );
    let lca_before = aggregate_lca(
        after_ast.root_node().id(),
        after_metadata,
        before_metadata,
        &diff.after_node_map,
    );

    // Sorted by preorder for a deterministic commit order. The mutual condition already makes each
    // pairing independent of the others, so this only fixes the order entries land in `mapping`.
    let mut candidates: Vec<(usize, usize)> = lca_after
        .iter()
        .filter(|(before_id, _)| !diff.before_node_map.contains_key(before_id))
        .filter_map(|(&before_id, &after_id)| {
            (lca_before.get(&after_id) == Some(&before_id)).then_some((before_id, after_id))
        })
        .collect();
    candidates.sort_unstable_by_key(|(before_id, _)| {
        before_metadata
            .node_info
            .get(before_id)
            .map(|info| info.preorder_index)
            .unwrap_or(usize::MAX)
    });

    for (before_id, after_id) in candidates {
        if diff.before_node_map.contains_key(&before_id)
            || diff.after_node_map.contains_key(&after_id)
        {
            continue;
        }
        let (Some(before_info), Some(after_info)) = (
            before_metadata.node_info.get(&before_id),
            after_metadata.node_info.get(&after_id),
        ) else {
            continue;
        };
        // Same kind-compatibility gate `solve_bottom_up_propagation` applies to its own pairings:
        // structural correspondence is evidence about *position*, and position alone shouldn't pair
        // two nodes the cost model would refuse to call an update.
        if !kinds_update_allowed(
            &before_info.kind,
            &after_info.kind,
            &before_metadata.language,
        ) {
            continue;
        }
        // Cost 0 / `MatchButNotIdentical`, for the same reason as `solve_unresolved_nodes`'
        // root pairing: `UnitCostModel::ren` prices a same-kind internal-node pairing at 0 because
        // its children carry their own costs, and everything below here already has its own
        // decision - this pass only ever fires on nodes with matched descendants.
        diff.add_mapping(
            before_id,
            after_id,
            ASTMapping::matched_not_identical(ASTMappingReason::MutualAncestor),
        );
    }
}

/// For every node of `metadata`'s tree, the lowest common ancestor - in the *other* tree - of every
/// partner its matched descendants (or it itself) map to. Absent when a node's subtree contains no
/// matched node at all.
///
/// One iterative postorder walk: a node's aggregate is the LCA of its children's aggregates and its
/// own partner, so the whole tree costs one pass plus the LCA walks, rather than a fresh descendant
/// sweep per node.
fn aggregate_lca(
    root_id: usize,
    metadata: &ASTMetadata,
    other_metadata: &ASTMetadata,
    node_map: &FxHashMap<usize, usize>,
) -> FxHashMap<usize, usize> {
    let mut aggregate: FxHashMap<usize, usize> = FxHashMap::default();
    let mut stack = vec![(root_id, false)];
    while let Some((node_id, processed)) = stack.pop() {
        let Some(info) = metadata.node_info.get(&node_id) else {
            continue;
        };
        if !processed {
            stack.push((node_id, true));
            for &child_id in info.children.iter().rev() {
                stack.push((child_id, false));
            }
            continue;
        }
        // A node's own partner counts too, so a matched node is its own aggregate - that is what
        // lets an unmatched parent one level up see it. `0` is the delete/insert sentinel, never a
        // real partner.
        let mut acc: Option<usize> = node_map.get(&node_id).copied().filter(|&id| id != 0);
        for &child_id in &info.children {
            let Some(&child_acc) = aggregate.get(&child_id) else {
                continue;
            };
            acc = Some(match acc {
                None => child_acc,
                Some(current) => lowest_common_ancestor(current, child_acc, other_metadata),
            });
        }
        if let Some(value) = acc {
            aggregate.insert(node_id, value);
        }
    }
    aggregate
}

/// Classic depth-equalising walk up `node_to_parent`. Returns `a` when either id has no depth
/// recorded, which can only happen for a node that isn't part of this tree - a state no caller
/// here can produce, but the conservative answer keeps this total rather than panicking.
fn lowest_common_ancestor(mut a: usize, mut b: usize, metadata: &ASTMetadata) -> usize {
    let (Some(&depth_a), Some(&depth_b)) = (
        metadata.node_to_depth.get(&a),
        metadata.node_to_depth.get(&b),
    ) else {
        return a;
    };
    let (mut depth_a, mut depth_b) = (depth_a, depth_b);
    while depth_a > depth_b {
        let Some(&parent) = metadata.node_to_parent.get(&a) else {
            return a;
        };
        a = parent;
        depth_a -= 1;
    }
    while depth_b > depth_a {
        let Some(&parent) = metadata.node_to_parent.get(&b) else {
            return b;
        };
        b = parent;
        depth_b -= 1;
    }
    while a != b {
        let (Some(&parent_a), Some(&parent_b)) = (
            metadata.node_to_parent.get(&a),
            metadata.node_to_parent.get(&b),
        ) else {
            return a;
        };
        a = parent_a;
        b = parent_b;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::Code;
    use crate::code::Language;
    use crate::diff::Diff;
    use crate::diff::NodeCache;

    /// Pre-matches exactly the statement pairs named by `statement_pairs` (as `(before_index,
    /// after_index)` into each block's named children) and then runs *only* this pass, so a passing
    /// assertion can only be this pass's own doing. Going through the real pipeline instead would
    /// prove nothing here: APTED reaches its own verdict about these containers and claims them
    /// before this pass ever sees them (an earlier draft of these tests did exactly that and failed
    /// for that reason, not because the rule was wrong).
    fn solve_with_statements_prematched(
        before: &Code,
        after: &Code,
        before_block: tree_sitter::Node,
        after_block: tree_sitter::Node,
        statement_pairs: &[(usize, usize)],
    ) -> ASTDiff {
        let node_cache = NodeCache::build(before, after);
        let mut diff = ASTDiff::default();
        for &(b, a) in statement_pairs {
            diff.add_mapping(
                before_block.named_child(b).unwrap().id(),
                after_block.named_child(a).unwrap().id(),
                ASTMapping::identical(ASTMappingReason::IdenticalHash),
            );
        }
        solve(
            &crate::diff::PassCtx::new(before, after, &node_cache),
            &mut diff,
        );
        diff
    }

    fn function_body<'a>(code: &'a Code, index: usize) -> tree_sitter::Node<'a> {
        code.ast
            .as_ref()
            .unwrap()
            .root_node()
            .child(index)
            .unwrap()
            .child_by_field_name("body")
            .unwrap()
    }

    /// The motivating shape: an inner wrapper is removed, so the statements that were inside it now
    /// sit one level up. The container above them has an exact counterpart and must be matched
    /// rather than reported deleted. Two statements, deliberately: with only one matched
    /// descendant a container's "lowest common ancestor" is that descendant itself, and the rule
    /// correctly has nothing to say.
    #[test]
    fn a_container_whose_content_moved_up_one_level_is_matched() {
        let before = Code::from_string(
            "fn f() {\n    {\n        a();\n        b();\n    }\n}\n",
            &Language::Rust,
        );
        let after = Code::from_string("fn f() {\n    a();\n    b();\n}\n", &Language::Rust);

        // A bare `{ ... }` in statement position parses as an `expression_statement` wrapping the
        // `block`, so the block is one level below the body's first named child.
        let before_statement = function_body(&before, 0).named_child(0).unwrap();
        let before_inner_block = if before_statement.kind() == "block" {
            before_statement
        } else {
            before_statement.named_child(0).unwrap()
        };
        let after_block = function_body(&after, 0);
        assert_eq!(
            before_inner_block.kind(),
            "block",
            "test setup sanity check"
        );

        let diff = solve_with_statements_prematched(
            &before,
            &after,
            before_inner_block,
            after_block,
            &[(0, 0), (1, 1)],
        );

        assert_eq!(
            diff.before_node_map.get(&before_inner_block.id()).copied(),
            Some(after_block.id()),
            "the block holding both surviving statements must pair with the block now holding them"
        );
        assert_eq!(
            diff.mapping
                .get(&(before_inner_block.id(), after_block.id()))
                .map(|m| m.reason),
            Some(ASTMappingReason::MutualAncestor),
            "and the pairing must be attributed to this pass, not to something else"
        );
    }

    /// Mutuality is the whole safeguard. Here two before-functions' statements are merged into one
    /// after-function: the first before-block's content does land inside the after block, so the
    /// one-directional test would pair them - but that after block also holds content from the
    /// *other* before function, so it is not exclusively anyone's counterpart and nothing may be
    /// paired on that basis.
    #[test]
    fn a_container_holding_foreign_matched_content_is_not_paired() {
        let before = Code::from_string(
            "fn f() {\n    a();\n    b();\n}\nfn g() {\n    c();\n    d();\n}\n",
            &Language::Rust,
        );
        let after = Code::from_string(
            "fn h() {\n    a();\n    b();\n    c();\n    d();\n}\n",
            &Language::Rust,
        );

        let before_f_block = function_body(&before, 0);
        let before_g_block = function_body(&before, 1);
        let after_block = function_body(&after, 0);

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();
        for (block, pairs) in [
            (before_f_block, [(0, 0), (1, 1)]),
            (before_g_block, [(0, 2), (1, 3)]),
        ] {
            for (b, a) in pairs {
                diff.add_mapping(
                    block.named_child(b).unwrap().id(),
                    after_block.named_child(a).unwrap().id(),
                    ASTMapping::identical(ASTMappingReason::IdenticalHash),
                );
            }
        }
        solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );

        assert!(
            !diff.before_node_map.contains_key(&before_f_block.id())
                && !diff.before_node_map.contains_key(&before_g_block.id()),
            "neither block exclusively corresponds to the merged one, so neither may be paired"
        );
    }

    /// End-to-end evidence that the rule survives the real pipeline, not just an isolated call:
    /// this is the corpus shape (`html-mozilla-firefox-firefox-remove-li-around-button`) reduced to
    /// one file - a wrapper removed from around repeated content, with ancestors above it.
    #[test]
    fn ancestors_above_an_unwrapped_list_are_matched_end_to_end() {
        let before = Code::from_string(
            "<div><ul><li><b>one</b></li><li><b>two</b></li></ul></div>",
            &Language::HTML,
        );
        let after = Code::from_string("<div><ul><b>one</b><b>two</b></ul></div>", &Language::HTML);

        let diff = Diff::from_code(&before, &after);
        let ast = diff.ast.as_ref().expect("html parses");
        let before_div = before
            .ast
            .as_ref()
            .unwrap()
            .root_node()
            .named_child(0)
            .unwrap();
        let after_div = after
            .ast
            .as_ref()
            .unwrap()
            .root_node()
            .named_child(0)
            .unwrap();

        assert_eq!(
            ast.before_node_map.get(&before_div.id()).copied(),
            Some(after_div.id()),
            "the outer element, whose content survives inside the corresponding one, must match"
        );
    }
}
