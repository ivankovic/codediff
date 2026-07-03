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
use anyhow::{Result, anyhow};
use regex::Regex;
use std::io::Read;
use std::sync::OnceLock;
use tree_sitter::{Node, Parser as TSParser};

use crate::code::language;
use crate::code::metadata;
use crate::code::{self, Code};

pub mod filesystem;

static AUTO_GENERATED_RE: OnceLock<Regex> = OnceLock::new();

/**
* All metadata that is not used in normal functionality of the system but is instead used for
* testing, research and planning.
*/
#[derive(Debug, Clone, Default)]
pub struct CodeStats {
    pub code: Code,

    // Boolean stats.
    pub automatically_generated: bool,

    // Numerical stats.
    pub ast_nodes: usize,
    pub lines_of_code: u64,
    pub bytes: u64,

    /// Per-node-kind stats (occurrence count and subtree-size histogram), keyed by TreeSitter
    /// node kind (e.g. "function_item", "if_expression"). See [`KindStats`].
    pub kind_stats: std::collections::HashMap<String, KindStats>,

    // Errors.
    pub failed_to_convert_to_utf8: bool,
    pub failed_to_parse: bool,
    pub too_large_to_parse: bool,
}

/**
* Aggregated stats for a single TreeSitter node kind within one file: how many nodes of that kind
* exist, and a log2-bucketed histogram of the size (node count) of the subtree rooted at each one.
*
* The subtree-size histogram uses `size.ilog2()` as the bucket key: bucket 0 is subtree size 1
* (a leaf of this kind), bucket 1 is sizes 2-3, bucket 2 is sizes 4-7, and so on - bucket B covers
* `[2^B, 2^(B+1))`. This keeps the histogram small (one entry per size *order of magnitude* seen,
* not one per exact size) while still letting downstream analysis approximate percentiles or
* answer "what fraction of `for_expression` subtrees are bigger than N nodes" questions.
*/
#[derive(Debug, Clone, Default)]
pub struct KindStats {
    pub count: u64,
    pub subtree_size_histogram: std::collections::HashMap<u32, u64>,
}

/**
* Statistics about a diff between two versions of a file.
*/
#[derive(Debug, Clone, Default)]
pub struct DiffStats {
    pub commit_id: String,
    pub relative_file_path: String,

    // Code objects for before and after versions
    pub before: Option<Code>,
    pub after: Option<Code>,

    pub git_reported_status: String,

    // Absolute values
    pub bytes_before: u64,
    pub bytes_after: u64,

    pub lines_before: u64,
    pub lines_after: u64,

    pub nodes_before: u64,
    pub nodes_after: u64,

    // Measures of the difference
    pub unix_diff_script_bytes: u64,

    pub lines_added: u64,
    pub lines_removed: u64,
    pub lines_changed: u64,

    pub nodes_added: u64,
    pub nodes_removed: u64,
    pub nodes_changed: u64,
}

/**
* Count the nodes in a TreeSitter tree.
*/
pub fn count_nodes(root: Node) -> usize {
    let mut count = 1;
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        count += count_nodes(child);
    }

    count
}

/**
* Compute per-kind occurrence counts and subtree-size histograms for every node in a TreeSitter
* tree, in a single post-order traversal. See [`KindStats`] for the histogram bucketing scheme.
*/
pub fn compute_kind_stats(root: Node) -> std::collections::HashMap<String, KindStats> {
    let mut stats = std::collections::HashMap::new();
    visit_for_kind_stats(root, &mut stats);
    stats
}

/// Returns the subtree size (including `node` itself) so the caller can bucket it.
fn visit_for_kind_stats(node: Node, stats: &mut std::collections::HashMap<String, KindStats>) -> usize {
    let mut size = 1;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        size += visit_for_kind_stats(child, stats);
    }

    let entry = stats.entry(node.kind().to_string()).or_default();
    entry.count += 1;
    let bucket = size.ilog2();
    *entry.subtree_size_histogram.entry(bucket).or_insert(0) += 1;

    size
}

