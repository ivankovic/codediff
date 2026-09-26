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
pub mod git;
pub mod license;
pub mod sampling;

static AUTO_GENERATED_RE: OnceLock<Regex> = OnceLock::new();

/// All metadata that is not used in normal functionality of the system but is instead used for
/// testing, research and planning.
#[derive(Debug, Clone, Default)]
pub struct CodeStats {
    pub code: Code,

    // Boolean stats.
    pub automatically_generated: bool,

    // Numerical stats.
    pub ast_nodes: usize,
    pub lines_of_code: u64,
    pub bytes: u64,

    /// Keyed by tree-sitter node kind.
    pub kind_stats: std::collections::HashMap<String, KindStats>,

    // Errors.
    pub failed_to_convert_to_utf8: bool,
    pub failed_to_parse: bool,
    /// Always false: there is no size cap. Kept so existing databases still load.
    pub too_large_to_parse: bool,
}

/// One node kind's stats within one file: its count, and a histogram of its subtree sizes keyed by
/// `size.ilog2()` (bucket B covers `[2^B, 2^(B+1))`), small enough to store per file.
#[derive(Debug, Clone, Default)]
pub struct KindStats {
    pub count: u64,
    pub subtree_size_histogram: std::collections::HashMap<u32, u64>,
}

/// Statistics about a diff between two versions of a file.
#[derive(Debug, Clone, Default)]
pub struct DiffStats {
    pub commit_id: String,
    pub relative_file_path: String,

    pub before: Option<Code>,
    pub after: Option<Code>,

    pub git_reported_status: String,

    pub bytes_before: u64,
    pub bytes_after: u64,

    pub lines_before: u64,
    pub lines_after: u64,

    pub nodes_before: u64,
    pub nodes_after: u64,

    pub unix_diff_script_bytes: u64,

    pub lines_added: u64,
    pub lines_removed: u64,
    pub lines_changed: u64,

    pub nodes_added: u64,
    pub nodes_removed: u64,
    pub nodes_changed: u64,
}

/// Count the nodes in a TreeSitter tree, the root included.
///
/// Iterative, like [`visit_for_kind_stats`]: tree-sitter parses trees nested deep enough (minified
/// bundles, data literals) to overflow the stack of a recursive walk.
pub fn count_nodes(root: Node) -> usize {
    let mut count = 0;
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        count += 1;
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    count
}

/// Per-kind [`KindStats`] for every node in the tree, plus the total node count, in one traversal.
pub fn compute_kind_stats(root: Node) -> (std::collections::HashMap<String, KindStats>, usize) {
    let mut stats = std::collections::HashMap::new();
    let total_nodes = visit_for_kind_stats(root, &mut stats);
    (stats, total_nodes)
}

/// Returns `root`'s subtree size.
///
/// Iterative post-order (see [`count_nodes`]): list nodes in pre-order with their parent's index,
/// then walk the list backwards, which reaches each node after all its descendants.
fn visit_for_kind_stats(
    root: Node,
    stats: &mut std::collections::HashMap<String, KindStats>,
) -> usize {
    let mut order: Vec<(Node, Option<usize>)> = Vec::new();
    let mut stack: Vec<(Node, Option<usize>)> = vec![(root, None)];
    while let Some((node, parent)) = stack.pop() {
        let index = order.len();
        order.push((node, parent));
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor).map(|child| (child, Some(index))));
    }

    let mut sizes = vec![1usize; order.len()];
    for i in (0..order.len()).rev() {
        let (node, parent) = order[i];
        let size = sizes[i];
        let entry = stats.entry(node.kind().to_string()).or_default();
        entry.count += 1;
        let bucket = size.ilog2();
        *entry.subtree_size_histogram.entry(bucket).or_insert(0) += 1;
        if let Some(parent) = parent {
            sizes[parent] += size;
        }
    }
    sizes[0]
}

/// Returns true if the code looks like it was automatically generated instead
/// of being human written.
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

