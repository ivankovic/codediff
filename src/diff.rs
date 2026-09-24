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
pub mod solve_bottom_up_propagation;
pub mod solve_greedy_anchor_blocks;
pub mod solve_hash_descent;
pub mod solve_heritage_clause_growth;
pub mod solve_identical_diagnostic_statements;
pub mod solve_large_flat_subtrees;
pub mod solve_leading_siblings;
pub mod solve_leaf_neighbour_agreement;
pub mod solve_moved_subtrees;
pub mod solve_mutual_ancestors;
pub mod solve_nested_condition_collapse;
pub mod solve_orphaned_leaves;
pub mod solve_syntax_aware_matching;
pub mod solve_unique_type_matching;
pub mod solve_unresolved_nodes;
pub mod solve_wrap_growth;
pub mod text;
pub mod text_range;

use tree_sitter::Node;

use crate::code::{Code, Language};
use crate::diff::text::TextDiff;

/// Every node of both sides' ASTs, keyed by node ID.
#[derive(Debug, Clone, Default)]
/// # Safety invariant
///
/// The `'static` lifetime is erased via `transmute` in `build`: each node really borrows from the
/// `tree_sitter::Tree` of the `Code` it was built from. A `NodeCache` must never be read from, or
/// outlive, that `Code`, nor survive a reparse of its `ast`. The type system does not enforce
/// this; breaking it is silent undefined behaviour.
pub struct NodeCache {
    /// `FxHashMap` because this small-integer-keyed map is read on every lookup of every pass,
    /// and SipHash's DoS resistance buys nothing here.
    pub before: rustc_hash::FxHashMap<usize, tree_sitter::Node<'static>>,
    pub after: rustc_hash::FxHashMap<usize, tree_sitter::Node<'static>>,
}

impl NodeCache {
    /// The returned cache must not outlive `before`/`after` (see the safety invariant on
    /// `NodeCache`). A side without an AST gets an empty map.
    pub fn build(before: &Code, after: &Code) -> Self {
        NodeCache {
            before: Self::cache_for(before),
            after: Self::cache_for(after),
        }
    }

