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
/// arguments). Shared shape behind `solve_similar_flow_control`'s
/// `collect_unmatched_containers` and `solve_identical_diagnostic_statements`'s
/// `collect_unmatched_diagnostic_statements`, which differed only in their predicate.
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
        Language::JSON | Language::YAML | Language::XML => {
            node_kind == "document" || node_kind == "fragment"
        }
        // Other languages - root node only as fallback
        _ => false,
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
            // `solve_named_reference_groups`'s fully-recursive, finer-grained walk - the two
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
        // a *name* from them, so `solve_named_reference_groups`/`solve_large_flat_subtrees` (both
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
            "function_definition" => node
                .child_by_field_name("declarator")
                .and_then(|d| c_family_declarator_name(d, bytes))
                .map(|name| (node_kind.to_string(), name.to_string())),
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
        // resolved scope chain (`solve_named_reference_groups`'s doc comment), so two pairs only
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
/// never correctness - `solve_named_reference_groups` still runs real APTED on whatever it groups.
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
];

/// True if `kind_a` and `kind_b` both appear in the same family in `families`.
fn in_shared_family(kind_a: &str, kind_b: &str, families: &[&[&str]]) -> bool {
    families
        .iter()
        .any(|family| family.contains(&kind_a) && family.contains(&kind_b))
}