/// How long one file may take to parse before the statistics give up on it: far above what any
/// well-formed file needs.
const PARSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Expand existing statistics by parsing the code and processing the AST.
pub fn expand_from_code(stats: &mut CodeStats, parser: &mut TSParser) -> Result<()> {
    match &stats.code.metadata.tip {
        Some(tip) => match tip {
            code::Type::Data(_) | code::Type::Documentation(_) => {
                return Err(anyhow!(
                    "Can't compute statistics for non-code pretending to be code"
                ));
            }
            _ => {}
        },
        None => {
            return Err(anyhow!(
                "Can't compute statistics for code that has no type"
            ));
        }
    }
    stats.bytes = stats.code.contents.len() as u64;

    // No size cap: the diff parses files of any size, so the statistics must too, or the largest
    // files drop out of every AST-node figure.
    if let Some(language) = &stats.code.metadata.language {
        // A known language without a grammar gets no AST statistics.
        if let Some(language) = language::to_treesitter(language) {
            parser.set_language(&language)?;

            if is_generated(&stats.code.contents) {
                stats.automatically_generated = true;
                return Ok(());
            }

            stats.lines_of_code = stats.code.contents.matches('\n').count() as u64;

            // Bounded: a file that is not the language its extension says (a `.h` that is one
            // big byte array) keeps tree-sitter in error recovery, its super-linear path, for
            // hours. Past the budget it counts as failed to parse.
            let contents = stats.code.contents.as_bytes();
            let started = std::time::Instant::now();
            let mut give_up = |_: &tree_sitter::ParseState| started.elapsed() > PARSE_TIMEOUT;
            let options = tree_sitter::ParseOptions::new().progress_callback(&mut give_up);
            let parsed = parser.parse_with_options(
                &mut |offset, _| &contents[offset.min(contents.len())..],
                None,
                Some(options),
            );
            match parsed {
                Some(tree) => {
                    let (kind_stats, total_nodes) = compute_kind_stats(tree.root_node());
                    stats.ast_nodes = total_nodes;
                    stats.kind_stats = kind_stats;
                }
                None => {
                    stats.failed_to_parse = true;
                    eprintln!(
                        "Parse gave up after {}s: {:?}",
                        PARSE_TIMEOUT.as_secs(),
                        stats.code.metadata.path
                    );
                }
            }
        }
    } else {
        return Err(anyhow!(
            "Can't compute statistics for code that has no language"
        ));
    };

    Ok(())
}

/// Generate statistics for the given path. Infallible: failures are recorded in `CodeStats`' error
/// fields.
pub fn for_path(path: &std::path::Path, parser: &mut TSParser) -> CodeStats {
    let mut stats = CodeStats {
        ..Default::default()
    };

    // Classify from the path first, so data files are never read.
    stats.code.metadata.path = Some(std::path::PathBuf::from(path));
    metadata::hermetic_expand(&mut stats.code.metadata);

    // Only what the path classifies as code or configuration is read; nothing is classified by
    // content.
    if !matches!(
        stats.code.metadata.tip,
        Some(code::Type::Code(_)) | Some(code::Type::Configuration(_))
    ) {
        return stats;
    }

    let mut f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to open file {:?}: {:?}", path, e);
            return stats;
        }
    };

    if let Err(e) = f.read_to_string(&mut stats.code.contents) {
        eprintln!("Failed to read file {:?}: {:?}", path, e);
        return stats;
    }

    if let Err(e) = expand_from_code(&mut stats, parser) {
        eprintln!("Failed to compute statistics: {:?}", e);
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
    fn deeply_nested_trees_are_walked_without_recursion() {
        // 50,000 nested JSON arrays: tree-sitter parses it, and a walk that recursed once per
        // level would overflow a test thread's stack long before the bottom.
        let depth = 50_000;
        let source = format!("{}{}", "[".repeat(depth), "]".repeat(depth));
        let code = crate::code::Code::from_string(&source, &crate::code::Language::JSON);
        let root = code.ast.as_ref().expect("json parses").root_node();
        let counted = count_nodes(root);
        let (kinds, total) = compute_kind_stats(root);
        assert_eq!(counted, total);
        assert!(total >= 2 * depth, "{total} nodes for {depth} levels");
        assert_eq!(kinds.values().map(|k| k.count).sum::<u64>(), total as u64);
    }

    #[test]
    fn kind_stats_bucket_subtree_sizes_by_log2() {
        // array = `[`, number, `,`, number, `]` plus itself: size 6, bucket 2 ([4, 8)).
        let code = crate::code::Code::from_string("[1, 2]", &crate::code::Language::JSON);
        let (kinds, _) = compute_kind_stats(code.ast.as_ref().unwrap().root_node());
        let number = &kinds["number"];
        assert_eq!(number.count, 2);
        assert_eq!(number.subtree_size_histogram.get(&0), Some(&2));
        let array = &kinds["array"];
        assert_eq!(array.count, 1);
        assert_eq!(array.subtree_size_histogram.get(&2), Some(&1));
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
