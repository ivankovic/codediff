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
use tree_sitter::Parser as TSParser;

use codediff::code::{Code, from_file};
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
fn print_ast_tree(node: tree_sitter::Node, contents: &[u8], indent: usize) {
    let indent_str = "  ".repeat(indent);
    let node_type = node.kind();
    let node_text = node.utf8_text(contents).unwrap_or("<invalid utf8>");

    println!(
        "{}{} [{}:{}-{}:{}] - {} ({} chars)",
        indent_str,
        node_type,
        node.start_position().row + 1,
        node.start_position().column + 1,
        node.end_position().row + 1,
        node.end_position().column + 1,
        node_text,
        node.end_byte() - node.start_byte()
    );

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        print_ast_tree(child, contents, indent + 1);
    }
}

/**
* This is a helper binary that can visualize Code objects in ASCII.
*
* TODO: Make it also visualize Diff objects.
*/
fn main() -> Result<()> {
    let args = Args::parse();

    // Create Code object from file
    let code = from_file(&args.file_path)?;

    println!("Visualizing AST for: {}", args.file_path.display());
    println!("Language: {:?}", code.metadata.language);
    println!("File size: {} bytes", code.contents.len());
    println!("\nAST Tree:");

    // Get AST and print tree
    let tree = get_ast(&code)?;
    let root_node = tree.root_node();
    let contents_bytes = code.contents.as_bytes();
    print_ast_tree(root_node, contents_bytes, 0);

    Ok(())
}
