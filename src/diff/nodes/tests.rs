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
// Split out of nodes.rs (formerly its trailing #[cfg(test)] mod tests block plus a second,
// separately-nested #[cfg(test)] mod is_commutative_container_tests further down - the two are
// merged into this one file/module) purely to shrink that file's visible size and stop production
// code (is_statement_sequence_body, is_commutative_container) from sitting between two test
// blocks. No behavior change - is_commutative_container_tests's own two local helpers
// (assert_recognizes, find_kind) and its `use crate::code::{Code, Language}` are folded in
// directly rather than kept as a nested module, since nothing else needs that extra layer and
// `use super::*` below already covers what its `use super::is_commutative_container` did.

use anyhow::Result;

use crate::code::{Code, Language};
use crate::test::helper;

use super::*;

/// The property the whole structural definition exists for: the visible set of a file is a
/// function of that file alone, so diffing it against two completely different counterparts -
/// or against nothing - must yield the identical set. The previous, renderer-derived definition
/// failed this by construction, which is what made it unusable as a metric (see
/// `is_structurally_visible`'s doc comment).
#[test]
fn structural_visibility_does_not_depend_on_what_the_file_is_diffed_against() {
    let subject = Code::from_string(
        "fn main() {\n    let x = 1;\n    foo(x); // note\n}\n",
        &Language::Rust,
    );
    let baseline = structurally_visible_node_ids(&subject);
    assert!(!baseline.is_empty());

    for counterpart in [
        "fn main() {\n    let x = 1;\n    foo(x); // note\n}\n", // identical
        "fn main() {\n    let y = 2;\n    bar(y); // other\n}\n", // every leaf changed
        "",                                                      // nothing at all
        "struct Wholly { different: bool }\n",                   // unrelated shape
    ] {
        let other = Code::from_string(counterpart, &Language::Rust);
        let _ = crate::diff::diff_code(&subject, &other);
        assert_eq!(
            structurally_visible_node_ids(&subject),
            baseline,
            "visible set moved when diffed against {counterpart:?}"
        );
    }
}

/// Pure containers are excluded and text-carrying nodes are kept - the distinction the metric
/// is built on. `block` is the canonical container (everything readable in it belongs to a
/// child); a Rust `line_comment` is the canonical interior-but-visible node, since its `//`
/// marker is a separate child leaving the words on the parent.
#[test]
fn structural_visibility_excludes_containers_and_keeps_text_carriers() {
    let code = Code::from_string(
        "fn main() {\n    // a comment\n    foo();\n}\n",
        &Language::Rust,
    );
    let source = code.contents.as_bytes();
    let root = code.ast.as_ref().unwrap().root_node();

    let mut seen: Vec<(String, bool)> = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        seen.push((n.kind().to_string(), is_structurally_visible(n, source)));
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }

    let visible_of = |kind: &str| seen.iter().find(|(k, _)| k == kind).map(|(_, v)| *v);
    assert_eq!(
        visible_of("block"),
        Some(false),
        "a block is pure structure"
    );
    assert_eq!(
        visible_of("line_comment"),
        Some(true),
        "a comment carries its own words"
    );
    assert_eq!(
        visible_of("identifier"),
        Some(true),
        "a leaf is always visible"
    );
    assert!(
        seen.iter().any(|(k, v)| k == "source_file" && !v),
        "the root is pure structure too"
    );
}

/// The bitmask form `UnitCostModel::ren` uses on the hot path must agree with the string-
/// scanning `kinds_update_allowed` it replaced, for *every* pair of kinds either of them knows
/// about, in every language. Exhaustive rather than sampled: the whole point of the mask is
/// that it is a pure derivation of the same `const` arrays, so a disagreement anywhere is a
/// bug in the derivation, and the cross product is small enough to just check outright.
#[test]
fn operator_family_masks_agree_with_string_scanning_kinds_update_allowed() {
    let mut kinds: Vec<&str> = ALL_OPERATOR_FAMILIES.concat();
    kinds.extend_from_slice(IDENTIFIER_KINDS);
    // A kind in no family at all, to pin the negative case too.
    kinds.push("if_statement");
    kinds.sort_unstable();
    kinds.dedup();

    let languages = [
        Language::C,
        Language::CPP,
        Language::Java,
        Language::Go,
        Language::CSharp,
        Language::Rust,
        Language::JavaScript,
        Language::TypeScript,
        Language::TSX,
        Language::Python,
        Language::Kotlin,
        Language::PHP,
        Language::Ruby,
        // No families at all - the `_ => &[]` arm.
        Language::Unknown,
    ];

    for language in &languages {
        let language_mask = language_operator_family_mask(language);
        for &a in &kinds {
            for &b in &kinds {
                if a == b {
                    continue; // Handled by `ren`'s own same-kind branch, before the masks.
                }
                let class_of = |kind: &str| crate::code::KindCostClass {
                    identifier_like: is_identifier_kind(kind),
                    literal_like: is_literal_kind(kind),
                    operator_families: operator_family_mask(kind),
                };
                assert_eq!(
                    update_allowed_from_masks(&class_of(a), &class_of(b), language_mask),
                    kinds_update_allowed(a, b, language),
                    "mask and string forms disagree on ({a:?}, {b:?}) in {language:?}"
                );
            }
        }
    }
}

