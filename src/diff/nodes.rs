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
//! What the diff passes need to know about node kinds, per language: which kinds are references,
//! imports, comments, literals and identifiers, which carry a name that identifies them
//! ([`is_semantically_structural`]), and which cross-kind pairs may count as an update
//! ([`kinds_update_allowed`]). Plus a few mutators shared by several passes, such as
//! [`map_identical_descendants`].
use tree_sitter::Node;

use crate::code::{ASTMetadata, Code, Language};
use crate::diff::apted::{self, Algorithm};
use crate::diff::{ASTDiff, ASTMapping, ASTMappingReason};

/// Maps every descendant of a matched pair of identical subtrees, position by position, as
/// `Identical`/`IdenticalHashOfAncestor`. Precondition: the subtrees are identical. It skips
/// (and does not descend into) a child pair whose kinds differ or whose before node is
/// already mapped.
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
                ASTMapping::identical(ASTMappingReason::IdenticalHashOfAncestor),
            );
            stack.push((before_child, after_child));
        }
    }
}

/// Every node under `root` that satisfies `predicate` and is not in `mapped`, in no particular
/// order. It does not descend into mapped nodes, but does descend into collected ones, since a
/// match can nest inside another (a diagnostic call in another's arguments).
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

/// Proposes the pair to APTED and, if APTED kept it as a match rather than delete + insert,
/// relabels that mapping's reason to `reason`.
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

/// Whether `node_kind` is a reference node: a unit a reader thinks of as a whole (a function, a
/// class, an import), used as an anchor for exact-hash matching whatever its size.
pub fn is_reference(node_kind: &str, language: &Language) -> bool {
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
        Language::JSON | Language::YAML => node_kind == "document" || node_kind == "fragment",
        // An XML element is far below `NodeSelectionConfig::min_subtree_size`, so without this a
        // file of many small elements (Android `strings.xml`) leaves nearly every node to the
        // fallback. Being a reference node only makes it a candidate; a match still needs
        // byte-identical subtrees.
        Language::XML => {
            node_kind == "document" || node_kind == "fragment" || node_kind == "element"
        }
        _ => false,
    }
}

/// True for the import/use/include statement kinds [`is_reference`] lists for `language`.
///
/// Phase 1's shape-only tier (`KindOnlyHash`) skips these: two unrelated imports with the same
/// number of path segments have the same shape, and pairing them plus a few identifier updates
/// is cheaper than delete + insert and wrong to every reader (`kotlin-remove-function`). The
/// byte-exact tier and every later pass still see them.
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

/// Whether [`local_identity_name`] has any arm for `language`, so the root-level prematch can
/// skip its whole-tree walk. Must list exactly the languages that function's arms do.
pub(crate) fn has_local_identity_coverage(language: &Language) -> bool {
    matches!(
        language,
        Language::Kotlin | Language::CSharp | Language::ShellScript
    )
}

/// The `(kind, name)` of a container member (Java method, constructor or field; C/C++ field or
/// function definition), for anchoring members whose bodies changed. Overloads are not a concern:
/// the caller only trusts a name unique among the leftovers on both sides.
pub(crate) fn member_identity_name(
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
    // Walks down through declarator wrappers (`*name`, `name[4]`, `(*name)(int)`) to the first
    // leaf of `wanted` kind.
    let through_declarators = |mut parent_id: usize, wanted: &str| -> Option<usize> {
        for _ in 0..8 {
            if let Some(id) = first_child_of_kind(parent_id, wanted) {
                return Some(id);
            }
            let next = meta
                .node_info
                .get(&parent_id)?
                .children
                .iter()
                .copied()
                .find(|&c| {
                    meta.node_info
                        .get(&c)
                        .is_some_and(|i| i.kind.ends_with("_declarator"))
                })?;
            parent_id = next;
        }
        None
    };
    let text_of = |id: usize| meta.node_info.get(&id).map(|i| i.text.clone());
    match (language, info.kind.as_str()) {
        (Language::Java, "method_declaration") => Some((
            "method_declaration",
            text_of(first_child_of_kind(node_id, "identifier")?)?,
        )),
        (Language::Java, "constructor_declaration") => Some((
            "constructor_declaration",
            text_of(first_child_of_kind(node_id, "identifier")?)?,
        )),
        (Language::Java, "field_declaration") => {
            let declarator = first_child_of_kind(node_id, "variable_declarator")?;
            Some((
                "field_declaration",
                text_of(first_child_of_kind(declarator, "identifier")?)?,
            ))
        }
        (Language::C | Language::CPP, "field_declaration") => Some((
            "field_declaration",
            text_of(through_declarators(node_id, "field_identifier")?)?,
        )),
        (Language::C | Language::CPP, "function_definition") => Some((
            "function_definition",
            text_of(through_declarators(node_id, "identifier")?)?,
        )),
        _ => None,
    }
}

