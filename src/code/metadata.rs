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

/// Pre-order stack walk (children pushed in document order, so the LIFO stack pops them back out
/// in reverse - fine here since `visit` doesn't care) over every node in `root`'s subtree, calling
/// `visit` once per node. Shared by `discover_reference_nodes` and
/// `discover_semantic_structure_nodes`, whose results don't depend on visitation order (the
/// former sorts its collected nodes afterward; the latter keys a map by node type, and this AST
/// has at most one node of each semantically-structural type per the type's own invariant). Not
/// shared with `compute_node_depths` (needs each node's depth threaded through the walk) or
/// `compute_subtree_sizes`/`compute_node_info` (need true post-order/preorder-indexed traversal).
fn walk_preorder<'a>(root: tree_sitter::Node<'a>, mut visit: impl FnMut(tree_sitter::Node<'a>)) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        visit(node);
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
}

/// The length of each row of `contents` in **bytes** - the column one past its last character,
/// in the unit every column in this codebase uses (see `diff::text_range::SourceColumn`).
///
/// Bytes, not characters, and the distinction is not academic. The only consumer is
/// `TextRange::from_treesitter_range`, which asks "does this node's end land exactly at the end of
/// its row?" by comparing this against tree-sitter's `Point::column` - a **byte** offset. While
/// this counted characters the comparison was between two different units, and it went wrong in
/// both directions:
///
/// * it **failed to fire** on a row whose byte length exceeded its character count, leaving a
///   genuine end-of-row unnormalized;
/// * it **fired spuriously** whenever a mid-row byte column happened to equal the row's character
///   count, rewriting that end as `(row + 1, 0)` and silently widening the range to the end of the
///   line. `let 漢 = "yy";` is 15 bytes and 13 characters, and the string's end at byte 13 was
///   read as end-of-row - which painted `yy";` where the human sees `yy` change.
///
/// Renamed from `compute_row_byte_lengths` deliberately: the unit is the whole point, so it belongs
/// in the name rather than in a doc comment nobody re-reads.
pub fn compute_row_byte_lengths(contents: &str) -> Vec<usize> {
    let mut result: Vec<usize> = contents.split('\n').map(str::len).collect();

    // `split` always yields one trailing empty piece for a string ending in a newline. That piece
    // is not a row: a file of "a\n" has one row, not two. An empty input keeps its single zero
    // row, matching the previous behaviour.
    if result.len() > 1 && result.last() == Some(&0) {
        result.pop();
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
    compute_widest_subtree_node(code, &mut metadata);
    compute_node_depths(code, &mut metadata)?;
    compute_node_parents(&mut metadata);
    discover_reference_nodes(code, &mut metadata)?;
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
        // Leaves only. Every consumer compares `text` between two leaves (`UnitCostModel::ren`,
        // `classify_match`, the slot alignment's leaf arms); an internal node's whole subtree text
        // was copied here too, which made this walk O(bytes x depth) and the single largest cost
        // of metadata on a large file (measured 2026-09-06 over the corpus: 11.9s of metadata
        // against 3.7s of parsing, before this change).
        let text = if node.child_count() == 0 {
            node.utf8_text(code.contents.as_bytes())
                .unwrap_or("")
                .to_string()
        } else {
            String::new()
        };

        // Get children IDs
        let mut child_cursor = node.walk();
        let child_nodes: Vec<tree_sitter::Node> = node.children(&mut child_cursor).collect();
        let owned_text_hash = owned_text_hash_of(&node, code.contents.as_bytes(), &child_nodes);
        let children: Vec<usize> = child_nodes.iter().map(|c| c.id()).collect();

        metadata.node_info.insert(
            node_id,
            ASTNodeMetadata {
                kind_cost_class: crate::code::KindCostClass {
                    identifier_like: nodes::is_identifier_kind(&kind),
                    literal_like: nodes::is_literal_kind(&kind),
                    operator_families: nodes::operator_family_mask(&kind),
                },
                kind,
                text,
                owned_text_hash,
                children,
                start_byte: node.start_byte(),
                preorder_index,
                is_named: node.is_named(),
            },
        );
        preorder_index += 1;

        // Push children in reverse so the stack (LIFO) pops them back out left-to-right, keeping
        // `preorder_index` a genuine preorder (root, then children in document order).
        let mut child_cursor = node.walk();
        for child in node
            .children(&mut child_cursor)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            stack.push(child);
        }
    }

    Ok(())
}

