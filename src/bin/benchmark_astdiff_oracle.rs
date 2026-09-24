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
//! half. `research/external/README.md` says what the data is and how to fetch it; this doc says
//! how a JDT mapping and a tree-sitter mapping are compared.
//!
//! **The common ground is the byte span.** A JDT record's UTF-16 offsets become a byte-span pair,
//! *resolved* if both files have a tree-sitter node with that span (allowing the boundary
//! differences in `Side::resolve`). Both mappings are then sets of span pairs, and precision and
//! recall are set arithmetic, as in the paper's Definition 5.1. Unresolved records (JDT-synthetic
//! nodes, Javadoc internals) are dropped and counted per JDT type under `--details`; **quote that
//! resolution rate next to any precision or recall**.
//!
//! **Which codediff pairs are judged.** Codediff maps tree-sitter-only structure (`;`, `modifiers`)
//! the oracle has no record of; counting that as false positives would measure the grammar. A pair
//! is judged when its left span is one the oracle maps on the left, or its right span one it maps
//! on the right, or its kind is whitelisted: a kind that resolves against the oracle nearly every
//! time it appears (`KindStats`). The whitelist catches codediff pairing a deleted node with an
//! inserted one where the oracle maps neither; it is data-driven so that a kind like `modifiers`,
//! which resolves only with a single modifier, stays out.
//!
//! **The exclusion rule**, as in the paper: a pair is excluded from both sets when its nearest
//! enclosing program element (`PROGRAM_ELEMENT_KINDS`) is byte-identical on both sides. A pair with
//! no enclosing element is kept, as the paper does.
//!
//! **Two granularities**, as in the paper's Tables 11 and 12: `statement` (`is_statement_level`,
//! and the statement-level whitelist on codediff's side) and `all`.
//!
//! Pass 1 learns the whitelist, pass 2 scores; the oracle JSON (gigabytes) is read twice rather
//! than held in memory.

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
    /// Score our own hand-authored mappings against the oracle instead of codediff's output, over
    /// the compilation units this fixture directory holds a solved fixture for. Their mapping stays
    /// the reference, so precision and recall measure agreement, not a verdict.
    ///
    /// A unit is matched to a fixture by content, not by name.
    #[arg(long)]
    human_mappings: Option<PathBuf>,
}

/// Fixture name by the `(before, after)` content of its pair - see `Args::human_mappings`.
type HumanIndex = HashMap<(u64, u64), String>;

/// Every solved fixture under `dir`, keyed by what its two files contain.
fn human_mapping_index(dir: &Path) -> Result<HumanIndex> {
    use std::hash::{Hash, Hasher};
    let digest = |bytes: &[u8]| -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hasher);
        hasher.finish()
    };
    let mut index = HumanIndex::new();
    for entry in std::fs::read_dir(dir)
        .with_context(|| format!("reading fixtures from {}", dir.display()))?
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if !path.join("human_mapping.json").is_file() {
            continue;
        }
        let mut sides: [Option<Vec<u8>>; 2] = [None, None];
        for file in std::fs::read_dir(&path)?.filter_map(Result::ok) {
            let name = file.file_name().to_string_lossy().into_owned();
            let slot = match name.split('.').next() {
                Some("before") => 0,
                Some("after") => 1,
                _ => continue,
            };
            sides[slot] = Some(std::fs::read(file.path())?);
        }
        let (Some(before), Some(after)) = (&sides[0], &sides[1]) else {
            continue;
        };
        index.insert(
            (digest(before), digest(after)),
            entry.file_name().to_string_lossy().into_owned(),
        );
    }
    Ok(index)
}

/// The fixture whose pair is byte-identical to this compilation unit's, if we have solved it.
fn human_fixture_for(index: &HumanIndex, before: &Side, after: &Side) -> Option<String> {
    use std::hash::{Hash, Hasher};
    let digest = |text: &str| -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.as_bytes().hash(&mut hasher);
        hasher.finish()
    };
    index
        .get(&(digest(&before.code.contents), digest(&after.code.contents)))
        .cloned()
}

/// One record of an oracle JSON file. The labels, the bulk of the data, are not deserialized.
/// `secondType` always equals `firstType`: the oracle maps same-typed nodes only.
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

/// Tree-sitter kinds that count as a "program element" for the exclusion rule: the paper's type,
/// method, field and enum declarations, plus imports and the package line, which every tool also
/// maps trivially when unchanged.
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

/// The paper's "statement mappings" granularity (its Table 11), by JDT type name: statements,
/// declarations (a parameter's `SingleVariableDeclaration` included) and block-like kinds.
fn is_statement_level(jdt_type: &str) -> bool {
    jdt_type.ends_with("Statement")
        || jdt_type.ends_with("Declaration")
        || matches!(
            jdt_type,
            "Block" | "CompilationUnit" | "SwitchCase" | "CatchClause" | "Initializer"
        )
}

