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
use std::collections::HashMap;

use crate::code::{ASTMetadata, Code, Language};
use crate::diff::{ASTDiff, ASTMapping, ASTMappingOperation, ASTMappingReason, NodeCache};

/// Normalizes an import path by:
/// - Removing surrounding quotes (" or ')
/// - Normalizing path separators (both / and \ to /)
/// - Trimming whitespace
/// - Handling relative imports (./, ../ prefixes)
///
/// This allows matching imports that have different formatting but refer to the same path.
fn normalize_import_path(path: &str) -> String {
    // Trim whitespace, remove surrounding quotes, and trim again
    let trimmed = path.trim();
    let unquoted = trimmed.trim_matches('"').trim_matches('\'').trim();
    
    // Normalize path separators
    let normalized_separators = unquoted.replace('\\', "/");
    
    // Normalize relative path prefixes
    // Handle ./prefix and ../prefix
    let normalized = if normalized_separators.starts_with("./") {
        normalized_separators[2..].to_string()
    } else if normalized_separators.starts_with("../") {
        // Keep .. as it changes the meaning
        normalized_separators
    } else {
        normalized_separators
    };
    
    normalized
}

/// Returns true if the node kind represents an import statement for a given language
///
/// Every kind checked here must be a *named* tree-sitter node, never a bare keyword token
/// (e.g. Rust's `use` keyword, or PHP's `include`/`require` keywords are anonymous leaves with
/// `named: false` in each grammar's `node-types.json` - not the import statement itself). Since
/// `collect_imports` walks every node in `metadata.node_info`, which includes anonymous nodes,
/// matching a bare keyword here would treat every occurrence of that keyword in the file as its
/// own "import" with a bogus path derived from the keyword text, causing spurious cross-statement
/// matches. Verified against each language's node-types.json.
fn is_import_node(node_kind: &str, language: &Language) -> bool {
    match language {
        Language::Rust => node_kind == "use_declaration" || node_kind == "extern_crate_declaration",
        Language::Go => node_kind == "import_spec" || node_kind == "import_declaration",
        Language::Python => node_kind == "import_statement" || node_kind == "import_from_statement",
        Language::Java | Language::CSharp => node_kind == "import_declaration",
        Language::JavaScript | Language::TypeScript | Language::TSX => {
            node_kind == "import_statement" || node_kind == "import_expression"
        }
        Language::C | Language::CPP => node_kind == "preproc_include",
        Language::Kotlin => node_kind == "import",
        Language::Scala => node_kind == "import_declaration",
        Language::Swift => node_kind == "import_declaration",
        Language::PHP => {
            node_kind == "include_expression"
                || node_kind == "include_once_expression"
                || node_kind == "require_expression"
                || node_kind == "require_once_expression"
        }
        // Ruby's `require "foo"` parses as a plain method `call` node, not a dedicated import
        // node kind, so it can't be distinguished from any other method call by kind alone -
        // left unmatched rather than risking false positives on arbitrary calls.
        _ => false,
    }
}

/// True if `kind` is a string-literal node kind in any grammar this pass supports. Deliberately
/// broader than any single language's spelling: Rust/C/C++/Java/C#/Kotlin/Swift/PHP call it
/// `string_literal`, Go calls it `interpreted_string_literal` (plus `raw_string_literal` for
/// backtick strings, shared with Rust's raw strings), and JS/TS/JSON/PHP call it plain `string` -
/// verified against each grammar's node-types.json. Checking only `string_literal` (as an earlier
/// version of this function did) silently breaks path extraction for Go and JS/TS/JSON imports,
/// falling through to using the whole import statement's raw text as the "path" instead.
fn is_string_literal_kind(kind: &str) -> bool {
    matches!(kind, "string_literal" | "raw_string_literal" | "interpreted_string_literal" | "string")
}