/// A hash of the text a node owns *directly* - the non-whitespace content in the gaps before,
/// between and after its children - or 0 when every gap is formatting (the overwhelmingly common
/// case: a well-behaved internal node's bytes are entirely covered by its children).
///
/// Not a curiosity. Grammars disagree about whether a construct's payload is a child node or text
/// the parent owns, and for several it is the latter. Census over the whole corpus (2026-08-18,
/// `code::gap_survey`): XML `AttValue` 21663 nodes / 394KB - *every* attribute value - CSS
/// `integer_value`/`color_value` 6962, Rust `line_comment`/`block_comment` 2149 / 146KB, YAML's
/// quoted scalars 1844. Zero in TypeScript, JSON, Go, Kotlin, JavaScript, C++, TSX and Java, which
/// is why the gap went unnoticed for so long.
///
/// A leaf owns its whole span, but that is already `ASTNodeMetadata::text`, which `ren` compares
/// directly - so this reports 0 for leaves rather than duplicating it.
fn owned_text_hash_of(
    node: &tree_sitter::Node,
    source: &[u8],
    children: &[tree_sitter::Node],
) -> u64 {
    use std::hash::Hasher;
    if children.is_empty() {
        return 0;
    }
    let mut hasher = metrohash::MetroHash64::new();
    let mut any_content = false;
    let mut hash_gap = |hasher: &mut metrohash::MetroHash64, start: usize, end: usize| {
        if start >= end {
            return;
        }
        if let Ok(text) = std::str::from_utf8(&source[start..end])
            && !text.trim().is_empty()
        {
            hasher.write(text.as_bytes());
            any_content = true;
        }
    };
    let mut gap_start = node.start_byte();
    for child in children {
        hash_gap(&mut hasher, gap_start, child.start_byte());
        gap_start = child.end_byte();
    }
    hash_gap(&mut hasher, gap_start, node.end_byte());
    if !any_content {
        return 0;
    }
    // 0 is reserved for "owns no text", so nudge a real hash off it.
    match hasher.finish() {
        0 => 1,
        hash => hash,
    }
}

