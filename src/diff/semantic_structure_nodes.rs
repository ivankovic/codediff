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
}
