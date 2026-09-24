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

//! Corpus-wide statistics over every `human_mapping.json` under `src/test/data/diffs/`: the shape
//! of the ground truth itself (sizes, operation mix, groups), plus corpus-wide signals for
//! pipeline-design questions (wrap/reparent rate, sibling-reorder rate). No diffing happens here;
//! `benchmark_optimal_solutions` measures codediff against the corpus.

use anyhow::Result;
use clap::Parser;
use codediff::code::Language;
use codediff::diff::NodeCache;
use codediff::test::helper;
use codediff::test::helper::PathCache;
use codediff::test::helper::human_mapping::{self, GroupPairing, HumanOperation};
use std::collections::HashMap;
use std::path::PathBuf;

/// Subtree size, root inclusive. A copy of `codediff::stats::count_nodes`, which sits behind the
/// `stats` feature this binary otherwise does not need; keep the two in sync.
fn count_nodes(root: tree_sitter::Node) -> usize {
    let mut count = 1;
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        count += count_nodes(child);
    }
    count
}

#[derive(Parser)]
#[command(
    about = "Corpus-wide statistics over src/test/data/diffs/ human_mapping.json fixtures - dataset characterization plus pipeline-design signals (wrap/reparent rate, sibling-reorder rate)"
)]
struct Args {
    /// Per-fixture CSV export path. Default: ./research/data/quality/human_mapping_analysis.csv
    #[arg(long, value_name = "PATH", num_args = 0..=1)]
    csv: Option<Option<PathBuf>>,

    /// Entries shown in each "top changed node kinds" table.
    #[arg(long, default_value_t = 15)]
    top_kinds: usize,

    /// Dump every paired entry whose two nodes have different kinds, with their text, as CSV, and
    /// stop. Default: ./research/data/quality/kind_mismatches.csv
    #[arg(long, value_name = "PATH", num_args = 0..=1)]
    kind_mismatches: Option<Option<PathBuf>>,

    /// Count the ground-truth delete+insert leaf pairs that rules R1/R2a would require to be a
    /// match if they were invariants, write them to
    /// ./research/data/quality/kind_invariant_candidates.csv, and stop.
    #[arg(long)]
    kind_invariant_cost: bool,

    /// List the pairs that extending invariant 18 by elimination would require, and stop: a child
    /// with no field name is still unambiguous when both parents have the same child count and
    /// every other child is matched in order.
    #[arg(long)]
    elimination_cost: bool,
}

/// Tally of `HumanOperation` counts for one fixture or the whole corpus.
#[derive(Default, Clone, Copy)]
struct OpCounts {
    identical: usize,
    update: usize,
    match_but_not_identical: usize,
    delete: usize,
    delete_with_children: usize,
    insert: usize,
    insert_with_children: usize,
}

impl OpCounts {
    fn add(&mut self, op: HumanOperation) {
        self.add_n(op, 1);
    }

    fn add_n(&mut self, op: HumanOperation, n: usize) {
        match op {
            HumanOperation::Identical => self.identical += n,
            HumanOperation::Update => self.update += n,
            HumanOperation::MatchButNotIdentical => self.match_but_not_identical += n,
            HumanOperation::Delete => self.delete += n,
            HumanOperation::DeleteWithChildren => self.delete_with_children += n,
            HumanOperation::Insert => self.insert += n,
            HumanOperation::InsertWithChildren => self.insert_with_children += n,
        }
    }

    fn merge(&mut self, other: &OpCounts) {
        self.identical += other.identical;
        self.update += other.update;
        self.match_but_not_identical += other.match_but_not_identical;
        self.delete += other.delete;
        self.delete_with_children += other.delete_with_children;
        self.insert += other.insert;
        self.insert_with_children += other.insert_with_children;
    }

    fn total(&self) -> usize {
        self.identical
            + self.update
            + self.match_but_not_identical
            + self.delete
            + self.delete_with_children
            + self.insert
            + self.insert_with_children
    }
}

/// Histogram of `|before_path.len() - after_path.len()|` over paired entries. A delta of 1 is the
/// wrap/reparent shape.
#[derive(Default, Clone, Copy)]
struct DepthDeltaCounts {
    zero: usize,
    one: usize,
    two: usize,
    three_plus: usize,
}

impl DepthDeltaCounts {
    fn add(&mut self, delta: usize) {
        match delta {
            0 => self.zero += 1,
            1 => self.one += 1,
            2 => self.two += 1,
            _ => self.three_plus += 1,
        }
    }

    fn merge(&mut self, other: &DepthDeltaCounts) {
        self.zero += other.zero;
        self.one += other.one;
        self.two += other.two;
        self.three_plus += other.three_plus;
    }
}

/// Splits a path segment `"kind:ordinal"`. The ordinal counts only same-kind siblings, so it is
/// not a positional index.
fn split_kind_index(segment: &str) -> (&str, Option<u32>) {
    match segment.rsplit_once(':') {
        Some((kind, idx)) => (kind, idx.parse().ok()),
        None => (segment, None),
    }
}

/// A paired entry whose two paths share the whole ancestor chain and the final kind, differing
/// at most in the per-kind ordinal.
struct SiblingCandidate {
    parent_path: Vec<String>,
    kind: String,
    before_idx: u32,
    after_idx: u32,
}

fn sibling_candidate(before_path: &[String], after_path: &[String]) -> Option<SiblingCandidate> {
    if before_path.len() != after_path.len() || before_path.is_empty() {
        return None;
    }
    let split = before_path.len() - 1;
    if before_path[..split] != after_path[..split] {
        return None;
    }
    let (before_kind, before_idx) = split_kind_index(&before_path[split]);
    let (after_kind, after_idx) = split_kind_index(&after_path[split]);
    if before_kind != after_kind {
        return None;
    }
    Some(SiblingCandidate {
        parent_path: before_path[..split].to_vec(),
        kind: before_kind.to_string(),
        before_idx: before_idx?,
        after_idx: after_idx?,
    })
}

type SiblingGroupKey = (Vec<String>, String);

/// Adjacent descents in `after_idx` within each `(parent_path, kind)` group sorted by
/// `before_idx`: a lower bound on inversions. Counting changed ordinals instead would flag every
/// later sibling after one deletion, although relative order is preserved. A lower bound on
/// reordering overall: a swap that also changes wrapper depth
/// (`lua-neovim-neovim-if-flips-two-branches`) never becomes a candidate.
fn count_reorder_inversions(candidates: Vec<SiblingCandidate>) -> usize {
    let mut groups: HashMap<SiblingGroupKey, Vec<(u32, u32)>> = HashMap::new();
    for c in candidates {
        groups
            .entry((c.parent_path, c.kind))
            .or_default()
            .push((c.before_idx, c.after_idx));
    }
    let mut inversions = 0;
    for mut pairs in groups.into_values() {
        if pairs.len() < 2 {
            continue;
        }
        pairs.sort_unstable_by_key(|&(before_idx, _)| before_idx);
        for window in pairs.windows(2) {
            if window[1].1 < window[0].1 {
                inversions += 1;
            }
        }
    }
    inversions
}

