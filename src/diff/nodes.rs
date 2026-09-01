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
use tree_sitter::Node;

use crate::code::{ASTMetadata, Code, Language};
use crate::diff::apted::{self, Algorithm};
use crate::diff::{ASTDiff, ASTMapping, ASTMappingOperation, ASTMappingReason};

/// Recursively maps every descendant of an already-matched pair as `Identical`/cost 0/
/// `IdenticalHashOfAncestor`, position-by-position in lockstep (stack-based DFS, so children are
/// visited in reverse sibling order - doesn't matter here since every child gets mapped
/// regardless of order). Stops descending into a child whose kind doesn't match its counterpart's
/// (can only happen if the caller's "these subtrees are identical" guarantee doesn't actually
/// hold) or that's already mapped on the before side.
///
/// Shared by `solve_comment_nodes` (whose precondition - matched comment nodes' full text is
/// byte-identical - guarantees kinds always match, making the kind check here a no-op for that
/// caller) and `solve_identical_diagnostic_statements` (whose precondition - matched statements'
/// full hash is identical - gives the same guarantee).
pub fn map_identical_descendants<'a>(
    before_node: Node<'a>,
    after_node: Node<'a>,
    diff: &mut ASTDiff,
) {
    let mut stack = vec![(before_node, after_node)];
    while let Some((before_parent, after_parent)) = stack.pop() {
        let mut before_cursor = before_parent.walk();
        let mut after_cursor = after_parent.walk();
        let before_children = before_parent.children(&mut before_cursor);
        let after_children = after_parent.children(&mut after_cursor);

        for (before_child, after_child) in before_children.zip(after_children) {
            if before_child.kind() != after_child.kind() {
                continue;
            }
            if diff.before_node_map.contains_key(&before_child.id()) {
                continue;
            }
            diff.add_mapping(
                before_child.id(),
                after_child.id(),
                ASTMapping {
                    cost: 0,
                    operation: ASTMappingOperation::Identical,
                    reason: ASTMappingReason::IdenticalHashOfAncestor,
                },
            );
            stack.push((before_child, after_child));
        }
    }
}

