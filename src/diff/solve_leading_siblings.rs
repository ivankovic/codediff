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
 *  You should have received a copy of the GNU Affero General License
 *  along with this program. If not, see <https://www.gnu.org/licenses/>.
 */
use crate::diff::PassCtx;
use crate::diff::nodes::{is_comment, is_leading_modifier, map_identical_descendants};
use crate::diff::{ASTDiff, ASTMapping, ASTMappingReason};

/// Matches unmatched comments and attribute/decorator modifiers that immediately precede matched
/// nodes and are textually identical, walking back through a chain of them.
///
/// Anchoring off the matched declaration is what makes an identity-less modifier like
/// `#[cfg(test)]` unambiguous; left to residual resolution it is one of hundreds of equal-cost
/// identical siblings (rust-adding-to-a-list-of-identical-attributes-should-favour-near-matches).
pub fn solve(ctx: &PassCtx, diff: &mut ASTDiff) {
    let (before, after, node_cache) = (ctx.before, ctx.after, ctx.node_cache);
    let before_src = before.contents.as_bytes();
    let after_src = after.contents.as_bytes();
    let language = ctx.language();

    let current_mappings: Vec<(usize, usize)> =
        diff.before_node_map.iter().map(|(&k, &v)| (k, v)).collect();

    // Anchors whose previous sibling is an unmatched comment/modifier; every other anchor is a
    // no-op, and this pass visits every matched pair. A valid superset filter because matching
    // only adds entries, so a skipped anchor can never become useful mid-loop.
    let candidate_anchors: rustc_hash::FxHashSet<usize> = node_cache
        .before
        .values()
        .filter(|node| {
            (is_comment(node.kind()) || is_leading_modifier(node.kind(), &language))
                && !diff.before_node_map.contains_key(&node.id())
        })
        .filter_map(|node| node.next_sibling().map(|next| next.id()))
        .collect();

    for (before_id, after_id) in current_mappings {
        // Skip if either node is 0 (delete/insert)
        if before_id == 0 || after_id == 0 {
            continue;
        }
        if !candidate_anchors.contains(&before_id) {
            continue;
        }

        let Some(&before_node) = node_cache.before.get(&before_id) else {
            continue;
        };

        let Some(&after_node) = node_cache.after.get(&after_id) else {
            continue;
        };

        let mut before_anchor = before_node;
        let mut after_anchor = after_node;

        // The node-map lookups must be live, not a snapshot: an earlier hop or chain may have just
        // matched the node this hop considers.
        while let (Some(before_prev), Some(after_prev)) =
            (before_anchor.prev_sibling(), after_anchor.prev_sibling())
        {
            if diff.before_node_map.contains_key(&before_prev.id())
                || diff.after_node_map.contains_key(&after_prev.id())
            {
                break;
            }
            if before_prev.kind() != after_prev.kind() {
                break;
            }
            if !(is_comment(before_prev.kind())
                || is_leading_modifier(before_prev.kind(), &language))
            {
                break;
            }

            let before_text = before_prev.utf8_text(before_src).unwrap_or("");
            let after_text = after_prev.utf8_text(after_src).unwrap_or("");
            if before_text != after_text {
                break;
            }

            diff.add_mapping(
                before_prev.id(),
                after_prev.id(),
                ASTMapping::identical(ASTMappingReason::LeadingSibling),
            );

            // Required: `PostorderIndexer` prunes below a mapped node, so unmapped marker tokens
            // (`//`, `#[`) would be left with no mapping at all.
            map_identical_descendants(before_prev, after_prev, diff);

            before_anchor = before_prev;
            after_anchor = after_prev;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::{Code, Language};
    use crate::diff::ASTMappingOperation;
    use crate::test::helper::find_first_of_kind;

    #[test]
    fn test_is_comment_function() {
        assert!(is_comment("comment"));
        assert!(is_comment("line_comment"));
        assert!(is_comment("block_comment"));
        assert!(is_comment("js_comment"));

        assert!(!is_comment("function_item"));
        assert!(!is_comment("identifier"));
        assert!(!is_comment("string"));
    }

    #[test]
    fn matches_leading_comment_of_an_unchanged_sibling_function() {
        // `other` changes shape, so only `hello` itself is matched by an earlier pass; its
        // comment is a sibling, left for this one.
        let before = Code::from_string(
            "// This is a comment\nfn hello() {\n    println!(\"Hello\");\n}\nfn other() {\n    1;\n}\n",
            &Language::Rust,
        );
        let after = Code::from_string(
            "// This is a comment\nfn hello() {\n    println!(\"Hello\");\n}\nfn other() {\n    if true {\n        2;\n    }\n}\n",
            &Language::Rust,
        );

        let diff = crate::diff::diff_code(&before, &after)
            .ast
            .expect("ast diff");

        let comment_mapping = diff
            .mapping
            .values()
            .find(|m| m.reason == ASTMappingReason::LeadingSibling)
            .expect("leading comment should be matched via LeadingSibling");
        assert_eq!(comment_mapping.operation, ASTMappingOperation::Identical);

        let before_root = before.ast.as_ref().unwrap().root_node();
        let before_marker =
            find_first_of_kind(before_root, "//").expect("before `//` token should exist");
        assert!(
            diff.before_node_map.contains_key(&before_marker.id()),
            "the comment's `//` marker token should also be mapped, not just the comment node"
        );
    }

    /// An identity-less attribute is anchored off the `mod_item` it precedes.
    #[test]
    fn matches_leading_attribute_of_an_unchanged_sibling_mod_item() {
        let before = Code::from_string(
            "#[cfg(test)]\nmod alpha;\n#[cfg(test)]\nmod beta;\n",
            &Language::Rust,
        );
        let after = Code::from_string(
            "#[cfg(test)]\nmod alpha;\n#[cfg(test)]\nmod gamma;\n#[cfg(test)]\nmod beta;\n",
            &Language::Rust,
        );

        let diff = crate::diff::diff_code(&before, &after)
            .ast
            .expect("ast diff");

        let leading_sibling_matches = diff
            .mapping
            .values()
            .filter(|m| m.reason == ASTMappingReason::LeadingSibling)
            .count();
        assert_eq!(
            leading_sibling_matches, 2,
            "both alpha's and beta's #[cfg(test)] should match via LeadingSibling, \
             independent of the newly-inserted gamma's own identical-looking attribute"
        );
    }

    #[test]
    fn matches_a_chain_of_two_leading_attributes() {
        let before = Code::from_string(
            "#[cfg(test)]\n#[allow(dead_code)]\nmod alpha;\nfn other() {}\n",
            &Language::Rust,
        );
        let after = Code::from_string(
            "#[cfg(test)]\n#[allow(dead_code)]\nmod alpha;\nfn other() {\n    1;\n}\n",
            &Language::Rust,
        );

        let diff = crate::diff::diff_code(&before, &after)
            .ast
            .expect("ast diff");

        let leading_sibling_matches = diff
            .mapping
            .values()
            .filter(|m| m.reason == ASTMappingReason::LeadingSibling)
            .count();
        assert_eq!(
            leading_sibling_matches, 2,
            "both stacked attributes above `mod alpha` should match, via two chain hops"
        );
    }

    #[test]
    fn does_not_match_a_changed_leading_attribute() {
        let before = Code::from_string("#[cfg(test)]\nmod alpha;\n", &Language::Rust);
        let after = Code::from_string("#[cfg(not(test))]\nmod alpha;\n", &Language::Rust);

        let diff = crate::diff::diff_code(&before, &after)
            .ast
            .expect("ast diff");

        assert!(
            diff.mapping
                .values()
                .all(|m| m.reason != ASTMappingReason::LeadingSibling),
            "a changed attribute must not be matched as an identical leading sibling"
        );
    }

    #[test]
    fn leading_sibling_chain_stops_at_first_text_mismatch_and_keeps_earlier_hops() {
        let before = Code::from_string(
            "#[cfg(test)]\n#[allow(dead_code)]\nmod alpha;\nfn other() {}\n",
            &Language::Rust,
        );
        let after = Code::from_string(
            "#[cfg(not(test))]\n#[allow(dead_code)]\nmod alpha;\nfn other() {\n    1;\n}\n",
            &Language::Rust,
        );

        let diff = crate::diff::diff_code(&before, &after)
            .ast
            .expect("ast diff");

        let leading_sibling_matches = diff
            .mapping
            .values()
            .filter(|m| m.reason == ASTMappingReason::LeadingSibling)
            .count();
        assert_eq!(leading_sibling_matches, 1);
    }
}