    fn cache_for(code: &Code) -> rustc_hash::FxHashMap<usize, tree_sitter::Node<'static>> {
        code.ast
            .as_ref()
            .map(|ast| {
                let mut cache = rustc_hash::FxHashMap::default();
                let mut stack = vec![ast.root_node()];

                while let Some(node) = stack.pop() {
                    // SAFETY: see the safety invariant on `NodeCache`; this cache must not outlive
                    // `code`.
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

/// The difference between two `Code` values.
///
/// Any function taking this or a sub-field must not assume a field is set: check, try to construct
/// it, and otherwise fail safe with a zero result. That is what lets large files and files that are
/// really data or configuration pass through cheaply, and why most fields are `Option`.
#[derive(Debug, Clone)]
pub struct Diff {
    pub ast: Option<ASTDiff>,
    pub language: Language,
    /// The difference as tree-sitter points, for viewing the code as text.
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
    /// The main entry point: diffs `before` against `after` with every pass enabled. The
    /// pipeline itself is documented at its call sites in `pending_with_config` and
    /// [`PendingDiff::finish`], not here, so it cannot go stale.
    pub fn from_code(before: &Code, after: &Code) -> Self {
        Self::from_code_with_config(before, after, &HeuristicConfig::default())
    }

    /// [`Diff::from_code`] with individual passes switchable, for ablation studies only: later
    /// passes assume every earlier one ran, so a disabled pass changes quality, not just speed.
    pub fn from_code_with_config(before: &Code, after: &Code, config: &HeuristicConfig) -> Self {
        Self::pending_with_config(before, after, config).finish()
    }

    /// Runs phases 1-4 and pauses before phase 6, so a caller can read
    /// [`PendingDiff::large_residual`] before calling [`PendingDiff::finish`].
    pub fn pending<'code>(before: &'code Code, after: &'code Code) -> PendingDiff<'code> {
        Self::pending_with_config(before, after, &HeuristicConfig::default())
    }

    /// [`Diff::pending`] with [`HeuristicConfig`]'s per-pass switches.
    pub fn pending_with_config<'code>(
        before: &'code Code,
        after: &'code Code,
        config: &HeuristicConfig,
    ) -> PendingDiff<'code> {
        let node_cache = NodeCache::build(before, after);
        let ctx = PassCtx::new(before, after, &node_cache);

        let mut ast_diff = ASTDiff {
            ..Default::default()
        };

        // The matching pipeline. Phases 1-4 run here and 6-10 in `PendingDiff::finish`; phases 3
        // and 5 do not exist. 1b, 1c, 8b and 9 re-tag or re-point an existing match rather than
        // make a new one. Each pass's module doc explains its mechanism; the comments here say
        // only why it sits where it does.
        solve_hash_descent::solve(&ctx, &mut ast_diff);

        // Phases 1b and 1c fix up attributions phase 1 cannot make on its own, so they react to
        // exactly what it matched.
        solve_nested_condition_collapse::solve(&ctx, &mut ast_diff);

        solve_heritage_clause_growth::solve(&ctx, &mut ast_diff);

        // Phase 2: cheap, exact matches phase 1 does not reach (diagnostic statements are held
        // back from phase 1 so they do not fragment a bigger match), claimed before the fuzzier
        // phases start guessing.
        solve_leading_siblings::solve(&ctx, &mut ast_diff);
        solve_identical_diagnostic_statements::solve(&ctx, &mut ast_diff);

        // Phase 4 also runs solve_greedy_anchor_blocks and solve_large_flat_subtrees.
        solve_syntax_aware_matching::solve(&ctx, &mut ast_diff);

        // Map lengths count only real matches here: no phase above records a delete or insert.
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

/// What every `solve_*` pass reads: both sides, their [`NodeCache`] and each side's
/// [`ASTMetadata`], resolved once per diff. One shared signature means a new input reaches every
/// pass through one field.
pub struct PassCtx<'a> {
    pub before: &'a Code,
    pub after: &'a Code,
    pub node_cache: &'a NodeCache,
    before_metadata: std::borrow::Cow<'a, crate::code::ASTMetadata>,
    after_metadata: std::borrow::Cow<'a, crate::code::ASTMetadata>,
}

impl<'a> PassCtx<'a> {
    pub fn new(before: &'a Code, after: &'a Code, node_cache: &'a NodeCache) -> Self {
        Self {
            before,
            after,
            node_cache,
            before_metadata: crate::code::metadata::metadata_of(before),
            after_metadata: crate::code::metadata::metadata_of(after),
        }
    }

    /// Borrowed when `Code` already carries its metadata, computed otherwise.
    pub fn before_metadata(&self) -> &crate::code::ASTMetadata {
        &self.before_metadata
    }

    pub fn after_metadata(&self) -> &crate::code::ASTMetadata {
        &self.after_metadata
    }

    /// The before side's language, which every pass uses for both sides.
    pub fn language(&self) -> Language {
        self.before_metadata.language
    }
}

/// Unmatched-node count after phase 4 above which [`PendingDiff::large_residual`] reports that
/// the coarse terminal fallback did most of the matching.
///
/// A diagnostic only (headless mode's stderr note and JSON `large_residual`); `finish` runs the
/// same pipeline either way. The value sits above what normal diffs leave, so re-measure it
/// before using it for anything more than a note.
pub const LARGE_RESIDUAL_THRESHOLD: usize = 5000;

/// A diff paused after phase 4; [`PendingDiff::finish`] runs phases 6-10.
///
/// # Safety invariant
///
/// Holds a [`NodeCache`] under that struct's invariant; the `&'code Code` borrows are what make
/// the compiler refuse to let it outlive the two sides. A caller that pauses for user input must
/// block its own thread rather than hand a `PendingDiff` to another thread.
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
    /// Whether either side's unmatched count exceeds [`LARGE_RESIDUAL_THRESHOLD`].
    pub fn large_residual(&self) -> bool {
        self.unmatched_before.max(self.unmatched_after) > LARGE_RESIDUAL_THRESHOLD
    }

    #[cfg(test)]
    pub fn unmatched_counts(&self) -> (usize, usize) {
        (self.unmatched_before, self.unmatched_after)
    }

    pub fn finish(self) -> Diff {
        let PendingDiff {
            before,
            after,
            node_cache,
            mut ast_diff,
            config,
            ..
        } = self;
        let ctx = PassCtx::new(before, after, &node_cache);

        // Phase 6: whole-file residual resolution. The root-level prematch covers named locals
        // with no enclosing named container (shell assignments), which phase 4's own call never
        // reaches.
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

        // Before the fallback, so every parent it resolves is one less unmatched root the fallback
        // would guess at by lossy whole-subtree hashing.
        if config.solver_bottom_up_propagation {
            solve_bottom_up_propagation::solve(&ctx, &mut ast_diff);
        }

        // After propagation for the most matched parent pairs, before the fallback so its precise
        // pairs are locked in first.
        if config.solver_unique_type_matching {
            solve_unique_type_matching::solve(&ctx, &mut ast_diff);
        }
        apted::for_roots_fallback(before, after, "fast_fallback", &mut ast_diff);

        // Again, so pockets the fallback matched inside a still-unmatched parent bubble up to
        // their now fully resolved ancestors.
        if config.solver_bottom_up_propagation {
            solve_bottom_up_propagation::solve(&ctx, &mut ast_diff);
        }

        // After the fallback and propagation, which produce the parent pairs it anchors on; before
        // phase 7, so its leaves are not left for move recovery to guess at.
        solve_orphaned_leaves::solve(&ctx, &mut ast_diff);

        // Phase 7: every phase above needs an anchor (a hash, a name, a matched ancestor, a named
        // container). Content moved between two containers that both changed identity has none,
        // and "still unmatched after everything else" is only a usable signal from here on.
        if config.solver_moved_subtrees {
            solve_moved_subtrees::solve(&ctx, &mut ast_diff);
        }

        // Phase 8: last among the matching passes, so its lowest-common-ancestor evidence uses
        // the most complete set of matched descendants.
        if config.solver_mutual_ancestors {
            solve_mutual_ancestors::solve(&ctx, &mut ast_diff);
        }

        // Phase 8b reads only neighbours, so it runs once no later pass can add a neighbouring
        // match; before phase 9, so that re-tags the corrected pairing.
        solve_leaf_neighbour_agreement::solve(&ctx, &mut ast_diff);

        // Phase 9: unlike phase 1c, the matched container above the new wrapper is often only
        // resolved by the fallback (`rust-add-if`, `typescript-add-error-handling`); any earlier
        // and it finds no ancestor to climb to and silently does nothing.
        solve_wrap_growth::solve(&ctx, &mut ast_diff);

        // Phase 10 must be last: it records a delete/insert for every undecided node, so a pass
        // after it would find every node claimed.
        solve_unresolved_nodes::solve(&ctx, &mut ast_diff);

        Diff {
            ast: Some(ast_diff),
            language: before.metadata.language.unwrap_or(Language::Unknown),
            text: None,
        }
    }
}

/// Per-pass on/off switches for ablation studies (`ablation_study.sh`, via
/// `benchmark_optimal_solutions --no-solver-X`). Removing exactly one pass measures its
/// contribution in a way `ASTMappingReason` counts cannot, since several passes share a reason.
/// Production callers use [`HeuristicConfig::default`].
#[derive(Debug, Clone, Copy)]
pub struct HeuristicConfig {
    pub solver_moved_subtrees: bool,
    pub solver_bottom_up_propagation: bool,
    /// Rarely fires: by the time it runs, earlier passes have usually resolved every child of a
    /// matched parent, or left several of one kind, which its ambiguity guard refuses. On by
    /// default because it costs nothing, not because it is a proven win.
    pub solver_unique_type_matching: bool,
    pub solver_mutual_ancestors: bool,
}

impl Default for HeuristicConfig {
    fn default() -> Self {
        Self {
            solver_moved_subtrees: true,
            solver_bottom_up_propagation: true,
            solver_unique_type_matching: true,
            solver_mutual_ancestors: true,
        }
    }
}

/// Difference between two Code structures, based on their TreeSitter ASTs.
#[derive(Debug, Clone, Default)]
pub struct ASTDiff {
    /// Every `(before_id, after_id)` pair, with 0 on the missing side of a delete or insert.
    ///
    /// These three maps are `FxHashMap` for lookup speed, so their iteration order is arbitrary:
    /// any code that iterates one must not let the order affect the result.
    pub mapping: rustc_hash::FxHashMap<(usize, usize), ASTMapping>,
    /// Before node to its after partner, or 0 for a delete.
    pub before_node_map: rustc_hash::FxHashMap<usize, usize>,
    /// After node to its before partner, or 0 for an insert.
    pub after_node_map: rustc_hash::FxHashMap<usize, usize>,
}

impl ASTDiff {
    /// Records a pair in all three maps. Either id may be 0, meaning an insert or a delete.
    pub fn add_mapping(&mut self, before_id: usize, after_id: usize, mapping: ASTMapping) {
        self.mapping.insert((before_id, after_id), mapping);
        self.before_node_map.insert(before_id, after_id);
        self.after_node_map.insert(after_id, before_id);
    }

    /// Removes a `(before_id, 0)` delete, so the node can be re-mapped. The shared
    /// `after_node_map[0]` slot is left alone: every delete overwrites it, so nothing may rely on
    /// what it holds.
    pub fn remove_delete_mapping(&mut self, before_id: usize) {
        self.mapping.remove(&(before_id, 0));
        self.before_node_map.remove(&before_id);
    }

    /// Removes a real pair from all three maps, leaving both nodes undecided rather than
    /// deleted/inserted: phase 10's `solve_unresolved_nodes` is the one place that decides what an
    /// undecided node becomes.
    pub fn remove_match_mapping(&mut self, before_id: usize, after_id: usize) {
        self.mapping.remove(&(before_id, after_id));
        self.before_node_map.remove(&before_id);
        self.after_node_map.remove(&after_id);
    }

    /// Removes a `(0, after_id)` insert mapping - see `remove_delete_mapping`.
    pub fn remove_insert_mapping(&mut self, after_id: usize) {
        self.mapping.remove(&(0, after_id));
        self.after_node_map.remove(&after_id);
    }

    /// Whether every real pair joins two nodes that exist and are of the same kind, or of a
    /// cross-kind pair `nodes::kinds_update_allowed` permits. Null mappings always pass.
    pub fn is_valid(&self, before: &Code, _after: &Code, node_cache: &NodeCache) -> bool {
        let language = before.metadata.language.unwrap_or_default();

        for (before_id, after_id) in self.mapping.keys() {
            if *before_id == 0 || *after_id == 0 {
                continue;
            }

            let before_node = node_cache.before.get(before_id);
            let after_node = node_cache.after.get(after_id);

            if before_node.is_none() || after_node.is_none() {
                return false;
            }

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

    /// Whether every node of both trees except the roots has a mapping.
    pub fn is_complete(&self, before: &Code, after: &Code, node_cache: &NodeCache) -> bool {
        let before_root_id = before.ast.as_ref().unwrap().root_node().id();
        for node_id in node_cache.before.keys() {
            if *node_id != before_root_id && !self.before_node_map.contains_key(node_id) {
                return false;
            }
        }

        let after_root_id = after.ast.as_ref().unwrap().root_node().id();
        for node_id in node_cache.after.keys() {
            if *node_id != after_root_id && !self.after_node_map.contains_key(node_id) {
                return false;
            }
        }

        true
    }

    /// The partner id and mapping of a node from either side.
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

/// Information about the mapping of two AST subtrees.
#[derive(Debug, Clone, Default)]
pub struct ASTMapping {
    /// The cost of the root operation plus, where the producing algorithm totals them, its
    /// subtree's.
    pub cost: u64,
    pub operation: ASTMappingOperation,
    pub reason: ASTMappingReason,
}

/// Constructors that pair each operation with its unit cost, so a call site cannot write one
/// without the other. Entries with a computed cost (APTED's subtree totals, `*WithChildren`) are
/// written out in full.
impl ASTMapping {
    pub fn identical(reason: ASTMappingReason) -> Self {
        Self {
            cost: 0,
            operation: ASTMappingOperation::Identical,
            reason,
        }
    }

    /// Same-kind internal nodes whose difference is carried by their descendants' own entries.
    pub fn matched_not_identical(reason: ASTMappingReason) -> Self {
        Self {
            cost: 0,
            operation: ASTMappingOperation::MatchButNotIdentical,
            reason,
        }
    }

    /// A leaf whose text changed. An interior node uses `matched_not_identical` instead.
    pub fn updated(reason: ASTMappingReason) -> Self {
        Self {
            cost: COST_UPDATE,
            operation: ASTMappingOperation::Update,
            reason,
        }
    }

    /// One node; its children, if any, need their own entries.
    pub fn deleted(reason: ASTMappingReason) -> Self {
        Self {
            cost: COST_DELETE,
            operation: ASTMappingOperation::Delete,
            reason,
        }
    }

    /// One node; its children, if any, need their own entries.
    pub fn inserted(reason: ASTMappingReason) -> Self {
        Self {
            cost: COST_INSERT,
            operation: ASTMappingOperation::Insert,
            reason,
        }
    }
}

/// The operations that can be used to transform one tree into another.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum ASTMappingOperation {
    #[default]
    /// Sentinel value.
    NotYetSet,
    /// No operation is needed. The match is perfect.
    Identical,
    /// The node's value is updated.
    Update,
    /// The node is inserted above a consecutive, possibly empty, run of its parent's children.
    Insert,
    /// The whole subtree is inserted: shorter edit scripts and shallower recursion, at the price
    /// of operations on subtrees rather than single nodes.
    InsertWithChildren,
    /// The node is deleted and its children are reattached to its parent.
    Delete,
    /// The whole subtree is deleted; see `InsertWithChildren`.
    DeleteWithChildren,
    /// The node maps to a different node, but not all of their children are identical.
    MatchButNotIdentical,
}

/// Why were the two subtrees mapped to each other?
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum ASTMappingReason {
    #[default]
    /// Identical subtree hashes. Leaves rarely get this: pairing one `;` with an arbitrary other
    /// `;` is meaningless to a reader.
    IdenticalHash,
    /// Matched as part of an `IdenticalHash` ancestor's subtree; kept distinct from it for
    /// explainability.
    IdenticalHashOfAncestor,
    /// A `nodes::is_commutative_container` whose children were reordered but are otherwise
    /// unchanged. Hashes of such containers are order-independent, so without this reason a
    /// reordered container would be indistinguishable from an untouched one. Set by
    /// `hash_tree_matching::pair_children_for_descent`'s reorder detection.
    FullyMappingSubtrees,
    /// Same structure, different leaf values (e.g. a changed constant).
    StructurallyIdenticalSubtrees,
    /// Matched as part of a `StructurallyIdenticalSubtrees` ancestor's subtree; a leaf whose value
    /// differs is an `Update` costing 1.
    StructurallyIdenticalAncestor,
    /// Optimal under insert, delete, update and identical operations only.
    OptimalIDU,
    /// Produced by the tree-edit-distance code in `apted`; the label names the call site that
    /// invoked it (`"fast_fallback"`, `"qualified_name"`, ...).
    APTED(&'static str),
    /// Mapping produced by Myers sequence diff on a flat tree (single root, all leaf children).
    FlatSequenceDiff,
    /// A fully deleted subtree paired with a byte-identical fully inserted one: a move. See
    /// `solve_moved_subtrees`.
    MovedSubtree,
    /// A comment or attribute in an unbroken run of text-identical siblings preceding a matched
    /// pair. See `solve_leading_siblings`.
    LeadingSibling,
    /// Paired greedily by a cheap alignment cost over direct children, gated on a shared
    /// positional anchor. See `solve_greedy_anchor_blocks`.
    GreedyAnchorBlock,
    /// Every child of the before node is decided and all matched ones agree on one after parent
    /// (or all were deleted). No threshold or vote, so it never invents a decision a later pass
    /// must fight. See `solve_bottom_up_propagation`.
    BottomUpPropagation,
    /// The only unmatched child of its kind on each side of a matched parent pair (GumTree
    /// Simple's unique type matching). See `solve_unique_type_matching`.
    UniqueTypeMatching,
    /// A leaf re-pointed from a same-text twin to the node between its matched neighbours'
    /// partners, at equal cost; typically a left-nested chain (`a || b || c`) that grew at its
    /// outer end. See `solve_leaf_neighbour_agreement`.
    LeafBetweenMatchedNeighbours,
    /// No pass decided this node, so the terminal sweep recorded its delete/insert. Not a matching
    /// verdict. See `solve_unresolved_nodes`.
    UnresolvedNode,
    /// Each node is the lowest common ancestor, in its tree, of what the other's matched
    /// descendants map to. See `solve_mutual_ancestors`.
    MutualAncestor,
    /// A chain of nested single-statement conditionals matched to one flattened multi-clause
    /// conditional (Rust `if let`-chains). See `solve_nested_condition_collapse`.
    NestedConditionCollapse,
    /// A re-tag of an `Identical` class/interface body shifted by a new `implements`/`extends`
    /// clause, so `ranges()` can tell it from a real move by reason alone; position at render
    /// time cannot make that distinction safely. Never painted as a move. See
    /// `solve_heritage_clause_growth`.
    HeritageClauseGrowth,
    /// A re-tag of an `Identical` node that a new wrapper (`try`, `if`, ...) now surrounds.
    /// Unlike `HeritageClauseGrowth`, painting is gated by
    /// [`crate::diff::text::RenderOptions::paint_reindent_only_moves`]: `rust-add-if`'s ground
    /// truth paints it `Move` under `Full` only. See `solve_wrap_growth`.
    WrapGrowth,
    /// A deleted and an inserted leaf with the same text under a matched parent pair, re-paired.
    /// See `solve_orphaned_leaves`.
    OrphanedLeafUnderMatchedParent,
}

impl ASTMappingReason {
    /// A short, stable label shared by the dev tools' displays. Every `APTED` variant maps to
    /// `"APTED"`; match `APTED(source)` directly for the call site.
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
            ASTMappingReason::GreedyAnchorBlock => "GreedyAnchor",
            ASTMappingReason::BottomUpPropagation => "BottomUpProp",
            ASTMappingReason::UniqueTypeMatching => "UniqueType",
            ASTMappingReason::UnresolvedNode => "Unresolved",
            ASTMappingReason::MutualAncestor => "MutualAnc",
            ASTMappingReason::NestedConditionCollapse => "CondCollapse",
            ASTMappingReason::HeritageClauseGrowth => "HeritageGrowth",
            ASTMappingReason::WrapGrowth => "WrapGrowth",
            ASTMappingReason::OrphanedLeafUnderMatchedParent => "OrphanLeaf",
            ASTMappingReason::LeafBetweenMatchedNeighbours => "NeighbourLeaf",
        }
    }
}

/// Same as [`Diff::from_code`].
pub fn diff_code(before: &Code, after: &Code) -> Diff {
    Diff::from_code(before, after)
}

/// Same as [`Diff::from_code_with_config`].
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

    /// A language without a grammar leaves `ast: None`, a valid state `Diff` must fail safe on.
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

    #[test]
    fn pending_finish_does_not_panic_when_language_is_unknown() {
        let before = Code::from_string("this is not code, just text", &Language::Unknown);
        let after = Code::from_string("this is different text now", &Language::Unknown);

        let diff = Diff::pending(&before, &after).finish();

        assert!(diff.ast.is_some());
    }

    /// Two files with nothing in common: the shape the terminal fallback exists for. The timing
    /// half runs only in an uninstrumented release build; a bound loose enough for debug or
    /// coverage builds would be too loose to catch the regression it guards.
    #[test]
    fn rust_completely_unrelated_main_files_resolves_fast() -> Result<()> {
        let (before, after) =
            &*test::helper::handmade_test_code_pair("rust-completely-unrelated-main-files")?;

        let pending = Diff::pending(before, after);
        assert!(
            pending.large_residual(),
            "this fixture's residual (~40%/~86% unmatched) should count as large - counts: {:?}",
            pending.unmatched_counts()
        );

        let started = std::time::Instant::now();
        let diff = Diff::from_code(before, after);
        let elapsed = started.elapsed();

        assert!(diff.ast.is_some());
        #[cfg(not(debug_assertions))]
        {
            // Either is set under `cargo llvm-cov`.
            let instrumented = std::env::var_os("LLVM_PROFILE_FILE").is_some()
                || std::env::var_os("CARGO_LLVM_COV").is_some();
            assert!(
                instrumented || elapsed < std::time::Duration::from_secs(5),
                "expected the terminal fallback to keep this fast, took {elapsed:?}"
            );
        }
        #[cfg(debug_assertions)]
        let _ = elapsed;
        Ok(())
    }

    #[test]
    fn test_compute_metadata() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;
        let code = test_codes.get("hello-world.rs").unwrap().clone();

        let metadata = crate::code::metadata::compute_ast_metadata(&code)?;

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

        // An empty Rust file still has a source_file node.
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
        assert_eq!(diff_ast.mapping.len(), 22);

        let before_root_id = before.ast.as_ref().unwrap().root_node().id();
        let after_root_id = after.ast.as_ref().unwrap().root_node().id();

        let root_mapping = diff_ast
            .mapping
            .get(&(before_root_id, after_root_id))
            .expect("Root node should be mapping");
        assert_eq!(root_mapping.reason, ASTMappingReason::IdenticalHash);
        assert_eq!(root_mapping.cost, 0);

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
        // Only the string_content leaf differs, but every ancestor's hash changes with it. The
        // minimal script is a single update of that leaf.
        let after = test_codes.get("zdravo-svijete.rs").unwrap().clone();

        let diff = diff_code(&before, &after);

        assert!(diff.ast.is_some());
        let diff_ast = diff.ast.unwrap();

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

        let before_node_id = before_string_node.id();
        let after_node_id = after_string_node.id();

        let mapping = diff_ast.mapping.get(&(before_node_id, after_node_id));
        assert!(mapping.is_some(), "String content nodes should be mapped");

        let mapping = mapping.unwrap();

        assert_eq!(
            mapping.operation,
            ASTMappingOperation::Update,
            "String content mapping should be an Update operation"
        );

        assert_eq!(
            mapping.reason,
            ASTMappingReason::StructurallyIdenticalAncestor,
            "String content mapping reason should be StructurallyIdenticalAncestor"
        );

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

            assert_eq!(root_mapping.cost, 0);

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

            // `Update` is for a leaf's own value; an interior node's cost is carried by the changed
            // leaf, not counted again here.
            assert_eq!(
                root_mapping.operation,
                ASTMappingOperation::MatchButNotIdentical,
                "Root node (an interior node) should be MatchButNotIdentical, not Update, for {}",
                filename
            );
            assert_eq!(root_mapping.cost, 0);

            // A reference node (C/C++'s `#include`) can be claimed first by reference-hash
            // matching, so it carries an identical-hash reason; the pairing is the same.
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

        let before_ast = before.ast.as_ref().unwrap();
        let after_ast = after.ast.as_ref().unwrap();

        let before_root = before_ast.root_node();
        let after_root = after_ast.root_node();

        let mut before_cursor = before_root.walk();
        let mut after_cursor = after_root.walk();

        let before_function_item = before_root.children(&mut before_cursor).next().unwrap();
        let after_function_item = after_root.children(&mut after_cursor).next().unwrap();

        let mut before_leaf_cursor = before_function_item.walk();
        let mut after_leaf_cursor = after_function_item.walk();

        let before_leaf = before_function_item
            .children(&mut before_leaf_cursor)
            .find(|child| child.kind() == "identifier")
            .unwrap();
        let after_leaf = after_function_item
            .children(&mut after_leaf_cursor)
            .find(|child| child.kind() == "block")
            .unwrap();

        let invalid_before_id = before_leaf.id();
        let invalid_after_id = after_leaf.id();

        diff_ast.mapping.clear();
        diff_ast.mapping.insert(
            (invalid_before_id, invalid_after_id),
            ASTMapping::identical(ASTMappingReason::IdenticalHash),
        );

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

        diff_ast.mapping.clear();
        diff_ast.add_mapping(0, 123, ASTMapping::inserted(ASTMappingReason::OptimalIDU));
        diff_ast.add_mapping(456, 0, ASTMapping::deleted(ASTMappingReason::OptimalIDU));

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

        diff.add_mapping(
            before_root_id,
            after_root_id,
            ASTMapping::identical(ASTMappingReason::IdenticalHash),
        );

        assert_eq!(diff.mapping.len(), 1);
        assert_eq!(diff.before_node_map.len(), 1);
        assert_eq!(diff.after_node_map.len(), 1);

        assert!(diff.mapping.contains_key(&(before_root_id, after_root_id)));

        assert_eq!(
            diff.before_node_map.get(&before_root_id),
            Some(&after_root_id)
        );

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

        assert_eq!(diff_ast.mapping.len(), diff_ast.before_node_map.len());
        assert_eq!(diff_ast.mapping.len(), diff_ast.after_node_map.len());

        assert!(!diff_ast.mapping.is_empty());

        for (before_id, after_id) in diff_ast.mapping.keys() {
            assert_eq!(diff_ast.before_node_map.get(before_id), Some(after_id));
            assert_eq!(diff_ast.after_node_map.get(after_id), Some(before_id));
        }

        Ok(())
    }

    #[test]
    fn remove_match_mapping_leaves_both_nodes_undecided() {
        let mut diff = ASTDiff::default();
        diff.add_mapping(1, 2, ASTMapping::identical(ASTMappingReason::IdenticalHash));

        diff.remove_match_mapping(1, 2);

        assert!(diff.mapping.is_empty());
        assert!(diff.mapping_for_node(&1).is_none());
        assert!(diff.mapping_for_node(&2).is_none());
        assert!(!diff.before_node_map.contains_key(&1));
        assert!(!diff.after_node_map.contains_key(&2));
    }

    #[test]
    fn is_complete_does_not_require_the_roots_to_be_mapped() -> Result<()> {
        let test_codes = test::helper::handmade_test_code()?;
        let code = test_codes.get("hello-world.rs").unwrap().clone();
        let node_cache = NodeCache::build(&code, &code);
        let root_id = code.ast.as_ref().unwrap().root_node().id();

        let mut diff = ASTDiff::default();
        for id in node_cache.before.keys().filter(|id| **id != root_id) {
            diff.add_mapping(
                *id,
                *id,
                ASTMapping::identical(ASTMappingReason::IdenticalHash),
            );
        }

        assert!(diff.is_complete(&code, &code, &node_cache));
        Ok(())
    }
}