/// Walks `root`'s subtree (stack-based DFS, so children are visited in reverse sibling order)
/// collecting every node for which `predicate` returns true and that isn't already mapped in
/// `mapped`. Doesn't descend into already-mapped nodes - their contents are presumed already
/// resolved by an earlier pass - but does keep descending past a collected node itself, in case a
/// second match is nested inside the first (e.g. one diagnostic call nested in another's
/// arguments). Originally factored out of two near-duplicate walks, one of which
/// (`solve_similar_flow_control`'s `collect_unmatched_containers`) was deleted 2026-08-14;
/// `solve_identical_diagnostic_statements`'s `collect_unmatched_diagnostic_statements` remains the
/// current caller.
pub fn collect_unmatched<'a>(
    root: Node<'a>,
    mapped: &rustc_hash::FxHashMap<usize, usize>,
    predicate: impl Fn(Node<'a>) -> bool,
) -> Vec<Node<'a>> {
    let mut result = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if mapped.contains_key(&node.id()) {
            continue;
        }
        if predicate(node) {
            result.push(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    result
}

/// Proposes `(before_id, after_id)` to APTED via `apted::for_nodes` and, if it actually resolved
/// the pair as a match (rather than a separate delete+insert - e.g. if the leftover residual
/// outweighs reuse), relabels the resulting mapping's reason to `reason` instead of leaving it as
/// whatever generic label `for_nodes` itself assigns. Shared "propose a pair, then stamp
/// provenance if it stuck" idiom behind `solve_bottom_up_expansion` and
/// `solve_greedy_anchor_blocks`, which both anchor single node pairs this same way.
pub fn anchor_pair_via_apted(
    before_id: usize,
    after_id: usize,
    before_metadata: &ASTMetadata,
    after_metadata: &ASTMetadata,
    source: &'static str,
    reason: ASTMappingReason,
    diff: &mut ASTDiff,
) {
    apted::for_nodes(
        before_metadata,
        after_metadata,
        vec![before_id],
        vec![after_id],
        Algorithm::Apted,
        source,
        diff,
    );

    if let Some(mapping) = diff.mapping.get_mut(&(before_id, after_id)) {
        mapping.reason = reason;
    }
}

/**
* Determines if a node with the given kind is a reference node for the given language.
*
* Reference nodes are nodes that are used as anchors for diffing. Typically these are
* nodes that represent significant structural elements in the code, such as source files
* or function definitions.
*
* You can think of reference nodes as "parts of code humans think about". I.e. humans rarely think
* about a specific semicolon in a C++ file, but they do think about entire functions as whole
* entities.
*/
pub fn is_reference(node_kind: &str, language: &Language) -> bool {
    // Language-specific reference nodes
    match language {
        Language::Rust => {
            node_kind == "source_file"
                || node_kind == "function_item"
                || node_kind == "impl_item"
                || node_kind == "struct_item"
                || node_kind == "enum_item"
                || node_kind == "trait_item"
                || node_kind == "type_item"
                || node_kind == "mod_item"
                || node_kind == "use_declaration"
                || node_kind == "if_expression"
        }
        Language::Python => {
            node_kind == "module"
                || node_kind == "function_definition"
                || node_kind == "class_definition"
                || node_kind == "import_statement"
                || node_kind == "import_from_statement"
                || node_kind == "future_import_statement"
        }
        Language::Java => {
            node_kind == "program"
                || node_kind == "class_declaration"
                || node_kind == "interface_declaration"
                || node_kind == "enum_declaration"
                || node_kind == "method_declaration"
                || node_kind == "field_declaration"
                || node_kind == "import_declaration"
        }
        Language::C => {
            node_kind == "translation_unit"
                || node_kind == "function_definition"
                || node_kind == "struct_specifier"
                || node_kind == "enum_specifier"
                || node_kind == "union_specifier"
                || node_kind == "typedef_declaration"
                || node_kind == "preproc_function_def"
                || node_kind == "preproc_include"
        }
        Language::CPP => {
            node_kind == "translation_unit"
                || node_kind == "function_definition"
                || node_kind == "class_specifier"
                || node_kind == "struct_specifier"
                || node_kind == "enum_specifier"
                || node_kind == "union_specifier"
                || node_kind == "namespace_definition"
                || node_kind == "typedef_declaration"
                || node_kind == "preproc_function_def"
                || node_kind == "preproc_include"
                || node_kind == "using_declaration"
        }
        Language::Go => {
            node_kind == "source_file"
                || node_kind == "function_declaration"
                || node_kind == "method_declaration"
                || node_kind == "type_spec"
                || node_kind == "type_alias"
                || node_kind == "import_declaration"
        }
        Language::JavaScript | Language::TypeScript | Language::TSX => {
            node_kind == "program"
                || node_kind == "function_declaration"
                || node_kind == "function_expression"
                || node_kind == "arrow_function"
                || node_kind == "class_declaration"
                || node_kind == "method_definition"
                || node_kind == "import_statement"
        }
        Language::PHP => {
            node_kind == "program"
                || node_kind == "class_declaration"
                || node_kind == "function_declaration"
                || node_kind == "method_definition"
                || node_kind == "namespace_use_declaration"
        }
        Language::Ruby => {
            node_kind == "program"
                || node_kind == "class"
                || node_kind == "module"
                || node_kind == "method"
        }
        Language::R => node_kind == "program" || node_kind == "function_definition",
        Language::ShellScript => node_kind == "program" || node_kind == "function_definition",
        Language::Swift => {
            node_kind == "source_file"
                || node_kind == "function_declaration"
                || node_kind == "class_declaration"
                || node_kind == "struct_declaration"
                || node_kind == "enum_declaration"
                || node_kind == "protocol_declaration"
                || node_kind == "import_declaration"
        }
        Language::Kotlin => {
            node_kind == "source_file"
                || node_kind == "function_declaration"
                || node_kind == "class_declaration"
                || node_kind == "object_declaration"
                || node_kind == "companion_object"
                || node_kind == "type_alias"
                || node_kind == "import"
        }
        Language::Scala => {
            node_kind == "compilation_unit"
                || node_kind == "class_definition"
                || node_kind == "object_definition"
                || node_kind == "trait_definition"
                || node_kind == "function_definition"
                || node_kind == "import_declaration"
        }
        Language::CSharp => {
            node_kind == "compilation_unit"
                || node_kind == "class_declaration"
                || node_kind == "struct_declaration"
                || node_kind == "enum_declaration"
                || node_kind == "interface_declaration"
                || node_kind == "method_declaration"
                || node_kind == "using_directive"
        }
        Language::HTML => {
            node_kind == "document"
                || node_kind == "element"
                || node_kind == "script_element"
                || node_kind == "style_element"
        }
        Language::CSS => {
            node_kind == "stylesheet" || node_kind == "rule_set" || node_kind == "import_statement"
        }
        Language::LUA => node_kind == "chunk" || node_kind == "function_declaration",
        Language::Vimscript => node_kind == "script_file" || node_kind == "function_definition",
        // Data formats and configuration - root nodes only
        Language::JSON | Language::YAML => node_kind == "document" || node_kind == "fragment",
        // XML's `element` (any tag, e.g. `<string name="...">...</string>`) gets the same
        // reference-node exception JSON/YAML's own entries above and everything else in this
        // function relies on: `NodeSelectionConfig::min_subtree_size` (45) otherwise excludes it
        // from exact-hash matching entirely, since a single leaf-ish element is far smaller than
        // that - confirmed 2026-08-05 (`TODO.md`) on `xml-nextcloud-android-delete-element`, a
        // ~1200-entry Android `strings.xml` file where every entry parses to ~16 nodes. Without
        // this, phases 1-5 left 94% of the file (20124/21396 nodes) unmatched despite being
        // 99.9% byte-identical text, tripping `EXPENSIVE_RESIDUAL_THRESHOLD` and substituting the
        // crude Myers fallback for the whole file. Safe the same way every other reference-node
        // exception here is: this only ever *enables candidacy* for exact-hash matching, which
        // still requires byte-identical subtrees to actually match anything - it can widen what's
        // eligible to be found, never produce a wrong match.
        Language::XML => {
            node_kind == "document" || node_kind == "fragment" || node_kind == "element"
        }
        // Other languages - root node only as fallback
        _ => false,
    }
}

/// True for the import/use/include statement kinds [`is_reference`] lists for `language`.
///
/// Split out because phase 1's *shape-only* tier (`solve_hash_descent`'s `KindOnlyHash` call)
/// must skip them: two imports of unrelated names with the same number of path segments have
/// identical shape, and the tier had nothing else to go on. In `kotlin-remove-function` it paired
/// `...service.ChatServiceError` with `...CloudLlmProvider.State.Ready` (both seven segments) and
/// then spent four identifier Updates making it true - cheaper under the unit cost model than
/// delete + insert, and wrong to every reader; 64 of that fixture's 66 mismatches (2026-09-01).
/// Imports were only added to `is_reference` on 2026-07-11, with zero measured benchmark effect,
/// so nothing is lost by keeping them out of the tier that can't tell them apart. The byte-exact
/// tier and every later pass still see them.
pub fn is_import_kind(node_kind: &str, language: &Language) -> bool {
    match language {
        Language::Rust => node_kind == "use_declaration",
        Language::Python => {
            node_kind == "import_statement"
                || node_kind == "import_from_statement"
                || node_kind == "future_import_statement"
        }
        Language::Java => node_kind == "import_declaration",
        Language::C => node_kind == "preproc_include",
        Language::CPP => node_kind == "preproc_include" || node_kind == "using_declaration",
        Language::Go => node_kind == "import_declaration",
        Language::JavaScript | Language::TypeScript | Language::TSX => {
            node_kind == "import_statement"
        }
        Language::PHP => node_kind == "namespace_use_declaration",
        Language::Swift => node_kind == "import_declaration",
        Language::Kotlin => node_kind == "import",
        Language::Scala => node_kind == "import_declaration",
        Language::CSharp => node_kind == "using_directive",
        Language::CSS => node_kind == "import_statement",
        _ => false,
    }
}

/// A *scope-local* identity name for nodes like parameters, local variable declarations, and
/// shell variable assignments - the same kind of stable identity signal `is_semantically_
/// structural` provides for top-level declarations, but deliberately **not** layered onto that
/// function or the global name-resolution pipeline it feeds (`solve_qualified_name_groups`):
/// that walks the *entire* file, so adding parameters/local variables there would make every one
/// of them in the whole codebase a top-level-matchable candidate - a much bigger, riskier change
/// than the narrow, per-container mechanism this is for (`apted::prematch_unique_named_locals`)
/// needs. Returns `(kind_bucket, name)` if `node_id` is a kind of node this should consider;
/// `kind_bucket` disambiguates same-named entities of different kinds (a parameter named `x`
/// must never match a local variable also named `x`).
///
/// Uses only `ASTNodeMetadata` (kind/children/text), not a real `tree_sitter::Node` - consistent
/// with everything else in `apted/common.rs` this feeds, and sufficient here since every case
/// below only needs to walk to a specific child by *kind*, not by field name.
///
/// Confirmed against real parse trees for each arm below (a throwaway `ascii_visualizer` dump per
/// language, 2026-08-06) - not assumed from other languages' grammars.
///
/// Cheap upfront filter for `apted::prematch_unique_named_locals`'s file-root-level call site
/// (`diff.rs`, phase 6): that call runs unconditionally on every diff regardless of language, so
/// without this, every fixture pays a full O(n) tree walk (`collect_local_identities`) that can
/// never find anything for the many languages `local_identity_name` has no arm for - measured
/// (`TODO.md`) to cost a real, corpus-wide p90 regression (~120ms -> ~150ms) before this guard was
/// added. Keep in sync with `local_identity_name`'s own `match` arms by construction: both list
/// exactly the same languages, on purpose, so a future language added to one is easy to notice is
/// missing from the other.
pub(crate) fn has_local_identity_coverage(language: &Language) -> bool {
    matches!(
        language,
        Language::Kotlin | Language::CSharp | Language::ShellScript
    )
}

pub(crate) fn local_identity_name(
    node_id: usize,
    meta: &ASTMetadata,
    language: &Language,
) -> Option<(&'static str, String)> {
    let info = meta.node_info.get(&node_id)?;
    let first_child_of_kind = |parent_id: usize, wanted: &str| -> Option<usize> {
        meta.node_info
            .get(&parent_id)?
            .children
            .iter()
            .copied()
            .find(|&c| meta.node_info.get(&c).is_some_and(|i| i.kind == wanted))
    };
    match (language, info.kind.as_str()) {
        // `parameter` - "task: Task" - first child is the `identifier` naming it.
        (Language::Kotlin, "parameter") => {
            let name_id = first_child_of_kind(node_id, "identifier")?;
            let text = meta.node_info.get(&name_id)?.text.clone();
            Some(("parameter", text))
        }
        // `local_declaration_statement` - "var resources = ...;" - descends through
        // `variable_declaration` -> `variable_declarator` -> the declared `identifier`. Keyed on
        // the *statement*, not the declarator, so the whole `var x = ...` re-anchors as one unit.
        (Language::CSharp, "local_declaration_statement") => {
            let decl_id = first_child_of_kind(node_id, "variable_declaration")?;
            let declarator_id = first_child_of_kind(decl_id, "variable_declarator")?;
            let name_id = first_child_of_kind(declarator_id, "identifier")?;
            let text = meta.node_info.get(&name_id)?.text.clone();
            Some(("local_declaration_statement", text))
        }
        // `variable_assignment` - `group="${args[4]}"` - first child is the `variable_name`.
        (Language::ShellScript, "variable_assignment") => {
            let name_id = first_child_of_kind(node_id, "variable_name")?;
            let text = meta.node_info.get(&name_id)?.text.clone();
            Some(("variable_assignment", text))
        }
        _ => None,
    }
}

/*
* Semantically structural nodes are nodes that have a semantic meaning that is "loosely fixed" and
* typically enforced by the compiler in some way. For example, there can only ever be ONE
* 'fn main()' in main.rs in a Rust project. It is sensible for the algorithm to match such nodes
* immediately.
*
* Returns (node_kind, identifier) if the node matches.
*/
pub fn is_semantically_structural<'a>(
    node: &Node<'a>,
    language: &Language,
    code: &Code,
) -> Option<(String, String)> {
    let node_kind = node.kind();

    let bytes = code.contents.as_bytes();

    // Language-specific reference nodes
    match language {
        Language::Rust => match node_kind {
            "function_item" | "mod_item" => node
                .child_by_field_name("name")
                .filter(|n| n.kind() == "identifier")
                .and_then(|n| n.utf8_text(bytes).ok())
                .map(|name| (node_kind.to_string(), name.to_string())),
            "struct_item" | "enum_item" | "trait_item" => node
                .child_by_field_name("name")
                .filter(|n| n.kind() == "type_identifier")
                .and_then(|n| n.utf8_text(bytes).ok())
                .map(|name| (node_kind.to_string(), name.to_string())),
            "impl_item" => {
                let type_name = node
                    .child_by_field_name("type")
                    .and_then(|n| n.utf8_text(bytes).ok())?;
                let trait_name = node
                    .child_by_field_name("trait")
                    .and_then(|n| n.utf8_text(bytes).ok());
                let key = match trait_name {
                    Some(t) => format!("{t} for {type_name}"),
                    None => type_name.to_string(),
                };
                Some((node_kind.to_string(), key))
            }
            _ => None,
        },
        Language::Python => match node_kind {
            "function_definition" | "class_definition" => node
                .child_by_field_name("name")
                .filter(|n| n.kind() == "identifier")
                .and_then(|n| n.utf8_text(bytes).ok())
                .map(|name| (node_kind.to_string(), name.to_string())),
            _ => None,
        },
        Language::Go => match node_kind {
            "function_declaration" => node
                .child_by_field_name("name")
                .filter(|n| n.kind() == "identifier")
                .and_then(|n| n.utf8_text(bytes).ok())
                .map(|name| (node_kind.to_string(), name.to_string())),
            "method_declaration" => {
                let method_name = node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(bytes).ok())?;
                let receiver = node.child_by_field_name("receiver")?;
                let mut rc = receiver.walk();
                let param_decl = receiver.named_children(&mut rc).next()?;
                let type_node = param_decl.child_by_field_name("type")?;
                let type_text = type_node.utf8_text(bytes).ok()?;
                // Strip leading `*` for pointer receivers: `*Foo` \u{2192} `Foo`
                let receiver_type = type_text.trim_start_matches('*');
                Some((
                    node_kind.to_string(),
                    format!("{receiver_type}.{method_name}"),
                ))
            }
            "type_spec" | "type_alias" => node
                .child_by_field_name("name")
                .filter(|n| n.kind() == "type_identifier")
                .and_then(|n| n.utf8_text(bytes).ok())
                .map(|name| (node_kind.to_string(), name.to_string())),
            // A top-level `var tests = []T{...}` (or `const`) declaration is exactly as common a
            // home for a large, table-driven data literal as a named function is, but had no
            // identity signal at all before this - confirmed via a live case
            // (jesseduffield/lazygit's `test_list.go`, a single `var tests = []*IntegrationTest{
            // ...}`): with no name for `solve_large_flat_subtrees`'s `top_level_identities` (which
            // only looks at direct children of the file root, not arbitrary depth) to key off, the
            // file's entire ~2,600-node content fell through every pass onto `final_pass`'s
            // unconstrained tree-edit-distance on every edit (4.6s). Keyed on the *declaration*
            // itself (not its `var_spec`/`const_spec` child) specifically so `top_level_identities`
            // can see it without recursing; a grouped `var (a = 1; b = 2)` block is keyed by its
            // first name only (a simplification, not a correctness issue - the group is still
            // matched as one unit across before/after as long as that first name is unchanged).
            // `var_spec`/`const_spec` are *also* matched independently below, for
            // `solve_qualified_name_groups`'s fully-recursive, finer-grained walk - the two
            // consumers have different needs (direct-children-only vs. any depth), so both arms
            // are useful rather than redundant.
            "var_declaration" | "const_declaration" => {
                let mut cursor = node.walk();
                let spec = node
                    .named_children(&mut cursor)
                    .find(|c| c.kind() == "var_spec" || c.kind() == "const_spec")?;
                go_spec_identifier_name(spec, bytes)
                    .map(|name| (node_kind.to_string(), name.to_string()))
            }
            "var_spec" | "const_spec" => go_spec_identifier_name(*node, bytes)
                .map(|name| (node_kind.to_string(), name.to_string())),
            "call_expression" => {
                go_subtest_call_name(node, bytes).map(|name| (node_kind.to_string(), name))
            }
            _ => None,
        },
        Language::Kotlin => match node_kind {
            "function_declaration" => {
                let name_node = node
                    .child_by_field_name("name")
                    .filter(|n| n.kind() == "identifier")?;
                let func_name = name_node.utf8_text(bytes).ok()?;
                let name_start = name_node.start_byte();

                // Extension function receiver: unnamed user_type child before the name node
                let receiver_prefix: String = {
                    let mut cur = node.walk();
                    node.named_children(&mut cur)
                        .find(|c| c.kind() == "user_type" && c.start_byte() < name_start)
                        .and_then(|r| r.utf8_text(bytes).ok())
                        .map(|s| {
                            // Strip type params for key stability: `List<String>` \u{2192} `List`
                            let base = s.split('<').next().unwrap_or(s).trim();
                            format!("{}.", base)
                        })
                        .unwrap_or_default()
                };

                // Parameter types for overload disambiguation
                let param_sig: String = {
                    let mut cur = node.walk();
                    let fvp_opt = node
                        .named_children(&mut cur)
                        .find(|c| c.kind() == "function_value_parameters");
                    match fvp_opt {
                        None => "()".to_string(),
                        Some(fvp) => {
                            let mut c2 = fvp.walk();
                            let types: Vec<String> = fvp
                                .named_children(&mut c2)
                                .filter(|c| c.kind() == "parameter")
                                .filter_map(|param| {
                                    let mut pc = param.walk();
                                    // First named child is the param name (identifier);
                                    // the next named child is the type.
                                    param
                                        .named_children(&mut pc)
                                        .find(|c| c.kind() != "identifier")
                                        .and_then(|t| t.utf8_text(bytes).ok())
                                        .map(|s| s.to_string())
                                })
                                .collect();
                            format!("({})", types.join(","))
                        }
                    }
                };

                Some((
                    node_kind.to_string(),
                    format!("{}{}{}", receiver_prefix, func_name, param_sig),
                ))
            }
            "class_declaration" | "object_declaration" => node
                .child_by_field_name("name")
                .filter(|n| n.kind() == "identifier")
                .and_then(|n| n.utf8_text(bytes).ok())
                .map(|name| (node_kind.to_string(), name.to_string())),
            "companion_object" => node
                .child_by_field_name("name")
                .filter(|n| n.kind() == "identifier")
                .and_then(|n| n.utf8_text(bytes).ok())
                .map(|name| (node_kind.to_string(), name.to_string())),
            // `typealias Foo = Bar` uses field "type" (not "name") for the alias identifier
            "type_alias" => node
                .child_by_field_name("type")
                .filter(|n| n.kind() == "identifier")
                .and_then(|n| n.utf8_text(bytes).ok())
                .map(|name| (node_kind.to_string(), name.to_string())),
            _ => None,
        },
        // Previously entirely unhandled here (confirmed empirically 2026-07-25, chasing a
        // 30s-on-235-lines pathology in `csharp-radarr-add-object-instance`): `is_reference`
        // above already lists these C# kinds for hash-candidate selection, but nothing extracted
        // a *name* from them, so `solve_qualified_name_groups`/`solve_large_flat_subtrees` (both
        // keyed on `is_semantically_structural`) silently treated every C# file as having zero
        // named declarations - no class, method, or field ever got the cheap identity-based match
        // every other supported language gets. The 49-vs-50-entry `new IsoLanguage(...)` list in
        // that fixture's one field was too small to qualify for `NodeSelectionConfig`'s
        // exact-hash candidate list (`min_subtree_size: 45`, each entry ~30 nodes) and never
        // reached `solve_large_flat_subtrees`'s Myers fast path either (scoped to *named*
        // top-level items only, and C# had none) - so the whole ~2,300-node file fell to
        // `final_pass`'s unconstrained tree-edit-distance on every edit: 30.6s, of which 30.3s
        // was that one pass (profiled - see `TODO.md`'s speed-tuning entry). Field names verified
        // empirically against the real grammar (a throwaway binary dumping `child_by_field_name`
        // results on this fixture), not assumed from other C-family grammars.
        Language::CSharp => match node_kind {
            "class_declaration"
            | "struct_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "method_declaration"
            | "namespace_declaration" => node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(bytes).ok())
                .map(|name| (node_kind.to_string(), name.to_string())),
            // No direct "name" field (unlike the kinds above) - wraps a `variable_declaration`
            // holding one or more `variable_declarator`s (`int a, b;` is one `field_declaration`
            // naming two variables). Keyed on the *first* declarator only, same simplification
            // Go's grouped `var (...)`/`const (...)` handling above already uses: not a
            // correctness issue, the field is still matched as one unit as long as its first
            // name is unchanged.
            "field_declaration" => {
                let mut cursor = node.walk();
                let variable_declaration = node
                    .named_children(&mut cursor)
                    .find(|c| c.kind() == "variable_declaration")?;
                let mut declarator_cursor = variable_declaration.walk();
                let declarator = variable_declaration
                    .named_children(&mut declarator_cursor)
                    .find(|c| c.kind() == "variable_declarator")?;
                declarator
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(bytes).ok())
                    .map(|name| (node_kind.to_string(), name.to_string()))
            }
            _ => None,
        },
        // Same 2026-07-25 gap as the CSharp arm above, found chasing the same class of pathology
        // once it turned out C and C++ dominate the remaining slow outliers even after the C# fix
        // (6 of the corpus's 10 slowest fixtures are C/C++). `function_definition`'s name isn't a
        // direct field - C/C++ grammars nest it inside a chain of declarator wrappers
        // (`pointer_declarator`/`array_declarator`/... for return-type modifiers, `function_
        // declarator` for the parameter list itself), terminating in the actual name node -
        // `c_family_declarator_name` walks that chain. Verified empirically against real fixtures
        // (`c-nginx-add-typedef`: `pointer_declarator -> function_declarator -> identifier`;
        // `cpp-ladybird-refactor-variables-if-changes`: `function_declarator ->
        // qualified_identifier`, which already carries full `Class::method` scoping - no separate
        // impl/class pre-pass needed the way Rust's `impl_item` handling has one).
        Language::C => match node_kind {
            "function_definition" => node
                .child_by_field_name("declarator")
                .and_then(|d| c_family_declarator_name(d, bytes))
                .map(|name| (node_kind.to_string(), name.to_string())),
            "struct_specifier" | "enum_specifier" | "union_specifier" => node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(bytes).ok())
                .map(|name| (node_kind.to_string(), name.to_string())),
            _ => None,
        },
        Language::CPP => match node_kind {
            // `c_family_test_macro_name` (see its own doc comment) takes priority: a googletest
            // `TEST(Suite, Case)` block parses as a real `function_definition` whose own name is
            // the literal macro name, which - left unhandled - collapses every such block in the
            // file into one shared-name, cost-tie-broken candidate group instead of matching each
            // uniquely by its real suite/case name.
            "function_definition" => c_family_test_macro_name(node, bytes)
                .map(|name| (node_kind.to_string(), name))
                .or_else(|| {
                    node.child_by_field_name("declarator")
                        .and_then(|d| c_family_declarator_name(d, bytes))
                        .map(|name| (node_kind.to_string(), name.to_string()))
                }),
            "class_specifier" | "struct_specifier" | "enum_specifier" | "union_specifier" => node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(bytes).ok())
                .map(|name| (node_kind.to_string(), name.to_string())),
            // Already returns a fully-qualified name for `namespace A::B { ... }` (a
            // `nested_namespace_specifier`, not a plain `namespace_identifier`) - confirmed
            // empirically, no extra unwrapping needed unlike `function_definition` above.
            "namespace_definition" => node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(bytes).ok())
                .map(|name| (node_kind.to_string(), name.to_string())),
            _ => None,
        },
        // Same 2026-07-26 gap as the CSharp/C/CPP arms above (`TODO.md`'s speed-goal
        // investigation) - completing coverage for every language `is_reference` above already
        // lists a kind set for. Java/JS/TS/TSX verified empirically against real corpus fixtures
        // (same throwaway-binary-against-real-grammar-output method as the C-family arms); the
        // rest (below, in the final `_ =>` fallback comment) have no fixtures in this corpus to
        // verify against - see that comment for what "unvalidated" means there.
        Language::Java => match node_kind {
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "method_declaration" => node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(bytes).ok())
                .map(|name| (node_kind.to_string(), name.to_string())),
            // No direct "name" field - wraps a `variable_declarator` (possibly several, for
            // `int a, b;`) the same shape C#'s `field_declaration` arm above already handles,
            // minus that language's extra `variable_declaration` wrapper layer (confirmed
            // empirically: Java nests `variable_declarator` directly under `field_declaration`).
            // Keyed on the first declarator only, same simplification as C#'s/Go's arms.
            "field_declaration" => {
                let mut cursor = node.walk();
                let declarator = node
                    .named_children(&mut cursor)
                    .find(|c| c.kind() == "variable_declarator")?;
                declarator
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(bytes).ok())
                    .map(|name| (node_kind.to_string(), name.to_string()))
            }
            _ => None,
        },
        Language::JavaScript | Language::TypeScript | Language::TSX => match node_kind {
            "function_declaration"
            | "class_declaration"
            | "method_definition"
            | "interface_declaration"
            | "type_alias_declaration" => node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(bytes).ok())
                .map(|name| (node_kind.to_string(), name.to_string())),
            // `const f = () => ...`/`const f = function() {...}`: the function itself has no
            // name field (confirmed empirically - only its *enclosing* `variable_declarator`
            // does), unlike a `function_declaration`. Deliberately narrow: only fires when the
            // function is the *direct* value of a declarator, not when it's passed as a callback
            // argument (`arr.map(x => ...)`) - confirmed empirically those parent as `arguments`,
            // not `variable_declarator`, and a callback argument genuinely has no identity of its
            // own to match on.
            "arrow_function" | "function_expression" => {
                let parent = node.parent()?;
                (parent.kind() == "variable_declarator")
                    .then(|| parent.child_by_field_name("name"))
                    .flatten()
                    .and_then(|n| n.utf8_text(bytes).ok())
                    .map(|name| (node_kind.to_string(), name.to_string()))
            }
            // Fallback for a top-level `const`/`let`/`var X = <value>` whose value isn't itself a
            // function/class (those already get a more specific identity above, keyed on the
            // *declarator*) - e.g. `const APP_STATE_STORAGE_CONF = (<generic IIFE>)({...90
            // properties...})`. Without this, such a declaration has no identity signal at all -
            // both `solve_large_flat_subtrees` (which looks for a top-level identity match via
            // `top_level_identities`, checking `program`'s *direct* children - a `lexical_
            // declaration`, not its nested `variable_declarator`) and phase 4's named-group
            // matching can't isolate it, so its entire subtree falls through to final whole-tree
            // APTED. Deliberately keyed on the *declaration* node (not the declarator, unlike the
            // `arrow_function` arm above) specifically so `top_level_identities`'s direct-children
            // walk sees it, letting `solve_large_flat_subtrees` Myers-diff the flat descendant
            // cheaply instead of routing through a real (and, for this shape, pathological)
            // `apted::for_nodes` call.
            //
            // Measured live (`typescript-excalidraw-excalidraw-add-values-to-lists`): a two-line
            // edit (one new property in each of two ~90-property object literals, one of them
            // exactly this IIFE-const shape, densely packed with dozens of byte-identical
            // `{ browser: false, export: false, server: false }`-shaped sibling values) took
            // **36 seconds** - by far the single worst outlier in the whole corpus (the next
            // slowest fixture is 25x faster) - because that IIFE-const's ~1500-node subtree had no
            // identity signal and landed in phase 6's final APTED, which chokes specifically on
            // that duplicate-value cluster (confirmed: keying this arm on `variable_declarator`
            // instead - visible to phase 4's recursive named-group walk, but invisible to `top_
            // level_identities`'s direct-children-only walk - still cost ~43s, i.e. no better,
            // since it still forced one giant real-APTED call over the same duplicate-heavy
            // subtree instead of the cheap Myers path).
            //
            // Deliberately scoped to a *top-level* declaration with exactly one declarator (parent
            // is `program` directly, or an `export_statement` that is) and a plain identifier name
            // (not a destructuring pattern): unlike a top-level function/class name, an arbitrary
            // local variable name (`result`, `i`, `config`) is not reliably unique within a file -
            // the same false-positive risk already documented and avoided elsewhere for
            // local-variable anchoring (see `TODO.md`'s "Explored and shelved: recognizing smaller
            // structural pieces within a changed method"). A top-level module const doesn't have
            // that problem: it's declared exactly once, at module scope, same as a top-level
            // function - and multi-declarator statements (`const a = 1, b = 2;`) are skipped
            // rather than guessing which declarator the single returned name should represent.
            "lexical_declaration" | "variable_declaration" => {
                let mut cursor = node.walk();
                let mut declarators = node
                    .children(&mut cursor)
                    .filter(|c| c.kind() == "variable_declarator");
                let declarator = declarators.next()?;
                if declarators.next().is_some() {
                    return None; // more than one declarator - ambiguous, skip.
                }
                let name_node = declarator
                    .child_by_field_name("name")
                    .filter(|n| n.kind() == "identifier")?;
                let is_function_or_class_value =
                    declarator.child_by_field_name("value").is_some_and(|v| {
                        matches!(v.kind(), "arrow_function" | "function_expression" | "class")
                    });
                let parent = node.parent()?;
                let is_top_level = parent.kind() == "program"
                    || (parent.kind() == "export_statement"
                        && parent.parent().is_some_and(|pp| pp.kind() == "program"));
                (!is_function_or_class_value && is_top_level)
                    .then(|| name_node.utf8_text(bytes).ok())
                    .flatten()
                    .map(|name| (node_kind.to_string(), name.to_string()))
            }
            _ => None,
        },
        // Was "unvalidated" (added via the same best-effort `name`-field convention every checked
        // language uses, but with no PHP fixture in the corpus to verify against) until real PHP
        // fixtures showed up and turned out slow for the same reason as Ruby's `singleton_method`
        // gap: two of these three kind names were simply wrong. Verified against tree-sitter-php's
        // actual grammar (throwaway sexp-dump test, deleted after use): a top-level `function foo()
        // {}` is `function_definition`, not `function_declaration`; a class method is `method_
        // declaration`, not `method_definition`. Only `class_declaration` was already correct.
        // Measured 2026-08-02 (`/goal` speed investigation): `php-wordpress-wordpress-add-null-to-
        // return` (2823 lines, 28 top-level functions, one `return;` -> `return null;` edit in one
        // of them) - every one of those 28 functions was invisible to phase 4, same "no named
        // candidates, falls back to one giant blob" pattern as Ruby/YAML.
        Language::PHP => match node_kind {
            "class_declaration" | "function_definition" | "method_declaration" => node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(bytes).ok())
                .map(|name| (node_kind.to_string(), name.to_string())),
            _ => None,
        },
        Language::Ruby => match node_kind {
            // `singleton_method` (`def self.foo`, Ruby's class/module-level method syntax) is a
            // distinct grammar node from plain instance `method` (`def foo`) - omitting it left
            // every `def self.*`-only file (e.g. a Homebrew formula-API helper module) with *no*
            // method-level named candidates at all, forcing phase 4's named-group matching to fall
            // back to whatever enclosing `class`/`module` it could still see - which, for a file
            // that's just a chain of near-empty wrapper modules around the real content, meant one
            // multi-thousand-node APTED call instead of many small per-method ones (measured
            // 2026-08-02: `ruby-homebrew-add-or-expression`, 4.2s dominated by a single `module`
            // pair with a 1022-node residual, for a fixture whose only real edit is one line inside
            // one `def self.*` method - see `TODO.md`).
            "class" | "module" | "method" | "singleton_method" => node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(bytes).ok())
                .map(|name| (node_kind.to_string(), name.to_string())),
            _ => None,
        },
        // Every remaining language `is_reference` above lists a kind set for, but with no
        // fixture in this corpus to verify field names against empirically (unlike every arm
        // above, all confirmed live against real grammar output). Following the same `name`-field
        // convention every verified language so far has used without exception, but genuinely
        // **unvalidated** - treat these as a best-effort starting point, not a confirmed fix, and
        // verify against real source the first time one of these languages gets an actual
        // fixture (the same throwaway-binary method used for every arm above works for this too).
        Language::Swift => match node_kind {
            "function_declaration"
            | "class_declaration"
            | "struct_declaration"
            | "enum_declaration"
            | "protocol_declaration" => node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(bytes).ok())
                .map(|name| (node_kind.to_string(), name.to_string())),
            _ => None,
        },
        Language::Scala => match node_kind {
            "class_definition"
            | "object_definition"
            | "trait_definition"
            | "function_definition" => node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(bytes).ok())
                .map(|name| (node_kind.to_string(), name.to_string())),
            _ => None,
        },
        Language::R | Language::ShellScript | Language::LUA | Language::Vimscript => {
            match node_kind {
                "function_definition" | "function_declaration" => node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(bytes).ok())
                    .map(|name| (node_kind.to_string(), name.to_string())),
                _ => None,
            }
        }
        // YAML has no declarations in the function/class sense, but `block_mapping_pair`'s `key`
        // field is the same kind of stable identity signal for exactly the same reason: a large
        // localization/config YAML file is nothing but nested `key: value` mappings, and without
        // this, phase 4 has zero named candidates to isolate a single changed key with - measured
        // 2026-08-02 (`/goal` speed investigation): `yaml-mastodon-remove-one-pair` (939 lines, one
        // `following: Abonaments` pair removed, otherwise untouched) cost 2442.1ms in phase 4 alone
        // (93.3% of its 2619.1ms total), one giant top-level-mapping APTED call, for exactly the
        // same reason Ruby's missing `singleton_method` arm did - see that fix's own comment above.
        // Safe against repeated keys (`one`/`other`/`name`, ubiquitous across sibling objects in a
        // locale file) the same way Ruby's `Bar::new` vs `Foo::new` is safe: every enclosing
        // `block_mapping_pair` that's itself a candidate contributes its own key to the fully-
        // resolved scope chain (`solve_qualified_name_groups`'s doc comment), so two pairs only
        // ever share an identity if their *entire* ancestor key path matches, not just the leaf key.
        Language::YAML if node_kind == "block_mapping_pair" => node
            .child_by_field_name("key")
            .and_then(|n| n.utf8_text(bytes).ok())
            .map(|name| (node_kind.to_string(), name.to_string())),
        _ => None,
    }
}

