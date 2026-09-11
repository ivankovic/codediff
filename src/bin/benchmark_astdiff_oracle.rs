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

//! Scores codediff's node mapping against an oracle nobody on this project wrote: the
//! Alikhanifard & Tsantalis AST node-mapping benchmark (TOSEM 2025, arXiv 2403.05939), Defects4J
//! half. `research/external/README.md` says what the data is and how to fetch it; this file says
//! how a JDT mapping and a tree-sitter mapping get compared at all, because that is where every
//! number this tool prints can go wrong.
//!
//! **The oracle's unit is a JDT node; ours is a tree-sitter node. The common ground is the span.**
//! A record `ExpressionStatement[3293-3364] : ExpressionStatement[3293-3364]` becomes the byte-span
//! pair `((3293,3364),(3293,3364))` after converting JDT's UTF-16 indices to bytes, and is
//! *resolved* if both files have a tree-sitter node with exactly that span. Both sides' mappings
//! are then plain sets of span pairs, so precision and recall are set arithmetic
//! (`TP = O ∩ C`, `FP = C \ O`, `FN = O \ C`), the same definition as the paper's Definition 5.1.
//! Records that do not resolve - JDT's synthetic `METHOD_INVOCATION_RECEIVER`, the `TextElement`s
//! inside a Javadoc that tree-sitter sees as one `block_comment` - are dropped from the oracle
//! set and counted, per JDT type, in the `--details` output. **Quote that resolution rate next
//! to any precision or recall from here**: it is the part of the oracle we could not ask.
//!
//! **Which of codediff's mappings are judged.** Codediff maps every node, including `;` tokens,
//! `modifiers` wrappers and other tree-sitter-only structure the oracle has no record of. Counting
//! those as false positives would measure the grammar, not the diff. A codediff pair is judged
//! when the oracle *could* have an opinion about it: its left span is one the oracle maps on the
//! left side, or its right span is one the oracle maps on the right side (`in-universe`), or its
//! node kind is one that, across the whole run, resolves against oracle records nearly every time
//! it appears (`whitelisted` - see `KindStats`). The second clause exists for the one error the
//! first cannot see: codediff pairing a deleted node with an inserted one where the oracle maps
//! neither. The whitelist is data-driven precisely so a kind like `modifiers` - which resolves
//! only when a declaration has a single modifier - stays out of it.
//!
//! **The exclusion rule.** The oracle lists *every* mapping, identical subtrees included (one
//! Joda-Time file has 11,087 records), and the paper excludes "all AST node mappings nested under
//! unchanged program elements" before computing anything, because every tool gets those right.
//! Implemented here as: a pair is excluded when the nearest enclosing program element (type,
//! method, constructor, field, import, ... - `PROGRAM_ELEMENT_KINDS`) on the left has byte-identical
//! text to the one on the right. Applied to both sets. A mapping with no enclosing element (the
//! root, a top-level comment) is kept; that is one or two guaranteed true positives per file and
//! the paper does the same.
//!
//! **Two granularities**, as in the paper's Tables 11 and 12: `statement` keeps only oracle
//! records whose JDT type is a statement, declaration or block (`is_statement_level`), `all` keeps
//! everything that resolves. Codediff's side is filtered to the same population through the
//! statement-level whitelist.
//!
//! Two passes over the data: the first resolves every oracle record to learn the kind whitelist,
//! the second scores. Parsing is cheap; the ~2.2 GB of oracle JSON is what costs, and it is read
//! twice rather than held in memory.

use anyhow::{Context, Result, bail};
use clap::Parser;
use codediff::code::{Code, Language};
use codediff::diff;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(
    about = "Score codediff against the Alikhanifard & Tsantalis AST node-mapping oracle (Defects4J half)"
)]
struct Args {
    /// RefactoringMiner sparse checkout holding src/test/resources/astDiff/ - written by
    /// research/external/fetch_astdiff_oracle.sh.
    #[arg(
        long,
        default_value = "/var/tmp/research/external/refactoringminer-astdiff"
    )]
    oracle: PathBuf,
    /// The GumTree Simple replication package's dataset/ folder, whose defects4j/{before,after}
    /// tree holds the files the oracle's offsets index into - written by
    /// research/external/fetch_gumtree_simple_package.sh.
    #[arg(
        long,
        default_value = "/var/tmp/research/external/gumtree-simple/dataset"
    )]
    sources: PathBuf,
    /// One row per (case, compilation unit).
    #[arg(
        long,
        default_value = "research/data/comparison/astdiff_oracle_defects4j.csv"
    )]
    csv: PathBuf,
    /// Only score these cases, as `Project-BugId` (e.g. `Time-20`), comma-separated.
    #[arg(long, value_delimiter = ',')]
    cases: Vec<String>,
    /// Print the unresolved-JDT-type census, the per-kind whitelist table, and every false
    /// positive / false negative with its source text.
    #[arg(long)]
    details: bool,
    /// Whitelist a node kind when at least this fraction of its occurrences (under changed
    /// program elements) resolve against an oracle record. See the module doc.
    #[arg(long, default_value_t = 0.95)]
    whitelist_ratio: f64,
    /// ... and only when at least this many occurrences were seen.
    #[arg(long, default_value_t = 50)]
    whitelist_min: usize,
}

