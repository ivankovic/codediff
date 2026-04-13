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

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;
use tempfile::tempdir;
use tree_sitter::Parser as TSParser;

use codediff::code::Code;
use codediff::code::language::to_treesitter;

/// Command line arguments for the ASCII visualizer
#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Visualize TreeSitter AST for code files",
    long_about = "This tool reads a code file, parses it using TreeSitter, and displays the Abstract Syntax Tree in ASCII format."
)]
struct Args {
    /// Path to the file to visualize
    #[arg(value_name = "FILE")]
    file_path: PathBuf,
}

/// Parse the code and get the TreeSitter AST
fn get_ast(code: &Code) -> Result<tree_sitter::Tree> {
    let language = code
        .metadata
        .language
        .as_ref()
        .context("Code has no language metadata")?;

    let ts_language = to_treesitter(language).context("Language not supported by TreeSitter")?;
    let mut parser = TSParser::new();
    parser.set_language(&ts_language)?;

    let tree = parser
        .parse(&code.contents, None)
        .context("Failed to parse code")?;

    Ok(tree)
}

/// Print the ASCII tree representation of the AST
fn print_ast_tree(node: tree_sitter::Node, indent: usize) -> usize {
    let indent_str = "  ".repeat(indent);

    println!("{}{} - {}", indent_str, node.kind(), node.id());

    let mut cursor = node.walk();
    let mut child_count = 0;
    for child in node.children(&mut cursor) {
        child_count += print_ast_tree(child, indent + 1);
    }

    child_count + 1
}

/**
* This is a helper binary that can visualize Code objects in ASCII.
*
* TODO: Make it also visualize Diff objects.
*/
fn main() -> Result<()> {
    let args = Args::parse();

    // Check if the file path ends with ".test"
    let (file_path, _temp_dir) = if args.file_path.to_string_lossy().ends_with(".test") {
        // Create a temporary directory
        let temp_dir = tempdir()?;
        let temp_dir_path = temp_dir.path();

        // Create filename with ".test" removed
        let original_filename = args.file_path.file_name().unwrap().to_string_lossy();
        let new_filename = original_filename.trim_end_matches(".test");
        let temp_path = temp_dir_path.join(new_filename);

        // Read the original file content
        let original_content = std::fs::read_to_string(&args.file_path).map_err(|e| {
            anyhow::anyhow!("Failed to read file {}: {}", args.file_path.display(), e)
        })?;

        // Write to the temporary file
        std::fs::write(&temp_path, original_content)
            .map_err(|e| anyhow::anyhow!("Failed to write to temp file: {}", e))?;

        (temp_path, Some(temp_dir))
    } else {
        (args.file_path, None)
    };

    // Create Code object from file
    let code = Code::from_file(&file_path)?;

    println!("Visualizing AST for: {}", file_path.display());
    println!("Language: {:?}", code.metadata.language);
    println!("File size: {} bytes", code.contents.len());
    println!("\nAST Tree:");

    // Get AST and print tree
    let tree = get_ast(&code)?;
    let root_node = tree.root_node();
    let total_nodes = print_ast_tree(root_node, 0);

    println!("\nTotal nodes: {}", total_nodes);

    // The temp_dir will be dropped here, cleaning up automatically
    Ok(())
}
