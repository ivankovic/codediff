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
pub mod apted;
pub mod cost;
pub(crate) mod grouped_greedy_matcher;
pub(crate) mod hash_tree_matching;
pub mod nodes;
pub mod solve_bottom_up_expansion;
pub mod solve_greedy_anchor_blocks;
pub mod solve_hash_descent;
pub mod solve_identical_diagnostic_statements;
pub mod solve_large_flat_subtrees;
pub mod solve_leading_siblings;
pub mod solve_moved_subtrees;
pub mod solve_syntax_aware_matching;
pub mod text;
pub mod text_range;

use tree_sitter::Node;

use crate::code::{Code, Language};
use crate::diff::text::TextDiff;

/// A structure that holds node caches for both before and after Code objects.
#[derive(Debug, Clone, Default)]
/// # Safety invariant
///
/// The `'static` lifetime on these nodes is a lie told via `transmute` in `build` below: each
/// node actually borrows from the `tree_sitter::Tree` owned by the `Code` (`before`/`after`)
/// passed into `build`. The compiler cannot check this, so it's on every caller: a `NodeCache`
/// must never be read from (or kept alive past) the point where the `Code` it was built from is
/// dropped or its `ast` field is replaced/reparsed. Every current caller builds the `Code` values
/// and the `NodeCache` as plain local variables within the same function and never lets the cache
/// outlive them, which is what makes this sound today - but nothing in the type signature
/// enforces it, so a future caller that stashes a `NodeCache` somewhere longer-lived than the
/// `Code` it was derived from would be silent undefined behavior, not a compile error or panic.
pub struct NodeCache {
    /// Cache of nodes from the before Code object, keyed by node ID.
    ///
    /// `FxHashMap`, not `std::collections::HashMap` - same rationale as `ASTMetadata::
    /// node_to_parent` (`code.rs`): a small-integer-keyed map read on every node lookup in every
    /// pipeline phase, where SipHash's per-process random reseed is pure overhead with no
    /// adversarial-input benefit. Converted 2026-07-26 alongside `ASTMetadata`'s remaining maps
    /// and `ASTDiff::mapping`/`before_node_map`/`after_node_map`.
    pub before: rustc_hash::FxHashMap<usize, tree_sitter::Node<'static>>,
    /// Cache of nodes from the after Code object, keyed by node ID.
    pub after: rustc_hash::FxHashMap<usize, tree_sitter::Node<'static>>,
}

impl NodeCache {
    /// Build node caches for both Code objects.
    /// This function will always build the caches - it assumes ASTs are parsed.
    ///
    /// See the safety invariant documented on `NodeCache` above: the returned cache borrows from
    /// `before`/`after`'s ASTs under an erased `'static` lifetime and must not outlive them.
    pub fn build(before: &Code, after: &Code) -> Self {
        NodeCache {
            before: Self::cache_for(before),
            after: Self::cache_for(after),
        }
    }

    /// Every node of `code`'s AST, keyed by node ID, under the erased `'static` lifetime
    /// described on `NodeCache`. Empty if `code` has no AST.
    fn cache_for(code: &Code) -> rustc_hash::FxHashMap<usize, tree_sitter::Node<'static>> {
        code.ast
            .as_ref()
            .map(|ast| {
                let mut cache = rustc_hash::FxHashMap::default();
                let mut stack = vec![ast.root_node()];

                while let Some(node) = stack.pop() {
                    // SAFETY: see the safety invariant documented on `NodeCache` - this cache
                    // (and therefore this erased lifetime) must not outlive `code`.
                    cache.insert(node.id(), unsafe {
                        std::mem::transmute::<tree_sitter::Node<'_>, tree_sitter::Node<'static>>(
                            node,
                        )
                    });

                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        stack.push(child);
                    }
                }

                cache
            })
            .unwrap_or_default()
    }

    pub fn get_in_any(&self, node_id: &usize) -> Option<&Node<'_>> {
        if self.before.contains_key(node_id) {
            return self.before.get(node_id);
        }
        if self.after.contains_key(node_id) {
            return self.after.get(node_id);
        }
        None
    }
}

pub const COST_INSERT: u64 = 1;
pub const COST_DELETE: u64 = 1;
pub const COST_UPDATE: u64 = 1;
pub const COST_MOVE: u64 = 0;

/**
* The main data structure. Contains the difference between two Code structures.
*
* Any function that accepts this structure or any sub-field should not assume any fields are set
* and should always first check that the required data is actually available, if not it should try
* to construct it, and if that doesn't work it should fail-safe, ideally returning a safe zero
* result. This allows the calling code to extremely efficiently process large files, and files that
* only pretend to be code but are data or configuration. To help the compiler enforce this, most
* fields should be wrapped in Option.
*
* See code.rs for the Code struct.
*/
#[derive(Debug, Clone)]
pub struct Diff {
    /// Difference based on the ASTs.
    pub ast: Option<ASTDiff>,
    /// The language used for this diff.
    pub language: Language,
    /// The difference, provided as a data structure of TreeSitter points.
    /// Very useful when the code is viewed as text, for example in editors.
    pub text: Option<TextDiff>,
}

impl Default for Diff {
    fn default() -> Self {
        Self {
            ast: None,
            language: Language::Unknown,
            text: None,
        }
    }
}

impl Diff {
    /**
     * Creates a new Diff from two Code objects.
     *
     * This is the main entry point in the AST diffing algorithm.
     * The algorithm is encoded in the code and is intentionally not explained in the Doccomment to
     * avoid it going stale. Please see the code.
     *
     * Thin wrapper around [`Diff::from_code_with_config`] with every heuristic enabled - see that
     * function for the actual pipeline and [`HeuristicConfig`] for the per-pass on/off switches.
     */
    pub fn from_code(before: &Code, after: &Code) -> Self {
        Self::from_code_with_config(before, after, &HeuristicConfig::default())
    }

    /**
     * Same as [`Diff::from_code`], but lets the caller disable individual passes via `config` -
     * built for `benchmark_optimal_solutions --no-solver-X`'s ablation study (see `ablation_study.sh`),
     * so each pass's actual contribution to accuracy can be measured by removing it and nothing
     * else. Not intended for production use: disabling a pass changes the diff's quality, not just
     * its performance, and later passes' doc comments (below) assume every earlier pass already
     * ran - `HeuristicConfig::default()` (i.e. plain `from_code`) is what every real caller wants.
     */
    pub fn from_code_with_config(before: &Code, after: &Code, config: &HeuristicConfig) -> Self {
        Self::pending_with_config(before, after, config).finish(DiffMode::Fast)
    }

    /// Runs phases 1-5 of the seven-phase pipeline (every heuristic pass except final APTED and
    /// the moved-subtree fallback) and returns a [`PendingDiff`] paused right before phase 6, so a
    /// caller can inspect [`PendingDiff::looks_expensive`] and choose a [`DiffMode`] - e.g. to ask
    /// a human, or to apply a CLI flag - before paying for (or skipping) full tree-edit-distance.
    /// See [`PendingDiff`]'s doc comment for why this split exists and its safety invariant.
    pub fn pending<'code>(before: &'code Code, after: &'code Code) -> PendingDiff<'code> {
        Self::pending_with_config(before, after, &HeuristicConfig::default())
    }

