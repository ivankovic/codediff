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
pub mod hash;
pub mod language;
pub mod metadata;
pub mod tip; // Since type is a reserved keyword in Rust, we use Croatian instead.

use anyhow::{Result, anyhow};
use std::collections::{HashMap, HashSet};
use std::fmt;

/**
* The main data structure. It owns and contains the actual code and all metadata.
*
* Any function that accepts this structure or any sub-field should not assume any fields are set
* and should always first check that the required data is actually available, if not it should try
* to construct it, and if that doesn't work it should fail-safe, ideally returning a safe zero
* result. This allows the calling code to extremely efficiently process large files, and files that
* only pretend to be code but are data or configuration. To help the compiler enforce this, most
* leaf fields should be wrapped in Option. Ideally, the only non-Option wrapped field is the code
* itself.
*/
#[derive(Debug, Clone, Default)]
pub struct Code {
    /// The actual code.
    pub contents: String,
    /// The metadata about the code.
    pub metadata: Metadata,
    /// The AST.
    pub ast: Option<tree_sitter::Tree>,
}

impl Code {
    /**
     * Parse the contents of the Code and fill out the AST.
     */
    pub fn parse(&mut self, parser: &mut tree_sitter::Parser) {
        let language = match self.metadata.language.as_ref() {
            Some(lang) => lang,
            None => return,
        };
        let ts_language = match crate::code::language::to_treesitter(language) {
            Some(ts_lang) => ts_lang,
            None => return,
        };
        if parser.set_language(&ts_language).is_err() {
            return;
        }
        self.ast = parser.parse(&self.contents, None);
    }

    /**
     * Ensure the code is parsed and metadata is computed.
     *
     * This function provides a convenient way to ensure that a Code structure has both its AST
     * parsed and its metadata computed. It follows these steps:
     *
     * 1. If the code is already parsed and metadata is set: Do nothing (early return)
     * 2. If the code is parsed but metadata is not computed: Compute the metadata (especially ASTMetadata)
     * 3. If the code is not parsed: Parse the code first, then compute metadata
     *
     * This is useful when you want to guarantee that all computable metadata is available
     * without having to manually check and call parse() and metadata computation separately.
     *
     * Returns an error if the language is not set in the metadata or if the language is not
     * supported by tree-sitter.
     */
    pub fn ensure_parsed(&mut self) -> Result<()> {
        // Return error if language is not set
        let language = match self.metadata.language.as_ref() {
            Some(lang) => lang,
            None => return Err(anyhow!("Language must be set to parse code")),
        };

        // Check if we need to parse
        let needs_parsing = self.ast.is_none();

        // Check if we need to compute metadata
        let needs_metadata = self.metadata.ast_metadata.is_none();

        // If nothing needs to be done, return early
        if !needs_parsing && !needs_metadata {
            return Ok(());
        }

        // Parse if needed
        if needs_parsing {
            let ts_language = match crate::code::language::to_treesitter(language) {
                Some(ts_lang) => ts_lang,
                None => {
                    return Err(anyhow!(
                        "Language {} is not supported by tree-sitter",
                        language
                    ));
                }
            };
            let mut parser = tree_sitter::Parser::new();
            if parser.set_language(&ts_language).is_err() {
                return Err(anyhow!("Failed to set tree-sitter language"));
            }
            self.ast = parser.parse(&self.contents, None);
        }

        // Compute metadata if needed (and if we have a valid AST)
        if needs_metadata && self.ast.is_some() {
            self.metadata.ast_metadata = Some(crate::code::metadata::compute_ast_metadata(self)?);
        }

        Ok(())
    }

    /**
     * Constructs a Code structure from the given string and language.
     *
     * Note that the metadata type will be assumed to be Code. If for some reason you want to use this
     * to construct configuration, data or documentation, make sure to update the metadata accordingly
     * after construction.
     *
     * TODO: Make this code auto-recognize type based on contents to correctly construct Code objects
     * that are actually Configuration, e.g. docker-compose YAML files. It will require expanding the
     * language.rs detection to support content aware metadata expansion.
     */
    pub fn from_string(contents: &str, language: &Language) -> Self {
        let mut code = Code {
            contents: contents.to_string(),
            metadata: Metadata {
                path: None,
                tip: Some(Type::Code("Code".to_string())),
                language: Some(*language),
                columns_for_row: None,
                ast_metadata: None,
            },
            ..Default::default()
        };

        // Parse the code to populate the AST
        let mut parser = tree_sitter::Parser::new();
        code.parse(&mut parser);

        // Compute AST metadata
        if let Ok(ast_metadata) = crate::code::metadata::compute_ast_metadata(&code) {
            code.metadata.ast_metadata = Some(ast_metadata);
        }

        code
    }

