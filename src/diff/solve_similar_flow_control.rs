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
use std::collections::{HashMap, HashSet, VecDeque};

use tree_sitter::Node;

use crate::code::{ASTMetadata, Code};
use crate::diff::apted::{self, Algorithm};
use crate::diff::grouped_greedy_matcher;
use crate::diff::nodes::{
    FlowControlArm, collect_unmatched, flow_control_arms, flow_control_family,
    flow_control_signature_set, flow_control_similarity_of_sets,
};
use crate::diff::{ASTDiff, NodeCache};

/**
* MatchSimilarFlowControl: pairs up still-unmatched `if`/`switch`/`match` constructs whose arms
* (patterns, case labels, or `if`/`else if` conditions) are mostly the same, so a reviewer sees
* "this is basically the same branching logic" instead of the whole construct being torn down as a
* delete+insert.
*
* Called from phase 4 (`solve_syntax_aware_matching`), after named-group matching has had its
* chance, and before the final APTED pass. Without it, a construct whose enclosing function/impl
* has no same-named counterpart on the other side is left for the final pass to swallow into a
* from-scratch subtree delete/insert, even when its arms are clearly the same set of cases, just
* moved to a differently-named container (e.g. logic hoisted from one function into another as
* part of a rename/restructure).
*
* See [`crate::diff::nodes::FlowControlFamily`] for exactly which constructs are recognized per
* language (Python's `if` is the one notable gap - its flat `elif`/`else` fields don't fit the
* recursive `if`/`else if` shape this pass relies on). Candidate collection and the Jaccard-
* similarity cost function are this pass's own; grouping by family, greedy assignment, and the
* threshold check (>=75% shared signatures) are handled generically by `grouped_greedy_matcher::
* solve` (`TODO.md`'s "generalization of phase 4" analysis, 2026-07-18) - `FlowControlFamily`
* serves directly as its compatibility key. Accepted pairs have their identical-signature arms
* individually resolved via APTED first, then the container itself is diffed so any leftover value
* expression, added arms or removed arms are handled normally. For `if` chains specifically, this
* per-arm resolution is naturally recursive: a matched non-terminal branch's own subtree already
* contains the rest of the chain, so resolving it resolves every branch nested inside it too.
*
* This is a heuristic for reducing spurious delete+insert pairs, not a general moved-code detector:
* it only pairs the *matching-signature* arms of a container pair it already decided to pair. Content
* that moved to some unrelated location (a different function entirely) is out of scope here.
*/
const SIMILARITY_THRESHOLD: f64 = 0.75;

