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
#[cfg(test)]
mod gap_survey;
pub mod hash;
pub mod language;
pub mod metadata;
pub mod similarity;
mod similarity_corpus_tests;
pub mod tip; // `type` is a Rust keyword, so Croatian.

use anyhow::{Result, anyhow};
use std::fmt;

/// Source code and everything derived from it.
///
/// Consumers must not assume any derived field is set: check, compute it if possible, and otherwise
/// fail safe with a zero result. That keeps large files and data-that-looks-like-code cheap, and is
/// why the derived fields are `Option`s.
#[derive(Debug, Default)]
pub struct Code {
    /// The actual code.
    pub contents: String,
    /// The metadata about the code.
    pub metadata: Metadata,
    /// The AST.
    pub ast: Option<tree_sitter::Tree>,
}

/// Hand-written, not derived: `tree_sitter::Tree::clone()` gives the root node a new `id()`, so
/// id-keyed `ast_metadata` would point at a node the clone does not have. The clone drops it, so
/// `ast_metadata` ids match its own `ast` by construction; it is recomputed on first use.
impl Clone for Code {
    fn clone(&self) -> Self {
        Code {
            contents: self.contents.clone(),
            metadata: Metadata {
                ast_metadata: None,
                ..self.metadata.clone()
            },
            ast: self.ast.clone(),
        }
    }
}

impl Code {
    /// Parses `contents` into `ast`; a no-op when the language is unset or has no grammar.
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

    /// Parses and computes `ast_metadata` where either is missing.
    ///
    /// Errors if the language is unset or has no tree-sitter grammar.
    pub fn ensure_parsed(&mut self) -> Result<()> {
        let language = match self.metadata.language.as_ref() {
            Some(lang) => lang,
            None => return Err(anyhow!("Language must be set to parse code")),
        };

        let needs_parsing = self.ast.is_none();
        let needs_metadata = self.metadata.ast_metadata.is_none();
        if !needs_parsing && !needs_metadata {
            return Ok(());
        }

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

        if needs_metadata && self.ast.is_some() {
            self.metadata.ast_metadata = Some(crate::code::metadata::compute_ast_metadata(self)?);
        }

        Ok(())
    }

    /// Parses `contents` as `language`, with `tip` set to `Type::Code` whatever the content is.
    // TODO: recognize configuration (e.g. docker-compose YAML) by content; needs content-aware
    // detection in language.rs.
    pub fn from_string(contents: &str, language: &Language) -> Self {
        let mut code = Code {
            contents: contents.to_string(),
            metadata: Metadata {
                path: None,
                tip: Some(Type::Code("Code".to_string())),
                language: Some(*language),
                ast_metadata: None,
            },
            ..Default::default()
        };

        let mut parser = tree_sitter::Parser::new();
        code.parse(&mut parser);

        // No AST (no grammar) is a valid state, not an error to log. A real failure cannot be
        // returned from this infallible constructor, so it is printed rather than swallowed.
        if code.ast.is_some() {
            match crate::code::metadata::compute_ast_metadata(&code) {
                Ok(ast_metadata) => code.metadata.ast_metadata = Some(ast_metadata),
                Err(e) => eprintln!("Failed to compute AST metadata: {:?}", e),
            }
        }

        code
    }

    /// Reads and parses `path`, detecting the language from its path and content (`Unknown` if
    /// unrecognized). Errors if the file cannot be read as UTF-8 text.
    // TODO: use the hermetic expansion from metadata.rs to expand the metadata.
    pub fn from_file(path: &std::path::Path) -> Result<Self> {
        use std::fs;

        let contents = fs::read_to_string(path)
            .map_err(|e| anyhow!("Failed to read file {}: {}", path.display(), e))?;

        let language =
            language::language_for_path_and_content(path, &contents).unwrap_or(Language::Unknown);

        let mut code = Code::from_string(&contents, &language);
        code.metadata.path = Some(path.to_path_buf());

        Ok(code)
    }
}

/// Whether `path` holds bytes `Code::from_file` cannot read as text. I/O failures propagate.
///
/// Deliberately `from_file`'s own failure condition (invalid UTF-8), not git's NUL-byte heuristic:
/// the two must agree on every input, and a Latin-1 source file has no NUL yet fails to decode.
pub fn is_binary_file(path: &std::path::Path) -> Result<bool> {
    let bytes = std::fs::read(path)
        .map_err(|e| anyhow!("Failed to read file {}: {}", path.display(), e))?;
    Ok(std::str::from_utf8(&bytes).is_err())
}