/// One record of an oracle JSON file. The `firstLabel`/`secondLabel`/`*ParentType` fields are
/// left out deliberately: serde skips unknown fields, and the labels are the bulk of the 2.2 GB.
/// `secondType` is left out too: in every record checked it equals `firstType` (the oracle maps
/// same-typed nodes only), so the first type is *the* type of a record.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct OracleRecord {
    first_type: String,
    first_pos: usize,
    first_end_pos: usize,
    second_pos: usize,
    second_end_pos: usize,
}

#[derive(Deserialize, Debug)]
struct CaseId {
    repo: String,
    commit: String,
}

/// Byte span `(start, end)`, end exclusive - `tree_sitter::Node::byte_range()` flattened.
type Span = (usize, usize);
type SpanPair = (Span, Span);

/// Tree-sitter kinds that count as a "program element" for the exclusion rule. JDT's
/// `matchedElements` section - the paper's own notion - covers type, method, field and enum
/// declarations; imports and the package line are added because they are the other top-level
/// declarations every tool maps trivially when unchanged.
const PROGRAM_ELEMENT_KINDS: &[&str] = &[
    "class_declaration",
    "interface_declaration",
    "enum_declaration",
    "record_declaration",
    "annotation_type_declaration",
    "annotation_type_element_declaration",
    "method_declaration",
    "constructor_declaration",
    "compact_constructor_declaration",
    "field_declaration",
    "constant_declaration",
    "enum_constant",
    "static_initializer",
    "import_declaration",
    "package_declaration",
];

/// The paper's coarse granularity: "statement mappings" (its Table 11) against "statement and
/// sub-expression mappings" (Table 12). JDT names make the split mechanical: statements end in
/// `Statement`, declarations in `Declaration` (which sweeps in `SingleVariableDeclaration`, a
/// parameter - a declaration to JDT, so kept), plus the handful of block-like kinds that carry
/// statements. `VariableDeclarationFragment` is the name-and-initialiser half of a `int x = 1;`
/// and is sub-expression level; it ends in `Fragment`, so it falls out naturally.
fn is_statement_level(jdt_type: &str) -> bool {
    jdt_type.ends_with("Statement")
        || jdt_type.ends_with("Declaration")
        || matches!(
            jdt_type,
            "Block" | "CompilationUnit" | "SwitchCase" | "CatchClause" | "Initializer"
        )
}

/// Everything the scorer needs to know about one side of one file: the parsed code, a
/// span -> nodes index, and the UTF-16 -> byte table when the file is not ASCII.
struct Side {
    code: Code,
    /// Every node with this exact byte span. Usually one; `type_identifier` alone where JDT has
    /// `SimpleType` over `SimpleName`, or an anonymous `public` token under a one-modifier
    /// `modifiers` node, are the multi-node cases.
    by_span: HashMap<Span, Vec<usize>>,
    /// Node id -> (kind, span, parent id). Indexed once so the scorer never walks the tree again.
    nodes: HashMap<usize, NodeInfo>,
    /// `utf16_to_byte[i]` = byte offset of UTF-16 code unit `i`; `None` when the file is ASCII
    /// and the identity applies. Length is units + 1 so an end offset resolves too.
    utf16_to_byte: Option<Vec<usize>>,
    /// Spans of every `block_comment` / `line_comment`, sorted by start - to tell an oracle
    /// record that lives *inside* a Javadoc (JDT parses those; tree-sitter does not) from one
    /// that failed to resolve for a reason worth looking at.
    comments: Vec<Span>,
}

#[derive(Clone, Debug)]
struct NodeInfo {
    kind: &'static str,
    span: Span,
    parent: Option<usize>,
}

