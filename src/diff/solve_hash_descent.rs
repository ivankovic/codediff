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
use crate::code::metadata::metadata_of;
use crate::code::{ASTMetadata, Code, Language};
use crate::diff::hash_tree_matching::{self, NodeSelectionConfig};
use crate::diff::{ASTDiff, ASTMappingReason, NodeCache};

/**
* Phase 1 of the seven-phase pipeline (`TODO.md`, 2026-07-17/18): "hash-based, largest-subtree-
* first descent". Runs the generalized `hash_tree_matching::solve_with_hash_map` engine three
* times, once per hash algorithm - each call finds and matches whatever it can, then the next call
* only sees whatever's left unmatched (`PostorderIndexer`/the engine's own `before_node_map` check
* skip anything already claimed):
*
* 1. `KindAndValueHash` - byte-identical subtrees. Extended node selection (reference nodes +
*    big-enough nodes).
* 2. `KindOnlyHash` - same shape, any leaf value. Reference nodes only. Deliberately one coarse
*    tier rather than several intermediate granularities (ignore punctuation only, literals only,
*    identifiers only, ...) - see `TODO.md`'s accepted precision-loss tradeoff.
* 3. Normalized import path hash - import statements matched by normalized path rather than
*    syntax, folded in as a hash variant instead of its own pass. Reference nodes only; the hash
*    is computed only for recognized import-statement kinds (`is_import_node`, below), so
*    non-import nodes never collide into this map.
*
* Both `KindAndValueHash` and `KindOnlyHash` are order-independent per `nodes::is_commutative_
* container` at *every* recursion level (see `code::hash::compute_kind_and_value_hash`'s doc
* comment) - order-independence is inherent to both hashes, not a bolted-on third one.
*/
pub fn solve(
    before: &Code,
    after: &Code,
    node_cache: &NodeCache,
    diff: &mut ASTDiff,
    solve_import_path_hash_enabled: bool,
) {
    let selector_config = NodeSelectionConfig::default();
    let before_metadata = metadata_of(before);
    let after_metadata = metadata_of(after);
    hash_tree_matching::solve_with_hash_map(
        before,
        after,
        node_cache,
        diff,
        &before_metadata.node_to_kind_and_value_hash,
        &after_metadata.kind_and_value_hash_to_node,
        ASTMappingReason::IdenticalHash,
        ASTMappingReason::IdenticalHashOfAncestor,
        selector_config.to_node_list_selector(),
    );

    hash_tree_matching::solve_with_hash_map(
        before,
        after,
        node_cache,
        diff,
        &before_metadata.node_to_kind_only_hash,
        &after_metadata.kind_only_hash_to_node,
        ASTMappingReason::StructurallyIdenticalSubtrees,
        ASTMappingReason::StructurallyIdenticalAncestor,
        |metadata| metadata.reference_nodes_ordered.clone(),
    );
    drop(before_metadata);
    drop(after_metadata);

    if solve_import_path_hash_enabled {
        solve_import_path_hash(before, after, node_cache, diff);
    }
}

/// Builds a normalized-import-path hash map (import nodes only, hashed by
/// `solve_import_nodes::normalize_import_path`'s output) and runs it through the same generalized
/// engine as the other two hash algorithms - see the module doc comment.
fn solve_import_path_hash(before: &Code, after: &Code, node_cache: &NodeCache, diff: &mut ASTDiff) {
    let before_metadata = metadata_of(before);
    let after_metadata = metadata_of(after);

    let before_hash = import_path_hash_map(&before_metadata);
    let after_hash = import_path_hash_map(&after_metadata);

    let mut after_reverse: rustc_hash::FxHashMap<u64, Vec<usize>> =
        rustc_hash::FxHashMap::default();
    for (&node_id, &hash) in &after_hash {
        after_reverse.entry(hash).or_default().push(node_id);
    }

    hash_tree_matching::solve_with_hash_map(
        before,
        after,
        node_cache,
        diff,
        &before_hash,
        &after_reverse,
        ASTMappingReason::NormalizedImportPath,
        ASTMappingReason::NormalizedImportPath,
        move |metadata| {
            let language = metadata.language;
            metadata
                .node_info
                .iter()
                .filter(|(_, info)| is_import_node(&info.kind, &language))
                .map(|(&id, _)| id)
                .collect()
        },
    );
}

fn import_path_hash_map(metadata: &ASTMetadata) -> rustc_hash::FxHashMap<usize, u64> {
    use std::hash::{Hash, Hasher};
    let language = metadata.language;
    let mut result = rustc_hash::FxHashMap::default();
    for (&node_id, info) in &metadata.node_info {
        if !is_import_node(&info.kind, &language) {
            continue;
        }
        let Some(path) = extract_import_path(node_id, metadata) else {
            continue;
        };
        let mut hasher = rustc_hash::FxHasher::default();
        path.hash(&mut hasher);
        result.insert(node_id, hasher.finish());
    }
    result
}

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

    // Normalize relative path prefixes: strip a leading "./", but keep "../" as-is since it
    // changes the meaning (goes up a directory).
    normalized_separators
        .strip_prefix("./")
        .map(str::to_string)
        .unwrap_or(normalized_separators)
}