    /// Same as [`Diff::pending`], but with [`HeuristicConfig`]'s per-pass switches - see
    /// [`Diff::from_code_with_config`]'s doc comment for why that config exists.
    pub fn pending_with_config<'code>(
        before: &'code Code,
        after: &'code Code,
        config: &HeuristicConfig,
    ) -> PendingDiff<'code> {
        // Build node cache for efficient lookup
        let node_cache = NodeCache::build(before, after);

        // Compute metadata fresh for the diff algorithm
        let mut ast_diff = ASTDiff {
            ..Default::default()
        };

        // Seven-phase pipeline (`TODO.md`, 2026-07-17/18) - replaced the previous ~15-pass
        // pipeline outright once verified to be at least as accurate on every fixture in the
        // `optimal_solutions` benchmark corpus (778 vs. the old pipeline's 782 mismatches, zero
        // fixtures worse). See `TODO.md` for the full design history and the accuracy gap that
        // had to be closed first. Phases 1-5 live here, in `pending_with_config`; phases 6-7 live
        // in `PendingDiff::finish` - see that struct's doc comment for why they're split.

        // Phase 1: hash-based, largest-subtree-first descent (KindAndValueHash, KindOnlyHash,
        // normalized-import-path hash). The import-path hash variant is gated on
        // `solver_import_nodes`: the 2026-07-15 ablation study found it net-negative (-89
        // disabling it individually), and nothing about folding it into a hash variant changes
        // that signal's own accuracy.
        solve_hash_descent::solve(
            before,
            after,
            &node_cache,
            &mut ast_diff,
            config.solver_import_nodes,
        );

        // Phase 2: contextual exact matching - houses solve_leading_siblings (a comment or
        // attribute/decorator modifier that immediately precedes a matched node, walking a whole
        // chain of them backward) and solve_identical_diagnostic_statements (byte-identical
        // logging/bail/assert/printf statements). Both are cheap, high-confidence exact matches
        // that phase 1's structural hash descent doesn't reach on its own (comments and modifiers
        // aren't part of the hashed AST shape - `attribute_item` specifically also has no name/
        // identity signal at all, see `solve_leading_siblings`'s own doc comment; diagnostic
        // statements are deliberately held back from phase 1 to avoid fragmenting a bigger match),
        // mopping up leftovers right after phase 1 before the fuzzier phases below start guessing.
        // Not `solve_moved_subtrees` - that's phase 7.
        solve_leading_siblings::solve(before, after, &node_cache, &mut ast_diff);
        solve_identical_diagnostic_statements::solve(before, after, &node_cache, &mut ast_diff);

        // Phase 3: bottom-up expansion. Gated on `solver_bottom_up_expansion`, same net-negative
        // finding as above (-69 individually).
        if config.solver_bottom_up_expansion {
            solve_bottom_up_expansion::solve(before, after, &node_cache, &mut ast_diff);
        }

        // Phase 4: syntax-aware subtree matching (fully-resolved-name N:M matching, absorbing
        // solve_greedy_anchor_blocks and solve_large_flat_subtrees unconditionally). Used to also
        // absorb solve_similar_flow_control (arm-overlap matching), gated on
        // `solver_similar_flow_control` - deleted 2026-08-14, net-negative in the 2026-07-15
        // ablation study (-82) and never re-enabled; see `TODO.md`.
        solve_syntax_aware_matching::solve(before, after, &node_cache, &mut ast_diff);

        // Phase 5: second bottom-up expansion, now that phase 4 has produced more matched
        // descendants for it to vote on. Same gate as phase 3.
        if config.solver_bottom_up_expansion {
            solve_bottom_up_expansion::solve(before, after, &node_cache, &mut ast_diff);
        }

        // Guard metric for `DiffMode::Fast` (see `PendingDiff::looks_expensive`): how many nodes
        // on each side are still unmatched heading into phase 6. Safe to compute from
        // `before_node_map`/`after_node_map`'s lengths directly - no phase above this point ever
        // calls `add_delete_mappings`/`add_insert_mappings` (only phase 6/7 and the fallback do),
        // so every entry already present here is a genuine non-zero match, not a delete/insert
        // sentinel.
        let unmatched_before = node_cache
            .before
            .len()
            .saturating_sub(ast_diff.before_node_map.len());
        let unmatched_after = node_cache
            .after
            .len()
            .saturating_sub(ast_diff.after_node_map.len());

        PendingDiff {
            before,
            after,
            node_cache,
            ast_diff,
            unmatched_before,
            unmatched_after,
            config: *config,
        }
    }
}

/// Controls how [`PendingDiff::finish`] runs phase 6 (final whole-tree APTED).
///
/// `Fast` is the default for every public entry point (`Diff::from_code`, `diff_code`, ...) - this
/// is an intentional behavior change from "always run exact APTED," made to fix a measured
/// pathological case: two completely unrelated Rust files (`src/test/data/diffs/handmade/
/// rust-completely-unrelated-main-files/`) took **14.5 seconds** to diff, ~36x the project's
/// stated 400ms budget, because phases 1-5 left 86% of the larger tree unmatched and phase 6 had
/// to run full tree-edit-distance over nearly the whole thing with almost nothing to prune
/// against. See `TODO.md`'s "Size/dissimilarity-capped approximate fallback" entry (2026-07-25)
/// for a documented *prior*, narrower attempt at a similar idea (a per-subtree-pair size/
/// similarity proxy checked repeatedly inside `resolve_forest`) that was reverted as an unreliable
/// predictor of APTED cost - this guard is deliberately at a different granularity (the aggregate
/// whole-file residual, checked once, after 5 dedicated matching passes already ran), which is a
/// distinct-enough proxy to be worth trying again, but carries the same category of risk: it is
/// an unproven heuristic, not a guarantee, and should be validated against the full
/// `benchmark_optimal_solutions` corpus (see that binary) before `EXPENSIVE_RESIDUAL_THRESHOLD` is
/// trusted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DiffMode {
    /// Run phase 6 normally unless [`PendingDiff::looks_expensive`] is true, in which case
    /// substitute [`apted::for_roots_fallback`] (Myers LCS over the residual forest) for full
    /// APTED.
    #[default]
    Fast,
    /// Always run full APTED for phase 6, ignoring the guard entirely - what `--exact` (CLI) and
    /// the TUI's "Exact" prompt choice select.
    Exact,
}

/// Guard threshold consulted by [`PendingDiff::looks_expensive`]: if the larger of
/// `unmatched_before`/`unmatched_after` (node counts still unmapped after phase 5) exceeds this,
/// [`DiffMode::Fast`] substitutes [`apted::for_roots_fallback`] for phase 6 instead of full APTED.
///
/// Measured against every fixture in the `optimal_solutions` corpus (`cargo test -- --nocapture`
/// a residual-distribution dump, or `benchmark_optimal_solutions`): the largest legitimate
/// residual is `c-microsoft-terminal-add-function` at 4261 (a real, ground-truth-scored fixture -
/// tripping the guard on it regressed several dozen previously-correct matches to delete/insert
/// during development of this feature, which is exactly the failure mode `DiffMode`'s doc comment
/// warns this kind of proxy is prone to). The next-largest legitimate fixtures fall off sharply
/// (1633, 779, 606, ...) - there's no fixture between 4261 and the pathological
/// `rust-completely-unrelated-main-files` fixture's 7309. `5000` sits in that gap with real margin
/// on both sides (~17% above the largest known-legitimate residual, ~32% below the pathological
/// one). Re-measure whenever this is revisited (new corpus fixtures could shift the ceiling) - see
/// [`DiffMode`]'s doc comment for the known limitations of this kind of guard in general.
pub const EXPENSIVE_RESIDUAL_THRESHOLD: usize = 5000;