struct FixtureStats {
    name: String,
    category: String,
    language: Language,
    before_loc: usize,
    after_loc: usize,
    before_bytes: usize,
    after_bytes: usize,
    before_nodes: usize,
    after_nodes: usize,
    has_mapping: bool,
    ops: OpCounts,
    /// `ops` counted in AST node instances rather than entries: a paired entry counts 2, a
    /// `*WithChildren` entry its whole subtree. Nodes with no entry default to identical, so
    /// `analyze_fixture` folds them into `identical` and `total()` equals
    /// `before_nodes + after_nodes`.
    node_ops: OpCounts,
    /// The part of `node_ops.identical` that has no explicit entry.
    implicit_identical_nodes: usize,
    depth_delta: DepthDeltaCounts,
    reorder_signals: usize,
    paired_entries: usize,
    group_count: usize,
    group_with_children: usize,
    /// Groups whose members all correspond to each other (`GroupPairing::AllToAll`) - an N:M
    /// correspondence the one-to-one algorithm cannot express yet, as opposed to the ambiguity
    /// the other groups record.
    group_all_to_all: usize,
    group_sizes: Vec<(usize, usize)>,
    delete_kinds: HashMap<String, usize>,
    insert_kinds: HashMap<String, usize>,
    current_mismatches: Option<usize>,
}

/// Fixture name -> mismatches from a `benchmark_optimal_solutions --csv` export. Empty, not an
/// error, when the file is missing: only the cross-reference section needs it.
fn load_current_mismatches(path: &std::path::Path) -> HashMap<String, usize> {
    let mut out = HashMap::new();
    let Ok(mut reader) = csv::Reader::from_path(path) else {
        return out;
    };
    let headers = reader.headers().cloned().unwrap_or_default();
    let Some(name_idx) = headers.iter().position(|h| h == "solution") else {
        return out;
    };
    let Some(mismatches_idx) = headers.iter().position(|h| h == "mismatches") else {
        return out;
    };
    for record in reader.records().flatten() {
        let (Some(name), Some(mismatches)) = (record.get(name_idx), record.get(mismatches_idx))
        else {
            continue;
        };
        if let Ok(count) = mismatches.parse::<usize>() {
            out.insert(name.to_string(), count);
        }
    }
    out
}

/// Subtree size of the node at `path`, or 1 when there is no tree or the path does not resolve.
/// A `fn`, not a closure, so that its lifetime stays generic across the two per-side calls.
fn subtree_size<'a>(
    root: Option<tree_sitter::Node<'a>>,
    cache: &mut Option<PathCache<'a>>,
    path: &[String],
    fixture_name: &str,
) -> usize {
    let (Some(root), Some(cache)) = (root, cache.as_mut()) else {
        return 1;
    };
    let path_refs: Vec<&str> = path.iter().map(String::as_str).collect();
    match cache.resolve(root, &path_refs) {
        Ok(node) => count_nodes(node),
        Err(e) => {
            eprintln!(
                "warning: {fixture_name}: could not resolve path {path:?} for subtree size: {e}"
            );
            1
        }
    }
}

fn analyze_fixture(
    name: &str,
    category: &str,
    before: &codediff::code::Code,
    after: &codediff::code::Code,
    current_mismatches: &HashMap<String, usize>,
) -> Result<FixtureStats> {
    let node_cache = NodeCache::build(before, after);
    let language = before
        .metadata
        .language
        .or(after.metadata.language)
        .unwrap_or_default();

    let mut stats = FixtureStats {
        name: name.to_string(),
        category: category.to_string(),
        language,
        before_loc: before.contents.lines().count(),
        after_loc: after.contents.lines().count(),
        before_bytes: before.contents.len(),
        after_bytes: after.contents.len(),
        before_nodes: node_cache.before.len(),
        after_nodes: node_cache.after.len(),
        has_mapping: false,
        ops: OpCounts::default(),
        node_ops: OpCounts::default(),
        implicit_identical_nodes: 0,
        depth_delta: DepthDeltaCounts::default(),
        reorder_signals: 0,
        paired_entries: 0,
        group_count: 0,
        group_with_children: 0,
        group_all_to_all: 0,
        group_sizes: Vec::new(),
        delete_kinds: HashMap::new(),
        insert_kinds: HashMap::new(),
        current_mismatches: current_mismatches.get(name).copied(),
    };

    if !human_mapping::mapping_path(name).exists() {
        return Ok(stats);
    }
    stats.has_mapping = true;
    let mapping = human_mapping::load(name)?;
    let mut sibling_candidates: Vec<SiblingCandidate> = Vec::new();

    let before_root = before.ast.as_ref().map(|a| a.root_node());
    let after_root = after.ast.as_ref().map(|a| a.root_node());
    let mut before_cache = before_root.map(|_| PathCache::new());
    let mut after_cache = after_root.map(|_| PathCache::new());

    for entry in &mapping.entries {
        stats.ops.add(entry.operation);
        match entry.operation {
            HumanOperation::Delete | HumanOperation::DeleteWithChildren => {
                if let Some(path) = &entry.before_path {
                    let n = if entry.operation == HumanOperation::DeleteWithChildren {
                        subtree_size(before_root, &mut before_cache, path, name)
                    } else {
                        1
                    };
                    stats.node_ops.add_n(entry.operation, n);
                    if let Some(last) = path.last() {
                        let (kind, _) = split_kind_index(last);
                        *stats.delete_kinds.entry(kind.to_string()).or_insert(0) += 1;
                    }
                }
            }
            HumanOperation::Insert | HumanOperation::InsertWithChildren => {
                if let Some(path) = &entry.after_path {
                    let n = if entry.operation == HumanOperation::InsertWithChildren {
                        subtree_size(after_root, &mut after_cache, path, name)
                    } else {
                        1
                    };
                    stats.node_ops.add_n(entry.operation, n);
                    if let Some(last) = path.last() {
                        let (kind, _) = split_kind_index(last);
                        *stats.insert_kinds.entry(kind.to_string()).or_insert(0) += 1;
                    }
                }
            }
            HumanOperation::Identical
            | HumanOperation::Update
            | HumanOperation::MatchButNotIdentical => {
                if let (Some(before_path), Some(after_path)) =
                    (&entry.before_path, &entry.after_path)
                {
                    stats.paired_entries += 1;
                    stats.node_ops.add_n(entry.operation, 2);
                    let delta = before_path.len().abs_diff(after_path.len());
                    stats.depth_delta.add(delta);
                    if let Some(candidate) = sibling_candidate(before_path, after_path) {
                        sibling_candidates.push(candidate);
                    }
                }
            }
        }
    }
    stats.reorder_signals = count_reorder_inversions(sibling_candidates);

    // Nodes covered only by a group also land here; groups are rare and small. Saturating because
    // a stale mapping can overcount, and that is reported, not a panic.
    let total_physical_nodes = stats.before_nodes + stats.after_nodes;
    stats.implicit_identical_nodes = total_physical_nodes.saturating_sub(stats.node_ops.total());
    stats.node_ops.identical += stats.implicit_identical_nodes;

    for group in &mapping.groups {
        stats.group_count += 1;
        if group.with_children {
            stats.group_with_children += 1;
        }
        if group.pairing == GroupPairing::AllToAll {
            stats.group_all_to_all += 1;
        }
        stats
            .group_sizes
            .push((group.before_paths.len(), group.after_paths.len()));
    }

    Ok(stats)
}