/// Recognizes Go's ubiquitous subtest idiom - `t.Run("name", func(t *testing.T) {...})`
/// (`testing.T`/`testing.B`), and the same shape from popular third-party test frameworks that
/// mirror it (quicktest's `c.Run`, testify's `suite.Run`, ...) - by structure alone: a call whose
/// callee is `<anything>.Run` and whose first argument is a string literal. The receiver name is
/// deliberately not checked (it varies: `t`, `c`, `s`, `suite`, ...); "a `.Run(\"literal\", ...)`"
/// call is itself already a strong, low-false-positive signal.
///
/// Confirmed via a live case (`gohugoio/hugo`'s `securitypolicies_test.go`): a single test
/// function's 12 subtests, individually renamed/restructured internally but keeping the same 12
/// subtest names, had no identity signal at all before this - `call_expression` isn't a
/// declaration `is_semantically_structural` otherwise recognizes - so the *entire* surrounding
/// test function (whichever one happened to contain them) fell to `final_apted` as one 3,286-node
/// blob on every edit (24s wall-clock) instead of 12 small, independently-anchored ~130-270-node
/// diffs. Only a mismatched (wrong-position, wrong-content) false positive elsewhere could make
/// this heuristic *wrong* rather than merely a no-op miss, and even then only affects match
/// quality (a coincidental non-test `.Run("...")` call getting grouped as if it had an identity),
/// never correctness - `solve_qualified_name_groups` still runs real APTED on whatever it groups.
/// The `identifier` name of a Go `var_spec`/`const_spec` (or, for a grouped `var (...)`/`const
/// (...)` declaration, the same lookup applied to its first spec child).
fn go_spec_identifier_name<'a>(spec: Node<'a>, bytes: &'a [u8]) -> Option<&'a str> {
    spec.child_by_field_name("name")
        .filter(|n| n.kind() == "identifier")
        .and_then(|n| n.utf8_text(bytes).ok())
}