/// Phases 1-5 of the seven-phase pipeline (see [`Diff::from_code_with_config`]), paused
/// immediately before phase 6 so a caller can inspect [`PendingDiff::looks_expensive`] and choose
/// a [`DiffMode`] before running (or skipping) full tree-edit-distance. [`PendingDiff::finish`]
/// runs phases 6-7 and assembles the final [`Diff`].
///
/// # Safety invariant
///
/// Holds a [`NodeCache`] under the exact same erased-`'static` invariant documented on that
/// struct (unchanged - this struct doesn't fix that), plus explicit `&'code Code` borrows of the
/// two [`Code`] values phases 1-5 ran against. The `'code` lifetime is what makes the compiler -
/// not just a doc comment - refuse to let a `PendingDiff` outlive those `Code` values.
///
/// If a caller needs to pause mid-computation for user input (e.g. a TUI prompting for
/// [`DiffMode`]), it must block its *own* worker thread until the answer arrives - see
/// `tui::app::compute_diff_interactive`'s use of `oneshot::Receiver::blocking_recv()` inside its
/// own `spawn_blocking` closure - never hand a `PendingDiff` (or its `NodeCache`) to a different
/// thread as a value.
pub struct PendingDiff<'code> {
    before: &'code Code,
    after: &'code Code,
    node_cache: NodeCache,
    ast_diff: ASTDiff,
    unmatched_before: usize,
    unmatched_after: usize,
    config: HeuristicConfig,
}

impl<'code> PendingDiff<'code> {
    /// `max(unmatched_before, unmatched_after) > EXPENSIVE_RESIDUAL_THRESHOLD`. Both this method
    /// and `finish(DiffMode::Fast)`'s own guard check consult the same stored counts (computed
    /// once, in `Diff::pending_with_config`), so they can never disagree.
    pub fn looks_expensive(&self) -> bool {
        self.unmatched_before.max(self.unmatched_after) > EXPENSIVE_RESIDUAL_THRESHOLD
    }

    /// The raw `(unmatched_before, unmatched_after)` counts `looks_expensive` is based on - for
    /// callers (e.g. a TUI prompt) that want to show the user why it's asking.
    pub fn unmatched_counts(&self) -> (usize, usize) {
        (self.unmatched_before, self.unmatched_after)
    }

    /// Runs phase 6 (full APTED under [`DiffMode::Exact`], or under [`DiffMode::Fast`] - full
    /// APTED unless [`Self::looks_expensive`], in which case [`apted::for_roots_fallback`]) and
    /// phase 7, and assembles the final [`Diff`].
    pub fn finish(self, mode: DiffMode) -> Diff {
        let use_fallback = mode == DiffMode::Fast && self.looks_expensive();
        let PendingDiff {
            before,
            after,
            node_cache,
            mut ast_diff,
            config,
            ..
        } = self;

        // Phase 6: final APTED on the whole-file residual, or - under DiffMode::Fast, once the
        // residual is too large for that to be affordable - the cheaper Myers-LCS-based fallback.
        // Apted is asymptotically better than Zhang-Shasha and (as of 2026-07-10) is
        // containment-aware, so the exact path runs it unconditionally instead of demoting itself
        // back to Zhang-Shasha on forests with real containment constraints - see
        // src/diff/TODO.md.
        if use_fallback {
            apted::for_roots_fallback(before, after, "fast_fallback", &mut ast_diff);
        } else {
            // Pre-match top-level scope-locally-named entities (e.g. shell variable assignments
            // with no enclosing named container at all, so `solve_syntax_aware_matching`'s own
            // call to this never fires for them) whose name is unique and survives a position
            // shift caused by an unrelated insertion elsewhere in the file - see
            // `apted::prematch_unique_named_locals`'s doc comment ("shift-due-to-insertion").
            if let (Some(before_ast), Some(after_ast)) = (before.ast.as_ref(), after.ast.as_ref()) {
                let before_metadata = crate::code::metadata::metadata_of(before);
                let after_metadata = crate::code::metadata::metadata_of(after);
                apted::prematch_unique_named_locals(
                    before_ast.root_node().id(),
                    after_ast.root_node().id(),
                    &before_metadata,
                    &after_metadata,
                    "unique_named_local",
                    &mut ast_diff,
                );
            }
            apted::for_roots(
                before,
                after,
                &node_cache,
                apted::Algorithm::Apted,
                "final_pass",
                &mut ast_diff,
            );
        }

        // Phase 7: unanchored-move fallback (`solve_moved_subtrees`). Dead last, after even final
        // APTED, by necessity - not a stylistic choice. Every phase above (1-6) requires *some*
        // anchor before it will consider a pair: a hash, a name, a shared matched ancestor, an
        // arm-signature overlap, or containment inside a named item. Content that relocated
        // between two containers that *both* changed identity (renamed, or one deleted and a new
        // one inserted) has none of those - the old container is gone, the new one has never been
        // seen before, so there is nothing to anchor to on either side until this point, when
        // "still fully unmatched after everything else, including ordered tree edit distance,
        // already had its shot" finally becomes a usable signal in its own right. Ordered TED
        // (phase 6) structurally cannot express a cross-tree relocation as anything but
        // delete+insert either - this is the only phase that can still convert that into a match.
        // See TODO.md's `solve_moved_subtrees` vs. phase 4 comparison for why phase 4's four
        // anchor-requiring mechanisms can't substitute for this.
        if config.solver_moved_subtrees {
            solve_moved_subtrees::solve(before, after, &node_cache, &mut ast_diff);
        }

        Diff {
            ast: Some(ast_diff),
            language: before.metadata.language.unwrap_or(Language::Unknown),
            text: None,
        }
    }
}

/**
* Per-pass on/off switches for [`Diff::from_code_with_config`]'s seven-phase pipeline, for the
* passes whose accuracy contribution is ambiguous enough to be worth re-measuring independently.
* [`HeuristicConfig::default`] is what plain [`Diff::from_code`]/[`diff_code`] use, and is the
* only configuration any production caller should need.
*
* Defaults were tuned by a 2026-07-15 leave-one-out ablation study over the `optimal_solutions`
* benchmark corpus (see `ablation_study.sh`, `research/ablation/`), run against the pipeline that
* existed at the time: `solver_import_nodes` and `solver_bottom_up_expansion` (gates both of
* phases 3 and 5) default to `false` because each was found net-negative when disabled
* individually (i.e. removing it *improved* the benchmark) - `-89` and `-69` respectively. A third
* knob, `solver_similar_flow_control` (`-82`, same study), gated `solve_similar_flow_control`
* (arm-overlap matching); that pass was deleted outright 2026-08-14 (net-negative, disabled by
* default since the study and never re-enabled - nothing left to gate), so only 3 fields remain.
* The seven-phase pipeline rework (`TODO.md`, 2026-07-17/18) that replaced the original ~15-pass
* pipeline these were tuned against kept these knobs, on the same passes, at the same defaults -
* nothing about the rework changes any of these signals' own accuracy contribution.
*
* Exists for the ablation study in `ablation_study.sh` (via `benchmark_optimal_solutions
* --no-solver-X`/`--solver-X`): disabling or enabling exactly one pass and comparing accuracy
* against the default baseline measures that pass's actual contribution, the way no amount of
* reading `ASTMappingReason` counts can (several passes share a reason, or emit none of their own
* - see that enum's doc comments).
*/
#[derive(Debug, Clone, Copy)]
pub struct HeuristicConfig {
    pub solver_import_nodes: bool,
    pub solver_bottom_up_expansion: bool,
    pub solver_moved_subtrees: bool,
}

