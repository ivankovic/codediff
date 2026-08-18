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
use anyhow::{Context, Result};
use metrohash::MetroHash64;

use crate::code::similarity::SimilaritySketch;
use std::hash::Hasher;

use crate::code::{ASTMetadata, Code, Language};
use crate::diff::nodes::is_commutative_container;

/**
* Compute hashes for the given TreeSitter tree from the given root node.
*
* This function computes both full hashes and structural hashes for all nodes in the AST
* and populates the provided ASTMetadata structure.
*
* Full hashes include both the structure (node types) and the values of the nodes and their
* entire subtree, in order. This creates unique hashes for nodes with different content.
*
* Structural hashes include only the types of AST nodes in the subtree, not the values of the
* nodes. This creates hashes that are robust to changes like constant value changes.
*
* The metadata structure will be populated with:
*   - node_to_full_hash: Map from node IDs to full hashes
*   - full_hash_to_node: Reverse map from full hashes to node IDs (since multiple nodes can have
*     the same full hash), in the deterministic order this function visits them
*   - node_to_structural_hash: Map from node IDs to structural hashes
*   - structural_hash_to_node: Reverse map from structural hashes to node IDs, same ordering
*     guarantee as full_hash_to_node
*
* Note that TS Node IDs are semi-stable. The TS documentation goes into detail, but for our purpose
* they are stable between edits and re-parsing, and since we do neither we are ok.
*
* The aim is for the hash to have the following properties:
*   - Fast. Speed is of the essence. 99.999% of files in the full dataset should hash in under 50ms.
*   - Robust. The hash is used for duplicate detection so statistical properties must be robust.
*
* There is NO requirement for security. Crypto hashes are way too slow for our use case and
* reversing the hash is irrelevant, we return the reverse map anyhow.
*
* @param code The Code structure containing the AST to hash
* @param metadata Mutable reference to ASTMetadata that will be populated with hash data
*/
/// Inserts `node_id -> hash` into `forward` and appends `node_id` to `reverse`'s bucket for
/// `hash` - the same "store both directions of one hash map" pair `hash_code` below repeats once
/// per hash kind (full/structural/kind-and-value/kind-only).
fn record(
    forward: &mut rustc_hash::FxHashMap<usize, u64>,
    reverse: &mut rustc_hash::FxHashMap<u64, Vec<usize>>,
    node_id: usize,
    hash: u64,
) {
    forward.insert(node_id, hash);
    reverse.entry(hash).or_default().push(node_id);
}