/// One side of one file, indexed for the scorer.
struct Side {
    code: Code,
    /// Every node with this exact byte span, outermost first. Several when a node has a single
    /// child of the same extent, such as `public` under a one-modifier `modifiers`.
    by_span: HashMap<Span, Vec<usize>>,
    nodes: HashMap<usize, NodeInfo>,
    /// Byte offset of each UTF-16 code unit, plus one past the end; `None` for ASCII files.
    utf16_to_byte: Option<Vec<usize>>,
    /// Comment spans, sorted by start. JDT parses Javadoc internals and tree-sitter does not, so
    /// an unresolved record inside a comment is expected, not a problem to look at.
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
        // Refuse invalid UTF-8: a lossy conversion would shift every JDT offset after it.
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

    /// The tree-sitter nodes standing for a JDT node with this byte span, and their actual span.
    /// Exact match first, then the systematic boundary differences between JDT and
    /// tree-sitter-java:
    ///
    /// * JDT starts a documented declaration at its Javadoc; tree-sitter after it.
    /// * A trailing `;` or `:` one side includes and the other does not (`for` initialisers,
    ///   `switch` labels).
    /// * JDT's `METHOD_INVOCATION_ARGUMENTS` excludes the parentheses `argument_list` includes.
    ///
    /// Each tolerance moves a boundary only over the delimiter it names, when that delimiter is in
    /// the text, so it cannot resolve to an unrelated node.
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

/// Per-kind evidence for the whitelist: of the codediff-paired nodes of this kind under changed
/// program elements, how many have their span in the oracle's universe. A high ratio means JDT
/// models the kind one-to-one.
#[derive(Default, Debug, Clone)]
struct KindStats {
    seen: usize,
    in_universe: usize,
}

/// Everything learned about one compilation unit that both passes need.
struct FileResolution {
    /// Resolved oracle pairs, with their JDT type and the node ids each end resolved to.
    oracle: Vec<(SpanPair, String, usize, usize)>,
    records: usize,
    unresolved: BTreeMap<String, usize>,
    /// Every span the oracle maps on each side, resolved or not: the universe a codediff pair is
    /// judged against. `[0]` for all records, `[1]` for statement-level ones.
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
        // The universe holds the span a record resolved to, since it is compared against
        // tree-sitter spans; an unresolved record's raw span matches nothing.
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
        // Several JDT records can collapse onto one span pair; they are the same claim.
        if !seen.insert(pair) {
            continue;
        }
        // The outermost node at the span (first in `by_span`) is the one to use for the
        // exclusion rule, since it is the element itself when any of them is.
        resolution
            .oracle
            .push((pair, record.first_type.clone(), left_ids[0], right_ids[0]));
    }
    Ok(resolution)
}

