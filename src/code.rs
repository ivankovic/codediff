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
pub mod tip; // Since type is a reserved keyword in Rust, we use Croatian instead.

use anyhow::{Result, anyhow};
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
#[derive(Debug, Default)]
pub struct Code {
    /// The actual code.
    pub contents: String,
    /// The metadata about the code.
    pub metadata: Metadata,
    /// The AST.
    pub ast: Option<tree_sitter::Tree>,
}

/**
* Hand-written, not `#[derive(Clone)]`: `tree_sitter::Tree::clone()` does not preserve the root
* node's `id()` (confirmed empirically 2026-07-26 - out of 33 nodes in a small fixture, exactly 32
* kept their id across a clone; only the root changed, every time, deterministically). Every other
* `Metadata` field is a pure function of `contents`/the parsed structure, so it survives a clone
* fine, but `ast_metadata` is a `HashMap`-of-maps keyed by node id (`ASTMetadata::node_info` and
* friends) - if it were cloned verbatim alongside a freshly-recloned `ast`, its cached root-id
* entries would silently point at a node that no longer exists in the clone's own tree, corrupting
* any lookup keyed on the old root id (this is exactly what `diff_identical_rust_code` and 111
* other tests caught when `ensure_parsed` was first tried in the test fixture loader - see
* TODO.md's "Found the real bottleneck" entry).
*
* Dropping `ast_metadata` back to `None` on every clone makes "an `ast_metadata`'s node ids always
* match its own `ast`'s node ids" hold by construction, for every one of this codebase's ~85 call
* sites, rather than trusting each one individually to re-derive metadata after cloning. The cost:
* a clone that goes on to get diffed pays one `ensure_parsed`-style recompute the first time
* `metadata_of` needs it (same as an un-cached `Code` always has) - callers that never clone at all
* (e.g. `benchmark_other`'s hot path, which reads `&Code` straight out of its fixture cache) are
* unaffected and keep the full caching benefit.
*/
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
                ast_metadata: None,
            },
            ..Default::default()
        };

        // Parse the code to populate the AST
        let mut parser = tree_sitter::Parser::new();
        code.parse(&mut parser);

        // Compute AST metadata, but only once there's an AST to compute it from: an unrecognized
        // language (or a grammar `to_treesitter` doesn't map, see `Code::parse`) leaves `ast` as
        // `None`, which is an expected, valid state - not a bug - and matches how `ensure_parsed`
        // and `metadata::metadata_of` both already treat a parse-less `Code` elsewhere. Before this
        // guard, every `Code::from_string`/`from_file` call for such a language unconditionally
        // logged "Failed to compute AST metadata: AST must be parsed before hashing" to stderr,
        // even though nothing had actually gone wrong.
        //
        // `from_string` is infallible by signature (unlike `ensure_parsed`, which propagates this
        // same error), so a genuine failure here still can't be returned - it's surfaced via
        // `eprintln!` instead of being swallowed silently, so a real `compute_ast_metadata` bug
        // (as opposed to this expected no-AST case) stays visible.
        if code.ast.is_some() {
            match crate::code::metadata::compute_ast_metadata(&code) {
                Ok(ast_metadata) => code.metadata.ast_metadata = Some(ast_metadata),
                Err(e) => eprintln!("Failed to compute AST metadata: {:?}", e),
            }
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
    /// Byte offset where this node starts in the source.
    ///
    /// Tree-sitter node ids are arena slots - stable within one parse, but not across separate
    /// parses of identical source (allocator/arena layout can differ run to run). Any tie-break
    /// that needs to be reproducible *across* parses (and therefore across process launches) must
    /// sort by a property of the source text, not by node id. `start_byte` is exactly that: a
    /// pure function of the parse tree's shape, identical for the "same" node no matter which
    /// process or arena produced it.
    ///
    /// Not unique on its own: an ancestor and its leftmost descendant (e.g. an
    /// `expression_statement` and the `call_expression` that starts it) always share a
    /// `start_byte`. Pair with `preorder_index` wherever a tie-break needs to be both parse-stable
    /// *and* guaranteed unique.
    pub start_byte: usize,
    /// This node's index in a preorder (root, then children left to right) walk of the tree.
    /// Unique per node and, like `start_byte`, a pure function of the tree's shape - not the
    /// arena that happened to produce it - so it's safe to use across separate parses.
    pub preorder_index: usize,
    /// Precomputed answers to the kind-membership questions
    /// [`crate::diff::nodes::kinds_update_allowed`] and [`crate::diff::nodes::is_literal_kind`]
    /// would otherwise re-derive by string scanning - see [`KindCostClass`] for why this is
    /// precomputed rather than computed on demand.
    pub kind_cost_class: KindCostClass,
}