impl Side {
    fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        // JDT read these files as text and reports `String` indices; if the bytes are not valid
        // UTF-8 there is no way to know what it saw, so refuse rather than guess with a lossy
        // conversion that would shift every offset after the first bad byte.
        let contents = String::from_utf8(bytes)
            .map_err(|_| anyhow::anyhow!("{} is not valid UTF-8", path.display()))?;
        let utf16_to_byte = if contents.is_ascii() {
            None
        } else {
            let mut table = Vec::with_capacity(contents.len() + 1);
            for (byte, ch) in contents.char_indices() {
                for _ in 0..ch.len_utf16() {
                    table.push(byte);
                }
            }
            table.push(contents.len());
            Some(table)
        };

        let mut code = Code::from_string(&contents, &Language::Java);
        code.ensure_parsed()
            .with_context(|| format!("parsing {}", path.display()))?;

        let mut by_span: HashMap<Span, Vec<usize>> = HashMap::new();
        let mut nodes = HashMap::new();
        {
            let tree = code.ast.as_ref().context("no AST after ensure_parsed")?;
            let mut stack = vec![(tree.root_node(), None)];
            while let Some((node, parent)) = stack.pop() {
                let span = (node.start_byte(), node.end_byte());
                by_span.entry(span).or_default().push(node.id());
                nodes.insert(
                    node.id(),
                    NodeInfo {
                        kind: node.kind(),
                        span,
                        parent,
                    },
                );
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    stack.push((child, Some(node.id())));
                }
            }
        }
        let mut comments: Vec<Span> = nodes
            .values()
            .filter(|n| matches!(n.kind, "block_comment" | "line_comment"))
            .map(|n| n.span)
            .collect();
        comments.sort_unstable();
        Ok(Side {
            code,
            by_span,
            nodes,
            utf16_to_byte,
            comments,
        })
    }

    /// The tree-sitter nodes standing for a JDT node with this byte span, and the span they
    /// actually have. Exact match first; then the handful of systematic ways JDT and
    /// tree-sitter-java draw the same node's boundary differently, each tried in turn:
    ///
    /// * **A body declaration starts at its Javadoc in JDT.** `MethodDeclaration[100-900]` where
    ///   bytes 100..350 are `/** ... */` is tree-sitter's `method_declaration` at 350..900 (after
    ///   any further comments and whitespace). Without this, every documented method, field and
    ///   type is unresolvable - 315 methods in a four-file probe.
    /// * **Trailing `;` / `:`.** JDT's `VariableDeclarationExpression` (a `for` initialiser) stops
    ///   before the `;` that tree-sitter's `local_variable_declaration` includes; JDT's
    ///   `SwitchCase` includes the `:` that tree-sitter's `switch_label` stops before.
    /// * **`METHOD_INVOCATION_ARGUMENTS`** is JDT's synthetic span over the arguments alone;
    ///   tree-sitter's `argument_list` includes the parentheses.
    ///
    /// Each tolerance widens or narrows by exactly the delimiter it names and only when that
    /// delimiter is there in the text, so it cannot resolve a span to an unrelated node.
    fn resolve(&self, start: usize, end: usize) -> Option<(Span, &[usize])> {
        let bytes = self.code.contents.as_bytes();
        if start > end || end > bytes.len() {
            return None;
        }
        let mut candidates: Vec<Span> = vec![(start, end)];
        let trimmed = self.skip_leading_comments(start, end);
        if trimmed != start {
            candidates.push((trimmed, end));
        }
        for &(s, e) in candidates.clone().iter() {
            if e < bytes.len() && matches!(bytes[e], b';' | b':') {
                candidates.push((s, e + 1));
            }
            if e > s && matches!(bytes[e - 1], b';' | b':') {
                candidates.push((s, e - 1));
            }
            if s > 0 && e < bytes.len() && bytes[s - 1] == b'(' && bytes[e] == b')' {
                candidates.push((s - 1, e + 1));
            }
        }
        candidates
            .into_iter()
            .find_map(|span| self.by_span.get(&span).map(|ids| (span, ids.as_slice())))
    }

    /// First byte in `start..end` that is not whitespace or inside a `/* */` or `//` comment.
    fn skip_leading_comments(&self, mut start: usize, end: usize) -> usize {
        let bytes = self.code.contents.as_bytes();
        loop {
            while start < end && bytes[start].is_ascii_whitespace() {
                start += 1;
            }
            let rest = &bytes[start..end];
            if rest.starts_with(b"/*") {
                match rest.windows(2).skip(2).position(|w| w == b"*/") {
                    Some(close) => start += close + 4,
                    None => return start,
                }
            } else if rest.starts_with(b"//") {
                match rest.iter().position(|&b| b == b'\n') {
                    Some(nl) => start += nl + 1,
                    None => return end,
                }
            } else {
                return start;
            }
        }
    }

    fn inside_comment(&self, span: Span) -> bool {
        // `comments` is sorted by start; the enclosing comment, if any, is the last one starting
        // at or before `span.0`.
        let idx = self.comments.partition_point(|c| c.0 <= span.0);
        idx > 0 && self.comments[idx - 1].1 >= span.1
    }

    fn byte_offset(&self, utf16: usize) -> Option<usize> {
        match &self.utf16_to_byte {
            None => (utf16 <= self.code.contents.len()).then_some(utf16),
            Some(table) => table.get(utf16).copied(),
        }
    }

    fn text(&self, span: Span) -> &str {
        &self.code.contents[span.0..span.1]
    }

    /// Nearest enclosing program element, the node itself included, or `None` above the
    /// outermost declaration (the root `program` node, top-level comments).
    fn enclosing_element(&self, mut id: usize) -> Option<Span> {
        loop {
            let info = self.nodes.get(&id)?;
            if PROGRAM_ELEMENT_KINDS.contains(&info.kind) {
                return Some(info.span);
            }
            id = info.parent?;
        }
    }
}