pub fn hash_code(code: &Code, metadata: &mut ASTMetadata) -> Result<()> {
    // Clear existing data in the metadata
    metadata.node_to_full_hash.clear();
    metadata.full_hash_to_node.clear();
    metadata.node_to_structural_hash.clear();
    metadata.structural_hash_to_node.clear();
    metadata.node_to_kind_and_value_hash.clear();
    metadata.kind_and_value_hash_to_node.clear();
    metadata.node_to_kind_only_hash.clear();
    metadata.kind_only_hash_to_node.clear();
    metadata.node_to_similarity_sketch.clear();

    let ast = code
        .ast
        .as_ref()
        .context("AST must be parsed before hashing")?;
    let root_node = ast.root_node();
    let language = metadata.language;
    let source = code.contents.as_bytes();

    let mut cursor = root_node.walk();
    // Post-order traversal via an explicit stack: each node is pushed once "unexpanded" (to
    // discover and stack its children first) and, once popped a second time as "expanded", every
    // one of its descendants has already had its four hashes computed and stored below - so this
    // node's own hash can be computed by looking up each child's hash directly instead of
    // recursing back into it.
    //
    // This replaces an earlier version where each of the four `compute_*_hash` functions
    // recursed into every descendant itself, on every node, independent of the fact that the very
    // same subtree had just been fully hashed a moment earlier as part of its parent's own
    // computation - i.e. one call per node here, but each call doing O(that node's subtree size)
    // work, made the whole pass O(n * average nested-subtree size): quadratic or worse on a
    // deeply left/right-nested tree (long chained method calls, deeply nested conditionals - all
    // realistic in real code). Found during a 2026-07 code-health pass; confirmed empirically
    // before this fix (a synthetic deeply-nested expression went from 35ms at 616 nodes to 8.46s
    // at 9616 nodes) and after (linear in node count, matching this function's own "under 50ms"
    // goal documented above).
    //
    // Changing the traversal order does change which of several hash-colliding nodes ends up
    // first in `full_hash_to_node`/etc.'s per-hash `Vec` (previously a pre-order, rightmost-child-
    // first walk; now post-order) - confirmed safe: both real consumers of that ordering
    // (`solve_moved_subtrees.rs`, `hash_tree_matching.rs`) already explicitly re-sort by document
    // position/proximity rather than relying on raw insertion order, using it only as a last-
    // resort tiebreak for an exact-distance tie.
    let mut stack: Vec<(tree_sitter::Node, bool)> = vec![(root_node, false)];

    while let Some((node, expanded)) = stack.pop() {
        if !expanded {
            stack.push((node, true));
            for child in node.children(&mut cursor) {
                stack.push((child, false));
            }
            continue;
        }

        let node_id = node.id();
        let children: Vec<tree_sitter::Node> = node.children(&mut cursor).collect();

        // Every child already has all four hashes computed and stored (post-order guarantee) -
        // look them up (missing only if some pathological grammar reports children that aren't
        // fully behaved nodes; falling back to 0 keeps this a lookup, never a panic).
        let child_hash = |map: &rustc_hash::FxHashMap<usize, u64>, child: &tree_sitter::Node| {
            map.get(&child.id()).copied().unwrap_or(0)
        };
        let full_child_hashes: Vec<u64> = children
            .iter()
            .map(|c| child_hash(&metadata.node_to_full_hash, c))
            .collect();
        let structural_child_hashes: Vec<u64> = children
            .iter()
            .map(|c| child_hash(&metadata.node_to_structural_hash, c))
            .collect();
        let kind_and_value_child_hashes: Vec<u64> = children
            .iter()
            .map(|c| child_hash(&metadata.node_to_kind_and_value_hash, c))
            .collect();
        let kind_only_child_hashes: Vec<u64> = children
            .iter()
            .map(|c| child_hash(&metadata.node_to_kind_only_hash, c))
            .collect();

        let full_hash = compute_full_hash(&node, source, &children, &full_child_hashes);
        let structural_hash = compute_structural_hash(&node, &structural_child_hashes);
        let kind_and_value_hash = compute_kind_and_value_hash(
            &node,
            source,
            &children,
            &kind_and_value_child_hashes,
            language,
        );
        let kind_only_hash = compute_kind_only_hash(&node, &kind_only_child_hashes, language);

        record(
            &mut metadata.node_to_full_hash,
            &mut metadata.full_hash_to_node,
            node_id,
            full_hash,
        );
        record(
            &mut metadata.node_to_structural_hash,
            &mut metadata.structural_hash_to_node,
            node_id,
            structural_hash,
        );
        record(
            &mut metadata.node_to_kind_and_value_hash,
            &mut metadata.kind_and_value_hash_to_node,
            node_id,
            kind_and_value_hash,
        );
        record(
            &mut metadata.node_to_kind_only_hash,
            &mut metadata.kind_only_hash_to_node,
            node_id,
            kind_only_hash,
        );

        // The similarity sketch rides along on the same post-order guarantee as the four hashes
        // above: a leaf seeds a one-element sketch from its own full hash, an internal node merges
        // its children's already-computed sketches. `SimilaritySketch` has no reverse map, so it
        // isn't `record`ed. Children are merged in document order for determinism's sake, though
        // `merge` sorts and dedups and so is order-independent by construction anyway.
        //
        // The extra element for owned gap text is not an embellishment - without it the sketch is
        // blind on whole languages. In tree-sitter-yaml a double-quoted scalar's *leaves* are the
        // two quote characters and the string body sits in the gap between them, so six completely
        // different URLs in a `flow_sequence` all sketch to the identical one-element set (measured
        // 2026-08-18; every pairing scored 1.00). Any node owning non-whitespace text contributes
        // it, exactly like a leaf does, which is what makes "the set of content tokens in this
        // subtree" a faithful description of it rather than a description of its tokenization.
        let mut elements = Vec::with_capacity(children.len() + 1);
        if children.is_empty() {
            elements.push(SimilaritySketch::leaf(full_hash));
        } else {
            for child in &children {
                if let Some(sketch) = metadata.node_to_similarity_sketch.get(&child.id()) {
                    elements.push(sketch.clone());
                }
            }
            if let Some(own_text_hash) = compute_owned_text_hash(&node, source, &children) {
                elements.push(SimilaritySketch::leaf(own_text_hash));
            }
        }
        metadata
            .node_to_similarity_sketch
            .insert(node_id, SimilaritySketch::merge(elements));
    }

    Ok(())
}

