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

use crate::code::metadata::NodeRecord;
use crate::code::{ASTMetadata, Code, Language};
use crate::diff::nodes::is_commutative_container;

/// Inserts `node_id -> hash` into `forward` and appends `node_id` to `reverse`'s bucket.
fn record_hash(
    forward: &mut rustc_hash::FxHashMap<usize, u64>,
    reverse: &mut rustc_hash::FxHashMap<u64, Vec<usize>>,
    node_id: usize,
    hash: u64,
) {
    forward.insert(node_id, hash);
    reverse.entry(hash).or_default().push(node_id);
}

/**
* Fills `metadata`'s four hash maps (full, structural, kind-and-value, kind-only; see
* [`ASTMetadata`]), their reverse maps, and the similarity sketches. Errors if `code` is unparsed.
*
* Speed matters (every file is hashed) and security does not, hence MetroHash. Node ids are only
* stable within one parse, which is all the maps need.
*/
pub fn hash_code(code: &Code, metadata: &mut ASTMetadata) -> Result<()> {
    let ast = code
        .ast
        .as_ref()
        .context("AST must be parsed before hashing")?;
    let nodes = crate::code::metadata::collect_nodes(ast.root_node());
    hash_nodes(
        &nodes,
        code.contents.as_bytes(),
        metadata.language,
        metadata,
    );
    Ok(())
}

/// [`hash_code`] over an already collected node table (see `metadata::collect_nodes`).
///
/// Visits the table in reverse preorder (post-order, children right to left), so each child's
/// hash is computed once and read by index: linear, not quadratic, on deep trees. That order is
/// also the order of the `*_hash_to_node` buckets, the last-resort tie-break of
/// `solve_moved_subtrees` and `hash_tree_matching`, so changing it changes diffs.
pub(crate) fn hash_nodes(
    nodes: &[NodeRecord],
    source: &[u8],
    language: Language,
    metadata: &mut ASTMetadata,
) {
    metadata.node_to_full_hash.clear();
    metadata.full_hash_to_node.clear();
    metadata.node_to_structural_hash.clear();
    metadata.structural_hash_to_node.clear();
    metadata.node_to_kind_and_value_hash.clear();
    metadata.kind_and_value_hash_to_node.clear();
    metadata.node_to_kind_only_hash.clear();
    metadata.kind_only_hash_to_node.clear();
    metadata.node_to_similarity_sketch.clear();

    let n = nodes.len();
    let mut full = vec![0u64; n];
    let mut structural = vec![0u64; n];
    let mut kind_and_value = vec![0u64; n];
    let mut kind_only = vec![0u64; n];

    for (index, record) in nodes.iter().enumerate().rev() {
        let full_child_hashes: Vec<u64> = record.children.iter().map(|&c| full[c]).collect();
        let structural_child_hashes: Vec<u64> =
            record.children.iter().map(|&c| structural[c]).collect();
        let kind_and_value_child_hashes: Vec<u64> =
            record.children.iter().map(|&c| kind_and_value[c]).collect();
        let kind_only_child_hashes: Vec<u64> =
            record.children.iter().map(|&c| kind_only[c]).collect();

        let full_hash = compute_full_hash(record, source, nodes, &full_child_hashes);
        let structural_hash = compute_structural_hash(record, &structural_child_hashes);
        let kind_and_value_hash = compute_kind_and_value_hash(
            record,
            source,
            nodes,
            &kind_and_value_child_hashes,
            language,
        );
        let kind_only_hash = compute_kind_only_hash(record, &kind_only_child_hashes, language);
        full[index] = full_hash;
        structural[index] = structural_hash;
        kind_and_value[index] = kind_and_value_hash;
        kind_only[index] = kind_only_hash;

        record_hash(
            &mut metadata.node_to_full_hash,
            &mut metadata.full_hash_to_node,
            record.id,
            full_hash,
        );
        record_hash(
            &mut metadata.node_to_structural_hash,
            &mut metadata.structural_hash_to_node,
            record.id,
            structural_hash,
        );
        record_hash(
            &mut metadata.node_to_kind_and_value_hash,
            &mut metadata.kind_and_value_hash_to_node,
            record.id,
            kind_and_value_hash,
        );
        record_hash(
            &mut metadata.node_to_kind_only_hash,
            &mut metadata.kind_only_hash_to_node,
            record.id,
            kind_only_hash,
        );

        // Owned gap text counts as a leaf: in tree-sitter-yaml a quoted scalar's leaves are just
        // its quotes, so without it every such string sketches identically.
        let mut elements = Vec::with_capacity(record.children.len() + 1);
        if record.children.is_empty() {
            elements.push(SimilaritySketch::leaf(full_hash));
        } else {
            for &child in &record.children {
                if let Some(sketch) = metadata.node_to_similarity_sketch.get(&nodes[child].id) {
                    elements.push(sketch.clone());
                }
            }
            if let Some(own_text_hash) = compute_owned_text_hash(record, source, nodes) {
                elements.push(SimilaritySketch::leaf(own_text_hash));
            }
        }
        metadata
            .node_to_similarity_sketch
            .insert(record.id, SimilaritySketch::merge(elements));
    }
}