impl Default for HeuristicConfig {
    fn default() -> Self {
        Self {
            // Net-negative when disabled individually (i.e. removing it *improved* the
            // benchmark) - ablation 2026-07-15.
            solver_import_nodes: false,
            solver_bottom_up_expansion: false,
            solver_moved_subtrees: true,
        }
    }
}

/**
* Difference between two Code structures, based on their TreeSitter ASTs.
*/
#[derive(Debug, Clone, Default)]
pub struct ASTDiff {
    /// Map of AST nodes from the before AST to the after AST.
    ///
    /// `FxHashMap`, not `std::collections::HashMap` on this and the two maps below - same
    /// rationale as `ASTMetadata::node_to_parent` (`code.rs`): read via `.contains_key()`/`.get()`
    /// on every candidate node considered by every pipeline phase, so SipHash's per-process random
    /// reseed is pure overhead here. Converted 2026-07-26 alongside `ASTMetadata`'s remaining maps
    /// and `NodeCache`. Direct iteration of all three (not just `.get()`/`.contains_key()`) was
    /// audited first: every call site either only exists in `#[test]`s doing order-independent
    /// assertions, or (like `solve_leading_siblings::solve`'s `current_mappings` snapshot, which
    /// only freezes the *anchor* list - the "already matched" checks inside its chain-walk read
    /// live, deliberately, so a later anchor's walk sees an earlier anchor's just-added matches)
    /// is provably order-independent by construction: which specific leading-sibling pair a given
    /// hop can possibly claim is fixed by sibling structure and text equality alone, not by which
    /// anchor's turn it is, so no two anchors can ever race for the same pair with different
    /// outcomes.
    pub mapping: rustc_hash::FxHashMap<(usize, usize), ASTMapping>,
    /// Map of nodes from the before tree to the after tree, or 0 if it is a delete.
    /// Useful if you are walking the before tree and need to look up the mapping.
    pub before_node_map: rustc_hash::FxHashMap<usize, usize>,
    /// Map of nodes from the after tree to the before tree, or 0 if it is an insert.
    /// Useful if you are walking the after tree and need to look up the mapping.
    pub after_node_map: rustc_hash::FxHashMap<usize, usize>,
}

impl ASTDiff {
    /**
     * Add a mapping between two nodes to the diff.
     *
     * Note that either id can be zero. Zero is used for insert or delete.
     */
    pub fn add_mapping(&mut self, before_id: usize, after_id: usize, mapping: ASTMapping) {
        self.mapping.insert((before_id, after_id), mapping);
        self.before_node_map.insert(before_id, after_id);
        self.after_node_map.insert(after_id, before_id);
    }

    /**
     * Removes a `(before_id, 0)` delete mapping, so the node can be re-mapped to a real partner.
     *
     * Deliberately only defined for *null* mappings (deletes/inserts): removing a real pair would
     * also need to fix up the partner's reverse entry, which no caller needs today. Note the
     * shared `after_node_map[0]` reverse slot is left alone - every delete overwrites it, so it
     * holds an arbitrary earlier delete's id by design and nothing may rely on it.
     */
    pub fn remove_delete_mapping(&mut self, before_id: usize) {
        self.mapping.remove(&(before_id, 0));
        self.before_node_map.remove(&before_id);
    }

    /// Removes a `(0, after_id)` insert mapping - see `remove_delete_mapping`.
    pub fn remove_insert_mapping(&mut self, after_id: usize) {
        self.mapping.remove(&(0, after_id));
        self.after_node_map.remove(&after_id);
    }

    /**
     * Checks that the mapping is valid for given trees.
     *
     * Useful in tests.
     */
    pub fn is_valid(&self, before: &Code, _after: &Code, node_cache: &NodeCache) -> bool {
        let language = before.metadata.language.unwrap_or_default();

        // Check that each mapping only maps nodes of the same type, or a hand-picked cross-kind
        // exception (see `crate::diff::nodes::kinds_update_allowed`).
        for (before_id, after_id) in self.mapping.keys() {
            if *before_id == 0 || *after_id == 0 {
                // Null-mapping is an Insert/Delete.
                continue;
            }

            // Get nodes from cache - if not found, mapping is invalid
            let before_node = node_cache.before.get(before_id);
            let after_node = node_cache.after.get(after_id);

            // If we can't get either node, the mapping is invalid
            if before_node.is_none() || after_node.is_none() {
                return false;
            }

            // Check that the node types match
            let before_node = before_node.unwrap();
            let after_node = after_node.unwrap();
            if before_node.kind() != after_node.kind()
                && !crate::diff::nodes::kinds_update_allowed(
                    before_node.kind(),
                    after_node.kind(),
                    &language,
                )
            {
                return false;
            }
        }

        true
    }

    /**
     * Checks that the mapping covers all nodes in both trees.
     *
     * Useful in tests.
     */
    pub fn is_complete(&self, before: &Code, after: &Code, node_cache: &NodeCache) -> bool {
        // `before_node_map`/`after_node_map` already record exactly "is this node covered by some
        // mapping" (every `add_mapping` call inserts into both) - no need to re-derive that from
        // `self.mapping` into fresh maps first.
        let before_root_id = before.ast.as_ref().unwrap().root_node().id();
        for node_id in node_cache.before.keys() {
            // Check if this is a root node that might not need mapping
            if *node_id != before_root_id && !self.before_node_map.contains_key(node_id) {
                return false;
            }
        }

        let after_root_id = after.ast.as_ref().unwrap().root_node().id();
        for node_id in node_cache.after.keys() {
            // Check if this is a root node that might not need mapping
            if *node_id != after_root_id && !self.after_node_map.contains_key(node_id) {
                return false;
            }
        }

        true
    }

    /**
     * Returns true if the node is mapped in any subtree.
     */
    pub fn is_node_mapped(&self, node_id: &usize) -> bool {
        self.before_node_map.contains_key(node_id) || self.after_node_map.contains_key(node_id)
    }

    /**
     * Returns the mapping for a given node, either side of the diff.
     */
    pub fn mapping_for_node(&self, node_id: &usize) -> Option<(usize, ASTMapping)> {
        if let Some(mapped_id) = self.before_node_map.get(node_id)
            && let Some(mapping) = self.mapping.get(&(*node_id, *mapped_id))
        {
            return Some((*mapped_id, mapping.clone()));
        }
        if let Some(mapped_id) = self.after_node_map.get(node_id)
            && let Some(mapping) = self.mapping.get(&(*mapped_id, *node_id))
        {
            return Some((*mapped_id, mapping.clone()));
        }
        None
    }
}