#[test]
fn kinds_update_allowed_same_kind_is_always_allowed() {
    assert!(kinds_update_allowed("<", "<", &Language::CPP));
    assert!(kinds_update_allowed(
        "identifier",
        "identifier",
        &Language::Unknown
    ));
}

#[test]
fn kinds_update_allowed_cross_kind_identifiers() {
    // Test that identifier-like kinds can match each other
    assert!(kinds_update_allowed(
        "identifier",
        "field_identifier",
        &Language::C
    ));
    assert!(kinds_update_allowed(
        "identifier",
        "type_identifier",
        &Language::Rust
    ));
    assert!(kinds_update_allowed(
        "field_identifier",
        "identifier",
        &Language::C
    ));
    assert!(kinds_update_allowed(
        "type_identifier",
        "field_identifier",
        &Language::Rust
    ));
    assert!(kinds_update_allowed(
        "property_identifier",
        "identifier",
        &Language::JavaScript
    ));

    // Test that it works across all languages
    assert!(kinds_update_allowed(
        "identifier",
        "field_identifier",
        &Language::Unknown
    ));
    assert!(kinds_update_allowed(
        "identifier",
        "field_identifier",
        &Language::Java
    ));
    assert!(kinds_update_allowed(
        "identifier",
        "field_identifier",
        &Language::Python
    ));

    // scoped_type_identifier is NOT included in IDENTIFIER_KINDS because it represents
    // qualified names where the qualification is part of the identity
    assert!(!kinds_update_allowed(
        "scoped_type_identifier",
        "type_identifier",
        &Language::Rust
    ));
}

#[test]
fn kinds_update_allowed_identifiers_do_not_match_non_identifiers() {
    // Test that identifier kinds don't match non-identifier kinds
    assert!(!kinds_update_allowed("identifier", "+", &Language::C));
    assert!(!kinds_update_allowed("field_identifier", "(", &Language::C));
    assert!(!kinds_update_allowed(
        "type_identifier",
        "string_literal",
        &Language::Rust
    ));
}

#[test]
fn flow_control_similarity_of_sets_ignores_wildcards_and_scores_jaccard() {
    // Regression guard for `flow_control_similarity_of_sets` after the arm-extraction helpers
    // that used to be its only caller (`solve_similar_flow_control`) were deleted 2026-08-14 -
    // `solve_import_list_overlap` is the sole remaining caller now, building its sets directly
    // from import symbols rather than flow-control arm signatures, but the Jaccard scoring
    // itself is generic and still worth its own direct test.
    let before: std::collections::HashSet<&str> =
        ["asset", "ecmascript", "wasm"].into_iter().collect();
    let after: std::collections::HashSet<&str> =
        ["asset", "ecmascript", "json"].into_iter().collect();

    // Shared: asset, ecmascript (2). Union: asset, ecmascript, wasm, json (4).
    let score = flow_control_similarity_of_sets(&before, &after);
    assert!(
        (score - 0.5).abs() < 1e-9,
        "expected 2/4 = 0.5, got {score}"
    );
}

#[test]
fn flow_control_similarity_of_sets_is_zero_when_either_side_is_empty() {
    let empty: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let non_empty: std::collections::HashSet<&str> = ["a"].into_iter().collect();
    assert_eq!(flow_control_similarity_of_sets(&empty, &non_empty), 0.0);
    assert_eq!(flow_control_similarity_of_sets(&non_empty, &empty), 0.0);
}

#[test]
fn cpp_relational_operators_cross_match() {
    // The motivating case: `for (...; i < size; ...)` -> `for (...; i <= size; ...)`.
    assert!(kinds_update_allowed("<", "<=", &Language::CPP));
    assert!(kinds_update_allowed("==", "!=", &Language::CPP));
    assert!(kinds_update_allowed(">=", "<=>", &Language::CPP));
}

#[test]
fn cpp_operators_never_cross_families() {
    assert!(!kinds_update_allowed("<", "+", &Language::CPP));
    assert!(!kinds_update_allowed("&&", "&", &Language::CPP));
    assert!(!kinds_update_allowed("=", "==", &Language::CPP));
}

