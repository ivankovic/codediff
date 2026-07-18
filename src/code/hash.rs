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

    let ast = code
        .ast
        .as_ref()
        .context("AST must be parsed before hashing")?;
    let root_node = ast.root_node();
    let language = metadata.language;

    let mut cursor = root_node.walk();
    let mut stack = Vec::new();
    stack.push(root_node);

    while let Some(node) = stack.pop() {
        let node_id = node.id();

        // Compute full hash for this node (includes structure and values)
        let full_hash = compute_full_hash(&node, &mut cursor, code.contents.as_bytes());

        // Compute structural hash for this node (includes only structure, not values)
        let structural_hash = compute_structural_hash(&node, &mut cursor);

        // Order-independence (per is_commutative_container) folded in at every recursion level,
        // not bolted on as a third separate hash - see each function's doc comment.
        let kind_and_value_hash = compute_kind_and_value_hash(&node, &mut cursor, code.contents.as_bytes(), language);
        let kind_only_hash = compute_kind_only_hash(&node, &mut cursor, language);

        // Store full hash mappings
        metadata.node_to_full_hash.insert(node_id, full_hash);
        metadata
            .full_hash_to_node
            .entry(full_hash)
            .or_default()
            .push(node_id);

        // Store structural hash mappings
        metadata
            .node_to_structural_hash
            .insert(node_id, structural_hash);
        metadata
            .structural_hash_to_node
            .entry(structural_hash)
            .or_default()
            .push(node_id);

        metadata.node_to_kind_and_value_hash.insert(node_id, kind_and_value_hash);
        metadata
            .kind_and_value_hash_to_node
            .entry(kind_and_value_hash)
            .or_default()
            .push(node_id);
        metadata.node_to_kind_only_hash.insert(node_id, kind_only_hash);
        metadata
            .kind_only_hash_to_node
            .entry(kind_only_hash)
            .or_default()
            .push(node_id);

        // Push children to stack for processing
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
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
fn compute_full_hash<'a>(
    node: &tree_sitter::Node<'a>,
    cursor: &mut tree_sitter::TreeCursor<'a>,
    source_code: &[u8],
) -> u64 {
    let mut hasher = MetroHash64::new();

    // Hash node type and child count
    hasher.write(node.kind_id().to_le_bytes().as_slice());
    hasher.write(node.child_count().to_le_bytes().as_slice());

    // We need to collect children first to avoid borrowing issues (same as compute_structural_hash).
    let children: Vec<tree_sitter::Node> = node.children(cursor).collect();
    let mut gap_start = node.start_byte();
    for child in &children {
        hash_gap(&mut hasher, source_code, gap_start, child.start_byte());
        let child_hash = compute_full_hash(child, cursor, source_code);
        hasher.write(child_hash.to_le_bytes().as_slice());
        gap_start = child.end_byte();
    }
    hash_gap(&mut hasher, source_code, gap_start, node.end_byte());

    hasher.finish()
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
fn compute_structural_hash<'a>(
    node: &tree_sitter::Node<'a>,
    cursor: &mut tree_sitter::TreeCursor<'a>,
) -> u64 {
    let mut hasher = MetroHash64::new();

    // Hash only node type and child count (structure), not position or values
    hasher.write(node.kind_id().to_le_bytes().as_slice());
    hasher.write(node.child_count().to_le_bytes().as_slice());

    // Compute children structural hashes recursively (if any)
    // We need to collect children first to avoid borrowing issues
    let children: Vec<tree_sitter::Node> = node.children(cursor).collect();
    for child in children {
        let child_hash = compute_structural_hash(&child, cursor);
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
fn compute_kind_and_value_hash<'a>(
    node: &tree_sitter::Node<'a>,
    cursor: &mut tree_sitter::TreeCursor<'a>,
    source_code: &[u8],
    language: Language,
) -> u64 {
    let mut hasher = MetroHash64::new();
    hasher.write(node.kind_id().to_le_bytes().as_slice());
    hasher.write(node.child_count().to_le_bytes().as_slice());

    let children: Vec<tree_sitter::Node> = node.children(cursor).collect();

    if is_commutative_container(node.kind(), &language) {
        // Order-independent: sort (child_hash) pairs, drop gap text (gap order/identity is
        // itself a document-order artifact that doesn't make sense to preserve once children are
        // allowed to reorder).
        let mut child_hashes: Vec<u64> = children
            .iter()
            .map(|child| compute_kind_and_value_hash(child, cursor, source_code, language))
            .collect();
        child_hashes.sort_unstable();
        for hash in child_hashes {
            hasher.write(hash.to_le_bytes().as_slice());
        }
    } else {
        let mut gap_start = node.start_byte();
        for child in &children {
            hash_gap(&mut hasher, source_code, gap_start, child.start_byte());
            let child_hash = compute_kind_and_value_hash(child, cursor, source_code, language);
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
fn compute_kind_only_hash<'a>(
    node: &tree_sitter::Node<'a>,
    cursor: &mut tree_sitter::TreeCursor<'a>,
    language: Language,
) -> u64 {
    let mut hasher = MetroHash64::new();
    hasher.write(node.kind_id().to_le_bytes().as_slice());
    hasher.write(node.child_count().to_le_bytes().as_slice());

    let children: Vec<tree_sitter::Node> = node.children(cursor).collect();
    let mut child_hashes: Vec<u64> =
        children.iter().map(|child| compute_kind_only_hash(child, cursor, language)).collect();

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