/**
* Returns true if the code looks like it was automatically generated instead
* of being human written.
*/
pub fn is_generated(code: &str) -> bool {
    let re = AUTO_GENERATED_RE.get_or_init(|| {
        Regex::new(indoc::indoc!(r#"
        (?imx)
        ^\s*
        (?:(?://|/\*+|\#|;)\s*)?   # optional comment prefix
        .*?                        # anything on the line
        (?:
            @generated\b |
            auto(?:matically)?\s+generated\b |
            this\s+(?:file\s+)?(?:was|is)\s+(?:an?\s+)?auto(?:matically)?\s+generated(?:\s+file)?\b |
            code\s+generated\s+by\b |
            generated\s+by\b |
            <auto-generated> |
            do\s+not\s+edit(?:\s+this\s+file)?\b
        )
    "#))
            .unwrap()
    });

    code.lines().take(50).any(|line| re.is_match(line))
}

/**
* Expand existing statistics by parsing the code and processing the AST.
*/
pub fn expand_from_code(stats: &mut CodeStats, parser: &mut TSParser) -> Result<()> {
    match &stats.code.metadata.tip {
        Some(tip) => {
            match tip {
                code::Type::Data(_) | code::Type::Documentation(_) => {
                    return Err(anyhow!(
                        "Can't compute statistics for non-code pretending to be code"
                    ));
                }
                _ => {
                    // This is fine, proceed.
                }
            }
        }
        None => {
            return Err(anyhow!(
                "Can't compute statistics for code that has no type"
            ));
        }
    }
    stats.bytes = stats.code.contents.len() as u64;

    if stats.code.contents.len() > 1024 * 1024 {
        stats.too_large_to_parse = true;
        return Ok(());
    }

    if let Some(language) = &stats.code.metadata.language {
        match language::to_treesitter(language) {
            Some(language) => {
                parser.set_language(&language)?;

                if is_generated(&stats.code.contents) {
                    stats.automatically_generated = true;
                    return Ok(());
                }

                stats.lines_of_code = stats.code.contents.matches('\n').count() as u64;

                match parser.parse(&stats.code.contents, None) {
                    Some(tree) => {
                        stats.ast_nodes = count_nodes(tree.root_node());
                        stats.kind_stats = compute_kind_stats(tree.root_node());
                    }
                    None => stats.failed_to_parse = true,
                }
            }
            None => {
                // We know what the langauge is, but we don't support TreeSitter parsing for this file.
            }
        }
    } else {
        // The language is not set...
        return Err(anyhow!(
            "Can't compute statistics for code that has no language"
        ));
    };

    Ok(())
}

/**
* Generate statistics for the given path.
*
* Note that this function cannot error out because the CodeStats object must itself contain any
* relevant error fields.
*/
pub fn for_path(path: &std::path::Path, parser: &mut TSParser) -> CodeStats {
    let mut stats = CodeStats {
        ..Default::default()
    };

    // First we try to get as much metadata as possible from the path alone.
    // This makes the code much faster because we can avoid reading data files.
    stats.code.metadata.path = Some(std::path::PathBuf::from(path));
    metadata::hermetic_expand(&mut stats.code.metadata);

    match stats.code.metadata.tip.as_ref() {
        // We only read code and config files, to improve performance.
        Some(code::Type::Code(_)) | Some(code::Type::Configuration(_)) => {
            let f = std::fs::File::open(path);
            match f {
                Ok(mut f) => match f.read_to_string(&mut stats.code.contents) {
                    Ok(_) => {
                        if let Err(e) = expand_from_code(&mut stats, parser) {
                            eprintln!("Failed to compute statistics: {:?}", e);
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to read file {:?}: {:?}", path, e);
                    }
                },
                Err(e) => {
                    eprintln!("Failed to open file {:?}: {:?}", path, e);
                }
            }
        }
        None => {
            // We were not able to determine the type from the path.
            // TODO: Fold this up and read the file and try to determine the type based on
            // contents.
        }
        _ => {
            // It's a data or documentation file, we don't need stats for those.
        }
    }

    stats
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_generated_files() {
        assert!(is_generated(
            r"//
// This is an automatically generated file.
// Do not edit.
//

{
    {0x00000000}, {0x33800000}, {0x34000000}, {0x34400000}
};"
        ));
        assert!(!is_generated(""));
    }

    #[test]
    fn counts_lines_correctly() {
        let mut stats = CodeStats {
            code: Code {
                contents: "line 1\nline 2\nline 3".to_string(),
                metadata: crate::code::Metadata {
                    language: Some(crate::code::Language::Rust),
                    tip: Some(crate::code::Type::Code("rust".to_string())),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let mut parser = tree_sitter::Parser::new();
        expand_from_code(&mut stats, &mut parser).unwrap();

        // 3 lines needs 2 newlines if there is no trailing newline :)
        assert_eq!(stats.lines_of_code, 2);
    }
}