/// Unwraps a C/C++ `function_definition`'s `declarator` field chain down to the actual name -
/// `pointer_declarator`/`array_declarator`/`parenthesized_declarator`/`reference_declarator` are
/// return-type/reference modifiers wrapping a nested `declarator` field of their own, terminating
/// in `function_declarator`, whose own `declarator` field is finally the real name node
/// (`identifier` in C; `identifier`/`qualified_identifier`/`destructor_name`/`operator_name` in
/// C++ - a `qualified_identifier` already carries full `Class::method` scoping, verified
/// empirically against `cpp-ladybird-refactor-variables-if-changes`). Verified against real C
/// fixtures too: `c-nginx-add-typedef` nests exactly one `pointer_declarator` before its
/// `function_declarator` for every pointer-returning function.
fn c_family_declarator_name<'a>(node: Node<'a>, bytes: &'a [u8]) -> Option<&'a str> {
    match node.kind() {
        "identifier"
        | "field_identifier"
        | "qualified_identifier"
        | "destructor_name"
        | "operator_name" => node.utf8_text(bytes).ok(),
        "function_declarator"
        | "pointer_declarator"
        | "array_declarator"
        | "parenthesized_declarator"
        | "reference_declarator" => node
            .child_by_field_name("declarator")
            .and_then(|d| c_family_declarator_name(d, bytes)),
        _ => None,
    }
}

/// Recognizes googletest's `TEST`/`TEST_F`/`TEST_P` macro idiom - `TEST(Suite, Case) { ... }` -
/// which tree-sitter-cpp parses as an *ordinary* `function_definition` (it has no idea `TEST` is a
/// macro): the macro name itself becomes the function's own declarator identifier, and `Suite`/
/// `Case` become two *anonymous* parameters typed `Suite`/`Case` (a valid, if unusual, C++ parse -
/// a function declaration with unnamed parameters). Confirmed empirically via a sexp dump:
/// `function_definition declarator: (function_declarator declarator: (identifier "TEST")
/// parameters: (parameter_list (parameter_declaration type: (type_identifier "Suite"))
/// (parameter_declaration type: (type_identifier "Case"))))`.
///
/// Without this, every `TEST(...)`/`TEST_F(...)`/`TEST_P(...)` block in a file resolves to the
/// *identical* literal name "TEST"/"TEST_F"/"TEST_P" via the ordinary `c_family_declarator_name`
/// path below, collapsing potentially dozens of genuinely distinct, uniquely-named test functions
/// into one shared-name candidate group - `solve_qualified_name_groups`'s N:M support then has to
/// pairwise cost-compare all of them to decide which pairs with which, instead of matching 1:1 by
/// name for free. Measured live (`cpp-opencv-add-test-case`): adding one new, uniquely-named
/// `TEST(...)` block among ~15 pre-existing ones cost 691ms in phase 4 alone, almost entirely this
/// pairwise cost-scoring over a group that should never have existed - see `TODO.md`.
///
/// Returns `"<macro>:<Suite>:<Case>"` so each test function gets its own genuinely unique identity.
/// Guards against colliding with a real, hand-written function that happens to be named `TEST`/
/// `TEST_F`/`TEST_P` and takes exactly two parameters (`bool TEST(int a, int b) {...}`, unlikely
/// but possible): only fires when both parameters are genuinely anonymous (no `declarator` field
/// of their own), the exact shape this macro idiom - and no ordinary named-parameter function -
/// produces.
fn c_family_test_macro_name(node: &Node, bytes: &[u8]) -> Option<String> {
    let declarator = node.child_by_field_name("declarator")?;
    if declarator.kind() != "function_declarator" {
        return None;
    }
    let macro_name = declarator
        .child_by_field_name("declarator")
        .filter(|d| d.kind() == "identifier")
        .and_then(|d| d.utf8_text(bytes).ok())
        .filter(|name| matches!(*name, "TEST" | "TEST_F" | "TEST_P"))?;

    let parameters = declarator.child_by_field_name("parameters")?;
    let mut cursor = parameters.walk();
    let params: Vec<Node> = parameters
        .named_children(&mut cursor)
        .filter(|c| c.kind() == "parameter_declaration")
        .collect();
    let [suite, case] = params.as_slice() else {
        return None;
    };
    if suite.child_by_field_name("declarator").is_some()
        || case.child_by_field_name("declarator").is_some()
    {
        return None; // Named parameters - a real function, not this macro idiom.
    }
    let suite_name = suite
        .child_by_field_name("type")
        .filter(|t| t.kind() == "type_identifier")
        .and_then(|t| t.utf8_text(bytes).ok())?;
    let case_name = case
        .child_by_field_name("type")
        .filter(|t| t.kind() == "type_identifier")
        .and_then(|t| t.utf8_text(bytes).ok())?;
    Some(format!("{macro_name}:{suite_name}:{case_name}"))
}

fn go_subtest_call_name(node: &Node, bytes: &[u8]) -> Option<String> {
    let function = node.child_by_field_name("function")?;
    if function.kind() != "selector_expression" {
        return None;
    }
    let method = function.child_by_field_name("field")?;
    if method.utf8_text(bytes).ok()? != "Run" {
        return None;
    }
    let arguments = node.child_by_field_name("arguments")?;
    let mut cursor = arguments.walk();
    let first_arg = arguments.named_children(&mut cursor).next()?;
    if !matches!(
        first_arg.kind(),
        "interpreted_string_literal" | "raw_string_literal"
    ) {
        return None;
    }
    let text = first_arg.utf8_text(bytes).ok()?;
    Some(text.trim_matches(|c| c == '"' || c == '`').to_string())
}

// Families of single-token operator kinds that occupy the same grammatical "slot" in a
// TreeSitter grammar (e.g. the `operator` field of `binary_expression`/`assignment_expression`).
// Two leaf nodes whose kinds fall in the same family represent the same conceptual operation with
// a different operator, so matching them as an Update (rather than a Delete+Insert) mirrors what
// a human would consider "the same node, tweaked" - e.g. the classic `<` -> `<=` off-by-one fix.
// Crucially these are separate arrays, not one big list: `kinds_update_allowed` only allows a
// match when both kinds fall in the *same* family, so `<` can cross to `<=` but never to `+`.
//
// Tokens are drawn from the actual TreeSitter grammars this project depends on (verified against
// each crate's `node-types.json`), not guessed. A token that never appears in a given language's
// grammar is harmless to leave in these shared lists - it simply never matches anything.

/// Relational and equality comparisons (including alternate spellings: C++'s `not_eq`/`<=>`,
/// JS/PHP's `===`/`!==`, Python/PHP's old-style `<>`, Ruby's `=~`/`!~` match operators).
///
/// Deliberately excludes keyword-based comparisons that double as other syntax in the same
/// grammar (Python/JS `in`, Python `is`/`is not`/`not in`, JS/PHP `instanceof`): those tokens
/// also appear outside of comparisons (e.g. `for x in y`), so allowing them here risks the DP
/// matching an unrelated keyword occurrence purely because it's a cheaper leaf-level swap.
const COMPARISON_OPS: &[&str] = &[
    "<", "<=", ">", ">=", "==", "!=", "===", "!==", "<>", "<=>", "not_eq", "=~", "!~",
];