pub fn solve(before: &Code, after: &Code, _node_cache: &NodeCache, diff: &mut ASTDiff) {
    let before_metadata = crate::code::metadata::metadata_of(before);
    let after_metadata = crate::code::metadata::metadata_of(after);

    let Some(before_ast) = before.ast.as_ref() else { return };
    let Some(after_ast) = after.ast.as_ref() else { return };
    let language = before_metadata.language;

    let before_source = before.contents.as_bytes();
    let after_source = after.contents.as_bytes();

    let before_items: Vec<(Node, Vec<FlowControlArm>)> = collect_unmatched(
        before_ast.root_node(),
        &diff.before_node_map,
        |node| flow_control_family(node.kind(), &language).is_some(),
    )
    .into_iter()
    .filter_map(|node| flow_control_arms(node, &language, before_source).map(|arms| (node, arms)))
    .collect();
    let after_items: Vec<(Node, Vec<FlowControlArm>)> = collect_unmatched(
        after_ast.root_node(),
        &diff.after_node_map,
        |node| flow_control_family(node.kind(), &language).is_some(),
    )
    .into_iter()
    .filter_map(|node| flow_control_arms(node, &language, after_source).map(|arms| (node, arms)))
    .collect();
    if before_items.is_empty() || after_items.is_empty() {
        return;
    }

    // Precompute each candidate's signature set once (keyed by node id) - `grouped_greedy_
    // matcher`'s cost function is called once per same-family pair, and would otherwise rebuild
    // both `HashSet`s from scratch every time.
    let before_sets: HashMap<usize, HashSet<&str>> =
        before_items.iter().map(|(node, arms)| (node.id(), flow_control_signature_set(arms))).collect();
    let after_sets: HashMap<usize, HashSet<&str>> =
        after_items.iter().map(|(node, arms)| (node.id(), flow_control_signature_set(arms))).collect();
    // Same idea for the arms themselves, needed by `anchor_matching_arms` on accept.
    let before_arms_by_id: HashMap<usize, &Vec<FlowControlArm>> =
        before_items.iter().map(|(node, arms)| (node.id(), arms)).collect();
    let after_arms_by_id: HashMap<usize, &Vec<FlowControlArm>> =
        after_items.iter().map(|(node, arms)| (node.id(), arms)).collect();

    // (id, family) candidate lists - `collect_unmatched`'s stack-based DFS order is deterministic
    // run-to-run (satisfying `grouped_greedy_matcher`'s determinism contract) even though it isn't
    // preorder per se.
    let before_candidates: Vec<(usize, crate::diff::nodes::FlowControlFamily)> = before_items
        .iter()
        .map(|(node, _)| (node.id(), flow_control_family(node.kind(), &language).expect("collect_unmatched already filtered to flow-control kinds")))
        .collect();
    let after_candidates: Vec<(usize, crate::diff::nodes::FlowControlFamily)> = after_items
        .iter()
        .map(|(node, _)| (node.id(), flow_control_family(node.kind(), &language).expect("collect_unmatched already filtered to flow-control kinds")))
        .collect();

    // `grouped_greedy_matcher` works in "lower cost is better" terms; Jaccard similarity is
    // "higher is better", so the transform is `1.0 - similarity` both for the cost function and
    // the acceptance threshold (`score >= SIMILARITY_THRESHOLD` <=> `1.0 - score <= 1.0 -
    // SIMILARITY_THRESHOLD`), preserving both the ordering (highest similarity first) and the
    // rejection rule exactly.
    grouped_greedy_matcher::solve(
        diff,
        &before_candidates,
        &after_candidates,
        |before_id, after_id| 1.0 - flow_control_similarity_of_sets(&before_sets[&before_id], &after_sets[&after_id]),
        Some(1.0 - SIMILARITY_THRESHOLD),
        |before_id, after_id, diff| {
            anchor_matching_arms(
                before_arms_by_id[&before_id],
                after_arms_by_id[&after_id],
                &before_metadata,
                &after_metadata,
                diff,
            );

            apted::for_nodes(
                &before_metadata,
                &after_metadata,
                vec![before_id],
                vec![after_id],
                Algorithm::Apted,
                "flow_control_container",
                diff,
            );
        },
    );
}