/// Extracts the import path text from a node's children
fn extract_import_path(node_id: usize, metadata: &ASTMetadata) -> Option<String> {
    let node_info = metadata.node_info.get(&node_id)?;

    // First, check if the node itself is a string literal or scoped identifier
    if is_string_literal_kind(&node_info.kind) {
        return Some(normalize_import_path(&node_info.text));
    }
    
    // Check if the node itself is a scoped identifier
    if node_info.kind == "scoped_identifier" {
        // Collect identifiers in preorder (left to right)
        let mut path_parts = Vec::new();
        let mut stack = vec![node_id];
        while let Some(id) = stack.pop() {
            if let Some(info) = metadata.node_info.get(&id) {
                // Visit children in order (left to right)
                // To maintain order with a stack, push them in reverse order
                for &child in info.children.iter().rev() {
                    stack.push(child);
                }
                // If this node is an identifier, add its text
                if info.kind == "identifier" {
                    path_parts.push(info.text.clone());
                }
            }
        }
        if !path_parts.is_empty() {
            return Some(path_parts.join("::"));
        }
    }
    
    // Try to find a string literal or identifier child that contains the path
    for &child_id in &node_info.children {
        if let Some(child_info) = metadata.node_info.get(&child_id) {
            // For string literals, return the text content
            if is_string_literal_kind(&child_info.kind) {
                // Get the text, which includes quotes - normalize it
                return Some(normalize_import_path(&child_info.text));
            }
            // For identifier nodes in import statements (e.g., `use std::path` in Rust)
            if child_info.kind == "identifier" {
                return Some(normalize_import_path(&child_info.text));
            }
            // For scoped identifiers, recurse
            if child_info.kind == "scoped_identifier" {
                if let Some(path) = extract_import_path(child_id, metadata) {
                    return Some(path);
                }
            }
        }
    }
    
    // If no specific child found, try the node's own text
    if !node_info.text.is_empty() {
        return Some(normalize_import_path(&node_info.text));
    }
    
    None
}

/// Collects all import nodes from an AST with their normalized paths.
///
/// Only considers *named* nodes (`node.is_named()`), keyed against the parsed tree via
/// `node_cache` rather than `metadata.node_info` alone: some grammars (e.g. Kotlin) reuse the
/// same kind string for both the named import-statement node and its own anonymous keyword
/// child, so kind-string matching alone (`is_import_node`) cannot tell them apart - only the
/// tree-sitter node's `named` flag can.
fn collect_imports(
    metadata: &ASTMetadata,
    node_cache: &HashMap<usize, tree_sitter::Node>,
) -> HashMap<usize, String> {
    let language = metadata.language;
    let mut imports = HashMap::new();

    for (&node_id, node_info) in &metadata.node_info {
        if !is_import_node(&node_info.kind, &language) {
            continue;
        }
        if !node_cache.get(&node_id).is_some_and(|n| n.is_named()) {
            continue;
        }
        if let Some(normalized_path) = extract_import_path(node_id, metadata) {
            imports.insert(node_id, normalized_path);
        }
    }

    imports
}