    /**
     * Constructs a Code structure from the given file path.
     *
     * The language is automatically detected from the file extension. If the extension is not
     * recognized, the language will be set to Unknown.
     *
     * Note that the metadata type will be assumed to be Code. The path will be stored in the metadata.
     *
     * TODO: Use the hermetic expansion from metadata.rs to expand the metadata.
     */
    pub fn from_file(path: &std::path::Path) -> Result<Self> {
        use std::fs;

        let contents = fs::read_to_string(path)
            .map_err(|e| anyhow!("Failed to read file {}: {}", path.display(), e))?;

        // Determine language from file extension
        let language = language::language_for_path(path).unwrap_or(Language::Unknown);

        let mut code = Code::from_string(&contents, &language);
        code.metadata.path = Some(path.to_path_buf());

        Ok(code)
    }
}

/**
* The metadata around the code, but not the code itself.
*
* This is only the metadata that is necessary for the diffing. Statistics and test data should not
* be added here.
*
* Most fields in this class should be optional, to allow for efficient computation.
*/
#[derive(Debug, Clone, Default)]
pub struct Metadata {
    /// The path to the code, if one exists.
    pub path: Option<std::path::PathBuf>,
    /// The gross type of the "code".
    /// Since type is a reserved keyword in Rust, we use Croatian instead.
    pub tip: Option<Type>,
    /// The language.
    pub language: Option<Language>,
    /// The number of columns in each row.
    pub columns_for_row: Option<Vec<usize>>,
    /// AST metadata including hashes and reference nodes.
    pub ast_metadata: Option<ASTMetadata>,
}

/// Node information for AST nodes
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ASTNodeMetadata {
    /// Node kind (type)
    pub kind: String,
    /// Node text content (for leaf nodes)
    pub text: String,
    /// Children IDs
    pub children: Vec<usize>,
}

/**
* Metadata about the AST.
*
* Note that hashes don't make sense for all nodes. E.g., the semicolon in Rust and C++ will have a
* leaf node that is repeated dozens or hundreds of times across a file. Those nodes will all have
* the exact same hash.
*/
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ASTMetadata {
    /// Map of node->hash. The hash is a full hash, hashing both the structure (types) and the
    /// values of the node and it's entire subtree, in order. The nodes are identified by their
    /// treesitter node id.
    pub node_to_full_hash: HashMap<usize, u64>,
    /// Reverse map to node_to_full_hash, going from <full hash> -> <treesitter node id>.
    /// Note that as mentioned above, many nodes will have the same hash, e.g. any variable
    /// declaration called "i" will hash to the same hash. Therefore, the map is actually going from
    /// a hash to a set of nodes.
    pub full_hash_to_node: HashMap<u64, HashSet<usize>>,
    /// Map of node->hash. The hash is a structural hash, hashing only the types of AST nodes in
    /// the subtree, not the value of the nodes. This hash is robust to changes like constant value
    /// changes. The nodes are identified by their treesitter node id.
    pub node_to_structural_hash: HashMap<usize, u64>,
    /// Reverse map to node_to_structural_hash, going from <structural hash> -> <node id>
    /// Note that as mentioned above, many nodes will have the same hash, e.g. any variable
    /// declaration will hash to the same structural hash. Therefore, the map value is a set.
    pub structural_hash_to_node: HashMap<u64, HashSet<usize>>,
    /// node.id() -> subtree size
    pub node_to_subtree_size: HashMap<usize, usize>,
    /// node.id() -> depth (root = 0, its children = 1, ...)
    pub node_to_depth: HashMap<usize, usize>,
    /// Set of reference nodes in this tree, ordered by subtree size.
    pub reference_nodes_ordered: Vec<usize>,
    /// Maps a (kind, identifier) pair, for example ('function_item', 'main') to a node_id.
    pub semantically_structural_nodes: HashMap<(String, String), usize>,
    /// Node information for each node, indexed by node_id.
    pub node_info: HashMap<usize, ASTNodeMetadata>,
    /// The language this tree was parsed as, so the cost model can consult
    /// [`crate::diff::nodes::kinds_update_allowed`] without threading a separate parameter
    /// through every APTED call site.
    pub language: Language,
}