/// Resolves each before-arm against the first not-yet-claimed after-arm with an identical
/// (non-wildcard) signature, via a dedicated `apted::for_nodes` call per pair - the same idiom
/// `solve_syntax_aware_matching` uses to pre-match same-named methods before diffing their
/// enclosing impl. Wildcard arms and arms with no counterpart signature are left for the
/// container-level `for_nodes` call to handle as ordinary adds/removals.
fn anchor_matching_arms(
    before_arms: &[FlowControlArm],
    after_arms: &[FlowControlArm],
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
    diff: &mut ASTDiff,
) {
    let mut after_by_signature: HashMap<&str, VecDeque<usize>> = HashMap::new();
    for arm in after_arms {
        if let Some(signature) = arm.signature.as_deref() {
            after_by_signature.entry(signature).or_default().push_back(arm.node_id);
        }
    }

    for arm in before_arms {
        let Some(signature) = arm.signature.as_deref() else { continue };
        let Some(queue) = after_by_signature.get_mut(signature) else { continue };
        let Some(after_arm_id) = queue.pop_front() else { continue };

        apted::for_nodes(
            before_metadata,
            after_metadata,
            vec![arm.node_id],
            vec![after_arm_id],
            Algorithm::Apted,
            "flow_control_arm",
            diff,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::{Code, Language};
    use crate::diff::ASTMappingOperation;
    use crate::test::helper::find_first_of_kind;

    #[test]
    fn rust_match_arms_with_shared_patterns_are_anchored_across_renamed_functions() {
        // `from_str` has no counterpart named function in `after` (it was renamed to `parse`), so
        // without this heuristic the whole function - including its match - would be swallowed
        // into a single delete+insert by the time the final APTED pass runs.
        // 8 shared patterns, one unique to each side: Jaccard = 8 / (8+1+1) = 0.8, comfortably
        // clearing the 0.75 threshold (mirrors the real-world fixture that motivated this
        // heuristic, where 9 of 10 patterns were shared).
        let before_src = r#"
fn from_str(s: &str) -> i32 {
    match s {
        "asset" => 1,
        "ecmascript" => 2,
        "typescript" => 3,
        "css" => 4,
        "css-module" => 5,
        "wasm" => 6,
        "raw" => 7,
        "node" => 8,
        "bytes" => 9,
        _ => 0,
    }
}
"#;
        let after_src = r#"
fn parse(s: &str) -> i32 {
    match s {
        "asset" => 10,
        "ecmascript" => 20,
        "typescript" => 30,
        "css" => 40,
        "css-module" => 50,
        "wasm" => 60,
        "raw" => 70,
        "node" => 80,
        "json" => 90,
        _ => 0,
    }
}
"#;
        let before = Code::from_string(before_src, &Language::Rust);
        let after = Code::from_string(after_src, &Language::Rust);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve(&before, &after, &node_cache, &mut diff);

        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();
        let before_match = find_first_of_kind(before_ast.root_node(), "match_expression").unwrap();
        let after_match = find_first_of_kind(after_ast.root_node(), "match_expression").unwrap();

        let mapping = diff
            .mapping
            .get(&(before_match.id(), after_match.id()))
            .expect("the two match_expressions should be mapped to each other");
        assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);

        // All 8 shared-pattern arms ("asset", "ecmascript", ...) should each be mapped, arm-for-
        // arm, even though their *values* (1 vs 10, etc.) differ.
        let before_arms = super::flow_control_arms(before_match, &Language::Rust, before.contents.as_bytes()).unwrap();
        let after_arms = super::flow_control_arms(after_match, &Language::Rust, after.contents.as_bytes()).unwrap();
        for signature in [
            "\"asset\"",
            "\"ecmascript\"",
            "\"typescript\"",
            "\"css\"",
            "\"css-module\"",
            "\"wasm\"",
            "\"raw\"",
            "\"node\"",
        ] {
            let before_arm = before_arms.iter().find(|a| a.signature.as_deref() == Some(signature)).unwrap();
            let after_arm = after_arms.iter().find(|a| a.signature.as_deref() == Some(signature)).unwrap();
            assert_eq!(
                diff.before_node_map.get(&before_arm.node_id),
                Some(&after_arm.node_id),
                "arm with pattern {signature} should be matched"
            );
        }
    }

    #[test]
    fn rust_if_chains_with_shared_conditions_are_anchored_across_renamed_functions() {
        let before_src = r#"
fn check(x: i32) -> i32 {
    if x > 100 {
        1
    } else if x > 10 {
        2
    } else if x > 0 {
        3
    } else {
        0
    }
}
"#;
        let after_src = r#"
fn validate(x: i32) -> i32 {
    if x > 100 {
        10
    } else if x > 10 {
        20
    } else if x > 0 {
        30
    } else {
        0
    }
}
"#;
        let before = Code::from_string(before_src, &Language::Rust);
        let after = Code::from_string(after_src, &Language::Rust);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve(&before, &after, &node_cache, &mut diff);

        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();
        let before_if = find_first_of_kind(before_ast.root_node(), "if_expression").unwrap();
        let after_if = find_first_of_kind(after_ast.root_node(), "if_expression").unwrap();

        let mapping = diff
            .mapping
            .get(&(before_if.id(), after_if.id()))
            .expect("the two if-chains should be mapped to each other");
        assert_eq!(mapping.operation, ASTMappingOperation::MatchButNotIdentical);

        // Confirm the whole chain - not just the outermost branch - was resolved: the second
        // `else if x > 10` branch, nested inside the first, should also be individually mapped.
        let before_arms = super::flow_control_arms(before_if, &Language::Rust, before.contents.as_bytes()).unwrap();
        let after_arms = super::flow_control_arms(after_if, &Language::Rust, after.contents.as_bytes()).unwrap();
        assert_eq!(
            diff.before_node_map.get(&before_arms[1].node_id),
            Some(&after_arms[1].node_id),
            "the nested `else if x > 10` branch should be mapped too"
        );
    }

    #[test]
    fn dissimilar_matches_are_not_paired() {
        let before_src = r#"
fn a(s: &str) -> i32 {
    match s {
        "one" => 1,
        "two" => 2,
        _ => 0,
    }
}
"#;
        let after_src = r#"
fn b(s: &str) -> i32 {
    match s {
        "totally" => 1,
        "different" => 2,
        "patterns" => 3,
        _ => 0,
    }
}
"#;
        let before = Code::from_string(before_src, &Language::Rust);
        let after = Code::from_string(after_src, &Language::Rust);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve(&before, &after, &node_cache, &mut diff);

        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();
        let before_match = find_first_of_kind(before_ast.root_node(), "match_expression").unwrap();
        let after_match = find_first_of_kind(after_ast.root_node(), "match_expression").unwrap();

        assert!(
            !diff.mapping.contains_key(&(before_match.id(), after_match.id())),
            "matches with no shared patterns should not be paired"
        );
    }

}