/**
* Compute the full hash for a node: a Merkle hash over structure (kind, child count), each
* child's own hash, and the "gap" text directly owned by this node but not covered by any child
* (before the first child, between children, after the last child - and for a leaf, its entire
* span). A node's own span can't be hashed wholesale: it would include every byte between
* descendant tokens (indentation, newlines), making the hash change on pure reformatting (e.g. a
* block re-indented one level deeper) even though no token actually changed. But a gap isn't
* always just formatting - e.g. tree-sitter-r represents `"Hello, World!\n"` as a `string_content`
* node whose only *child* is the `\n` escape sequence, with "Hello, World!" itself sitting
* uncaptured in the gap before it - so gaps can't be dropped outright either. Splitting the
* difference: skip a gap only when it's *entirely* whitespace (safe to assume that's formatting,
* not content), otherwise hash it in full - never trimmed, since trimming would still lose real
* whitespace embedded inside otherwise-meaningful gap text (e.g. two files whose only difference
* is trailing spaces before a `\n` escape would trim down to the same gap and falsely collide).
*/
fn compute_full_hash(
    node: &tree_sitter::Node,
    source_code: &[u8],
    children: &[tree_sitter::Node],
    child_hashes: &[u64],
) -> u64 {
    let mut hasher = MetroHash64::new();

    // Hash node type and child count
    hasher.write(node.kind_id().to_le_bytes().as_slice());
    hasher.write(node.child_count().to_le_bytes().as_slice());

    let mut gap_start = node.start_byte();
    for (child, &child_hash) in children.iter().zip(child_hashes) {
        hash_gap(&mut hasher, source_code, gap_start, child.start_byte());
        hasher.write(child_hash.to_le_bytes().as_slice());
        gap_start = child.end_byte();
    }
    hash_gap(&mut hasher, source_code, gap_start, node.end_byte());

    hasher.finish()
}

/// A hash of the text an internal node owns directly - the gaps before, between and after its
/// children - or `None` when every one of those gaps is empty or pure formatting.
///
/// `compute_full_hash` folds the same gaps into a Merkle hash over the whole subtree; this pulls
/// them out on their own so [`crate::code::similarity`]'s leaf-set sketch can count them as content
/// tokens. Grammars differ on whether a scalar's body is a child node or gap text (tree-sitter-yaml
/// chooses the latter for quoted strings), and a similarity measure must not depend on which choice
/// a grammar made.
fn compute_owned_text_hash(
    node: &tree_sitter::Node,
    source_code: &[u8],
    children: &[tree_sitter::Node],
) -> Option<u64> {
    let mut hasher = MetroHash64::new();
    hasher.write(node.kind_id().to_le_bytes().as_slice());

    let mut any_content = false;
    let mut hash_if_content = |hasher: &mut MetroHash64, start: usize, end: usize| {
        if start < end
            && let Ok(text) = std::str::from_utf8(&source_code[start..end])
            && !text.trim().is_empty()
        {
            hasher.write(text.as_bytes());
            any_content = true;
        }
    };

    let mut gap_start = node.start_byte();
    for child in children {
        hash_if_content(&mut hasher, gap_start, child.start_byte());
        gap_start = child.end_byte();
    }
    hash_if_content(&mut hasher, gap_start, node.end_byte());

    any_content.then(|| hasher.finish())
}

/// Hashes `source[start..end]` into `hasher`, unless that span is empty or entirely whitespace
/// (formatting between/around structural children, not owned content - see `compute_full_hash`).
fn hash_gap(hasher: &mut MetroHash64, source: &[u8], start: usize, end: usize) {
    if start >= end {
        return;
    }
    if let Ok(text) = std::str::from_utf8(&source[start..end]) {
        if !text.trim().is_empty() {
            hasher.write(text.as_bytes());
        }
    }
}