/**
* The programming language.
*
* Implemented as a crate enum instead of reusing something like TreeSitter language to allow for
* better error handling of unknown or not-supported languages.
*/
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Language {
    #[default]
    Unknown,
    // Alphabetically ordered.
    Bazel,
    C,
    CPP,
    CSS,
    CSharp,
    Dart,
    Go,
    HTML,
    JSON,
    Java,
    JavaScript,
    Kotlin,
    LUA,
    Lisp,
    MarkDown,
    PHP,
    ProtoBuf,
    Python,
    R,
    Ruby,
    Rust,
    SQL,
    Scala,
    ShellScript,
    Swift,
    TSX,
    TypeScript,
    Vimscript,
    YAML,
    XML,
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

/**
* The type of "code". Very often, code files are not actually code, but rather configuration or
* data, or, much more rarely, documentation.
*
* The enums gross values separate the four big areas. Each instantiation can further contain an
* arbitrary string that provides fine grained information on what type of code, configuration,
* data or documentation exactly the file contents are.
*/
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Type {
    #[default]
    Unknown,
    Code(String),
    Configuration(String),
    Data(String),
    Documentation(String),
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::helper;

    #[test]
    fn code_from_empty_string() {
        let code = Code::from_string("", &Language::Rust);

        assert_eq!(code.contents, "");
        assert_eq!(code.metadata.language, Some(Language::Rust));

        assert!(code.ast.is_some());
    }

    #[test]
    fn code_from_file() -> Result<()> {
        let paths = helper::handmade_test_code_as_paths()?;
        let hello_world_path = paths
            .get("hello-world.rs")
            .expect("hello-world.rs should exist in test data");

        let code = Code::from_file(hello_world_path)?;

        assert_eq!(code.metadata.language, Some(Language::Rust));
        assert!(code.metadata.path.is_some());
        assert!(code.contents.contains("fn main()"));
        assert!(code.contents.contains("Hello, World"));

        assert!(code.ast.is_some());

        Ok(())
    }

    #[test]
    fn parse_code() -> Result<()> {
        let mut codes = helper::handmade_test_code()?;
        let hello_world = codes
            .get_mut("hello-world.rs")
            .expect("hello-world.rs should exist in test data");

        let language = hello_world
            .metadata
            .language
            .as_ref()
            .expect("Language should be set");

        let ts_language = crate::code::language::to_treesitter(language).expect("Unable to convert CodeDiff language to TreeSitter language in tests. Something is wrong with the test infrastructure.");

        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&ts_language)?;

        hello_world.parse(&mut parser);

        assert!(hello_world.ast.is_some());

        Ok(())
    }

    #[test]
    fn ast_metadata_computed_in_from_string() {
        let code = Code::from_string("fn main() { println!(\"Hello, World\"); }", &Language::Rust);

        assert!(code.metadata.ast_metadata.is_some());

        let ast_metadata = code.metadata.ast_metadata.as_ref().unwrap();
        assert!(!ast_metadata.node_to_full_hash.is_empty());
        assert!(!ast_metadata.full_hash_to_node.is_empty());
        assert!(!ast_metadata.node_to_structural_hash.is_empty());
        assert!(!ast_metadata.structural_hash_to_node.is_empty());
    }

    #[test]
    fn ast_metadata_computed_in_from_file() -> Result<()> {
        let paths = helper::handmade_test_code_as_paths()?;
        let hello_world_path = paths
            .get("hello-world.rs")
            .expect("hello-world.rs should exist in test data");

        let code = Code::from_file(hello_world_path)?;

        assert!(code.metadata.ast_metadata.is_some());

        let ast_metadata = code.metadata.ast_metadata.as_ref().unwrap();
        assert!(!ast_metadata.node_to_full_hash.is_empty());
        assert!(!ast_metadata.full_hash_to_node.is_empty());
        assert!(!ast_metadata.node_to_structural_hash.is_empty());
        assert!(!ast_metadata.structural_hash_to_node.is_empty());

        // Test that reference nodes are discovered and ordered
        assert!(!ast_metadata.reference_nodes_ordered.is_empty());

        Ok(())
    }

    #[test]
    fn ast_metadata_consistency() -> Result<()> {
        let paths = helper::handmade_test_code_as_paths()?;
        let hello_world_path = paths
            .get("hello-world.rs")
            .expect("hello-world.rs should exist in test data");

        // Read the file content
        let content = std::fs::read_to_string(hello_world_path)?;

        // Create code from file
        let code_from_file = Code::from_file(hello_world_path)?;

        // Create code from string
        let code_from_string = Code::from_string(&content, &Language::Rust);

        // Both should have AST metadata
        assert!(code_from_file.metadata.ast_metadata.is_some());
        assert!(code_from_string.metadata.ast_metadata.is_some());

        let metadata_from_file = code_from_file.metadata.ast_metadata.as_ref().unwrap();
        let metadata_from_string = code_from_string.metadata.ast_metadata.as_ref().unwrap();

        // The metadata should be identical (same content, same language)
        assert_eq!(
            metadata_from_file.node_to_full_hash.len(),
            metadata_from_string.node_to_full_hash.len()
        );
        assert_eq!(
            metadata_from_file.full_hash_to_node.len(),
            metadata_from_string.full_hash_to_node.len()
        );
        assert_eq!(
            metadata_from_file.node_to_structural_hash.len(),
            metadata_from_string.node_to_structural_hash.len()
        );
        assert_eq!(
            metadata_from_file.structural_hash_to_node.len(),
            metadata_from_string.structural_hash_to_node.len()
        );

        Ok(())
    }

    #[test]
    fn ensure_parsed_already_parsed_and_metadata_set() -> Result<()> {
        // Test case 1: Code is already parsed and metadata is set
        let mut code = Code::from_string("fn main() { println!(\"Hello\"); }", &Language::Rust);

        // Both AST and metadata should already be set by from_string
        assert!(code.ast.is_some());
        assert!(code.metadata.ast_metadata.is_some());

        // Store original metadata for comparison
        let original_metadata = code.metadata.ast_metadata.clone();

        // Call ensure_parsed - should do nothing and return Ok
        code.ensure_parsed()?;

        // Verify nothing changed (AST should still be Some, metadata should be unchanged)
        assert!(code.ast.is_some());
        assert_eq!(code.metadata.ast_metadata, original_metadata);

        Ok(())
    }

    #[test]
    fn ensure_parsed_parsed_but_no_metadata() -> Result<()> {
        // Test case 2: Code is parsed but metadata is not computed
        let mut code = Code {
            contents: "fn main() { println!(\"Hello\"); }".to_string(),
            metadata: Metadata {
                path: None,
                tip: Some(Type::Code("Code".to_string())),
                language: Some(Language::Rust),
                columns_for_row: None,
                ast_metadata: None, // Metadata not set
            },
            ..Default::default()
        };

        // Parse the code manually
        let mut parser = tree_sitter::Parser::new();
        code.parse(&mut parser);

        // AST should be set, but metadata should not be
        assert!(code.ast.is_some());
        assert!(code.metadata.ast_metadata.is_none());

        // Call ensure_parsed - should compute metadata
        code.ensure_parsed()?;

        // Verify AST is still set and metadata is now computed
        assert!(code.ast.is_some());
        assert!(code.metadata.ast_metadata.is_some());

        let metadata = code.metadata.ast_metadata.as_ref().unwrap();
        assert!(!metadata.node_to_full_hash.is_empty());
        assert!(!metadata.node_to_structural_hash.is_empty());

        Ok(())
    }

    #[test]
    fn ensure_parsed_not_parsed() -> Result<()> {
        // Test case 3: Code is not parsed at all
        let mut code = Code {
            contents: "fn main() { println!(\"Hello\"); }".to_string(),
            metadata: Metadata {
                path: None,
                tip: Some(Type::Code("Code".to_string())),
                language: Some(Language::Rust),
                columns_for_row: None,
                ast_metadata: None,
            },
            ..Default::default()
        };

        // Neither AST nor metadata should be set
        assert!(code.ast.is_none());
        assert!(code.metadata.ast_metadata.is_none());

        // Call ensure_parsed - should parse and compute metadata
        code.ensure_parsed()?;

        // Verify both AST and metadata are now set
        assert!(code.ast.is_some());
        assert!(code.metadata.ast_metadata.is_some());

        let metadata = code.metadata.ast_metadata.as_ref().unwrap();
        assert!(!metadata.node_to_full_hash.is_empty());
        assert!(!metadata.node_to_structural_hash.is_empty());

        Ok(())
    }

    #[test]
    fn ensure_parsed_no_language() {
        // Test case 4: Code has no language set - should return error
        let mut code = Code {
            contents: "fn main() { println!(\"Hello\"); }".to_string(),
            metadata: Metadata {
                path: None,
                tip: Some(Type::Code("Code".to_string())),
                language: None, // No language set
                columns_for_row: None,
                ast_metadata: None,
            },
            ..Default::default()
        };

        // Call ensure_parsed - should return error
        let result = code.ensure_parsed();

        // Verify it returns an error
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Language must be set")
        );

        // Verify nothing changed
        assert!(code.ast.is_none());
        assert!(code.metadata.ast_metadata.is_none());
    }

    #[test]
    fn ensure_parsed_unsupported_language() {
        // Test case 5: Code has unsupported language - should return error
        let mut code = Code {
            contents: "fn main() { println!(\"Hello\"); }".to_string(),
            metadata: Metadata {
                path: None,
                tip: Some(Type::Code("Code".to_string())),
                language: Some(Language::Bazel), // Bazel is not supported by tree-sitter
                columns_for_row: None,
                ast_metadata: None,
            },
            ..Default::default()
        };

        // Call ensure_parsed - should return error for unsupported language
        let result = code.ensure_parsed();

        // Verify it returns an error
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("not supported by tree-sitter")
        );

        // Verify nothing changed
        assert!(code.ast.is_none());
        assert!(code.metadata.ast_metadata.is_none());
    }
}