/// Codediff's mapping as span pairs with the node ids and kinds behind them. Inserts and deletes
/// are not pairs.
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
    human: Option<(&str, &codediff::test::helper::human_mapping::HumanMapping)>,
) -> Result<FileScore> {
    let started = Instant::now();
    let human_ast = match human {
        Some((_, mapping)) => Some(
            codediff::test::helper::human_mapping::as_ast_diff_for_mapping(
                mapping,
                &before.code,
                &after.code,
            )
            .context("building an ASTDiff from the human mapping")?,
        ),
        None => None,
    };
    let diff = match &human_ast {
        Some(_) => None,
        None => Some(diff::diff_code(&before.code, &after.code)),
    };
    let codediff_ms = started.elapsed().as_secs_f64() * 1000.0;
    let ast = human_ast
        .as_ref()
        .or_else(|| diff.as_ref().and_then(|diff| diff.ast.as_ref()));
    let pairs = ast
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
                // A whitelisted kind the oracle maps on neither side. Unmoved and unchanged, the
                // oracle simply did not list it (it records comments selectively): nothing to
                // judge. Otherwise its silence is its verdict, and codediff's pair is an error.
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
    Ok(score)
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
    let human_index = match &args.human_mappings {
        Some(dir) => {
            let index = human_mapping_index(dir)?;
            eprintln!(
                "human mode: {} solved fixtures in {}; units without one are skipped, not scored \
                 as zero",
                index.len(),
                dir.display()
            );
            Some(index)
        }
        None => None,
    };
    if cases.is_empty() {
        bail!("no cases selected");
    }
    let total_files: usize = cases.iter().map(|c| c.2.len()).sum();
    eprintln!(
        "{} cases, {} compilation units; pass 1 (resolve oracle, learn kind whitelist)",
        cases.len(),
        total_files
    );

    // Pass 1: learn the kind whitelist. It counts only nodes codediff pairs, so this pass runs
    // the diff too.
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
            // In human mode an unsolved unit is skipped, not scored as zero: counting it would
            // report our coverage as our accuracy.
            let mapping = match &human_index {
                Some(index) => match human_fixture_for(index, &before, &after) {
                    Some(name) => {
                        let mapping = codediff::test::helper::human_mapping::load(&name)
                            .with_context(|| format!("loading the human mapping for {name}"))?;
                        Some((name, mapping))
                    }
                    None => continue,
                },
                None => None,
            };
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
                mapping
                    .as_ref()
                    .map(|(name, mapping)| (name.as_str(), mapping)),
            )?);
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

    // "Perfect" is per case, as in the paper's Tables 13 and 14: no FP and no FN in any unit.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn side(java: &str) -> Side {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(java.as_bytes()).unwrap();
        Side::load(file.path()).unwrap()
    }

    fn span_of(side: &Side, text: &str) -> Span {
        let start = side.code.contents.find(text).unwrap();
        (start, start + text.len())
    }

    fn kinds_at(side: &Side, ids: &[usize]) -> Vec<&'static str> {
        ids.iter().map(|id| side.nodes[id].kind).collect()
    }

    #[test]
    fn resolve_skips_a_leading_javadoc() {
        let s = side("class A {\n  /** Doc. */\n  // more\n  void f() {}\n}\n");
        let start = s.code.contents.find("/**").unwrap();
        let end = span_of(&s, "void f() {}").1;
        let (span, ids) = s.resolve(start, end).unwrap();
        assert_eq!(span, span_of(&s, "void f() {}"));
        assert_eq!(kinds_at(&s, ids)[0], "method_declaration");
    }

    #[test]
    fn resolve_tolerates_a_trailing_semicolon_or_colon() {
        let s = side("class A { void f() { for (int i = 0; i < 1; i++) {} } }");
        let (start, end) = span_of(&s, "int i = 0");
        let (span, ids) = s.resolve(start, end).unwrap();
        assert_eq!(span, (start, end + 1));
        assert_eq!(kinds_at(&s, ids)[0], "local_variable_declaration");
    }

    #[test]
    fn resolve_widens_arguments_to_their_parentheses() {
        let s = side("class A { void f() { g(1, 2); } }");
        let (start, end) = span_of(&s, "1, 2");
        let (span, ids) = s.resolve(start, end).unwrap();
        assert_eq!(span, (start - 1, end + 1));
        assert_eq!(kinds_at(&s, ids)[0], "argument_list");
    }

    #[test]
    fn resolve_rejects_a_span_no_tolerance_explains() {
        let s = side("class A { void f() { g(1, 2); } }");
        let (start, _) = span_of(&s, "1, 2");
        assert!(s.resolve(start, start + 2).is_none());
    }

    #[test]
    fn by_span_lists_the_outermost_node_first() {
        let s = side("class A { public int x; }");
        let ids = &s.by_span[&span_of(&s, "public")];
        assert_eq!(kinds_at(&s, ids), ["modifiers", "public"]);
    }

    #[test]
    fn byte_offset_converts_utf16_units_past_non_ascii_text() {
        // 'é' is one UTF-16 unit and two bytes; '😀' is two units and four bytes.
        let s = side("class A { String s = \"é😀\"; int x; }");
        let x = s.code.contents.find("int x").unwrap();
        let utf16 = s.code.contents[..x].encode_utf16().count();
        assert_eq!(s.byte_offset(utf16), Some(x));
        assert_eq!(
            s.byte_offset(s.code.contents.encode_utf16().count()),
            Some(s.code.contents.len())
        );
    }

    #[test]
    fn statement_level_keeps_parameters_but_not_declaration_fragments() {
        assert!(is_statement_level("SingleVariableDeclaration"));
        assert!(is_statement_level("ExpressionStatement"));
        assert!(is_statement_level("Block"));
        assert!(!is_statement_level("VariableDeclarationFragment"));
        assert!(!is_statement_level("MethodInvocation"));
    }

    #[test]
    fn a_pair_under_a_byte_identical_element_is_excluded() {
        let before = side("class A { void f() { g(); } void h() { g(); } }");
        let after = side("class A { void f() { g(); } void h() { k(); } }");
        let call = |s: &Side, nth: usize| {
            let start = s.code.contents.match_indices("g();").nth(nth).unwrap().0;
            s.by_span[&(start, start + 3)][0]
        };
        assert!(under_unchanged_element(
            &before,
            &after,
            call(&before, 0),
            call(&after, 0)
        ));
        let k = after.code.contents.find("k()").unwrap();
        let k_call = after.by_span[&(k, k + 3)][0];
        assert!(!under_unchanged_element(
            &before,
            &after,
            call(&before, 1),
            k_call
        ));
    }
}