/// A scope-local `(kind_bucket, name)` for parameters, local declarations and shell assignments,
/// used by `apted::prematch_unique_named_locals` within one container. Kept out of
/// [`is_semantically_structural`], whose whole-file walk would make every local in the file a
/// candidate. `kind_bucket` keeps a parameter `x` from matching a local `x`.
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
        (Language::Kotlin, "parameter") => {
            let name_id = first_child_of_kind(node_id, "identifier")?;
            let text = meta.node_info.get(&name_id)?.text.clone();
            Some(("parameter", text))
        }
        // Keyed on the statement, not the declarator, so `var x = ...` re-anchors as one unit.
        (Language::CSharp, "local_declaration_statement") => {
            let decl_id = first_child_of_kind(node_id, "variable_declaration")?;
            let declarator_id = first_child_of_kind(decl_id, "variable_declarator")?;
            let name_id = first_child_of_kind(declarator_id, "identifier")?;
            let text = meta.node_info.get(&name_id)?.text.clone();
            Some(("local_declaration_statement", text))
        }
        (Language::ShellScript, "variable_assignment") => {
            let name_id = first_child_of_kind(node_id, "variable_name")?;
            let text = meta.node_info.get(&name_id)?.text.clone();
            Some(("variable_assignment", text))
        }
        // No CSS `declaration` arm: the same selector text can name different rules on the two
        // sides (a selector renamed in place, a copy under the old name added elsewhere), and a
        // declaration's identity is its rule's, not its selector text's.
        _ => None,
    }
}

/// `(node_kind, text of the field child)`, when that child exists and, if `kind` is given, has
/// that kind.
fn named_child_text(
    node: &Node,
    bytes: &[u8],
    field: &str,
    kind: Option<&str>,
) -> Option<(String, String)> {
    let child = node.child_by_field_name(field)?;
    if kind.is_some_and(|kind| child.kind() != kind) {
        return None;
    }
    let text = child.utf8_text(bytes).ok()?;
    Some((node.kind().to_string(), text.to_string()))
}