/**
* Compute the structural hash for a node, including only the structure (node types).
* This is a recursive function that hashes only the node type and child structure,
* ignoring the actual values and positions.
*/
fn compute_structural_hash(node: &tree_sitter::Node, child_hashes: &[u64]) -> u64 {
    let mut hasher = MetroHash64::new();

    // Hash only node type and child count (structure), not position or values
    hasher.write(node.kind_id().to_le_bytes().as_slice());
    hasher.write(node.child_count().to_le_bytes().as_slice());

    for &child_hash in child_hashes {
        hasher.write(child_hash.to_le_bytes().as_slice());
    }

    hasher.finish()
}

/**
* `KindAndValueHash` - like `compute_full_hash` (kind, child count, gap text, each child's own
* hash, all in document order), but order-independence is checked at *every* recursion level via
* `is_commutative_container`, not bolted on as a separate third hash the way an earlier,
* now-removed `compute_commutative_structural_hash` was: that function's non-commutative branch
* delegated to plain `compute_structural_hash` instead of recursing back into itself, so order-
* invariance never propagated past the commutative container itself - an ancestor of a reordered
* container (e.g. the `enum_item` wrapping a reordered `enum_variant_list`) still hashed
* identically to its plain structural hash before/after the reorder, so a pass matching on the
* ancestor reference node (rather than the bare container) never actually fired for its documented
* use case. Recursing into *this same function* unconditionally, instead of falling back once
* outside a commutative container, fixes that by construction: a reordered commutative container's
* ancestors hash identically before/after too, so hash-descent matches them directly. This in turn
* requires `hash_tree_matching`'s descendant pairing to be commutative-aware (see
* `pair_children_for_descent`), or reordered children get re-mangled by a positional `zip`.
*/
fn compute_kind_and_value_hash(
    node: &tree_sitter::Node,
    source_code: &[u8],
    children: &[tree_sitter::Node],
    child_hashes: &[u64],
    language: Language,
) -> u64 {
    let mut hasher = MetroHash64::new();
    hasher.write(node.kind_id().to_le_bytes().as_slice());
    hasher.write(node.child_count().to_le_bytes().as_slice());

    if is_commutative_container(node.kind(), &language) {
        // Order-independent: sort (child_hash) pairs, drop gap text (gap order/identity is
        // itself a document-order artifact that doesn't make sense to preserve once children are
        // allowed to reorder).
        let mut sorted_hashes: Vec<u64> = child_hashes.to_vec();
        sorted_hashes.sort_unstable();
        for hash in sorted_hashes {
            hasher.write(hash.to_le_bytes().as_slice());
        }
    } else {
        let mut gap_start = node.start_byte();
        for (child, &child_hash) in children.iter().zip(child_hashes) {
            hash_gap(&mut hasher, source_code, gap_start, child.start_byte());
            hasher.write(child_hash.to_le_bytes().as_slice());
            gap_start = child.end_byte();
        }
        hash_gap(&mut hasher, source_code, gap_start, node.end_byte());
    }

    hasher.finish()
}