/// `p` in `[0.0, 1.0]`. `values` must already be sorted ascending.
fn percentile(values: &[usize], p: f64) -> usize {
    if values.is_empty() {
        return 0;
    }
    let idx = ((values.len() as f64 - 1.0) * p).round() as usize;
    values[idx.min(values.len() - 1)]
}

fn print_size_distribution(label: &str, mut values: Vec<usize>) {
    if values.is_empty() {
        return;
    }
    values.sort_unstable();
    let sum: usize = values.iter().sum();
    let mean = sum as f64 / values.len() as f64;
    println!(
        "  {label:<22} min={:<8} p50={:<8} mean={:<10.1} p90={:<8} p99={:<8} max={:<8}",
        values[0],
        percentile(&values, 0.5),
        mean,
        percentile(&values, 0.9),
        percentile(&values, 0.99),
        values[values.len() - 1],
    );
}

fn print_top_kinds(title: &str, counts: &HashMap<String, usize>, top_n: usize) {
    let mut entries: Vec<(&String, &usize)> = counts.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    println!("\n{title}");
    let total: usize = counts.values().sum();
    for (kind, count) in entries.iter().take(top_n) {
        let pct = if total > 0 {
            100.0 * **count as f64 / total as f64
        } else {
            0.0
        };
        println!("  {kind:<30} {count:>7}  ({pct:>5.1}%)");
    }
    if entries.len() > top_n {
        println!("  ... and {} more kinds", entries.len() - top_n);
    }
}

fn write_csv(stats: &[FixtureStats], path: &std::path::Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::File::create(path)?;
    let mut wtr = csv::Writer::from_writer(file);
    wtr.write_record([
        "name",
        "category",
        "language",
        "before_loc",
        "after_loc",
        "before_bytes",
        "after_bytes",
        "before_nodes",
        "after_nodes",
        "has_mapping",
        "op_identical",
        "op_update",
        "op_match_but_not_identical",
        "op_delete",
        "op_delete_with_children",
        "op_insert",
        "op_insert_with_children",
        "node_op_identical",
        "node_op_update",
        "node_op_match_but_not_identical",
        "node_op_delete",
        "node_op_delete_with_children",
        "node_op_insert",
        "node_op_insert_with_children",
        "implicit_identical_nodes",
        "paired_entries",
        "depth_delta_0",
        "depth_delta_1",
        "depth_delta_2",
        "depth_delta_3plus",
        "reorder_signals",
        "group_count",
        "group_with_children",
        "group_all_to_all",
        "current_mismatches",
    ])?;
    for s in stats {
        wtr.write_record([
            s.name.clone(),
            s.category.clone(),
            s.language.to_string(),
            s.before_loc.to_string(),
            s.after_loc.to_string(),
            s.before_bytes.to_string(),
            s.after_bytes.to_string(),
            s.before_nodes.to_string(),
            s.after_nodes.to_string(),
            s.has_mapping.to_string(),
            s.ops.identical.to_string(),
            s.ops.update.to_string(),
            s.ops.match_but_not_identical.to_string(),
            s.ops.delete.to_string(),
            s.ops.delete_with_children.to_string(),
            s.ops.insert.to_string(),
            s.ops.insert_with_children.to_string(),
            s.node_ops.identical.to_string(),
            s.node_ops.update.to_string(),
            s.node_ops.match_but_not_identical.to_string(),
            s.node_ops.delete.to_string(),
            s.node_ops.delete_with_children.to_string(),
            s.node_ops.insert.to_string(),
            s.node_ops.insert_with_children.to_string(),
            s.implicit_identical_nodes.to_string(),
            s.paired_entries.to_string(),
            s.depth_delta.zero.to_string(),
            s.depth_delta.one.to_string(),
            s.depth_delta.two.to_string(),
            s.depth_delta.three_plus.to_string(),
            s.reorder_signals.to_string(),
            s.group_count.to_string(),
            s.group_with_children.to_string(),
            s.group_all_to_all.to_string(),
            s.current_mismatches
                .map(|m| m.to_string())
                .unwrap_or_default(),
        ])?;
    }
    wtr.flush()?;
    Ok(())
}

/// One paired entry whose two nodes have different kinds - a row of `--kind-mismatches`.
struct KindMismatch {
    fixture: String,
    language: String,
    before_kind: String,
    after_kind: String,
    before_text: String,
    after_text: String,
    before_parent: String,
    after_parent: String,
    before_named: bool,
    after_named: bool,
    /// No named children. A cross-kind leaf pair only says "this slot's token changed"; a
    /// composite pair is a judgement that two structures correspond. Anonymous children such as
    /// `(`/`)` do not make a node composite.
    before_leaf: bool,
    after_leaf: bool,
    /// Whether the two parents are matched to each other; if not, the pair is a level shift.
    parents_matched: bool,
    depth: usize,
}

/// Named children only - see [`KindMismatch::before_leaf`].
fn is_leaf(node: tree_sitter::Node) -> bool {
    node.named_child_count() == 0
}

/// A node's text on one line, capped at 80 chars so that a large matched subtree stays one
/// readable CSV cell.
fn cell_text(node: tree_sitter::Node, src: &str) -> String {
    let raw = node.utf8_text(src.as_bytes()).unwrap_or("<unreadable>");
    let flat: String = raw
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect();
    let collapsed = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > 80 {
        collapsed.chars().take(77).collect::<String>() + "..."
    } else {
        collapsed
    }
}

/// Every cross-kind paired entry in one fixture. A path that does not resolve (a stale mapping)
/// is warned about and skipped.
fn kind_mismatches_of(
    name: &str,
    before: &codediff::code::Code,
    after: &codediff::code::Code,
) -> Vec<KindMismatch> {
    let Ok(mapping) = human_mapping::load(name) else {
        return Vec::new();
    };
    let (Some(before_root), Some(after_root)) = (
        before.ast.as_ref().map(|a| a.root_node()),
        after.ast.as_ref().map(|a| a.root_node()),
    ) else {
        return Vec::new();
    };
    let language = before
        .metadata
        .language
        .or(after.metadata.language)
        .unwrap_or_default();
    let mut before_cache = PathCache::default();
    let mut after_cache = PathCache::default();
    let mut out = Vec::new();

    // The resolved correspondence, not the entry list: most parents are matched implicitly, under
    // an ancestor's entry, and have no entry of their own.
    let caches = human_mapping::rebuild_caches_for_mapping(&mapping, before_root, after_root);

    for entry in &mapping.entries {
        if !matches!(
            entry.operation,
            HumanOperation::Identical
                | HumanOperation::Update
                | HumanOperation::MatchButNotIdentical
        ) {
            continue;
        }
        let (Some(bp), Some(ap)) = (entry.before_path.as_ref(), entry.after_path.as_ref()) else {
            continue;
        };
        if bp.is_empty() || ap.is_empty() {
            continue;
        }
        let b_refs: Vec<&str> = bp.iter().map(String::as_str).collect();
        let a_refs: Vec<&str> = ap.iter().map(String::as_str).collect();
        let (Ok(b), Ok(a)) = (
            before_cache.resolve(before_root, &b_refs),
            after_cache.resolve(after_root, &a_refs),
        ) else {
            eprintln!("warning: {name}: could not resolve {bp:?} <-> {ap:?}");
            continue;
        };
        if b.kind() == a.kind() {
            continue;
        }
        out.push(KindMismatch {
            fixture: name.to_string(),
            language: format!("{language:?}"),
            before_kind: b.kind().to_string(),
            after_kind: a.kind().to_string(),
            before_text: cell_text(b, &before.contents),
            after_text: cell_text(a, &after.contents),
            before_parent: b.parent().map_or("<root>".into(), |p| p.kind().to_string()),
            after_parent: a.parent().map_or("<root>".into(), |p| p.kind().to_string()),
            // An anonymous node's kind is its text, so for them a text change is a kind change.
            before_named: b.is_named(),
            after_named: a.is_named(),
            before_leaf: is_leaf(b),
            after_leaf: is_leaf(a),
            parents_matched: match (b.parent(), a.parent()) {
                (Some(pb), Some(pa)) => caches.before_match.get(&pb.id()) == Some(&pa.id()),
                // Both at the root: the roots always correspond.
                (None, None) => true,
                _ => false,
            },
            depth: bp.len(),
        });
    }
    out
}