/// `(node_kind, identity)` for a declaration whose name the language makes (nearly) unique, such
/// as the one `fn main()` in a Rust crate, so the pipeline can match it by name at once.
pub fn is_semantically_structural<'a>(
    node: &Node<'a>,
    language: &Language,
    code: &Code,
) -> Option<(String, String)> {
    let node_kind = node.kind();

    let bytes = code.contents.as_bytes();

    match language {
        Language::Rust => match node_kind {
            "function_item" | "mod_item" => {
                named_child_text(node, bytes, "name", Some("identifier"))
            }
            "struct_item" | "enum_item" | "trait_item" => {
                named_child_text(node, bytes, "name", Some("type_identifier"))
            }
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
            "function_definition" | "class_definition" => {
                named_child_text(node, bytes, "name", Some("identifier"))
            }
            _ => None,
        },
        Language::Go => match node_kind {
            "function_declaration" => named_child_text(node, bytes, "name", Some("identifier")),
            "method_declaration" => {
                let method_name = node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(bytes).ok())?;
                let receiver = node.child_by_field_name("receiver")?;
                let mut rc = receiver.walk();
                let param_decl = receiver.named_children(&mut rc).next()?;
                let type_node = param_decl.child_by_field_name("type")?;
                let type_text = type_node.utf8_text(bytes).ok()?;
                let receiver_type = type_text.trim_start_matches('*');
                Some((
                    node_kind.to_string(),
                    format!("{receiver_type}.{method_name}"),
                ))
            }
            "type_spec" | "type_alias" => {
                named_child_text(node, bytes, "name", Some("type_identifier"))
            }
            // A top-level `var`/`const` often holds a large table-driven literal. The declaration
            // is keyed (by its first name) for `top_level_identities`, which sees only the root's
            // direct children; the specs are keyed too, for the any-depth name walk.
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

                // An extension function's receiver is the `user_type` before the name.
                let receiver_prefix: String = {
                    let mut cur = node.walk();
                    node.named_children(&mut cur)
                        .find(|c| c.kind() == "user_type" && c.start_byte() < name_start)
                        .and_then(|r| r.utf8_text(bytes).ok())
                        .map(|s| {
                            let base = s.split('<').next().unwrap_or(s).trim();
                            format!("{}.", base)
                        })
                        .unwrap_or_default()
                };

                // Parameter types disambiguate overloads.
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
            "class_declaration" | "object_declaration" => {
                named_child_text(node, bytes, "name", Some("identifier"))
            }
            "companion_object" => named_child_text(node, bytes, "name", Some("identifier")),
            // The alias name is under field "type", not "name".
            "type_alias" => named_child_text(node, bytes, "type", Some("identifier")),
            _ => None,
        },
        Language::CSharp => match node_kind {
            "class_declaration"
            | "struct_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "method_declaration"
            | "namespace_declaration" => named_child_text(node, bytes, "name", None),
            // `int a, b;` is one declaration; keyed by its first name, like Go's grouped `var`.
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
        Language::C => match node_kind {
            "function_definition" => node
                .child_by_field_name("declarator")
                .and_then(|d| c_family_declarator_name(d, bytes))
                .map(|name| (node_kind.to_string(), name.to_string())),
            "struct_specifier" | "enum_specifier" | "union_specifier" => {
                named_child_text(node, bytes, "name", None)
            }
            _ => None,
        },
        Language::CPP => match node_kind {
            "function_definition" => c_family_test_macro_name(node, bytes)
                .map(|name| (node_kind.to_string(), name))
                .or_else(|| {
                    node.child_by_field_name("declarator")
                        .and_then(|d| c_family_declarator_name(d, bytes))
                        .map(|name| (node_kind.to_string(), name.to_string()))
                }),
            "class_specifier" | "struct_specifier" | "enum_specifier" | "union_specifier" => {
                named_child_text(node, bytes, "name", None)
            }
            // `namespace A::B` yields the fully qualified name as is.
            "namespace_definition" => named_child_text(node, bytes, "name", None),
            _ => None,
        },
        Language::Java => match node_kind {
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "method_declaration" => named_child_text(node, bytes, "name", None),
            // Keyed by the first declarator, as in C#.
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
            | "type_alias_declaration" => named_child_text(node, bytes, "name", None),
            // `const f = () => ...` takes the declarator's name; a callback argument has no
            // identity of its own.
            "arrow_function" | "function_expression" => {
                let parent = node.parent()?;
                (parent.kind() == "variable_declarator")
                    .then(|| parent.child_by_field_name("name"))
                    .flatten()
                    .and_then(|n| n.utf8_text(bytes).ok())
                    .map(|name| (node_kind.to_string(), name.to_string()))
            }
            // A top-level `const X = <non-function value>`. Keyed on the declaration, not the
            // declarator, because `top_level_identities` sees only the root's direct children, and
            // only that lets `solve_large_flat_subtrees` Myers-diff a huge literal instead of
            // sending it to APTED (`typescript-excalidraw-excalidraw-add-values-to-lists`).
            // Top-level, single-declarator, plain-identifier only: a local name is not reliably
            // unique in a file.
            "lexical_declaration" | "variable_declaration" => {
                let mut cursor = node.walk();
                let mut declarators = node
                    .children(&mut cursor)
                    .filter(|c| c.kind() == "variable_declarator");
                let declarator = declarators.next()?;
                if declarators.next().is_some() {
                    return None;
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
        // tree-sitter-php names these `function_definition` and `method_declaration`, unlike
        // `is_reference`'s spelling.
        Language::PHP => match node_kind {
            "class_declaration" | "function_definition" | "method_declaration" => {
                named_child_text(node, bytes, "name", None)
            }
            _ => None,
        },
        Language::Ruby => match node_kind {
            // `singleton_method` is `def self.foo`, a distinct kind from `method`.
            "class" | "module" | "method" | "singleton_method" => {
                named_child_text(node, bytes, "name", None)
            }
            _ => None,
        },
        // Unvalidated: these follow the usual `name`-field convention but no fixture has checked
        // them against the real grammar.
        Language::Swift => match node_kind {
            "function_declaration"
            | "class_declaration"
            | "struct_declaration"
            | "enum_declaration"
            | "protocol_declaration" => named_child_text(node, bytes, "name", None),
            _ => None,
        },
        Language::Scala => match node_kind {
            "class_definition"
            | "object_definition"
            | "trait_definition"
            | "function_definition" => named_child_text(node, bytes, "name", None),
            _ => None,
        },
        Language::R | Language::ShellScript | Language::LUA | Language::Vimscript => {
            match node_kind {
                "function_definition" | "function_declaration" => {
                    named_child_text(node, bytes, "name", None)
                }
                _ => None,
            }
        }
        // A mapping key is the identity of a config file's entries. Repeated keys (`one`, `name`)
        // are safe: identities are resolved through the whole ancestor key path.
        Language::YAML if node_kind == "block_mapping_pair" => {
            named_child_text(node, bytes, "key", None)
        }
        _ => None,
    }
}

/// The `identifier` name of a Go `var_spec`/`const_spec`.
fn go_spec_identifier_name<'a>(spec: Node<'a>, bytes: &'a [u8]) -> Option<&'a str> {
    spec.child_by_field_name("name")
        .filter(|n| n.kind() == "identifier")
        .and_then(|n| n.utf8_text(bytes).ok())
}

/// The name at the end of a C/C++ declarator chain (`*`, `[]`, `()`, `&` wrappers). A C++
/// `qualified_identifier` already carries its `Class::method` scope.
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

/// `"<macro>:<Suite>:<Case>"` for a googletest `TEST(Suite, Case)` (or `TEST_F`/`TEST_P`) block,
/// which tree-sitter-cpp parses as a function named `TEST` with two anonymous parameters. Without
/// it every test in a file shares the name `TEST`. A real function with named parameters is not
/// taken for the macro.
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
        return None;
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

/// The name of a Go subtest, `<anything>.Run("name", ...)`, recognized by shape alone since the
/// receiver varies (`t`, `c`, `suite`). A coincidental match only groups a call by name; APTED
/// still decides the pairing.
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

// Cross-kind update families: leaves whose kinds share a family occupy the same grammatical slot,
// so `<` -> `<=` is an update, not delete + insert. Families are separate so `<` never crosses to
// `+`. A token a grammar lacks is harmless in a shared list.

/// Excludes keyword comparisons (`in`, `is`, `instanceof`): they also appear outside
/// comparisons (`for x in y`), where a swap would pair unrelated keywords.
const COMPARISON_OPS: &[&str] = &[
    "<", "<=", ">", ">=", "==", "!=", "===", "!==", "<>", "<=>", "not_eq", "=~", "!~",
];

const ARITHMETIC_OPS: &[&str] = &["+", "-", "*", "/", "%", "**", "//", "@"];

/// PHP's `.` concatenation sits in the arithmetic slot.
const PHP_ARITHMETIC_OPS: &[&str] = &["+", "-", "*", "/", "%", "**", "."];

const BITWISE_OPS: &[&str] = &[
    "&", "|", "^", "<<", ">>", ">>>", "&^", "bitand", "bitor", "xor",
];

/// `??` and `?:` occupy the same fallback-value slot as `||`.
const LOGICAL_OPS: &[&str] = &["&&", "||", "and", "or", "??", "?:"];

const ASSIGNMENT_OPS: &[&str] = &[
    "=", "+=", "-=", "*=", "/=", "%=", "**=", "//=", "&=", "|=", "^=", "<<=", ">>=", ">>>=", "&&=",
    "||=", "??=", "@=", ".=", "and_eq", "or_eq", "xor_eq", "&^=",
];

const INCREMENT_OPS: &[&str] = &["++", "--"];

const RUST_RANGE_OPS: &[&str] = &["..", "..=", "..."];

/// `foo.bar` -> `foo?.bar`, `s.x` -> `s->x`. Not given to PHP, where `.` is concatenation.
const MEMBER_ACCESS_OPS: &[&str] = &[".", "?.", "->", "?->", "&."];

// No C/C++ `type_identifier`/`primitive_type` family: the ground truth contradicts itself.
// `cpp-tensorflow-switch-to-primitive-types` pairs the two as an update; `cpp-add-templates` and
// `c-linux-small-change-struct-to-char` delete one and insert the other in the same slot.

/// `1` -> `1.0` is a value edit; human mappings pair these without exception.
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

/// Shell `[ a == b ]` test operators (`shellscript-torvalds-linux-double-equals-to-equals`).
const SHELL_TEST_OPS: &[&str] = &["==", "=", "!="];

/// `java-scrcpy-public-to-protected`. Not given to C#: in
/// `csharp-glibsharp-gtksharp-interesting-case-...` it pairs modifiers across a deleted
/// constructor and an inserted property.
const ACCESS_MODIFIERS: &[&str] = &[
    "public",
    "private",
    "protected",
    "internal",
    "fileprivate",
    "open",
];

const DECLARATION_KEYWORDS: &[&str] = &["let", "var", "const", "val"];

/// `html-hugo-tag-to-selfclosing-tag`. The re-kinded parent tags are deliberately not a family:
/// pair the leaf, or APTED takes the parent pairing and the leaf mapping comes out wrong.
const HTML_TAG_END: &[&str] = &["/>", ">"];

/// `:key => v` vs `key: v` (`ruby-jmespath-jmespath-formatting-and-style-guide-fixes`). The keys'
/// `simple_symbol`/`hash_key_symbol` pair is not a family: it pairs symbols across unrelated
/// deleted and inserted code (`ruby-homebrew-brew-*`).
const RUBY_HASH_SEPARATORS: &[&str] = &["=>", ":"];

/// Python `x = None` -> `x = default_value`
/// (`python-pytorch-pytorch-add-param-to-many-places-and-update-one`). Python only: in C it pairs
/// a `return NULL` with an unrelated name (`c-nginx-add-typedef`).
const NULL_LITERAL_KINDS: &[&str] = &["none", "identifier"];

/// Shell string bodies, whose kind is an artifact of the quoting around them
/// (`shellscript-scikit-learn-scikit-learn-string-to-regex`).
const SHELL_STRING_BODY_KINDS: &[&str] = &["string_content", "raw_string", "regex"];

/// `return true` -> `return false` where each literal has its own kind
/// (`java-defects4j-math-22-fdistribution`). Java only until a fixture asks for another language.
const BOOLEAN_LITERAL_KINDS: &[&str] = &["true", "false"];

/// TypeScript's type keywords (the leaves inside `predefined_type`) with `type_identifier`, for
/// `value: number` -> `value: T`. The keyword leaves, not `predefined_type`: pairing the parent
/// costs the same, so APTED would be free to take it, and the human mapping pairs the leaf.
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

/// Name kinds that may pair across kinds (`pwd` -> `cb_data.pwd`). Scoped identifiers are
/// excluded: their qualification is part of the identity.
const IDENTIFIER_KINDS: &[&str] = &[
    "identifier",
    "field_identifier",
    "type_identifier",
    "property_identifier",
    "shorthand_property_identifier",
    "shorthand_property_identifier_pattern",
];

/// Whether `kind` is in `IDENTIFIER_KINDS`.
pub fn is_identifier_kind(kind: &str) -> bool {
    IDENTIFIER_KINDS.contains(&kind)
}

fn in_shared_family(kind_a: &str, kind_b: &str, families: &[&[&str]]) -> bool {
    families
        .iter()
        .any(|family| family.contains(&kind_a) && family.contains(&kind_b))
}

/// Every family in one fixed order: index `i` is bit `i` of a [`FamilyMask`]. Both
/// [`operator_family_mask`] and [`language_operator_family_mask`] derive their bits from this
/// list, so they cannot disagree.
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
    BOOLEAN_LITERAL_KINDS,
    SHELL_STRING_BODY_KINDS,
];

/// A family past the mask's width would wrap silently in release; fail to compile instead.
const _: () = assert!(ALL_OPERATOR_FAMILIES.len() <= FamilyMask::BITS as usize);

/// One bit per entry of `ALL_OPERATOR_FAMILIES`.
pub type FamilyMask = u32;

/// Bit `i` set iff `kind` is in `ALL_OPERATOR_FAMILIES[i]`; a kind may be in several (`+`).
/// Computed once per node at metadata-build time, so [`update_allowed_from_masks`] is a bitwise
/// AND.
pub fn operator_family_mask(kind: &str) -> FamilyMask {
    let mut mask: FamilyMask = 0;
    for (i, family) in ALL_OPERATOR_FAMILIES.iter().enumerate() {
        if family.contains(&kind) {
            mask |= 1 << i;
        }
    }
    mask
}

/// Bit `i` set iff `language` recognizes `ALL_OPERATOR_FAMILIES[i]`. Compares array pointers, not
/// contents, since families share tokens.
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

/// The precomputed-mask equivalent of [`kinds_update_allowed`] for two different kinds. The
/// caller handles the same-kind case.
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

/// Whether a `kind_a` node may be updated into a `kind_b` node. Different kinds never pair, except
/// identifier kinds with each other and kinds sharing one of `language`'s families above.
pub fn kinds_update_allowed(kind_a: &str, kind_b: &str, language: &Language) -> bool {
    if kind_a == kind_b {
        return true;
    }

    if is_identifier_kind(kind_a) && is_identifier_kind(kind_b) {
        return true;
    }

    in_shared_family(kind_a, kind_b, families_for_language(language))
}

/// The cross-kind families `language` recognizes; the one source for both
/// [`kinds_update_allowed`] and [`language_operator_family_mask`].
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
            BOOLEAN_LITERAL_KINDS,
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
        // `let x = ...` -> `let x .= ...` (`vimscript-neovim-neovim-small-change-2`).
        Language::Vimscript => &[
            COMPARISON_OPS,
            ARITHMETIC_OPS,
            LOGICAL_OPS,
            ASSIGNMENT_OPS,
            NUMERIC_LITERAL_KINDS,
        ],
        Language::ShellScript => &[SHELL_TEST_OPS, SHELL_STRING_BODY_KINDS],
        Language::HTML | Language::XML => &[HTML_TAG_END],
        Language::CSS => &[NUMERIC_LITERAL_KINDS],
        _ => &[],
    }
}