#[test]
fn cpp_increment_decrement_cross_match() {
    assert!(kinds_update_allowed("++", "--", &Language::CPP));
}

#[test]
fn rust_range_operators_cross_match() {
    // The classic Rust off-by-one fix: `0..n` -> `0..=n`.
    assert!(kinds_update_allowed("..", "..=", &Language::Rust));
    assert!(!kinds_update_allowed("..", "+", &Language::Rust));
}

#[test]
fn rust_compound_assignment_crosses_with_plain_assignment() {
    assert!(kinds_update_allowed("=", "+=", &Language::Rust));
    assert!(kinds_update_allowed("+=", "-=", &Language::Rust));
}

#[test]
fn unknown_language_never_allows_cross_kind_matches() {
    assert!(!kinds_update_allowed("<", "<=", &Language::Unknown));
}

#[test]
fn python_excludes_keyword_comparisons_from_family() {
    // `in`/`is`/`instanceof` double as other syntax elsewhere in the grammar, so they're
    // deliberately excluded from the shared comparison family (see COMPARISON_OPS doc).
    assert!(!kinds_update_allowed("in", "==", &Language::Python));
    assert!(kinds_update_allowed("<", "<=", &Language::Python));
}

#[test]
fn generic_token_kind_covers_operators_and_punctuation() {
    assert!(is_generic_token_kind("<"));
    assert!(is_generic_token_kind("<="));
    assert!(is_generic_token_kind("("));
    assert!(is_generic_token_kind(")"));
    assert!(is_generic_token_kind("{"));
    assert!(is_generic_token_kind("::"));
    assert!(is_generic_token_kind("and")); // keyword-spelled operator, not purely symbolic
}

#[test]
fn generic_token_kind_excludes_content_bearing_leaves() {
    assert!(!is_generic_token_kind("identifier"));
    assert!(!is_generic_token_kind("string_literal"));
    assert!(!is_generic_token_kind("return_statement"));
}

#[test]
fn matching_allowed_rejects_whatever_kinds_update_allowed_rejects() {
    // `+` and `<` are never kind-compatible, regardless of context.
    assert!(!matching_allowed("<", "+", &Language::CPP, || true));
}

#[test]
fn matching_allowed_requires_context_for_generic_tokens_only() {
    // Same identifier kind, not a generic token: context is never even consulted.
    assert!(matching_allowed(
        "identifier",
        "identifier",
        &Language::CPP,
        || { panic!("must not evaluate parents_matched for a non-generic-token kind") }
    ));

    // Generic token, kind-compatible (same kind): allowed only if context says so.
    assert!(matching_allowed("<", "<", &Language::CPP, || true));
    assert!(!matching_allowed("<", "<", &Language::CPP, || false));

    // Generic token, cross-kind family swap: same rule applies.
    assert!(matching_allowed("<", "<=", &Language::CPP, || true));
    assert!(!matching_allowed("<", "<=", &Language::CPP, || false));
}

#[test]
fn leaf_texts_similar_accepts_clear_renames() {
    assert!(leaf_texts_similar("fetch_user", "fetch_user_data"));
    assert!(leaf_texts_similar("user_data", "userData"));
    assert!(leaf_texts_similar("same", "same"));
}

#[test]
fn leaf_texts_similar_rejects_unrelated_and_tiny_texts() {
    assert!(!leaf_texts_similar("i", "numbers"));
    assert!(!leaf_texts_similar("min", "result"));
    // Too short for bigram evidence - the context arm of the caller's OR handles these.
    assert!(!leaf_texts_similar("i", "j"));
    assert!(!leaf_texts_similar("0", "1"));
}

// Tests for is_reference (formerly node_matches from reference_nodes.rs)

#[test]
fn root_nodes_are_reference_in_all_languages() -> Result<()> {
    let codes = helper::handmade_test_code()?;

    // Test on all handmade code files
    for (filename, code) in &codes {
        // Get the language from metadata
        let language_msg = format!("Language should be set for file: {}", filename);
        let language = code.metadata.language.as_ref().expect(&language_msg);

        // Get the AST
        let ast_msg = format!("AST should be parsed for file: {}", filename);
        let ast = code.ast.as_ref().expect(&ast_msg);

        // Get the root node
        let root_node = ast.root_node();
        let root_node_kind = root_node.kind();

        // Check if the root node is a reference node
        assert!(
            is_reference(root_node_kind, language),
            "Root node '{}' should be a reference node for language {:?} in file {}",
            root_node_kind,
            language,
            filename
        );
    }

    Ok(())
}

// Tests for is_semantically_structural (formerly node_matches from semantic_structure_nodes.rs)

