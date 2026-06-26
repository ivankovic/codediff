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
            "function_item" => node
                .child_by_field_name("name")
                .filter(|n| n.kind() == "identifier")
                .and_then(|n| n.utf8_text(bytes).ok())
                .map(|name| (node_kind.to_string(), name.to_string())),
            "struct_item" | "enum_item" => node
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
            "type_spec" => node
                .child_by_field_name("name")
                .filter(|n| n.kind() == "type_identifier")
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
}
