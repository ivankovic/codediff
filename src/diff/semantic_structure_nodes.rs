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

use crate::code::{Code, Language};

/*
* Semantically structural nodes are nodes that have a semantic meaning that is "loosely fixed" and
* typically enforced by the compiler in some way. For example, there can only ever be ONE
* 'fn main()' in main.rs in a Rust project. It is sensible for the algorithm to match such nodes
* immediately.
*
* Returns (node_kind, identifier) if the node matches.
*/
pub fn node_matches<'a>(
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
                // Strip leading `*` for pointer receivers: `*Foo` → `Foo`
                let receiver_type = type_text.trim_start_matches('*');
                Some((node_kind.to_string(), format!("{receiver_type}.{method_name}")))
            }
            "type_spec" | "type_alias" => node
                .child_by_field_name("name")
                .filter(|n| n.kind() == "type_identifier")
                .and_then(|n| n.utf8_text(bytes).ok())
                .map(|name| (node_kind.to_string(), name.to_string())),
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
                            // Strip type params for key stability: `List<String>` → `List`
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

                Some((node_kind.to_string(), format!("{}{}{}", receiver_prefix, func_name, param_sig)))
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
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::code::{Code, Language};
    use crate::test::helper;

    use super::*;

    fn collect_matches(src: &str) -> Vec<(String, String)> {
        let code = Code::from_string(src, &Language::Rust);
        let ast = code.ast.as_ref().expect("AST should parse");
        let root = ast.root_node();
        let mut cursor = root.walk();
        root.children(&mut cursor)
            .filter_map(|child| node_matches(&child, &Language::Rust, &code))
            .collect()
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

            // The root node is in principle NOT a semantically structural node.
            // This is because it doesn't actually change the semantic of the code in any way.
            assert!(
                node_matches(&root_node, language, code).is_none(),
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

        fn collect_all(node: tree_sitter::Node, lang: &Language, code: &Code) -> Vec<(String, String)> {
            let mut out = Vec::new();
            if let Some(m) = node_matches(&node, lang, code) { out.push(m); }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                out.extend(collect_all(child, lang, code));
            }
            out
        }
        let matches = collect_all(ast.root_node(), &Language::Go, &code);

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
        assert!(node_matches(&ast.root_node(), &Language::Go, &code).is_none());
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

        // Collect matches from all nodes (DFS), as metadata.rs does
        fn collect_all(node: tree_sitter::Node, lang: &Language, code: &Code) -> Vec<(String, String)> {
            let mut out = Vec::new();
            if let Some(m) = node_matches(&node, lang, code) {
                out.push(m);
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                out.extend(collect_all(child, lang, code));
            }
            out
        }
        let matches = collect_all(ast.root_node(), &Language::Python, &code);

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
        assert!(node_matches(&ast.root_node(), &Language::Python, &code).is_none());
    }

    #[test]
    fn python_methods_in_class_are_pre_matched() {
        use crate::diff::{ASTDiff, NodeCache};
        use crate::diff::solve_semantically_structural_nodes::solve;

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

        // Both methods should be matched.
        let matched_names: Vec<&str> = ["add", "subtract"].iter().copied().filter(|&name| {
            let bm = before.metadata.ast_metadata.as_ref().unwrap();
            let am = after.metadata.ast_metadata.as_ref().unwrap();
            let bk = ("function_definition".to_string(), name.to_string());
            let ak = ("function_definition".to_string(), name.to_string());
            bm.semantically_structural_nodes.get(&bk)
                .and_then(|&bid| am.semantically_structural_nodes.get(&ak).map(|&aid| (bid, aid)))
                .map_or(false, |(bid, aid)| diff.mapping.contains_key(&(bid, aid)))
        }).collect();
        assert_eq!(matched_names.len(), 2, "both methods should be matched; got {matched_names:?}");
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

        fn collect_all(node: tree_sitter::Node, lang: &Language, code: &Code) -> Vec<(String, String)> {
            let mut out = Vec::new();
            if let Some(m) = node_matches(&node, lang, code) { out.push(m); }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                out.extend(collect_all(child, lang, code));
            }
            out
        }
        let matches = collect_all(ast.root_node(), &Language::Go, &code);

        for (kind, name) in &[
            ("type_alias", "MyInt"),
            ("type_spec", "Stringer"),
        ] {
            assert!(
                matches.iter().any(|(k, n)| k == kind && n == name),
                "missing ({kind}, {name}) in {matches:?}"
            );
        }
    }

    fn collect_all_kotlin(src: &str) -> Vec<(String, String)> {
        let code = Code::from_string(src, &Language::Kotlin);
        let ast = code.ast.as_ref().expect("AST should parse");
        fn walk(node: tree_sitter::Node, lang: &Language, code: &Code) -> Vec<(String, String)> {
            let mut out = Vec::new();
            if let Some(m) = node_matches(&node, lang, code) {
                out.push(m);
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                out.extend(walk(child, lang, code));
            }
            out
        }
        walk(ast.root_node(), &Language::Kotlin, &code)
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
            matches.iter().any(|(k, n)| k == "companion_object" && n == "Factory"),
            "missing (companion_object, Factory) in {matches:?}"
        );
    }

    #[test]
    fn kotlin_methods_in_class_are_pre_matched() {
        use crate::diff::{ASTDiff, NodeCache};
        use crate::diff::solve_semantically_structural_nodes::solve;

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

        let matched_names: Vec<&str> = ["add(Int,Int)", "subtract(Int,Int)"]
            .iter()
            .copied()
            .filter(|&name| {
                let bm = before.metadata.ast_metadata.as_ref().unwrap();
                let am = after.metadata.ast_metadata.as_ref().unwrap();
                let key = ("function_declaration".to_string(), name.to_string());
                bm.semantically_structural_nodes
                    .get(&key)
                    .and_then(|&bid| am.semantically_structural_nodes.get(&key).map(|&aid| (bid, aid)))
                    .map_or(false, |(bid, aid)| diff.mapping.contains_key(&(bid, aid)))
            })
            .collect();
        assert_eq!(matched_names.len(), 2, "both methods should be pre-matched; got {matched_names:?}");
    }
}