fn collect_matches(src: &str) -> Vec<(String, String)> {
    let code = Code::from_string(src, &Language::Rust);
    let ast = code.ast.as_ref().expect("AST should parse");
    let root = ast.root_node();
    let mut cursor = root.walk();
    root.children(&mut cursor)
        .filter_map(|child| is_semantically_structural(&child, &Language::Rust, &code))
        .collect()
}

/// Recursively collects every semantically-structural match in `node`'s subtree (unlike
/// `collect_matches`, which only looks at direct children of the root).
fn collect_semantic_matches(
    node: tree_sitter::Node,
    lang: &Language,
    code: &Code,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(m) = is_semantically_structural(&node, lang, code) {
        out.push(m);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        out.extend(collect_semantic_matches(child, lang, code));
    }
    out
}

#[test]
fn rust_public_items_are_matched() {
    let matches = collect_matches(
        "pub fn pub_fn() {} \
         fn private_fn() {} \
         pub struct PubStruct; \
         struct PrivateStruct; \
         pub enum PubEnum { A } \
         enum PrivateEnum { B } \
         impl PubStruct {} \
         impl Display for PubStruct {}",
    );
    let expected = [
        ("function_item", "pub_fn"),
        ("function_item", "private_fn"),
        ("struct_item", "PubStruct"),
        ("struct_item", "PrivateStruct"),
        ("enum_item", "PubEnum"),
        ("enum_item", "PrivateEnum"),
        ("impl_item", "PubStruct"),
        ("impl_item", "Display for PubStruct"),
    ];
    for (kind, name) in &expected {
        assert!(
            matches.iter().any(|(k, n)| k == kind && n == name),
            "missing ({kind}, {name}) in {matches:?}"
        );
    }
}

#[test]
fn root_nodes_are_not_semantically_structural_in_all_languages() -> Result<()> {
    let codes = helper::handmade_test_code()?;

    // Test on all handmade code files
    for (filename, code) in &codes {
        // Get the language from metadata
        let language_msg = format!("Language should be set for file: {}", filename);
        let language = code.metadata.language.as_ref().expect(&language_msg);

        // Get the AST
        let ast_msg = format!("AST should be parsed for file: {}", filename);
        let ast = code.ast.as_ref().expect(&ast_msg);

        // Get the root node
        let root_node = ast.root_node();

        // The root node is in principle NOT a semantically structural node.
        // This is because it doesn't actually change the semantic of the code in any way.
        assert!(
            is_semantically_structural(&root_node, language, code).is_none(),
            "Root node should not be a semantically structural node in language {:?} in file {}",
            language,
            filename
        );
    }

    Ok(())
}

#[test]
fn go_functions_methods_and_types_are_matched() {
    let src = r#"
package main

func TopLevel() {}

func (s *Server) HandleRequest() {}
func (s Server) Name() string { return "" }

type Server struct { port int }
type Handler interface { Handle() }
"#;
    let code = Code::from_string(src, &Language::Go);
    let ast = code.ast.as_ref().expect("AST should parse");

    let matches = collect_semantic_matches(ast.root_node(), &Language::Go, &code);

    for (kind, name) in &[
        ("function_declaration", "TopLevel"),
        ("method_declaration", "Server.HandleRequest"),
        ("method_declaration", "Server.Name"),
        ("type_spec", "Server"),
        ("type_spec", "Handler"),
    ] {
        assert!(
            matches.iter().any(|(k, n)| k == kind && n == name),
            "missing ({kind}, {name}) in {matches:?}"
        );
    }
    assert!(is_semantically_structural(&ast.root_node(), &Language::Go, &code).is_none());
}

/// Regression guard for the 2026-07-23 fix (`go_subtest_call_name`): `t.Run("literal", ...)`-
/// shaped calls (Go's standard subtest idiom, also used by quicktest/testify) get their own
/// identity keyed on the literal name, regardless of which variable the call is made through.
#[test]
fn go_subtest_run_calls_are_matched_by_their_literal_name() {
    let src = r#"
package main

func TestThings(t *testing.T) {
	t.Run("first case", func(t *testing.T) {})
	c.Run("second case", func(c *qt.C) {})
	suite.Run("third case", func(t *testing.T) {})
}
"#;
    let code = Code::from_string(src, &Language::Go);
    let ast = code.ast.as_ref().expect("AST should parse");

    let matches = collect_semantic_matches(ast.root_node(), &Language::Go, &code);

    for (kind, name) in &[
        ("call_expression", "first case"),
        ("call_expression", "second case"),
        ("call_expression", "third case"),
    ] {
        assert!(
            matches.iter().any(|(k, n)| k == kind && n == name),
            "missing ({kind}, {name}) in {matches:?}"
        );
    }
}

