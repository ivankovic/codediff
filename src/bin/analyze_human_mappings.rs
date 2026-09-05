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

//! Corpus-wide statistics over every `human_mapping.json` under `src/test/data/diffs/` - two
//! distinct purposes in one binary:
//!
//! 1. **Dataset characterization** (fixture/language/size counts, operation mix, multi-map group
//!    prevalence) - the kind of table a paper's evaluation section needs, and which didn't exist
//!    anywhere before this: `benchmark_optimal_solutions` reports codediff's *performance* against
//!    the corpus, not the corpus's own shape.
//! 2. **Settling open pipeline-design questions with real numbers.** Two came up directly in the
//!    same session this binary was written in (see `TODO.md`'s 2026-08-17 entries): "how common is
//!    the wrap/reparent shape `TRIVIAL_ENTRY_MAX_SIZE` targets, corpus-wide?" and "how common is
//!    sibling reordering (the commutative-matching gap), corpus-wide?" - both were previously
//!    answered by manually reading 2-3 fixtures' mismatch paths, which twice turned out to be
//!    actively misleading once checked against the real source diff. This computes both signals
//!    over the whole corpus instead.
//!
//! Every stat here comes from one of two cheap sources: `human_mapping.json`'s `before_path`/
//! `after_path` strings (already `"kind:sibling_ordinal"`, so kind/depth/position signals need no
//! tree-sitter parsing at all - pure string splitting), or `Code`/`NodeCache` for the handful of
//! stats that genuinely need the parsed tree (node counts, language). No AST diffing happens here;
//! this is strictly about the *ground truth*, not codediff's output.

use anyhow::Result;
use clap::Parser;
use codediff::code::Language;
use codediff::diff::NodeCache;
use codediff::test::helper;
use codediff::test::helper::PathCache;
use codediff::test::helper::human_mapping::{self, HumanOperation};
use std::collections::HashMap;
use std::path::PathBuf;

/// Counts a tree-sitter subtree's size (root inclusive) - a local copy of
/// `codediff::stats::count_nodes`, not a reuse of it: that function lives behind the `stats`
/// feature (git2/rusqlite and the rest of that feature's build-time cost), which this binary has
/// no other reason to pull in - it only needs `test-fixtures`. Keep this in sync if
/// `stats::count_nodes`'s definition ever changes.
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
}

/// Tally of `HumanOperation` counts for one fixture (or the whole corpus). A plain struct, not a
/// `HashMap<HumanOperation, usize>`: `HumanOperation` doesn't derive `Hash` (nothing else in this
/// codebase has needed it to), and there are only 7 variants - named fields are simpler than
/// wiring up a hashable wrapper for this alone.
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

    /// Same tally, weighted by `n` instead of always 1 - used for the *node-instance* count
    /// (see `node_ops` on `FixtureStats`), where a single `DeleteWithChildren`/`InsertWithChildren`
    /// entry covers an entire subtree, not one node.
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

/// Depth-delta histogram: `|before_path.len() - after_path.len()|` for every paired entry
/// (`Identical`/`Update`/`MatchButNotIdentical`), bucketed 0/1/2/"3+". A delta of exactly 1 is the
/// wrap/reparent signature `TRIVIAL_ENTRY_MAX_SIZE` (`apted/common/residual.rs`) targets - see that fix's
/// `TODO.md` entry (2026-08-17) for the shape this is measuring the true corpus-wide prevalence of.
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

/// Splits a human-mapping path segment (`"kind:ordinal"`) into its kind and parsed ordinal.
/// `ordinal` is the segment's *per-kind* sibling count (the Nth node of this exact kind under its
/// parent, not a raw positional index across all sibling kinds - see `HumanMappingEntry`'s path
/// format) - `split_kind_index` is named for what it returns, not for a general "tree position"
/// concept.
fn split_kind_index(segment: &str) -> (&str, Option<u32>) {
    match segment.rsplit_once(':') {
        Some((kind, idx)) => (kind, idx.parse().ok()),
        None => (segment, None),
    }
}

/// One same-kind-sibling candidate for reorder detection: a paired entry whose `before_path`/
/// `after_path` share the exact same ancestor chain (no depth change anywhere above it) and the
/// same final-segment kind, differing only in per-kind sibling ordinal.
struct SiblingCandidate {
    parent_path: Vec<String>,
    kind: String,
    before_idx: u32,
    after_idx: u32,
}