/// Arithmetic operators.
const ARITHMETIC_OPS: &[&str] = &["+", "-", "*", "/", "%", "**", "//", "@"];

/// PHP additionally uses `.` as its string-concatenation operator, in the same `binary_expression`
/// slot as the arithmetic operators - a human swapping `.` for `+` (or vice versa) is a classic
/// PHP typo/bugfix.
const PHP_ARITHMETIC_OPS: &[&str] = &["+", "-", "*", "/", "%", "**", "."];

/// Bitwise operators (including C++'s alternative keyword spellings and Go's `&^` AND-NOT).
const BITWISE_OPS: &[&str] = &[
    "&", "|", "^", "<<", ">>", ">>>", "&^", "bitand", "bitor", "xor",
];

/// Logical/boolean operators, including keyword spellings and the null-coalescing/Elvis operators
/// (`??`, `?:`), which occupy the same "fallback value" slot as `||` in these grammars.
const LOGICAL_OPS: &[&str] = &["&&", "||", "and", "or", "??", "?:"];

/// Plain `=` and every compound/augmented-assignment spelling. Converting `x = x + 1` into
/// `x += 1` is a one-token operator change on the same kind of statement, not a different one.
const ASSIGNMENT_OPS: &[&str] = &[
    "=", "+=", "-=", "*=", "/=", "%=", "**=", "//=", "&=", "|=", "^=", "<<=", ">>=", ">>>=", "&&=",
    "||=", "??=", "@=", ".=", "and_eq", "or_eq", "xor_eq", "&^=",
];

/// Increment/decrement.
const INCREMENT_OPS: &[&str] = &["++", "--"];

/// Rust's range operators: `a..b` (exclusive), `a..=b` (inclusive), and the pattern-only `a...b`
/// spelling. Switching between exclusive and inclusive bounds is the single most common Rust
/// off-by-one fix (e.g. `for i in 0..n` -> `for i in 0..=n`).
const RUST_RANGE_OPS: &[&str] = &["..", "..=", "..."];

/// Member-access operators: plain `.`, null-safe `?.` (Kotlin/Swift/TypeScript/C#), C/C++'s
/// pointer `->`, PHP's `?->` and Ruby's `&.`. Switching one for another is the single most common
/// shape of a null-safety or value-to-pointer refactor (`foo.bar` -> `foo?.bar`, `s.x` -> `s->x`),
/// and the human mappings consistently read it as the same access edited - the operator leaf sits
/// in the same `navigation_expression`/`field_expression` slot either way. Not given to PHP, where
/// `.` is string concatenation (`PHP_ARITHMETIC_OPS`) and pairing it with `->` would be a category
/// error.
const MEMBER_ACCESS_OPS: &[&str] = &[".", "?.", "->", "?->", "&."];

// Deliberately *no* C/C++ `type_identifier`/`primitive_type` family, although the pair is the
// most frequent cross-kind leaf edit in the corpus: the ground truth contradicts itself on it.
// `cpp-tensorflow-switch-to-primitive-types` (alias -> `int`) pairs the two leaves as an update;
// `cpp-add-templates` (`int` -> `T`) and `c-linux-small-change-struct-to-char` (`struct x` ->
// `char`) delete one and insert the other in exactly the same declaration slot. Measured
// 2026-09-01: the family trades 6 -> 0 on the first for 0 -> 6 and 2 -> 4 on the other two.
// Until the annotations agree, neither reading can be encoded.

/// Numeric literal kinds across every grammar that splits integers from floats (tree-sitter names
/// vary per language, hence the length). `1` -> `1.0` or `0` -> `0.5f` is a value edit of one
/// literal, not the removal of one literal and the arrival of an unrelated other - the human
/// mappings pair them without exception. Kinds a grammar doesn't have are simply never seen.
const NUMERIC_LITERAL_KINDS: &[&str] = &[
    "integer_literal",
    "float_literal",
    "number_literal",
    "real_literal",
    "long_literal",
    "hex_literal",
    "bin_literal",
    "decimal_integer_literal",
    "hex_integer_literal",
    "octal_integer_literal",
    "binary_integer_literal",
    "decimal_floating_point_literal",
    "hex_floating_point_literal",
    "int_literal",
    "imaginary_literal",
    "floating_point_literal",
    "integer",
    "float",
    "integer_value",
    "float_value",
];

/// Shell's `[ a == b ]` / `[ a = b ]` / `[ a != b ]` test-command operators. The bash grammar
/// yields each as its own anonymous leaf kind, and `==` -> `=` (the POSIX-portable spelling) is a
/// classic shell cleanup (`shellscript-torvalds-linux-double-equals-to-equals`).
const SHELL_TEST_OPS: &[&str] = &["==", "=", "!="];

/// Access-modifier keywords. `public` -> `protected` is an edit of the modifier slot, not a
/// deletion plus an insertion (`java-scrcpy-public-to-protected`). Not given to C#: in
/// `csharp-glibsharp-gtksharp-interesting-case-...` the only such pair sits in a deleted
/// constructor and an inserted property, where the cheaper cross-kind pairing is exactly the
/// wrong answer, and no C# fixture has the same-slot edit that would pay for it.
const ACCESS_MODIFIERS: &[&str] = &[
    "public",
    "private",
    "protected",
    "internal",
    "fileprivate",
    "open",
];

/// Variable-declaration keywords: `let`/`var`/`const` (JavaScript, TypeScript, Swift) and
/// Kotlin's `val`/`var`. Flipping mutability is an edit of the declaration's keyword slot.
const DECLARATION_KEYWORDS: &[&str] = &["let", "var", "const", "val"];

/// HTML/XML's two ways to end an opening tag: `>` and the self-closing `/>`. Toggling one is a
/// one-token edit of the tag (`html-hugo-tag-to-selfclosing-tag`), though the grammar then
/// re-kinds the whole tag (`start_tag` vs `self_closing_tag`) - that container pair is
/// deliberately *not* a family here, per the `TS_TYPE_KEYWORD_KINDS` lesson: pair the leaf, not
/// the parent, or APTED takes the parent-level pairing and the leaf mapping comes out wrong.
const HTML_TAG_END: &[&str] = &["/>", ">"];

/// Ruby's two hash-pair separators - `:key => v` and `key: v`. A style-guide rewrite from one to
/// the other edits the separator of every pair in place
/// (`ruby-jmespath-jmespath-formatting-and-style-guide-fixes`). The key itself also changes kind
/// (`simple_symbol` -> `hash_key_symbol`), but that pair is deliberately *not* a family: measured
/// 2026-09-01, it let APTED pair a symbol inside a deleted `pair` with one inside an inserted
/// `element_reference` in both `ruby-homebrew-brew-*` fixtures (cost 1 beats delete + insert),
/// costing more than the same-slot edits it bought.
const RUBY_HASH_SEPARATORS: &[&str] = &["=>", ":"];

/// Python's `None` (kind `none`) swapped for a name, or back: `x = None` -> `x = default_value`
/// edits the value slot (`python-pytorch-pytorch-add-param-to-many-places-and-update-one`). Only
/// `identifier`, not the wider `IDENTIFIER_KINDS`: a null literal never stands where a field or
/// type name would. Python only: the C equivalent (`null` <-> `identifier`) was measured to pair
/// a `return NULL` with an unrelated name in `c-nginx-add-typedef` and buy nothing elsewhere.
const NULL_LITERAL_KINDS: &[&str] = &["none", "identifier"];

/// TypeScript's built-in type keywords - the anonymous leaf tokens tree-sitter yields *inside* a
/// `predefined_type` node (`number`, `string`, `boolean`, ... - tree-sitter names an anonymous
/// token by its own literal text, so the keyword `number` really does have kind `"number"`; see
/// `tree-sitter-typescript`'s grammar rule for `predefined_type`) - paired with `type_identifier`,
/// the leaf that names a class/interface/generic type parameter. The paradigmatic
/// `private value: number` -> `private value: T` edit (introducing a generic) swaps one for the
/// other at exactly this leaf, which a human reads as the same type-annotation slot being edited.
/// Deliberately the *keyword* leaves, not `predefined_type` itself: `predefined_type` is their
/// parent and `type_identifier` is a bare leaf with no children, so pairing the parent instead
/// costs the same as pairing the leaf (both land on `COST_UPDATE` + one `COST_DELETE` for the
/// keyword child) - APTED took the parent-level pairing when tried, which is structurally wrong
/// per the human mapping (it wants `predefined_type` deleted and its keyword child matched to
/// `type_identifier` directly), and regressed `typescript-add-generics` (14 -> 18 mismatches).
/// Restricting the family to the keyword leaves removes that spurious parent-level option.
const TS_TYPE_KEYWORD_KINDS: &[&str] = &[
    "any",
    "number",
    "boolean",
    "string",
    "symbol",
    "unique symbol",
    "void",
    "unknown",
    "never",
    "object",
    "type_identifier",
];

/// Identifier-like node kinds that can match each other across different kinds.
/// These all represent "names" in the code - variables, fields, types, properties - and a human
/// would consider them the same logical entity even if the AST node kind differs.
/// For example, in C: `pwd` (identifier) -> `cb_data.pwd` (field_identifier) should match as
/// the same conceptual "pwd" being referenced, just with different qualification.
///
/// Note: scoped_identifier and scoped_type_identifier are excluded because they represent
/// qualified names (e.g., `std::fs::File`) where the qualification is part of the identity.
const IDENTIFIER_KINDS: &[&str] = &[
    "identifier",
    "field_identifier",
    "type_identifier",
    "property_identifier",
    "shorthand_property_identifier",
    "shorthand_property_identifier_pattern",
];

/// True if `kind` is one of the name-like leaf kinds in [`IDENTIFIER_KINDS`] - the membership test
/// [`kinds_update_allowed`] runs to decide whether two differently-kinded names may still match.
pub fn is_identifier_kind(kind: &str) -> bool {
    IDENTIFIER_KINDS.contains(&kind)
}

/// True if `kind_a` and `kind_b` both appear in the same family in `families`.
fn in_shared_family(kind_a: &str, kind_b: &str, families: &[&[&str]]) -> bool {
    families
        .iter()
        .any(|family| family.contains(&kind_a) && family.contains(&kind_b))
}

/// Every cross-kind-update family above - operator families and [`TS_TYPE_KEYWORD_KINDS`] alike -
/// in one fixed order, so a kind's membership across all of them can be packed into the bits of a
/// single [`FamilyMask`] ([`operator_family_mask`]) and a language's applicable subset into another
/// ([`language_operator_family_mask`]). Order is arbitrary but must stay consistent between those
/// two functions - which is exactly why both derive from *this* list rather than hardcoding bit
/// positions of their own. Widen [`FamilyMask`] (currently `u32`) before adding a family past its
/// bit width - the `const` assertion below refuses to compile otherwise: `u8` silently wrapped (`1u8 << 8` shifts modulo the bit width in release builds)
/// and collided `TS_TYPE_KEYWORD_KINDS` (index 8) onto `COMPARISON_OPS` (index 0) until this was
/// caught by `operator_family_masks_agree_with_string_scanning_kinds_update_allowed`.
///
/// Deliberately built from the same `const` arrays [`kinds_update_allowed`] itself uses, not a
/// hand-transcribed copy: the arrays stay the single source of truth, and the bitmask form is a
/// pure derivation of them, so the two cannot drift apart as families are edited.
const ALL_OPERATOR_FAMILIES: &[&[&str]] = &[
    COMPARISON_OPS,
    ARITHMETIC_OPS,
    PHP_ARITHMETIC_OPS,
    BITWISE_OPS,
    LOGICAL_OPS,
    ASSIGNMENT_OPS,
    INCREMENT_OPS,
    RUST_RANGE_OPS,
    TS_TYPE_KEYWORD_KINDS,
    MEMBER_ACCESS_OPS,
    NUMERIC_LITERAL_KINDS,
    SHELL_TEST_OPS,
    ACCESS_MODIFIERS,
    DECLARATION_KEYWORDS,
    HTML_TAG_END,
    RUBY_HASH_SEPARATORS,
    NULL_LITERAL_KINDS,
];