/// A pair is "under an unchanged program element" when both ends have an enclosing element and
/// those two elements are byte-identical.
fn under_unchanged_element(before: &Side, after: &Side, before_id: usize, after_id: usize) -> bool {
    match (
        before.enclosing_element(before_id),
        after.enclosing_element(after_id),
    ) {
        (Some(b), Some(a)) => before.text(b) == after.text(a),
        _ => false,
    }
}

/// Per-kind evidence for the judged-pair whitelist: how often a node of this kind (under a
/// changed program element, mapped by codediff to a real partner) had its span in the oracle's
/// universe. A kind whose ratio is high is one JDT models one-to-one; a kind whose ratio is low
/// is tree-sitter structure JDT slices differently, and a codediff mapping of it can never be
/// judged either way.
#[derive(Default, Debug, Clone)]
struct KindStats {
    seen: usize,
    in_universe: usize,
}

/// Everything learned about one compilation unit that both passes need.
struct FileResolution {
    /// Resolved oracle pairs, with the JDT type they came from (first type; the two always agree
    /// in this oracle) and the node ids each end resolved to.
    oracle: Vec<(SpanPair, String, usize, usize)>,
    /// How many records the JSON held, and which JDT types failed to resolve, with counts.
    records: usize,
    unresolved: BTreeMap<String, usize>,
    /// Every left span / right span the oracle maps, resolved or not - the "universe" a codediff
    /// pair is judged against. Indexed `[0]` for all records, `[1]` for statement-level ones.
    left_universe: [HashSet<Span>; 2],
    right_universe: [HashSet<Span>; 2],
}

fn resolve_oracle(json_path: &Path, before: &Side, after: &Side) -> Result<FileResolution> {
    let text =
        std::fs::read(json_path).with_context(|| format!("reading {}", json_path.display()))?;
    let records: Vec<OracleRecord> = serde_json::from_slice(&text)
        .with_context(|| format!("parsing {}", json_path.display()))?;

    let mut resolution = FileResolution {
        oracle: Vec::with_capacity(records.len()),
        records: records.len(),
        unresolved: BTreeMap::new(),
        left_universe: [HashSet::new(), HashSet::new()],
        right_universe: [HashSet::new(), HashSet::new()],
    };
    let mut seen: HashSet<SpanPair> = HashSet::new();
    for record in &records {
        let spans = (
            before.byte_offset(record.first_pos),
            before.byte_offset(record.first_end_pos),
            after.byte_offset(record.second_pos),
            after.byte_offset(record.second_end_pos),
        );
        let (Some(ls), Some(le), Some(rs), Some(re)) = spans else {
            *resolution
                .unresolved
                .entry(format!("{} (offset out of range)", record.first_type))
                .or_default() += 1;
            continue;
        };
        let left = before.resolve(ls, le);
        let right = after.resolve(rs, re);
        // The universe is compared against codediff's tree-sitter spans, so it holds the span a
        // record *resolved to*; an unresolved record contributes its raw span, which matches
        // nothing and is harmless.
        let pair: SpanPair = (
            left.map_or((ls, le), |(span, _)| span),
            right.map_or((rs, re), |(span, _)| span),
        );
        resolution.left_universe[0].insert(pair.0);
        resolution.right_universe[0].insert(pair.1);
        if is_statement_level(&record.first_type) {
            resolution.left_universe[1].insert(pair.0);
            resolution.right_universe[1].insert(pair.1);
        }
        let (Some((_, left_ids)), Some((_, right_ids))) = (left, right) else {
            let where_ = if before.inside_comment((ls, le)) {
                " (inside a comment)"
            } else {
                ""
            };
            *resolution
                .unresolved
                .entry(format!("{}{where_}", record.first_type))
                .or_default() += 1;
            continue;
        };
        // Several JDT records can collapse onto one span pair (`SimpleType` over `SimpleName`);
        // keep the first, they are the same claim.
        if !seen.insert(pair) {
            continue;
        }
        // Any node at the span serves for the exclusion rule - all nodes sharing a span share an
        // enclosing element unless one of them *is* the element, in which case the outermost
        // (last pushed = first in the vec, since the walk is a DFS pushing children after the
        // parent) is the element itself and gives the right answer.
        resolution
            .oracle
            .push((pair, record.first_type.clone(), left_ids[0], right_ids[0]));
    }
    Ok(resolution)
}