/// Compute `ASTMetadata::node_to_widest_subtree_node` (see its doc comment) via a bottom-up
/// (post-order) pass over `node_info`, already populated by `compute_node_info` - id-based, no
/// second walk of the raw tree-sitter AST needed beyond reading the root id.
fn compute_widest_subtree_node(code: &Code, metadata: &mut ASTMetadata) {
    let Some(ast) = code.ast.as_ref() else { return };
    let root_id = ast.root_node().id();

    // Post-order via the same (id, processed) stack idiom `compute_subtree_sizes` uses, just
    // operating on `node_info.children` instead of raw tree-sitter nodes.
    let mut stack = vec![(root_id, false)];
    while let Some((node_id, processed)) = stack.pop() {
        if processed {
            let Some(children) = metadata
                .node_info
                .get(&node_id)
                .map(|info| info.children.clone())
            else {
                continue;
            };
            let mut best = (children.len(), node_id);
            for &child_id in &children {
                if let Some(&(child_best_count, child_best_id)) =
                    metadata.node_to_widest_subtree_node.get(&child_id)
                    && child_best_count > best.0
                {
                    best = (child_best_count, child_best_id);
                }
            }
            metadata.node_to_widest_subtree_node.insert(node_id, best);
        } else {
            stack.push((node_id, true));
            if let Some(info) = metadata.node_info.get(&node_id) {
                for &child_id in &info.children.clone() {
                    stack.push((child_id, false));
                }
            }
        }
    }
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
    // `metadata.language`, not `code.metadata.language` - the former is already fail-safed to
    // `Language::Unknown` by `compute_ast_metadata` (this function's only caller) a few lines
    // before calling here; re-reading the latter and `.expect()`-ing it was inconsistent with
    // that and could panic in a case the caller had already made safe (found in a 2026-07
    // code-health pass).
    let language = &metadata.language;

    // Collect reference nodes with their subtree sizes
    let mut reference_nodes_with_sizes = Vec::new();

    walk_preorder(root_node, |node| {
        let node_id = node.id();
        // `is_named`: an anonymous keyword token can share its kind string with the statement it
        // introduces (Kotlin's `import`), and must not be listed as a reference node itself.
        if node.is_named()
            && nodes::is_reference(node.kind(), language)
            && let Some(&subtree_size) = metadata.node_to_subtree_size.get(&node_id)
        {
            reference_nodes_with_sizes.push((node_id, subtree_size));
        }
    });

    // Sort reference nodes by subtree size in descending order
    reference_nodes_with_sizes.sort_by_key(|&(_, subtree_size)| std::cmp::Reverse(subtree_size));

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

    /// Regression test: `discover_reference_nodes` used to read `code.metadata.language` and
    /// `.expect()` it directly, instead of the `metadata.language` its own caller
    /// (`compute_ast_metadata`) had already fail-safed a few lines earlier - so a `Code` with a
    /// real parsed AST but an unset `metadata.language` (constructible directly, since every
    /// field here is `pub`) panicked instead of degrading gracefully like everything else in this
    /// pipeline.
    #[test]
    fn compute_ast_metadata_does_not_panic_when_language_is_unset() {
        let mut code = crate::code::Code::from_string("fn main() {}", &crate::code::Language::Rust);
        code.metadata.language = None;
        code.metadata.ast_metadata = None;

        let metadata = compute_ast_metadata(&code).expect("should fail safe, not panic");
        assert_eq!(metadata.language, crate::code::Language::Unknown);
    }

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
    fn compute_row_byte_lengths_empty_string() {
        let result = compute_row_byte_lengths("");
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn compute_row_byte_lengths_single_line() {
        let result = compute_row_byte_lengths("hello");
        assert_eq!(result, vec![5]);
    }

    #[test]
    fn compute_row_byte_lengths_single_line_with_newline() {
        let result = compute_row_byte_lengths("hello\n");
        assert_eq!(result, vec![5]);
    }

    #[test]
    fn compute_row_byte_lengths_multiple_lines() {
        let result = compute_row_byte_lengths("abc\ndef\nghi");
        assert_eq!(result, vec![3, 3, 3]);
    }

    #[test]
    fn compute_row_byte_lengths_varying_lengths() {
        let result = compute_row_byte_lengths("a\nbb\nccc\n");
        assert_eq!(result, vec![1, 2, 3]);
    }

    #[test]
    fn compute_row_byte_lengths_with_empty_lines() {
        let result = compute_row_byte_lengths("abc\n\ndef");
        assert_eq!(result, vec![3, 0, 3]);
    }

    #[test]
    fn compute_row_byte_lengths_multibyte_characters() {
        let result = compute_row_byte_lengths("a🎉b\nc🎉d");
        // Six bytes, not three characters: the emoji is four bytes, and the one consumer of this
        // compares the result against tree-sitter's byte columns. Asserting 3 here is what let the
        // end-of-row check fire on a mid-row column - see `compute_row_byte_lengths`' doc comment.
        assert_eq!(result, vec![6, 6]);
    }

    /// The end-of-row check this feeds must agree with tree-sitter's own column for that position,
    /// on rows where bytes and characters disagree in either direction.
    #[test]
    fn compute_row_byte_lengths_agrees_with_byte_offsets_on_mixed_rows() {
        for line in [
            "ascii only",
            "é two-byte",
            "漢 three-byte",
            "𝛼 four-byte",
            "",
        ] {
            let text = format!("{line}\n");
            assert_eq!(
                compute_row_byte_lengths(&text),
                vec![line.len()],
                "row length must be the byte column one past the last character of {line:?}"
            );
        }
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

        for &node_id in &ast_metadata.reference_nodes_ordered {
            assert!(ast_metadata.node_to_full_hash.contains_key(&node_id));
        }

        // Reference nodes must be ordered by subtree size, descending (largest first) - this is
        // what lets `solve_hash_descent`'s phase 1 process the biggest duplicated subtrees before
        // their descendants, so the descendants' own matches don't need to be decided
        // independently.
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

        Ok(())
    }
}