/// The mask type below must have a bit per family - a silent shift-overflow is exactly the bug
/// the `u8` -> `u16` widening fixed, so the 33rd family fails to compile instead.
const _: () = assert!(ALL_OPERATOR_FAMILIES.len() <= FamilyMask::BITS as usize);

/// Bit-per-family mask type shared by [`operator_family_mask`], [`language_operator_family_mask`]
/// and `KindCostClass::operator_families`.
pub type FamilyMask = u32;

/// Bit `i` set iff `kind` belongs to `ALL_OPERATOR_FAMILIES[i]`. A kind may belong to several
/// (e.g. `+` is in both `ARITHMETIC_OPS` and `PHP_ARITHMETIC_OPS`), which is why this is a mask
/// rather than a single family id.
///
/// Computed once per node at metadata-build time (see `ASTNodeMetadata::kind_cost_class`), turning
/// what used to be a linear scan over every family on every comparison into a bitwise AND - see
/// [`update_allowed_from_masks`].
pub fn operator_family_mask(kind: &str) -> FamilyMask {
    let mut mask: FamilyMask = 0;
    for (i, family) in ALL_OPERATOR_FAMILIES.iter().enumerate() {
        if family.contains(&kind) {
            mask |= 1 << i;
        }
    }
    mask
}

/// Bit `i` set iff `ALL_OPERATOR_FAMILIES[i]` is one of the families `language` recognizes - the
/// bitmask form of [`kinds_update_allowed`]'s own `match language` arm, derived from it by
/// identity comparison on the array pointers so the two can't disagree about which families a
/// language has.
pub fn language_operator_family_mask(language: &Language) -> FamilyMask {
    let mut mask: FamilyMask = 0;
    for family in families_for_language(language) {
        for (i, known) in ALL_OPERATOR_FAMILIES.iter().enumerate() {
            if std::ptr::eq(*family as *const [&str], *known as *const [&str]) {
                mask |= 1 << i;
            }
        }
    }
    mask
}

/// True if two nodes' precomputed kind classes permit a cross-kind update under `language_mask`,
/// i.e. the mask-based equivalent of [`kinds_update_allowed`]'s identifier-family and
/// shared-operator-family checks. Assumes the callers have already handled the same-kind case
/// (which [`kinds_update_allowed`] short-circuits first).
///
/// `(a & b & language) != 0` is exactly `families.iter().any(|f| f.contains(a) && f.contains(b))`
/// restricted to `language`'s families: bit `i` survives the AND iff both kinds are in family `i`
/// *and* `language` recognizes it - no assumption that a kind belongs to at most one family.
pub fn update_allowed_from_masks(
    a: &crate::code::KindCostClass,
    b: &crate::code::KindCostClass,
    language_mask: FamilyMask,
) -> bool {
    if a.identifier_like && b.identifier_like {
        return true;
    }
    (a.operator_families & b.operator_families & language_mask) != 0
}

/**
* Returns true if a node with kind_a is allowed to be matched with node with kind_b
* with an Update operation.
*
* By default (and for any language/kind pair not covered below), nodes of different kinds are
* never allowed to match - see `UnitCostModel::ren`'s doc comment for why. The families below are
* deliberate, hand-picked exceptions: single-token leaves (operators, and since 2026-09-01 also
* member-access tokens, numeric literals, type names, modifiers and declaration keywords - each
* family's own doc comment names the corpus fixture that motivated it) that occupy the same
* syntactic slot in their language's grammar, where a human would consider a kind change (e.g.
* `<` -> `<=`, `.` -> `?.`, `1` -> `1.0`) to be an edit of the same node rather than a wholesale
* replacement.
*
* Additionally, identifier-like kinds (identifier, field_identifier, type_identifier, etc.) are allowed
* to match each other across all languages, since they all represent "names" that a human would
* consider the same logical entity regardless of qualification level.
*/
pub fn kinds_update_allowed(kind_a: &str, kind_b: &str, language: &Language) -> bool {
    if kind_a == kind_b {
        return true;
    }

    // Allow identifier-like kinds to match each other across all languages.
    // This enables matching e.g. identifier "pwd" to field_identifier "pwd" in
    // expressions like `pwd++` -> `cb_data.pwd++`, which is a common pattern
    // in real code changes (see c-nginx-add-typedef optimal solution).
    if is_identifier_kind(kind_a) && is_identifier_kind(kind_b) {
        return true;
    }

    in_shared_family(kind_a, kind_b, families_for_language(language))
}

/// Which cross-kind-update families (operator families, and [`TS_TYPE_KEYWORD_KINDS`])
/// [`kinds_update_allowed`] recognizes for `language` - empty for any language with no hand-picked
/// cross-kind exceptions. Extracted so [`language_operator_family_mask`] derives its bitmask from
/// this same list rather than duplicating the language-to-families mapping.
fn families_for_language(language: &Language) -> &'static [&'static [&'static str]] {
    match language {
        Language::C => &[
            COMPARISON_OPS,
            ARITHMETIC_OPS,
            BITWISE_OPS,
            LOGICAL_OPS,
            ASSIGNMENT_OPS,
            INCREMENT_OPS,
            MEMBER_ACCESS_OPS,
        ],
        Language::CPP => &[
            COMPARISON_OPS,
            ARITHMETIC_OPS,
            BITWISE_OPS,
            LOGICAL_OPS,
            ASSIGNMENT_OPS,
            INCREMENT_OPS,
            MEMBER_ACCESS_OPS,
            ACCESS_MODIFIERS,
        ],
        Language::Java => &[
            COMPARISON_OPS,
            ARITHMETIC_OPS,
            BITWISE_OPS,
            LOGICAL_OPS,
            ASSIGNMENT_OPS,
            INCREMENT_OPS,
            NUMERIC_LITERAL_KINDS,
            ACCESS_MODIFIERS,
        ],
        Language::Go => &[
            COMPARISON_OPS,
            ARITHMETIC_OPS,
            BITWISE_OPS,
            LOGICAL_OPS,
            ASSIGNMENT_OPS,
            INCREMENT_OPS,
            NUMERIC_LITERAL_KINDS,
        ],
        Language::CSharp => &[
            COMPARISON_OPS,
            ARITHMETIC_OPS,
            BITWISE_OPS,
            LOGICAL_OPS,
            ASSIGNMENT_OPS,
            INCREMENT_OPS,
            MEMBER_ACCESS_OPS,
            NUMERIC_LITERAL_KINDS,
        ],
        Language::Rust => &[
            COMPARISON_OPS,
            ARITHMETIC_OPS,
            BITWISE_OPS,
            LOGICAL_OPS,
            ASSIGNMENT_OPS,
            RUST_RANGE_OPS,
            NUMERIC_LITERAL_KINDS,
        ],
        Language::TypeScript | Language::TSX => &[
            COMPARISON_OPS,
            ARITHMETIC_OPS,
            BITWISE_OPS,
            LOGICAL_OPS,
            ASSIGNMENT_OPS,
            INCREMENT_OPS,
            TS_TYPE_KEYWORD_KINDS,
            MEMBER_ACCESS_OPS,
            ACCESS_MODIFIERS,
            DECLARATION_KEYWORDS,
        ],
        Language::JavaScript => &[
            COMPARISON_OPS,
            ARITHMETIC_OPS,
            BITWISE_OPS,
            LOGICAL_OPS,
            ASSIGNMENT_OPS,
            INCREMENT_OPS,
            MEMBER_ACCESS_OPS,
            DECLARATION_KEYWORDS,
        ],
        Language::Python => &[
            COMPARISON_OPS,
            ARITHMETIC_OPS,
            LOGICAL_OPS,
            ASSIGNMENT_OPS,
            NUMERIC_LITERAL_KINDS,
            NULL_LITERAL_KINDS,
        ],
        Language::Kotlin => &[
            COMPARISON_OPS,
            LOGICAL_OPS,
            ASSIGNMENT_OPS,
            MEMBER_ACCESS_OPS,
            NUMERIC_LITERAL_KINDS,
            ACCESS_MODIFIERS,
            DECLARATION_KEYWORDS,
        ],
        Language::PHP => &[
            COMPARISON_OPS,
            PHP_ARITHMETIC_OPS,
            BITWISE_OPS,
            LOGICAL_OPS,
            ASSIGNMENT_OPS,
            NUMERIC_LITERAL_KINDS,
            ACCESS_MODIFIERS,
        ],
        Language::Ruby => &[
            COMPARISON_OPS,
            ARITHMETIC_OPS,
            BITWISE_OPS,
            LOGICAL_OPS,
            ASSIGNMENT_OPS,
            MEMBER_ACCESS_OPS,
            NUMERIC_LITERAL_KINDS,
            RUBY_HASH_SEPARATORS,
        ],
        Language::Swift => &[
            COMPARISON_OPS,
            ARITHMETIC_OPS,
            BITWISE_OPS,
            LOGICAL_OPS,
            ASSIGNMENT_OPS,
            MEMBER_ACCESS_OPS,
            NUMERIC_LITERAL_KINDS,
            ACCESS_MODIFIERS,
            DECLARATION_KEYWORDS,
        ],
        Language::Scala => &[
            COMPARISON_OPS,
            ARITHMETIC_OPS,
            LOGICAL_OPS,
            ASSIGNMENT_OPS,
            NUMERIC_LITERAL_KINDS,
            ACCESS_MODIFIERS,
            DECLARATION_KEYWORDS,
        ],
        Language::ShellScript => &[SHELL_TEST_OPS],
        Language::HTML | Language::XML => &[HTML_TAG_END],
        Language::CSS => &[NUMERIC_LITERAL_KINDS],
        _ => &[],
    }
}

/// Generic structural punctuation: bracket/separator tokens that exist purely as grammar glue and
/// carry no content of their own, in every language this project supports. Deliberately a flat,
/// language-agnostic list (unlike the operator families above) since these symbols play the same
/// "structural glue" role in effectively every grammar - there's no language where `(` means
/// something other than "start of a grouped/parenthesized thing".
const GENERIC_PUNCTUATION: &[&str] = &["(", ")", "{", "}", "[", "]", ";", ",", ":", "::", "."];

/// Literal-value leaf kinds (string, number, boolean, ...), shared by every consumer that needs
/// to distinguish "this leaf's identity is its value" (a literal) from "this leaf's identity is
/// its name" (an identifier, see `IDENTIFIER_KINDS`) - e.g. the APTED rename-cost model and the
/// multi-level normalized hashing in `code::hash`. Kept as a single list so both stay in sync.
const LITERAL_KINDS: &[&str] = &[
    "string_literal",
    "number_literal",
    "integer_literal",
    "float_literal",
    "boolean_literal",
    "char_literal",
    "regex_literal",
    "template_literal",
];

/// True if `kind` is a literal-value node kind (string, number, boolean, etc.).
pub fn is_literal_kind(kind: &str) -> bool {
    LITERAL_KINDS.contains(&kind)
}

/// True if `kind` denotes a generic punctuation/operator token - a single TreeSitter leaf that
/// exists as grammar glue (`<`, `<=`, `(`, `{`, `::`, ...) rather than content a human would
/// recognize as meaningful on its own. Used by `matching_allowed` to decide which matches need
/// "small context" support beyond a bare kind check: unlike an identifier or literal, whose own
/// text already carries evidence of a real correspondence, two `)` tokens (or a `<`/`<=` pair) are
/// identical/compatible essentially everywhere in a file, so kind-compatibility alone is never
/// enough to justify matching them.
///
/// Backed by the same operator-family lists `kinds_update_allowed` uses (plus `GENERIC_PUNCTUATION`
/// for brackets/separators) rather than a generic "is it all symbols" heuristic, since several
/// families include keyword-spelled operators (`and`, `bitand`, `not_eq`, ...) that a purely
/// symbolic check would miss.
pub fn is_generic_token_kind(kind: &str) -> bool {
    GENERIC_PUNCTUATION.contains(&kind)
        || [
            COMPARISON_OPS,
            ARITHMETIC_OPS,
            PHP_ARITHMETIC_OPS,
            BITWISE_OPS,
            LOGICAL_OPS,
            ASSIGNMENT_OPS,
            INCREMENT_OPS,
            RUST_RANGE_OPS,
        ]
        .iter()
        .any(|family| family.contains(&kind))
}