/// What diffing needs to know about the code, beyond the code itself. Statistics and test data do
/// not belong here.
#[derive(Debug, Clone, Default)]
pub struct Metadata {
    pub path: Option<std::path::PathBuf>,
    /// Whether this is code, configuration, data or documentation (`type` is a keyword).
    pub tip: Option<Type>,
    pub language: Option<Language>,
    pub ast_metadata: Option<ASTMetadata>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ASTNodeMetadata {
    pub kind: String,
    /// The node's text, for leaves; empty for an internal node, whose own text is only hashed
    /// into `owned_text_hash` - copying every subtree's text would dominate metadata build time.
    pub text: String,
    /// A hash of the non-whitespace text this node owns *directly*, in the gaps around its
    /// children; 0 for leaves and for nodes whose children cover every byte.
    ///
    /// Some grammars keep a construct's payload as parent-owned text (XML's `AttValue`, CSS's
    /// `integer_value`, YAML's quoted scalars); without this `UnitCostModel::ren` prices changing
    /// it at zero. A hash, not text, because `ren` runs per APTED DP cell.
    pub owned_text_hash: u64,
    pub children: Vec<usize>,
    /// Byte offset where this node starts in the source.
    ///
    /// Node ids are arena slots that differ between parses of identical source, so a tie-break
    /// that must be reproducible sorts by this instead. Not unique (an ancestor shares it with its
    /// leftmost descendant): pair with `preorder_index` when uniqueness matters.
    pub start_byte: usize,
    /// Index in a preorder walk of the tree: unique, and stable across parses like `start_byte`.
    pub preorder_index: usize,
    /// tree-sitter's `Node::is_named`: `false` for anonymous tokens (`import`, `{`, `->`). A
    /// keyword can share its kind with its statement (Kotlin's `import`), and reference-node
    /// discovery must not make the keyword a hash-matching candidate.
    pub is_named: bool,
    /// Precomputed kind-membership answers; see [`KindCostClass`].
    pub kind_cost_class: KindCostClass,
}

impl ASTNodeMetadata {
    /// A hand-built node (for test fixtures) with `kind_cost_class` derived from `kind`, owning no
    /// text and named. The production builder is `code::metadata::compute_node_info`.
    pub fn new(
        kind: String,
        text: String,
        children: Vec<usize>,
        start_byte: usize,
        preorder_index: usize,
    ) -> Self {
        let kind_cost_class = KindCostClass {
            identifier_like: crate::diff::nodes::is_identifier_kind(&kind),
            literal_like: crate::diff::nodes::is_literal_kind(&kind),
            operator_families: crate::diff::nodes::operator_family_mask(&kind),
        };
        ASTNodeMetadata {
            kind,
            text,
            owned_text_hash: 0,
            children,
            start_byte,
            preorder_index,
            is_named: true,
            kind_cost_class,
        }
    }
}

/// The parts of a node's *kind* that [`crate::diff::apted`]'s rename-cost model needs, reduced to
/// integer/boolean form once per node at metadata-build time.
///
/// Purely a performance representation of what [`crate::diff::nodes::kinds_update_allowed`] and
/// [`crate::diff::nodes::is_literal_kind`] answer from the kind string, derived from the same
/// arrays: `ren` runs once per DP cell, and this moves the string scans to an O(n) precompute.
///
/// Language-independent: `operator_families` records every family, and the comparison picks the
/// subset for its own language ([`crate::diff::nodes::language_operator_family_mask`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KindCostClass {
    /// `IDENTIFIER_KINDS.contains(kind)` - name-like leaves that may match each other across
    /// differing kinds in every language.
    pub identifier_like: bool,
    /// `is_literal_kind(kind)` - selects `COST_LITERAL_UPDATE` for a changed value, a separate
    /// tier kept for tuning that currently equals `COST_UPDATE`.
    pub literal_like: bool,
    /// Bit `i` set iff this kind is in `ALL_OPERATOR_FAMILIES[i]` (see
    /// [`crate::diff::nodes::operator_family_mask`]). A kind can be in several families at once.
    pub operator_families: crate::diff::nodes::FamilyMask,
}

