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
* You can think of reference nodes as "parts of code humans think about". I.e. humans rarely think
* about a specific semicolon in a C++ file, but they do think about entire functions as whole
* entities.
*/
pub fn node_matches(node_kind: &str, language: &Language) -> bool {
    // Language-specific reference nodes
    match language {
        Language::Rust => {
            node_kind == "source_file" ||  // The root node of any sopurce file
            node_kind == "function_item" ||
            node_kind == "impl_item" ||
            node_kind == "struct_item" ||
            node_kind == "use_declaration"
        }
        Language::Python => node_kind == "module", // Python modules/source files
        Language::Java => node_kind == "program", // Java source files are represented as "program" nodes
        Language::C => node_kind == "translation_unit", // C source files use "translation_unit" as root
        Language::CPP => node_kind == "translation_unit", // C++ also uses "translation_unit" as root
        Language::Go => node_kind == "source_file", // Go source files use "source_file" as root
        Language::JavaScript => node_kind == "program", // JavaScript programs use "program" as root
        Language::TypeScript => node_kind == "program", // TypeScript also uses "program" as root
        Language::TSX => node_kind == "program",    // TSX also uses "program" as root
        Language::PHP => node_kind == "program",    // PHP also uses "program" as root
        Language::Ruby => node_kind == "program",   // Ruby also uses "program" as root
        Language::R => node_kind == "program",      // R also uses "program" as root
        Language::Swift => node_kind == "source_file", // Swift source files use "source_file" as root
        Language::Kotlin => node_kind == "source_file", // Kotlin source files use "source_file" as root
        Language::Scala => node_kind == "compilation_unit", // Scala source files use "compilation_unit" as root
        Language::CSharp => node_kind == "compilation_unit", // C# source files use "compilation_unit" as root
        Language::HTML => node_kind == "document", // HTML documents use "document" as root
        Language::CSS => node_kind == "stylesheet", // CSS stylesheets use "stylesheet" as root
        Language::LUA => node_kind == "chunk",     // Lua chunks use "chunk" as root
        Language::Vimscript => node_kind == "script_file", // Vimscript files use "script_file" as root
        Language::ShellScript => node_kind == "program",   // Shell scripts use "program" as root
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
                node_matches(root_node_kind, language),
                "Root node '{}' should be a reference node for language {:?} in file {}",
                root_node_kind,
                language,
                filename
            );
        }

        Ok(())
    }
}