/// Brackets and separators: grammar glue in every supported language.
const GENERIC_PUNCTUATION: &[&str] = &["(", ")", "{", "}", "[", "]", ";", ",", ":", "::", "."];

/// Delimiters that come in pairs, opener to the closers that may end it (`<` also ends in `/>`).
/// `<` is a tag opener in HTML and an operator in Rust, so a caller must also check that the other
/// half is among the same parent's children.
pub const PAIRED_DELIMITERS: &[(&str, &[&str])] = &[
    ("(", &[")"]),
    ("[", &["]"]),
    ("{", &["}"]),
    ("<", &[">", "/>"]),
    ("</", &[">"]),
    ("<?", &["?>"]),
];

/// `kind`'s closers if it is an opener, its openers if it is a closer, `None` otherwise.
pub fn delimiter_complement_kinds(kind: &str) -> Option<Vec<&'static str>> {
    if let Some((_, closers)) = PAIRED_DELIMITERS.iter().find(|(open, _)| *open == kind) {
        return Some(closers.to_vec());
    }
    let openers: Vec<&'static str> = PAIRED_DELIMITERS
        .iter()
        .filter(|(_, closers)| closers.contains(&kind))
        .map(|(open, _)| *open)
        .collect();
    (!openers.is_empty()).then_some(openers)
}