/// Collects a `SiblingCandidate` for `before_path`/`after_path` if they qualify - `None` for any
/// depth change, reparenting, or kind change (those are wrap/reparent or unrelated-move shapes,
/// not sibling reordering).
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

/// Genuine sibling-reorder count for one fixture (2026-08-17, see `TODO.md`) - a real inversion
/// count, not "does the per-kind ordinal differ." A naive per-entry "ordinal changed" check was
/// tried first and found empirically wrong: deleting one `<string>` from an ~1800-entry Android
/// `strings.xml` translation list renumbers every later same-kind sibling, which such a check
/// flags as "reordered" hundreds of times even though relative order is fully preserved - a ripple
/// from one deletion, not a swap (`xml-nextcloud-android-delete-element-2` was the empirical
/// evidence: 1818 false-positive "signals" from ordinary list-entry deletion).
///
/// Groups every candidate by `(parent_path, kind)` (same-kind direct siblings under an otherwise-
/// unchanged parent - the `lua-neovim-neovim-if-flips-two-branches` shape, two `elseif` branches
/// swapping position, found by hand this session), sorts each group by `before_idx`, then counts
/// *adjacent* descents in the resulting `after_idx` sequence - the same lower-bound-on-inversions
/// measure a bubble sort's swap count gives. A pure renumbering ripple is strictly increasing after
/// sorting by `before_idx` (zero adjacent descents, however many members); a genuine swap produces
/// at least one, however large the surrounding group. O(n log n) per group from the sort, not the
/// O(n²) a full inversion count would cost - deliberately a lower bound, not an exact count, since
/// this only needs to answer "is there a real reorder here," not "how many pairs are involved."
///
/// Deliberately conservative, not a general move detector: requires the *entire* ancestor path to
/// match exactly and the kind to match, so it only catches reordering among direct same-kind
/// siblings, not reordering nested deeper or across different kinds. Zero here doesn't mean "not
/// reordered" in general - only "not this specific, narrow signal."
///
/// Known false negative, confirmed empirically: `lua-neovim-neovim-if-flips-two-branches` - the
/// exact fixture that motivated this metric - scores **0** here, not 1+. Its swap isn't a pure
/// same-kind reorder: one branch's condition sits as a bare child of `if_statement` while the
/// swapped-in one is wrapped in `elseif_statement`, so ancestor-path length differs and the
/// candidate is dropped by `sibling_candidate`'s exact-path-match requirement before it ever
/// reaches this function - it shows up in the depth-delta-1 bucket instead. So corpus-wide
/// "fixtures with a reorder signal" is a lower bound on true reorder prevalence, not a census -
/// wrap+swap cases are systematically invisible to it and land in depth-delta instead.
type SiblingGroupKey = (Vec<String>, String);

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
    /// Same seven-way breakdown as `ops`, but counting AST *node instances*, not mapping
    /// *entries*: a plain `Identical`/`Update`/`MatchButNotIdentical` entry still contributes 2
    /// (one before-tree node, one after-tree node), a plain `Delete`/`Insert` contributes 1 (the
    /// single node on its one side), but a `DeleteWithChildren`/`InsertWithChildren` entry
    /// contributes its whole subtree's size, resolved via `PathCache` + `count_nodes` - see
    /// `analyze_fixture`. `node_ops.total()` equals `before_nodes + after_nodes` exactly, by
    /// construction: `human_mapping.rs` documents "a node with no entry at all defaults to
    /// identical" (its own `is_identical_before`/`is_identical_after` test), so ground truth for a
    /// large file is often sparse - only changed/relevant nodes get an explicit entry, not every
    /// untouched one - and `analyze_fixture` folds that implicit remainder into `node_ops.identical`
    /// after processing the explicit entries. See `implicit_identical_nodes` for how much of a
    /// given fixture's Identical count is implicit rather than explicit.
    node_ops: OpCounts,
    /// How many of `node_ops.identical`'s node instances came from the implicit-identical rule
    /// (see `node_ops`'s doc comment) rather than an explicit `Identical` entry in
    /// `human_mapping.json`. Zero for a fully-annotated fixture; large for a sparsely-annotated one
    /// (e.g. a huge file where only a localized real change was explicitly marked) - see `main`'s
    /// node-instance section for the corpus-wide prevalence this reveals.
    implicit_identical_nodes: usize,
    depth_delta: DepthDeltaCounts,
    reorder_signals: usize,
    paired_entries: usize,
    group_count: usize,
    group_with_children: usize,
    group_sizes: Vec<(usize, usize)>,
    delete_kinds: HashMap<String, usize>,
    insert_kinds: HashMap<String, usize>,
    current_mismatches: Option<usize>,
}

