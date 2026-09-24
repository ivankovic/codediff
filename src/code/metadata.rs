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

use crate::code::language;
use crate::code::tip;
use crate::code::{ASTMetadata, ASTNodeMetadata, Code, Metadata};
use crate::diff::nodes;

/// The length of each row of `contents` in **bytes** - the column one past its last character,
/// in the unit every column in this codebase uses (see `diff::text_range::SourceColumn`).
///
/// Bytes, not characters: `TextRange::from_treesitter_range` compares this against tree-sitter's
/// byte `Point::column` to detect a range ending at end of row, and in characters that check both
/// misses real ends and fires mid-row (`let 漢 = "yy";` would paint `yy";`).
pub fn compute_row_byte_lengths(contents: &str) -> Vec<usize> {
    let mut result: Vec<usize> = contents.split('\n').map(str::len).collect();

    // A trailing newline does not start a row; an empty input still has one empty row.
    if result.len() > 1 && result.last() == Some(&0) {
        result.pop();
    }

    result
}

/// Fills in the metadata derivable without reading the file (type and language, from the path).
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

/// Computes every [`ASTMetadata`] field for `code`. Errors if `code` is unparsed; an unset language
/// becomes `Language::Unknown`.
pub fn compute_ast_metadata(code: &Code) -> Result<ASTMetadata> {
    let mut metadata = ASTMetadata::default();
    metadata.language = code.metadata.language.unwrap_or_default();
    let ast = code
        .ast
        .as_ref()
        .context("AST must be parsed before computing metadata")?;
    // One walk of the tree-sitter tree serves every step: cursor traffic is expensive.
    let nodes = collect_nodes(ast.root_node());
    let source = code.contents.as_bytes();
    crate::code::hash::hash_nodes(&nodes, source, metadata.language, &mut metadata);
    compute_subtree_sizes(&nodes, &mut metadata);
    compute_node_info(&nodes, source, &mut metadata);
    compute_widest_subtree_node(code, &mut metadata);
    for record in &nodes {
        metadata.node_to_depth.insert(record.id, record.depth);
        if let Some(parent) = record.parent {
            metadata.node_to_parent.insert(record.id, nodes[parent].id);
        }
    }
    discover_reference_nodes(&nodes, &mut metadata);
    Ok(metadata)
}

/// One node of the parsed tree as captured by [`collect_nodes`]: everything the metadata steps
/// read, so that none of them has to walk the tree-sitter tree itself.
pub(crate) struct NodeRecord {
    pub(crate) id: usize,
    pub(crate) kind: &'static str,
    pub(crate) kind_id: u16,
    pub(crate) start_byte: usize,
    pub(crate) end_byte: usize,
    pub(crate) is_named: bool,
    pub(crate) depth: usize,
    /// Index into the same table; `None` for the root.
    pub(crate) parent: Option<usize>,
    /// Indices into the same table, in document order.
    pub(crate) children: Vec<usize>,
}

/// Every node under `root` in left-to-right preorder (so a record's index is its
/// `preorder_index`, and every descendant's index is greater than its ancestor's), from a single
/// cursor traversal. All children, anonymous tokens included - the same set
/// `Node::children` yields and `Node::child_count` counts.
pub(crate) fn collect_nodes(root: tree_sitter::Node) -> Vec<NodeRecord> {
    let mut records: Vec<NodeRecord> = Vec::new();
    let mut ancestors: Vec<usize> = Vec::new();
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        let index = records.len();
        let parent = ancestors.last().copied();
        records.push(NodeRecord {
            id: node.id(),
            kind: node.kind(),
            kind_id: node.kind_id(),
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            is_named: node.is_named(),
            depth: ancestors.len(),
            parent,
            children: Vec::new(),
        });
        if let Some(parent) = parent {
            records[parent].children.push(index);
        }
        if cursor.goto_first_child() {
            ancestors.push(index);
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return records;
            }
            ancestors.pop();
        }
    }
}

/// Borrows `code`'s AST metadata, computing an owned copy only when it is missing. An unparsed
/// `Code` yields empty metadata, per the fail-safe convention on `Diff`.
pub fn metadata_of(code: &Code) -> std::borrow::Cow<'_, ASTMetadata> {
    match &code.metadata.ast_metadata {
        Some(metadata) => std::borrow::Cow::Borrowed(metadata),
        None => std::borrow::Cow::Owned(compute_ast_metadata(code).unwrap_or_default()),
    }
}

