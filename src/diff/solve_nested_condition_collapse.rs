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
use crate::diff::hash_tree_matching::pair_children_for_descent;
use crate::diff::{
    ASTDiff, ASTMapping, ASTMappingOperation, ASTMappingReason, COST_UPDATE, NodeCache,
};

/// Rust `let`-chains collapse nested `if let` wrappers (each the sole statement of its parent's
/// body) into one `if A && B && ... { BODY }`. Phase 1 matches `BODY` by hash but never the
/// wrappers, whose text differs from everything on the after side, so the outer `if` and its
/// unchanged condition come out as delete+insert.
///
/// When every one of the chain's conditions is hash-identical, in order, to the after `let_chain`'s
/// clauses, this maps the outermost `if` (`MatchButNotIdentical`), its `if` token and each condition,
/// and re-tags `BODY` as `NestedConditionCollapse`. The wrappers' braces are left as phase 1 has
/// them: rust-next-font-imports-generator's ground truth follows no single outer-vs-inner rule
/// for them.
///
/// `BODY` is re-tagged because `ranges()` judges `Move` from a node's own column alone, and a
/// reindent looks like rust-add-if's genuine move. Whether `ranges()` acts on the tag is
/// [`crate::diff::text::RenderOptions::paint_reindent_only_moves`]: `Minimal` wants the body
/// unpainted, `Full` wants `Move`.
pub fn solve(ctx: &PassCtx, diff: &mut ASTDiff) {
    let (before, after, node_cache) = (ctx.before, ctx.after, ctx.node_cache);
    let Some(before_tree) = &before.ast else {
        return;
    };
    if after.ast.is_none() {
        return;
    }
    let before_metadata = ctx.before_metadata();
    let after_metadata = ctx.after_metadata();

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
        try_collapse(node_cache, before_metadata, after_metadata, diff, outer_if);
    }
}

/// The `if_expression` that is `block`'s sole statement, bare (tail expression) or wrapped in an
/// `expression_statement`.
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

/// `if_expression`'s condition and block; `None` with an `else` branch, which a let-chain cannot hold.
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
    if conditions.len() < 2 {
        return;
    }

    // Anchor on `BODY`'s match, or on a matched child if hash descent landed one level lower.
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
    // Otherwise the anchor sits under some unrelated `if`.
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

/// Maps two hash-identical subtrees `Identical` in lockstep, via phase 1's own
/// `pair_children_for_descent` so a commutative container gets the same reorder-aware pairing.
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

    #[test]
    fn outer_if_and_its_condition_are_matched_across_a_let_chain_collapse() {
        // Large enough for phase 1's `min_subtree_size`, or there is no anchor.
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
        crate::diff::solve_hash_descent::solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );
        super::solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );

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

    #[test]
    fn a_lone_if_let_is_left_alone() {
        let before = crate::code::Code::from_string(
            "fn f() {\n    if let A(a) = x {\n        body();\n    }\n}\n",
            &Language::Rust,
        );
        let after = before.clone();
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = crate::diff::ASTDiff::default();
        crate::diff::solve_hash_descent::solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );
        let before_matches = diff.mapping.len();

        super::solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );

        assert_eq!(
            diff.mapping.len(),
            before_matches,
            "a lone if-let is already fully matched by phase 1; this pass must add nothing"
        );
    }

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
        crate::diff::solve_hash_descent::solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );

        let before_root = before.ast.as_ref().unwrap().root_node();
        let outer_if = find_first_of_kind(before_root, "if_expression").unwrap();
        assert!(
            !diff.before_node_map.contains_key(&outer_if.id()),
            "test setup: the outer if must be unmatched before this pass runs"
        );

        super::solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );

        assert!(
            !diff.before_node_map.contains_key(&outer_if.id()),
            "an else-bearing chain must not be collapsed - condition_and_block rejects it"
        );
    }

    #[test]
    fn a_chain_whose_condition_changed_is_left_alone() {
        let body = "        a();\n        b();\n        c();\n        d();\n        e();\n";
        let before = crate::code::Code::from_string(
            &format!(
                "fn f() {{\n    if let A(a) = x {{\n        if let B(b) = y {{\n{body}        }}\n    }}\n}}\n"
            ),
            &Language::Rust,
        );
        let after = crate::code::Code::from_string(
            &format!("fn f() {{\n    if let A(a) = x && let B(b) = w {{\n{body}    }}\n}}\n"),
            &Language::Rust,
        );
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = crate::diff::ASTDiff::default();
        crate::diff::solve_hash_descent::solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );
        super::solve(
            &crate::diff::PassCtx::new(&before, &after, &node_cache),
            &mut diff,
        );

        assert!(
            diff.mapping
                .values()
                .all(|m| m.reason != ASTMappingReason::NestedConditionCollapse)
        );
    }
}