/**
* Perform import path normalization and matching between two AST trees.
*
* This pass identifies import statements in both trees, normalizes their paths
* (removing quotes, normalizing separators, handling relative imports), and matches
* them by normalized path rather than syntax. This allows the algorithm to recognize
* that imports with different formatting but the same path are actually the same.
*
* Reordered imports within the same group (e.g., imports in the same import block)
* are matched using Move operations, which better represents the actual change.
*
* Runs early in the pipeline (after identical/hash matching, before structural matching)
* so that path-normalized matches can be established before later passes build on them.
*/
pub fn solve(before: &Code, after: &Code, node_cache: &NodeCache, diff: &mut ASTDiff) {
    let before_metadata = crate::code::metadata::metadata_of(before);
    let after_metadata = crate::code::metadata::metadata_of(after);
    let language = before_metadata.language;

    // If language is not determined, we can't match import nodes
    if language == Language::Unknown { return }

    // Collect imports from both sides with normalized paths
    let before_imports = collect_imports(&before_metadata, &node_cache.before);
    let after_imports = collect_imports(&after_metadata, &node_cache.after);

    // Build a map from normalized path to node_id for the after side. Each bucket is sorted by
    // (start_byte, preorder_index) - node ids themselves are tree-sitter arena slots that aren't
    // stable across separate parses of identical source, so they can't be used as a tiebreak (see
    // the project's benchmark-determinism fix for the same issue elsewhere in the pipeline).
    let after_sort_key = |id: &usize| {
        after_metadata
            .node_info
            .get(id)
            .map(|info| (info.start_byte, info.preorder_index))
            .unwrap_or((usize::MAX, usize::MAX))
    };
    let mut path_to_after: HashMap<&str, Vec<usize>> = HashMap::new();
    for (node_id, path) in &after_imports {
        path_to_after.entry(path.as_str()).or_default().push(*node_id);
    }
    for ids in path_to_after.values_mut() {
        ids.sort_by_key(after_sort_key);
    }

    // Visit before-side imports in deterministic (start_byte, preorder_index) order, not raw
    // HashMap iteration order.
    let mut before_ids: Vec<&usize> = before_imports.keys().collect();
    before_ids.sort_by_key(|id| {
        before_metadata
            .node_info
            .get(id)
            .map(|info| (info.start_byte, info.preorder_index))
            .unwrap_or((usize::MAX, usize::MAX))
    });

    // Match before imports to after imports by normalized path
    for before_id in before_ids {
        // Skip if already matched by an earlier pass
        if diff.before_node_map.contains_key(before_id) {
            continue;
        }
        let before_path = &before_imports[before_id];

        // Find all after nodes with the same normalized path
        let Some(after_ids) = path_to_after.get(before_path.as_str()) else {
            continue;
        };
        // Among unclaimed after candidates sharing this path, pick whichever sits closest in the
        // file to `before_id`'s own position - the same proximity tiebreak `hash_tree_matching`
        // uses, so that a real (unmoved) match is preferred over an arbitrary same-path duplicate.
        let before_start_byte = before_metadata
            .node_info
            .get(before_id)
            .map(|info| info.start_byte)
            .unwrap_or(0);
        let Some(&after_id) = after_ids
            .iter()
            .filter(|id| !diff.after_node_map.contains_key(*id))
            .filter(|id| {
                let before_kind = node_cache.before.get(before_id).map(|n| n.kind());
                let after_kind = node_cache.after.get(id).map(|n| n.kind());
                before_kind.is_some() && before_kind == after_kind
            })
            .min_by_key(|id| {
                after_metadata
                    .node_info
                    .get(id)
                    .map(|info| info.start_byte.abs_diff(before_start_byte))
                    .unwrap_or(usize::MAX)
            })
        else {
            continue;
        };

        let operation = if before_metadata.node_info.get(before_id).map_or(false, |info|
            info.text == after_metadata.node_info.get(&after_id).map_or("", |i| i.text.as_str())
        ) {
            ASTMappingOperation::Identical
        } else {
            ASTMappingOperation::Update
        };

        diff.add_mapping(
            *before_id,
            after_id,
            ASTMapping {
                cost: 0, // Normalized path match has no cost
                operation,
                reason: ASTMappingReason::NormalizedImportPath,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::{Code, Language};

    #[test]
    fn test_normalize_import_path() {
        // Test various import path formats
        assert_eq!(normalize_import_path("\"std::path\""), "std::path");
        assert_eq!(normalize_import_path("'std::path'"), "std::path");
        assert_eq!(normalize_import_path("std::path"), "std::path");
        assert_eq!(normalize_import_path("./local/module"), "local/module");
        assert_eq!(normalize_import_path("../parent/module"), "../parent/module");
        assert_eq!(normalize_import_path("path\\to\\module"), "path/to/module");
        assert_eq!(normalize_import_path("  \"  std::path  \"  "), "std::path");
    }

    #[test]
    fn test_import_matching_with_different_quotes() {
        // Rust's `use` statement is never quoted (`use "std::path";` isn't valid Rust - `use`
        // always takes a bare path), so quote-style normalization has no real Rust example. Use
        // JavaScript instead, where `import "./foo";` and `import './foo';` are both valid ES
        // module syntax and genuinely differ only in quote character.
        let before = Code::from_string(
            r#"import "./foo";"#,
            &Language::JavaScript,
        );
        let after = Code::from_string(
            r#"import './foo';"#,
            &Language::JavaScript,
        );

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve(&before, &after, &node_cache, &mut diff);

        // Exactly one NormalizedImportPath mapping (the import_statement itself), not e.g. one
        // per anonymous keyword token colliding on a bogus shared path.
        let normalized_import_matches: Vec<_> = diff
            .mapping
            .values()
            .filter(|m| m.reason == ASTMappingReason::NormalizedImportPath)
            .collect();
        assert_eq!(
            normalized_import_matches.len(),
            1,
            "Expected exactly one NormalizedImportPath mapping, got {:?}",
            normalized_import_matches
        );
    }

    #[test]
    fn test_import_reordering() {
        let before = Code::from_string(
            r#"use std::io;
use std::path;"#,
            &Language::Rust,
        );
        let after = Code::from_string(
            r#"use std::path;
use std::io;"#,
            &Language::Rust,
        );

        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve(&before, &after, &node_cache, &mut diff);

        // Both use_declarations should match by normalized path despite being reordered - not
        // e.g. every anonymous "use" keyword token colliding on the same bogus path "use".
        let normalized_import_matches: Vec<_> = diff
            .mapping
            .values()
            .filter(|m| m.reason == ASTMappingReason::NormalizedImportPath)
            .collect();
        assert_eq!(
            normalized_import_matches.len(),
            2,
            "Expected exactly two NormalizedImportPath mappings (one per use_declaration), got {:?}",
            normalized_import_matches
        );
    }
}