/// Per-tree hashes and indexes, keyed by tree-sitter node id.
///
/// Every map is an `FxHashMap`: SipHash's per-process reseed makes lookup time on these small
/// integer keys vary by an order of magnitude between runs, and the ancestor walk in APTED's DP does
/// a lookup per step. Reverse maps hold a `Vec` in deterministic traversal order, not a set, so the
/// duplicate a caller picks first is reproducible; many nodes share a hash (every `;`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ASTMetadata {
    /// Full hash: kinds and values of the whole subtree, in order.
    pub node_to_full_hash: rustc_hash::FxHashMap<usize, u64>,
    /// Reverse of `node_to_full_hash`, in `hash::hash_code`'s traversal order.
    pub full_hash_to_node: rustc_hash::FxHashMap<u64, Vec<usize>>,
    /// Structural hash: the subtree's kinds only, in order, so a changed value keeps it.
    pub node_to_structural_hash: rustc_hash::FxHashMap<usize, u64>,
    pub structural_hash_to_node: rustc_hash::FxHashMap<u64, Vec<usize>>,
    /// Kind+value hash, order-independent inside `nodes::is_commutative_container` at every
    /// level. For identical-subtree matching (`solve_hash_descent`); order-sensitive equality
    /// uses `node_to_full_hash`.
    pub node_to_kind_and_value_hash: rustc_hash::FxHashMap<usize, u64>,
    pub kind_and_value_hash_to_node: rustc_hash::FxHashMap<u64, Vec<usize>>,
    /// Kind-only hash, order-independent like `node_to_kind_and_value_hash`. For same-shape,
    /// differing-leaves matching (`solve_hash_descent`).
    pub node_to_kind_only_hash: rustc_hash::FxHashMap<usize, u64>,
    pub kind_only_hash_to_node: rustc_hash::FxHashMap<u64, Vec<usize>>,
    /// A bottom-k MinHash sketch of the leaf hashes in the subtree: "how nearly the same?" for two
    /// known nodes in O(k), where the Merkle hashes above only answer "identical?". See
    /// [`crate::code::similarity`].
    pub node_to_similarity_sketch:
        rustc_hash::FxHashMap<usize, crate::code::similarity::SimilaritySketch>,
    pub node_to_subtree_size: rustc_hash::FxHashMap<usize, usize>,
    /// `(count, node_id)` of the node with the most *direct* children in this subtree (inclusive),
    /// for `solve_large_flat_subtrees::largest_flat_container_in` in O(1).
    pub node_to_widest_subtree_node: rustc_hash::FxHashMap<usize, (usize, usize)>,
    /// node.id() -> depth (root = 0, its children = 1, ...)
    pub node_to_depth: rustc_hash::FxHashMap<usize, usize>,
    /// Child id -> parent id for every non-root node.
    pub node_to_parent: rustc_hash::FxHashMap<usize, usize>,
    /// Reference nodes, ordered by subtree size.
    pub reference_nodes_ordered: Vec<usize>,
    pub node_info: rustc_hash::FxHashMap<usize, ASTNodeMetadata>,
    /// The language this tree was parsed as, for the cost model.
    pub language: Language,
}

impl ASTMetadata {
    /// Whether `id` has no children; `false` for an unknown id.
    pub fn is_leaf(&self, id: usize) -> bool {
        self.node_info
            .get(&id)
            .is_some_and(|info| info.children.is_empty())
    }
}

/// The programming language: a crate enum rather than a tree-sitter language, so unknown and
/// grammar-less languages are representable.
#[derive(Debug, Clone, Copy, Default, PartialEq, strum::EnumIter)]
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
    XML,
    YAML,
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