/// Only a `.Run("string literal", ...)` call qualifies - a `.Run` call with a non-literal
/// (variable) first argument, the table-driven-test idiom (`t.Run(tc.name, ...)`), and an
/// unrelated call to some other method must not be misidentified as a named subtest.
#[test]
fn go_calls_that_are_not_literal_named_subtests_are_not_matched() {
    let src = r#"
package main

func TestThings(t *testing.T) {
	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {})
	}
	fmt.Println("not a subtest")
}
"#;
    let code = Code::from_string(src, &Language::Go);
    let ast = code.ast.as_ref().expect("AST should parse");

    let matches = collect_semantic_matches(ast.root_node(), &Language::Go, &code);

    assert!(
        !matches.iter().any(|(k, _)| k == "call_expression"),
        "no call_expression should have matched: {matches:?}"
    );
}

/// Regression guard for the 2026-07-23 fix: a top-level `var`/`const` declaration gets an
/// identity keyed on its own name (not just its `var_spec`/`const_spec` child), so
/// `solve_large_flat_subtrees`'s direct-children-only `top_level_identities` can see it - a
/// large data literal assigned to a top-level `var` (Go's common table-driven-test-data
/// idiom, e.g. `var tests = []T{...}`) previously had no identity signal at all.
#[test]
fn go_top_level_var_and_const_declarations_are_matched() {
    let src = r#"
package main

var tests = []int{1, 2, 3}
const MaxRetries = 5
var a, b = 1, 2
"#;
    let code = Code::from_string(src, &Language::Go);
    let ast = code.ast.as_ref().expect("AST should parse");

    let matches = collect_semantic_matches(ast.root_node(), &Language::Go, &code);

    for (kind, name) in &[
        ("var_declaration", "tests"),
        ("const_declaration", "MaxRetries"),
        ("var_declaration", "a"),
    ] {
        assert!(
            matches.iter().any(|(k, n)| k == kind && n == name),
            "missing ({kind}, {name}) in {matches:?}"
        );
    }
}

#[test]
fn python_functions_and_classes_are_matched() {
    let src = "
def top_fn():
pass

async def async_fn():
pass

class MyClass:
def __init__(self):
    pass

def method(self):
    pass

@decorator
def decorated_fn():
pass

@decorator
class DecoratedClass:
pass
";
    let code = Code::from_string(src, &Language::Python);
    let ast = code.ast.as_ref().expect("AST should parse");

    let matches = collect_semantic_matches(ast.root_node(), &Language::Python, &code);

    for (kind, name) in &[
        ("function_definition", "top_fn"),
        ("function_definition", "async_fn"),
        ("class_definition", "MyClass"),
        ("function_definition", "__init__"),
        ("function_definition", "method"),
        ("function_definition", "decorated_fn"),
        ("class_definition", "DecoratedClass"),
    ] {
        assert!(
            matches.iter().any(|(k, n)| k == kind && n == name),
            "missing ({kind}, {name}) in {matches:?}"
        );
    }
    // Root (module) should not match
    assert!(is_semantically_structural(&ast.root_node(), &Language::Python, &code).is_none());
}

/// Node id of the first `(kind, name)` semantic match found in `root`'s subtree, or `None`.
/// Test-only helper for looking up a specific named declaration without depending on the
/// precomputed `ASTMetadata::semantically_structural_nodes` map (which the pipeline itself no
/// longer populates or reads - see `TODO.md`'s final-cleanup notes).
fn find_semantic_node(
    root: tree_sitter::Node,
    lang: &Language,
    code: &Code,
    target: &(String, String),
) -> Option<usize> {
    if is_semantically_structural(&root, lang, code).as_ref() == Some(target) {
        return Some(root.id());
    }
    let mut cursor = root.walk();
    root.children(&mut cursor)
        .find_map(|child| find_semantic_node(child, lang, code, target))
}