/// What R1/R2a would flag in one fixture if they were invariants.
///
/// **R1**: matched parents where the before parent has exactly one removed named leaf and the
/// after parent exactly one inserted one, in the same single-child field.
/// **R2a**: a removed and an inserted leaf with the same text, each unique in the fixture.
///
/// Both demand uniqueness, because with two candidates no invariant can demand a particular
/// match. Neither requires equal kinds; the split is reported.
struct InvariantCost {
    r1_same_kind: usize,
    r1_diff_kind: usize,
    r2a_same_kind: usize,
    r2a_diff_kind: usize,
    /// `(rule, before_kind, after_kind, before_text, after_text, slot, before_row, after_row)` per
    /// candidate, so each can be judged by hand rather than trusted as a count.
    #[allow(clippy::type_complexity)]
    samples: Vec<(
        &'static str,
        String,
        String,
        String,
        String,
        String,
        usize,
        usize,
    )>,
}

fn collect_nodes<'a>(root: tree_sitter::Node<'a>, out: &mut Vec<tree_sitter::Node<'a>>) {
    out.push(root);
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        collect_nodes(child, out);
    }
}

/// No children at all. Stricter than [`is_leaf`] on purpose: an empty `()` argument list has no
/// named children but is a container. An invariant may under-fire; it may not over-fire.
fn is_lexeme(node: tree_sitter::Node) -> bool {
    node.child_count() == 0
}

/// How many of `parent`'s children carry `field`. More than one makes the field a list, where
/// position is not identity.
fn field_arity(parent: tree_sitter::Node, field: &str) -> usize {
    let mut cursor = parent.walk();
    let children: Vec<_> = parent.children(&mut cursor).collect();
    (0..children.len())
        .filter(|i| parent.field_name_for_child(*i as u32) == Some(field))
        .count()
}

/// Which field of its parent a node occupies, or `None` when the grammar gives it no field.
fn field_of(node: tree_sitter::Node) -> Option<String> {
    let parent = node.parent()?;
    let mut cursor = parent.walk();
    for (i, child) in parent.children(&mut cursor).enumerate() {
        if child.id() == node.id() {
            return parent.field_name_for_child(i as u32).map(str::to_string);
        }
    }
    None
}

fn kind_invariant_cost_of(
    name: &str,
    before: &codediff::code::Code,
    after: &codediff::code::Code,
) -> Option<InvariantCost> {
    let mapping = human_mapping::load(name).ok()?;
    let before_root = before.ast.as_ref()?.root_node();
    let after_root = after.ast.as_ref()?.root_node();
    let caches = human_mapping::rebuild_caches_for_mapping(&mapping, before_root, after_root);

    let mut before_nodes = Vec::new();
    let mut after_nodes = Vec::new();
    collect_nodes(before_root, &mut before_nodes);
    collect_nodes(after_root, &mut after_nodes);

    let removed_leaf = |n: &tree_sitter::Node| {
        n.is_named()
            && is_lexeme(*n)
            && matches!(
                human_mapping::status_before(*n, &caches),
                human_mapping::NodeStatus::Marked { .. }
            )
    };
    let inserted_leaf = |n: &tree_sitter::Node| {
        n.is_named()
            && is_lexeme(*n)
            && matches!(
                human_mapping::status_after(*n, &caches),
                human_mapping::NodeStatus::Marked { .. }
            )
    };

    // ---- R1 ----
    let mut removed_by_parent: HashMap<usize, Vec<tree_sitter::Node>> = HashMap::new();
    for n in before_nodes.iter().filter(|n| removed_leaf(n)) {
        if let Some(p) = n.parent() {
            removed_by_parent.entry(p.id()).or_default().push(*n);
        }
    }
    let mut inserted_by_parent: HashMap<usize, Vec<tree_sitter::Node>> = HashMap::new();
    for n in after_nodes.iter().filter(|n| inserted_leaf(n)) {
        if let Some(p) = n.parent() {
            inserted_by_parent.entry(p.id()).or_default().push(*n);
        }
    }

    let mut cost = InvariantCost {
        r1_same_kind: 0,
        r1_diff_kind: 0,
        r2a_same_kind: 0,
        r2a_diff_kind: 0,
        samples: Vec::new(),
    };
    let mut r1_pairs: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    for (pb_id, removed) in &removed_by_parent {
        if removed.len() != 1 {
            continue;
        }
        let Some(pa_id) = caches.before_match.get(pb_id) else {
            continue;
        };
        let Some(inserted) = inserted_by_parent.get(pa_id) else {
            continue;
        };
        if inserted.len() != 1 {
            continue;
        }
        let (b, a) = (removed[0], inserted[0]);
        // Same named field, not merely the same parent pair: a field is a role that persists,
        // while two unrelated adjacent comments share only a position.
        let (fb, fa) = (field_of(b), field_of(a));
        let (Some(fb_name), Some(fa_name)) = (fb.as_ref(), fa.as_ref()) else {
            continue;
        };
        if fb_name != fa_name {
            continue;
        }
        let (Some(pb), Some(pa)) = (b.parent(), a.parent()) else {
            continue;
        };
        if field_arity(pb, fb_name) != 1 || field_arity(pa, fa_name) != 1 {
            continue;
        }
        r1_pairs.insert((b.id(), a.id()));
        cost.samples.push((
            "R1",
            b.kind().to_string(),
            a.kind().to_string(),
            cell_text(b, &before.contents),
            cell_text(a, &after.contents),
            format!("{}.{}", pb.kind(), fb_name),
            b.start_position().row + 1,
            a.start_position().row + 1,
        ));
        if b.kind() == a.kind() {
            cost.r1_same_kind += 1;
        } else {
            cost.r1_diff_kind += 1;
        }
    }

    // ---- R2a ----
    let mut removed_by_text: HashMap<&str, Vec<tree_sitter::Node>> = HashMap::new();
    for n in before_nodes.iter().filter(|n| removed_leaf(n)) {
        if let Ok(t) = n.utf8_text(before.contents.as_bytes()) {
            removed_by_text.entry(t).or_default().push(*n);
        }
    }
    let mut inserted_by_text: HashMap<&str, Vec<tree_sitter::Node>> = HashMap::new();
    for n in after_nodes.iter().filter(|n| inserted_leaf(n)) {
        if let Ok(t) = n.utf8_text(after.contents.as_bytes()) {
            inserted_by_text.entry(t).or_default().push(*n);
        }
    }
    for (text, removed) in &removed_by_text {
        if removed.len() != 1 {
            continue;
        }
        let Some(inserted) = inserted_by_text.get(text) else {
            continue;
        };
        if inserted.len() != 1 {
            continue;
        }
        let (b, a) = (removed[0], inserted[0]);
        if r1_pairs.contains(&(b.id(), a.id())) {
            continue;
        }
        cost.samples.push((
            "R2a",
            b.kind().to_string(),
            a.kind().to_string(),
            cell_text(b, &before.contents),
            cell_text(a, &after.contents),
            String::new(),
            b.start_position().row + 1,
            a.start_position().row + 1,
        ));
        if b.kind() == a.kind() {
            cost.r2a_same_kind += 1;
        } else {
            cost.r2a_diff_kind += 1;
        }
    }
    Some(cost)
}

