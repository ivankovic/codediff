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
use crate::code::Code;
use crate::diff::{
    ASTDiff, ASTMapping, ASTMappingOperation, ASTMappingReason, COST_DELETE, COST_INSERT, NodeCache,
};

/// Terminal completeness sweep: give every node still carrying no decision at all an explicit
/// `Delete` (before side) or `Insert` (after side).
///
/// Every other pass in the pipeline is free to leave a node alone when it has nothing confident to
/// say about it. That is the right default for a *matching* pass, but it means the finished
/// mapping could come back partial, which is a different thing from "this node was deleted": a
/// consumer walking the after tree found no entry for a genuinely new node and had no way to
/// distinguish "inserted" from "not considered".
///
/// The gap is not exotic - it is the ordinary shape of a wrap. In
/// `typescript-add-error-handling` (2026-08-17), two statements get wrapped in a new `try`/`catch`:
/// the statements themselves match straight through, but the new `try_statement` and its
/// `statement_block`, and the `program` root above them, all came out with **no entry at all**.
/// The terminal Myers fallback only ever assigns decisions to *maximal* unmatched roots
/// (`resolve_residual_forest_via_myers_lcs` -> `maximal_unmatched_roots`), and neither the new
/// wrapper (whose subtree contains matched descendants, so it is not maximal) nor an unmatched
/// ancestor of matched children is one. `solve_bottom_up_propagation` cannot rescue them either:
/// its rule 3 deliberately requires every child to have matched into the *same direct* after-parent,
/// and a wrap is precisely the case where they didn't. 10 fixtures corpus-wide were affected,
/// 81 nodes in total.
///
/// Deliberately not a matching heuristic: it never pairs anything, so it cannot make a wrong guess
/// or take a partner away from a later pass - there is no later pass. It only converts "no
/// decision" into the decision the absence already implied, which is what makes it safe to run
/// unconditionally as the last thing in the pipeline.
///
/// `Delete`/`Insert`, never the `WithChildren` variants: a node reaching this sweep may well have
/// matched descendants (the wrap case above is exactly that), and `DeleteWithChildren` would claim
/// the whole subtree went away. Cost is this node alone for the same reason - its children carry
/// their own entries and their own costs.
pub fn solve(before: &Code, after: &Code, node_cache: &NodeCache, diff: &mut ASTDiff) {
    match_roots_if_unresolved(before, after, diff);

    // Sorted for determinism: `NodeCache`'s maps are `FxHashMap`s, and while the mappings added
    // here are independent of each other (each node's decision depends only on whether that node
    // is absent, never on what this sweep did to another node), iterating in a stable order keeps
    // `mapping`'s insertion sequence reproducible run to run.
    let mut unresolved_before: Vec<usize> = node_cache
        .before
        .keys()
        .copied()
        .filter(|id| !diff.before_node_map.contains_key(id))
        .collect();
    unresolved_before.sort_unstable();
    for before_id in unresolved_before {
        diff.add_mapping(
            before_id,
            0,
            ASTMapping {
                cost: COST_DELETE,
                operation: ASTMappingOperation::Delete,
                reason: ASTMappingReason::UnresolvedNode,
            },
        );
    }

    let mut unresolved_after: Vec<usize> = node_cache
        .after
        .keys()
        .copied()
        .filter(|id| !diff.after_node_map.contains_key(id))
        .collect();
    unresolved_after.sort_unstable();
    for after_id in unresolved_after {
        diff.add_mapping(
            0,
            after_id,
            ASTMapping {
                cost: COST_INSERT,
                operation: ASTMappingOperation::Insert,
                reason: ASTMappingReason::UnresolvedNode,
            },
        );
    }
}