#[test]
fn python_methods_in_class_are_pre_matched() {
    use crate::diff::solve_syntax_aware_matching::solve;
    use crate::diff::{ASTDiff, NodeCache};

    let before_src = "
class Calculator:
def add(self, a, b):
    return a + b
def subtract(self, a, b):
    return a - b
";
    let after_src = "
class Calculator:
def add(self, a, b):
    return a + b
def subtract(self, a, b):
    return a - b - 1  # changed
";
    let before = Code::from_string(before_src, &Language::Python);
    let after = Code::from_string(after_src, &Language::Python);
    let node_cache = NodeCache::build(&before, &after);
    let mut diff = ASTDiff::default();
    solve(&before, &after, &node_cache, &mut diff);

    let before_root = before.ast.as_ref().unwrap().root_node();
    let after_root = after.ast.as_ref().unwrap().root_node();

    // Both methods should be matched.
    let matched_names: Vec<&str> = ["add", "subtract"]
        .iter()
        .copied()
        .filter(|&name| {
            let key = ("function_definition".to_string(), name.to_string());
            let bid = find_semantic_node(before_root, &Language::Python, &before, &key);
            let aid = find_semantic_node(after_root, &Language::Python, &after, &key);
            bid.zip(aid)
                .is_some_and(|(bid, aid)| diff.mapping.contains_key(&(bid, aid)))
        })
        .collect();
    assert_eq!(
        matched_names.len(),
        2,
        "both methods should be matched; got {matched_names:?}"
    );
}

#[test]
fn rust_traits_and_modules_are_matched() {
    let matches = collect_matches(
        "pub trait Display { fn fmt(&self); } \
         trait PartialEq {} \
         mod utils {} \
         pub mod helpers {}",
    );
    for (kind, name) in &[
        ("trait_item", "Display"),
        ("trait_item", "PartialEq"),
        ("mod_item", "utils"),
        ("mod_item", "helpers"),
    ] {
        assert!(
            matches.iter().any(|(k, n)| k == kind && n == name),
            "missing ({kind}, {name}) in {matches:?}"
        );
    }
}

#[test]
fn go_type_alias_is_matched() {
    let src = r#"
package main
type MyInt = int
type Stringer interface { String() string }
"#;
    let code = Code::from_string(src, &Language::Go);
    let ast = code.ast.as_ref().expect("AST should parse");

    let matches = collect_semantic_matches(ast.root_node(), &Language::Go, &code);

    for (kind, name) in &[("type_alias", "MyInt"), ("type_spec", "Stringer")] {
        assert!(
            matches.iter().any(|(k, n)| k == kind && n == name),
            "missing ({kind}, {name}) in {matches:?}"
        );
    }
}

fn collect_all_kotlin(src: &str) -> Vec<(String, String)> {
    let code = Code::from_string(src, &Language::Kotlin);
    let ast = code.ast.as_ref().expect("AST should parse");
    collect_semantic_matches(ast.root_node(), &Language::Kotlin, &code)
}

#[test]
fn kotlin_top_level_functions_are_matched() {
    let matches = collect_all_kotlin(
        "fun greet(name: String): String { return \"Hello\" }\n\
         fun add(a: Int, b: Int): Int = a + b\n\
         fun add(a: String): String = a\n",
    );
    for (kind, name) in &[
        ("function_declaration", "greet(String)"),
        ("function_declaration", "add(Int,Int)"),
        ("function_declaration", "add(String)"),
    ] {
        assert!(
            matches.iter().any(|(k, n)| k == kind && n == name),
            "missing ({kind}, {name}) in {matches:?}"
        );
    }
}

#[test]
fn kotlin_extension_functions_use_receiver_prefix() {
    let matches = collect_all_kotlin(
        "fun String.shout(): String = this.uppercase()\n\
         fun List<String>.joinWithComma(): String = joinToString(\", \")\n",
    );
    for (kind, name) in &[
        ("function_declaration", "String.shout()"),
        ("function_declaration", "List.joinWithComma()"),
    ] {
        assert!(
            matches.iter().any(|(k, n)| k == kind && n == name),
            "missing ({kind}, {name}) in {matches:?}"
        );
    }
}

#[test]
fn kotlin_classes_objects_and_aliases_are_matched() {
    let src = "
class Calculator {
fun add(a: Int, b: Int): Int = a + b
}
object Singleton {}
typealias StringList = List<String>
";
    let matches = collect_all_kotlin(src);
    for (kind, name) in &[
        ("class_declaration", "Calculator"),
        ("function_declaration", "add(Int,Int)"),
        ("object_declaration", "Singleton"),
        ("type_alias", "StringList"),
    ] {
        assert!(
            matches.iter().any(|(k, n)| k == kind && n == name),
            "missing ({kind}, {name}) in {matches:?}"
        );
    }
}

#[test]
fn kotlin_unnamed_companion_object_is_not_matched() {
    let matches = collect_all_kotlin("class MyClass { companion object {} }\n");
    assert!(
        !matches.iter().any(|(k, _)| k == "companion_object"),
        "unnamed companion_object should not be matched; got {matches:?}"
    );
}

#[test]
fn kotlin_named_companion_object_is_matched() {
    let matches = collect_all_kotlin("class MyClass { companion object Factory {} }\n");
    assert!(
        matches
            .iter()
            .any(|(k, n)| k == "companion_object" && n == "Factory"),
        "missing (companion_object, Factory) in {matches:?}"
    );
}