/// One pair the elimination extension would newly require, for judging rather than counting.
struct EliminationHit {
    fixture: String,
    parent: String,
    before_kind: String,
    after_kind: String,
    before_text: String,
    after_text: String,
    same_kind: bool,
    /// The mapping already pairs these two: not a violation, but a check that the test agrees
    /// with the ground truth.
    already_matched: bool,
}

/// Pairs that invariant 18 misses because the slot has no field name, but where position is fixed
/// by elimination: equal child counts, and every sibling pairwise matched in order.
fn elimination_hits_of(
    name: &str,
    before: &codediff::code::Code,
    after: &codediff::code::Code,
) -> Vec<EliminationHit> {
    let mut out = Vec::new();
    let Ok(mapping) = human_mapping::load(name) else {
        return out;
    };
    let (Some(before_root), Some(after_root)) = (
        before.ast.as_ref().map(|a| a.root_node()),
        after.ast.as_ref().map(|a| a.root_node()),
    ) else {
        return out;
    };
    let caches = human_mapping::rebuild_caches_for_mapping(&mapping, before_root, after_root);
    let mut before_nodes = Vec::new();
    collect_nodes(before_root, &mut before_nodes);
    let after_by_id: HashMap<usize, tree_sitter::Node> = {
        let mut v = Vec::new();
        collect_nodes(after_root, &mut v);
        v.into_iter().map(|n| (n.id(), n)).collect()
    };

    for parent in &before_nodes {
        let Some(after_parent) = caches
            .before_match
            .get(&parent.id())
            .and_then(|id| after_by_id.get(id))
        else {
            continue;
        };
        let mut bc = parent.walk();
        let before_children: Vec<_> = parent.children(&mut bc).collect();
        let mut ac = after_parent.walk();
        let after_children: Vec<_> = after_parent.children(&mut ac).collect();
        if before_children.len() != after_children.len() || before_children.is_empty() {
            continue;
        }
        let mut candidate = None;
        let mut ok = true;
        for (b, a) in before_children.iter().zip(after_children.iter()) {
            let paired = caches.before_match.get(&b.id()) == Some(&a.id());
            if paired && b.kind() == a.kind() {
                continue;
            }
            let b_removed = matches!(
                human_mapping::status_before(*b, &caches),
                human_mapping::NodeStatus::Marked { .. }
            );
            let a_removed = matches!(
                human_mapping::status_after(*a, &caches),
                human_mapping::NodeStatus::Marked { .. }
            );
            let interesting = (b_removed && a_removed) || paired;
            if interesting && b.is_named() && a.is_named() && is_lexeme(*b) && is_lexeme(*a) {
                if candidate.is_some() {
                    ok = false;
                    break;
                }
                candidate = Some((*b, *a, paired));
            } else {
                ok = false;
                break;
            }
        }
        let (Some((b, a, already_matched)), true) = (candidate, ok) else {
            continue;
        };
        if b.kind() == a.kind() && already_matched {
            continue;
        }
        if field_of(b).is_some() && field_of(b) == field_of(a) {
            continue;
        }
        out.push(EliminationHit {
            fixture: name.to_string(),
            parent: parent.kind().to_string(),
            before_kind: b.kind().to_string(),
            after_kind: a.kind().to_string(),
            before_text: cell_text(b, &before.contents),
            after_text: cell_text(a, &after.contents),
            same_kind: b.kind() == a.kind(),
            already_matched,
        });
    }
    out
}