/// Returns true if the node kind represents an import statement for a given language
///
/// Every kind checked here must be a *named* tree-sitter node, never a bare keyword token
/// (e.g. Rust's `use` keyword, or PHP's `include`/`require` keywords are anonymous leaves with
/// `named: false` in each grammar's `node-types.json` - not the import statement itself), since
/// this is checked against every node in `metadata.node_info`, which includes anonymous nodes -
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
/// verified against each grammar's node-types.json. Checking only `string_literal` silently
/// breaks path extraction for Go and JS/TS/JSON imports, falling through to using the whole
/// import statement's raw text as the "path" instead.
fn is_string_literal_kind(kind: &str) -> bool {
    matches!(
        kind,
        "string_literal" | "raw_string_literal" | "interpreted_string_literal" | "string"
    )
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
            if child_info.kind == "scoped_identifier"
                && let Some(path) = extract_import_path(child_id, metadata)
            {
                return Some(path);
            }
        }
    }

    // If no specific child found, try the node's own text
    if !node_info.text.is_empty() {
        return Some(normalize_import_path(&node_info.text));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::Language;
    use crate::diff::{ASTMappingOperation, ASTMappingReason, COST_UPDATE};
    use crate::test::helper::find_first_of_kind;

    /// Reordering a Rust `use_list` (a `nodes::is_commutative_container` kind) with no other
    /// change must be distinguishable from a genuinely untouched one - see the user request this
    /// responds to ("we do need a way to distinguish between truly identical and reordered", then
    /// "make reordering cost more than 0 and change operation to MatchButNotIdentical") and
    /// `ASTMappingReason::FullymappingSubtrees`'s doc comment.
    #[test]
    fn reordered_commutative_container_is_distinguished_from_truly_identical() {
        let before = Code::from_string("use std::{a, b, c};\nfn f() {}\n", &Language::Rust);
        let after = Code::from_string("use std::{c, a, b};\nfn f() {}\n", &Language::Rust);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve(&before, &after, &node_cache, &mut diff, true);

        let before_root = before.ast.as_ref().unwrap().root_node();
        let after_root = after.ast.as_ref().unwrap().root_node();
        let before_use_list = find_first_of_kind(before_root, "use_list").unwrap();
        let after_use_list = find_first_of_kind(after_root, "use_list").unwrap();

        let mapping = diff
            .mapping
            .get(&(before_use_list.id(), after_use_list.id()))
            .expect("reordered use_list should still be matched");
        assert_eq!(
            mapping.reason,
            ASTMappingReason::FullymappingSubtrees,
            "a reordered-but-unchanged use_list must be tagged FullymappingSubtrees, not plain IdenticalHash"
        );
        assert_eq!(
            mapping.operation,
            ASTMappingOperation::MatchButNotIdentical,
            "a pure reorder is not a no-op - operation must be MatchButNotIdentical, not Identical"
        );
        assert_eq!(
            mapping.cost, COST_UPDATE,
            "a pure reorder must cost more than 0"
        );

        // The unrelated, untouched `fn f() {}` must NOT get relabeled - only the actually-
        // reordered node should carry the distinguishing reason.
        let before_fn = find_first_of_kind(before_root, "function_item").unwrap();
        let after_fn = find_first_of_kind(after_root, "function_item").unwrap();
        let fn_mapping = diff.mapping.get(&(before_fn.id(), after_fn.id())).unwrap();
        assert_ne!(
            fn_mapping.reason,
            ASTMappingReason::FullymappingSubtrees,
            "an untouched, non-reordered node must not be tagged FullymappingSubtrees"
        );

        // `use_declaration` and `scoped_use_list` wrap `use_list` but aren't commutative
        // containers themselves - a reorder several levels down must still downgrade them from
        // Identical, since neither is a true no-op match either.
        let before_use_decl = find_first_of_kind(before_root, "use_declaration").unwrap();
        let after_use_decl = find_first_of_kind(after_root, "use_declaration").unwrap();
        let use_decl_mapping = diff
            .mapping
            .get(&(before_use_decl.id(), after_use_decl.id()))
            .unwrap();
        assert_eq!(
            use_decl_mapping.operation,
            ASTMappingOperation::MatchButNotIdentical,
            "an ancestor of a reordered container must also be downgraded from Identical"
        );
        assert_eq!(use_decl_mapping.cost, COST_UPDATE);
        // The ancestor itself didn't reorder anything - only the actual commutative container
        // gets tagged FullymappingSubtrees.
        assert_ne!(
            use_decl_mapping.reason,
            ASTMappingReason::FullymappingSubtrees
        );
    }

    #[test]
    fn normalize_import_path_strips_quotes_and_normalizes_separators() {
        assert_eq!(normalize_import_path("\"foo/bar\""), "foo/bar");
        assert_eq!(normalize_import_path("'foo\\bar'"), "foo/bar");
        assert_eq!(normalize_import_path("  \"./foo\"  "), "foo");
    }

    #[test]
    fn normalize_import_path_keeps_parent_relative_prefix() {
        // "../" changes meaning (goes up a directory), unlike "./" - it must survive normalization.
        assert_eq!(normalize_import_path("\"../foo\""), "../foo");
    }

    #[test]
    fn is_import_node_recognizes_each_supported_language() {
        assert!(is_import_node("use_declaration", &Language::Rust));
        assert!(is_import_node("extern_crate_declaration", &Language::Rust));
        assert!(is_import_node("import_spec", &Language::Go));
        assert!(is_import_node("import_declaration", &Language::Go));
        assert!(is_import_node("import_statement", &Language::Python));
        assert!(is_import_node("import_from_statement", &Language::Python));
        assert!(is_import_node("import_declaration", &Language::Java));
        assert!(is_import_node("import_declaration", &Language::CSharp));
        assert!(is_import_node("import_statement", &Language::JavaScript));
        assert!(is_import_node("import_expression", &Language::TypeScript));
        assert!(is_import_node("preproc_include", &Language::C));
        assert!(is_import_node("preproc_include", &Language::CPP));
        assert!(is_import_node("import", &Language::Kotlin));
        assert!(is_import_node("import_declaration", &Language::Scala));
        assert!(is_import_node("import_declaration", &Language::Swift));
        assert!(is_import_node("include_expression", &Language::PHP));
        assert!(is_import_node("require_once_expression", &Language::PHP));
    }

    #[test]
    fn is_import_node_rejects_rubys_require_since_it_is_a_plain_method_call() {
        // Ruby's `require "foo"` parses as an ordinary `call` node - indistinguishable from any
        // other method call by kind alone, so it's deliberately left unmatched here.
        assert!(!is_import_node("call", &Language::Ruby));
        assert!(!is_import_node("import_statement", &Language::Ruby));
    }

    #[test]
    fn is_import_node_rejects_a_kind_from_the_wrong_language() {
        // Go's `import_spec` must not accidentally match under Rust just because the string matches.
        assert!(!is_import_node("import_spec", &Language::Rust));
    }

    #[test]
    fn extract_import_path_joins_rust_scoped_identifier_with_double_colon() {
        let code = Code::from_string("use std::collections::HashMap;\n", &Language::Rust);
        let metadata = metadata_of(&code);
        let root = code.ast.as_ref().unwrap().root_node();
        let use_decl = find_first_of_kind(root, "use_declaration").unwrap();
        let path = extract_import_path(use_decl.id(), &metadata).unwrap();
        assert_eq!(path, "std::collections::HashMap");
    }

    #[test]
    fn extract_import_path_normalizes_relative_js_string_literal() {
        let code = Code::from_string("import Foo from \"./foo\";\n", &Language::JavaScript);
        let metadata = metadata_of(&code);
        let root = code.ast.as_ref().unwrap().root_node();
        let import_stmt = find_first_of_kind(root, "import_statement").unwrap();
        let path = extract_import_path(import_stmt.id(), &metadata).unwrap();
        assert_eq!(path, "foo");
    }

    /// Regression/integration guard for the whole pass: a relative import whose formatting
    /// (leading `./`) differs from its counterpart must still match by normalized path, even
    /// though the raw string literal text (and therefore any byte-identical hash) differs.
    #[test]
    fn solve_import_path_hash_matches_reformatted_relative_import() {
        let before = Code::from_string("import Foo from \"./foo\";\n", &Language::JavaScript);
        let after = Code::from_string("import Foo from \"foo\";\n", &Language::JavaScript);
        let node_cache = NodeCache::build(&before, &after);
        let mut diff = ASTDiff::default();

        solve_import_path_hash(&before, &after, &node_cache, &mut diff);

        let before_root = before.ast.as_ref().unwrap().root_node();
        let after_root = after.ast.as_ref().unwrap().root_node();
        let before_import = find_first_of_kind(before_root, "import_statement").unwrap();
        let after_import = find_first_of_kind(after_root, "import_statement").unwrap();

        let mapping = diff
            .mapping
            .get(&(before_import.id(), after_import.id()))
            .expect("\"./foo\" and \"foo\" should normalize to the same import path");
        assert_eq!(mapping.reason, ASTMappingReason::NormalizedImportPath);
    }
}