/// Leaves whose identity is their value, as opposed to `IDENTIFIER_KINDS`, whose identity is a
/// name.
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

pub fn is_literal_kind(kind: &str) -> bool {
    LITERAL_KINDS.contains(&kind)
}

/// Whether `kind` is punctuation or an operator: a leaf compatible with its twin almost anywhere in
/// a file, so its kind alone is no evidence of a correspondence. Uses the family lists, not an
/// all-symbols test, to catch keyword operators (`and`, `bitand`).
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

/// Whether `kind` is a comment in any supported grammar.
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

/// Whether `kind` is an attribute/decorator that is a *sibling* of the declaration it modifies.
/// Most grammars nest these inside the declaration, where they match with it for free. Note that
/// JavaScript's `decorator` is nested, unlike TypeScript's of the same name.
pub fn is_leading_modifier(kind: &str, language: &Language) -> bool {
    match language {
        Language::Rust => kind == "attribute_item",
        Language::Python => kind == "decorator",
        Language::TypeScript | Language::TSX => kind == "decorator",
        _ => false,
    }
}

/// Keeps clear renames (`fetch_user` -> `fetch_user_data`, `user_id` -> `userId`) and rejects
/// names sharing only a stray character pair.
const LEAF_TEXT_SIMILARITY_THRESHOLD: f64 = 0.6;