/**
* Returns true if a node with kind_a is allowed to be matched with node with kind_b
* with an Update operation.
*
* By default (and for any language/kind pair not covered below), nodes of different kinds are
* never allowed to match - see `UnitCostModel::ren`'s doc comment for why. The families below are
* deliberate, hand-picked exceptions: single-token operator leaves that occupy the same syntactic
* slot in their language's grammar, where a human would consider a kind change (e.g. `<` -> `<=`)
* to be an edit of the same node rather than a wholesale replacement.
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
    if IDENTIFIER_KINDS.contains(&kind_a) && IDENTIFIER_KINDS.contains(&kind_b) {
        return true;
    }

    let families: &[&[&str]] = match language {
        Language::C | Language::Java | Language::Go | Language::CSharp => &[
            COMPARISON_OPS,
            ARITHMETIC_OPS,
            BITWISE_OPS,
            LOGICAL_OPS,
            ASSIGNMENT_OPS,
            INCREMENT_OPS,
        ],
        Language::CPP => &[
            COMPARISON_OPS,
            ARITHMETIC_OPS,
            BITWISE_OPS,
            LOGICAL_OPS,
            ASSIGNMENT_OPS,
            INCREMENT_OPS,
        ],
        Language::Rust => &[
            COMPARISON_OPS,
            ARITHMETIC_OPS,
            BITWISE_OPS,
            LOGICAL_OPS,
            ASSIGNMENT_OPS,
            RUST_RANGE_OPS,
        ],
        Language::JavaScript | Language::TypeScript | Language::TSX => &[
            COMPARISON_OPS,
            ARITHMETIC_OPS,
            BITWISE_OPS,
            LOGICAL_OPS,
            ASSIGNMENT_OPS,
            INCREMENT_OPS,
        ],
        Language::Python => &[COMPARISON_OPS, ARITHMETIC_OPS, LOGICAL_OPS, ASSIGNMENT_OPS],
        Language::Kotlin => &[COMPARISON_OPS, LOGICAL_OPS, ASSIGNMENT_OPS],
        Language::PHP => &[
            COMPARISON_OPS,
            PHP_ARITHMETIC_OPS,
            BITWISE_OPS,
            LOGICAL_OPS,
            ASSIGNMENT_OPS,
        ],
        Language::Ruby => &[
            COMPARISON_OPS,
            ARITHMETIC_OPS,
            BITWISE_OPS,
            LOGICAL_OPS,
            ASSIGNMENT_OPS,
        ],
        _ => return false,
    };

    in_shared_family(kind_a, kind_b, families)
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

/// A flow-control construct family, used to keep `MatchSimilarFlowControl` from ever pairing a
/// `match` against a `switch`, etc. `Hash` (alongside `Eq`) so it can serve directly as
/// `grouped_greedy_matcher`'s compatibility key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlowControlFamily {
    Match,
    Switch,
    If,
}

/// One arm/case of a flow-control container, together with its normalized discriminant text (the
/// match pattern or case label), used by `MatchSimilarFlowControl` to score how similar two
/// containers are.
///
/// `signature` is `None` for a wildcard/default arm (Rust/Python `_`, a bare `default:`): matching
/// those across two constructs is trivial and would inflate the similarity score without telling
/// us anything real, so they're excluded from scoring (see `flow_control_similarity`).
#[derive(Debug, Clone)]
pub struct FlowControlArm {
    pub node_id: usize,
    pub signature: Option<String>,
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

/// Extracts the byte range of `container`'s discriminant, excluding a trailing guard/`when`
/// clause if the grammar attaches one directly to the pattern node under a `condition` field
/// (Rust's `match_pattern` does this for `pattern if guard`; Python's `case_pattern` doesn't need
/// it since the guard is a sibling field on `case_clause` instead, so this is a no-op there).
fn signature_text(container: Node, source: &[u8]) -> Option<String> {
    let start = container.start_byte();
    let guard = container.child_by_field_name("condition");
    let end = guard
        .map(|g| g.start_byte())
        .unwrap_or_else(|| container.end_byte());
    if end <= start {
        return None;
    }
    let text = std::str::from_utf8(&source[start..end]).ok()?.trim();
    // When a guard is present, the slice still includes the `if`/`when` keyword introducing it
    // (the keyword itself isn't part of the `condition` field) - strip it back off.
    let text = if guard.is_some() {
        text.strip_suffix("if")
            .or_else(|| text.strip_suffix("when"))
            .map(str::trim_end)
            .unwrap_or(text)
    } else {
        text
    };
    if text.is_empty() || text == "_" {
        None
    } else {
        Some(text.to_string())
    }
}

/// Extracts the arm/case list for a recognized flow-control container node (see
/// [`flow_control_family`]). Returns `None` if `node`'s kind isn't a recognized container.
pub fn flow_control_arms(
    node: Node,
    language: &Language,
    source: &[u8],
) -> Option<Vec<FlowControlArm>> {
    match flow_control_family(node.kind(), language)? {
        FlowControlFamily::Match => match_arms(node, language, source),
        FlowControlFamily::Switch => switch_arms(node, language, source),
        FlowControlFamily::If => if_chain_arms(node, language, source),
    }
}

fn match_arms(node: Node, language: &Language, source: &[u8]) -> Option<Vec<FlowControlArm>> {
    match language {
        Language::Rust => {
            let body = node.child_by_field_name("body")?; // match_block
            let mut cursor = body.walk();
            Some(
                body.children(&mut cursor)
                    .filter(|c| c.kind() == "match_arm")
                    .map(|arm| FlowControlArm {
                        node_id: arm.id(),
                        signature: arm
                            .child_by_field_name("pattern")
                            .and_then(|pattern| signature_text(pattern, source)),
                    })
                    .collect(),
            )
        }
        Language::Python => {
            let body = node.child_by_field_name("body")?; // block
            let mut cursor = body.walk();
            Some(
                body.children(&mut cursor)
                    .filter(|c| c.kind() == "case_clause")
                    .map(|arm| FlowControlArm {
                        node_id: arm.id(),
                        // `case_pattern` is `case_clause`'s sole unnamed child, always the first
                        // one (the optional `guard`/`consequence` fields always follow it).
                        signature: arm
                            .named_child(0)
                            .and_then(|pattern| signature_text(pattern, source)),
                    })
                    .collect(),
            )
        }
        _ => None,
    }
}

fn switch_arms(node: Node, language: &Language, source: &[u8]) -> Option<Vec<FlowControlArm>> {
    let arm = |n: Node, value_field: &str| FlowControlArm {
        node_id: n.id(),
        signature: n
            .child_by_field_name(value_field)
            .and_then(|v| signature_text(v, source)),
    };
    match language {
        Language::C | Language::CPP => {
            let body = node.child_by_field_name("body")?; // compound_statement
            let mut cursor = body.walk();
            Some(
                body.children(&mut cursor)
                    .filter(|c| c.kind() == "case_statement")
                    .map(|n| arm(n, "value"))
                    .collect(),
            )
        }
        Language::JavaScript | Language::TypeScript | Language::TSX => {
            let body = node.child_by_field_name("body")?; // switch_body
            let mut cursor = body.walk();
            Some(
                body.children(&mut cursor)
                    .filter(|c| c.kind() == "switch_case" || c.kind() == "switch_default")
                    .map(|n| arm(n, "value"))
                    .collect(),
            )
        }
        Language::CSharp => {
            let body = node.child_by_field_name("body")?; // switch_body
            let mut cursor = body.walk();
            Some(
                body.children(&mut cursor)
                    .filter(|c| c.kind() == "switch_section")
                    .map(|n| FlowControlArm {
                        node_id: n.id(),
                        signature: n
                            .child_by_field_name("expression")
                            .or_else(|| n.child_by_field_name("pattern"))
                            .and_then(|v| signature_text(v, source)),
                    })
                    .collect(),
            )
        }
        Language::Go => {
            // No `body` field: `expression_case`/`default_case` are direct children.
            let mut cursor = node.walk();
            Some(
                node.children(&mut cursor)
                    .filter(|c| c.kind() == "expression_case" || c.kind() == "default_case")
                    .map(|n| arm(n, "value"))
                    .collect(),
            )
        }
        _ => None,
    }
}

/// Walks an `if`/`else if`/`else` chain starting at `node` (which must already be the outermost
/// unmatched `if` for this comparison), producing one arm per branch: the condition text for each
/// `if`/`else if`, and a final wildcard (`signature: None`) arm for a trailing bare `else`, if any.
///
/// `else_clause`-wrapping grammars (Rust, C, C++, JS/TS/TSX) always give that wrapper exactly one
/// child - either a block or a nested `if` - so unwrapping it is a single `named_child(0)`. Grammars
/// that put `alternative` directly on the next branch (Java, Go, C#) need no unwrapping at all.
fn if_chain_arms(node: Node, language: &Language, source: &[u8]) -> Option<Vec<FlowControlArm>> {
    let wraps_else_clause = matches!(
        language,
        Language::Rust
            | Language::C
            | Language::CPP
            | Language::JavaScript
            | Language::TypeScript
            | Language::TSX
    );

    let mut arms = Vec::new();
    let mut current = node;
    // A chain this long would be a code smell in the source itself; the cap is just to keep a
    // malformed/unexpected tree from looping forever.
    for _ in 0..64 {
        let signature = current
            .child_by_field_name("condition")
            .and_then(|condition| trimmed_text(condition, source));
        arms.push(FlowControlArm {
            node_id: current.id(),
            signature,
        });

        let Some(alternative) = current.child_by_field_name("alternative") else {
            break;
        };
        let next = if wraps_else_clause {
            match alternative.named_child(0) {
                Some(inner) => inner,
                None => break,
            }
        } else {
            alternative
        };

        if flow_control_family(next.kind(), language) == Some(FlowControlFamily::If) {
            current = next;
        } else {
            // A bare `else { ... }`: terminal, no condition of its own.
            arms.push(FlowControlArm {
                node_id: next.id(),
                signature: None,
            });
            break;
        }
    }
    Some(arms)
}

fn trimmed_text(node: Node, source: &[u8]) -> Option<String> {
    let text = node.utf8_text(source).ok()?.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

/// The set of non-wildcard arm signatures for a flow-control container, as used by
/// `flow_control_similarity_of_sets`. Callers comparing one candidate against many others (e.g.
/// `solve_similar_flow_control`'s all-pairs scoring) should build this once per candidate and
/// reuse it, rather than re-deriving it from `arms` on every pairwise comparison.
pub fn flow_control_signature_set(arms: &[FlowControlArm]) -> std::collections::HashSet<&str> {
    arms.iter().filter_map(|a| a.signature.as_deref()).collect()
}

/// Fraction of non-wildcard arm signatures shared between two flow-control containers (Jaccard
/// similarity: shared signatures / all distinct signatures across both sides), given their
/// precomputed signature sets (see `flow_control_signature_set`).
///
/// Returns 0.0 if either side has no non-wildcard signatures at all (nothing meaningful to
/// compare), so an empty/all-wildcard construct never spuriously "matches" another one.
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

/// Fraction of non-wildcard arm signatures shared between two flow-control containers (Jaccard
/// similarity: shared signatures / all distinct signatures across both sides).
///
/// Returns 0.0 if either side has no non-wildcard signatures at all (nothing meaningful to
/// compare), so an empty/all-wildcard construct never spuriously "matches" another one.
pub fn flow_control_similarity(
    before_arms: &[FlowControlArm],
    after_arms: &[FlowControlArm],
) -> f64 {
    flow_control_similarity_of_sets(
        &flow_control_signature_set(before_arms),
        &flow_control_signature_set(after_arms),
    )
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

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::code::{Code, Language};
    use crate::test::helper;

    use super::*;

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

    fn rust_match_container(src: &str) -> Code {
        Code::from_string(src, &Language::Rust)
    }

    #[test]
    fn rust_match_arms_extracts_string_literal_patterns() {
        let code = rust_match_container(
            r#"
fn f(s: &str) {
    match s {
        "a" => 1,
        "b" => 2,
        _ => 0,
    };
}
"#,
        );
        let ast = code.ast.as_ref().unwrap();
        let match_expr = helper::find_first_of_kind(ast.root_node(), "match_expression").unwrap();
        let arms = match_arms(match_expr, &Language::Rust, code.contents.as_bytes()).unwrap();
        let signatures: Vec<Option<&str>> = arms.iter().map(|a| a.signature.as_deref()).collect();
        assert_eq!(signatures, vec![Some("\"a\""), Some("\"b\""), None]);
    }

    #[test]
    fn rust_match_arm_guard_is_excluded_from_signature() {
        let code = rust_match_container(
            r#"
fn f(s: i32) {
    match s {
        n if n > 0 => 1,
        _ => 0,
    };
}
"#,
        );
        let ast = code.ast.as_ref().unwrap();
        let match_expr = helper::find_first_of_kind(ast.root_node(), "match_expression").unwrap();
        let arms = match_arms(match_expr, &Language::Rust, code.contents.as_bytes()).unwrap();
        assert_eq!(arms[0].signature.as_deref(), Some("n"));
    }

    #[test]
    fn flow_control_similarity_ignores_wildcard_and_scores_jaccard() {
        let before = rust_match_container(
            r#"
fn f(s: &str) {
    match s {
        "asset" => 1,
        "ecmascript" => 2,
        "wasm" => 3,
        _ => 0,
    };
}
"#,
        );
        let after = rust_match_container(
            r#"
fn f(s: &str) {
    match s {
        "asset" => 1,
        "ecmascript" => 2,
        "json" => 4,
        _ => 0,
    };
}
"#,
        );
        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();
        let before_expr =
            helper::find_first_of_kind(before_ast.root_node(), "match_expression").unwrap();
        let after_expr =
            helper::find_first_of_kind(after_ast.root_node(), "match_expression").unwrap();
        let before_arms =
            match_arms(before_expr, &Language::Rust, before.contents.as_bytes()).unwrap();
        let after_arms =
            match_arms(after_expr, &Language::Rust, after.contents.as_bytes()).unwrap();

        // Shared: asset, ecmascript (2). Union: asset, ecmascript, wasm, json (4). Wildcards
        // excluded from both sets entirely, so a trivial `_`<->`_` match can't inflate the score.
        let score = flow_control_similarity(&before_arms, &after_arms);
        assert!(
            (score - 0.5).abs() < 1e-9,
            "expected 2/4 = 0.5, got {score}"
        );
    }

    #[test]
    fn c_switch_arms_extracts_case_values_and_default() {
        let code = Code::from_string(
            r#"
void f(int x) {
    switch (x) {
        case 1: break;
        case 2: break;
        default: break;
    }
}
"#,
            &Language::C,
        );
        let ast = code.ast.as_ref().unwrap();
        let switch_stmt = helper::find_first_of_kind(ast.root_node(), "switch_statement").unwrap();
        let arms = switch_arms(switch_stmt, &Language::C, code.contents.as_bytes()).unwrap();
        let signatures: Vec<Option<&str>> = arms.iter().map(|a| a.signature.as_deref()).collect();
        assert_eq!(signatures, vec![Some("1"), Some("2"), None]);
    }

    #[test]
    fn rust_if_chain_extracts_conditions_and_trailing_else() {
        let code = rust_match_container(
            r#"
fn f(x: i32) -> i32 {
    if x > 0 {
        1
    } else if x < 0 {
        2
    } else {
        0
    }
}
"#,
        );
        let ast = code.ast.as_ref().unwrap();
        let if_expr = helper::find_first_of_kind(ast.root_node(), "if_expression").unwrap();
        let arms = if_chain_arms(if_expr, &Language::Rust, code.contents.as_bytes()).unwrap();
        let signatures: Vec<Option<&str>> = arms.iter().map(|a| a.signature.as_deref()).collect();
        assert_eq!(signatures, vec![Some("x > 0"), Some("x < 0"), None]);
    }

    #[test]
    fn rust_if_chain_without_else_has_no_trailing_wildcard_arm() {
        let code = rust_match_container(
            r#"
fn f(x: i32) {
    if x > 0 {
        1;
    }
}
"#,
        );
        let ast = code.ast.as_ref().unwrap();
        let if_expr = helper::find_first_of_kind(ast.root_node(), "if_expression").unwrap();
        let arms = if_chain_arms(if_expr, &Language::Rust, code.contents.as_bytes()).unwrap();
        let signatures: Vec<Option<&str>> = arms.iter().map(|a| a.signature.as_deref()).collect();
        assert_eq!(signatures, vec![Some("x > 0")]);
    }

    #[test]
    fn c_if_chain_extracts_conditions() {
        let code = Code::from_string(
            r#"
int f(int x) {
    if (x > 0) {
        return 1;
    } else if (x < 0) {
        return 2;
    } else {
        return 0;
    }
}
"#,
            &Language::C,
        );
        let ast = code.ast.as_ref().unwrap();
        let if_stmt = helper::find_first_of_kind(ast.root_node(), "if_statement").unwrap();
        let arms = if_chain_arms(if_stmt, &Language::C, code.contents.as_bytes()).unwrap();
        let signatures: Vec<Option<&str>> = arms.iter().map(|a| a.signature.as_deref()).collect();
        assert_eq!(signatures, vec![Some("(x > 0)"), Some("(x < 0)"), None]);
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
        solve(&before, &after, &node_cache, &mut diff, true);

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
                    .map_or(false, |(bid, aid)| diff.mapping.contains_key(&(bid, aid)))
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
        solve(&before, &after, &node_cache, &mut diff, true);

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
                    .map_or(false, |(bid, aid)| diff.mapping.contains_key(&(bid, aid)))
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

    fn collect_all<'a>(
        node: tree_sitter::Node<'a>,
        kind: &str,
        out: &mut Vec<tree_sitter::Node<'a>>,
    ) {
        if node.kind() == kind {
            out.push(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            collect_all(child, kind, out);
        }
    }
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
mod is_commutative_container_tests {
    use super::is_commutative_container;
    use crate::code::{Code, Language};

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
}