#[test]
fn kotlin_methods_in_class_are_pre_matched() {
    use crate::diff::solve_syntax_aware_matching::solve;
    use crate::diff::{ASTDiff, NodeCache};

    let before_src = "
class Calculator {
fun add(a: Int, b: Int): Int = a + b
fun subtract(a: Int, b: Int): Int = a - b
}
";
    let after_src = "
class Calculator {
fun add(a: Int, b: Int): Int = a + b
fun subtract(a: Int, b: Int): Int = a - b - 1
}
";
    let before = Code::from_string(before_src, &Language::Kotlin);
    let after = Code::from_string(after_src, &Language::Kotlin);
    let node_cache = NodeCache::build(&before, &after);
    let mut diff = ASTDiff::default();
    solve(&before, &after, &node_cache, &mut diff);

    let before_root = before.ast.as_ref().unwrap().root_node();
    let after_root = after.ast.as_ref().unwrap().root_node();

    let matched_names: Vec<&str> = ["add(Int,Int)", "subtract(Int,Int)"]
        .iter()
        .copied()
        .filter(|&name| {
            let key = ("function_declaration".to_string(), name.to_string());
            let bid = find_semantic_node(before_root, &Language::Kotlin, &before, &key);
            let aid = find_semantic_node(after_root, &Language::Kotlin, &after, &key);
            bid.zip(aid)
                .is_some_and(|(bid, aid)| diff.mapping.contains_key(&(bid, aid)))
        })
        .collect();
    assert_eq!(
        matched_names.len(),
        2,
        "both methods should be pre-matched; got {matched_names:?}"
    );
}

#[test]
fn rust_bail_macro_and_ordinary_call_are_classified_correctly() {
    let code = Code::from_string(
        "fn f(s: &str) -> Result<()> { if s.is_empty() { bail!(\"empty\"); } compute(1); Ok(()) }",
        &Language::Rust,
    );
    let root = code.ast.as_ref().unwrap().root_node();
    let source = code.contents.as_bytes();
    let bail = helper::find_first_of_kind(root, "macro_invocation").unwrap();
    let call = helper::find_first_of_kind(root, "call_expression").unwrap();
    assert!(
        is_diagnostic_statement(bail, &Language::Rust, source),
        "bail! should be diagnostic"
    );
    assert!(
        !is_diagnostic_statement(call, &Language::Rust, source),
        "an ordinary call like compute(1) should not be diagnostic"
    );
}

#[test]
fn c_fprintf_to_stderr_is_diagnostic() {
    let code = Code::from_string(
        "void f(void) { fprintf(stderr, \"bad thing\\n\"); ok(1); }",
        &Language::C,
    );
    let root = code.ast.as_ref().unwrap().root_node();
    let source = code.contents.as_bytes();
    let mut calls = Vec::new();
    collect_all(root, "call_expression", &mut calls);
    let fprintf = calls
        .iter()
        .find(|n| callee_text(**n, &Language::C, source) == Some("fprintf"))
        .expect("fprintf call should be present");
    let ok_call = calls
        .iter()
        .find(|n| callee_text(**n, &Language::C, source) == Some("ok"))
        .expect("ok call should be present");
    assert!(is_diagnostic_statement(*fprintf, &Language::C, source));
    assert!(!is_diagnostic_statement(*ok_call, &Language::C, source));
}

#[test]
fn python_logging_error_is_diagnostic_via_attribute_access() {
    let code = Code::from_string(
        "logging.error('bad thing')\ncompute(1)\n",
        &Language::Python,
    );
    let root = code.ast.as_ref().unwrap().root_node();
    let source = code.contents.as_bytes();
    let mut calls = Vec::new();
    collect_all(root, "call", &mut calls);
    let log_call = calls
        .iter()
        .find(|n| callee_text(**n, &Language::Python, source) == Some("logging.error"))
        .expect("logging.error call should be present");
    let compute_call = calls
        .iter()
        .find(|n| callee_text(**n, &Language::Python, source) == Some("compute"))
        .expect("compute call should be present");
    assert!(is_diagnostic_statement(
        *log_call,
        &Language::Python,
        source
    ));
    assert!(!is_diagnostic_statement(
        *compute_call,
        &Language::Python,
        source
    ));
}

