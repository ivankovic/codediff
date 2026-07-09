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

use crate::code::language;
use crate::code::tip;
use crate::code::{ASTMetadata, ASTNodeMetadata, Code, Metadata};
use crate::diff::nodes;

/// Compute the number of columns (characters) in each row of the given code.
///
/// For each line in the code, this function counts the number of characters before each newline.
/// The last line (which may not end with a newline) is also included.
///
/// # Arguments
///
/// * `contents` - The code contents as a string slice
///
/// # Returns
///
/// A vector where each element represents the number of columns (characters) in the corresponding row.
pub fn compute_columns_per_row(contents: &str) -> Vec<usize> {
    let mut result = Vec::new();
    let mut start = 0;

    for (idx, ch) in contents.char_indices() {
        if ch == '\n' {
            // Count characters from start to this newline
            let count = contents[start..idx].chars().count();
            result.push(count);
            start = idx + 1;
        }
    }

    // Handle the last line (which may not end with a newline)
    if start < contents.len() {
        let count = contents[start..].chars().count();
        result.push(count);
    } else if result.is_empty() {
        // Handle empty string case
        result.push(0);
    }

    result
}

/**
* Compute all metadata fields, that can be computed without reading any new information.
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
    metadata.language = code.metadata.language.unwrap_or_default();
    crate::code::hash::hash_code(code, &mut metadata)?;
    compute_subtree_sizes(code, &mut metadata)?;
    compute_node_info(code, &mut metadata)?;
    compute_node_depths(code, &mut metadata)?;
    compute_node_parents(&mut metadata);
    discover_reference_nodes(code, &mut metadata)?;
    discover_semantic_structure_nodes(code, &mut metadata)?;
    Ok(metadata)
}

/**
* Borrow `code`'s AST metadata if it has already been computed, computing a fresh (owned) copy
* only when it hasn't.
*
* Every diff pass needs both sides' metadata; before this helper each pass deep-cloned the whole
* `ASTMetadata` (several whole-tree HashMaps) per side just to sidestep borrow bookkeeping. In the
* normal pipeline the metadata is always already present, so this is a plain borrow and costs
* nothing. A `Code` that failed to parse yields the default (empty) metadata, matching the
* fail-safe convention documented on `Diff`.
*/
pub fn metadata_of(code: &Code) -> std::borrow::Cow<'_, ASTMetadata> {
    match &code.metadata.ast_metadata {
        Some(metadata) => std::borrow::Cow::Borrowed(metadata),
        None => std::borrow::Cow::Owned(compute_ast_metadata(code).unwrap_or_default()),
    }
}

/// Populates `node_to_parent` from `node_info`'s children lists. Must run after
/// `compute_node_info`.
fn compute_node_parents(metadata: &mut ASTMetadata) {
    for (&id, info) in &metadata.node_info {
        for &child in &info.children {
            metadata.node_to_parent.insert(child, id);
        }
    }
}

fn compute_subtree_sizes(code: &Code, metadata: &mut ASTMetadata) -> Result<()> {
    let ast = code.ast.as_ref().expect("AST must be parsed");
    let root_node = ast.root_node();

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
                if let Some(&child_size) = metadata.node_to_subtree_size.get(&child.id()) {
                    size += child_size;
                }
            }

            metadata.node_to_subtree_size.insert(node_id, size);
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

    Ok(())
}

/// Compute node information (kind, text, children) for all nodes
fn compute_node_info(code: &Code, metadata: &mut ASTMetadata) -> Result<()> {
    let ast = code.ast.as_ref().expect("AST must be parsed");
    let root_node = ast.root_node();

    let mut stack = Vec::new();
    stack.push(root_node);
    let mut preorder_index = 0usize;

    while let Some(node) = stack.pop() {
        let node_id = node.id();
        let kind = node.kind().to_string();
        let text = node
            .utf8_text(code.contents.as_bytes())
            .unwrap_or("")
            .to_string();

        // Get children IDs
        let mut child_cursor = node.walk();
        let children: Vec<usize> = node.children(&mut child_cursor).map(|c| c.id()).collect();

        metadata.node_info.insert(
            node_id,
            ASTNodeMetadata {
                kind,
                text,
                children,
                start_byte: node.start_byte(),
                preorder_index,
            },
        );
        preorder_index += 1;

        // Push children in reverse so the stack (LIFO) pops them back out left-to-right, keeping
        // `preorder_index` a genuine preorder (root, then children in document order).
        let mut child_cursor = node.walk();
        for child in node.children(&mut child_cursor).collect::<Vec<_>>().into_iter().rev() {
            stack.push(child);
        }
    }

    Ok(())
}

/// Compute the depth of every node (root = 0, its children = 1, ...).
fn compute_node_depths(code: &Code, metadata: &mut ASTMetadata) -> Result<()> {
    let ast = code.ast.as_ref().expect("AST must be parsed");

    let mut stack = vec![(ast.root_node(), 0)];
    while let Some((node, depth)) = stack.pop() {
        metadata.node_to_depth.insert(node.id(), depth);

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push((child, depth + 1));
        }
    }

    Ok(())
}