/**
* Returns true if the node kind represents a comment in any supported language.
*
* TreeSitter grammars use various node kinds for comments:
* - `comment` (generic, used by many languages)
* - `line_comment` (Java, Rust, Kotlin, etc.)
* - `block_comment` (Java, Rust, Kotlin, etc.)
* - `js_comment` (JavaScript/TypeScript specific)
*
* This function provides a unified way to check if a node is a comment across all languages.
*/
pub fn is_comment(kind: &str) -> bool {
    matches!(
        kind,
        "comment"
            | "line_comment"
            | "block_comment"
            | "js_comment"
            | "html_comment"
            | "xml_comment"
            | "css_comment"
            | "c_comment"
            | "cpp_comment"
    )
}

/**
* Returns true if `kind` is an attribute/decorator/annotation node that sits as an actual
* *sibling* of the declaration it modifies, in `language`'s grammar - as opposed to being nested
* *inside* that declaration's own subtree (a `modifiers`/similar wrapper child), which is how most
* grammars actually model this and needs no special handling at all: it's already covered for
* free the moment the declaration itself matches.
*
* Verified per-language by parsing a small sample and inspecting the resulting tree (not just
* going by the grammar's node-kind name, which alone doesn't tell you sibling vs. child) - see the
* 2026-08 "digging into #1" investigation this responds to:
*   - **Sibling** (this function returns `true`): Rust `attribute_item` (a direct sibling of the
*     item it precedes, at any level - `#[derive(...)] struct Foo` and a top-level `#[cfg(test)]
*     mod tests` alike), Python `decorator` (sibling of the `function_definition`/`class_definition`
*     it precedes, both children of a wrapping `decorated_definition`), TypeScript/TSX `decorator`
*     (sibling of the `method_definition`/... it precedes, inside `class_body`).
*   - **Child, not sibling** (deliberately excluded - confirmed, not assumed): Java
*     `marker_annotation`/`annotation` and Kotlin `annotation` (both nested inside a `modifiers`
*     child of the method/class they annotate), Scala `annotation` (direct child of the
*     `function_definition`), PHP and C# `attribute_list` (direct child of the declaration), Swift
*     `attribute` (nested inside a `modifiers` child) - and, easy to get wrong by assuming it
*     matches TypeScript, plain JavaScript's own `decorator` (nested inside `method_definition`,
*     *not* a sibling the way TypeScript's is - the two grammars model this differently despite
*     sharing the node-kind name).
*
* Only the sibling case benefits from `solve_leading_siblings`'s "walk backward from an
* already-matched node" mechanism; including a child-only kind here would just never fire (its
* node is never any other node's `prev_sibling`), which is harmless but misleading about what this
* function actually does.
*/
pub fn is_leading_modifier(kind: &str, language: &Language) -> bool {
    match language {
        Language::Rust => kind == "attribute_item",
        Language::Python => kind == "decorator",
        Language::TypeScript | Language::TSX => kind == "decorator",
        _ => false,
    }
}

/// Character-bigram Dice similarity threshold for `leaf_texts_similar`. 0.6 keeps clear renames
/// (`fetch_user` -> `fetch_user_data`, `user_id` -> `userId`) while rejecting unrelated
/// identifiers that share only a stray character pair.
const LEAF_TEXT_SIMILARITY_THRESHOLD: f64 = 0.6;

/**
* True if two leaf texts are similar enough that a human would read the pair as "the same token,
* renamed/tweaked" rather than two unrelated tokens.
*
* Uses character-bigram Dice similarity: cheap, symmetric, no allocation beyond two small sets,
* and robust to affix changes (`foo` -> `foo_bar` scores well). Texts too short to have bigrams
* (length <= 1, or length 2 with no overlap) effectively always fail - deliberately so: `i` -> `j`
* or `0` -> `1` carry no textual evidence on their own, and the caller's other arm (a nearby
* matched ancestor, i.e. same-slot context) is the correct way for those legitimate small renames
* to survive.
*/
pub fn leaf_texts_similar(text_a: &str, text_b: &str) -> bool {
    if text_a == text_b {
        return true;
    }
    let bigrams = |s: &str| -> std::collections::HashSet<(char, char)> {
        s.chars().zip(s.chars().skip(1)).collect()
    };
    let a = bigrams(text_a);
    let b = bigrams(text_b);
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let common = a.intersection(&b).count();
    2.0 * common as f64 / (a.len() + b.len()) as f64 >= LEAF_TEXT_SIMILARITY_THRESHOLD
}

/**
* Generalizes `kinds_update_allowed` with a "small context" requirement for generic tokens.
*
* Delegates the kind-compatibility question to `kinds_update_allowed` first. If that allows the
* pair, and *neither* kind is a generic token (see `is_generic_token_kind`) - e.g. two
* `identifier`s, or two `return_statement`s - the kind check alone is enough, exactly as before.
*
* But if either kind is a generic token, kind-compatibility is necessary and not sufficient:
* additionally requires `parents_matched()` to hold, i.e. the two nodes' immediate enclosing
* nodes must themselves already correspond. Without this, the tree-edit-distance search is free to
* match any lone `<` (or unrelated `<`/`<=` pair) between two statements that have nothing else in
* common, purely because reusing a leaf is cheaper than deleting one and inserting the other -
* exactly the "surprising cheap match" a human wouldn't read as the same token edited in place.
*
* `parents_matched` is a callback rather than a plain bool so callers that already know the answer
* is irrelevant (neither kind is a generic token) never pay for computing it.
*/
pub fn matching_allowed(
    kind_a: &str,
    kind_b: &str,
    language: &Language,
    parents_matched: impl FnOnce() -> bool,
) -> bool {
    if !kinds_update_allowed(kind_a, kind_b, language) {
        return false;
    }
    if !is_generic_token_kind(kind_a) && !is_generic_token_kind(kind_b) {
        return true;
    }
    parents_matched()
}

/// A flow-control construct family. Originally existed to keep the since-deleted
/// `solve_similar_flow_control` (`MatchSimilarFlowControl`) from ever pairing a `match` against a
/// `switch`, etc.; its only remaining consumer is [`flow_control_family`], used by
/// [`is_block_container`] to recognize `if`/`match`/`switch` constructs as anonymous-container
/// candidates for `solve_greedy_anchor_blocks`. `Hash` (alongside `Eq`) is a holdover from once
/// serving as `grouped_greedy_matcher`'s compatibility key - harmless to keep, not required by the
/// current use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlowControlFamily {
    Match,
    Switch,
    If,
}

/// Returns which [`FlowControlFamily`] `node_kind` belongs to for `language`, if any.
///
/// `if`/`else if` chains are supported too, but only for languages where `alternative` recurses
/// into either a bare block or another `if` (Python's flat `elif_clause`/`else_clause` fields on a
/// single `if_statement` don't fit that shape, so Python `if` isn't covered here - only its
/// `match_statement` is).
pub fn flow_control_family(node_kind: &str, language: &Language) -> Option<FlowControlFamily> {
    match (language, node_kind) {
        (Language::Rust, "match_expression") => Some(FlowControlFamily::Match),
        (Language::Rust, "if_expression") => Some(FlowControlFamily::If),
        (Language::Python, "match_statement") => Some(FlowControlFamily::Match),
        (
            Language::C
            | Language::CPP
            | Language::JavaScript
            | Language::TypeScript
            | Language::TSX
            | Language::CSharp,
            "switch_statement",
        ) => Some(FlowControlFamily::Switch),
        (Language::Go, "expression_switch_statement") => Some(FlowControlFamily::Switch),
        (
            Language::C
            | Language::CPP
            | Language::Java
            | Language::Go
            | Language::JavaScript
            | Language::TypeScript
            | Language::TSX
            | Language::CSharp,
            "if_statement",
        ) => Some(FlowControlFamily::If),
        _ => None,
    }
}

/// True if `node_kind` is a statement-sequence container ("a block") for `language`: its direct
/// children are an ordered sequence of statements/expressions - exactly the shape
/// `solve_greedy_anchor_blocks::sequence_edit_cost` treats each child as an opaque token of.
/// Deliberately narrow (not "any node with several children"): an early version of that pass
/// considered every such node a candidate and regressed 9 `optimal_solutions` fixtures by
/// occasionally anchoring two unrelated `call_expression`/`binary_expression` nodes whose
/// `argument_list`/operator happened to hash-match by coincidence - restricting candidates to
/// genuine statement containers (plus the flow-control constructs themselves via
/// [`flow_control_family`], since a whole `if`/`match`/`switch` is exactly the kind of anonymous
/// "block" a name- or arm-based heuristic could still miss) keeps that cheap, position-blind cost
/// estimate from firing on expression-level coincidences.
pub fn is_block_container(node_kind: &str, language: &Language) -> bool {
    if flow_control_family(node_kind, language).is_some() {
        return true;
    }
    matches!(
        (language, node_kind),
        (Language::Rust, "block")
            | (Language::Python, "block")
            | (Language::C | Language::CPP, "compound_statement")
            | (
                Language::Java | Language::Go | Language::CSharp | Language::Kotlin,
                "block"
            )
            | (
                Language::JavaScript | Language::TypeScript | Language::TSX,
                "statement_block"
            )
    )
}

/// Jaccard similarity (shared entries / all distinct entries across both sides) of two precomputed
/// string sets - generic set-overlap scoring, originally written for comparing flow-control arm
/// signatures (the name and doc comment predate that caller's 2026-08-14 deletion; the name stuck
/// since `solve_import_list_overlap` reuses it unchanged for import-symbol-set overlap).
///
/// Returns 0.0 if either side is empty (nothing meaningful to compare), so two empty sets never
/// spuriously "match" each other.
pub fn flow_control_similarity_of_sets(
    before_set: &std::collections::HashSet<&str>,
    after_set: &std::collections::HashSet<&str>,
) -> f64 {
    if before_set.is_empty() || after_set.is_empty() {
        return 0.0;
    }
    let intersection = before_set.intersection(after_set).count();
    let union = before_set.union(after_set).count();
    intersection as f64 / union as f64
}

/// Substrings that mark a call/macro as "meant for the programmer" (logging, error bailouts,
/// assertions, debug prints) rather than output meant for the end user. Matched against the
/// *lowercased last segment* of the callee path (e.g. `log::error!` -> "error",
/// `logger.WarnF` -> "warnf", `self.logger.debug` -> "debug"), so e.g. Go's `Errorf`/`Warnf`/
/// `Fatalln` or Java/C#'s `LogError`/`LogWarning` all match via substring containment without
/// needing one entry per per-language spelling convention.
const DIAGNOSTIC_CALLEE_KEYWORDS: &[&str] = &[
    "printf",
    "fprintf",
    "sprintf",
    "eprintln",
    "eprint",
    "panic",
    "bail",
    "unreachable",
    "todo",
    "unimplemented",
    "assert",
    "log",
    "error",
    "err",
    "warn",
    "warning",
    "info",
    "debug",
    "trace",
    "fatal",
    "critical",
    "die",
];

/// Whether `node_kind` is a call-like node this pass should even consider - i.e. worth extracting
/// a callee name from. Deliberately per-language, since "a function call" is a different node
/// kind (and a different callee field name) in every grammar.
fn is_call_like(node_kind: &str, language: &Language) -> bool {
    matches!(
        (language, node_kind),
        (Language::Rust, "call_expression" | "macro_invocation")
            | (
                Language::C | Language::CPP | Language::Go,
                "call_expression"
            )
            | (
                Language::JavaScript | Language::TypeScript | Language::TSX,
                "call_expression"
            )
            | (Language::Java, "method_invocation")
            | (Language::CSharp, "invocation_expression")
            | (Language::Python, "call")
    )
}