/**
* Information about the mapping of two AST subtrees.
*/
#[derive(Debug, Clone, Default)]
pub struct ASTMapping {
    /// The cost of the mapping.
    ///
    /// The cost is recursively defined the cost to match the root nodes plus the total cost to match
    /// the subtrees of the root nodes. The cost to match the root nodes corresponds to the
    /// operation required to match the nodes in the diff script: a delete, insert, update or move.
    ///
    /// The cost strategy is dynamic, but a common cost strategy is unit cost (1) for delete,
    /// insert and update and a free (0 cost) move.
    pub cost: u64,
    /// What operation has to be done to the root nodes to make this mapping valid?
    pub operation: ASTMappingOperation,
    /// Why were the two subtrees mapping together?
    pub reason: ASTMappingReason,
}

/**
* The operations that can be used to transform one tree into another.
*/
#[derive(Debug, Clone, Default, PartialEq)]
pub enum ASTMappingOperation {
    #[default]
    /// Sentinel value.
    NotYetSet,
    /// A meta-operation that only makes sense if the diff is partially solved by other means.
    DoNothing,
    /// No operation is needed. The match is perfect.
    Identical,
    /// The node and its entire subtree is moved to a different parent node.
    Move,
    /// The node's value is updated.
    Update,
    /// The node is inserted between a parent node and a consecutive subsequence of the parent
    /// node's children. Note that the subsequence can be empty.
    Insert,
    /// The node and all its children are inserted. This is a special operation that makes the
    /// algorithm more efficient but also uglier to implement. It results in much shorter edit
    /// scripts and shallower recursion depth, but it changes the domain of operations from "one
    /// node" to "subtrees".
    InsertWithChildren,
    /// The node is deleted and its children, if any, are connected to its parent node. If the
    /// root node is deleted, in theory the children form a forest of trees instead. This only
    /// happens theoretically during some algorithm computations, since a diff script that deletes
    /// the root node would be guaranteed to create invalid code, unless the code is already empty.
    Delete,
    /// The node and all its children are deleted. Same as InsertWithChildren, this is a more
    /// complex operation that results in more efficient code.
    DeleteWithChildren,
    /// The node maps to a different node, but not all of their children are identical.
    MatchButNotIdentical,
}

/**
* Why were the two subtrees mapped to each other?
*/
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum ASTMappingReason {
    #[default]
    /// The hash of the nodes and their subtrees is identical and so they were matched. Note that
    /// typically leaf nodes will never get this mapping reason, since mapping common nodes, e.g.
    /// ";" to random other ";" in the code is extremely confusing and unnatural to humans.
    IdenticalHash,
    /// This node is part of a subtree that got matched when a parent or another ancestor node was
    /// matched due to identical hashes. Technically, this should imply this matching also has
    /// identical hashes, but we want to differentiate between the two for debugging, visualization
    /// and explainability.
    IdenticalHashOfAncestor,
    /// The node's children were reordered inside a `nodes::is_commutative_container` parent (e.g.
    /// enum variants, struct fields, import lists) but are otherwise unchanged - distinguishes
    /// "this is genuinely untouched" (`IdenticalHash`/`StructurallyIdenticalSubtrees`) from "this
    /// matched only because the new pipeline's hashes are order-independent for commutative
    /// containers, and the order actually did change" (this variant). Before the seven-phase
    /// pipeline rework (`TODO.md`, 2026-07-17/18), this reason was produced by the now-removed
    /// `solve_commutative_structural_trees` pass, which used a separate third hash dedicated to
    /// order-independence; the rework folded that order-independence directly into
    /// `KindAndValueHash`/`KindOnlyHash` (see `code::hash::compute_kind_and_value_hash`'s doc
    /// comment), which fixed a real propagation bug but also meant a reordered-but-unchanged
    /// container would otherwise silently collapse into the same `IdenticalHash` reason as a
    /// container that didn't change at all - undetectable by a diff viewer. Repurposed
    /// (2026-07-18, at the user's request: "we do need a way to distinguish between truly
    /// identical and reordered") to keep making that distinction under the new mechanism - see
    /// `hash_tree_matching::pair_children_for_descent`'s reorder detection.
    FullyMappingSubtrees,
    /// The subtrees of the nodes are structurally identical, but the values of leaf nodes differ.
    /// E.g., a constant value was changed but the code structure is identical.
    StructurallyIdenticalSubtrees,
    /// This node is part of a subtree that got matched when a parent or another ancestor node was
    /// matched due to structure of the code. Note that it is possible for these nodes to be
    /// identical in both type or value, or they can only be identical in type. If they are only
    /// identical in type, it implies a "update()" command in the diff script, thus increasing the
    /// cost of the script by 1.
    StructurallyIdenticalAncestor,
    /// Using highly modified edit distance algorithm it was determined that this is the optimal
    /// mapping if only Insert, Delete, Update and Identical operations are allowed.
    OptimalIDU,
    /// Mapping determined by the APTED (A Preorder Tree Edit Distance) algorithm. Carries the
    /// exact provenance: a short, call-site-specific label identifying which `apted::for_nodes`/
    /// `for_roots` call produced this entry (e.g. `"final_pass"`, `"bottom_up_expansion"`,
    /// `"flow_control_container"`) - threaded all the way from that call site down through
    /// `resolve_forest`'s internal emission helpers (`emit_match`, `add_delete_mappings`, ...), so
    /// two mappings both labeled `APTED` can still be told apart by *which* heuristic actually
    /// invoked the tree-edit-distance algorithm to produce them, not just that one did.
    APTED(&'static str),
    /// Mapping produced by Myers sequence diff on a flat tree (single root, all leaf children).
    FlatSequenceDiff,
    /// A fully-deleted subtree and a byte-identical fully-inserted subtree were paired up after
    /// the main pipeline finished - the code didn't change, it moved (possibly into or out of a
    /// newly-inserted wrapper). See `solve_moved_subtrees`.
    MovedSubtree,
    /// A comment or attribute/decorator node that was matched because it's part of an unbroken
    /// run of unmatched, textually-identical leading siblings immediately preceding an
    /// already-matched node on both before and after sides. See `solve_leading_siblings`.
    LeadingSibling,
    /// Neither node was matched directly, but enough of its descendants were already matched to
    /// the other's descendants (see `DICE_THRESHOLD`) that the two nodes were matched too. See
    /// `solve_bottom_up_expansion`.
    BottomUpExpansion,
    /// Neither node was matched by name, arm structure, or already-matched descendants - instead
    /// a fast sequence-alignment cost estimate over the two nodes' direct children, gated on a
    /// shared positional anchor (see `MAX_COST_RATIO`), found them cheap enough to anchor
    /// together, greedily. See `solve_greedy_anchor_blocks`.
    GreedyAnchorBlock,
    /// Import statements matched by normalized path (quotes stripped, separators normalized,
    /// relative import prefixes handled) - a `KindAndValueHash` variant, see `solve_hash_descent`.
    NormalizedImportPath,
}

impl ASTMappingReason {
    /// Short, stable column/label abbreviation for a mapping reason, independent of `APTED`'s
    /// provenance payload (bucketed to the bare `"APTED"`). Shared by `src/bin/human_solver.rs`'s
    /// per-node display and `src/bin/benchmark_optimal_solutions.rs`'s reason-count columns, so
    /// the same abbreviation means the same thing in both tools - previously two independently
    /// hand-maintained copies of this match that could silently drift when a variant was added.
    /// Callers that need the `APTED` provenance itself (e.g. one CSV column per call site)
    /// should match `ASTMappingReason::APTED(source)` directly instead of using this label.
    pub fn bucket_label(&self) -> &'static str {
        match self {
            ASTMappingReason::IdenticalHash => "IdHash",
            ASTMappingReason::IdenticalHashOfAncestor => "IdHashAnc",
            ASTMappingReason::FullyMappingSubtrees => "FullMap",
            ASTMappingReason::StructurallyIdenticalSubtrees => "StructId",
            ASTMappingReason::StructurallyIdenticalAncestor => "StructAnc",
            ASTMappingReason::OptimalIDU => "OptIDU",
            ASTMappingReason::APTED(_) => "APTED",
            ASTMappingReason::FlatSequenceDiff => "FlatSeq",
            ASTMappingReason::MovedSubtree => "Moved",
            ASTMappingReason::LeadingSibling => "LeadSib",
            ASTMappingReason::BottomUpExpansion => "BottomUp",
            ASTMappingReason::GreedyAnchorBlock => "GreedyAnchor",
            ASTMappingReason::NormalizedImportPath => "NormImport",
        }
    }
}

