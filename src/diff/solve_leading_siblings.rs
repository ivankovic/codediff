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
use crate::code::Code;
use crate::code::metadata::metadata_of;
use crate::diff::nodes::{is_comment, is_leading_modifier, map_identical_descendants};
use crate::diff::{ASTDiff, ASTMapping, ASTMappingOperation, ASTMappingReason, NodeCache};

/// Match comments and attribute/decorator modifiers that immediately precede already-matched
/// nodes, walking backward through however many consecutive ones there are.
///
/// Runs after the hash and structural matching passes, catching leading siblings early so the
/// slow tree edit distance pass has less work left - especially valuable for a modifier like
/// Rust's `#[cfg(test)]`, which has no name/identity of its own (`nodes::is_reference`/
/// `is_semantically_structural` don't cover it) and is typically byte-identical across every
/// occurrence in a file: with nothing here to anchor it, it falls all the way through to
/// `final_pass`'s real tree-edit-distance, which - facing hundreds of equal-cost candidates for
/// "which one is new" - has no reason to prefer the one actually nearest the real change
/// (confirmed against a live case: `rust-adding-to-a-list-of-identical-attributes-should-favour-
/// near-matches`, see that fixture's own doc comment). Anchoring off the already-matched
/// declaration this modifier precedes (typically matched by name in an earlier phase, so
/// unambiguous even when the modifier itself is one of hundreds of identical siblings)
/// sidesteps that ambiguity entirely instead of trying to resolve it after the fact.
///
/// Deliberately conservative in what counts as a "leading sibling" at all: only `nodes::
/// is_comment`/`nodes::is_leading_modifier` kinds, and only ones that are the immediate preceding
/// sibling of an already-matched node (or of a leading sibling this same walk just matched -
/// see below), unmatched themselves, and textually identical. This avoids matching a comment or
/// modifier that actually belongs to a different declaration.
///
/// Walks a whole chain, not just one hop: once a leading sibling matches, its own immediate
/// preceding sibling becomes the next candidate, so a declaration with several stacked modifiers
/// (or a modifier plus a doc comment above it) gets all of them, not just the one touching the
/// declaration directly. Stops at the first hop that fails (different kind, not a recognized
/// leading-sibling kind, already matched on either side, or text differs) - a failed hop never
/// causes an *earlier* successful hop in the same chain to be undone.
pub fn solve(before: &Code, after: &Code, node_cache: &NodeCache, diff: &mut ASTDiff) {
    let before_src = before.contents.as_bytes();
    let after_src = after.contents.as_bytes();
    let language = metadata_of(before).language;

    let current_mappings: Vec<(usize, usize)> =
        diff.before_node_map.iter().map(|(&k, &v)| (k, v)).collect();

    // Anchors worth walking back from: exactly those whose immediately-preceding sibling is an
    // unmatched comment/modifier, which is the necessary condition for the loop below to match
    // anything at all. Everything else is a provable no-op that would still pay two `node_cache`
    // lookups, two tree-sitter `prev_sibling` calls and two `node_map` probes to discover that.
    //
    // Worth building because this pass runs on *every* matched pair, and on a large,
    // mostly-unchanged file nearly all of them are matched - the pass was measured (2026-08-17) at
    // ~400ms of a 907ms diff on a 258k-node fixture whose actual edit was one string literal.
    // Building the set is a single O(n) walk whose per-node work is a kind check (a cheap `match`
    // on `&str`) and, only for the few nodes that pass it, one hash probe.
    //
    // Safe as a *superset* filter under this loop's live-mutation contract: matching only ever
    // adds entries to `before_node_map`, never removes them, so a node unmatched when this set is
    // built can only become matched later - which the in-loop checks still catch. It can never go
    // the other way and make a skipped anchor become useful mid-loop.
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

        // Each iteration's `diff.before_node_map`/`after_node_map` lookups are always live, not a
        // snapshot from before this loop started - required for correctness here, not just an
        // optimization: an earlier hop in *this* chain (or an earlier anchor's chain, earlier in
        // this same `current_mappings` loop) may have just matched the very node a later hop is
        // about to consider, and the "already matched" check below must see that.
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
                ASTMapping {
                    cost: 0, // Byte-identical text, so a true no-op match.
                    operation: ASTMappingOperation::Identical,
                    reason: ASTMappingReason::LeadingSibling,
                },
            );

            // The leading sibling's own text matched byte-for-byte, so every descendant (e.g. a
            // comment's `//`/`/*`/`*/` marker tokens, or an attribute's `#`/`[`/`]` delimiters) is
            // identical too - map them now. Without this, `PostorderIndexer` would prune the whole
            // subtree the moment it sees the leading sibling itself already mapped, leaving those
            // descendants with no mapping at all (not even a delete) for every later pass to find.
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
    use crate::test::helper::find_first_of_kind;

    #[test]
    fn test_is_comment_function() {
        // Test various comment node kinds
        assert!(is_comment("comment"));
        assert!(is_comment("line_comment"));
        assert!(is_comment("block_comment"));
        assert!(is_comment("js_comment"));

        // Test non-comment node kinds
        assert!(!is_comment("function_item"));
        assert!(!is_comment("identifier"));
        assert!(!is_comment("string"));
    }

    #[test]
    fn matches_leading_comment_of_an_unchanged_sibling_function() {
        // `hello` and its leading comment are unchanged, but `other`'s body gains an `if`
        // statement, changing its tree shape - so neither the hash nor the structural pass can
        // match the whole file as one unit; only `hello`'s `function_item` matches, via
        // `solve_hash_descent`. Its leading comment is a *sibling*, not a descendant, so it's
        // untouched by that match and left for this pass to pick up specifically, which we
        // verify via `ASTMappingReason::LeadingSibling`.
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

        // The comment node itself matching isn't enough - tree-sitter's Rust grammar gives
        // `line_comment` its own `//` child. If that child isn't also mapped, later passes can
        // never find it (its parent is already mapped, so `PostorderIndexer` prunes the whole
        // subtree), leaving it with no mapping at all.
        let before_root = before.ast.as_ref().unwrap().root_node();
        let before_marker =
            find_first_of_kind(before_root, "//").expect("before `//` token should exist");
        assert!(
            diff.before_node_map.contains_key(&before_marker.id()),
            "the comment's `//` marker token should also be mapped, not just the comment node"
        );
    }

    /// The case this generalization exists for: an attribute with no name/identity of its own
    /// (`nodes::is_reference` doesn't cover `attribute_item`), anchored off the already-matched
    /// `mod_item` it precedes rather than needing its own identity signal at all.
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

    /// Two stacked modifiers above one declaration: both should match, not just the one directly
    /// touching the declaration - the chain-walk must take a second hop.
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

    /// A leading attribute whose text actually changed must not match (falls through to a later
    /// pass instead) - and, just as importantly, must not block whatever's stacked *above* it from
    /// its own separate consideration in a future run of this pass (there's nothing above it here,
    /// but the point is this hop's failure doesn't panic or corrupt state for the rest of the
    /// chain-walk loop).
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
}
