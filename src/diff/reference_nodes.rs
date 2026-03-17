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

use crate::code::Language;

/**
 * Determines if a node with the given kind is a reference node for the given language.
 *
 * Reference nodes are nodes that are used as anchors for diffing. Typically these are
 * nodes that represent significant structural elements in the code, such as source files
 * or function definitions.
 *
 * @param node_kind The kind of the node (e.g., "source_file", "function_item")
 * @param language The programming language
 * @return true if the node is a reference node, false otherwise
 */
pub fn is_reference_node(node_kind: &str, language: &Language) -> bool {
    // Common reference nodes across many languages
    if node_kind == "source_file" {
        return true;
    }

    // Language-specific reference nodes
    match language {
        Language::Rust => node_kind == "function_item",
        // Add other languages as needed
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::test::helper;

    use super::*;

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
                is_reference_node(root_node_kind, language),
                "Root node '{}' should be a reference node for language {:?} in file {}",
                root_node_kind,
                language,
                filename
            );
        }

        Ok(())
    }
}