/**
* Creates a Diff from two Code objects.
*
* This is a convenience wrapper around Diff::from_code for backwards compatibility.
* The algorithm is encoded in the code and is intentionally not explained in the Doccomment to
* avoid it going stale. Please see the code.
*/
pub fn diff_code(before: &Code, after: &Code) -> Diff {
    Diff::from_code(before, after)
}

/// Same as [`diff_code`], but forwards `config` to [`Diff::from_code_with_config`] - see that
/// function and [`HeuristicConfig`] for what it's for.
pub fn diff_code_with_config(before: &Code, after: &Code, config: &HeuristicConfig) -> Diff {
    Diff::from_code_with_config(before, after, config)
}

#[cfg(test)]
mod tests {
    use crate::{
        code::{Code, Language},
        test,
    };
    use anyhow::Result;

    use super::*;

    /// Regression test for a real panic: `diff_code`/`from_code` used to `unwrap()` a `None`
    /// `Code.ast` whenever either side's language has no tree-sitter grammar (`Language::Unknown`
    /// and several others - `Code::from_string` deliberately leaves `ast: None` for those, a
    /// valid state, not a bug). `Diff`'s own doc comment promises every function taking a `Code`
    /// should "fail-safe... returning a safe zero result" - this pins that phase 6 (`for_roots`/
    /// `for_roots_fallback`) actually honors that instead of crashing.
    #[test]
    fn diff_code_does_not_panic_when_language_is_unknown() {
        let before = Code::from_string("this is not code, just text", &Language::Unknown);
        let after = Code::from_string("this is different text now", &Language::Unknown);

        let diff = diff_code(&before, &after);

        assert!(
            diff.ast.is_some(),
            "should still produce an (empty) ASTDiff, not panic"
        );
    }

    /// Same guard for the `DiffMode::Exact` path, which goes through the same `for_roots` call.
    #[test]
    fn diff_code_with_exact_mode_does_not_panic_when_language_is_unknown() {
        let before = Code::from_string("this is not code, just text", &Language::Unknown);
        let after = Code::from_string("this is different text now", &Language::Unknown);

        let diff = Diff::pending(&before, &after).finish(DiffMode::Exact);

        assert!(diff.ast.is_some());
    }