/**
* The full hash: a Merkle hash of kind, child count, each child's hash, and the gap text this node
* owns around its children (a leaf's whole span).
*
* Gaps rather than the whole span, so reformatting keeps the hash; but a gap can be content
* (tree-sitter-r leaves a string's body outside its only child), so only an all-whitespace gap is
* skipped, and a kept gap is hashed untrimmed so embedded whitespace still counts.
*/
fn compute_full_hash(
    record: &NodeRecord,
    source_code: &[u8],
    nodes: &[NodeRecord],
    child_hashes: &[u64],
) -> u64 {
    let mut hasher = MetroHash64::new();

    hasher.write(record.kind_id.to_le_bytes().as_slice());
    hasher.write(record.children.len().to_le_bytes().as_slice());

    let mut gap_start = record.start_byte;
    for (&child, &child_hash) in record.children.iter().zip(child_hashes) {
        hash_gap(&mut hasher, source_code, gap_start, nodes[child].start_byte);
        hasher.write(child_hash.to_le_bytes().as_slice());
        gap_start = nodes[child].end_byte;
    }
    hash_gap(&mut hasher, source_code, gap_start, record.end_byte);

    hasher.finish()
}

/// A hash of the gap text an internal node owns around its children, or `None` when it is all
/// whitespace - the gaps alone, so the similarity sketch does not depend on whether a grammar made
/// a scalar's body a child or gap text.
fn compute_owned_text_hash(
    record: &NodeRecord,
    source_code: &[u8],
    nodes: &[NodeRecord],
) -> Option<u64> {
    let mut hasher = MetroHash64::new();
    hasher.write(record.kind_id.to_le_bytes().as_slice());

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

    let mut gap_start = record.start_byte;
    for &child in &record.children {
        hash_if_content(&mut hasher, gap_start, nodes[child].start_byte);
        gap_start = nodes[child].end_byte;
    }
    hash_if_content(&mut hasher, gap_start, record.end_byte);

    any_content.then(|| hasher.finish())
}

/// Hashes `source[start..end]` into `hasher` unless it is all whitespace; see `compute_full_hash`.
fn hash_gap(hasher: &mut MetroHash64, source: &[u8], start: usize, end: usize) {
    if start >= end {
        return;
    }
    if let Ok(text) = std::str::from_utf8(&source[start..end])
        && !text.trim().is_empty()
    {
        hasher.write(text.as_bytes());
    }
}

/// The structural hash: kinds and child counts only, in order.
fn compute_structural_hash(record: &NodeRecord, child_hashes: &[u64]) -> u64 {
    let mut hasher = MetroHash64::new();

    hasher.write(record.kind_id.to_le_bytes().as_slice());
    hasher.write(record.children.len().to_le_bytes().as_slice());

    for &child_hash in child_hashes {
        hasher.write(child_hash.to_le_bytes().as_slice());
    }

    hasher.finish()
}

/**
* Like `compute_full_hash`, but a commutative container's children are hashed in sorted order.
* Children's hashes come from this same function, so the order-independence reaches every
* ancestor: the `enum_item` around a reordered `enum_variant_list` keeps its hash. That requires
* `hash_tree_matching::pair_children_for_descent` to pair such children by hash, not position.
*/
fn compute_kind_and_value_hash(
    record: &NodeRecord,
    source_code: &[u8],
    nodes: &[NodeRecord],
    child_hashes: &[u64],
    language: Language,
) -> u64 {
    let mut hasher = MetroHash64::new();
    hasher.write(record.kind_id.to_le_bytes().as_slice());
    hasher.write(record.children.len().to_le_bytes().as_slice());

    if is_commutative_container(record.kind, &language) {
        // Gap text is a document-order artifact, so it is dropped here.
        let mut sorted_hashes: Vec<u64> = child_hashes.to_vec();
        sorted_hashes.sort_unstable();
        for hash in sorted_hashes {
            hasher.write(hash.to_le_bytes().as_slice());
        }
    } else {
        let mut gap_start = record.start_byte;
        for (&child, &child_hash) in record.children.iter().zip(child_hashes) {
            hash_gap(&mut hasher, source_code, gap_start, nodes[child].start_byte);
            hasher.write(child_hash.to_le_bytes().as_slice());
            gap_start = nodes[child].end_byte;
        }
        hash_gap(&mut hasher, source_code, gap_start, record.end_byte);
    }

    hasher.finish()
}