#[test]
fn go_log_fatal_is_diagnostic_via_selector_expression() {
    let code = Code::from_string(
        "package main\nfunc f() {\n\tlog.Fatal(\"boom\")\n\tcompute(1)\n}\n",
        &Language::Go,
    );
    let root = code.ast.as_ref().unwrap().root_node();
    let source = code.contents.as_bytes();
    let mut calls = Vec::new();
    collect_all(root, "call_expression", &mut calls);
    let fatal_call = calls
        .iter()
        .find(|n| callee_text(**n, &Language::Go, source) == Some("log.Fatal"))
        .expect("log.Fatal call should be present");
    let compute_call = calls
        .iter()
        .find(|n| callee_text(**n, &Language::Go, source) == Some("compute"))
        .expect("compute call should be present");
    assert!(is_diagnostic_statement(*fatal_call, &Language::Go, source));
    assert!(!is_diagnostic_statement(
        *compute_call,
        &Language::Go,
        source
    ));
}

fn collect_all<'a>(node: tree_sitter::Node<'a>, kind: &str, out: &mut Vec<tree_sitter::Node<'a>>) {
    if node.kind() == kind {
        out.push(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_all(child, kind, out);
    }
}

/// Parses `source` in `language`, finds the first node of kind `container_kind`, and asserts
/// `is_commutative_container` recognizes it - proving the kind string actually occurs in a
/// real parse tree instead of just matching a made-up name that no grammar ever produces
/// (exactly the bug this whole function had for most languages before 2026-07-29).
fn assert_recognizes(language: Language, source: &str, container_kind: &str) {
    let code = Code::from_string(source, &language);
    let root = code.ast.as_ref().unwrap().root_node();
    let mut cursor = root.walk();
    let found = find_kind(&mut cursor, container_kind);
    assert!(
        found,
        "expected a `{container_kind}` node in {language:?} source {source:?}, but none was found - the container kind string is wrong"
    );
    assert!(is_commutative_container(container_kind, &language));
}

fn find_kind(cursor: &mut tree_sitter::TreeCursor, kind: &str) -> bool {
    loop {
        if cursor.node().kind() == kind {
            return true;
        }
        if cursor.goto_first_child() {
            if find_kind(cursor, kind) {
                cursor.goto_parent();
                return true;
            }
            cursor.goto_parent();
        }
        if !cursor.goto_next_sibling() {
            return false;
        }
    }
}

#[test]
fn rust_recognizes_struct_fields_enum_variants_and_use_lists() {
    assert_recognizes(
        Language::Rust,
        "struct S { a: i32, b: i32 }\n",
        "field_declaration_list",
    );
    assert_recognizes(Language::Rust, "enum E { A, B }\n", "enum_variant_list");
    assert_recognizes(Language::Rust, "use std::{a, b};\n", "use_list");
}

#[test]
fn go_recognizes_struct_fields_and_import_specs() {
    assert_recognizes(
        Language::Go,
        "package main\ntype T struct {\n A int\n}\n",
        "field_declaration_list",
    );
    assert_recognizes(
        Language::Go,
        "package main\nimport (\n \"fmt\"\n)\n",
        "import_spec_list",
    );
}

#[test]
fn python_recognizes_dictionary() {
    assert_recognizes(Language::Python, "d = {1: 2}\n", "dictionary");
}

#[test]
fn java_recognizes_enum_body() {
    assert_recognizes(Language::Java, "enum E { A, B }\n", "enum_body");
}

#[test]
fn csharp_recognizes_enum_member_declaration_list() {
    assert_recognizes(
        Language::CSharp,
        "enum E { A, B }\n",
        "enum_member_declaration_list",
    );
}

#[test]
fn c_and_cpp_recognize_enumerator_list() {
    assert_recognizes(Language::C, "enum E { A, B };\n", "enumerator_list");
    assert_recognizes(Language::CPP, "enum E { A, B };\n", "enumerator_list");
}

#[test]
fn js_ts_tsx_recognize_object() {
    assert_recognizes(Language::JavaScript, "const o = {a: 1};\n", "object");
    assert_recognizes(Language::TypeScript, "const o = {a: 1};\n", "object");
    assert_recognizes(Language::TSX, "const o = {a: 1};\n", "object");
}

#[test]
fn scala_recognizes_braced_import_selectors() {
    assert_recognizes(
        Language::Scala,
        "import a.b.{X, Y}\n",
        "namespace_selectors",
    );
}

#[test]
fn swift_recognizes_enum_class_body() {
    assert_recognizes(Language::Swift, "enum E { case a, b }\n", "enum_class_body");
}

/// Kotlin has no commutative container at all - imports are unwrapped repeated children of
/// `source_file`, so this must stay `false` for every kind, not just a made-up string.
#[test]
fn kotlin_has_no_commutative_container() {
    assert!(!is_commutative_container("import_list", &Language::Kotlin));
    assert!(!is_commutative_container("import", &Language::Kotlin));
    assert!(!is_commutative_container("source_file", &Language::Kotlin));
}
