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
use std::collections::HashMap;

use crate::code::{Code, ASTMetadata, Metadata};
use crate::code::language;
use crate::code::tip;
use crate::diff::reference_nodes;

/**
* Compute all metadata fileds, that can be computed without reading any new information.
*/
pub fn hermetic_expand(m: &mut Metadata) {
    if m.tip.is_none()
        && let Some(path) = &m.path
    {
        m.tip = tip::type_from_path(path.as_path());
    }

    if m.language.is_none()
        && let Some(path) = &m.path
    {
        m.language = language::language_for_path(path.as_path());
    }
}

/**
* Compute AST metadata for the given Code structure.
*
* This function creates a default ASTMetadata object and populates it by calling hash_code
* from hash.rs to compute both full and structural hashes for all nodes in the AST.
* It also discovers all reference nodes and orders them by subtree size.
*/
pub fn compute_ast_metadata(code: &Code) -> Result<ASTMetadata> {
    let mut metadata = ASTMetadata::default();
    crate::code::hash::hash_code(code, &mut metadata)?;
    // Discover all reference nodes and order them by subtree size
    discover_reference_nodes(code, &mut metadata)?;
    Ok(metadata)
}

/**
* Discover all reference nodes in the AST and order them by subtree size.
*
* This function traverses the AST to find all nodes that are considered reference nodes
* (as defined by is_reference_node), calculates their subtree sizes, and stores them
* in the metadata.reference_nodes_ordered vector, sorted by size in descending order.
*/
fn discover_reference_nodes(code: &Code, metadata: &mut ASTMetadata) -> Result<()> {
    let ast = code.ast.as_ref().expect("AST must be parsed");
    let root_node = ast.root_node();
    let language = code
        .metadata
        .language
        .as_ref()
        .expect("Language must be set");

    // Map to store subtree sizes for all nodes
    let mut node_to_subtree_size = HashMap::new();

    // Perform post-order traversal to compute subtree sizes efficiently
    let mut stack = Vec::new();
    stack.push((root_node, false)); // (node, processed)

    while let Some((node, processed)) = stack.pop() {
        if processed {
            // Post-order processing: compute subtree size
            let node_id = node.id();
            let mut size = 1; // Count this node itself

            // Add sizes of all children
            let mut child_cursor = node.walk();
            for child in node.children(&mut child_cursor) {
                if let Some(&child_size) = node_to_subtree_size.get(&child.id()) {
                    size += child_size;
                }
            }

            node_to_subtree_size.insert(node_id, size);
        } else {
            // Pre-order: push back as processed, then push children
            stack.push((node, true));

            // Push children in reverse order for proper traversal
            let mut child_cursor = node.walk();
            let children: Vec<_> = node.children(&mut child_cursor).collect();
            for child in children.into_iter().rev() {
                stack.push((child, false));
            }
        }
    }

    // Collect reference nodes with their subtree sizes
    let mut reference_nodes_with_sizes = Vec::new();

    // Traverse again to find reference nodes
    let mut stack = Vec::new();
    stack.push(root_node);

    while let Some(node) = stack.pop() {
        let node_id = node.id();

        // Check if this node is a reference node
        if reference_nodes::is_reference_node(node.kind(), language)
            && let Some(&subtree_size) = node_to_subtree_size.get(&node_id)
        {
            reference_nodes_with_sizes.push((node_id, subtree_size));
        }

        // Continue traversal - add children to stack
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }

    // Sort reference nodes by subtree size in descending order
    reference_nodes_with_sizes.sort_by(|a, b| b.1.cmp(&a.1));

    // Extract just the node IDs in order
    metadata.reference_nodes_ordered = reference_nodes_with_sizes
        .into_iter()
        .map(|(node_id, _)| node_id)
        .collect();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    #[test]
    fn hermetic_expand_from_path() {
        let mut m = Metadata {
            path: Some(PathBuf::from("/tmp/test/fake/test_value.cpp")),
            ..Default::default()
        };

        hermetic_expand(&mut m);

        assert!(m.tip.is_some());
        assert!(m.language.is_some());
    }
}