/// Codediff's mapping as span pairs with the node ids and kinds behind them. Null mappings
/// (insert/delete) are not pairs and are skipped.
fn codediff_pairs(
    before: &Side,
    after: &Side,
    ast: &diff::ASTDiff,
) -> Vec<(SpanPair, usize, usize, &'static str)> {
    let mut out = Vec::new();
    for &(before_id, after_id) in ast.mapping.keys() {
        if before_id == 0 || after_id == 0 {
            continue;
        }
        let (Some(b), Some(a)) = (before.nodes.get(&before_id), after.nodes.get(&after_id)) else {
            continue;
        };
        out.push(((b.span, a.span), before_id, after_id, b.kind));
    }
    out
}

#[derive(Default, Debug, Clone, Copy)]
struct Counts {
    oracle_scored: usize,
    tp: usize,
    fp: usize,
    fn_: usize,
}

impl Counts {
    fn precision(&self) -> f64 {
        ratio(self.tp, self.tp + self.fp)
    }
    fn recall(&self) -> f64 {
        ratio(self.tp, self.tp + self.fn_)
    }
    fn perfect(&self) -> bool {
        self.fp == 0 && self.fn_ == 0
    }
    fn add(&mut self, other: &Counts) {
        self.oracle_scored += other.oracle_scored;
        self.tp += other.tp;
        self.fp += other.fp;
        self.fn_ += other.fn_;
    }
}

fn ratio(num: usize, den: usize) -> f64 {
    if den == 0 {
        f64::NAN
    } else {
        num as f64 / den as f64
    }
}

struct FileScore {
    case: String,
    file: String,
    problematic: bool,
    records: usize,
    resolved: usize,
    all: Counts,
    statement: Counts,
    codediff_ms: f64,
}