/**
* Discover all reference nodes in the AST and order them by subtree size.
*
* Reference nodes are nodes that humans use to "think about code". Prioritizing matching reference
* nodes results in diffs that "make sense" to humans.
*
* To speed up the algorithm, we sort the nodes by tree size.
*/
fn discover_reference_nodes(code: &Code, metadata: &mut ASTMetadata) -> Result<()> {
    let ast = code.ast.as_ref().expect("AST must be parsed");
    let root_node = ast.root_node();
    let language = code
        .metadata
        .language
        .as_ref()
        .expect("Language must be set");

    // Collect reference nodes with their subtree sizes
    let mut reference_nodes_with_sizes = Vec::new();

    let mut stack = Vec::new();
    stack.push(root_node);

    while let Some(node) = stack.pop() {
        let node_id = node.id();

        // Check if this node is a reference node
        if nodes::is_reference(node.kind(), language)
            && let Some(&subtree_size) = metadata.node_to_subtree_size.get(&node_id)
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

/**
* Discover all semantically structural nodes in the AST.
*
* semantically structural nodes are nodes that have a semantic meaning that is "loosely fixed" and
* typically enforced by the compiler in some way. For example, there can only ever be ONE
* 'fn main()' in main.rs in a Rust project. It is sensible for the algorithm to match such nodes
* immediately.
*/
fn discover_semantic_structure_nodes(code: &Code, metadata: &mut ASTMetadata) -> Result<()> {
    let ast = code.ast.as_ref().expect("AST must be parsed");
    let root_node = ast.root_node();
    let language = code
        .metadata
        .language
        .as_ref()
        .expect("Language must be set");

    let mut stack = Vec::new();
    stack.push(root_node);

    while let Some(node) = stack.pop() {
        let node_id = node.id();

        // Check if this node is semantically structural
        if let Some(t) = nodes::is_semantically_structural(&node, language, code) {
            metadata.semantically_structural_nodes.insert(t, node_id);
        }

        // Continue traversal - add children to stack
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }

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

    #[test]
    fn compute_columns_per_row_empty_string() {
        let result = compute_columns_per_row("");
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn compute_columns_per_row_single_line() {
        let result = compute_columns_per_row("hello");
        assert_eq!(result, vec![5]);
    }

    #[test]
    fn compute_columns_per_row_single_line_with_newline() {
        let result = compute_columns_per_row("hello\n");
        assert_eq!(result, vec![5]);
    }

    #[test]
    fn compute_columns_per_row_multiple_lines() {
        let result = compute_columns_per_row("abc\ndef\nghi");
        assert_eq!(result, vec![3, 3, 3]);
    }

    #[test]
    fn compute_columns_per_row_varying_lengths() {
        let result = compute_columns_per_row("a\nbb\nccc\n");
        assert_eq!(result, vec![1, 2, 3]);
    }

    #[test]
    fn compute_columns_per_row_with_empty_lines() {
        let result = compute_columns_per_row("abc\n\ndef");
        assert_eq!(result, vec![3, 0, 3]);
    }

    #[test]
    fn compute_columns_per_row_multibyte_characters() {
        // Test with multi-byte UTF-8 characters (emojis)
        let result = compute_columns_per_row("a🎉b\nc🎉d");
        // Each line has 3 characters: a, emoji, b (or c, emoji, d)
        assert_eq!(result, vec![3, 3]);
    }

    #[test]
    fn compute_ast_metadata_works() -> Result<()> {
        use crate::test::helper;

        let codes = helper::handmade_test_code()?;
        let code = codes
            .get("hello-world.rs")
            .expect("hello-world.rs should exist");

        let ast_metadata = compute_ast_metadata(code)?;

        // Test that all metadata fields are populated
        assert!(!ast_metadata.node_to_full_hash.is_empty());
        assert!(!ast_metadata.full_hash_to_node.is_empty());
        assert!(!ast_metadata.node_to_structural_hash.is_empty());
        assert!(!ast_metadata.structural_hash_to_node.is_empty());
        assert!(!ast_metadata.reference_nodes_ordered.is_empty());
        assert!(!ast_metadata.semantically_structural_nodes.is_empty());

        for &node_id in &ast_metadata.reference_nodes_ordered {
            assert!(ast_metadata.node_to_full_hash.contains_key(&node_id));
        }

        // Reference nodes must be ordered by subtree size, descending (largest first) - this is
        // what lets `solve_identical_trees` process the biggest duplicated subtrees before their
        // descendants, so the descendants' own matches don't need to be decided independently.
        let sizes: Vec<usize> = ast_metadata
            .reference_nodes_ordered
            .iter()
            .map(|node_id| {
                *ast_metadata
                    .node_to_subtree_size
                    .get(node_id)
                    .expect("every reference node must have a recorded subtree size")
            })
            .collect();
        assert!(
            sizes.is_sorted_by(|a, b| a >= b),
            "reference_nodes_ordered must be sorted by subtree size descending, got {sizes:?}"
        );

        assert!(
            ast_metadata
                .semantically_structural_nodes
                .contains_key(&(String::from("function_item"), String::from("main")))
        );

        Ok(())
    }
}