    /// Regression guard for the pathological case that motivated `DiffMode`: two files with
    /// nothing structurally in common (see the fixture's own `before.rs.test`/`after.rs.test`)
    /// used to take 14.5s under the old always-exact pipeline, ~36x this project's 400ms budget,
    /// because phases 1-5 left the bulk of both trees unmatched and full APTED had almost nothing
    /// to prune against. Under the default `DiffMode::Fast`, `PendingDiff::looks_expensive()`
    /// should trip and substitute the cheap fallback well before that. Run under `--release` (see
    /// this crate's `[profile.release]`) - the bound here is generous specifically to avoid
    /// flakiness in an unoptimized debug build, not because the fast path is expected to need it.
    #[test]
    fn rust_completely_unrelated_main_files_resolves_fast_under_default_fast_mode() -> Result<()> {
        let (before, after) =
            test::helper::handmade_test_code_pair("rust-completely-unrelated-main-files")?;

        let pending = Diff::pending(&before, &after);
        assert!(
            pending.looks_expensive(),
            "this fixture's residual (~40%/~86% unmatched) should trip the guard - counts: {:?}",
            pending.unmatched_counts()
        );

        let started = std::time::Instant::now();
        let diff = Diff::from_code(&before, &after);
        let elapsed = started.elapsed();

        assert!(diff.ast.is_some());
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "expected DiffMode::Fast's guard to substitute the cheap fallback, took {elapsed:?}"
        );
        Ok(())
    }

    #[test]
    fn test_compute_metadata() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;
        let code = test_codes.get("hello-world.rs").unwrap().clone();

        // Compute metadata
        let metadata = crate::code::metadata::compute_ast_metadata(&code)?;

        // Verify metadata is computed
        assert!(!metadata.node_to_full_hash.is_empty());
        assert!(!metadata.full_hash_to_node.is_empty());
        assert!(!metadata.node_to_structural_hash.is_empty());
        assert!(!metadata.structural_hash_to_node.is_empty());
        assert!(!metadata.reference_nodes_ordered.is_empty());

        // The first, largest reference node must always be the root.
        let root_id = code.ast.as_ref().unwrap().root_node().id();

        assert_eq!(metadata.reference_nodes_ordered[0], root_id);
        assert_eq!(metadata.reference_nodes_ordered.len(), 2);

        Ok(())
    }

    #[test]
    fn diff_empty_rust_code() -> Result<()> {
        let before = Code::from_string("", &Language::Rust);
        let after = Code::from_string("", &Language::Rust);

        let diff = diff_code(&before, &after);

        assert!(diff.ast.is_some());
        let diff_ast = diff.ast.unwrap();

        // Rust treesitter files contain a source_file node even when empty.
        // The algorithm should map this node correctly.
        assert_eq!(diff_ast.mapping.len(), 1);

        Ok(())
    }

    #[test]
    fn diff_identical_rust_code() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;
        let before = test_codes.get("hello-world.rs").unwrap().clone();
        let after = test_codes.get("hello-world.rs").unwrap().clone();

        let diff = diff_code(&before, &after);

        assert!(diff.ast.is_some());
        let diff_ast = diff.ast.unwrap();

        // The hello-world.rs TreeSitter AST has 22 nodes.
        // It looks like this:
        //
        // source_file
        //   function_item
        //     fn
        //     identifier
        //     parameters
        //       (
        //       )
        //     block
        //       {
        //       expression_statement
        //         macro_invocation
        //           identifier
        //           !
        //           token_tree
        //             (
        //             string_literal
        //               "
        //               string_content
        //               "
        //             )
        //         ;
        //       }
        //
        //  The code is identical, so the minimal diff script is just empty.
        assert_eq!(diff_ast.mapping.len(), 22);

        // The root node of before should match to the root node of after, and the reason should be
        // an identical hash. All other nodes should have the reason being an identical hash of
        // ancestors.

        // Check that root node has IdenticalHash reason
        let before_root_id = before.ast.as_ref().unwrap().root_node().id();
        let after_root_id = after.ast.as_ref().unwrap().root_node().id();

        let root_mapping = diff_ast
            .mapping
            .get(&(before_root_id, after_root_id))
            .expect("Root node should be mapping");
        assert_eq!(root_mapping.reason, ASTMappingReason::IdenticalHash);

        // A fully identical code can never have a cost.
        assert_eq!(root_mapping.cost, 0);

        // Check that all other nodes have IdenticalHashOfAncestor reason
        for ((before_id, after_id), mapping) in &diff_ast.mapping {
            if *before_id != before_root_id && *after_id != after_root_id {
                assert_eq!(mapping.reason, ASTMappingReason::IdenticalHashOfAncestor);
            }
        }

        Ok(())
    }

    #[test]
    fn diff_hello_world_with_translated_string() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;
        let before = test_codes.get("hello-world.rs").unwrap().clone();
        // The after code is the same basic Rust hello world, but the message is in Croatian
        // instead of in English. So there is exactly 1 different node, the string_content node,
        // and only the value of the node is different, going from "Hello, World" to "Zdravo,
        // Svijete".
        //
        // But because this node is deep in the tree, the following nodes no longer have exact same
        // hashes:
        //
        // source_file
        //   function_item
        //     block
        //       expression_statement
        //         macro_invocation
        //           token_tree
        //             string_literal
        //               string_content
        //
        //  The optimal smallest diff script is
        //
        //  update(<string content node>, "Zdravo, Svijete")
        let after = test_codes.get("zdravo-svijete.rs").unwrap().clone();

        let diff = diff_code(&before, &after);

        assert!(diff.ast.is_some());
        let diff_ast = diff.ast.unwrap();

        // One mapping is an update, but it still maps.
        assert_eq!(diff_ast.mapping.len(), 22);

        let before_ast = before.ast.unwrap();
        let after_ast = after.ast.unwrap();

        let before_string_node = test::helper::node_for_path(
            before_ast.root_node(),
            &[
                "function_item",
                "block",
                "expression_statement",
                "macro_invocation",
                "token_tree",
                "string_literal",
                "string_content",
            ],
        )?;
        let after_string_node = test::helper::node_for_path(
            after_ast.root_node(),
            &[
                "function_item",
                "block",
                "expression_statement",
                "macro_invocation",
                "token_tree",
                "string_literal",
                "string_content",
            ],
        )?;

        // Verify that these nodes are mapped in the diff
        let before_node_id = before_string_node.id();
        let after_node_id = after_string_node.id();

        let mapping = diff_ast.mapping.get(&(before_node_id, after_node_id));
        assert!(mapping.is_some(), "String content nodes should be mapped");

        let mapping = mapping.unwrap();

        // The mapping should be an Update operation since the content changed
        assert_eq!(
            mapping.operation,
            ASTMappingOperation::Update,
            "String content mapping should be an Update operation"
        );

        // The reason should be StructurallyIdenticalAncestor since this node is part of a
        // structurally identical subtree (the parent nodes match structurally)
        assert_eq!(
            mapping.reason,
            ASTMappingReason::StructurallyIdenticalAncestor,
            "String content mapping reason should be StructurallyIdenticalAncestor"
        );

        // The cost should be COST_UPDATE (1)
        assert_eq!(
            mapping.cost, COST_UPDATE,
            "String content mapping cost should be COST_UPDATE"
        );

        Ok(())
    }

    #[test]
    fn identical_code_must_always_match() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;

        for (filename, code) in &test_codes {
            let diff = diff_code(code, code);

            assert!(
                diff.ast.is_some(),
                "AST diff should be computed for {}",
                filename
            );
            let diff_ast = diff.ast.unwrap();

            let node_cache = NodeCache::build(code, code);
            assert!(
                diff_ast.is_valid(code, code, &node_cache),
                "Identical code must always produce a valid diff: {}",
                filename
            );
            assert!(
                diff_ast.is_complete(code, code, &node_cache),
                "Identical code must always produce a complete diff: {}",
                filename
            );

            let before_root_id = code.ast.as_ref().unwrap().root_node().id();
            let after_root_id = code.ast.as_ref().unwrap().root_node().id();

            // Check that root node has IdenticalHash reason
            let root_mapping = diff_ast
                .mapping
                .get(&(before_root_id, after_root_id))
                .expect("Root node should be mapping");
            assert_eq!(
                root_mapping.reason,
                ASTMappingReason::IdenticalHash,
                "Root node should have IdenticalHash reason for {}",
                filename
            );

            // A fully identical code can never have a cost.
            assert_eq!(root_mapping.cost, 0);

            // Check that all other nodes have IdenticalHashOfAncestor reason
            for ((before_id, after_id), mapping) in &diff_ast.mapping {
                if *before_id != before_root_id || *after_id != after_root_id {
                    assert_eq!(
                        mapping.reason,
                        ASTMappingReason::IdenticalHashOfAncestor,
                        "Non-root node should have IdenticalHashOfAncestor reason for {}, got {:?}",
                        filename,
                        mapping.reason
                    );
                }
            }
        }

        Ok(())
    }

    #[test]
    fn hello_world_translations_in_all_languages() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;

        for (filename, before) in &test_codes {
            if !filename.starts_with("hello-world") {
                continue;
            }
            let after = test_codes
                .get(&filename.replace("hello-world", "zdravo-svijete"))
                .unwrap()
                .clone();

            let diff = diff_code(before, &after);

            assert!(
                diff.ast.is_some(),
                "AST diff should be computed for {}",
                filename
            );
            let diff_ast = diff.ast.unwrap();

            let before_root_id = before.ast.as_ref().unwrap().root_node().id();
            let after_root_id = after.ast.as_ref().unwrap().root_node().id();

            // Check that root node has IdenticalHash reason
            let root_mapping = diff_ast
                .mapping
                .get(&(before_root_id, after_root_id))
                .expect("Root node should be mapping");
            assert_eq!(
                root_mapping.reason,
                ASTMappingReason::StructurallyIdenticalSubtrees,
                "Root node should have StructurallyIdenticalSubtrees reason for {}",
                filename
            );

            // The root is an interior node (it has children), so per the leaf-vs-interior rule
            // `classify`/`classify_match` both use (2026-07-23 fix - see `hash_tree_matching.rs`'s
            // `classify` closure doc comment), it's `MatchButNotIdentical` at cost 0, not `Update`:
            // `Update` is reserved for a *leaf's own* value changing, and the cost of that one
            // translated string literal is carried by the leaf itself, not double-counted here too.
            assert_eq!(
                root_mapping.operation,
                ASTMappingOperation::MatchButNotIdentical,
                "Root node (an interior node) should be MatchButNotIdentical, not Update, for {}",
                filename
            );
            assert_eq!(root_mapping.cost, 0);

            // Every other node should be mapped as part of the root's structural match. Usually
            // that's `StructurallyIdenticalAncestor`, but a node that's *also* a reference node
            // (e.g. C/C++'s `#include`) can get claimed first by the separate reference-hash
            // matching pass, which runs before the whole-tree structural check and doesn't know
            // the whole tree is about to match anyway - so it's tagged `IdenticalHash`/
            // `IdenticalHashOfAncestor` instead. The actual mapping (which node pairs with which)
            // is still correct either way; only the recorded reason differs, so accept all three.
            for ((before_id, after_id), mapping) in &diff_ast.mapping {
                if *before_id != before_root_id || *after_id != after_root_id {
                    assert!(
                        matches!(
                            mapping.reason,
                            ASTMappingReason::StructurallyIdenticalAncestor
                                | ASTMappingReason::IdenticalHash
                                | ASTMappingReason::IdenticalHashOfAncestor
                        ),
                        "Non-root node should have StructurallyIdenticalAncestor, IdenticalHash \
                         or IdenticalHashOfAncestor reason for {}, got {:?}",
                        filename,
                        mapping.reason
                    );
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_is_valid_with_identical_code() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;
        let before = test_codes.get("hello-world.rs").unwrap().clone();
        let after = test_codes.get("hello-world.rs").unwrap().clone();

        let diff = diff_code(&before, &after);
        let diff_ast = diff.ast.unwrap();

        // The mapping should be valid for identical code
        let node_cache = NodeCache::build(&before, &after);
        assert!(diff_ast.is_valid(&before, &after, &node_cache));

        Ok(())
    }

    #[test]
    fn test_is_valid_with_different_code() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;
        let before = test_codes.get("hello-world.rs").unwrap().clone();
        let after = test_codes.get("zdravo-svijete.rs").unwrap().clone();

        let diff = diff_code(&before, &after);
        let diff_ast = diff.ast.unwrap();

        // The mapping should still be valid for different code as long as node types match
        let node_cache = NodeCache::build(&before, &after);
        assert!(diff_ast.is_valid(&before, &after, &node_cache));

        Ok(())
    }

    #[test]
    fn test_is_valid_with_invalid_mapping() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;
        let before = test_codes.get("hello-world.rs").unwrap().clone();
        let after = test_codes.get("hello-world.rs").unwrap().clone();

        let mut diff = diff_code(&before, &after);
        let diff_ast = diff.ast.as_mut().unwrap();

        // Create an invalid mapping by mapping nodes of different types
        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();

        let before_root = before_ast.root_node();
        let after_root = after_ast.root_node();

        // Find nodes of different types by traversing the tree
        let mut before_cursor = before_root.walk();
        let mut after_cursor = after_root.walk();

        // Get the function_item node (first child of root)
        let before_function_item = before_root.children(&mut before_cursor).next().unwrap();
        let after_function_item = after_root.children(&mut after_cursor).next().unwrap();

        // Now get different types of nodes - function_item vs some leaf node
        let mut before_leaf_cursor = before_function_item.walk();
        let mut after_leaf_cursor = after_function_item.walk();

        // Find a leaf node (like identifier or string_literal)
        let before_leaf = before_function_item
            .children(&mut before_leaf_cursor)
            .find(|child| child.kind() == "identifier")
            .unwrap();
        let after_leaf = after_function_item
            .children(&mut after_leaf_cursor)
            .find(|child| child.kind() == "block")
            .unwrap();

        // Create an invalid mapping by mapping different types
        let invalid_before_id = before_leaf.id();
        let invalid_after_id = after_leaf.id();

        // Clear existing mapping and add invalid one
        diff_ast.mapping.clear();
        diff_ast.mapping.insert(
            (invalid_before_id, invalid_after_id),
            ASTMapping {
                cost: 0,
                operation: ASTMappingOperation::Identical,
                reason: ASTMappingReason::IdenticalHash,
            },
        );

        // The mapping should be invalid
        let node_cache = NodeCache::build(&before, &after);
        assert!(
            !diff_ast.is_valid(&before, &after, &node_cache),
            "Mapping should be invalid for different node types: {} vs {}",
            before_leaf.kind(),
            after_leaf.kind()
        );

        Ok(())
    }

    #[test]
    fn test_is_valid_with_null_mapping() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;
        let before = test_codes.get("hello-world.rs").unwrap().clone();
        let after = test_codes.get("hello-world.rs").unwrap().clone();

        let mut diff = diff_code(&before, &after);
        let diff_ast = diff.ast.as_mut().unwrap();

        // Clear existing mapping and add a null mapping (ID 0 represents insert/delete)
        diff_ast.mapping.clear();
        diff_ast.add_mapping(
            0,
            123, // 0 represents a null node (insert)
            ASTMapping {
                cost: COST_INSERT,
                operation: ASTMappingOperation::Insert,
                reason: ASTMappingReason::OptimalIDU,
            },
        );
        diff_ast.add_mapping(
            456,
            0, // 0 represents a null node (delete)
            ASTMapping {
                cost: COST_DELETE,
                operation: ASTMappingOperation::Delete,
                reason: ASTMappingReason::OptimalIDU,
            },
        );

        // Null mappings should be considered valid
        let node_cache = NodeCache::build(&before, &after);
        assert!(
            diff_ast.is_valid(&before, &after, &node_cache),
            "Null mappings (insert/delete) should be valid"
        );

        Ok(())
    }

    #[test]
    fn test_add_mapping_updates_all_maps() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;
        let before = test_codes.get("hello-world.rs").unwrap().clone();
        let after = test_codes.get("hello-world.rs").unwrap().clone();

        let mut diff = ASTDiff {
            ..Default::default()
        };

        let before_root_id = before.ast.as_ref().unwrap().root_node().id();
        let after_root_id = after.ast.as_ref().unwrap().root_node().id();

        // Add a mapping using the new method
        diff.add_mapping(
            before_root_id,
            after_root_id,
            ASTMapping {
                cost: 0,
                operation: ASTMappingOperation::Identical,
                reason: ASTMappingReason::IdenticalHash,
            },
        );

        // Verify that all maps were updated correctly
        assert_eq!(diff.mapping.len(), 1);
        assert_eq!(diff.before_node_map.len(), 1);
        assert_eq!(diff.after_node_map.len(), 1);

        // Check that the main mapping contains the correct entry
        assert!(diff.mapping.contains_key(&(before_root_id, after_root_id)));

        // Check that before_node_map contains the correct mapping
        assert_eq!(
            diff.before_node_map.get(&before_root_id),
            Some(&after_root_id)
        );

        // Check that after_node_map contains the correct mapping
        assert_eq!(
            diff.after_node_map.get(&after_root_id),
            Some(&before_root_id)
        );

        Ok(())
    }

    #[test]
    fn test_diff_populates_all_maps() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;
        let before = test_codes.get("hello-world.rs").unwrap().clone();
        let after = test_codes.get("hello-world.rs").unwrap().clone();

        let diff = diff_code(&before, &after);
        let diff_ast = diff.ast.unwrap();

        // All maps should have the same number of entries
        assert_eq!(diff_ast.mapping.len(), diff_ast.before_node_map.len());
        assert_eq!(diff_ast.mapping.len(), diff_ast.after_node_map.len());

        // For identical code, we should have a complete mapping
        assert!(!diff_ast.mapping.is_empty());

        // Verify that the maps are consistent
        for (before_id, after_id) in diff_ast.mapping.keys() {
            assert_eq!(diff_ast.before_node_map.get(before_id), Some(after_id));
            assert_eq!(diff_ast.after_node_map.get(after_id), Some(before_id));
        }

        Ok(())
    }
}