/// One compilation unit, scored at both granularities.
#[allow(clippy::too_many_arguments)]
fn score_file(
    case: &str,
    file: &str,
    problematic: bool,
    before: &Side,
    after: &Side,
    resolution: &FileResolution,
    whitelist_all: &HashSet<&'static str>,
    whitelist_statement: &HashSet<&'static str>,
    details: bool,
) -> FileScore {
    let started = Instant::now();
    let diff = diff::diff_code(&before.code, &after.code);
    let codediff_ms = started.elapsed().as_secs_f64() * 1000.0;
    let pairs = diff
        .ast
        .as_ref()
        .map(|ast| codediff_pairs(before, after, ast))
        .unwrap_or_default();

    let mut score = FileScore {
        case: case.to_string(),
        file: file.to_string(),
        problematic,
        records: resolution.records,
        resolved: resolution.oracle.len(),
        all: Counts::default(),
        statement: Counts::default(),
        codediff_ms,
    };

    for statement_level in [false, true] {
        let whitelist = if statement_level {
            whitelist_statement
        } else {
            whitelist_all
        };
        let oracle: HashSet<SpanPair> = resolution
            .oracle
            .iter()
            .filter(|(_, jdt_type, _, _)| !statement_level || is_statement_level(jdt_type))
            .filter(|(_, _, b, a)| !under_unchanged_element(before, after, *b, *a))
            .map(|(pair, _, _, _)| *pair)
            .collect();
        let judged: HashSet<SpanPair> = pairs
            .iter()
            .filter(|(_, b, a, _)| !under_unchanged_element(before, after, *b, *a))
            .filter(|(pair, _, _, kind)| {
                let g = usize::from(statement_level);
                if resolution.left_universe[g].contains(&pair.0)
                    || resolution.right_universe[g].contains(&pair.1)
                {
                    return true;
                }
                // A whitelisted kind the oracle has no record of, either side. If the node did
                // not move or change - same bytes at the same offsets - the oracle is silent
                // about a node it simply did not list (RefactoringMiner records comments
                // selectively: the probe's first "false positives" were four `// SECTION` line
                // comments mapped to themselves), and there is nothing to judge. If it did move
                // or change, the oracle's silence *is* its verdict - both ends are unmapped to
                // it - and codediff's pair is the error the whitelist exists to catch.
                whitelist.contains(kind)
                    && (pair.0 != pair.1 || before.text(pair.0) != after.text(pair.1))
            })
            .map(|(pair, _, _, _)| *pair)
            .collect();

        let counts = Counts {
            oracle_scored: oracle.len(),
            tp: oracle.intersection(&judged).count(),
            fp: judged.difference(&oracle).count(),
            fn_: oracle.difference(&judged).count(),
        };
        if details && !statement_level && !counts.perfect() {
            println!("\n{case} {file}: {} FP, {} FN", counts.fp, counts.fn_);
            let mut fps: Vec<_> = judged.difference(&oracle).collect();
            fps.sort();
            for pair in fps {
                println!(
                    "  FP  {:>6}-{:<6} {:?}  ->  {:>6}-{:<6} {:?}",
                    pair.0.0,
                    pair.0.1,
                    excerpt(before.text(pair.0)),
                    pair.1.0,
                    pair.1.1,
                    excerpt(after.text(pair.1))
                );
            }
            let mut fns: Vec<_> = oracle.difference(&judged).collect();
            fns.sort();
            for pair in fns {
                println!(
                    "  FN  {:>6}-{:<6} {:?}  ->  {:>6}-{:<6} {:?}",
                    pair.0.0,
                    pair.0.1,
                    excerpt(before.text(pair.0)),
                    pair.1.0,
                    pair.1.1,
                    excerpt(after.text(pair.1))
                );
            }
        }
        if statement_level {
            score.statement = counts;
        } else {
            score.all = counts;
        }
    }
    score
}

fn excerpt(text: &str) -> String {
    let one_line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() > 60 {
        format!("{}…", one_line.chars().take(59).collect::<String>())
    } else {
        one_line
    }
}

/// (case name, problematic?, JSON files) for every Defects4J case in the oracle checkout.
fn defects4j_cases(
    oracle_root: &Path,
    only: &[String],
) -> Result<Vec<(String, bool, Vec<PathBuf>)>> {
    let d4j = oracle_root.join("src/test/resources/astDiff/defects4j");
    if !d4j.is_dir() {
        bail!(
            "{} not found - run research/external/fetch_astdiff_oracle.sh",
            d4j.display()
        );
    }
    let read_cases = |name: &str| -> Result<Vec<CaseId>> {
        let path = d4j.join(name);
        let text = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_slice(&text).with_context(|| format!("parsing {}", path.display()))
    };
    let mut cases = Vec::new();
    for (list, problematic) in [("cases.json", false), ("cases-problematic.json", true)] {
        for id in read_cases(list)? {
            let name = format!("{}-{}", id.repo, id.commit);
            if !only.is_empty() && !only.iter().any(|o| o == &name) {
                continue;
            }
            let dir = d4j.join(&id.repo).join(&id.commit);
            let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
                .with_context(|| format!("listing {}", dir.display()))?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|e| e == "json"))
                .collect();
            files.sort();
            cases.push((name, problematic, files));
        }
    }
    cases.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(cases)
}

/// The buggy/fixed source pair a Defects4J oracle file refers to, in the replication package.
fn source_pair(sources: &Path, case: &str, json: &Path) -> Result<(PathBuf, PathBuf)> {
    let (project, bug) = case.split_once('-').context("case name is Project-BugId")?;
    let java = json
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| format!("{s}.java"))
        .context("json file has no stem")?;
    let before = sources
        .join("defects4j/before")
        .join(project)
        .join(bug)
        .join(&java);
    let after = sources
        .join("defects4j/after")
        .join(project)
        .join(bug)
        .join(&java);
    if !before.is_file() || !after.is_file() {
        bail!(
            "no source pair for {case} {java} under {} - run research/external/fetch_gumtree_simple_package.sh",
            sources.display()
        );
    }
    Ok((before, after))
}