/// Whether two leaf texts read as the same token renamed (character-bigram Dice similarity).
/// Single-character texts never pass unless equal: `i` -> `j` carries no textual evidence, and the
/// caller's matched-ancestor context is what lets such renames through.
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

/// [`kinds_update_allowed`], plus: if either kind is a generic token, the parents must already
/// correspond. Otherwise tree edit distance pairs a lone `<` across unrelated statements just
/// because reuse is cheaper than delete + insert. `parents_matched` is called only when needed.
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

/// See [`flow_control_family`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlowControlFamily {
    Match,
    Switch,
    If,
}

/// The [`FlowControlFamily`] of `node_kind` in `language`, if any. `If` covers only grammars whose
/// `alternative` recurses into a block or another `if`, so not Python's flat `elif_clause`.
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

/// Whether `node_kind` is a statement block or a flow-control construct, the candidates of
/// `solve_greedy_anchor_blocks`. Deliberately narrow: treating every node with several children as
/// a candidate anchors unrelated calls whose argument lists happen to hash-match.
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

/// Jaccard similarity of two sets; 0.0 if either is empty, so two empty sets never match.
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

/// Substrings of the lowercased last callee segment that mark a call as meant for the programmer
/// (logging, bailouts, assertions). Substring matching covers `Errorf`, `LogWarning` and the like.
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