fn compute_subtree_sizes(nodes: &[NodeRecord], metadata: &mut ASTMetadata) {
    // Reverse preorder visits every child before its parent.
    let mut sizes = vec![0usize; nodes.len()];
    for (index, record) in nodes.iter().enumerate().rev() {
        let size = 1 + record.children.iter().map(|&c| sizes[c]).sum::<usize>();
        sizes[index] = size;
        metadata.node_to_subtree_size.insert(record.id, size);
    }
}

fn compute_node_info(nodes: &[NodeRecord], source: &[u8], metadata: &mut ASTMetadata) {
    for (preorder_index, record) in nodes.iter().enumerate() {
        let kind = record.kind.to_string();
        // Leaves only; see `ASTNodeMetadata::text`.
        let text = if record.children.is_empty() {
            std::str::from_utf8(&source[record.start_byte..record.end_byte])
                .unwrap_or("")
                .to_string()
        } else {
            String::new()
        };
        let owned_text_hash = owned_text_hash_of(record, source, nodes);
        let children: Vec<usize> = record.children.iter().map(|&c| nodes[c].id).collect();

        metadata.node_info.insert(
            record.id,
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
                start_byte: record.start_byte,
                preorder_index,
                is_named: record.is_named,
            },
        );
    }
}

/// A hash of the text a node owns *directly* - the non-whitespace content in the gaps before,
/// between and after its children - or 0 when every gap is formatting (the overwhelmingly common
/// case: a well-behaved internal node's bytes are entirely covered by its children).
///
/// Grammars that keep a payload as parent-owned text include XML (`AttValue`, every attribute
/// value), CSS (`integer_value`, `color_value`), Rust comments and YAML quoted scalars;
/// `code::gap_survey` measures them. Leaves report 0: their span is already `text`.
fn owned_text_hash_of(record: &NodeRecord, source: &[u8], nodes: &[NodeRecord]) -> u64 {
    use std::hash::Hasher;
    if record.children.is_empty() {
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
    let mut gap_start = record.start_byte;
    for &child in &record.children {
        hash_gap(&mut hasher, gap_start, nodes[child].start_byte);
        gap_start = nodes[child].end_byte;
    }
    hash_gap(&mut hasher, gap_start, record.end_byte);
    if !any_content {
        return 0;
    }
    // 0 is reserved for "owns no text", so nudge a real hash off it.
    match hasher.finish() {
        0 => 1,
        hash => hash,
    }
}

/// Computes `ASTMetadata::node_to_widest_subtree_node` bottom-up over `node_info`, which must
/// already be populated.
fn compute_widest_subtree_node(code: &Code, metadata: &mut ASTMetadata) {
    let Some(ast) = code.ast.as_ref() else { return };
    let root_id = ast.root_node().id();

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

/// Lists the reference nodes - the units humans think about code in, matched first so diffs make
/// sense - largest subtree first, so hash descent settles big duplicated subtrees before their
/// descendants.
fn discover_reference_nodes(nodes: &[NodeRecord], metadata: &mut ASTMetadata) {
    let language = &metadata.language;

    // Collected in preorder, children right to left, which is the order the stable sort below
    // breaks size ties in.
    let mut reference_nodes_with_sizes = Vec::new();
    let mut stack = vec![0usize];
    while let Some(index) = stack.pop() {
        let record = &nodes[index];
        // See `ASTNodeMetadata::is_named`.
        if record.is_named
            && nodes::is_reference(record.kind, language)
            && let Some(&subtree_size) = metadata.node_to_subtree_size.get(&record.id)
        {
            reference_nodes_with_sizes.push((record.id, subtree_size));
        }
        stack.extend(record.children.iter().copied());
    }

    reference_nodes_with_sizes.sort_by_key(|&(_, subtree_size)| std::cmp::Reverse(subtree_size));

    metadata.reference_nodes_ordered = reference_nodes_with_sizes
        .into_iter()
        .map(|(node_id, _)| node_id)
        .collect();
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

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
        assert_eq!(result, vec![6, 6]);
    }

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

        assert!(!ast_metadata.node_to_full_hash.is_empty());
        assert!(!ast_metadata.full_hash_to_node.is_empty());
        assert!(!ast_metadata.node_to_structural_hash.is_empty());
        assert!(!ast_metadata.structural_hash_to_node.is_empty());
        assert!(!ast_metadata.reference_nodes_ordered.is_empty());

        for &node_id in &ast_metadata.reference_nodes_ordered {
            assert!(ast_metadata.node_to_full_hash.contains_key(&node_id));
        }

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