/// Extracts the callee (function/macro path) text of a call-like node, e.g. `log::error` out of
/// `log::error!(...)`, or `logger.warn` out of `logger.warn(...)`.
fn callee_text<'a>(node: Node, language: &Language, source: &'a [u8]) -> Option<&'a str> {
    let field_name = match (language, node.kind()) {
        (Language::Rust, "macro_invocation") => "macro",
        (Language::Java, "method_invocation") => "name",
        _ => "function",
    };
    node.child_by_field_name(field_name)?.utf8_text(source).ok()
}

/// Whether `node` carries text of its own that a reader can actually see, rather than being pure
/// structure whose every readable byte belongs to some descendant.
///
/// True for a leaf (its text is its own by definition), and for an interior node with non-
/// whitespace content in the gaps its children don't cover - a `line_comment` whose `//` marker is
/// a separate child, say, leaving the comment's actual words on the parent. False for a `block`,
/// `argument_list` or `declaration_list`, whose entire visible content is its children's.
///
/// **A pure function of the AST and the source bytes - deliberately not of any diff.** An earlier
/// version of this idea derived visibility from the renderer instead (does `diff::text::ranges`
/// emit a span for this node), which made it depend on the mapping: the same `block` is one
/// `Identical` span inside an unchanged function and is descended into inside a changed one. That
/// is a fine description of what got drawn, but it is unusable as a measurement, because both the
/// numerator and the denominator of any rate built on it move when the algorithm changes - a diff
/// that renders coarsely has almost nothing "visible" and so almost nothing it can get visibly
/// wrong. Measured on the corpus at the time: `css-shadcn-ui-ui-completely-broken-treesitter-
/// parsing` collapsed 32,682 nodes into 2 rendered spans and thereby scored 0 visible mismatches
/// while holding 124 real ones. Structural visibility cannot be gamed that way: the set is fixed
/// by the input alone.
///
/// Non-ASCII bytes count as content (they are not ASCII whitespace), which biases toward calling a
/// node visible - the safe direction for a metric that exists to *find* mistakes.
pub fn is_structurally_visible(node: Node, source: &[u8]) -> bool {
    if node.child_count() == 0 {
        return true;
    }
    let has_content =
        |range: std::ops::Range<usize>| !source[range].iter().all(u8::is_ascii_whitespace);

    let mut pos = node.start_byte();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.start_byte() > pos && has_content(pos..child.start_byte()) {
            return true;
        }
        pos = pos.max(child.end_byte());
    }
    node.end_byte() > pos && has_content(pos..node.end_byte())
}

/// Every node id in `code` for which [`is_structurally_visible`] holds. Depends only on `code`, so
/// two different diffs of the same file always agree on it - see that function's doc comment for
/// why that property is the whole point.
pub fn structurally_visible_node_ids(code: &Code) -> std::collections::HashSet<usize> {
    let mut visible = std::collections::HashSet::new();
    let Some(ast) = code.ast.as_ref() else {
        return visible;
    };
    let source = code.contents.as_bytes();
    let mut stack = vec![ast.root_node()];
    while let Some(node) = stack.pop() {
        if is_structurally_visible(node, source) {
            visible.insert(node.id());
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    visible
}

/// Whether `node` is a call/macro invocation whose callee looks like it's meant for the
/// programmer (logging, `bail!`/`panic!`, assertions, debug `printf`s) rather than the end user.
/// This is intentionally a loose, substring-based heuristic - see [`DIAGNOSTIC_CALLEE_KEYWORDS`] -
/// since the pass that uses it only ever pairs nodes whose *entire subtree hash* is identical, so
/// an over-eager match here is harmless: it just means two byte-for-byte identical statements get
/// matched, which is a reasonable outcome regardless of why they were flagged as candidates.
pub fn is_diagnostic_statement(node: Node, language: &Language, source: &[u8]) -> bool {
    if !is_call_like(node.kind(), language) {
        return false;
    }
    let Some(callee) = callee_text(node, language, source) else {
        return false;
    };
    let last_segment = callee
        .rsplit(|c: char| !(c.is_alphanumeric() || c == '_'))
        .find(|segment| !segment.is_empty())
        .unwrap_or(callee)
        .to_lowercase();
    DIAGNOSTIC_CALLEE_KEYWORDS
        .iter()
        .any(|keyword| last_segment.contains(keyword))
}

/// Recognizes the node kind that directly holds a function/method/class/namespace's own ordered
/// sequence of statements or members - `compound_statement` (C/C++), `body_statement` (Ruby),
/// `function_body`/`class_body` (Kotlin), `block` (Rust), `declaration_list` (C++ namespaces).
/// Language-agnostic by design: every one of these strings is unambiguous on its own (no two
/// supported grammars reuse the same kind name for something else), so - unlike most classifiers
/// in this file - there is no need to also gate on `Language`.
///
/// Used to find the right anchor for [`crate::diff::apted::prematch_identical_statement_siblings`]
/// - deliberately a kind allow-list, not "whichever descendant happens to have the most direct
///   children" (`ASTMetadata::node_to_widest_subtree_node`, which `solve_large_flat_subtrees` uses):
///   confirmed live that the latter can pick an unrelated, wider, but semantically irrelevant sibling
///   instead - a Rust function containing a macro call whose `token_tree` has more raw tokens than
///   the function's own `block` has statements gets the `token_tree` instead, missing the actual
///   statement sequence entirely (measured on `rust-tauri-cli-ios-dev`: picked a 26-token `token_tree`
///   over the 21-statement `block` sitting right next to it, so the pre-match found nothing worth
///   matching there at all).
///
/// Not exhaustive - only the kinds this session's own measurements confirmed against real
/// fixtures (`TODO.md`, 2026-08-05). Safe to extend as more languages show the same pattern: this
/// is a pure performance pre-pass (see that function's doc comment for why a missing or wrong
/// entry here only costs a missed optimization, never a wrong answer), so a narrow list is a
/// reasonable starting point, not a correctness risk.
pub fn is_statement_sequence_body(node_kind: &str) -> bool {
    matches!(
        node_kind,
        "compound_statement"
            | "body_statement"
            | "function_body"
            | "class_body"
            | "block"
            | "declaration_list"
    )
}

/// Returns true if the given node kind represents a container whose children are order-independent.
/// For these containers, the order of children doesn't affect the semantic meaning.
///
/// Examples: struct/record fields, enum variants, import statements.
/// These are the nodes where reordering children should NOT be considered a semantic change.
///
/// Note: This is intentionally conservative. We only mark containers that are definitively
/// order-independent according to the language semantics (not just "often reordered" by formatters).
///
/// KNOWN ISSUE, fixed 2026-07-29 (originally logged 2026-07-15, JSON/YAML fixed 2026-07-23):
/// every kind string below is now verified against ground truth, not just each grammar's
/// node-types.json (which can omit or rename aliased node kinds) but the actual node kinds a real
/// `tree_sitter::Parser` reports for a representative snippet in each language (see git history
/// for the throwaway `examples/grammar_check.rs` used to confirm these). Before this pass, most of
/// this function was near-dead weight: `Go` checked `field_list` (real name:
/// `field_declaration_list`); `Python` checked `pair_list` (real container: `dictionary`, whose
/// children are `pair` nodes); `JS`/`TS`/`TSX` also checked `pair_list` (real container: `object`);
/// `Java` and `CSharp` shared one arm checking `enum_constants` (doesn't exist for either - and the
/// real containers differ per language: `enum_body` for Java, `enum_member_declaration_list` for
/// C#, so they can no longer share an arm); `Swift` checked `enum_member_list` (real name:
/// `enum_class_body`); `Scala` checked `import_expr_list` (real name: `namespace_selectors` - and
/// only the braced `import a.b.{X, Y, Z}` selector list is wrapped in a node at all; the top-level
/// comma-separated import list itself has no wrapper). `Rust` was the only originally-correct arm,
/// and was still missing `field_declaration_list` (struct fields - same node kind name as Go's).
///
/// `Kotlin` has no fix available: imports are direct repeated children of `source_file`
/// (interleaved with the package header and top-level statements in the grammar), never wrapped in
/// any list/container node at all - confirmed via `tree-sitter-kotlin-ng`'s `grammar.js`. There is
/// no string that could make this arm correct, so it's left `false` rather than guessing.
///
/// The 2026-07-15 version of this comment additionally claimed that even corrected strings would
/// have no effect, because `compute_commutative_structural_hash` (a separate, bolted-on third
/// hash) only applied commutative sorting to the container node itself, not to its ancestors, and
/// `hash_tree_matching`'s descendant-pairing wasn't commutative-aware either. Both of those are
/// now stale: the 2026-07-17/18 pipeline rework replaced that separate hash with `is_commutative_
/// container` support folded directly into `compute_kind_and_value_hash`/`compute_kind_only_hash`
/// at every recursion level (`code::hash`), and `pair_children_for_descent`
/// (`hash_tree_matching.rs`) now checks `is_commutative_container` itself when pairing children.
/// JSON's fix (confirmed against a real 3,075-node case: a single deleted key in a ~140-key
/// localization JSON object was landing 100% of its mapping on the expensive `APTED` fallback
/// before that fix, vs. instantly beforehand) is the model for what fixing the rest should do too,
/// now that the plumbing actually respects this function's answer.
pub fn is_commutative_container(node_kind: &str, language: &Language) -> bool {
    match language {
        Language::Rust => {
            // Enum variants are order-independent for matching purposes
            node_kind == "enum_variant_list"
                // Use tree items can be reordered
                || node_kind == "use_list"
                // Struct/union field declarations can be reordered
                || node_kind == "field_declaration_list"
        }
        Language::Go => {
            // Struct field list - fields can be reordered
            node_kind == "field_declaration_list"
                // Import spec list - imports can be reordered
                || node_kind == "import_spec_list"
        }
        Language::Python => {
            // Dictionary - key/value pairs can be reordered
            node_kind == "dictionary"
        }
        Language::Java => {
            // Enum body - enum constants can be reordered
            node_kind == "enum_body"
        }
        Language::CSharp => {
            // Enum member declaration list - enum constants can be reordered
            node_kind == "enum_member_declaration_list"
        }
        Language::C | Language::CPP => {
            // Enum specifiers - enumerators can be reordered
            node_kind == "enumerator_list"
        }
        Language::JavaScript | Language::TypeScript | Language::TSX => {
            // Object - properties can be reordered
            node_kind == "object"
        }
        // Imports aren't wrapped in any container node in this grammar at all - see this
        // function's doc comment. Nothing to match; always false.
        Language::Kotlin => false,
        Language::Scala => {
            // Braced import selector list (`import a.b.{X, Y, Z}`) - selectors can be reordered.
            // The plain, unbraced multi-import form has no wrapper node to match here.
            node_kind == "namespace_selectors"
        }
        Language::Swift => {
            // Enum class body - enum cases can be reordered
            node_kind == "enum_class_body"
        }
        // JSON, YAML - object/mapping keys are commutative. Verified directly against
        // tree-sitter-json/tree-sitter-yaml's actual parse trees (2026-07-23) - the previous
        // strings here ("pair_list", "mapping_content") don't exist in either grammar at all (see
        // this function's doc comment), so this arm was pure dead code: `is_commutative_container`
        // always returned `false` for JSON/YAML, meaning `pair_children_for_descent`
        // (`hash_tree_matching.rs`) always took the plain positional-zip path for every JSON object
        // and YAML mapping. A single inserted/deleted key anywhere in a large flat object (e.g. one
        // new string added to a localization file) then desyncs every subsequent key's position,
        // orphaning the whole rest of the object onto the expensive `final_apted` fallback - this is
        // confirmed to be the exact mechanism behind a real observed case (jellyfin-jellyfin's
        // `cs.json`, one deleted key out of ~140: 1.2s and 100% `APTED`-attributed mappings for a
        // 3,075-combined-node file before this fix).
        Language::JSON => node_kind == "object",
        // YAML has two mapping shapes: `block_mapping` (the common indented `key: value` form) and
        // `flow_mapping` (the JSON-style inline `{key: value}` form) - both are order-independent.
        Language::YAML => node_kind == "block_mapping" || node_kind == "flow_mapping",
        // Default: no commutative containers
        _ => false,
    }
}

#[cfg(test)]
mod tests;
