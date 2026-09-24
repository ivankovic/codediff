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

use codediff::code::Code;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Visualize TreeSitter AST for code files",
    long_about = "This tool reads a code file, parses it using TreeSitter, and displays the Abstract Syntax Tree in ASCII format."
)]
struct Args {
    /// File to parse; a trailing `.test` is ignored when detecting the language.
    #[arg(value_name = "FILE")]
    file_path: PathBuf,
}

/// Prints `node`'s subtree, one node per line, and returns how many nodes it printed.
fn print_ast_tree(node: tree_sitter::Node, indent: usize, code: &Code) -> usize {
    let indent_str = "  ".repeat(indent);

    println!(
        "{}{} - {} - {:?}",
        indent_str,
        node.kind(),
        node.id(),
        node.utf8_text(code.contents.as_bytes())
    );

    let mut cursor = node.walk();
    let mut child_count = 0;
    for child in node.children(&mut cursor) {
        child_count += print_ast_tree(child, indent + 1, code);
    }

    child_count + 1
}

// TODO: also visualize Diff objects.
fn main() -> Result<()> {
    let args = Args::parse();

    // Language detection goes by extension, so a `foo.rs.test` fixture is copied to `foo.rs` first.
    let (file_path, _temp_dir) = if args.file_path.to_string_lossy().ends_with(".test") {
        let temp_dir = tempdir()?;
        let temp_dir_path = temp_dir.path();

        let original_filename = args.file_path.file_name().unwrap().to_string_lossy();
        let new_filename = original_filename.trim_end_matches(".test");
        let temp_path = temp_dir_path.join(new_filename);

        let original_content = std::fs::read_to_string(&args.file_path).map_err(|e| {
            anyhow::anyhow!("Failed to read file {}: {}", args.file_path.display(), e)
        })?;

        std::fs::write(&temp_path, original_content)
            .map_err(|e| anyhow::anyhow!("Failed to write to temp file: {}", e))?;

        (temp_path, Some(temp_dir))
    } else {
        (args.file_path, None)
    };

    let code = Code::from_file(&file_path)?;

    println!("Visualizing AST for: {}", file_path.display());
    println!("Language: {:?}", code.metadata.language);
    println!("File size: {} bytes", code.contents.len());
    println!("\nAST Tree:");

    let tree = code.ast.as_ref().context("Code has no parsed AST")?;
    let root_node = tree.root_node();
    let total_nodes = print_ast_tree(root_node, 0, &code);

    println!("\nTotal nodes: {}", total_nodes);

    Ok(())
}