fn write_kind_mismatches(rows: &[KindMismatch], path: &std::path::Path) -> Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(
        f,
        "fixture,language,before_kind,after_kind,before_named,after_named,before_leaf,after_leaf,parents_matched,before_parent,after_parent,depth,before_text,after_text"
    )?;
    // Quote every field: an anonymous node's kind is its text, so a kind can be `,` or `"`.
    for r in rows {
        let cells = [
            r.fixture.as_str(),
            r.language.as_str(),
            r.before_kind.as_str(),
            r.after_kind.as_str(),
            if r.before_named { "true" } else { "false" },
            if r.after_named { "true" } else { "false" },
            if r.before_leaf { "true" } else { "false" },
            if r.after_leaf { "true" } else { "false" },
            if r.parents_matched { "true" } else { "false" },
            r.before_parent.as_str(),
            r.after_parent.as_str(),
            &r.depth.to_string(),
            r.before_text.as_str(),
            r.after_text.as_str(),
        ];
        let line: Vec<String> = cells
            .iter()
            .map(|c| format!("\"{}\"", c.replace('"', "\"\"")))
            .collect();
        writeln!(f, "{}", line.join(","))?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();

    let code_pairs = helper::handmade_test_code_pairs()?;
    let mut names: Vec<String> = code_pairs.keys().cloned().collect();
    names.sort();

    if args.elimination_cost {
        let mut hits = Vec::new();
        for name in &names {
            let (before, after) = code_pairs
                .get(name)
                .expect("name came from code_pairs.keys()");
            hits.extend(elimination_hits_of(name, before, after));
        }
        let (would_require, validation): (Vec<_>, Vec<_>) =
            hits.iter().partition(|h| !h.already_matched);
        let cross = would_require.iter().filter(|h| !h.same_kind).count();
        let fixtures: std::collections::HashSet<&str> =
            hits.iter().map(|h| h.fixture.as_str()).collect();
        println!("=== Elimination extension to invariant 18 ===");
        println!(
            "{} pair(s) the extension would newly REQUIRE: {} cross-kind, {} same-kind, across {} fixture(s)",
            would_require.len(),
            cross,
            would_require.len() - cross,
            fixtures.len()
        );
        println!(
            "{} pair(s) already matched this way - the test recognising what the corpus resolved",
            validation.len()
        );
        println!();
        for h in hits
            .iter()
            .filter(|h| !h.already_matched)
            .chain(hits.iter().filter(|h| h.already_matched))
        {
            println!(
                "  [{}{}] {}  {} `{}` -> {} `{}`   [{}]",
                if h.already_matched { "ok " } else { "REQ" },
                if h.same_kind { " SAME" } else { " cross" },
                h.parent,
                h.before_kind,
                h.before_text,
                h.after_kind,
                h.after_text,
                h.fixture
            );
        }
        return Ok(());
    }

    if args.kind_invariant_cost {
        let (mut r1s, mut r1d, mut r2s, mut r2d) = (0usize, 0usize, 0usize, 0usize);
        let mut fixtures_hit = 0usize;
        let mut worst: Vec<(usize, String)> = Vec::new();
        #[allow(clippy::type_complexity)]
        let mut all_samples: Vec<(
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
        )> = Vec::new();
        for name in &names {
            let (before, after) = code_pairs
                .get(name)
                .expect("name came from code_pairs.keys()");
            let Some(c) = kind_invariant_cost_of(name, before, after) else {
                continue;
            };
            let total = c.r1_same_kind + c.r1_diff_kind + c.r2a_same_kind + c.r2a_diff_kind;
            if total > 0 {
                fixtures_hit += 1;
                worst.push((total, name.clone()));
            }
            for (rule, bk, ak, bt, at, field, br, ar) in &c.samples {
                all_samples.push((
                    name.clone(),
                    (*rule).to_string(),
                    bk.clone(),
                    ak.clone(),
                    bt.clone(),
                    at.clone(),
                    field.clone(),
                    br.to_string(),
                    ar.to_string(),
                ));
            }
            r1s += c.r1_same_kind;
            r1d += c.r1_diff_kind;
            r2s += c.r2a_same_kind;
            r2d += c.r2a_diff_kind;
        }
        worst.sort_by_key(|entry| std::cmp::Reverse(entry.0));
        {
            use std::io::Write;
            let path =
                std::path::Path::new("./research/data/quality/kind_invariant_candidates.csv");
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
            writeln!(
                f,
                "rule,fixture,before_kind,after_kind,before_text,after_text,slot,before_row,after_row"
            )?;
            for (fixture, rule, bk, ak, bt, at, slot, br, ar) in &all_samples {
                let cells = [
                    rule.as_str(),
                    fixture.as_str(),
                    bk.as_str(),
                    ak.as_str(),
                    bt.as_str(),
                    at.as_str(),
                    slot.as_str(),
                    br.as_str(),
                    ar.as_str(),
                ];
                let line: Vec<String> = cells
                    .iter()
                    .map(|c| format!("\"{}\"", c.replace('"', "\"\"")))
                    .collect();
                writeln!(f, "{}", line.join(","))?;
            }
            println!("candidates written to {}", path.display());
        }
        println!("=== If R1 and R2a were invariants ===");
        println!("R1  (leaf deleted + leaf inserted under corresponding parents, unambiguous)");
        println!(
            "      same kind: {r1s}    different kind: {r1d}    total: {}",
            r1s + r1d
        );
        println!("R2a (identical lexeme, unique on both sides, not already counted by R1)");
        println!(
            "      same kind: {r2s}    different kind: {r2d}    total: {}",
            r2s + r2d
        );
        println!("TOTAL candidate violations: {}", r1s + r1d + r2s + r2d);
        println!("Fixtures affected: {fixtures_hit} of {}", names.len());
        println!("\nWorst fixtures:");
        for (n, name) in worst.iter().take(15) {
            println!("   {n:>5}  {name}");
        }
        return Ok(());
    }

    if let Some(target) = args.kind_mismatches.clone() {
        let path =
            target.unwrap_or_else(|| PathBuf::from("./research/data/quality/kind_mismatches.csv"));
        let mut rows = Vec::new();
        for name in &names {
            let (before, after) = code_pairs
                .get(name)
                .expect("name came from code_pairs.keys()");
            rows.extend(kind_mismatches_of(name, before, after));
        }
        write_kind_mismatches(&rows, &path)?;
        println!(
            "{} cross-kind paired entries across {} fixtures -> {}",
            rows.len(),
            rows.iter()
                .map(|r| r.fixture.as_str())
                .collect::<std::collections::HashSet<_>>()
                .len(),
            path.display()
        );
        return Ok(());
    }

    let current_mismatches = load_current_mismatches(std::path::Path::new(
        "./research/data/quality/optimal_solutions_benchmark.csv",
    ));

    let mut all_stats = Vec::with_capacity(names.len());
    for name in &names {
        let (before, after) = code_pairs
            .get(name)
            .expect("name came from code_pairs.keys()");
        let category = helper::diffs_case_dir(name)
            .and_then(|p| {
                p.parent()
                    .and_then(|p| p.file_name())
                    .map(|s| s.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "unknown".to_string());
        all_stats.push(analyze_fixture(
            name,
            &category,
            before,
            after,
            &current_mismatches,
        )?);
    }

    // ---- 1. Corpus overview ----
    println!("=== Corpus overview ===");
    println!("Total fixtures: {}", all_stats.len());
    let solved = all_stats.iter().filter(|s| s.has_mapping).count();
    println!(
        "With human_mapping.json: {solved} ({:.1}%), unsolved: {}",
        100.0 * solved as f64 / all_stats.len().max(1) as f64,
        all_stats.len() - solved
    );

    let mut by_category: HashMap<&str, usize> = HashMap::new();
    for s in &all_stats {
        *by_category.entry(s.category.as_str()).or_insert(0) += 1;
    }
    let mut categories: Vec<(&str, usize)> = by_category.into_iter().collect();
    categories.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    for (category, count) in &categories {
        println!("  {category:<12} {count}");
    }

    let mut by_language: HashMap<String, usize> = HashMap::new();
    for s in &all_stats {
        *by_language.entry(s.language.to_string()).or_insert(0) += 1;
    }
    let mut languages: Vec<(String, usize)> = by_language.into_iter().collect();
    languages.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    println!("\nBy language ({} distinct):", languages.len());
    for (language, count) in &languages {
        println!("  {language:<12} {count}");
    }

    // ---- 2. Size distributions ----
    println!("\n=== Size distributions ===");
    print_size_distribution(
        "Before LOC",
        all_stats.iter().map(|s| s.before_loc).collect(),
    );
    print_size_distribution("After LOC", all_stats.iter().map(|s| s.after_loc).collect());
    print_size_distribution(
        "Before nodes",
        all_stats.iter().map(|s| s.before_nodes).collect(),
    );
    print_size_distribution(
        "After nodes",
        all_stats.iter().map(|s| s.after_nodes).collect(),
    );
    let total_before_nodes: usize = all_stats.iter().map(|s| s.before_nodes).sum();
    let total_after_nodes: usize = all_stats.iter().map(|s| s.after_nodes).sum();
    let total_before_loc: usize = all_stats.iter().map(|s| s.before_loc).sum();
    println!(
        "\nTotals: {} before-tree nodes, {} after-tree nodes, {} before-side lines of code",
        total_before_nodes, total_after_nodes, total_before_loc
    );

    // ---- 3. Human operation mix (solved fixtures only) ----
    let mut corpus_ops = OpCounts::default();
    let mut corpus_node_ops = OpCounts::default();
    let mut corpus_depth_delta = DepthDeltaCounts::default();
    let mut total_reorder_signals = 0usize;
    let mut total_paired_entries = 0usize;
    let mut total_groups = 0usize;
    let mut total_group_with_children = 0usize;
    let mut total_group_all_to_all = 0usize;
    let mut total_implicit_identical = 0usize;
    let mut all_group_sizes: Vec<(usize, usize)> = Vec::new();
    let mut delete_kinds: HashMap<String, usize> = HashMap::new();
    let mut insert_kinds: HashMap<String, usize> = HashMap::new();
    for s in all_stats.iter().filter(|s| s.has_mapping) {
        corpus_ops.merge(&s.ops);
        corpus_node_ops.merge(&s.node_ops);
        corpus_depth_delta.merge(&s.depth_delta);
        total_reorder_signals += s.reorder_signals;
        total_paired_entries += s.paired_entries;
        total_groups += s.group_count;
        total_group_with_children += s.group_with_children;
        total_group_all_to_all += s.group_all_to_all;
        total_implicit_identical += s.implicit_identical_nodes;
        all_group_sizes.extend(&s.group_sizes);
        for (kind, count) in &s.delete_kinds {
            *delete_kinds.entry(kind.clone()).or_insert(0) += count;
        }
        for (kind, count) in &s.insert_kinds {
            *insert_kinds.entry(kind.clone()).or_insert(0) += count;
        }
    }

    println!("\n=== Human-authored operation mix ({solved} solved fixtures) ===");
    let total_ops = corpus_ops.total();
    for (label, count) in [
        ("Identical", corpus_ops.identical),
        ("Update", corpus_ops.update),
        ("MatchButNotIdentical", corpus_ops.match_but_not_identical),
        ("Delete", corpus_ops.delete),
        ("DeleteWithChildren", corpus_ops.delete_with_children),
        ("Insert", corpus_ops.insert),
        ("InsertWithChildren", corpus_ops.insert_with_children),
    ] {
        let pct = if total_ops > 0 {
            100.0 * count as f64 / total_ops as f64
        } else {
            0.0
        };
        println!("  {label:<22} {count:>10}  ({pct:>5.1}%)");
    }
    println!("  {:<22} {total_ops:>10}", "Total");
    println!(
        "Headline: {:.1}% of all entries are Identical - real edits touch a small fraction of \
         the tree. Every percentage below is computed against this Identical-dominated total, so \
         a small-looking share (e.g. depth-delta-1's {:.2}%) can still be the majority of edited \
         nodes and the strongest failure predictor found - see the non-Identical-relative figures \
         alongside each.",
        100.0 * corpus_ops.identical as f64 / total_ops.max(1) as f64,
        100.0 * corpus_depth_delta.one as f64 / total_paired_entries.max(1) as f64
    );

    // ---- 3b. Same mix, in AST node instances rather than mapping entries ----
    println!("\n=== Same mix, in AST node instances rather than mapping entries ===");
    println!(
        "(a DeleteWithChildren/InsertWithChildren entry counts its whole subtree here, not 1 - \
         see node_op_* columns in the --csv export)"
    );
    let total_node_ops = corpus_node_ops.total();
    for (label, count) in [
        ("Identical", corpus_node_ops.identical),
        ("Update", corpus_node_ops.update),
        (
            "MatchButNotIdentical",
            corpus_node_ops.match_but_not_identical,
        ),
        ("Delete", corpus_node_ops.delete),
        ("DeleteWithChildren", corpus_node_ops.delete_with_children),
        ("Insert", corpus_node_ops.insert),
        ("InsertWithChildren", corpus_node_ops.insert_with_children),
    ] {
        let pct = if total_node_ops > 0 {
            100.0 * count as f64 / total_node_ops as f64
        } else {
            0.0
        };
        println!("  {label:<22} {count:>10}  ({pct:>5.1}%)");
    }
    println!("  {:<22} {total_node_ops:>10}", "Total");
    // Solved fixtures only, the same scope as `corpus_node_ops`.
    let expected_node_total: usize = all_stats
        .iter()
        .filter(|s| s.has_mapping)
        .map(|s| s.before_nodes + s.after_nodes)
        .sum();
    println!(
        "Sanity check: node-instance total ({total_node_ops}) equals before_nodes + after_nodes \
         summed over solved fixtures ({expected_node_total}) by construction - see below for how \
         much of Identical is implicit."
    );
    debug_assert_eq!(
        total_node_ops, expected_node_total,
        "node_ops.total() must equal before_nodes + after_nodes after the implicit-identical fold"
    );

    let fixtures_with_implicit = all_stats
        .iter()
        .filter(|s| s.has_mapping && s.implicit_identical_nodes > 0)
        .count();
    println!(
        "\nOf {} total Identical node instances, {} ({:.1}%) are implicit (no explicit entry in \
         human_mapping.json - see `node_ops`'s doc comment), across {fixtures_with_implicit}/{solved} \
         fixtures ({:.1}%). Ground truth is sparse for a large file: only changed/relevant nodes \
         get an explicit entry, so a fixture's own `paired_entries`/`ops.total()` alone \
         understates how much of it was actually reviewed as Identical.",
        corpus_node_ops.identical,
        total_implicit_identical,
        100.0 * total_implicit_identical as f64 / corpus_node_ops.identical.max(1) as f64,
        100.0 * fixtures_with_implicit as f64 / solved.max(1) as f64
    );
    let mut top_implicit: Vec<&FixtureStats> = all_stats
        .iter()
        .filter(|s| s.implicit_identical_nodes > 0)
        .collect();
    top_implicit.sort_by_key(|s| std::cmp::Reverse(s.implicit_identical_nodes));
    for s in top_implicit.iter().take(5) {
        println!("  {:<55} {}", s.name, s.implicit_identical_nodes);
    }

    // ---- 4. Depth-delta distribution (wrap/reparent prevalence) ----
    println!(
        "\n=== Depth-delta distribution ({total_paired_entries} paired before/after entries) ==="
    );
    println!("(|before_path.len() - after_path.len()|; delta=1 is the wrap/reparent shape)");
    for (label, count) in [
        ("0 (no depth change)", corpus_depth_delta.zero),
        ("1 (wrap/reparent shape)", corpus_depth_delta.one),
        ("2", corpus_depth_delta.two),
        ("3+", corpus_depth_delta.three_plus),
    ] {
        let pct = if total_paired_entries > 0 {
            100.0 * count as f64 / total_paired_entries as f64
        } else {
            0.0
        };
        println!("  {label:<26} {count:>10}  ({pct:>5.2}%)");
    }
    let non_identical_paired = corpus_ops.update + corpus_ops.match_but_not_identical;
    println!(
        "  (of {non_identical_paired} non-Identical paired entries, delta=1 is {:.1}% - the \
         percentages above are diluted by the {:.1}% of paired entries that are unchanged \
         Identical nodes, which are essentially always delta=0)",
        100.0 * corpus_depth_delta.one as f64 / non_identical_paired.max(1) as f64,
        100.0 * corpus_ops.identical as f64 / total_paired_entries.max(1) as f64
    );
    let fixtures_with_delta_one = all_stats
        .iter()
        .filter(|s| s.has_mapping && s.depth_delta.one > 0)
        .count();
    println!(
        "Fixtures with >=1 depth-delta-1 entry: {fixtures_with_delta_one}/{solved} ({:.1}%)",
        100.0 * fixtures_with_delta_one as f64 / solved.max(1) as f64
    );

    // ---- 5. Sibling-reorder signal ----
    println!("\n=== Same-kind sibling-reorder inversions (narrow signal, lower bound) ===");
    println!(
        "(direct same-kind siblings under an unchanged parent only; wrap+swap cases - e.g. \
         lua-neovim-neovim-if-flips-two-branches, a real branch swap that also changes wrapper \
         depth - score 0 here and land in depth-delta-1 instead, so this UNDERcounts true reorder \
         prevalence)"
    );
    println!(
        "Total signal occurrences: {total_reorder_signals} (of {total_paired_entries} paired entries)"
    );
    let fixtures_with_reorder = all_stats
        .iter()
        .filter(|s| s.has_mapping && s.reorder_signals > 0)
        .count();
    println!(
        "Fixtures with >=1 signal: {fixtures_with_reorder}/{solved} ({:.1}%) - a floor, not a census",
        100.0 * fixtures_with_reorder as f64 / solved.max(1) as f64
    );
    let mut top_reorder: Vec<&FixtureStats> =
        all_stats.iter().filter(|s| s.reorder_signals > 0).collect();
    top_reorder.sort_by_key(|s| std::cmp::Reverse(s.reorder_signals));
    for s in top_reorder.iter().take(10) {
        println!("  {:<55} {}", s.name, s.reorder_signals);
    }

    // ---- 6. Multi-map groups (genuine ambiguity floor) ----
    println!("\n=== Multi-map groups (human-confirmed ambiguity) ===");
    let fixtures_with_groups = all_stats
        .iter()
        .filter(|s| s.has_mapping && s.group_count > 0)
        .count();
    println!(
        "Total groups: {total_groups}, across {fixtures_with_groups}/{solved} fixtures ({:.1}%)",
        100.0 * fixtures_with_groups as f64 / solved.max(1) as f64
    );
    if total_groups > 0 {
        println!(
            "with_children: {total_group_with_children}/{total_groups} ({:.1}%)",
            100.0 * total_group_with_children as f64 / total_groups as f64
        );
        println!(
            "all-to-all (an N:M correspondence, not ambiguity): {total_group_all_to_all}/{total_groups} ({:.1}%)",
            100.0 * total_group_all_to_all as f64 / total_groups as f64
        );
        let sizes: Vec<usize> = all_group_sizes.iter().map(|(b, a)| (*b).max(*a)).collect();
        print_size_distribution("max(before_paths, after_paths)", sizes);
    }

    // ---- 7. Top changed node kinds ----
    print_top_kinds("Top deleted node kinds", &delete_kinds, args.top_kinds);
    print_top_kinds("Top inserted node kinds", &insert_kinds, args.top_kinds);

    // ---- 8. Cross-reference against current codediff results ----
    let with_current: Vec<&FixtureStats> = all_stats
        .iter()
        .filter(|s| s.current_mismatches.is_some())
        .collect();
    if with_current.is_empty() {
        println!(
            "\n=== Cross-reference against current results ===\n\
             (skipped: no ./research/data/quality/optimal_solutions_benchmark.csv found - run \
             `benchmark_optimal_solutions --csv` first)"
        );
    } else {
        println!(
            "\n=== Cross-reference against current results ({} fixtures) ===",
            with_current.len()
        );
        let zero_rate = |fixtures: &[&FixtureStats]| -> (usize, usize) {
            let zero = fixtures
                .iter()
                .filter(|s| s.current_mismatches == Some(0))
                .count();
            (zero, fixtures.len())
        };
        let (overall_zero, overall_total) = zero_rate(&with_current);
        println!(
            "Overall zero-mismatch rate: {overall_zero}/{overall_total} ({:.1}%)",
            100.0 * overall_zero as f64 / overall_total.max(1) as f64
        );

        let (with_delta1, without_delta1): (Vec<&FixtureStats>, Vec<&FixtureStats>) =
            with_current.iter().partition(|s| s.depth_delta.one > 0);
        let (d1_zero, d1_total) = zero_rate(&with_delta1);
        let (nd1_zero, nd1_total) = zero_rate(&without_delta1);
        println!(
            "  Fixtures WITH a depth-delta-1 entry:    {d1_zero}/{d1_total} zero-mismatch ({:.1}%)",
            100.0 * d1_zero as f64 / d1_total.max(1) as f64
        );
        println!(
            "  Fixtures WITHOUT a depth-delta-1 entry: {nd1_zero}/{nd1_total} zero-mismatch ({:.1}%)",
            100.0 * nd1_zero as f64 / nd1_total.max(1) as f64
        );

        let (with_reorder, without_reorder): (Vec<&FixtureStats>, Vec<&FixtureStats>) =
            with_current.iter().partition(|s| s.reorder_signals > 0);
        let (r_zero, r_total) = zero_rate(&with_reorder);
        let (nr_zero, nr_total) = zero_rate(&without_reorder);
        println!(
            "  Fixtures WITH a reorder inversion (narrow signal): {r_zero}/{r_total} zero-mismatch ({:.1}%)",
            100.0 * r_zero as f64 / r_total.max(1) as f64
        );
        println!(
            "  Fixtures WITHOUT one (includes wrap+swap false negatives): {nr_zero}/{nr_total} zero-mismatch ({:.1}%)",
            100.0 * nr_zero as f64 / nr_total.max(1) as f64
        );
    }

    if let Some(csv_path) = args.csv {
        let path = csv_path
            .unwrap_or_else(|| PathBuf::from("./research/data/quality/human_mapping_analysis.csv"));
        write_csv(&all_stats, &path)?;
        println!("\nPer-fixture CSV written to {}", path.display());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(segments: &[&str]) -> Vec<String> {
        segments.iter().map(|s| s.to_string()).collect()
    }

    fn candidate(before: &[&str], after: &[&str]) -> Option<SiblingCandidate> {
        sibling_candidate(&path(before), &path(after))
    }

    #[test]
    fn sibling_candidate_rejects_depth_parent_and_kind_changes() {
        assert!(candidate(&["block:0", "if:1"], &["block:0", "else:0", "if:0"]).is_none());
        assert!(candidate(&["block:0", "if:1"], &["block:1", "if:1"]).is_none());
        assert!(candidate(&["block:0", "if:1"], &["block:0", "while:1"]).is_none());
        let c = candidate(&["block:0", "if:1"], &["block:0", "if:0"]).unwrap();
        assert_eq!((c.before_idx, c.after_idx), (1, 0));
    }

    #[test]
    fn renumbering_after_a_deletion_is_not_a_reorder() {
        let ripple = (1..50)
            .map(|i| {
                candidate(
                    &["list:0", &format!("item:{i}")],
                    &["list:0", &format!("item:{}", i - 1)],
                )
            })
            .map(Option::unwrap)
            .collect();
        assert_eq!(count_reorder_inversions(ripple), 0);
    }

    #[test]
    fn swapping_two_siblings_is_a_reorder() {
        let swap = vec![
            candidate(&["list:0", "item:0"], &["list:0", "item:1"]).unwrap(),
            candidate(&["list:0", "item:1"], &["list:0", "item:0"]).unwrap(),
            candidate(&["list:0", "item:2"], &["list:0", "item:2"]).unwrap(),
        ];
        assert_eq!(count_reorder_inversions(swap), 1);
    }

    #[test]
    fn reorder_inversions_are_counted_per_parent_and_kind() {
        // Descending across groups is not an inversion within either group.
        let groups = vec![
            candidate(&["a:0", "item:5"], &["a:0", "item:5"]).unwrap(),
            candidate(&["b:0", "item:0"], &["b:0", "item:0"]).unwrap(),
        ];
        assert_eq!(count_reorder_inversions(groups), 0);
    }
}