/// Pair the two trees' root nodes when nothing else has, before the delete/insert sweep above gets
/// to them.
///
/// The roots of two versions of one file always correspond - that is what "two versions of one
/// file" means - so this is the one pairing that needs no evidence beyond both roots still being
/// unclaimed. They *usually* are claimed: `solve_hash_descent` matches them outright when the file
/// is unchanged, and propagation reaches them when the edit is contained. What defeats both is a
/// wrap at top level (`typescript-add-error-handling`: statements moved inside a new `try`, so the
/// root's children matched into two *different* after-parents and propagation's same-direct-parent
/// rule correctly declined). Without this, the sweep below would then declare the file's own root
/// deleted and a new one inserted, which is never what happened.
///
/// Cost 0 and `MatchButNotIdentical` rather than a real edit-distance computation: `UnitCostModel::
/// ren` already prices a same-kind internal-node pairing at 0 (children carry their own costs), and
/// running APTED over two whole files to rediscover that would cost more than the entire rest of
/// the pipeline. If the roots' subtrees were identical, hash descent would have matched them long
/// before this point, so "matched but not identical" is the only state left to record.
fn match_roots_if_unresolved(before: &Code, after: &Code, diff: &mut ASTDiff) {
    let (Some(before_ast), Some(after_ast)) = (before.ast.as_ref(), after.ast.as_ref()) else {
        return;
    };
    let before_root = before_ast.root_node().id();
    let after_root = after_ast.root_node().id();
    if diff.before_node_map.contains_key(&before_root)
        || diff.after_node_map.contains_key(&after_root)
    {
        return;
    }
    diff.add_mapping(
        before_root,
        after_root,
        ASTMapping {
            cost: 0,
            operation: ASTMappingOperation::MatchButNotIdentical,
            reason: ASTMappingReason::UnresolvedNode,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::Language;
    use crate::diff::Diff;

    /// The wrap that motivated this pass: statements move inside a new `try` block. The wrapper
    /// nodes and the root above them must come out as explicit decisions, not as absences.
    #[test]
    fn wrapper_nodes_left_undecided_by_matching_get_explicit_inserts() {
        let before = Code::from_string("const x = f();\nlog(x);\n", &Language::TypeScript);
        let after = Code::from_string(
            "try {\n  const x = f();\n  log(x);\n} catch (e) {\n  report(e);\n}\n",
            &Language::TypeScript,
        );

        let diff = Diff::from_code(&before, &after);
        let ast = diff.ast.as_ref().expect("typescript parses");
        let after_ast = after.ast.as_ref().unwrap();
        let try_statement = after_ast.root_node().child(0).unwrap();
        assert_eq!(
            try_statement.kind(),
            "try_statement",
            "test setup sanity check"
        );

        assert_eq!(
            ast.after_node_map.get(&try_statement.id()).copied(),
            Some(0),
            "the new try wrapper should be an explicit insert, not an absent entry"
        );
        assert_eq!(
            ast.mapping
                .get(&(0, try_statement.id()))
                .map(|m| m.operation.clone()),
            Some(ASTMappingOperation::Insert),
            "and a plain Insert, not InsertWithChildren - its descendants match the before side"
        );
    }

    /// A top-level wrap leaves both roots unclaimed - they must still be paired with each other,
    /// never reported as the whole file being deleted and a new one inserted.
    #[test]
    fn both_roots_are_paired_rather_than_deleted_and_inserted() {
        let before = Code::from_string("const x = f();\nlog(x);\n", &Language::TypeScript);
        let after = Code::from_string(
            "try {\n  const x = f();\n  log(x);\n} catch (e) {\n  report(e);\n}\n",
            &Language::TypeScript,
        );

        let diff = Diff::from_code(&before, &after);
        let ast = diff.ast.as_ref().expect("typescript parses");
        let before_root = before.ast.as_ref().unwrap().root_node().id();
        let after_root = after.ast.as_ref().unwrap().root_node().id();

        assert_eq!(
            ast.before_node_map.get(&before_root).copied(),
            Some(after_root),
            "the two files' roots must map to each other"
        );
    }

    /// The whole point of the sweep: no node of either tree is left without a decision.
    #[test]
    fn every_node_of_both_trees_ends_up_with_a_decision() {
        let before = Code::from_string(
            "function f(a) {\n  return a + 1;\n}\nf(2);\n",
            &Language::JavaScript,
        );
        let after = Code::from_string(
            "function f(a, b) {\n  if (b) {\n    return a;\n  }\n  return a + 1;\n}\ng(f(2));\n",
            &Language::JavaScript,
        );

        let diff = Diff::from_code(&before, &after);
        let ast = diff.ast.as_ref().expect("javascript parses");
        let cache = NodeCache::build(&before, &after);

        let missing_before: Vec<usize> = cache
            .before
            .keys()
            .copied()
            .filter(|id| !ast.before_node_map.contains_key(id))
            .collect();
        let missing_after: Vec<usize> = cache
            .after
            .keys()
            .copied()
            .filter(|id| !ast.after_node_map.contains_key(id))
            .collect();

        assert!(
            missing_before.is_empty() && missing_after.is_empty(),
            "every node must carry a decision: {} before-node(s) and {} after-node(s) had none",
            missing_before.len(),
            missing_after.len()
        );
    }
}