/**
* Six-phase pipeline rework (`TODO.md`, 2026-07-17): `KindOnlyHash` - like `compute_structural_hash`
* (kind, child count, each child's own hash; no leaf values, no gap text), with the same per-level
* `is_commutative_container` order-independence fix described on `compute_kind_and_value_hash`.
* Replaces both `compute_structural_hash` and the 4 `compute_normalized_*` variants for the new
* pipeline: those existed to bridge different granularities between "byte-identical" and "same
* shape, any leaf value" (ignore punctuation only, ignore literals only, ignore identifiers only,
* ignore both) - `KindOnlyHash` collapses all of that into the single coarsest tier (any leaf
* value, since leaf values aren't hashed at all), an accepted precision loss - see `TODO.md`.
*/
fn compute_kind_only_hash(
    node: &tree_sitter::Node,
    child_hashes: &[u64],
    language: Language,
) -> u64 {
    let mut hasher = MetroHash64::new();
    hasher.write(node.kind_id().to_le_bytes().as_slice());
    hasher.write(node.child_count().to_le_bytes().as_slice());

    let mut child_hashes = child_hashes.to_vec();
    if is_commutative_container(node.kind(), &language) {
        child_hashes.sort_unstable();
    }
    for hash in child_hashes {
        hasher.write(hash.to_le_bytes().as_slice());
    }

    hasher.finish()
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use std::collections::HashSet;

    use crate::test::helper;

    use super::*;

    #[test]
    fn hash_all_handmade_codes() -> Result<()> {
        let codes = helper::handmade_test_code()?;

        for (_, code) in codes {
            let mut metadata = ASTMetadata::default();
            hash_code(&code, &mut metadata)?;

            // Test full hashing
            assert!(!metadata.node_to_full_hash.is_empty());
            assert!(!metadata.full_hash_to_node.is_empty());

            // Verify that all nodes are covered in both directions
            assert_eq!(
                metadata.node_to_full_hash.len(),
                metadata
                    .full_hash_to_node
                    .values()
                    .map(|set| set.len())
                    .sum::<usize>()
            );

            // Test that each node's hash correctly maps back to a set containing that node
            for (node_id, hash) in &metadata.node_to_full_hash {
                if let Some(node_set) = metadata.full_hash_to_node.get(hash) {
                    assert!(
                        node_set.contains(node_id),
                        "Node {} with hash {} not found in reverse map",
                        node_id,
                        hash
                    );
                } else {
                    panic!(
                        "Hash {} from node {} not found in reverse map",
                        hash, node_id
                    );
                }
            }

            // Test structural hashing
            assert!(!metadata.node_to_structural_hash.is_empty());
            assert!(!metadata.structural_hash_to_node.is_empty());

            // Verify that all nodes are covered in both directions for structural hashing
            assert_eq!(
                metadata.node_to_structural_hash.len(),
                metadata
                    .structural_hash_to_node
                    .values()
                    .map(|set| set.len())
                    .sum::<usize>()
            );

            // Test that each node's structural hash correctly maps back to a set containing that node
            for (node_id, hash) in &metadata.node_to_structural_hash {
                if let Some(node_set) = metadata.structural_hash_to_node.get(hash) {
                    assert!(
                        node_set.contains(node_id),
                        "Node {} with structural hash {} not found in reverse map",
                        node_id,
                        hash
                    );
                } else {
                    panic!(
                        "Structural hash {} from node {} not found in reverse map",
                        hash, node_id
                    );
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_full_vs_structural_hashing() -> Result<()> {
        let codes = helper::handmade_test_code()?;

        // Test on all handmade code files
        for (filename, code) in &codes {
            let mut metadata = ASTMetadata::default();
            hash_code(code, &mut metadata)?;

            // Full hashes should be more unique than structural hashes
            let full_hash_count = metadata.full_hash_to_node.len();
            let structural_hash_count = metadata.structural_hash_to_node.len();

            // Structural hashes should generally have fewer unique values since they ignore content
            assert!(
                structural_hash_count <= full_hash_count,
                "For file {}: Structural hashes ({}) should be <= full hashes ({})",
                filename,
                structural_hash_count,
                full_hash_count
            );

            // Test that nodes with same structural hash can have different full hashes
            // (this happens when nodes have same structure but different content)
            let mut found_different_content_same_structure = false;

            for node_set in metadata.structural_hash_to_node.values() {
                if node_set.len() > 1 {
                    // Multiple nodes share the same structural hash
                    let mut full_hashes = HashSet::new();
                    for node_id in node_set {
                        if let Some(full_hash) = metadata.node_to_full_hash.get(node_id) {
                            full_hashes.insert(full_hash);
                        }
                    }

                    // If there are multiple full hashes for the same structural hash,
                    // it means we found nodes with same structure but different content
                    if full_hashes.len() > 1 {
                        found_different_content_same_structure = true;
                        break;
                    }
                }
            }

            // This should be true for most non-trivial code
            // (e.g., multiple string literals, different variable names, etc.)
            //
            // 20 was chosen so that the hello world in JavaScript and TypeScript are excluded,
            // since those are so trivial they don't actually pass this check.
            if metadata.node_to_full_hash.len() > 20 {
                assert!(
                    found_different_content_same_structure,
                    "For file {}: Expected to find nodes with same structure but different content in non-trivial code",
                    filename
                );
            }
        }

        Ok(())
    }

    #[test]
    fn test_identical_code_produces_same_hashes() -> Result<()> {
        let codes = helper::handmade_test_code()?;

        let code = codes
            .get("hello-world.rs")
            .ok_or_else(|| anyhow::anyhow!("Test file 'hello-world.rs' not found"))?;
        let mut metadata1 = ASTMetadata::default();
        let mut metadata2 = ASTMetadata::default();
        hash_code(code, &mut metadata1)?;
        hash_code(code, &mut metadata2)?;

        // Both full and structural hashes should be identical for identical code
        assert_eq!(metadata1.node_to_full_hash, metadata2.node_to_full_hash);
        assert_eq!(metadata1.full_hash_to_node, metadata2.full_hash_to_node);
        assert_eq!(
            metadata1.node_to_structural_hash,
            metadata2.node_to_structural_hash
        );
        assert_eq!(
            metadata1.structural_hash_to_node,
            metadata2.structural_hash_to_node
        );

        Ok(())
    }

    #[test]
    fn test_different_code_structural_similarity() -> Result<()> {
        let codes = helper::handmade_test_code()?;

        // Compare hello-world.rs with zdravo-svijete.rs (same structure, different string content)
        let code1 = codes
            .get("hello-world.rs")
            .ok_or_else(|| anyhow::anyhow!("Test file 'hello-world.rs' not found"))?;
        let code2 = codes
            .get("zdravo-svijete.rs")
            .ok_or_else(|| anyhow::anyhow!("Test file 'zdravo-svijete.rs' not found"))?;
        let mut metadata1 = ASTMetadata::default();
        let mut metadata2 = ASTMetadata::default();
        hash_code(code1, &mut metadata1)?;
        hash_code(code2, &mut metadata2)?;

        // Full hashes should be different (different value of the string constant)...
        assert_ne!(metadata1.node_to_full_hash, metadata2.node_to_full_hash);

        // ...but structural hashes should be the same (same distribution of hashes).
        // Compare the structural_hash_to_node maps by checking they have the same keys
        // and that each key maps to sets of the same size (same number of nodes per hash).
        assert_eq!(
            metadata1.structural_hash_to_node.len(),
            metadata2.structural_hash_to_node.len(),
            "Different number of unique structural hashes"
        );

        for (hash1, nodes1) in &metadata1.structural_hash_to_node {
            if let Some(nodes2) = metadata2.structural_hash_to_node.get(hash1) {
                assert_eq!(
                    nodes1.len(),
                    nodes2.len(),
                    "Different number of nodes for structural hash {:?}",
                    hash1
                );
            } else {
                panic!(
                    "Structural hash {:?} found in first code but not in second",
                    hash1
                );
            }
        }

        Ok(())
    }

    /// Benchmark function for hash_code performance
    /// This can be used for quick performance testing without criterion
    pub fn benchmark_hash_code(code: &Code, iterations: usize) -> Result<std::time::Duration> {
        use std::time::Instant;

        let start = Instant::now();

        for _ in 0..iterations {
            let mut metadata = ASTMetadata::default();
            hash_code(code, &mut metadata)?;
        }

        let duration = start.elapsed();
        Ok(duration)
    }

    #[test]
    fn test_benchmark_function_works() -> Result<()> {
        let codes = helper::handmade_test_code()?;

        let code = codes
            .get("hello-world.rs")
            .ok_or_else(|| anyhow::anyhow!("Test file 'hello-world.rs' not found"))?;

        // Test that benchmark function runs without error
        let duration = benchmark_hash_code(code, 1000)?;

        // Should complete in reasonable time (less than 2000 millisecond for 1000 iterations, or
        // 1 milliseconds per iteration)
        assert!(
            duration.as_millis() < 2000,
            "Benchmark took too long: {:?}",
            duration
        );

        // Duration should be measurable (greater than 0)
        assert!(
            duration.as_nanos() > 0,
            "Benchmark duration should be measurable"
        );

        Ok(())
    }
}