fn callee_text<'a>(node: Node, language: &Language, source: &'a [u8]) -> Option<&'a str> {
    let field_name = match (language, node.kind()) {
        (Language::Rust, "macro_invocation") => "macro",
        (Language::Java, "method_invocation") => "name",
        _ => "function",
    };
    node.child_by_field_name(field_name)?.utf8_text(source).ok()
}

/// Whether `node` has text of its own: a leaf, or an interior node with non-whitespace bytes in
/// the gaps between its children (a comment whose `//` is a separate child). False for a `block`
/// or `argument_list`, whose every byte belongs to a child.
///
/// **A function of the AST and source only, never of a diff.** Derived from the renderer, both
/// the numerator and the denominator of a visible-mismatch rate would move with the algorithm, and
/// a diff that renders coarsely would have almost nothing it could get visibly wrong.
///
/// Non-ASCII bytes count as content, biasing toward visible: the safe side for a metric that
/// exists to find mistakes.
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

/// Every node id in `code` for which [`is_structurally_visible`] holds.
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

/// Whether `node` is a call whose callee looks diagnostic (see `DIAGNOSTIC_CALLEE_KEYWORDS`).
/// Loose on purpose: its caller only pairs byte-identical subtrees, so a false positive is
/// harmless.
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

/// Whether `node_kind` directly holds a function's, class's or namespace's statements or members.
/// The kind names are unambiguous across grammars, so no `Language` is needed.
///
/// The anchor for `crate::diff::apted::prematch_identical_statement_siblings`. An allow-list,
/// not "the widest descendant": that picks a macro's `token_tree` over the function's own `block`
/// (`rust-tauri-cli-ios-dev`). Not exhaustive; a missing entry only costs a missed speed-up.
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

/// Whether the order of `node_kind`'s children carries no meaning in `language` (struct fields,
/// enum variants, imports, object keys). Conservative: only containers the language itself makes
/// order-independent, not ones formatters often reorder. Hashes and child pairing honour it.
///
/// Verify each kind against a real parse, not `node-types.json`, which can omit aliased kinds: a
/// kind that never occurs makes its arm silently dead.
pub fn is_commutative_container(node_kind: &str, language: &Language) -> bool {
    match language {
        Language::Rust => {
            node_kind == "enum_variant_list"
                || node_kind == "use_list"
                || node_kind == "field_declaration_list"
        }
        Language::Go => node_kind == "field_declaration_list" || node_kind == "import_spec_list",
        Language::Python => node_kind == "dictionary",
        Language::Java => node_kind == "enum_body",
        Language::CSharp => node_kind == "enum_member_declaration_list",
        Language::C | Language::CPP => node_kind == "enumerator_list",
        Language::JavaScript | Language::TypeScript | Language::TSX => node_kind == "object",
        // Kotlin imports are direct children of `source_file`, with no wrapper to name.
        Language::Kotlin => false,
        Language::Scala => {
            // `import a.b.{X, Y, Z}`.
            node_kind == "namespace_selectors"
        }
        Language::Swift => node_kind == "enum_class_body",
        // Without these, one inserted key desyncs every later key's position in a large object.
        Language::JSON => node_kind == "object",
        Language::YAML => node_kind == "block_mapping" || node_kind == "flow_mapping",
        _ => false,
    }
}

#[cfg(test)]
mod tests;
