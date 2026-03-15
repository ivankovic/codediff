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
use anyhow::Result;
use metrohash::MetroHash64;
use std::collections::{HashMap, HashSet};
use std::hash::Hasher;

use crate::code::Code;
use crate::diff::ASTMetadata;

/**
* Compute hashes for the given TreeSitter tree from the given root node.
*
* This function computes both full hashes and structural hashes for all nodes in the AST.
*
* Full hashes include both the structure (node types) and the values of the nodes and their
* entire subtree, in order. This creates unique hashes for nodes with different content.
*
* Structural hashes include only the types of AST nodes in the subtree, not the values of the
* nodes. This creates hashes that are robust to changes like constant value changes.
*
* The result is an ASTMetadata structure containing:
*   - node_to_full_hash: Map from node IDs to full hashes
*   - full_hash_to_node: Reverse map from full hashes to sets of node IDs (since multiple nodes
*     can have the same full hash)
*   - node_to_structural_hash: Map from node IDs to structural hashes
*   - structural_hash_to_node: Reverse map from structural hashes to sets of node IDs
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
*/
pub fn hash_code(code: &Code) -> Result<ASTMetadata> {
    let mut metadata = ASTMetadata {
        node_to_full_hash: HashMap::new(),
        full_hash_to_node: HashMap::new(),
        node_to_structural_hash: HashMap::new(),
        structural_hash_to_node: HashMap::new(),
    };

    let ast = code
        .ast
        .as_ref()
        .expect("AST must be parsed before hashing");
    let root_node = ast.root_node();

    let mut cursor = root_node.walk();
    let mut stack = Vec::new();
    stack.push(root_node);

    while let Some(node) = stack.pop() {
        let node_id = node.id();

        // Compute full hash for this node (includes structure and values)
        let full_hash = compute_full_hash(
            &node,
            &mut cursor,
            &metadata.node_to_full_hash,
            &code.contents,
        );

        // Compute structural hash for this node (includes only structure, not values)
        let structural_hash = compute_structural_hash(&node, &mut cursor);

        // Store full hash mappings
        metadata.node_to_full_hash.insert(node_id, full_hash);
        metadata
            .full_hash_to_node
            .entry(full_hash)
            .or_insert_with(HashSet::new)
            .insert(node_id);

        // Store structural hash mappings
        metadata
            .node_to_structural_hash
            .insert(node_id, structural_hash);
        metadata
            .structural_hash_to_node
            .entry(structural_hash)
            .or_insert_with(HashSet::new)
            .insert(node_id);

        // Push children to stack for processing
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }

    Ok(metadata)
}

/**
* Compute the full hash for a node, including both structure and values.
* This is a recursive function that hashes the node type, text content, and all children.
*/
fn compute_full_hash<'a>(
    node: &tree_sitter::Node<'a>,
    cursor: &mut tree_sitter::TreeCursor<'a>,
    node_to_hash: &HashMap<usize, u64>,
    source_code: &str,
) -> u64 {
    let mut hasher = MetroHash64::new();

    // Hash node type and child count
    hasher.write(node.kind_id().to_le_bytes().as_slice());
    hasher.write(node.child_count().to_le_bytes().as_slice());

    // Hash the actual text content of the node
    if let Ok(text) = node.utf8_text(source_code.as_bytes()) {
        hasher.write(text.as_bytes());
    }

    // Add children hashes to the hash (if any)
    for child in node.children(cursor) {
        if let Some(&child_hash) = node_to_hash.get(&child.id()) {
            hasher.write(child_hash.to_le_bytes().as_slice());
        }
    }

    hasher.finish()
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

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::test::helper;

    use super::*;

    #[test]
    fn hash_all_handmade_codes() -> Result<()> {
        let codes = helper::handmade_test_code()?;

        for (_, code) in codes {
            let metadata = hash_code(&code)?;

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
            let metadata = hash_code(code)?;

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

            for (_, node_set) in &metadata.structural_hash_to_node {
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
            if metadata.node_to_full_hash.len() > 10 {
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
        let metadata1 = hash_code(code)?;
        let metadata2 = hash_code(code)?;

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
        let metadata1 = hash_code(code1)?;
        let metadata2 = hash_code(code2)?;

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
}
