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
use crate::code::metadata::metadata_of;
use crate::code::{ASTMetadata, Code};
use crate::diff::hash_tree_matching::pair_children_for_descent;
use crate::diff::{
    ASTDiff, ASTMapping, ASTMappingOperation, ASTMappingReason, COST_UPDATE, NodeCache,
};

/**
* Rust's `let`-chains collapse a run of nested `if let PATTERN = EXPR { ... }` statements (each
* one the sole statement of its parent's body) into a single `if PATTERN1 = EXPR1 && PATTERN2 =
* EXPR2 && ... { BODY }`. `N` nested `if_expression`/`block` wrapper pairs become one
* `if_expression` whose condition is a `let_chain` and whose block is the innermost `BODY`
* directly.
*
* Phase 1 (`solve_hash_descent`) already matches `BODY` correctly on its own - `BODY`'s text is
* byte-identical before and after, so its hash-based root match finds it regardless of how deeply
* nested it started out. What phase 1 gets wrong is *attribution*: because it operates purely on
* subtree hashes, it has no way to know that the *outermost* wrapper `if_expression` is the one
* that structurally persists as the merged `if` - every wrapper's own text differs from anything
* on the after side (each contains the *next* wrapper, not `BODY` directly), so phase 1 simply
* never considers them as candidates, leaving the outermost wrapper's own condition to fall
* through to `Delete`+re-`Insert` rather than being recognized as unchanged.
*
* This pass runs *after* phase 1, specifically to react to what phase 1 already matched rather
* than duplicate it: it looks for a before-side chain of trivial single-statement if-wrapper
* levels whose innermost block phase 1 already mapped, finds the after-side `if_expression`
* ancestor of that mapped block, and - only if every one of the chain's `N` conditions is
* byte-identical, in order, to the after `let_chain`'s `N` clauses - adds the mappings phase 1
* structurally cannot reach: the outermost `if_expression` itself (`MatchButNotIdentical`, since
* its condition text did change), each condition subtree (`Identical`, reusing
* `pair_children_for_descent` for consistency with how phase 1 itself descends), and re-affirms
* `terminal_block`'s (`BODY`'s) own match under this pass's own `NestedConditionCollapse` reason.
* Every wrapper node's own braces are deliberately left exactly as phase 1 already has them.
*
* **What this pass does *not* fix, and why**: an earlier version also tried re-attributing
* `after_block`'s `{`/`}` tokens from the innermost wrapper (where phase 1's hash descent leaves
* them) to the outermost one, on the theory that a human always reads the outermost wrapper's own
* braces as the ones that persist. Measured against this fixture's own hand-painted ground truth
* (`rust-next-font-imports-generator`, `benchmark_optimal_solutions --details`), that theory was
* wrong in a way that isn't simply "backwards": which brace the ground truth keeps does not follow
* a single consistent outer-vs-inner rule across this fixture's own two if-let chains (a 3-level
* and a 2-level one). Reverted rather than guessed at again - see the 2026-09-01 painting-baseline
* investigation for the measurement. A real fix for the brace attribution needs a clearer picture
* of what the ground truth actually wants, not a second guess at the same theory.
*
* **Why `BODY`'s match is re-tagged**: `crate::diff::text::ranges` classifies a matched node's
* `Move` vs `Identical` purely from that one node's own before/after column position, independent
* of its ancestors - and `BODY` is genuinely reindented by this transformation (nesting levels
* removed around it), so by position alone it looks exactly like a real relocation (the same
* signature `rust-add-if`'s genuinely-moved block has - see `ranges`'s own doc comment on why that
* case can't be excluded by column delta alone). This pass has already verified, structurally,
* that `BODY`'s move is *only* a reindent, so it tells `ranges` that directly via the
* `NestedConditionCollapse` reason. Whether `ranges` actually *acts* on that tag is gated by
* [`crate::diff::text::RenderOptions::paint_reindent_only_moves`] - `MINIMAL` and `FULL` disagree
* about this (measured against this fixture's own separate `Minimal`/`Full` ground truths:
* `Minimal` wants the body unpainted, `Full` wants it painted `Move`), so the tag alone isn't a
* rendering verdict, just the fact this pass is positioned to know that a bare heuristic in
* `ranges` cannot safely derive on its own.
*/
pub fn solve(before: &Code, after: &Code, node_cache: &NodeCache, diff: &mut ASTDiff) {
    let Some(before_tree) = &before.ast else {
        return;
    };
    if after.ast.is_none() {
        return;
    }
    let before_metadata = metadata_of(before);
    let after_metadata = metadata_of(after);

    let mut candidates: Vec<tree_sitter::Node> = Vec::new();
    let mut stack = vec![before_tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.kind() == "if_expression" {
            candidates.push(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }

    for outer_if in candidates {
        if diff.before_node_map.contains_key(&outer_if.id()) {
            continue;
        }
        try_collapse(
            node_cache,
            &before_metadata,
            &after_metadata,
            diff,
            outer_if,
        );
    }
}

/// The next node in the wrapper chain below `block`, if `block` is a trivial single-statement
/// wrapper whose sole statement is itself an `if_expression` (an `if` used as a statement is
/// wrapped in `expression_statement`; as a tail expression it's the direct child - both are
/// accepted).
fn single_nested_if(block: tree_sitter::Node) -> Option<tree_sitter::Node> {
    if block.named_child_count() != 1 {
        return None;
    }
    let only = block.named_child(0)?;
    match only.kind() {
        "if_expression" => Some(only),
        "expression_statement" if only.named_child_count() == 1 => {
            let inner = only.named_child(0)?;
            (inner.kind() == "if_expression").then_some(inner)
        }
        _ => None,
    }
}

/// `if_expression`'s condition and block, rejecting anything with an `else` branch (a let-chain
/// has no room for one - each wrapper level in the chain must be a bare `if`, no `else`).
fn condition_and_block(
    if_expr: tree_sitter::Node,
) -> Option<(tree_sitter::Node, tree_sitter::Node)> {
    if if_expr.named_child_count() != 2 {
        return None;
    }
    let condition = if_expr.named_child(0)?;
    let block = if_expr.named_child(1)?;
    (block.kind() == "block").then_some((condition, block))
}

fn try_collapse(
    node_cache: &NodeCache,
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
    diff: &mut ASTDiff,
    outer_if: tree_sitter::Node,
) {
    // Walk the wrapper chain, collecting each level's condition and stopping at the first level
    // that isn't a trivial single-if wrapper - that level's block is `BODY`.
    let mut conditions = Vec::new();
    let mut level = outer_if;
    let terminal_block = loop {
        let Some((condition, block)) = condition_and_block(level) else {
            return;
        };
        if condition.kind() != "let_condition" {
            return;
        }
        conditions.push(condition);
        match single_nested_if(block) {
            Some(next) => level = next,
            None => break block,
        }
    };
    // `>= 2` levels: a lone if-let (no nesting at all) needs no help from this pass.
    if conditions.len() < 2 {
        return;
    }

    // Anchor on wherever phase 1 already matched `BODY` - `terminal_block` itself is the expected
    // case (see this module's doc comment), but fall back to its mapped child (if phase 1 instead
    // matched some node inside it) so a slightly different hash-descent outcome doesn't silently
    // defeat this pass.
    let anchor_after_id = diff
        .before_node_map
        .get(&terminal_block.id())
        .copied()
        .or_else(|| {
            let mut cursor = terminal_block.walk();
            terminal_block
                .children(&mut cursor)
                .find_map(|child| diff.before_node_map.get(&child.id()).copied())
        });
    let Some(anchor_after_id) = anchor_after_id else {
        return;
    };
    let Some(anchor_after_node) = node_cache.after.get(&anchor_after_id) else {
        return;
    };

    // Walk up from wherever the anchor landed to the nearest `if_expression` ancestor - the
    // merged `if`, if this really is a let-chain collapse.
    let mut after_if = *anchor_after_node;
    loop {
        if after_if.kind() == "if_expression" {
            break;
        }
        let Some(parent) = after_if.parent() else {
            return;
        };
        after_if = parent;
    }
    if diff.after_node_map.contains_key(&after_if.id()) {
        return;
    }
    let Some((after_condition, after_block)) = condition_and_block(after_if) else {
        return;
    };
    // The if_expression we walked up to must actually own `BODY` directly (or own the mapped
    // child the fallback branch above found) - otherwise the anchor landed under some unrelated
    // ancestor and this isn't the shape we think it is.
    let owns_anchor = after_block.id() == anchor_after_node.id() || {
        let mut cursor = after_block.walk();
        after_block
            .children(&mut cursor)
            .any(|child| child.id() == anchor_after_node.id())
    };
    if !owns_anchor {
        return;
    }
    if after_condition.kind() != "let_chain" {
        return;
    }
    let mut cursor = after_condition.walk();
    let after_conditions: Vec<_> = after_condition
        .named_children(&mut cursor)
        .filter(|c| c.kind() == "let_condition")
        .collect();
    if after_conditions.len() != conditions.len() {
        return;
    }

    // Every condition must match exactly, in order - deliberately conservative (see module doc).
    for (before_cond, after_cond) in conditions.iter().zip(&after_conditions) {
        let before_hash = before_metadata
            .node_to_kind_and_value_hash
            .get(&before_cond.id());
        let after_hash = after_metadata
            .node_to_kind_and_value_hash
            .get(&after_cond.id());
        if before_hash.is_none() || before_hash != after_hash {
            return;
        }
    }

    // Confirmed: record the outer if_expression's own match, its `if` token, every condition
    // subtree, and re-affirm `terminal_block`'s (`BODY`'s) own match under this pass's own
    // `NestedConditionCollapse` reason - see the module doc comment for why. Every wrapper's own
    // braces are left exactly as phase 1 already matched them - see the module doc comment for
    // why that part isn't fixed here.
    diff.add_mapping(
        outer_if.id(),
        after_if.id(),
        ASTMapping {
            cost: COST_UPDATE,
            operation: ASTMappingOperation::MatchButNotIdentical,
            reason: ASTMappingReason::NestedConditionCollapse,
        },
    );
    if let (Some(before_if_token), Some(after_if_token)) = (outer_if.child(0), after_if.child(0))
        && before_if_token.kind() == "if"
        && after_if_token.kind() == "if"
    {
        diff.add_mapping(
            before_if_token.id(),
            after_if_token.id(),
            ASTMapping::identical(ASTMappingReason::NestedConditionCollapse),
        );
    }
    diff.add_mapping(
        terminal_block.id(),
        after_block.id(),
        ASTMapping::identical(ASTMappingReason::NestedConditionCollapse),
    );
    for (before_cond, after_cond) in conditions.into_iter().zip(after_conditions) {
        map_identical_subtree(
            before_cond,
            after_cond,
            before_metadata,
            after_metadata,
            diff,
        );
    }
}

/// Marks `before_node`/`after_node` (already confirmed `kind_and_value_hash`-identical by the
/// caller) `Identical`, and every descendant pair the same way - the same lockstep descent
/// `hash_tree_matching::solve_with_hash_map` itself uses for a root hash match, reused here rather
/// than re-implemented so a commutative-container condition (unlikely inside a `let_condition`,
/// but not impossible) gets the same reorder-aware handling either way.
fn map_identical_subtree(
    before_node: tree_sitter::Node,
    after_node: tree_sitter::Node,
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
    diff: &mut ASTDiff,
) {
    if diff.before_node_map.contains_key(&before_node.id()) {
        return;
    }
    diff.add_mapping(
        before_node.id(),
        after_node.id(),
        ASTMapping::identical(ASTMappingReason::NestedConditionCollapse),
    );
    let mut stack = vec![(before_node, after_node)];
    while let Some((b, a)) = stack.pop() {
        let (pairs, _reordered) = pair_children_for_descent(b, a, before_metadata, after_metadata);
        for (before_child, after_child) in pairs {
            if diff.before_node_map.contains_key(&before_child.id()) {
                continue;
            }
            diff.add_mapping(
                before_child.id(),
                after_child.id(),
                ASTMapping::identical(ASTMappingReason::NestedConditionCollapse),
            );
            stack.push((before_child, after_child));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::code::Language;
    use crate::diff::{ASTMappingOperation, ASTMappingReason, NodeCache};
    use crate::test::helper::find_first_of_kind;

    /// The rules doc's own motivating shape, minimized: three nested `if let`s collapsing into
    /// one let-chain. The outermost `if_expression` and its own condition must end up mapped -
    /// the one gap phase 1 structurally cannot close on its own (see the module doc comment).
    #[test]
    fn outer_if_and_its_condition_are_matched_across_a_let_chain_collapse() {
        // The body has to be big enough to clear phase 1's own `min_subtree_size` selection
        // threshold (`NodeSelectionConfig::default`) - otherwise phase 1 never matches it in the
        // first place, and this pass (which only reacts to what phase 1 already matched) has
        // nothing to anchor on.
        let body = "\x20               step_one();\n\
                     \x20               step_two();\n\
                     \x20               step_three();\n\
                     \x20               step_four();\n\
                     \x20               step_five();\n\
                     \x20               step_six();\n";
        let before = crate::code::Code::from_string(
            &format!(
                "fn f() {{\n\
                 \x20   if let A(a) = x {{\n\
                 \x20       if let B(b) = y {{\n\
                 \x20           if let C(c) = z {{\n\
                 {body}\
                 \x20           }}\n\
                 \x20       }}\n\
                 \x20   }}\n\
                 }}\n"
            ),
            &Language::Rust,
        );
        let after = crate::code::Code::from_string(
            &format!(
                "fn f() {{\n\
                 \x20   if let A(a) = x\n\
                 \x20       && let B(b) = y\n\
                 \x20       && let C(c) = z\n\
                 \x20   {{\n\
                 {body}\
                 \x20   }}\n\
                 }}\n"
            ),
            &Language::Rust,
        );
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = crate::diff::ASTDiff::default();
        crate::diff::solve_hash_descent::solve(&before, &after, &node_cache, &mut diff);
        super::solve(&before, &after, &node_cache, &mut diff);

        let before_root = before.ast.as_ref().unwrap().root_node();
        let after_root = after.ast.as_ref().unwrap().root_node();
        let before_outer_if = find_first_of_kind(before_root, "if_expression").unwrap();
        let after_if = find_first_of_kind(after_root, "if_expression").unwrap();

        let mapping = diff
            .mapping
            .get(&(before_outer_if.id(), after_if.id()))
            .expect("outer if_expression must be matched to the merged if_expression");
        assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);
        assert_eq!(mapping.reason, ASTMappingReason::NestedConditionCollapse);

        let before_condition = before_outer_if.named_child(0).unwrap();
        assert!(
            diff.before_node_map.contains_key(&before_condition.id()),
            "the outermost condition (`let A(a) = x`) must be matched, not deleted"
        );
    }

    /// A single, non-nested if-let needs no help from this pass - it's already handled correctly
    /// upstream, and firing here would be pure risk for zero benefit.
    #[test]
    fn a_lone_if_let_is_left_alone() {
        let before = crate::code::Code::from_string(
            "fn f() {\n    if let A(a) = x {\n        body();\n    }\n}\n",
            &Language::Rust,
        );
        let after = before.clone();
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = crate::diff::ASTDiff::default();
        crate::diff::solve_hash_descent::solve(&before, &after, &node_cache, &mut diff);
        let before_matches = diff.mapping.len();

        super::solve(&before, &after, &node_cache, &mut diff);

        assert_eq!(
            diff.mapping.len(),
            before_matches,
            "a lone if-let is already fully matched by phase 1; this pass must add nothing"
        );
    }

    /// An `else` branch anywhere in the chain rules out a let-chain reading (Rust let-chains have
    /// no room for one) - must not fire.
    #[test]
    fn a_chain_with_an_else_branch_is_left_alone() {
        let before = crate::code::Code::from_string(
            "fn f() {\n\
             \x20   if let A(a) = x {\n\
             \x20       if let B(b) = y {\n\
             \x20           body();\n\
             \x20       } else {\n\
             \x20           other();\n\
             \x20       }\n\
             \x20   }\n\
             }\n",
            &Language::Rust,
        );
        let after = crate::code::Code::from_string(
            "fn f() {\n\
             \x20   if let A(a) = x\n\
             \x20       && let B(b) = y\n\
             \x20   {\n\
             \x20       body();\n\
             \x20   } else {\n\
             \x20       other();\n\
             \x20   }\n\
             }\n",
            &Language::Rust,
        );
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = crate::diff::ASTDiff::default();
        crate::diff::solve_hash_descent::solve(&before, &after, &node_cache, &mut diff);

        let before_root = before.ast.as_ref().unwrap().root_node();
        let outer_if = find_first_of_kind(before_root, "if_expression").unwrap();
        assert!(
            !diff.before_node_map.contains_key(&outer_if.id()),
            "test setup: the outer if must be unmatched before this pass runs"
        );

        super::solve(&before, &after, &node_cache, &mut diff);

        assert!(
            !diff.before_node_map.contains_key(&outer_if.id()),
            "an else-bearing chain must not be collapsed - condition_and_block rejects it"
        );
    }
}
