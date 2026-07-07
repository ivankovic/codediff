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
use std::collections::{HashMap, VecDeque};

use tree_sitter::Node;

use crate::code::{ASTMetadata, Code, Language};
use crate::diff::apted::{self, Algorithm};
use crate::diff::nodes::{FlowControlArm, flow_control_arms, flow_control_family, flow_control_similarity};
use crate::diff::{ASTDiff, NodeCache};

/**
* MatchSimilarFlowControl: pairs up still-unmatched `if`/`switch`/`match` constructs whose arms
* (patterns, case labels, or `if`/`else if` conditions) are mostly the same, so a reviewer sees
* "this is basically the same branching logic" instead of the whole construct being torn down as a
* delete+insert.
*
* This runs after `solve_semantically_structural_nodes` (name-based matching) and before the final
* APTED pass. Without it, a construct whose enclosing function/impl has no same-named counterpart
* on the other side is left for the final pass to swallow into a from-scratch subtree delete/insert,
* even when its arms are clearly the same set of cases, just moved to a differently-named container
* (e.g. logic hoisted from one function into another as part of a rename/restructure).
*
* See [`crate::diff::nodes::FlowControlFamily`] for exactly which constructs are recognized per
* language (Python's `if` is the one notable gap - its flat `elif`/`else` fields don't fit the
* recursive `if`/`else if` shape this pass relies on). Candidates are scored by the Jaccard
* similarity of their non-wildcard arm signatures and paired off greedily, highest score first;
* accepted pairs (>=75% shared signatures) have their identical-signature arms individually
* resolved via APTED, then the container itself is diffed so any leftover value expression, added
* arms or removed arms are handled normally. For `if` chains specifically, this per-arm resolution
* is naturally recursive: a matched non-terminal branch's own subtree already contains the rest of
* the chain, so resolving it resolves every branch nested inside it too.
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

    let before_candidates: Vec<(Node, Vec<FlowControlArm>)> =
        collect_unmatched_containers(before_ast.root_node(), &language, &diff.before_node_map)
            .into_iter()
            .filter_map(|node| flow_control_arms(node, &language, before_source).map(|arms| (node, arms)))
            .collect();
    let after_candidates: Vec<(Node, Vec<FlowControlArm>)> =
        collect_unmatched_containers(after_ast.root_node(), &language, &diff.after_node_map)
            .into_iter()
            .filter_map(|node| flow_control_arms(node, &language, after_source).map(|arms| (node, arms)))
            .collect();
    if before_candidates.is_empty() || after_candidates.is_empty() {
        return;
    }

    // Score every same-family candidate pair, keep the ones clearing the threshold.
    let mut scored_pairs: Vec<(f64, usize, usize)> = Vec::new();
    for (before_idx, (before_node, before_arms)) in before_candidates.iter().enumerate() {
        let before_family = flow_control_family(before_node.kind(), &language);
        for (after_idx, (after_node, after_arms)) in after_candidates.iter().enumerate() {
            if before_family != flow_control_family(after_node.kind(), &language) {
                continue;
            }
            let score = flow_control_similarity(before_arms, after_arms);
            if score >= SIMILARITY_THRESHOLD {
                scored_pairs.push((score, before_idx, after_idx));
            }
        }
    }
    // Greedy one-to-one assignment, highest-similarity pair first.
    scored_pairs.sort_by(|a, b| b.0.total_cmp(&a.0));

    let mut before_used = vec![false; before_candidates.len()];
    let mut after_used = vec![false; after_candidates.len()];

    for (_, before_idx, after_idx) in scored_pairs {
        if before_used[before_idx] || after_used[after_idx] {
            continue;
        }
        before_used[before_idx] = true;
        after_used[after_idx] = true;

        let (before_node, before_arms) = &before_candidates[before_idx];
        let (after_node, after_arms) = &after_candidates[after_idx];

        anchor_matching_arms(before_arms, after_arms, &before_metadata, &after_metadata, diff);

        let _ = apted::for_nodes(
            &before_metadata,
            &after_metadata,
            vec![before_node.id()],
            vec![after_node.id()],
            Algorithm::ZhangShasha,
            diff,
        );
    }
}

/// Walks `root`'s subtree collecting every node that is a recognized flow-control container (see
/// [`flow_control_family`]) and not already mapped in `mapped`. Doesn't descend into already-mapped
/// nodes - their contents are presumed already resolved by an earlier pass, so there's nothing left
/// here for this heuristic to usefully claim.
fn collect_unmatched_containers<'a>(
    root: Node<'a>,
    language: &Language,
    mapped: &HashMap<usize, usize>,
) -> Vec<Node<'a>> {
    let mut result = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if mapped.contains_key(&node.id()) {
            continue;
        }
        if flow_control_family(node.kind(), language).is_some() {
            result.push(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    result
}

/// Resolves each before-arm against the first not-yet-claimed after-arm with an identical
/// (non-wildcard) signature, via a dedicated `apted::for_nodes` call per pair - the same idiom
/// `solve_semantically_structural_nodes` uses to pre-match same-named methods before diffing their
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

        let _ = apted::for_nodes(
            before_metadata,
            after_metadata,
            vec![arm.node_id],
            vec![after_arm_id],
            Algorithm::ZhangShasha,
            diff,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::Code;
    use crate::diff::ASTMappingOperation;

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
        let before_match = find_first(before_ast.root_node(), "match_expression").unwrap();
        let after_match = find_first(after_ast.root_node(), "match_expression").unwrap();

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
        let before_if = find_first(before_ast.root_node(), "if_expression").unwrap();
        let after_if = find_first(after_ast.root_node(), "if_expression").unwrap();

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
        let before_match = find_first(before_ast.root_node(), "match_expression").unwrap();
        let after_match = find_first(after_ast.root_node(), "match_expression").unwrap();

        assert!(
            !diff.mapping.contains_key(&(before_match.id(), after_match.id())),
            "matches with no shared patterns should not be paired"
        );
    }

    fn find_first<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
        if node.kind() == kind {
            return Some(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = find_first(child, kind) {
                return Some(found);
            }
        }
        None
    }
}
