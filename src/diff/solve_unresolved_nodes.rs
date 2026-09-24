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
use crate::diff::PassCtx;
use crate::diff::{ASTDiff, ASTMapping, ASTMappingReason};

/// Terminal completeness sweep: every node still without a decision gets an explicit `Delete` or
/// `Insert`, so a consumer can tell "inserted" from "not considered".
///
/// The ordinary source of such nodes is a wrap (typescript-add-error-handling): the new wrapper
/// contains matched descendants, so it is not a maximal unmatched root for the Myers fallback, and
/// `solve_bottom_up_propagation` declines it by design. Never pairs anything, so it is safe to run
/// unconditionally last.
///
/// Plain `Delete`/`Insert` with this node's own cost, never the `WithChildren` variants: such a
/// node may well have matched descendants.
pub fn solve(ctx: &PassCtx, diff: &mut ASTDiff) {
    let (before, after, node_cache) = (ctx.before, ctx.after, ctx.node_cache);
    match_roots_if_unresolved(before, after, diff);

    // Sorted so `mapping`'s insertion sequence is reproducible run to run.
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
            ASTMapping::deleted(ASTMappingReason::UnresolvedNode),
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
            ASTMapping::inserted(ASTMappingReason::UnresolvedNode),
        );
    }
}

/// Pairs the two roots when nothing else has: two versions of one file always correspond, and a
/// top-level wrap defeats both hash descent and propagation.
///
/// Cost 0 and `MatchButNotIdentical` without APTED: `UnitCostModel::ren` prices a same-kind
/// internal pairing at 0, and identical roots would already have been hash-matched.
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
        ASTMapping::matched_not_identical(ASTMappingReason::UnresolvedNode),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::Language;
    use crate::diff::NodeCache;
    use crate::diff::{ASTMappingOperation, Diff};

    /// Statements moved inside a new `try`: the wrapper must be an explicit insert.
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