/**
* Like `compute_structural_hash`, with `compute_kind_and_value_hash`'s order-independence for
* commutative containers. The single "same shape, any leaf values" tier: coarser than separate
* ignore-identifiers / ignore-literals tiers, a deliberate precision trade.
*/
fn compute_kind_only_hash(record: &NodeRecord, child_hashes: &[u64], language: Language) -> u64 {
    let mut hasher = MetroHash64::new();
    hasher.write(record.kind_id.to_le_bytes().as_slice());
    hasher.write(record.children.len().to_le_bytes().as_slice());

    let mut child_hashes = child_hashes.to_vec();
    if is_commutative_container(record.kind, &language) {
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

            assert!(!metadata.node_to_full_hash.is_empty());
            assert!(!metadata.full_hash_to_node.is_empty());

            assert_eq!(
                metadata.node_to_full_hash.len(),
                metadata
                    .full_hash_to_node
                    .values()
                    .map(|set| set.len())
                    .sum::<usize>()
            );

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

            assert!(!metadata.node_to_structural_hash.is_empty());
            assert!(!metadata.structural_hash_to_node.is_empty());

            assert_eq!(
                metadata.node_to_structural_hash.len(),
                metadata
                    .structural_hash_to_node
                    .values()
                    .map(|set| set.len())
                    .sum::<usize>()
            );

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

        for (filename, code) in &codes {
            let mut metadata = ASTMetadata::default();
            hash_code(code, &mut metadata)?;

            let full_hash_count = metadata.full_hash_to_node.len();
            let structural_hash_count = metadata.structural_hash_to_node.len();

            assert!(
                structural_hash_count <= full_hash_count,
                "For file {}: Structural hashes ({}) should be <= full hashes ({})",
                filename,
                structural_hash_count,
                full_hash_count
            );

            let mut found_different_content_same_structure = false;

            for node_set in metadata.structural_hash_to_node.values() {
                if node_set.len() > 1 {
                    let mut full_hashes = HashSet::new();
                    for node_id in node_set {
                        if let Some(full_hash) = metadata.node_to_full_hash.get(node_id) {
                            full_hashes.insert(full_hash);
                        }
                    }

                    if full_hashes.len() > 1 {
                        found_different_content_same_structure = true;
                        break;
                    }
                }
            }

            // Above 20 nodes; the JavaScript and TypeScript hello worlds are too trivial.
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

    fn root_hashes(source: &str, language: Language) -> (u64, u64) {
        let code = Code::from_string(source, &language);
        let root = code.ast.as_ref().unwrap().root_node().id();
        let mut metadata = ASTMetadata {
            language,
            ..Default::default()
        };
        hash_code(&code, &mut metadata).unwrap();
        (
            metadata.node_to_full_hash[&root],
            metadata.node_to_kind_and_value_hash[&root],
        )
    }

    #[test]
    fn full_hash_ignores_reindentation() {
        let (flat, _) = root_hashes("fn f() {\n    x();\n}\n", Language::Rust);
        let (deeper, _) = root_hashes("fn f() {\n        x();\n}\n", Language::Rust);
        assert_eq!(flat, deeper);
    }

    #[test]
    fn full_hash_counts_string_content_a_grammar_leaves_in_a_gap() {
        // tree-sitter-r's `string_content` has the `\n` escape as its only child.
        let (hello, _) = root_hashes("x <- \"Hello, World!\\n\"\n", Language::R);
        let (other, _) = root_hashes("x <- \"Goodbye, World!\\n\"\n", Language::R);
        assert_ne!(hello, other);
    }

    #[test]
    fn kind_and_value_hash_of_an_ancestor_survives_reordering_a_commutative_container() {
        let (full_ab, kv_ab) = root_hashes("enum E { A, B }\n", Language::Rust);
        let (full_ba, kv_ba) = root_hashes("enum E { B, A }\n", Language::Rust);
        assert_ne!(full_ab, full_ba);
        assert_eq!(kv_ab, kv_ba);
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

        // Same structure, different string content.
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

        assert_ne!(metadata1.node_to_full_hash, metadata2.node_to_full_hash);

        // Node ids differ, so compare how many nodes share each structural hash.
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

        let duration = benchmark_hash_code(code, 1000)?;

        assert!(
            duration.as_millis() < 2000,
            "Benchmark took too long: {:?}",
            duration
        );

        assert!(
            duration.as_nanos() > 0,
            "Benchmark duration should be measurable"
        );

        Ok(())
    }
}