/// What a "code" file really is; the string refines the variant (e.g. which configuration).
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
    #[test]
    fn is_binary_file_agrees_with_from_file_on_valid_utf8() {
        let mut file = tempfile::NamedTempFile::new().expect("create temp file");
        std::io::Write::write_all(&mut file, "fn main() {}\n".as_bytes()).expect("write");
        assert!(!is_binary_file(file.path()).expect("classify"));
        assert!(Code::from_file(file.path()).is_ok());
    }

    #[test]
    fn is_binary_file_agrees_with_from_file_on_invalid_utf8() {
        let mut file = tempfile::NamedTempFile::new().expect("create temp file");
        std::io::Write::write_all(&mut file, b"%PDF-1.7\n\xff\xfe\x00binary").expect("write");
        assert!(is_binary_file(file.path()).expect("classify"));
        assert!(Code::from_file(file.path()).is_err());
    }

    /// git passes `/dev/null` for the missing side of an added or deleted file.
    #[cfg(unix)]
    #[test]
    fn is_binary_file_says_dev_null_is_not_binary() {
        assert!(!is_binary_file(std::path::Path::new("/dev/null")).expect("classify"));
    }

    /// git's NUL-byte heuristic would call this binary; `from_file` reads it fine.
    #[test]
    fn is_binary_file_says_valid_utf8_containing_a_nul_byte_is_not_binary() {
        let mut file = tempfile::NamedTempFile::new().expect("create temp file");
        std::io::Write::write_all(&mut file, b"let s = \"a\x00b\";\n").expect("write");
        assert!(!is_binary_file(file.path()).expect("classify"));
        assert!(Code::from_file(file.path()).is_ok());
    }

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
    fn code_from_string_skips_ast_metadata_when_the_language_has_no_grammar() {
        let code = Code::from_string("", &Language::Unknown);

        assert!(code.ast.is_none());
        assert!(code.metadata.ast_metadata.is_none());
    }

    #[test]
    fn cloning_code_drops_ast_metadata_so_its_ids_cannot_outlive_the_tree() -> Result<()> {
        let code = Code::from_string("fn main() { let x = 1; }", &Language::Rust);
        assert!(code.metadata.ast_metadata.is_some());

        let mut clone = code.clone();
        assert!(clone.ast.is_some());
        assert!(clone.metadata.ast_metadata.is_none());

        clone.ensure_parsed()?;
        let root = clone.ast.as_ref().unwrap().root_node().id();
        let metadata = clone.metadata.ast_metadata.as_ref().unwrap();
        assert!(metadata.node_info.contains_key(&root));
        Ok(())
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

        assert!(!ast_metadata.reference_nodes_ordered.is_empty());

        Ok(())
    }

    #[test]
    fn ast_metadata_consistency() -> Result<()> {
        let paths = helper::handmade_test_code_as_paths()?;
        let hello_world_path = paths
            .get("hello-world.rs")
            .expect("hello-world.rs should exist in test data");

        let content = std::fs::read_to_string(hello_world_path)?;

        let code_from_file = Code::from_file(hello_world_path)?;

        let code_from_string = Code::from_string(&content, &Language::Rust);

        assert!(code_from_file.metadata.ast_metadata.is_some());
        assert!(code_from_string.metadata.ast_metadata.is_some());

        let metadata_from_file = code_from_file.metadata.ast_metadata.as_ref().unwrap();
        let metadata_from_string = code_from_string.metadata.ast_metadata.as_ref().unwrap();

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
        let mut code = Code::from_string("fn main() { println!(\"Hello\"); }", &Language::Rust);

        assert!(code.ast.is_some());
        assert!(code.metadata.ast_metadata.is_some());

        let original_metadata = code.metadata.ast_metadata.clone();

        code.ensure_parsed()?;

        assert!(code.ast.is_some());
        assert_eq!(code.metadata.ast_metadata, original_metadata);

        Ok(())
    }

    #[test]
    fn ensure_parsed_parsed_but_no_metadata() -> Result<()> {
        let mut code = Code {
            contents: "fn main() { println!(\"Hello\"); }".to_string(),
            metadata: Metadata {
                path: None,
                tip: Some(Type::Code("Code".to_string())),
                language: Some(Language::Rust),
                ast_metadata: None,
            },
            ..Default::default()
        };

        let mut parser = tree_sitter::Parser::new();
        code.parse(&mut parser);

        assert!(code.ast.is_some());
        assert!(code.metadata.ast_metadata.is_none());

        code.ensure_parsed()?;

        assert!(code.ast.is_some());
        assert!(code.metadata.ast_metadata.is_some());

        let metadata = code.metadata.ast_metadata.as_ref().unwrap();
        assert!(!metadata.node_to_full_hash.is_empty());
        assert!(!metadata.node_to_structural_hash.is_empty());

        Ok(())
    }

    #[test]
    fn ensure_parsed_not_parsed() -> Result<()> {
        let mut code = Code {
            contents: "fn main() { println!(\"Hello\"); }".to_string(),
            metadata: Metadata {
                path: None,
                tip: Some(Type::Code("Code".to_string())),
                language: Some(Language::Rust),
                ast_metadata: None,
            },
            ..Default::default()
        };

        assert!(code.ast.is_none());
        assert!(code.metadata.ast_metadata.is_none());

        code.ensure_parsed()?;

        assert!(code.ast.is_some());
        assert!(code.metadata.ast_metadata.is_some());

        let metadata = code.metadata.ast_metadata.as_ref().unwrap();
        assert!(!metadata.node_to_full_hash.is_empty());
        assert!(!metadata.node_to_structural_hash.is_empty());

        Ok(())
    }

    #[test]
    fn ensure_parsed_no_language() {
        let mut code = Code {
            contents: "fn main() { println!(\"Hello\"); }".to_string(),
            metadata: Metadata {
                path: None,
                tip: Some(Type::Code("Code".to_string())),
                language: None,
                ast_metadata: None,
            },
            ..Default::default()
        };

        let result = code.ensure_parsed();

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Language must be set")
        );

        assert!(code.ast.is_none());
        assert!(code.metadata.ast_metadata.is_none());
    }

    #[test]
    fn ensure_parsed_unsupported_language() {
        let mut code = Code {
            contents: "fn main() { println!(\"Hello\"); }".to_string(),
            metadata: Metadata {
                path: None,
                tip: Some(Type::Code("Code".to_string())),
                language: Some(Language::Bazel), // Bazel is not supported by tree-sitter
                ast_metadata: None,
            },
            ..Default::default()
        };

        let result = code.ensure_parsed();

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("not supported by tree-sitter")
        );

        assert!(code.ast.is_none());
        assert!(code.metadata.ast_metadata.is_none());
    }
}