/// Reads `research/data/quality/optimal_solutions_benchmark.csv` (whatever the most recent `benchmark_optimal_
/// solutions --csv` run left there) for the "solution"/"mismatches" columns, if the file exists -
/// the cross-reference this binary's item #3 (does the wrap/reorder signal actually predict
/// current failure?) needs. Returns an empty map, not an error, when the file is missing: this
/// binary's other sections are still useful without it, and requiring a fresh benchmark run just to
/// see the dataset-characterization tables would be an unnecessary coupling.
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

/// Resolves `path` against `root` via `cache` and returns the resulting node's subtree size
/// (root inclusive), for a `*WithChildren` entry's node-instance count. Falls back to 1 (as if it
/// were a plain, non-subtree entry) when `root`/`cache` are unavailable (no AST on that side - see
/// `analyze_fixture`'s fail-safe elsewhere) or when the path fails to resolve, logging a warning in
/// the latter case since that should not happen against a valid ground-truth mapping. A standalone
/// `fn`, not a closure: a closure's captured/parameter lifetimes get unified at its definition
/// site, which conflicts here since `analyze_fixture` calls this once per side with independently-
/// lived `Node`/`PathCache` values - a plain `fn` stays generic over the lifetime per call site.
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

    // Node-instance counting needs to resolve a mapping-entry path down to the real tree-sitter
    // node it names, only for the two `*WithChildren` variants (everything else's contribution is
    // a fixed 1 or 2, no tree lookup needed - see `node_ops`'s doc comment). `PathCache` amortizes
    // repeated lookups through the same high-fanout parent across a fixture's many entries; built
    // lazily (`Option`, not unconditionally) since plenty of fixtures have zero `*WithChildren`
    // entries and would pay index-building cost for nothing.
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
                    // One node in the before tree, one in the after tree - both carry this entry's
                    // operation label, unlike Delete/Insert's single-sided node.
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

    // Fold the implicit-identical remainder into node_ops.identical (see that field's doc
    // comment): whatever `entries` didn't explicitly account for, out of this fixture's true
    // `before_nodes + after_nodes` total, is either a node the ground truth left implicit (the
    // common case) or one covered only by a `mapping.groups` entry (not tallied above - groups are
    // rare, 130 across 33/417 fixtures corpus-wide, and small, p50 size 2, so lumping their few
    // node instances in here rather than tallying them separately by operation isn't worth the
    // extra complexity). `saturating_sub` rather than a bare subtraction: entries could in
    // principle overcount (e.g. a stale/inconsistent mapping) and this section must not panic on
    // a fixture that turns out to violate the invariant, only report it accurately via `main`'s
    // corpus-wide section.
    let total_physical_nodes = stats.before_nodes + stats.after_nodes;
    stats.implicit_identical_nodes = total_physical_nodes.saturating_sub(stats.node_ops.total());
    stats.node_ops.identical += stats.implicit_identical_nodes;

    for group in &mapping.groups {
        stats.group_count += 1;
        if group.with_children {
            stats.group_with_children += 1;
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
            s.current_mismatches
                .map(|m| m.to_string())
                .unwrap_or_default(),
        ])?;
    }
    wtr.flush()?;
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();

    let code_pairs = helper::handmade_test_code_pairs()?;
    let mut names: Vec<String> = code_pairs.keys().cloned().collect();
    names.sort();

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
    // A `*WithChildren` entry is one line in human_mapping.json but can cover an entire subtree,
    // so the entry-count mix above (§3) understates how many actual nodes each operation touches.
    // This section re-tallies in node instances (see `node_ops`'s doc comment on `FixtureStats`),
    // which is what research/analysis/human_mapping_shapes_report.py's per-fixture chart plots.
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
    // Scoped to `has_mapping` fixtures only, matching `corpus_node_ops`'s own scope - the corpus's
    // one unsolved fixture (no human_mapping.json at all) contributes to `total_before_nodes`/
    // `total_after_nodes` above but never enters the entries loop, so including it here would
    // introduce a spurious gap unrelated to the implicit-identical mechanism this checks for.
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