fn main() -> Result<()> {
    let args = Args::parse();
    let cases = defects4j_cases(&args.oracle, &args.cases)?;
    if cases.is_empty() {
        bail!("no cases selected");
    }
    let total_files: usize = cases.iter().map(|c| c.2.len()).sum();
    eprintln!(
        "{} cases, {} compilation units; pass 1 (resolve oracle, learn kind whitelist)",
        cases.len(),
        total_files
    );

    // Pass 1: resolve every oracle file once to learn which tree-sitter kinds JDT models
    // one-to-one. Counted only under changed program elements and only for nodes codediff would
    // actually pair, so the ratio measures "can the oracle judge this kind" and nothing else -
    // which means pass 1 has to run the diff too. The oracle resolution itself is not kept
    // (2.2 GB of JSON would be); pass 2 re-reads it.
    let mut kind_all: HashMap<&'static str, KindStats> = HashMap::new();
    let mut kind_statement: HashMap<&'static str, KindStats> = HashMap::new();
    let mut unresolved_total: BTreeMap<String, usize> = BTreeMap::new();
    let mut records_total = 0usize;
    let mut resolved_total = 0usize;
    let mut skipped: Vec<String> = Vec::new();
    for (case, _, files) in &cases {
        for json in files {
            let (before_path, after_path) = source_pair(&args.sources, case, json)?;
            let (before, after) = match (Side::load(&before_path), Side::load(&after_path)) {
                (Ok(b), Ok(a)) => (b, a),
                (Err(e), _) | (_, Err(e)) => {
                    skipped.push(format!("{case} {}: {e}", json.display()));
                    continue;
                }
            };
            let resolution = resolve_oracle(json, &before, &after)?;
            records_total += resolution.records;
            resolved_total += resolution.oracle.len();
            for (t, n) in &resolution.unresolved {
                *unresolved_total.entry(t.clone()).or_default() += n;
            }
            let diff = diff::diff_code(&before.code, &after.code);
            let Some(ast) = diff.ast.as_ref() else {
                continue;
            };
            for (pair, b, a, kind) in codediff_pairs(&before, &after, ast) {
                if under_unchanged_element(&before, &after, b, a) {
                    continue;
                }
                for (stats, universe) in [
                    (&mut kind_all, &resolution.left_universe[0]),
                    (&mut kind_statement, &resolution.left_universe[1]),
                ] {
                    let stats = stats.entry(kind).or_default();
                    stats.seen += 1;
                    if universe.contains(&pair.0) {
                        stats.in_universe += 1;
                    }
                }
            }
        }
    }
    let whitelist = |stats: &HashMap<&'static str, KindStats>| -> HashSet<&'static str> {
        stats
            .iter()
            .filter(|(_, s)| {
                s.seen >= args.whitelist_min && ratio(s.in_universe, s.seen) >= args.whitelist_ratio
            })
            .map(|(k, _)| *k)
            .collect()
    };
    let whitelist_all = whitelist(&kind_all);
    let whitelist_statement = whitelist(&kind_statement);

    let in_comments: usize = unresolved_total
        .iter()
        .filter(|(t, _)| t.ends_with("(inside a comment)"))
        .map(|(_, n)| n)
        .sum();
    let resolution_line = format!(
        "oracle: {records_total} records, {resolved_total} resolved to tree-sitter span pairs \
         ({:.2}%; {:.2}% of the {} not inside a Javadoc/comment, which tree-sitter does not parse)",
        100.0 * ratio(resolved_total, records_total),
        100.0 * ratio(resolved_total, records_total - in_comments),
        records_total - in_comments,
    );
    eprintln!(
        "{resolution_line}; whitelist: {} kinds (all), {} kinds (statement)",
        whitelist_all.len(),
        whitelist_statement.len()
    );
    if args.details {
        let mut unresolved: Vec<_> = unresolved_total.iter().collect();
        unresolved.sort_by(|a, b| b.1.cmp(a.1));
        println!("\nUnresolved oracle records by JDT type:");
        for (t, n) in unresolved {
            println!("  {n:>8}  {t}");
        }
        for (label, stats, wl) in [
            ("all", &kind_all, &whitelist_all),
            ("statement", &kind_statement, &whitelist_statement),
        ] {
            let mut rows: Vec<_> = stats.iter().collect();
            rows.sort_by_key(|(_, s)| std::cmp::Reverse(s.seen));
            println!("\nKind whitelist ({label}): kind, seen, in-universe, ratio, whitelisted");
            for (k, s) in rows {
                println!(
                    "  {:<40} {:>8} {:>8} {:>6.3} {}",
                    k,
                    s.seen,
                    s.in_universe,
                    ratio(s.in_universe, s.seen),
                    if wl.contains(k) { "yes" } else { "" }
                );
            }
        }
    }

    // Pass 2: score.
    eprintln!("pass 2 (score)");
    let mut scores: Vec<FileScore> = Vec::with_capacity(total_files);
    for (case, problematic, files) in &cases {
        for json in files {
            let (before_path, after_path) = source_pair(&args.sources, case, json)?;
            let (Ok(before), Ok(after)) = (Side::load(&before_path), Side::load(&after_path))
            else {
                continue;
            };
            let resolution = resolve_oracle(json, &before, &after)?;
            let file = json
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("?")
                .to_string();
            scores.push(score_file(
                case,
                &file,
                *problematic,
                &before,
                &after,
                &resolution,
                &whitelist_all,
                &whitelist_statement,
                args.details,
            ));
        }
    }

    if let Some(parent) = args.csv.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut writer = csv::Writer::from_path(&args.csv)
        .with_context(|| format!("writing {}", args.csv.display()))?;
    writer.write_record([
        "case",
        "file",
        "problematic",
        "oracle_records",
        "oracle_resolved",
        "all_oracle_scored",
        "all_tp",
        "all_fp",
        "all_fn",
        "statement_oracle_scored",
        "statement_tp",
        "statement_fp",
        "statement_fn",
        "codediff_ms",
    ])?;
    for s in &scores {
        writer.write_record([
            s.case.clone(),
            s.file.clone(),
            s.problematic.to_string(),
            s.records.to_string(),
            s.resolved.to_string(),
            s.all.oracle_scored.to_string(),
            s.all.tp.to_string(),
            s.all.fp.to_string(),
            s.all.fn_.to_string(),
            s.statement.oracle_scored.to_string(),
            s.statement.tp.to_string(),
            s.statement.fp.to_string(),
            s.statement.fn_.to_string(),
            format!("{:.3}", s.codediff_ms),
        ])?;
    }
    writer.flush()?;

    // Summary, by population and granularity. "Perfect" is per case, as in the paper's Table 13
    // and 14: every compilation unit of the case has no FP and no FN.
    println!(
        "\n{} compilation units in {} cases scored ({} skipped); CSV: {}",
        scores.len(),
        cases.len(),
        skipped.len(),
        args.csv.display()
    );
    for s in &skipped {
        println!("  skipped: {s}");
    }
    println!("{resolution_line}");
    println!(
        "\n{:<28} {:>6} {:>10} {:>9} {:>9} {:>8} {:>8} {:>8} {:>9}",
        "population / granularity",
        "cases",
        "oracle",
        "precision",
        "recall",
        "F",
        "TP",
        "FP+FN",
        "perfect"
    );
    for (label, filter) in [
        ("all cases", None),
        ("cases.json", Some(false)),
        ("cases-problematic.json", Some(true)),
    ] {
        for statement_level in [false, true] {
            let mut total = Counts::default();
            let mut case_perfect: BTreeMap<&str, bool> = BTreeMap::new();
            for s in &scores {
                if filter.is_some_and(|p| p != s.problematic) {
                    continue;
                }
                let c = if statement_level {
                    &s.statement
                } else {
                    &s.all
                };
                total.add(c);
                let entry = case_perfect.entry(&s.case).or_insert(true);
                *entry = *entry && c.perfect();
            }
            let n_cases = case_perfect.len();
            let n_perfect = case_perfect.values().filter(|p| **p).count();
            let (p, r) = (total.precision(), total.recall());
            println!(
                "{:<28} {:>6} {:>10} {:>8.2}% {:>8.2}% {:>7.2}% {:>8} {:>8} {:>8.1}%",
                format!(
                    "{label} / {}",
                    if statement_level { "statement" } else { "all" }
                ),
                n_cases,
                total.oracle_scored,
                100.0 * p,
                100.0 * r,
                100.0 * 2.0 * p * r / (p + r),
                total.tp,
                total.fp + total.fn_,
                100.0 * ratio(n_perfect, n_cases)
            );
        }
    }
    println!(
        "\nPaper (Table 12/14, Defects4J, statement+sub-expression): RM 3.0 99.7/99.3 perfect 85.9%; \
         GumTree 3.0 simple 98.4/97.8 perfect 63.3%; GumTree 3.0 greedy 97.5/93.1 perfect 18.1%."
    );
    println!(
        "Paper (Table 11/13, statement only): RM 99.8/99.6 perfect 89.4%; GT simple 99.1/98.5 \
         perfect 72.4%; GT greedy 99.2/98.4 perfect 75.4%; IJM 99.0/98.6; MTDiff 98.4/98.2."
    );
    Ok(())
}