impl ASTNodeMetadata {
    /// Builds a node's metadata with [`kind_cost_class`](ASTNodeMetadata::kind_cost_class) derived
    /// from `kind`, so no caller has to know how that derivation works (or can get it wrong by
    /// hand). The production builder (`code::metadata::compute_node_info`) fills the struct
    /// literally, field by field, because it already computes each field from the tree-sitter node
    /// - this is for everything else, principally test fixtures.
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
            children,
            start_byte,
            preorder_index,
            kind_cost_class,
        }
    }
}

/// The parts of a node's *kind* that [`crate::diff::apted`]'s rename-cost model needs, reduced to
/// integer/boolean form once per node at metadata-build time.
///
/// Purely a performance representation - it answers exactly the questions
/// [`crate::diff::nodes::kinds_update_allowed`] and [`crate::diff::nodes::is_literal_kind`]
/// already answer from the kind string, and is derived from the very same `const` family arrays
/// (see [`crate::diff::nodes::operator_family_mask`]), so the two can't disagree.
///
/// Why it exists: `UnitCostModel::ren` is evaluated once per tree-edit-distance DP cell - O(n1*n2)
/// times and more - and each evaluation used to walk `IDENTIFIER_KINDS` and up to six operator
/// family arrays comparing `&str`s, tens of string comparisons per cell. Every input to those
/// scans depends only on the node's kind, never on the pair, so the whole cost moves from the
/// O(n^2) inner loop to an O(n) precompute (measured 2026-08-17: roughly half of all APTED time
/// was in `ren` plus the containment adjustment it feeds).
///
/// Language-independent by construction: `operator_families` records membership in *every* family,
/// and which subset applies is decided at comparison time from the cost model's own language (see
/// [`crate::diff::nodes::language_operator_family_mask`]). That keeps this correct even when a
/// node's own tree was parsed as a different language than the comparison is being run under.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KindCostClass {
    /// `IDENTIFIER_KINDS.contains(kind)` - name-like leaves that may match each other across
    /// differing kinds in every language.
    pub identifier_like: bool,
    /// `is_literal_kind(kind)` - drives the higher `COST_LITERAL_UPDATE` for a changed value.
    pub literal_like: bool,
    /// Bit `i` set iff this kind is in `ALL_OPERATOR_FAMILIES[i]` (see
    /// [`crate::diff::nodes::operator_family_mask`]). A kind can be in several families at once.
    pub operator_families: u8,
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
    ///
    /// `FxHashMap`, not `std::collections::HashMap`: see `node_to_parent`'s doc comment below for
    /// the measured rationale (SipHash's per-process random reseed causes real, confirmed 10x
    /// run-to-run wall-time variance on this exact key shape). Every `ASTMetadata` map keyed by
    /// node id or hash value shares that same small-integer-key shape, so all of them got the same
    /// treatment (2026-07-26) once the pattern was found not to be applied consistently -
    /// `node_to_parent` was the only one converted at the time the variance was first diagnosed.
    pub node_to_full_hash: rustc_hash::FxHashMap<usize, u64>,
    /// Reverse map to node_to_full_hash, going from <full hash> -> <treesitter node ids>.
    /// Note that as mentioned above, many nodes will have the same hash, e.g. any variable
    /// declaration called "i" will hash to the same hash. Therefore, the map is actually going from
    /// a hash to a list of nodes.
    ///
    /// Deliberately a `Vec`, not a `HashSet`: nodes are pushed in the same deterministic traversal
    /// order every time (see `hash::hash_code`), so which duplicate a caller picks first (e.g.
    /// `hash_tree_matching::solve_with_hash_map`'s candidate selection) is reproducible run to
    /// run. A `HashSet` here previously made that choice depend on the hasher's per-instance random
    /// seed, silently changing codediff's output between otherwise-identical runs whenever a hash
    /// had more than one node (see `describe_nondeterminism` in test/helper/human_mapping.rs).
    /// There are never true duplicate entries within one list (each node is visited exactly once),
    /// so this loses nothing a `HashSet` provided. See `node_to_full_hash` for why the outer map
    /// itself is `FxHashMap` - the *values* being `Vec`s (not `HashSet`s) is what already made the
    /// *content* order-independent-safe; nothing here ever iterates the outer map directly in an
    /// order-sensitive way (checked 2026-07-26 before converting).
    pub full_hash_to_node: rustc_hash::FxHashMap<u64, Vec<usize>>,
    /// Map of node->hash. The hash is a structural hash, hashing only the types of AST nodes in
    /// the subtree, not the value of the nodes. This hash is robust to changes like constant value
    /// changes. The nodes are identified by their treesitter node id.
    pub node_to_structural_hash: rustc_hash::FxHashMap<usize, u64>,
    /// Reverse map to node_to_structural_hash, going from <structural hash> -> <node ids>. See
    /// `full_hash_to_node` for why this is a `Vec`, not a `HashSet`.
    pub structural_hash_to_node: rustc_hash::FxHashMap<u64, Vec<usize>>,
    /// Kind+value hash, order-independent per `nodes::is_commutative_container` at *every*
    /// recursion level (not just the top - see `hash::compute_kind_and_value_hash`'s doc comment
    /// for the propagation-bug fix this depends on, and `TODO.md` for the pipeline rework this
    /// hash was built for). Used for byte-identical-subtree matching (`solve_hash_descent`),
    /// alongside `node_to_full_hash` (the order-*dependent* full hash, still used wherever
    /// document-order-sensitive content equality is needed - APTED's own cost model
    /// (`apted/common.rs`), `solve_moved_subtrees`, `solve_greedy_anchor_blocks`,
    /// `solve_identical_diagnostic_statements`).
    pub node_to_kind_and_value_hash: rustc_hash::FxHashMap<usize, u64>,
    /// Reverse map for `node_to_kind_and_value_hash`. See `full_hash_to_node` for why this is a
    /// `Vec`, not a `HashSet`.
    pub kind_and_value_hash_to_node: rustc_hash::FxHashMap<u64, Vec<usize>>,
    /// Structural (kind-only) hash, order-independent per `nodes::is_commutative_container` at
    /// every recursion level. Used for same-shape/differing-leaves matching (`solve_hash_descent`)
    /// - see `TODO.md`'s "New hash algorithms" section for the design.
    pub node_to_kind_only_hash: rustc_hash::FxHashMap<usize, u64>,
    /// Reverse map for `node_to_kind_only_hash`.
    pub kind_only_hash_to_node: rustc_hash::FxHashMap<u64, Vec<usize>>,
    /// node.id() -> a bottom-k MinHash sketch of the *leaf* hashes in that node's subtree.
    ///
    /// The one map here that does not answer "identical?". All four hashes above are Merkle
    /// hashes: one changed token makes them differ, and they then say nothing about *how much*
    /// differs. This answers "how nearly the same?" in O(k) for any two nodes, without touching
    /// either subtree - see [`crate::code::similarity`] for why that question keeps coming up and
    /// why the sketch covers leaves rather than all descendants. There is deliberately no reverse
    /// map: a sketch is for comparing two known nodes, not for looking a node up by content
    /// (which is what `full_hash_to_node` and friends are for).
    pub node_to_similarity_sketch:
        rustc_hash::FxHashMap<usize, crate::code::similarity::SimilaritySketch>,
    /// node.id() -> subtree size
    pub node_to_subtree_size: rustc_hash::FxHashMap<usize, usize>,
    /// node.id() -> `(count, node_id)` of the node with the most *direct* children found
    /// anywhere in this node's own subtree (inclusive of itself). Lets
    /// `solve_large_flat_subtrees::largest_flat_container_in` answer "does this subtree contain
    /// a node with >= N direct children, and which one" in O(1) instead of a per-query BFS -
    /// `node_to_subtree_size` alone isn't enough for that (a node's *total* descendant count
    /// says nothing about whether any single one of them has many *direct* children; a subtree
    /// can easily have hundreds of nodes while every individual node in it has only 2-3
    /// children).
    pub node_to_widest_subtree_node: rustc_hash::FxHashMap<usize, (usize, usize)>,
    /// node.id() -> depth (root = 0, its children = 1, ...)
    pub node_to_depth: rustc_hash::FxHashMap<usize, usize>,
    /// child node.id() -> parent node.id(), covering every non-root node. `ASTNodeMetadata` has
    /// no parent pointer, so this is derived once here from `node_info`'s children lists, rather
    /// than every ancestor/containment check re-deriving its own copy from scratch.
    ///
    /// `FxHashMap`, not `std::collections::HashMap`: this map backs `ContainmentCtx`'s
    /// `is_ancestor_or_self` ancestor walk (`apted/common.rs`), which does a `.get()` per step of
    /// an O(depth) walk on every `vren_adjusted` call inside APTED's core DP - an enormous number
    /// of lookups on any fixture with real containment. The default hasher (`SipHash`) is
    /// correctness-fine but randomly reseeded per process, so its collision behavior for this
    /// specific integer key set varies run to run - confirmed empirically (2026-07-16): identical
    /// input/output (same residual forest, fingerprinted by sorted `start_byte`, byte-identical
    /// across runs) but wall time on `kotlin-nextcloud-a-few-small-removals` ranged 2.8s-26.4s
    /// across separate process invocations, CPU-bound the whole time (`User time` ≈ `Elapsed`,
    /// ruling out scheduling/IO noise). `FxHashMap` is unseeded (deterministic performance) and
    /// faster on small integer keys regardless.
    pub node_to_parent: rustc_hash::FxHashMap<usize, usize>,
    /// Set of reference nodes in this tree, ordered by subtree size.
    pub reference_nodes_ordered: Vec<usize>,
    /// Node information for each node, indexed by node_id.
    pub node_info: rustc_hash::FxHashMap<usize, ASTNodeMetadata>,
    /// The language this tree was parsed as, so the cost model can consult
    /// [`crate::diff::nodes::kinds_update_allowed`] without threading a separate parameter
    /// through every APTED call site.
    pub language: Language,
}

impl ASTMetadata {
    /// Whether `id` is a leaf (has no children) in this tree. `false` for an id with no
    /// `node_info` entry, matching the conservative default every existing call site already
    /// used before this helper was extracted.
    pub fn is_leaf(&self, id: usize) -> bool {
        self.node_info
            .get(&id)
            .is_some_and(|info| info.children.is_empty())
    }
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

    /// `Language::Unknown` has no tree-sitter grammar (`to_treesitter` returns `None`), so
    /// `Code::parse` leaves `ast` unset - an expected, valid outcome (e.g. every add/delete-file
    /// diff run through `tui::app::compute_diff`'s `/dev/null` fallback starts from exactly this
    /// state before it's corrected to the other side's language). `compute_ast_metadata` must not
    /// even be attempted in that case: it requires a parsed AST and previously logged a spurious
    /// "Failed to compute AST metadata" error to stderr on every such call despite nothing being
    /// actually wrong.
    #[test]
    fn code_from_string_skips_ast_metadata_when_the_language_has_no_grammar() {
        let code = Code::from_string("", &Language::Unknown);

        assert!(code.ast.is_none());
        assert!(code.metadata.ast_metadata.is_none());
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
